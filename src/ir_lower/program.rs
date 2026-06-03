//! Purpose:
//! Orchestrates AST-to-EIR lowering for a complete checked program.
//!
//! Called from:
//! - `crate::ir_lower::lower_program()`.
//!
//! Key details:
//! - Declaration bodies are lowered before synthetic `main`; declaration
//!   statements themselves are no-ops inside `main`.
//! - The module is validated before it is returned to CLI/test callers.

use crate::codegen::platform::Target;
use crate::ir::{
    validate_module, ExternDecl, ExternParamDecl, IrType, Module,
};
use crate::ir_lower::{function, LoweringError};
use crate::parser::ast::{ClassMethod, ExprKind, Program, Stmt, StmtKind};
use crate::types::{CheckResult, PhpType};

/// Lowers an optimized typed AST program into a validated EIR module.
pub(crate) fn lower(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
) -> Result<Module, LoweringError> {
    let mut module = Module::new(target);
    let constants = crate::codegen::collect_constants(program, target.platform);
    populate_metadata(&mut module, program, check_result);
    lower_function_declarations(program, &mut module, check_result, &constants);
    lower_class_like_methods(program, &mut module, check_result, &constants);
    function::lower_main(program, &mut module, check_result, &constants);
    validate_module(&module)?;
    Ok(module)
}

/// Copies declaration metadata into the EIR module placeholder tables.
fn populate_metadata(module: &mut Module, program: &Program, check_result: &CheckResult) {
    module.class_table.names = sorted_keys(&check_result.classes);
    module.enum_table.names = sorted_keys(&check_result.enums);
    module.interface_table.names = sorted_keys(&check_result.interfaces);
    module.packed_layouts.names = sorted_keys(&check_result.packed_classes);
    module.extern_decls = check_result
        .extern_functions
        .values()
        .map(|sig| ExternDecl {
            name: sig.name.clone(),
            params: sig
                .params
                .iter()
                .map(|(name, php_type)| ExternParamDecl {
                    name: name.clone(),
                    ir_type: value_or_void_ir_type(php_type),
                    php_type: php_type.clone(),
                })
                .collect(),
            return_type: value_or_void_ir_type(&sig.return_type),
            return_php_type: sig.return_type.clone(),
            link_libs: sig.library.iter().cloned().collect(),
        })
        .collect();
    module.required_runtime_features =
        crate::codegen::runtime_features_for_program_and_classes(program, &check_result.classes);
}

/// Returns deterministic sorted keys for metadata placeholder tables.
fn sorted_keys<T>(map: &std::collections::HashMap<String, T>) -> Vec<String> {
    let mut keys = map.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    keys
}

/// Converts a PHP type to EIR storage while preserving true void returns.
fn value_or_void_ir_type(php_type: &PhpType) -> IrType {
    match php_type {
        PhpType::Void | PhpType::Never => IrType::Void,
        other => IrType::from_php(other),
    }
}

/// Lowers every function declaration reachable in the statement tree.
fn lower_function_declarations(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                params,
                variadic: _,
                return_type,
                body,
            } => function::lower_user_function(
                name,
                params,
                return_type.as_ref(),
                body,
                module,
                check_result,
                constants,
            ),
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                lower_function_declarations(body, module, check_result, constants);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                lower_function_declarations(then_body, module, check_result, constants);
                for (_, body) in elseif_clauses {
                    lower_function_declarations(body, module, check_result, constants);
                }
                if let Some(body) = else_body {
                    lower_function_declarations(body, module, check_result, constants);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                lower_function_declarations(then_body, module, check_result, constants);
                if let Some(body) = else_body {
                    lower_function_declarations(body, module, check_result, constants);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                lower_function_declarations(body, module, check_result, constants);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    lower_function_declarations(body, module, check_result, constants);
                }
                if let Some(body) = default {
                    lower_function_declarations(body, module, check_result, constants);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                lower_function_declarations(try_body, module, check_result, constants);
                for catch in catches {
                    lower_function_declarations(&catch.body, module, check_result, constants);
                }
                if let Some(body) = finally_body {
                    lower_function_declarations(body, module, check_result, constants);
                }
            }
            _ => {}
        }
    }
}

/// Lowers methods declared on classes, interfaces, and traits when a body exists.
fn lower_class_like_methods(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::ClassDecl { name, methods, .. } | StmtKind::TraitDecl { name, methods, .. } => {
                lower_methods_for_class_like(name, methods, module, check_result, constants);
            }
            StmtKind::InterfaceDecl { name, methods, .. } => {
                lower_methods_for_class_like(name, methods, module, check_result, constants);
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                lower_class_like_methods(body, module, check_result, constants);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                lower_class_like_methods(then_body, module, check_result, constants);
                for (_, body) in elseif_clauses {
                    lower_class_like_methods(body, module, check_result, constants);
                }
                if let Some(body) = else_body {
                    lower_class_like_methods(body, module, check_result, constants);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                lower_class_like_methods(then_body, module, check_result, constants);
                if let Some(body) = else_body {
                    lower_class_like_methods(body, module, check_result, constants);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                lower_class_like_methods(body, module, check_result, constants);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    lower_class_like_methods(body, module, check_result, constants);
                }
                if let Some(body) = default {
                    lower_class_like_methods(body, module, check_result, constants);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                lower_class_like_methods(try_body, module, check_result, constants);
                for catch in catches {
                    lower_class_like_methods(&catch.body, module, check_result, constants);
                }
                if let Some(body) = finally_body {
                    lower_class_like_methods(body, module, check_result, constants);
                }
            }
            _ => {}
        }
    }
}

/// Lowers all concrete methods for one class-like declaration.
fn lower_methods_for_class_like(
    class_name: &str,
    methods: &[ClassMethod],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
) {
    for method in methods {
        if !method.has_body {
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
        );
    }
}
