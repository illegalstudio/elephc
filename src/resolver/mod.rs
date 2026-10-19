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

use std::collections::HashSet;
use std::path::{Path, PathBuf};

mod contains;
mod declarations;
mod discovery;
pub(crate) mod dynamic_includes;
pub use dynamic_includes::{
    orphan_entries as dynamic_include_orphan_entries,
    render_report as render_dynamic_include_report, report_mode as dynamic_include_report_mode,
    set_app_root as set_dynamic_include_app_root, set_manifest_override as set_include_manifest,
    set_report_mode as set_dynamic_include_report_mode,
};
mod engine;
mod engine_includes;
mod exprs;
mod files;
mod function_variants;
/// Read by the checker, which must compare a CALL SITE name against a variant declaration.
mod include_once;
pub(crate) use include_once::include_once_label;
mod include_path;
mod state;
mod stmt_exprs;
mod value_include_hoist;

use crate::errors::CompileError;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::span::Span;

use contains::has_includes;
use discovery::discover_include_declarations;
use engine::resolve_stmts;
use state::ResolveState;

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

/// Resolves includes while applying the invocation's conditional symbols to every loaded file.
///
/// Wrapped in `with_compiler_stack` like every other recursive phase. This is the entry all
/// three public spellings funnel through, and the walk starts immediately: `has_includes`
/// descends the whole tree before the early return, so even a program with no includes at all
/// needs the budget at `MAX_COMPILER_NESTING`. Without it an embedder calling the resolver
/// from its own worker thread aborted with `has overflowed its stack` (issues #686, #1150).
pub fn resolve_collecting_includes_with_defines(
    program: Program,
    base_dir: &Path,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>), CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        Span::reset_source_ids();
        resolve_collecting_includes_with_defines_inner(program, base_dir, defines)
    })
}

/// The resolver's hidden-temporary counters at one moment; see [`counters_checkpoint`].
#[derive(Debug, Clone, Copy)]
pub struct ResolverCounters([usize; 3]);

/// Captures the counters that name the resolver's hidden temporaries, to hand back to
/// [`restore_counters`] after a resolution whose result is discarded (the autoload prelude
/// probe), so the real resolution names its temporaries exactly as without it.
pub fn counters_checkpoint() -> ResolverCounters {
    use std::sync::atomic::Ordering::SeqCst;
    ResolverCounters([
        value_include_hoist::HOIST_COUNTER.load(SeqCst),
        engine_includes::VALUE_INCLUDE_COUNTER.load(SeqCst),
        dynamic_includes::TEMP_COUNTER.load(SeqCst),
    ])
}

/// Rewinds the resolver's hidden-temporary counters to a [`counters_checkpoint`].
pub fn restore_counters(checkpoint: ResolverCounters) {
    use std::sync::atomic::Ordering::SeqCst;
    let [hoist, value_include, temp] = checkpoint.0;
    value_include_hoist::HOIST_COUNTER.store(hoist, SeqCst);
    engine_includes::VALUE_INCLUDE_COUNTER.store(value_include, SeqCst);
    dynamic_includes::TEMP_COUNTER.store(temp, SeqCst);
}

/// Resolves the includes of a file loaded INTO an ongoing compile (an autoloaded class file).
///
/// Unlike [`resolve_collecting_includes_with_defines`] it keeps the source identities the
/// compile already handed out, so a file this pulls in never reuses the identity of another
/// file of the same program: spans of different files stay distinct, and each diagnostic can
/// name its own file.
pub fn resolve_collecting_includes_continuing(
    program: Program,
    base_dir: &Path,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>), CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        resolve_collecting_includes_with_defines_inner(program, base_dir, defines)
    })
}

/// The resolver body, run on a stack `with_compiler_stack` has already sized.
fn resolve_collecting_includes_with_defines_inner(
    program: Program,
    base_dir: &Path,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>), CompileError> {
    if !has_includes(&program) {
        return Ok((program, Vec::new()));
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

    let mut included_files: Vec<PathBuf> = declared_once.into_iter().collect();
    included_files.sort();

    if discovery.declarations.is_empty() {
        return Ok((resolved, included_files));
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
    Ok((resolved_with_prelude, included_files))
}
