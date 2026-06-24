//! Purpose:
//! Coordinates include/require resolution before namespace canonicalization.
//! Loads included files, discovers declarations, and rewrites include-loaded function variants.
//!
//! Called from:
//! - `crate::pipeline::compile()` after conditionals and before `crate::name_resolver::resolve()`.
//!
//! Key details:
//! - Includes are resolved in source-file context so declarations are available before type checking.

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
mod hoist_includes;
mod include_once;
mod include_path;
pub(crate) mod path_eval;
mod state;
mod stmt_exprs;

use crate::errors::CompileError;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::span::Span;

use contains::has_includes;
use discovery::discover_include_declarations;
use engine::resolve_stmts;
use state::ResolveState;

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
pub fn resolve(program: Program, base_dir: &Path) -> Result<Program, CompileError> {
    resolve_inner(program, base_dir, false)
}

/// Like [`resolve`], but lowers an unresolvable runtime-dynamic include/require path into a
/// diverging runtime-fatal stub instead of failing compilation. Used by the autoloader when
/// splicing transitively-referenced library code (`crate::autoload`): such files may contain
/// lazy dynamic includes (e.g. a polyfill that `require`s a data table by a computed path) that
/// never execute for the program being built, so they must not block the closed-world compile.
/// The main program keeps the strict [`resolve`] behavior.
pub fn resolve_lenient_includes(
    program: Program,
    base_dir: &Path,
) -> Result<Program, CompileError> {
    resolve_inner(program, base_dir, true)
}

/// Shared implementation of [`resolve`] and [`resolve_lenient_includes`]. `lenient_includes`
/// selects whether an unresolvable runtime-dynamic include path becomes a runtime-fatal stub
/// (`true`) or a hard compile error (`false`).
fn resolve_inner(
    program: Program,
    base_dir: &Path,
    lenient_includes: bool,
) -> Result<Program, CompileError> {
    if !has_includes(&program) {
        return Ok(program);
    }

    let discovery = discover_include_declarations(&program, base_dir, lenient_includes)?;
    let mut declared_once: HashSet<PathBuf> = HashSet::new();
    let mut include_chain: Vec<PathBuf> = Vec::new();
    let mut state = ResolveState {
        lenient_dynamic_includes: lenient_includes,
        ..ResolveState::default()
    };
    let resolved = resolve_stmts(
        program,
        base_dir,
        &mut declared_once,
        &mut include_chain,
        &mut state,
        &discovery.function_variants,
    )?;

    if discovery.declarations.is_empty() {
        return Ok(resolved);
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
    Ok(resolved_with_prelude)
}
