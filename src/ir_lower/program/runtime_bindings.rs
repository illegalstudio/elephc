//! Collects runtime-bound public names without confusing wrappers with activation.
//!
//! Used by program lowering before individual function bodies are lowered.

use std::collections::HashSet;

use crate::names::php_symbol_key;
use crate::parser::ast::{Stmt, StmtKind};

pub(super) fn collect_runtime_bound_functions(program: &[Stmt]) -> HashSet<String> {
    fn collect(stmts: &[Stmt], names: &mut HashSet<String>) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::FunctionVariantGroup { name, .. } => {
                    names.insert(php_symbol_key(name));
                }
                StmtKind::Synthetic(body)
                | StmtKind::NamespaceBlock { body, .. }
                | StmtKind::IncludeOnceGuard { body, .. } => collect(body, names),
                _ => {}
            }
        }
    }
    let mut names = HashSet::new();
    collect(program, &mut names);
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    #[test]
    fn runtime_binding_collection_preserves_transparent_wrappers() {
        let group = || Stmt::new(StmtKind::FunctionVariantGroup {
            name: "Scope\\Callable".into(),
            variants: vec!["internal_body".into()],
        }, Span::dummy());
        let program = vec![Stmt::new(StmtKind::Synthetic(vec![Stmt::new(
            StmtKind::NamespaceBlock { name: None, body: vec![Stmt::new(
                StmtKind::IncludeOnceGuard {
                    source_path: std::path::PathBuf::from("unit.php"),
                    required: false,
                    body: vec![group()],
                }, Span::dummy(),
            )] }, Span::dummy(),
        )]), Span::dummy()), group()];
        let names = collect_runtime_bound_functions(&program);
        assert_eq!(names.len(), 1);
        assert!(names.contains("scope\\callable"));
        assert!(!names.contains("internal_body"));
    }
}
