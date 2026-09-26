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
mod engine;
mod engine_includes;
mod exprs;
mod files;
mod function_variants;
/// Read by the checker, which must compare a CALL SITE name against a variant declaration.
mod include_once;
mod include_path;
mod state;
mod stmt_exprs;

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
        resolve_collecting_includes_with_defines_inner(program, base_dir, defines)
    })
}

/// The resolver body, run on a stack `with_compiler_stack` has already sized.
fn resolve_collecting_includes_with_defines_inner(
    program: Program,
    base_dir: &Path,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>), CompileError> {
    Span::reset_source_ids();
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
