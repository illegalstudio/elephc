//! Purpose:
//! Coordinates include/require resolution before namespace canonicalization.
//! Loads included files, discovers declarations, and rewrites include-loaded function variants.
//!
//! Called from:
//! - `crate::pipeline::compile()` after conditionals and before `crate::name_resolver::resolve()`.
//!
//! Key details:
//! - Includes are resolved in source-file context so declarations are available before type checking.
//! - `resolve_collecting_includes` additionally surfaces the canonical path of every file the
//!   resolver statically loaded, which `crate::opcache_prelude` bakes into the OPcache script
//!   manifest.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

mod contains;
mod declarations;
mod discovery;
mod engine;
mod engine_includes;
mod exprs;
mod files;
mod function_variants;
mod include_path;
pub(crate) mod path_eval;
mod state;
mod source_units;
mod stmt_exprs;

pub use source_units::SourceUnit;

use crate::errors::CompileError;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::span::Span;

use contains::has_includes;
use discovery::discover_include_declarations;
use engine::resolve_stmts;
use state::ResolveState;

thread_local! {
    /// Whether an included file the compiler cannot parse is skipped instead of failing the
    /// build. Raised only while `opcache.preload` is resolved; see
    /// [`skip_unparseable_includes`].
    static SKIP_UNPARSEABLE_INCLUDES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Restores the previous unparseable-include policy when dropped.
pub(crate) struct SkipUnparseableIncludesGuard {
    previous: bool,
}

impl Drop for SkipUnparseableIncludesGuard {
    /// Puts back the policy that was active before [`skip_unparseable_includes`].
    fn drop(&mut self) {
        SKIP_UNPARSEABLE_INCLUDES.with(|flag| flag.set(self.previous));
    }
}

/// Makes an included file the compiler cannot parse a warning instead of a build failure, for as
/// long as the returned guard lives.
///
/// Used for `opcache.preload`, whose requires only feed declarations into the closed world: a file
/// left out there is still loaded at run time by the autoloader, exactly as php loads a class it
/// did not preload. The entry program's own includes keep failing the build.
pub(crate) fn skip_unparseable_includes() -> SkipUnparseableIncludesGuard {
    let previous = SKIP_UNPARSEABLE_INCLUDES.with(|flag| flag.replace(true));
    SkipUnparseableIncludesGuard { previous }
}

/// Returns whether the unparseable-include policy is active, without reporting anything.
pub(super) fn skipping_unparseable_includes() -> bool {
    SKIP_UNPARSEABLE_INCLUDES.with(std::cell::Cell::get)
}

/// Returns whether an unparseable include is skipped, and reports it when it is.
pub(super) fn skips_unparseable_include(path: &Path, error: &CompileError) -> bool {
    if !SKIP_UNPARSEABLE_INCLUDES.with(std::cell::Cell::get) {
        return false;
    }
    crate::errors::report_warning(&crate::errors::CompileWarning::new(
        Span::dummy(),
        &format!(
            "opcache.preload: '{}' could not be parsed ({}); it is not preloaded and its classes \
             are loaded at run time by the autoloader instead",
            path.display(),
            error.message
        ),
    ));
    true
}

/// Maximum nested include depth accepted by both discovery and resolution.
///
/// This bound is independent of parser delimiter depth: a long acyclic include
/// chain otherwise recurses through the host stack before resolution begins.
pub(super) const MAX_INCLUDE_DEPTH: usize = 256;

/// Resolves all include/require statements by inlining the referenced files.
///
/// Inputs: `program` is the parsed AST; `base_dir` is the directory used for
/// resolving relative include paths.
///
/// Runs between parsing and type checking. Skips processing entirely if the
/// program contains no include/require statements (fast path).
///
/// Outputs: Returns the program with all includes inlined. If any included files
/// declared functions or classes, they are prepended as a `NamespaceBlock`
/// prelude so declarations are visible before the rest of the program.
///
/// Side effects: Populates `declared_once` (set of `__FILE__`-resolved paths for
/// `include_once`/`require_once` guards), `include_chain` (stack of files being
/// processed for cycle detection), and `ResolveState` (per-file state
/// including discovered function variants). The `discovery` phase performs
/// filesystem I/O to locate included files before any AST rewriting occurs.
///
/// This is the include-set-discarding wrapper over [`resolve_collecting_includes`]. It is kept
/// because the fourteen call sites that do not care about the include set (the `ir_lower`,
/// `error_tests` and `tests/codegen/support` harnesses) read better without a `.0`; only
/// `crate::pipeline`, which bakes the OPcache script manifest, takes the longer form.
#[allow(dead_code)] // Consumed by the test harnesses; `crate::pipeline` uses the collecting form.
pub fn resolve(program: Program, base_dir: &Path) -> Result<Program, CompileError> {
    resolve_collecting_includes(program, base_dir).map(|(program, _)| program)
}

/// Same as [`resolve`], but also returns the CANONICAL path of every source file the
/// resolver statically loaded through `include` / `require` / `include_once` /
/// `require_once`, each exactly once.
///
/// SOURCE OF THE SET: the engine's `declared_once`, threaded through
/// `resolver::engine::resolve_stmts` and inserted by
/// `resolver::engine_includes::resolve_include_stmt` for every one of the four include
/// forms once its target has been parsed. It is deliberately NOT
/// `discovery::DiscoveryEntry::canonical`: `DiscoveryOutput::push` drops any entry whose
/// `declarations` are empty, so a file that only holds executable statements (a config
/// array, a bootstrap side effect) would never be recorded — yet it IS compiled into the
/// binary and therefore IS a cached script. `declared_once` records every include the
/// engine actually inlined, declarations or not, and its `HashSet` gives the
/// exactly-once property for free (repeat plain `include`s of one file collapse to one
/// manifest entry, which is what "cached script" means).
///
/// The paths carry the SAME normalization `__FILE__` bakes
/// (`crate::magic_constants::file_pass`, `Path::canonicalize`): `resolve_include_stmt`
/// canonicalizes before inserting, and only inserts after the target parsed — so a
/// missing file (whose `canonicalize` would have fallen back to the raw path) never
/// reaches the set.
///
/// The vector is SORTED so a build is byte-reproducible; `declared_once` is a `HashSet`
/// and its iteration order is not.
pub fn resolve_collecting_includes(
    program: Program,
    base_dir: &Path,
) -> Result<(Program, Vec<PathBuf>), CompileError> {
    resolve_collecting_includes_with_defines(program, base_dir, &HashSet::new())
}

/// Which physical file declares each class-like and function pulled in through includes.
///
/// Collected while each include is loaded, so a declaration is attributed to the file it was
/// written in rather than to whichever file happened to include it. Consumed by
/// `Reflection*::getFileName()`.
#[derive(Default)]
pub struct IncludedDeclarationSources {
    /// Canonical class-like name to the physical path that declares it.
    pub class_likes: HashMap<String, String>,
    /// Canonical function name to the physical path that declares it.
    pub functions: HashMap<String, String>,
    /// Immutable inputs for later compiled file entries, not a list of executed files.
    pub source_units: std::collections::BTreeMap<PathBuf, SourceUnit>,
    /// Files the compiler read but did NOT take whole, mapped to the names it dropped.
    pub unbound_conditional_declaration_sources: HashMap<PathBuf, Vec<String>>,
    /// Names bound through the function-variant mechanism; subtracted from the dropped names.
    pub bound_conditional_declaration_names: HashSet<String>,
}

/// Resolves includes while applying the invocation's conditional symbols to every loaded file.
///
/// Wrapped in `with_compiler_stack` like every other recursive phase. This is the entry all
/// three public spellings funnel through, and the walk starts immediately: `has_includes`
/// descends the whole tree before the early return, so even a program with no includes at all
/// needs the budget at `MAX_COMPILER_NESTING`. Without it an embedder calling the resolver
/// from its own worker thread aborted with `has overflowed its stack` (issues #686, #1150).
///
/// This is the ROOT resolution of a compile, so it starts the included-file source identities
/// over (`Span::reset_source_ids`). A file loaded later in the same compile resolves its own
/// includes through [`resolve_collecting_includes_continuing`] instead.
pub fn resolve_collecting_includes_with_defines(
    program: Program,
    base_dir: &Path,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>), CompileError> {
    resolve_collecting_includes_with_defines_and_sources(program, base_dir, defines)
        .map(|(program, files, _)| (program, files))
}

/// Same as [`resolve_collecting_includes_with_defines`], and also reports which physical file
/// declares each class-like and function the includes brought in.
pub fn resolve_collecting_includes_with_defines_and_sources(
    program: Program,
    base_dir: &Path,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>, IncludedDeclarationSources), CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        Span::reset_source_ids();
        resolve_collecting_includes_with_defines_and_sources_inner(program, base_dir, defines)
    })
}

/// Resolves the includes of a file loaded INTO an ongoing compile (an autoloaded class file).
///
/// Unlike [`resolve_collecting_includes_with_defines_and_sources`] it keeps the source identities
/// the compile already handed out, so a file this pulls in never reuses the identity of another
/// file of the same program. Rewinding here gave an autoloaded file's includes the identities of
/// the entry's includes, so span-keyed checker results leaked between them (issue #1445), and
/// two files' spans collided.
pub(crate) fn resolve_collecting_includes_continuing(
    program: Program,
    base_dir: &Path,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>, IncludedDeclarationSources), CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        resolve_collecting_includes_with_defines_and_sources_inner(program, base_dir, defines)
    })
}

/// The resolver body, run on a stack `with_compiler_stack` has already sized.
///
/// Leaves the source-identity counter alone: the public entries decide whether this resolution
/// starts a compile (and rewinds it) or joins one already under way. The autoloader resolves
/// every class file through the `continuing` entry, so a reset per call would number each file's
/// includes from 1 again.
fn resolve_collecting_includes_with_defines_and_sources_inner(
    program: Program,
    base_dir: &Path,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>, IncludedDeclarationSources), CompileError> {
    if !has_includes(&program) {
        return Ok((program, Vec::new(), IncludedDeclarationSources::default()));
    }

    let discovery = discover_include_declarations(&program, base_dir, defines)?;
    let mut declared_once: HashSet<PathBuf> = HashSet::new();
    let mut include_chain: Vec<PathBuf> = Vec::new();
    let mut state = ResolveState::with_conditional_defines(defines);
    let resolved = resolve_stmts(
        program,
        base_dir,
        &mut declared_once,
        &mut include_chain,
        &mut state,
        &discovery.function_variants,
    )?;

    // What resolution expanded wins; discovery adds the files only it reached.
    let mut class_likes = std::mem::take(&mut state.declared_class_files);
    for (name, file) in discovery.class_files {
        class_likes.entry(name).or_insert(file);
    }
    let mut functions = std::mem::take(&mut state.declared_function_files);
    for (name, file) in discovery.function_files {
        functions.entry(name).or_insert(file);
    }
    let mut sources = IncludedDeclarationSources {
        class_likes,
        functions,
        source_units: state.source_units.snapshot(),
        unbound_conditional_declaration_sources: state
            .unbound_conditional_declaration_sources
            .snapshot(),
        bound_conditional_declaration_names: state.bound_conditional_declaration_names.snapshot(),
    };
    // A function declared inside an include is renamed to a variant symbol
    // (`__elephc_include_variant_<hash>_<name>`), and that renamed symbol is the only name later
    // passes ever see. Recording it here — the registry key already carries the canonical path —
    // is what lets a diagnostic or `ReflectionFunction::getFileName()` on such a function still
    // name the file it was written in; the public name recorded above stays alongside it.
    for (key, info) in &discovery.function_variants {
        sources.functions.insert(
            crate::names::php_symbol_key(info.variant_name.trim_start_matches('\\')),
            key.canonical().display().to_string(),
        );
    }

    let mut included_files: Vec<PathBuf> = declared_once.into_iter().collect();
    included_files.sort();

    if discovery.declarations.is_empty() {
        return Ok((resolved, included_files, sources));
    }

    let prelude_span = discovery
        .declarations
        .first()
        .map(|stmt| stmt.span)
        .unwrap_or_else(Span::dummy);
    let mut resolved_with_prelude = vec![Stmt::new(
        StmtKind::NamespaceBlock {
            name: None,
            body: discovery.declarations,
        },
        prelude_span,
    )];
    resolved_with_prelude.extend(resolved);
    Ok((resolved_with_prelude, included_files, sources))
}
