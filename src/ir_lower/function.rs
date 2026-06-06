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
    Builder, Function, FunctionFlags, FunctionParam, Immediate, IrType, Module, Op, Ownership,
    Terminator,
};
use crate::ir_lower::context::{
    return_ir_type, type_expr_to_php_type, value_ir_type, LoweringContext,
};
use crate::ir_lower::effects_lookup;
use crate::parser::ast::{ExprKind, Program, Stmt, StmtKind, TypeExpr};
use crate::types::{CheckResult, FunctionSig, PackedClassInfo, PhpType, TypeEnv};

/// AST parameter tuple shape used by function, method, and closure declarations.
type AstParams = [(String, Option<TypeExpr>, Option<crate::parser::ast::Expr>, bool)];

const CALLED_CLASS_ID_PARAM: &str = "__elephc_called_class_id";

/// Lowers the top-level statement list as the synthetic `main` EIR function.
pub(crate) fn lower_main(
    program: &Program,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
) {
    let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    function.flags.is_main = true;
    let all_global_var_names = collect_global_var_names(program);
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        program,
        check_result.global_env.clone(),
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.callable_param_sigs,
        &check_result.classes,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        constants,
        None,
        PhpType::Void,
        &[],
        true,
        all_global_var_names,
    );
    add_closures(module, closures);
    module.add_function(function);
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
                init,
                update,
                body,
                ..
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
    body: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
) {
    let fallback = signature_from_ast(params, return_type);
    let signature = check_result.functions.get(name).unwrap_or(&fallback);
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&signature.return_type),
        signature.return_type.clone(),
    );
    function.params = function_params(signature);
    function.source_signature = Some(source_signature(name, signature));
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        body,
        env_from_signature(signature),
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.callable_param_sigs,
        &check_result.classes,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        constants,
        None,
        signature.return_type.clone(),
        &signature.params,
        false,
        std::collections::HashSet::new(),
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
) {
    let fallback = signature_from_ast(params, return_type);
    let signature = check_result
        .classes
        .get(class_name)
        .and_then(|class| method_signature(class, method_name, is_static))
        .unwrap_or(&fallback);
    let name = format!("{}::{}", class_name, method_name);
    let mut function = Function::new(
        name.clone(),
        return_ir_type(&signature.return_type),
        signature.return_type.clone(),
    );
    function.flags = FunctionFlags {
        is_method: true,
        is_static,
        ..FunctionFlags::default()
    };
    function.source_signature = Some(source_signature(&name, signature));
    let mut env = env_from_signature(signature);
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
    function.params.extend(function_params(signature));
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        body,
        env,
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.callable_param_sigs,
        &check_result.classes,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        constants,
        Some(class_name.to_string()),
        signature.return_type.clone(),
        &body_params,
        false,
        std::collections::HashSet::new(),
    );
    add_closures(module, closures);
    module.class_methods.push(function);
}

/// Lowers one closure literal into an EIR function plus any nested closure functions.
pub(crate) fn lower_closure_function(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    params: &AstParams,
    variadic: Option<&str>,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
) -> FunctionSig {
    let signature = closure_signature_from_ast(params, variadic, return_type, body);
    lower_closure_function_with_signature(parent, name, signature, body, captures)
}

/// Lowers one closure literal using contextual types for unannotated parameters.
pub(crate) fn lower_closure_function_with_context(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    params: &AstParams,
    variadic: Option<&str>,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    contextual_arg_types: &[PhpType],
) -> FunctionSig {
    let mut signature = closure_signature_from_ast(params, variadic, return_type, body);
    for (idx, (_, type_ann, _, _)) in params.iter().enumerate() {
        if type_ann.is_none() {
            if let Some(contextual_ty) = contextual_arg_types.get(idx) {
                if let Some((_, param_ty)) = signature.params.get_mut(idx) {
                    *param_ty = contextual_ty.clone();
                }
            }
        }
    }
    lower_closure_function_with_signature(parent, name, signature, body, captures)
}

/// Lowers one closure function from an already-built signature.
fn lower_closure_function_with_signature(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    signature: FunctionSig,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
) -> FunctionSig {
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&signature.return_type),
        signature.return_type.clone(),
    );
    function.flags = FunctionFlags {
        is_closure: true,
        ..FunctionFlags::default()
    };
    function.params = function_params(&signature);
    function.params.extend(closure_capture_params(captures));
    function.source_signature = Some(source_signature(name, &signature));
    let env = env_with_closure_captures(&signature, captures);
    let lowered_params = params_with_closure_captures(&signature, captures);
    let closures = lower_body_into_function(
        &mut function,
        parent.data,
        body,
        env,
        parent.top_level_env.clone(),
        parent.functions,
        parent.extern_functions,
        parent.callable_param_sigs,
        parent.classes,
        parent.enums,
        parent.interfaces,
        parent.packed_classes,
        &parent.constants,
        parent.current_class.clone(),
        signature.return_type.clone(),
        &lowered_params,
        false,
        collect_global_var_names(body),
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
    callable_param_sigs: &std::collections::HashMap<(String, String), FunctionSig>,
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
    enums: &std::collections::HashMap<String, crate::types::EnumInfo>,
    interfaces: &std::collections::HashMap<String, crate::types::InterfaceInfo>,
    packed_classes: &std::collections::HashMap<String, PackedClassInfo>,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    current_class: Option<String>,
    return_php_type: PhpType,
    params: &[(String, PhpType)],
    in_main: bool,
    all_global_var_names: std::collections::HashSet<String>,
) -> Vec<Function> {
    let owner_name = function.name.clone();
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
        callable_param_sigs,
        classes,
        enums,
        interfaces,
        packed_classes,
        constants,
        top_level_env,
        current_class,
        owner_name,
        return_php_type,
        in_main,
        all_global_var_names,
    );
    for (name, php_type) in params {
        ctx.declare_local(name, php_type.clone());
        ctx.mark_local_initialized(name);
    }
    for stmt in body {
        crate::ir_lower::stmt::lower_stmt(&mut ctx, stmt);
    }
    terminate_open_block(&mut ctx);
    ctx.into_closures()
}

/// Appends lowered closure functions to the module with stable closure-table ids.
fn add_closures(module: &mut Module, closures: Vec<Function>) {
    for closure in closures {
        module.add_closure(closure);
    }
}

/// Adds a default function terminator when the current block can still fall through.
fn terminate_open_block(ctx: &mut LoweringContext<'_, '_>) {
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    if matches!(ctx.return_php_type, PhpType::Never) {
        let message =
            ctx.intern_string("Fatal error: A never-returning function must not implicitly return\n");
        ctx.builder.terminate(Terminator::Fatal { message });
        return;
    }
    if ctx.return_type == IrType::Void {
        ctx.builder.terminate(Terminator::Return { value: None });
        return;
    }
    let value = emit_default_return_value(ctx);
    ctx.builder.terminate(Terminator::Return { value: Some(value) });
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
fn env_from_signature(signature: &FunctionSig) -> TypeEnv {
    signature
        .params
        .iter()
        .map(|(name, php_type)| (name.clone(), php_type.clone()))
        .collect()
}

/// Creates a closure environment that includes hidden captured locals.
fn env_with_closure_captures(
    signature: &FunctionSig,
    captures: &[(String, PhpType, bool)],
) -> TypeEnv {
    let mut env = env_from_signature(signature);
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
    signature_from_ast_with_variadic(params, return_type, None)
}

/// Builds an EIR closure signature and infers fallthrough-only closures as `void`.
fn closure_signature_from_ast(
    params: &AstParams,
    variadic: Option<&str>,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
) -> FunctionSig {
    let mut signature = signature_from_ast_with_variadic(params, return_type, variadic);
    if return_type.is_none()
        && !crate::types::checker::yield_validation::body_contains_yield(body)
        && !body_contains_value_return(body)
    {
        signature.return_type = PhpType::Void;
    }
    signature
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
            init,
            update,
            body,
            ..
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
) -> FunctionSig {
    FunctionSig {
        params: params
            .iter()
            .map(|(name, ty, _, _)| {
                (
                    name.clone(),
                    ty.as_ref().map(type_expr_to_php_type).unwrap_or(PhpType::Mixed),
                )
            })
            .collect(),
        defaults: params.iter().map(|(_, _, default, _)| default.clone()).collect(),
        return_type: return_type
            .map(type_expr_to_php_type)
            .unwrap_or(PhpType::Mixed),
        declared_return: return_type.is_some(),
        ref_params: params.iter().map(|(_, _, _, by_ref)| *by_ref).collect(),
        declared_params: params.iter().map(|(_, ty, _, _)| ty.is_some()).collect(),
        variadic: variadic.map(str::to_string),
        deprecation: None,
    }
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
