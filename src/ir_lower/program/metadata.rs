//! Purpose:
//! Program metadata population and method ABI normalization.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;

/// Converts a PHP source path into the canonical display string stored in EIR metadata.
pub(super) fn canonical_source_path(source_path: &Path) -> String {
    source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf())
        .display()
        .to_string()
}

/// Copies declaration metadata into the EIR module placeholder tables.
pub(super) fn populate_metadata(module: &mut Module, program: &Program, check_result: &CheckResult) {
    module.class_table.names = sorted_keys(&check_result.classes);
    module.enum_table.names = sorted_keys(&check_result.enums);
    module.interface_table.names = sorted_keys(&check_result.interfaces);
    module.trait_table.names = collect_declared_trait_names(program);
    module.declared_class_names = collect_declared_class_names(program, &check_result.classes);
    module.declared_interface_names =
        collect_declared_interface_names(program, &check_result.interfaces);
    module.declared_trait_names = collect_declared_trait_names(program);
    module.declared_trait_source_lines = collect_declared_trait_source_lines(program);
    module.declared_trait_uses = collect_declared_trait_uses(program);
    module.declared_trait_method_names = collect_declared_trait_method_names(program);
    module.declared_trait_methods = collect_declared_trait_methods(program);
    module.declared_trait_property_names = collect_declared_trait_property_names(program);
    module.declared_trait_constant_names = collect_declared_trait_constant_names(program);
    module.declared_trait_constants = collect_declared_trait_constants(program);
    module.declared_trait_constant_types = collect_declared_trait_constant_types(program);
    module.declared_trait_constant_visibilities =
        collect_declared_trait_constant_visibilities(program);
    module.declared_trait_final_constants = collect_declared_trait_final_constants(program);
    module.class_infos = check_result.classes.clone();
    normalize_class_method_signatures_for_eir(module, &check_result.callable_param_sigs);
    module.interface_infos = check_result.interfaces.clone();
    module.enum_infos = check_result.enums.clone();
    module.extern_class_infos = check_result.extern_classes.clone();
    module.packed_class_infos = check_result.packed_classes.clone();
    module.packed_layouts.names = sorted_keys(&check_result.packed_classes);
    module.extern_globals = check_result.extern_globals.clone();
    module.callable_param_sigs = check_result.callable_param_sigs.clone();
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

/// Normalizes class method metadata to the ABI contracts emitted in EIR.
pub(super) fn normalize_class_method_signatures_for_eir(
    module: &mut Module,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
) {
    for (class_name, class_info) in module.class_infos.iter_mut() {
        normalize_method_map_for_eir(
            class_name,
            &mut class_info.methods,
            &class_info.method_decls,
            false,
            callable_param_sigs,
        );
        normalize_method_map_for_eir(
            class_name,
            &mut class_info.static_methods,
            &class_info.method_decls,
            true,
            callable_param_sigs,
        );
    }
}

/// Normalizes one instance/static method table for EIR call and bridge metadata.
pub(super) fn normalize_method_map_for_eir(
    class_name: &str,
    methods: &mut HashMap<String, FunctionSig>,
    method_decls: &[ClassMethod],
    is_static: bool,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
) {
    // Stream-wrapper and user-filter contract methods are invoked through
    // runtime vtables with raw fixed-ABI arguments; widening their untyped
    // params to boxed Mixed would desynchronize the dispatcher and the body.
    let is_wrapper_class = methods
        .keys()
        .any(|key| crate::codegen_support::runtime::is_user_wrapper_marker_method(key));
    let is_filter_class = methods.contains_key("filter");
    for (method_key, signature) in methods.iter_mut() {
        if (is_wrapper_class
            && crate::codegen_support::runtime::is_user_wrapper_contract_method(method_key))
            || (is_filter_class
                && crate::codegen_support::runtime::is_user_filter_contract_method(method_key))
        {
            continue;
        }
        let owner_name = format!("{}::{}", class_name, method_key);
        let mut normalized = function::eir_signature_with_php_param_contracts(
            &owner_name,
            signature,
            callable_param_sigs,
        );
        if method_decls
            .iter()
            .find(|method| {
                method.is_static == is_static && php_symbol_key(&method.name) == *method_key
            })
            .is_some_and(|method| {
                method_return_exposes_dynamic_param(
                    method,
                    signature,
                    &owner_name,
                    callable_param_sigs,
                )
            })
        {
            normalized.return_type = PhpType::Mixed;
        }
        *signature = normalized;
    }
}

/// Returns true when an untyped method return can expose a dynamic parameter directly.
pub(super) fn method_return_exposes_dynamic_param(
    method: &ClassMethod,
    signature: &FunctionSig,
    owner_name: &str,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
) -> bool {
    if signature.declared_return {
        return false;
    }
    let dynamic_params = dynamic_untyped_param_names(owner_name, signature, callable_param_sigs);
    !dynamic_params.is_empty() && body_returns_dynamic_param(&method.body, &dynamic_params)
}

/// Collects untyped by-value parameter names that need a boxed EIR ABI.
pub(super) fn dynamic_untyped_param_names(
    owner_name: &str,
    signature: &FunctionSig,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for (index, (name, php_type)) in signature.params.iter().enumerate() {
        let declared = signature
            .declared_params
            .get(index)
            .copied()
            .unwrap_or(false);
        let by_ref = signature.ref_params.get(index).copied().unwrap_or(false);
        let variadic = signature.variadic.as_deref() == Some(name.as_str());
        let preserved = matches!(php_type.codegen_repr(), PhpType::Callable)
            || callable_param_sigs.contains_key(&(owner_name.to_string(), name.to_string()));
        if !declared && !by_ref && !variadic && !preserved {
            names.insert(name.clone());
        }
    }
    names
}

/// Recursively scans a method body for returns that expose dynamic parameters.
pub(super) fn body_returns_dynamic_param(body: &[Stmt], dynamic_params: &HashSet<String>) -> bool {
    body.iter()
        .any(|stmt| stmt_returns_dynamic_param(stmt, dynamic_params))
}

/// Returns true when one statement can return a dynamic parameter directly.
pub(super) fn stmt_returns_dynamic_param(stmt: &Stmt, dynamic_params: &HashSet<String>) -> bool {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => expr_exposes_dynamic_param(expr, dynamic_params),
        StmtKind::Return(None) => false,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            body_returns_dynamic_param(then_body, dynamic_params)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| body_returns_dynamic_param(body, dynamic_params))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            body_returns_dynamic_param(then_body, dynamic_params)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => body_returns_dynamic_param(body, dynamic_params),
        StmtKind::For {
            init, update, body, ..
        } => {
            init.as_ref()
                .is_some_and(|stmt| stmt_returns_dynamic_param(stmt.as_ref(), dynamic_params))
                || update
                    .as_ref()
                    .is_some_and(|stmt| stmt_returns_dynamic_param(stmt.as_ref(), dynamic_params))
                || body_returns_dynamic_param(body, dynamic_params)
        }
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| body_returns_dynamic_param(body, dynamic_params))
                || default
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_returns_dynamic_param(try_body, dynamic_params)
                || catches
                    .iter()
                    .any(|catch| body_returns_dynamic_param(&catch.body, dynamic_params))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        _ => false,
    }
}

/// Returns true when an expression can yield one of the dynamic parameters directly.
pub(super) fn expr_exposes_dynamic_param(expr: &Expr, dynamic_params: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => dynamic_params.contains(name),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            expr_exposes_dynamic_param(value, dynamic_params)
                || expr_exposes_dynamic_param(default, dynamic_params)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            expr_exposes_dynamic_param(then_expr, dynamic_params)
                || expr_exposes_dynamic_param(else_expr, dynamic_params)
        }
        ExprKind::Match { arms, default, .. } => {
            arms.iter()
                .any(|(_, arm)| expr_exposes_dynamic_param(arm, dynamic_params))
                || default
                    .as_ref()
                    .is_some_and(|expr| expr_exposes_dynamic_param(expr, dynamic_params))
        }
        ExprKind::ErrorSuppress(inner) => expr_exposes_dynamic_param(inner, dynamic_params),
        ExprKind::Assignment { value, .. } => {
            expr_exposes_dynamic_param(value, dynamic_params)
        }
        _ => false,
    }
}

