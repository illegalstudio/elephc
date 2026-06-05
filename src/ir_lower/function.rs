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
use crate::parser::ast::{ExprKind, Program, Stmt, TypeExpr};
use crate::types::{CheckResult, FunctionSig, PackedClassInfo, PhpType, TypeEnv};

/// AST parameter tuple shape used by function, method, and closure declarations.
type AstParams = [(String, Option<TypeExpr>, Option<crate::parser::ast::Expr>, bool)];

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
    lower_body_into_function(
        &mut function,
        &mut module.data,
        program,
        check_result.global_env.clone(),
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
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
    lower_body_into_function(
        &mut function,
        &mut module.data,
        body,
        env_from_signature(signature),
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
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
    if !is_static {
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
    lower_body_into_function(
        &mut function,
        &mut module.data,
        body,
        env,
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
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
    module.class_methods.push(function);
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
) {
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
        classes,
        enums,
        interfaces,
        packed_classes,
        constants,
        top_level_env,
        current_class,
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

/// Creates an initial local type environment from a function signature.
fn env_from_signature(signature: &FunctionSig) -> TypeEnv {
    signature
        .params
        .iter()
        .map(|(name, php_type)| (name.clone(), php_type.clone()))
        .collect()
}

/// Builds a fallback function signature from AST syntax when checker metadata is unavailable.
fn signature_from_ast(params: &AstParams, return_type: Option<&TypeExpr>) -> FunctionSig {
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
        variadic: None,
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
