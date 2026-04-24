use crate::errors::CompileWarning;
use crate::parser::ast::{ClassMethod, Stmt, StmtKind, Visibility};

pub(super) fn collect_oop_warnings(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl { methods, .. } | StmtKind::TraitDecl { methods, .. } => {
                collect_method_modifier_warnings(methods, warnings);
            }
            StmtKind::NamespaceBlock { body, .. } => collect_oop_warnings(body, warnings),
            _ => {}
        }
    }
}

fn collect_method_modifier_warnings(methods: &[ClassMethod], warnings: &mut Vec<CompileWarning>) {
    for method in methods {
        if method.is_final
            && method.visibility == Visibility::Private
            && !method.name.eq_ignore_ascii_case("__construct")
        {
            warnings.push(CompileWarning::new(
                method.span,
                "Private methods cannot be final as they are never overridden by other classes",
            ));
        }
    }
}
