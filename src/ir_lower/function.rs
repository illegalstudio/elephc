//! Purpose:
//! Builds EIR function bodies from AST function-like declarations and main code.
//!
//! Called from:
//! - `crate::ir_lower::program` while assembling an EIR module.
//!
//! Key details:
//! - Function parameters are represented as metadata plus initialized PHP local
//!   slots; the Phase 03 EIR contract keeps PHP locals addressable.
//! - Every lowered function leaves all blocks terminated before validation.

use crate::ir::{
    Builder, Function, FunctionFlags, FunctionParam, GeneratorSource, Immediate, IrType, Module,
    Op, Ownership, Terminator,
};
use crate::ir_lower::context::{
    return_ir_type, type_expr_to_php_type, value_ir_type, ClosureCapture, LoweringContext,
    StaticCallableBinding,
};
use crate::ir_lower::effects_lookup;
use crate::names::php_symbol_key;
use crate::parser::ast::{
    AttributeGroup, ClassMethod, Expr, ExprKind, Program, Stmt, StmtKind, TypeExpr,
};
use crate::span::Span;
use crate::types::{
    collect_attribute_args, collect_attribute_names, CheckResult, ClassInfo, FunctionSig,
    PackedClassInfo, PhpType, TypeEnv,
};

/// AST parameter tuple shape used by function, method, and closure declarations.
type AstParams = [(
    String,
    Option<TypeExpr>,
    Option<crate::parser::ast::Expr>,
    bool,
)];

const EVAL_AOT_SCOPE_PARAM: &str = "__eir_eval_scope";

const CALLED_CLASS_ID_PARAM: &str = "__elephc_called_class_id";

/// Compile-time callable binding to seed for a self-recursive closure capture.
struct RecursiveClosureBinding {
    local_name: String,
    closure_name: String,
    signature: FunctionSig,
    capture_names: Vec<String>,
}

/// Lowers the top-level statement list as the synthetic `main` EIR function.
pub(crate) fn lower_main(
    program: &Program,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
) {
    let web = module.web;
    let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    function.flags.is_main = true;
    let all_global_var_names = collect_global_var_names(program);
    let top_level_env = web_gated_global_env(&check_result.global_env, web);
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        program,
        top_level_env.clone(),
        top_level_env,
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        constants,
        None,
        PhpType::Void,
        &[],
        None,
        true,
        all_global_var_names,
        module.source_path.clone(),
        None,
        web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Returns `global_env` with request-superglobal entries removed unless `web`.
///
/// `check_result.global_env` (the checker's top-level environment) always
/// carries the fixed `AssocArray{Str, Mixed}` type for `$_SERVER`/`$_SESSION`/…
/// because the checker seeds every scope so PHP source can read/write them
/// without a `global` declaration. Only `--web` builds pre-initialize the
/// shared `_eir_global_*` storage those types imply; a non-web `main`/function
/// env must not inherit the seeded type, or a bare read dereferences a
/// zeroed (never-initialized) global as if it were a live Hash pointer and
/// crashes. Stripping the entries here makes the env-derived type lookups
/// fall back to `Mixed`, matching `env_from_signature`'s web gate.
fn web_gated_global_env(global_env: &TypeEnv, web: bool) -> TypeEnv {
    if web {
        return global_env.clone();
    }
    let mut env = global_env.clone();
    for name in crate::superglobals::SUPERGLOBALS {
        env.remove(*name);
    }
    env
}

/// Collects PHP variable names that any function-like body declares with `global`.
fn collect_global_var_names(statements: &[Stmt]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    collect_global_var_names_in_body(statements, &mut names);
    names
}

/// Recursively scans statement bodies for `global` declarations.
fn collect_global_var_names_in_body(
    statements: &[Stmt],
    names: &mut std::collections::HashSet<String>,
) {
    for stmt in statements {
        match &stmt.kind {
            crate::parser::ast::StmtKind::Global { vars } => {
                names.extend(vars.iter().cloned());
            }
            crate::parser::ast::StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_global_var_names_in_body(then_body, names);
                for (_, body) in elseif_clauses {
                    collect_global_var_names_in_body(body, names);
                }
                if let Some(body) = else_body {
                    collect_global_var_names_in_body(body, names);
                }
            }
            crate::parser::ast::StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_global_var_names_in_body(then_body, names);
                if let Some(body) = else_body {
                    collect_global_var_names_in_body(body, names);
                }
            }
            crate::parser::ast::StmtKind::While { body, .. }
            | crate::parser::ast::StmtKind::DoWhile { body, .. }
            | crate::parser::ast::StmtKind::Foreach { body, .. }
            | crate::parser::ast::StmtKind::FunctionDecl { body, .. }
            | crate::parser::ast::StmtKind::NamespaceBlock { body, .. }
            | crate::parser::ast::StmtKind::IncludeOnceGuard { body, .. }
            | crate::parser::ast::StmtKind::Synthetic(body) => {
                collect_global_var_names_in_body(body, names);
            }
            crate::parser::ast::StmtKind::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_global_var_names_in_body(std::slice::from_ref(init.as_ref()), names);
                }
                if let Some(update) = update {
                    collect_global_var_names_in_body(std::slice::from_ref(update.as_ref()), names);
                }
                collect_global_var_names_in_body(body, names);
            }
            crate::parser::ast::StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_global_var_names_in_body(body, names);
                }
                if let Some(body) = default {
                    collect_global_var_names_in_body(body, names);
                }
            }
            crate::parser::ast::StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_global_var_names_in_body(try_body, names);
                for catch in catches {
                    collect_global_var_names_in_body(&catch.body, names);
                }
                if let Some(body) = finally_body {
                    collect_global_var_names_in_body(body, names);
                }
            }
            crate::parser::ast::StmtKind::ClassDecl { methods, .. }
            | crate::parser::ast::StmtKind::InterfaceDecl { methods, .. }
            | crate::parser::ast::StmtKind::TraitDecl { methods, .. } => {
                for method in methods {
                    collect_global_var_names_in_body(&method.body, names);
                }
            }
            _ => {}
        }
    }
}

/// Lowers one user-defined function declaration into an EIR function.
pub(crate) fn lower_user_function(
    name: &str,
    params: &AstParams,
    return_type: Option<&TypeExpr>,
    attributes: &[AttributeGroup],
    body: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
) {
    let web = module.web;
    let fallback = signature_from_ast(params, return_type);
    let signature = check_result.functions.get(name).unwrap_or(&fallback);
    let eir_signature =
        eir_signature_with_php_param_contracts(name, signature, &check_result.callable_param_sigs);
    // A generator's compiled body is a coroutine that returns the value passed
    // to `return` (Mixed, read back via `Generator::getReturn()`), not the
    // `Generator` object itself. The public signature stays `Generator` for
    // callers; only the EIR body return type becomes Mixed so `return $x`
    // lowers to a plain boxed Mixed return instead of a Generator coercion.
    let body_return_type = generator_body_return_type(body, &eir_signature.return_type);
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&body_return_type),
        body_return_type.clone(),
    );
    function.params = function_params(&eir_signature);
    function.flags.by_ref_return = signature.by_ref_return;
    function.source_signature = Some(source_signature(name, &eir_signature));
    function.signature = Some(eir_runtime_metadata_signature(&eir_signature));
    function.attribute_names = check_result
        .function_attribute_names
        .get(name)
        .cloned()
        .unwrap_or_else(|| collect_attribute_names(attributes));
    function.attribute_args = check_result
        .function_attribute_args
        .get(name)
        .cloned()
        .unwrap_or_else(|| collect_attribute_args(attributes));
    attach_generator_source_if_needed(&mut function, body, eir_signature.params.len());
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        body,
        env_from_signature(&eir_signature, web),
        web_gated_global_env(&check_result.global_env, web),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        constants,
        None,
        body_return_type.clone(),
        &eir_signature.params,
        None,
        false,
        std::collections::HashSet::new(),
        module.source_path.clone(),
        None,
        web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Lowers one class-like method body into an EIR class-method function.
pub(crate) fn lower_class_method(
    class_name: &str,
    method_name: &str,
    is_static: bool,
    params: &AstParams,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
) {
    let web = module.web;
    let fallback = signature_from_ast(params, return_type);
    let signature = module
        .class_infos
        .get(class_name)
        .and_then(|class| method_signature(class, method_name, is_static))
        .cloned()
        .unwrap_or(fallback);
    let name = format!("{}::{}", class_name, method_name);
    // Generator methods lower their body as a Mixed-returning coroutine; see
    // `generator_body_return_type`.
    let method_body_return_type = generator_body_return_type(body, &signature.return_type);
    let mut function = Function::new(
        name.clone(),
        return_ir_type(&method_body_return_type),
        method_body_return_type.clone(),
    );
    function.flags = FunctionFlags {
        is_method: true,
        is_static,
        by_ref_return: signature.by_ref_return,
        ..FunctionFlags::default()
    };
    function.source_signature = Some(source_signature(&name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    let mut env = env_from_signature(&signature, web);
    let mut body_params = signature.params.clone();
    if is_static {
        let hidden_called_class = (CALLED_CLASS_ID_PARAM.to_string(), PhpType::Int);
        function.params.push(FunctionParam {
            name: hidden_called_class.0.clone(),
            ir_type: value_ir_type(&hidden_called_class.1),
            php_type: hidden_called_class.1.clone(),
            by_ref: false,
            variadic: false,
        });
        env.insert(hidden_called_class.0.clone(), hidden_called_class.1.clone());
        body_params.insert(0, hidden_called_class);
    } else {
        let this_type = PhpType::Object(class_name.to_string());
        function.params.push(FunctionParam {
            name: "this".to_string(),
            ir_type: value_ir_type(&this_type),
            php_type: this_type.clone(),
            by_ref: false,
            variadic: false,
        });
        env.insert("this".to_string(), this_type.clone());
        body_params.insert(0, ("this".to_string(), this_type));
    }
    function.params.extend(function_params(&signature));
    attach_generator_source_if_needed(&mut function, body, body_params.len());
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        body,
        env,
        web_gated_global_env(&check_result.global_env, web),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        constants,
        Some(class_name.to_string()),
        method_body_return_type.clone(),
        &body_params,
        None,
        false,
        std::collections::HashSet::new(),
        module.source_path.clone(),
        None,
        web,
    );
    add_closures(module, closures);
    module.class_methods.push(function);
}

/// Lowers one no-scope literal eval fragment as an internal EIR function.
pub(crate) fn lower_eval_aot_function(
    name: &str,
    body: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
) {
    let return_type = PhpType::Mixed;
    let signature = FunctionSig {
        params: Vec::new(),
        param_type_exprs: Vec::new(),
        param_attributes: Vec::new(),
        defaults: Vec::new(),
        return_type: return_type.clone(),
        declared_return: false,
        by_ref_return: false,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
        deprecation: None,
    };
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&return_type),
        return_type.clone(),
    );
    function.source_signature = Some(source_signature(name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        body,
        TypeEnv::new(),
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        constants,
        None,
        return_type,
        &[],
        None,
        false,
        collect_global_var_names(body),
        module.source_path.clone(),
        None,
        module.web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Lowers one literal eval fragment as an internal scope-aware EIR function.
pub(crate) fn lower_eval_aot_scope_function(
    name: &str,
    body: &[Stmt],
    scope_reads: &std::collections::BTreeSet<String>,
    scope_direct_writes: &std::collections::BTreeSet<String>,
    scope_flush_writes: &std::collections::BTreeSet<String>,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
) {
    let return_type = PhpType::Mixed;
    let use_read_params =
        !scope_reads.is_empty() && scope_direct_writes.is_empty() && scope_flush_writes.is_empty();
    let params = if use_read_params {
        scope_reads
            .iter()
            .map(|name| (name.clone(), PhpType::Mixed))
            .collect::<Vec<_>>()
    } else {
        vec![(EVAL_AOT_SCOPE_PARAM.to_string(), PhpType::Int)]
    };
    let signature = FunctionSig {
        params,
        param_type_exprs: Vec::new(),
        param_attributes: Vec::new(),
        defaults: Vec::new(),
        return_type: return_type.clone(),
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![
            false;
            if use_read_params {
                scope_reads.len()
            } else {
                1
            }
        ],
        declared_params: vec![
            false;
            if use_read_params {
                scope_reads.len()
            } else {
                1
            }
        ],
        variadic: None,
        deprecation: None,
    };
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&return_type),
        return_type.clone(),
    );
    function.params = function_params(&signature);
    function.source_signature = Some(source_signature(name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    let mut env = TypeEnv::new();
    for (param_name, param_type) in &signature.params {
        env.insert(param_name.clone(), param_type.clone());
    }
    let eval_scope_reads = (!use_read_params).then(|| {
        (
            EVAL_AOT_SCOPE_PARAM.to_string(),
            scope_reads.iter().cloned().collect(),
            scope_direct_writes.iter().cloned().collect(),
            scope_flush_writes.clone(),
        )
    });
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        body,
        env,
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        constants,
        None,
        return_type,
        &signature.params,
        None,
        false,
        collect_global_var_names(body),
        module.source_path.clone(),
        eval_scope_reads,
        module.web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Builds fallback method signature metadata from parsed class-like method syntax.
pub(crate) fn method_signature_from_ast(method: &ClassMethod) -> FunctionSig {
    let mut signature = signature_from_ast_with_variadic(
        &method.params,
        method.return_type.as_ref(),
        method.variadic.as_deref(),
        method.variadic_by_ref,
    );
    if !method.variadic_by_ref {
        if let Some(variadic_type) = &method.variadic_type {
            if let Some((_, php_type)) = signature.params.last_mut() {
                *php_type = type_expr_to_php_type(variadic_type);
            }
            if let Some(declared) = signature.declared_params.last_mut() {
                *declared = true;
            }
        }
    }
    signature
}

/// Lowers a synthetic `_class_propinit_<id>` function for dynamic by-name allocation.
pub(crate) fn lower_property_init_thunk(
    class_name: &str,
    class_info: &ClassInfo,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
) {
    if !class_info.defaults.iter().any(|default| default.is_some()) {
        return;
    }
    let web = module.web;
    let body = property_init_body(class_info);
    let function_name = format!("_class_propinit_{}", class_info.class_id);
    let this_type = PhpType::Object(class_name.to_string());
    let mut function = Function::new(function_name.clone(), IrType::Void, PhpType::Void);
    function.flags.is_synthetic = true;
    function.params.push(FunctionParam {
        name: "this".to_string(),
        ir_type: value_ir_type(&this_type),
        php_type: this_type.clone(),
        by_ref: false,
        variadic: false,
    });
    let sig = FunctionSig {
        params: vec![("this".to_string(), this_type.clone())],
        param_type_exprs: vec![None],
        param_attributes: Vec::new(),
        defaults: vec![None],
        return_type: PhpType::Void,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false],
        declared_params: vec![false],
        variadic: None,
        deprecation: None,
    };
    function.source_signature = Some(source_signature(&function_name, &sig));
    function.signature = Some(eir_runtime_metadata_signature(&sig));
    let mut env = TypeEnv::new();
    env.insert("this".to_string(), this_type.clone());
    let params = vec![("this".to_string(), this_type)];
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        &body,
        env,
        web_gated_global_env(&check_result.global_env, web),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        constants,
        Some(class_name.to_string()),
        PhpType::Void,
        &params,
        None,
        false,
        std::collections::HashSet::new(),
        module.source_path.clone(),
        None,
        web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Builds `$this->property = <default>;` statements for property-default initialization.
///
/// A null default whose slot type cannot represent null (a scalar slot rebound by
/// constructor-argument propagation) is skipped: those slots are always overwritten
/// before an observable read, and the store would be unrepresentable.
fn property_init_body(class_info: &ClassInfo) -> Vec<Stmt> {
    let span = Span::dummy();
    class_info
        .defaults
        .iter()
        .enumerate()
        .filter_map(|(index, default)| {
            let default = default.as_ref()?;
            let (name, php_type) = class_info.properties.get(index)?;
            if matches!(default.kind, ExprKind::Null) && !php_type.null_property_default_required() {
                return None;
            }
            let property = name.clone();
            Some(Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::Assignment {
                        target: Box::new(Expr::new(
                            ExprKind::PropertyAccess {
                                object: Box::new(Expr::new(ExprKind::This, span)),
                                property,
                            },
                            span,
                        )),
                        value: Box::new(default.clone()),
                        result_target: None,
                        prelude: Vec::new(),
                        conditional_value_temp: None,
                    },
                    span,
                )),
                span,
            ))
        })
        .collect()
}

/// Lowers one closure literal into an EIR function plus any nested closure functions.
pub(crate) fn lower_closure_function(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    params: &AstParams,
    variadic: Option<&str>,
    variadic_by_ref: bool,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    self_ref_callable_capture: Option<&str>,
    by_ref_return: bool,
) -> FunctionSig {
    let mut signature = closure_signature_from_ast(
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        captures,
        parent.classes,
    );
    signature.by_ref_return = by_ref_return;
    lower_closure_function_with_signature(
        parent,
        name,
        signature,
        body,
        captures,
        self_ref_callable_capture,
    )
}

/// Lowers one closure literal using contextual types for unannotated parameters.
pub(crate) fn lower_closure_function_with_context(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    params: &AstParams,
    variadic: Option<&str>,
    variadic_by_ref: bool,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    contextual_arg_types: &[PhpType],
    self_ref_callable_capture: Option<&str>,
    by_ref_return: bool,
) -> FunctionSig {
    let mut signature = closure_signature_from_ast(
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        captures,
        parent.classes,
    );
    signature.by_ref_return = by_ref_return;
    for (idx, (_, type_ann, _, _)) in params.iter().enumerate() {
        if type_ann.is_none() {
            if let Some(contextual_ty) = contextual_arg_types.get(idx) {
                if let Some((_, param_ty)) = signature.params.get_mut(idx) {
                    *param_ty = contextual_ty.clone();
                }
            }
        }
    }
    lower_closure_function_with_signature(
        parent,
        name,
        signature,
        body,
        captures,
        self_ref_callable_capture,
    )
}

/// Lowers one closure function from an already-built signature.
fn lower_closure_function_with_signature(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    signature: FunctionSig,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    self_ref_callable_capture: Option<&str>,
) -> FunctionSig {
    // Generator closures lower their body as a Mixed-returning coroutine; see
    // `generator_body_return_type`.
    let closure_body_return_type = generator_body_return_type(body, &signature.return_type);
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&closure_body_return_type),
        closure_body_return_type.clone(),
    );
    function.flags = FunctionFlags {
        is_closure: true,
        by_ref_return: signature.by_ref_return,
        ..FunctionFlags::default()
    };
    function.params = function_params(&signature);
    function.params.extend(closure_capture_params(captures));
    function.source_signature = Some(source_signature(name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    attach_generator_source_if_needed(&mut function, body, signature.params.len());
    let env = env_with_closure_captures(&signature, captures, parent.web);
    let lowered_params = params_with_closure_captures(&signature, captures);
    let recursive_binding = self_ref_callable_capture.map(|local_name| RecursiveClosureBinding {
        local_name: local_name.to_string(),
        closure_name: name.to_string(),
        signature: signature.clone(),
        capture_names: captures
            .iter()
            .map(|(capture_name, _, _)| capture_name.clone())
            .collect(),
    });
    let closures = lower_body_into_function(
        &mut function,
        parent.data,
        body,
        env,
        parent.top_level_env.clone(),
        parent.functions,
        parent.extern_functions,
        parent.extern_globals,
        parent.callable_param_sigs,
        parent.return_alias_summaries,
        parent.fiber_return_sigs,
        parent.classes,
        parent.enums,
        parent.interfaces,
        parent.packed_classes,
        parent.throw_access_sites,
        parent.builtin_call_types,
        &parent.constants,
        parent.current_class.clone(),
        closure_body_return_type.clone(),
        &lowered_params,
        recursive_binding,
        false,
        collect_global_var_names(body),
        parent.source_path().map(str::to_string),
        None,
        parent.web,
    );
    parent.extend_closures(std::iter::once(function).chain(closures));
    signature
}

/// Lowers the supplied statements into `function` and appends a default terminator if needed.
fn lower_body_into_function(
    function: &mut Function,
    data: &mut crate::ir::DataPool,
    body: &[Stmt],
    env: TypeEnv,
    top_level_env: TypeEnv,
    functions: &std::collections::HashMap<String, FunctionSig>,
    extern_functions: &std::collections::HashMap<String, crate::types::ExternFunctionSig>,
    extern_globals: &std::collections::HashMap<String, PhpType>,
    callable_param_sigs: &std::collections::HashMap<(String, String), FunctionSig>,
    return_alias_summaries: &crate::types::ReturnAliasSummaries,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
    enums: &std::collections::HashMap<String, crate::types::EnumInfo>,
    interfaces: &std::collections::HashMap<String, crate::types::InterfaceInfo>,
    packed_classes: &std::collections::HashMap<String, PackedClassInfo>,
    throw_access_sites: &std::collections::HashMap<Span, crate::types::ThrowAccessInfo>,
    builtin_call_types: &std::collections::HashMap<Span, PhpType>,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    current_class: Option<String>,
    return_php_type: PhpType,
    params: &[(String, PhpType)],
    recursive_closure_binding: Option<RecursiveClosureBinding>,
    in_main: bool,
    all_global_var_names: std::collections::HashSet<String>,
    source_path: Option<String>,
    eval_scope_reads: Option<(
        String,
        std::collections::HashSet<String>,
        std::collections::HashSet<String>,
        std::collections::BTreeSet<String>,
    )>,
    web: bool,
) -> Vec<Function> {
    let owner_name = function.name.clone();
    let function_by_ref_return = function.flags.by_ref_return;
    let by_ref_params = function
        .params
        .iter()
        .map(|param| param.by_ref)
        .collect::<Vec<_>>();
    let mut builder = Builder::new(function);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let mut ctx = LoweringContext::new(
        builder,
        data,
        env,
        functions,
        extern_functions,
        extern_globals,
        callable_param_sigs,
        return_alias_summaries,
        fiber_return_sigs,
        classes,
        enums,
        interfaces,
        packed_classes,
        throw_access_sites,
        builtin_call_types,
        constants,
        top_level_env,
        current_class,
        owner_name,
        return_php_type,
        in_main,
        all_global_var_names,
        source_path,
        web,
    );
    ctx.by_ref_return = function_by_ref_return;
    if let Some((scope_param, read_names, write_names, flush_names)) = eval_scope_reads {
        ctx.enable_eval_scope_access(scope_param, read_names, write_names, flush_names);
    }
    for (index, (name, php_type)) in params.iter().enumerate() {
        ctx.declare_local(name, php_type.clone());
        ctx.mark_local_initialized(name);
        if by_ref_params.get(index).copied().unwrap_or(false) {
            ctx.mark_ref_bound_local(name);
        }
    }
    seed_recursive_closure_binding(&mut ctx, recursive_closure_binding);
    for stmt in body {
        crate::ir_lower::stmt::lower_stmt(&mut ctx, stmt);
    }
    terminate_open_block(&mut ctx);
    // Final storage types are now known: erase deferred loop-store releases that
    // guard slots which never widened to lifetime-tracked storage (issue #534).
    ctx.builder.prune_untracked_release_local_slot_ops();
    // Likewise, erase provisional releases for concrete local loads unless a
    // later store widened their final frame slot to Mixed (issue #538).
    ctx.builder.prune_borrowed_local_load_release_ops();
    ctx.into_closures()
}

/// Seeds a self-recursive closure capture as a static callable local inside its body.
fn seed_recursive_closure_binding(
    ctx: &mut LoweringContext<'_, '_>,
    binding: Option<RecursiveClosureBinding>,
) {
    let Some(binding) = binding else {
        return;
    };
    let captures = binding
        .capture_names
        .iter()
        .map(|capture_name| ClosureCapture {
            value: ctx.load_local(capture_name, None).value,
        })
        .collect();
    ctx.bind_static_callable_local(
        &binding.local_name,
        StaticCallableBinding::Closure {
            name: binding.closure_name,
            signature: binding.signature,
            captures,
        },
    );
}

/// Appends lowered closure functions to the module with stable closure-table ids.
fn add_closures(module: &mut Module, closures: Vec<Function>) {
    for closure in closures {
        module.add_closure(closure);
    }
}

/// Retains generator source metadata until the EIR backend has native generator-state lowering.
fn attach_generator_source_if_needed(
    function: &mut Function,
    body: &[Stmt],
    visible_param_count: usize,
) {
    if !crate::types::checker::yield_validation::body_contains_yield(body)
        && !is_generator_return_type(&function.return_php_type)
    {
        return;
    }
    function.flags.is_generator = true;
    function.generator_source = Some(GeneratorSource {
        body: body.to_vec(),
        visible_param_count,
    });
}

/// Returns true when checked function metadata already identifies a generator return.
fn is_generator_return_type(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Object(name) if name.trim_start_matches('\\') == "Generator")
}

/// Returns the EIR return type to lower a function body with.
///
/// For a generator (body contains `yield`, or the declared return type is
/// `Generator`) the compiled body is a coroutine whose `return` produces the
/// value later read by `Generator::getReturn()`, so the body return type is
/// `Mixed`. For every other function it is the declared signature return type.
fn generator_body_return_type(body: &[Stmt], signature_return: &PhpType) -> PhpType {
    if crate::types::checker::yield_validation::body_contains_yield(body)
        || is_generator_return_type(signature_return)
    {
        PhpType::Mixed
    } else {
        signature_return.clone()
    }
}

/// Adds a default function terminator when the current block can still fall through.
fn terminate_open_block(ctx: &mut LoweringContext<'_, '_>) {
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    if matches!(ctx.return_php_type, PhpType::Never) {
        let message = ctx
            .intern_string("Fatal error: A never-returning function must not implicitly return\n");
        ctx.builder.terminate(Terminator::Fatal { message });
        return;
    }
    if ctx.return_type == IrType::Void {
        ctx.emit_eval_scope_finalizer(None);
        ctx.builder.terminate(Terminator::Return { value: None });
        return;
    }
    ctx.emit_eval_scope_finalizer(None);
    let value = emit_default_return_value(ctx);
    ctx.builder
        .terminate(Terminator::Return { value: Some(value) });
}

/// Emits a placeholder value compatible with the function return storage type.
fn emit_default_return_value(ctx: &mut LoweringContext<'_, '_>) -> crate::ir::ValueId {
    match ctx.return_type {
        IrType::I64 => ctx
            .builder
            .emit_with_effects(
                Op::ConstNull,
                Vec::new(),
                None,
                IrType::I64,
                PhpType::Void,
                Ownership::NonHeap,
                Op::ConstNull.default_effects(),
                None,
            )
            .expect("const_null produces a value"),
        IrType::F64 => ctx
            .builder
            .emit_with_effects(
                Op::ConstF64,
                Vec::new(),
                Some(Immediate::F64(0.0)),
                IrType::F64,
                PhpType::Float,
                Ownership::NonHeap,
                Op::ConstF64.default_effects(),
                None,
            )
            .expect("const_f64 produces a value"),
        IrType::Str => {
            let data = ctx.intern_string("");
            ctx.builder
                .emit_with_effects(
                    Op::ConstStr,
                    Vec::new(),
                    Some(Immediate::Data(data)),
                    IrType::Str,
                    PhpType::Str,
                    Ownership::Persistent,
                    Op::ConstStr.default_effects(),
                    None,
                )
                .expect("const_str produces a value")
        }
        IrType::TaggedScalar => ctx
            .builder
            .emit_with_effects(
                Op::ConstNull,
                Vec::new(),
                None,
                IrType::TaggedScalar,
                PhpType::TaggedScalar,
                Ownership::NonHeap,
                Op::ConstNull.default_effects(),
                None,
            )
            .expect("const_null produces a tagged scalar value"),
        IrType::Heap(_) if ctx.return_php_type.codegen_repr() == PhpType::Mixed => {
            // A Mixed-returning body that falls through yields PHP null. This is
            // how a generator with no explicit `return` produces the value later
            // read by `Generator::getReturn()`. Mirror the `return null;` path
            // (`coerce_to_return_type`): a null scalar boxed into a Mixed cell.
            let null_value = ctx
                .builder
                .emit_with_effects(
                    Op::ConstNull,
                    Vec::new(),
                    None,
                    IrType::I64,
                    PhpType::Void,
                    Ownership::NonHeap,
                    Op::ConstNull.default_effects(),
                    None,
                )
                .expect("const_null produces a value");
            // A fresh null is non-refcounted: there is no producer reference to
            // release, so this boxes directly rather than via box_value_as_mixed
            // (issue #484).
            ctx.emit_value(
                Op::MixedBox,
                vec![null_value],
                None,
                ctx.return_php_type.clone(),
                Op::MixedBox.default_effects(),
                None,
            )
            .value
        }
        IrType::Heap(_) => {
            let lowered = ctx.emit_value(
                Op::RuntimeCall,
                Vec::new(),
                None,
                ctx.return_php_type.clone(),
                effects_lookup::runtime_effects(),
                None,
            );
            lowered.value
        }
        IrType::Void => unreachable!("void returns do not materialize values"),
    }
}

/// Converts a checker signature into EIR parameter metadata.
fn function_params(signature: &FunctionSig) -> Vec<FunctionParam> {
    signature
        .params
        .iter()
        .enumerate()
        .map(|(index, (name, php_type))| FunctionParam {
            name: name.clone(),
            ir_type: value_ir_type(php_type),
            php_type: php_type.clone(),
            by_ref: signature.ref_params.get(index).copied().unwrap_or(false),
            variadic: signature.variadic.as_deref() == Some(name.as_str()),
        })
        .collect()
}

/// Returns an EIR ABI signature that keeps dynamic untyped PHP parameters boxed.
pub(crate) fn eir_signature_with_php_param_contracts(
    owner_name: &str,
    signature: &FunctionSig,
    callable_param_sigs: &std::collections::HashMap<(String, String), FunctionSig>,
) -> FunctionSig {
    let mut eir_signature = signature.clone();
    let mut has_dynamic_untyped_param = false;
    for (index, (name, php_type)) in eir_signature.params.iter_mut().enumerate() {
        let declared = signature
            .declared_params
            .get(index)
            .copied()
            .unwrap_or(false);
        let by_ref = signature.ref_params.get(index).copied().unwrap_or(false);
        let variadic = signature.variadic.as_deref() == Some(name.as_str());
        if !declared && !by_ref && !variadic {
            if preserve_untyped_eir_param_contract(
                owner_name,
                index,
                name,
                php_type,
                callable_param_sigs,
            ) {
                continue;
            }
            *php_type = PhpType::Mixed;
            has_dynamic_untyped_param = true;
        }
    }
    if has_dynamic_untyped_param && !signature.declared_return {
        eir_signature.return_type = dynamic_param_container_return_type(&eir_signature.return_type);
    }
    eir_signature
}

/// Marks boxed ABI parameters as materialization targets for reused runtime invokers.
fn eir_runtime_metadata_signature(signature: &FunctionSig) -> FunctionSig {
    let mut signature = signature.clone();
    for (index, (_, php_type)) in signature.params.iter().enumerate() {
        if matches!(php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            if let Some(declared) = signature.declared_params.get_mut(index) {
                *declared = true;
            }
        }
    }
    signature
}

/// Returns true when an inferred untyped parameter has an EIR-safe concrete ABI contract.
fn preserve_untyped_eir_param_contract(
    owner_name: &str,
    param_index: usize,
    param_name: &str,
    php_type: &PhpType,
    callable_param_sigs: &std::collections::HashMap<(String, String), FunctionSig>,
) -> bool {
    magic_method_param_keeps_eir_contract(owner_name, param_index, php_type)
        || matches!(php_type.codegen_repr(), PhpType::Callable)
        || callable_param_sigs.contains_key(&(owner_name.to_string(), param_name.to_string()))
}

/// Returns whether a checker-patched magic-method parameter must keep its real ABI type.
fn magic_method_param_keeps_eir_contract(
    owner_name: &str,
    param_index: usize,
    php_type: &PhpType,
) -> bool {
    let Some((_, method_name)) = owner_name.rsplit_once("::") else {
        return false;
    };
    let method_key = php_symbol_key(method_name);
    match method_key.as_str() {
        "__get" | "__isset" | "__unset" => {
            param_index == 0 && matches!(php_type.codegen_repr(), PhpType::Str)
        }
        "__set" => {
            param_index == 0 && matches!(php_type.codegen_repr(), PhpType::Str)
        }
        "__call" | "__callstatic" => {
            // The $args array keeps its contract only once call sites have
            // specialized the element type. The checker seeds it as
            // Array<Never>; eval-only magic calls never specialize it, and a
            // Never element would lower every $args[N] read to an empty
            // constant, so those fall back to the boxed Mixed widening.
            (param_index == 0 && matches!(php_type.codegen_repr(), PhpType::Str))
                || (param_index == 1
                    && matches!(php_type.codegen_repr(), PhpType::Array(_))
                    // Check the raw element type: codegen_repr normalizes the
                    // Never seed to Void and would hide it.
                    && !matches!(
                        php_type,
                        PhpType::Array(elem) if matches!(elem.as_ref(), PhpType::Never)
                    ))
        }
        _ => false,
    }
}

/// Widens inferred container return elements that may be built from dynamic params.
fn dynamic_param_container_return_type(return_type: &PhpType) -> PhpType {
    match return_type.codegen_repr() {
        PhpType::Array(_) => PhpType::Array(Box::new(PhpType::Mixed)),
        PhpType::AssocArray { key, .. } => PhpType::AssocArray {
            key,
            value: Box::new(PhpType::Mixed),
        },
        PhpType::Union(members) => PhpType::Union(
            members
                .iter()
                .map(dynamic_param_container_return_type)
                .collect(),
        ),
        other => other,
    }
}

/// Converts closure captures into hidden EIR ABI parameters.
fn closure_capture_params(captures: &[(String, PhpType, bool)]) -> Vec<FunctionParam> {
    captures
        .iter()
        .map(|(name, php_type, by_ref)| FunctionParam {
            name: name.clone(),
            ir_type: value_ir_type(php_type),
            php_type: php_type.clone(),
            by_ref: *by_ref,
            variadic: false,
        })
        .collect()
}

/// Creates an initial local type environment from a function signature.
///
/// Under `--web`, request superglobals (`$_SERVER`/`$_GET`/`$_POST`/`$_SESSION`/…)
/// are seeded here so `local_type` returns their fixed `AssocArray{Str, Mixed}`
/// type inside function bodies. Without this, `$_SESSION = []` in a function
/// contextualizes as a scalar `Array(Never)` instead of a hash and crashes the
/// runtime when the store targets the shared `_eir_global__u_SESSION` slot.
/// `or_insert` never clobbers a parameter that happens to share a superglobal
/// name.
///
/// Outside `--web` nothing pre-initializes that shared global storage, so the
/// seeding is skipped: `local_type` falls back to `Mixed` for these names,
/// matching pre-superglobal-support behavior and avoiding a read/index-write
/// that dereferences a never-initialized (zeroed) global as a live Hash
/// pointer. See `crate::ir_lower::context::LoweringContext::global_alias_type`
/// for the matching gate on the fallback lookup path.
fn env_from_signature(signature: &FunctionSig, web: bool) -> TypeEnv {
    let mut env: TypeEnv = signature
        .params
        .iter()
        .map(|(name, php_type)| (name.clone(), php_type.clone()))
        .collect();
    if web {
        for name in crate::superglobals::SUPERGLOBALS {
            env.entry((*name).to_string())
                .or_insert_with(crate::superglobals::superglobal_type);
        }
    }
    env
}

/// Creates a closure environment that includes hidden captured locals.
fn env_with_closure_captures(
    signature: &FunctionSig,
    captures: &[(String, PhpType, bool)],
    web: bool,
) -> TypeEnv {
    let mut env = env_from_signature(signature, web);
    for (name, php_type, _) in captures {
        env.insert(name.clone(), php_type.clone());
    }
    env
}

/// Returns visible params followed by hidden closure capture params for slot setup.
fn params_with_closure_captures(
    signature: &FunctionSig,
    captures: &[(String, PhpType, bool)],
) -> Vec<(String, PhpType)> {
    let mut params = signature.params.clone();
    params.extend(
        captures
            .iter()
            .map(|(name, php_type, _)| (name.clone(), php_type.clone())),
    );
    params
}

/// Builds a fallback function signature from AST syntax when checker metadata is unavailable.
fn signature_from_ast(params: &AstParams, return_type: Option<&TypeExpr>) -> FunctionSig {
    signature_from_ast_with_variadic(params, return_type, None, false)
}

/// Builds an EIR closure signature and infers fallthrough-only closures as `void`.
fn closure_signature_from_ast(
    params: &AstParams,
    variadic: Option<&str>,
    variadic_by_ref: bool,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> FunctionSig {
    let mut signature =
        signature_from_ast_with_variadic(params, return_type, variadic, variadic_by_ref);
    if crate::types::checker::yield_validation::body_contains_yield(body) {
        signature.return_type = PhpType::Object("Generator".to_string());
        return signature;
    }
    if return_type.is_none() {
        if let Some(return_ty) =
            direct_closure_return_type(body, captures, &signature.params, classes)
        {
            signature.return_type = return_ty;
        } else if !body_contains_value_return(body) {
            signature.return_type = PhpType::Void;
        }
    }
    signature
}

/// Infers a closure return type for the no-fallthrough `return <expr>;` shape.
fn direct_closure_return_type(
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    params: &[(String, PhpType)],
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> Option<PhpType> {
    let [stmt] = body else {
        return None;
    };
    let StmtKind::Return(Some(expr)) = &stmt.kind else {
        return None;
    };
    Some(direct_closure_return_expr_type(expr, captures, params, classes))
}

/// Returns a direct closure return expression type, consulting capture and parameter
/// metadata first. A bare `return $x` where `$x` is a parameter must adopt the parameter's
/// declared type (e.g. `mixed`) rather than falling back to the syntactic integer default,
/// which would otherwise coerce a boxed Mixed argument to an integer on return. A
/// `return $obj->prop` where `$obj` is a captured/parameter object of a known class adopts
/// the property's declared type, so a `fn &() => $o->items` closure returns the array type
/// rather than the syntactic integer default.
fn direct_closure_return_expr_type(
    expr: &crate::parser::ast::Expr,
    captures: &[(String, PhpType, bool)],
    params: &[(String, PhpType)],
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> PhpType {
    if let ExprKind::Variable(name) = &expr.kind {
        if let Some((_, php_type, _)) = captures
            .iter()
            .find(|(capture_name, _, _)| capture_name == name)
        {
            return php_type.clone();
        }
        if let Some((_, php_type)) = params.iter().find(|(param_name, _)| param_name == name) {
            return php_type.clone();
        }
    }
    if let ExprKind::PropertyAccess { object, property } = &expr.kind {
        let receiver_name = match &object.kind {
            ExprKind::Variable(name) => Some(name.as_str()),
            ExprKind::This => Some("this"),
            _ => None,
        };
        if let Some(receiver_name) = receiver_name {
            let receiver_ty = captures
                .iter()
                .find(|(capture_name, _, _)| capture_name == receiver_name)
                .map(|(_, ty, _)| ty)
                .or_else(|| {
                    params
                        .iter()
                        .find(|(param_name, _)| param_name == receiver_name)
                        .map(|(_, ty)| ty)
                });
            if let Some(PhpType::Object(class)) = receiver_ty {
                if let Some(info) = classes.get(class.trim_start_matches('\\')) {
                    if let Some((_, ty)) =
                        info.properties.iter().find(|(name, _)| name == property)
                    {
                        return ty.clone();
                    }
                }
            }
        }
    }
    crate::types::checker::infer_expr_type_syntactic(expr)
}

/// Returns true when a statement list contains a `return <expr>` for its own function body.
fn body_contains_value_return(statements: &[Stmt]) -> bool {
    statements.iter().any(stmt_contains_value_return)
}

/// Returns true when `stmt` can return a value from the currently lowered function body.
fn stmt_contains_value_return(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(Some(_)) => true,
        StmtKind::Return(None) => false,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            body_contains_value_return(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| body_contains_value_return(body))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body_contains_value_return(body))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            body_contains_value_return(then_body)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body_contains_value_return(body))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => body_contains_value_return(body),
        StmtKind::For {
            init, update, body, ..
        } => {
            init.as_ref()
                .is_some_and(|stmt| stmt_contains_value_return(stmt.as_ref()))
                || update
                    .as_ref()
                    .is_some_and(|stmt| stmt_contains_value_return(stmt.as_ref()))
                || body_contains_value_return(body)
        }
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| body_contains_value_return(body))
                || default
                    .as_ref()
                    .is_some_and(|body| body_contains_value_return(body))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_contains_value_return(try_body)
                || catches
                    .iter()
                    .any(|catch| body_contains_value_return(&catch.body))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body_contains_value_return(body))
        }
        _ => false,
    }
}

/// Builds a fallback function signature from AST syntax and optional variadic metadata.
fn signature_from_ast_with_variadic(
    params: &AstParams,
    return_type: Option<&TypeExpr>,
    variadic: Option<&str>,
    variadic_by_ref: bool,
) -> FunctionSig {
    let mut signature = FunctionSig {
        params: params
            .iter()
            .map(|(name, ty, _, _)| {
                (
                    name.clone(),
                    ty.as_ref()
                        .map(type_expr_to_php_type)
                        .unwrap_or(PhpType::Mixed),
                )
            })
            .collect(),
        param_type_exprs: params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.clone())
            .collect(),
        param_attributes: Vec::new(),
        defaults: params
            .iter()
            .map(|(_, _, default, _)| default.clone())
            .collect(),
        return_type: return_type
            .map(type_expr_to_php_type)
            .unwrap_or(PhpType::Mixed),
        declared_return: return_type.is_some(),
        by_ref_return: false,
        ref_params: params.iter().map(|(_, _, _, by_ref)| *by_ref).collect(),
        declared_params: params.iter().map(|(_, ty, _, _)| ty.is_some()).collect(),
        variadic: variadic.map(str::to_string),
        deprecation: None,
    };
    append_variadic_param_slot(&mut signature, variadic_by_ref);
    signature
}

/// Adds the variadic `array<mixed>` parameter slot omitted from parsed parameter tuples.
fn append_variadic_param_slot(signature: &mut FunctionSig, variadic_by_ref: bool) {
    let Some(variadic) = signature.variadic.clone() else {
        return;
    };
    if signature.params.iter().any(|(name, _)| name == &variadic) {
        return;
    }
    signature
        .params
        .push((variadic, PhpType::Array(Box::new(PhpType::Mixed))));
    signature.param_type_exprs.push(None);
    signature.defaults.push(None);
    signature.ref_params.push(variadic_by_ref);
    signature.declared_params.push(false);
}

/// Finds a method signature using PHP's case-insensitive method key convention.
fn method_signature<'a>(
    class: &'a crate::types::ClassInfo,
    method_name: &str,
    is_static: bool,
) -> Option<&'a FunctionSig> {
    let methods = if is_static {
        &class.static_methods
    } else {
        &class.methods
    };
    methods
        .get(method_name)
        .or_else(|| methods.get(&method_name.to_ascii_lowercase()))
}

/// Formats a compact source signature string for textual EIR diagnostics.
fn source_signature(name: &str, signature: &FunctionSig) -> String {
    let params = signature
        .params
        .iter()
        .map(|(param, php_type)| format!("{}: {}", param, php_type))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}({}) -> {}", name, params, signature.return_type)
}
