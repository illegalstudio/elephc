//! Purpose:
//! Registers compiler-generated native default trees as owned EvalIR expression data.
//!
//! Called from:
//! - Native method and constructor metadata emitted by the compiler's eval backend.
//!
//! Key details:
//! - Registration parses one return expression but performs no PHP evaluation.
//! - Deferred exceptions are interpreted, never raised through a raw native default callback.
//! - Borrowed ABI bytes are copied and validated before signature metadata is updated.

use super::*;

/// Registers a nested free-function default with an explicitly empty lexical class scope.
///
/// # Safety
/// The context and borrowed UTF-8 buffers must remain valid for their declared lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function_param_default_expression(
    ctx: *mut ElephcEvalContext, key: *const u8, key_len: u64, index: u64,
    source: *const u8, source_len: u64,
) -> i32 {
    register_expression(ctx, key, key_len, index, source, source_len, 3)
}

/// Registers a nested instance-method default expression.
///
/// # Safety
/// The context and borrowed UTF-8 buffers must remain valid for their declared lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_method_param_default_expression(
    ctx: *mut ElephcEvalContext, key: *const u8, key_len: u64, index: u64,
    source: *const u8, source_len: u64,
) -> i32 {
    register_expression(ctx, key, key_len, index, source, source_len, 0)
}

/// Registers a nested static-method default expression.
///
/// # Safety
/// The context and borrowed UTF-8 buffers must remain valid for their declared lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_static_method_param_default_expression(
    ctx: *mut ElephcEvalContext, key: *const u8, key_len: u64, index: u64,
    source: *const u8, source_len: u64,
) -> i32 {
    register_expression(ctx, key, key_len, index, source, source_len, 1)
}

/// Registers a nested constructor default expression.
///
/// # Safety
/// The context and borrowed UTF-8 buffers must remain valid for their declared lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_constructor_param_default_expression(
    ctx: *mut ElephcEvalContext, key: *const u8, key_len: u64, index: u64,
    source: *const u8, source_len: u64,
) -> i32 {
    register_expression(ctx, key, key_len, index, source, source_len, 2)
}

/// Parses exactly one expression and installs it through existing signature validation.
///
/// # Safety
/// Mirrors the exported registration functions' borrowed buffer and context requirements.
unsafe fn register_expression(
    ctx: *mut ElephcEvalContext, key: *const u8, key_len: u64, index: u64,
    source: *const u8, source_len: u64, kind: u8,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        let Ok(source) = abi_name_to_string(source, source_len) else { return 0; };
        let Ok(name) = abi_name_to_string(key, key_len) else { return 0; };
        let Ok(program) = crate::parser::parse_fragment(source.as_bytes()) else { return 0; };
        let mut statements = program.into_statements();
        if statements.len() != 1 { return 0; }
        let Some(crate::eval_ir::EvalStmt::Return(Some(mut expression))) = statements.pop()
            else { return 0; };
        let mut lexical_class = if kind == 3 { String::new() } else if kind == 2 { name }
            else { let Some((class, _)) = name.rsplit_once("::") else { return 0; }; class.to_string() };
        if let crate::eval_ir::EvalExpr::Call { name, args } = &expression {
            if name == "__elephc_default_lexical_scope" {
                let [scope, value] = args.as_slice() else { return 0; };
                if scope.name().is_some() || scope.is_spread()
                    || value.name().is_some() || value.is_spread() { return 0; }
                let crate::eval_ir::EvalExpr::Const(crate::eval_ir::EvalConst::String(scope)) = scope.value()
                    else { return 0; };
                lexical_class = scope.clone();
                expression = value.value().clone();
            }
        }
        if !normalize_expression(&mut expression) { return 0; }
        let default = NativeCallableDefault::Expression {
            expression: Box::new(expression), lexical_class, reflection_factory: None,
        };
        if kind == 3 {
            crate::ffi::native_functions::register_native_function_param_default_inner(
                ctx, key, key_len, index, default)
        } else if kind == 2 {
            register_native_constructor_param_default_inner(ctx, key, key_len, index, default)
        } else {
            register_native_method_param_default_inner(ctx, key, key_len, kind == 1, index, default)
        }
    }).unwrap_or(0)
}

/// Registers a persistent native factory used only after entering a protected Reflection getter.
///
/// # Safety
/// Context and key bytes must be valid; factory must name a persistent capture-free descriptor.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_param_default_factory(
    ctx: *mut ElephcEvalContext, key: *const u8, key_len: u64,
    index: u64, kind: u64, factory: usize,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        if factory == 0 { return 0; }
        let Some(context) = ctx.as_mut() else { return 0; };
        if context.abi_version() != ABI_VERSION { return 0; }
        let Ok(name) = abi_name_to_string(key, key_len) else { return 0; };
        let Ok(position) = usize::try_from(index) else { return 0; };
        let default = if kind == 3 {
            context.native_function(&name.to_ascii_lowercase())
                .and_then(|function| function.param_default(position).cloned())
        } else {
            let signature = if kind == 2 { context.native_constructor_signature(&name) } else {
                let Some((class, method)) = name.rsplit_once("::") else { return 0; };
                match kind {
                    0 => context.native_method_signature(class, method),
                    1 => context.native_static_method_signature(class, method),
                    _ => return 0,
                }
            };
            let Some(signature) = signature else { return 0; };
            signature.param_default(position).cloned()
        };
        let Some(mut default) = default else { return 0; };
        let NativeCallableDefault::Expression { reflection_factory, .. } = &mut default
            else { return 0; };
        *reflection_factory = Some(factory);
        if kind == 3 {
            crate::ffi::native_functions::register_native_function_param_default_inner(
                ctx, key, key_len, index, default)
        } else if kind == 2 {
            register_native_constructor_param_default_inner(ctx, key, key_len, index, default)
        } else {
            register_native_method_param_default_inner(ctx, key, key_len, kind == 1, index, default)
        }
    }).unwrap_or(0)
}

/// Validates the constant-expression subset and decodes compiler-only lazy Error markers.
fn normalize_expression(expression: &mut crate::eval_ir::EvalExpr) -> bool {
    use crate::eval_ir::{EvalArrayElement, EvalConst, EvalExpr};
    match expression {
        EvalExpr::Call { name, args } if name == "__elephc_deferred_default_error" => {
            let [arg] = args.as_slice() else { return false; };
            if arg.name().is_some() || arg.is_spread() { return false; }
            let EvalExpr::Const(EvalConst::String(message)) = arg.value() else { return false; };
            *expression = EvalExpr::DeferredDefaultError(message.clone());
            true
        }
        EvalExpr::Const(_) | EvalExpr::ConstFetch(_) | EvalExpr::ClassConstantFetch { .. }
        | EvalExpr::ClassNameFetch { .. } | EvalExpr::NamespacedConstFetch { .. } => true,
        EvalExpr::Array(entries) => entries.iter_mut().all(|entry| match entry {
            EvalArrayElement::Value(value) => normalize_array_value(value),
            EvalArrayElement::KeyValue { key, value } =>
                normalize_expression(key) && normalize_expression(value),
            EvalArrayElement::Reference(_) | EvalArrayElement::KeyReference { .. } => false,
        }),
        EvalExpr::ArrayGet { array, index } => normalize_expression(array) && normalize_expression(index),
        EvalExpr::Unary { expr, .. } | EvalExpr::Cast { expr, .. } => normalize_expression(expr),
        EvalExpr::Binary { left, right, .. } => normalize_expression(left) && normalize_expression(right),
        EvalExpr::NullCoalesce { value, default } =>
            normalize_expression(value) && normalize_expression(default),
        EvalExpr::Ternary { condition, then_branch, else_branch } => normalize_expression(condition)
            && then_branch.as_deref_mut().is_none_or(normalize_expression)
            && normalize_expression(else_branch),
        EvalExpr::Match { subject, arms, default } => normalize_expression(subject)
            && arms.iter_mut().all(|arm| arm.patterns.iter_mut().all(normalize_expression)
                && normalize_expression(&mut arm.value))
            && default.as_deref_mut().is_none_or(normalize_expression),
        EvalExpr::NewObject { args, .. } => args.iter_mut().all(|arg| {
            let mut value = arg.value().clone();
            if !normalize_expression(&mut value) { return false; }
            *arg = if arg.is_spread() {
                crate::eval_ir::EvalCallArg::spread(value)
            } else if let Some(name) = arg.name() {
                crate::eval_ir::EvalCallArg::named(name.to_string(), value)
            } else { crate::eval_ir::EvalCallArg::positional(value) };
            true
        }),
        _ => false,
    }
}

/// Decodes a compiler-only spread marker without adding public eval grammar or a callable hook.
fn normalize_array_value(value: &mut crate::eval_ir::EvalExpr) -> bool {
    use crate::eval_ir::{EvalCallArg, EvalExpr};
    if let EvalExpr::Call { name, args } = value {
        if name == "__elephc_default_array_spread" {
            let [arg] = args.as_slice() else { return false; };
            if arg.name().is_some() || arg.is_spread() { return false; }
            let mut source = arg.value().clone();
            if !normalize_expression(&mut source) { return false; }
            // A PHP identifier cannot spell this sentinel. Only the array builder consumes it.
            *name = "\0elephc_native_default_array_spread".into();
            *args = vec![EvalCallArg::positional(source)];
            return true;
        }
    }
    normalize_expression(value)
}
