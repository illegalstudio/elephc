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
use crate::types::{CheckResult, FunctionSig, PhpType, TypeEnv};

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
    lower_body_into_function(
        &mut function,
        &mut module.data,
        program,
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
        constants,
        PhpType::Void,
        &[],
    );
    module.add_function(function);
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
        &check_result.functions,
        &check_result.extern_functions,
        constants,
        signature.return_type.clone(),
        &signature.params,
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
    function.params = function_params(signature);
    function.flags = FunctionFlags {
        is_method: true,
        is_static,
        ..FunctionFlags::default()
    };
    function.source_signature = Some(source_signature(&name, signature));
    let mut env = env_from_signature(signature);
    if !is_static {
        env.insert("this".to_string(), PhpType::Object(class_name.to_string()));
    }
    lower_body_into_function(
        &mut function,
        &mut module.data,
        body,
        env,
        &check_result.functions,
        &check_result.extern_functions,
        constants,
        signature.return_type.clone(),
        &signature.params,
    );
    module.class_methods.push(function);
}

/// Lowers the supplied statements into `function` and appends a default terminator if needed.
fn lower_body_into_function(
    function: &mut Function,
    data: &mut crate::ir::DataPool,
    body: &[Stmt],
    env: TypeEnv,
    functions: &std::collections::HashMap<String, FunctionSig>,
    extern_functions: &std::collections::HashMap<String, crate::types::ExternFunctionSig>,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    return_php_type: PhpType,
    params: &[(String, PhpType)],
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
        constants,
        return_php_type,
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
