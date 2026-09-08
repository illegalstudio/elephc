//! Purpose:
//! Class-like method and referenced builtin method lowering.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;

/// Validates checker/AST method agreement in debug builds, then lowers class-like methods.
pub(super) fn lower_class_like_methods(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    #[cfg(debug_assertions)]
    debug_assert_checker_methods_have_ast_sources(statements, check_result);
    lower_class_like_methods_inner(
        statements,
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Lowers concrete class/interface methods, including trait methods flattened into classes.
fn lower_class_like_methods_inner(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::ClassDecl { name, methods, .. } => {
                let methods = check_result
                    .classes
                    .get(name)
                    .map(|class_info| class_info.method_decls.as_slice())
                    .unwrap_or(methods.as_slice());
                lower_methods_for_class_like(
                    name,
                    methods,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            StmtKind::TraitDecl { .. } => {}
            StmtKind::InterfaceDecl { name, methods, .. } => {
                lower_methods_for_class_like(
                    name,
                    methods,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            StmtKind::EnumDecl { name, methods, .. } => {
                // Enum methods are lowered like class methods on the case singleton; prefer the
                // checker's flattened declarations (with `self` types resolved to the enum).
                let methods = check_result
                    .classes
                    .get(name)
                    .map(|class_info| class_info.method_decls.as_slice())
                    .unwrap_or(methods.as_slice());
                lower_methods_for_class_like(
                    name,
                    methods,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                lower_class_like_methods_inner(body, module, check_result, constants, fiber_return_sigs);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                lower_class_like_methods_inner(
                    then_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                for (_, body) in elseif_clauses {
                    lower_class_like_methods_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = else_body {
                    lower_class_like_methods_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                lower_class_like_methods_inner(
                    then_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                if let Some(body) = else_body {
                    lower_class_like_methods_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                lower_class_like_methods_inner(body, module, check_result, constants, fiber_return_sigs);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    lower_class_like_methods_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = default {
                    lower_class_like_methods_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                lower_class_like_methods_inner(
                    try_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                for catch in catches {
                    lower_class_like_methods_inner(
                        &catch.body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = finally_body {
                    lower_class_like_methods_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Panics when an AST-backed checker class still contains a method removed from the source tree.
#[cfg(debug_assertions)]
fn debug_assert_checker_methods_have_ast_sources(
    statements: &[Stmt],
    check_result: &CheckResult,
) {
    let mut classes = std::collections::HashSet::new();
    let mut methods = std::collections::HashSet::new();
    collect_ast_method_sources(statements, &mut classes, &mut methods);
    for (class_name, class_info) in &check_result.classes {
        if !classes.contains(&php_symbol_key(class_name)) {
            continue;
        }
        for method in &class_info.method_decls {
            if method.span == crate::span::Span::dummy() {
                // DateTime subclass hydration helpers and comparable checker-injected methods
                // carry a complete synthetic AST body in `method_decls`, but deliberately have
                // no source declaration to match. They are lowered from that generated body below.
                continue;
            }
            assert!(
                methods.contains(&(method.span, method.is_static)),
                "checker method declaration {}::{} has no matching AST method",
                class_name,
                method.name,
            );
        }
    }
}

/// Collects class names and stable method source identities throughout declaration-hosting blocks.
#[cfg(debug_assertions)]
fn collect_ast_method_sources(
    statements: &[Stmt],
    classes: &mut std::collections::HashSet<String>,
    methods: &mut std::collections::HashSet<(crate::span::Span, bool)>,
) {
    for statement in statements {
        match &statement.kind {
            StmtKind::ClassDecl {
                name,
                methods: declarations,
                ..
            }
            | StmtKind::EnumDecl {
                name,
                methods: declarations,
                ..
            }
            | StmtKind::InterfaceDecl {
                name,
                methods: declarations,
                ..
            } => {
                classes.insert(php_symbol_key(name));
                methods.extend(
                    declarations
                        .iter()
                        .map(|method| (method.span, method.is_static)),
                );
            }
            StmtKind::TraitDecl {
                methods: declarations,
                ..
            } => {
                methods.extend(
                    declarations
                        .iter()
                        .map(|method| (method.span, method.is_static)),
                );
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                collect_ast_method_sources(body, classes, methods);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_ast_method_sources(then_body, classes, methods);
                for (_, body) in elseif_clauses {
                    collect_ast_method_sources(body, classes, methods);
                }
                if let Some(body) = else_body {
                    collect_ast_method_sources(body, classes, methods);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_ast_method_sources(then_body, classes, methods);
                if let Some(body) = else_body {
                    collect_ast_method_sources(body, classes, methods);
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_ast_method_sources(body, classes, methods);
                }
                if let Some(body) = default {
                    collect_ast_method_sources(body, classes, methods);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_ast_method_sources(try_body, classes, methods);
                for catch in catches {
                    collect_ast_method_sources(&catch.body, classes, methods);
                }
                if let Some(body) = finally_body {
                    collect_ast_method_sources(body, classes, methods);
                }
            }
            _ => {}
        }
    }
}

/// Lowers all concrete methods for one class-like declaration.
pub(super) fn lower_methods_for_class_like(
    class_name: &str,
    methods: &[ClassMethod],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    for method in methods {
        if !method.has_body {
            continue;
        }
        let method_key = php_method_key(&method.name);
        if class_method_already_lowered(module, class_name, &method_key, method.is_static) {
            continue;
        }
        function::lower_class_method(
            class_name,
            &method.name,
            method.is_static,
            &method.params,
            method.return_type.as_ref(),
            &method.body,
            module,
            check_result,
            constants,
            fiber_return_sigs,
        );
    }
}
