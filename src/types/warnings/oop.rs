//! Purpose:
//! Emits checker warnings for oop cases.
//! Scans typed AST and checker metadata for suspicious but non-fatal program patterns.
//!
//! Called from:
//! - `crate::types::warnings`
//!
//! Key details:
//! - Warning analysis should preserve source spans and avoid rejecting programs that type checking accepted.

use crate::errors::CompileWarning;
use crate::parser::ast::{ClassMethod, Stmt, StmtKind, Visibility};

/// Recursively walks statements to collect OOP-related warnings.
///
/// Visits `ClassDecl`, `TraitDecl`, `NamespaceBlock`, and `IncludeOnceGuard` nodes,
/// delegating modifier validation to `collect_method_modifier_warnings` for each class/trait.
pub(super) fn collect_oop_warnings(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl { methods, .. } | StmtKind::TraitDecl { methods, .. } => {
                collect_method_modifier_warnings(methods, warnings);
            }
            StmtKind::NamespaceBlock { body, .. } => collect_oop_warnings(body, warnings),
            StmtKind::IncludeOnceGuard { body, .. } => collect_oop_warnings(body, warnings),
            _ => {}
        }
    }
}

/// Checks each class/trait method for invalid modifier combinations and emits warnings.
///
/// Currently warns when a non-constructor private method is marked `final`, since private
/// methods cannot be overridden and the `final` qualifier has no effect in that context.
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
