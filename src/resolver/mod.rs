use std::collections::HashSet;
use std::path::{Path, PathBuf};

mod contains;
mod declarations;
mod discovery;
mod engine;
mod exprs;
mod files;
mod function_variants;
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

/// Resolves all include/require statements by inlining the referenced files.
/// Runs between parsing and type checking.
pub fn resolve(program: Program, base_dir: &Path) -> Result<Program, CompileError> {
    if !has_includes(&program) {
        return Ok(program);
    }

    let discovery = discover_include_declarations(&program, base_dir)?;
    let mut declared_once: HashSet<PathBuf> = HashSet::new();
    let mut include_chain: Vec<PathBuf> = Vec::new();
    let mut state = ResolveState::default();
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
