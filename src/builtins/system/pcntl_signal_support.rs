//! Purpose:
//! Validates PCNTL signal-set arrays, write-only info outputs, and timeout values.
//!
//! Called from:
//! - The `pcntl_sigprocmask`, `pcntl_sigwaitinfo`, and `pcntl_sigtimedwait` builtin homes.
//!
//! Key details:
//! - Signal-set values accept PHP's weak integer-coercible scalar array forms; codegen
//!   normalizes indexed and associative storage into the AOT bridge ABI.
//! - By-reference outputs are write-only and may introduce previously undefined variables.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Checks the signal number, handler disposition or callable, and restart flag.
pub(super) fn check_signal(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for (index, arg) in cx.args.iter().enumerate() {
        let name = argument_name(arg);
        let value = argument_value(arg);
        if name.as_deref() == Some("handler") || (name.is_none() && index == 1) {
            let ty = cx.checker.infer_type(value, cx.env)?;
            if matches!(value.kind, ExprKind::IntLiteral(disposition) if !matches!(disposition, 0 | 1)) {
                return Err(CompileError::new(
                    value.span,
                    "pcntl_signal() integer handler must be SIG_DFL (0) or SIG_IGN (1)",
                ));
            }
            if !matches!(
                ty.codegen_repr(),
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::Callable
                    | PhpType::Str
                    | PhpType::Array(_)
                    | PhpType::AssocArray { .. }
                    | PhpType::Object(_)
                    | PhpType::Mixed
                    | PhpType::Union(_)
            ) {
                return Err(CompileError::new(
                    value.span,
                    &format!(
                        "pcntl_signal() parameter $handler must be callable or SIG_DFL/SIG_IGN, {ty:?} given"
                    ),
                ));
            }
        } else {
            cx.checker.infer_type(value, cx.env)?;
        }
    }
    Ok(PhpType::Bool)
}

/// Checks signal-mask mode, the selected signal set, and optional old-mask output.
pub(super) fn check_sigprocmask(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_signal_arguments(cx, SignalCall::Mask)?;
    Ok(PhpType::Bool)
}

/// Checks a nonempty signal set and optional write-only siginfo output.
pub(super) fn check_sigwaitinfo(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_signal_arguments(cx, SignalCall::Wait)?;
    Ok(PhpType::Mixed)
}

/// Checks the timed-wait signal set, optional output, and statically known timeout bounds.
pub(super) fn check_sigtimedwait(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_signal_arguments(cx, SignalCall::TimedWait)?;
    let seconds = parameter(cx.args, "seconds", 2)
        .map(integer_literal)
        .unwrap_or(Some(0));
    let nanoseconds = parameter(cx.args, "nanoseconds", 3)
        .map(integer_literal)
        .unwrap_or(Some(0));
    if matches!(seconds, Some(value) if value < 0) {
        return Err(CompileError::new(
            cx.span,
            "pcntl_sigtimedwait() parameter $seconds must be greater than or equal to 0",
        ));
    }
    if matches!(nanoseconds, Some(value) if !(0..1_000_000_000).contains(&value)) {
        return Err(CompileError::new(
            cx.span,
            "pcntl_sigtimedwait() parameter $nanoseconds must be between 0 and 1000000000",
        ));
    }
    if seconds == Some(0) && nanoseconds == Some(0) {
        return Err(CompileError::new(
            cx.span,
            "pcntl_sigtimedwait() requires a positive seconds or nanoseconds timeout",
        ));
    }
    Ok(PhpType::Mixed)
}

/// Signal-set call shape whose parameter positions are being validated.
#[derive(Clone, Copy)]
enum SignalCall {
    Mask,
    Wait,
    TimedWait,
}

/// Validates the indexed integer set and any write-only output for one signal operation.
fn check_signal_arguments(
    cx: &mut BuiltinCheckCtx,
    call: SignalCall,
) -> Result<(), CompileError> {
    let (signals_index, output_name, output_index) = match call {
        SignalCall::Mask => (1, "old_signals", 2),
        SignalCall::Wait | SignalCall::TimedWait => (0, "info", 1),
    };
    for (index, arg) in cx.args.iter().enumerate() {
        let name = argument_name(arg);
        let value = argument_value(arg);
        if name.as_deref() == Some(output_name) || (name.is_none() && index == output_index) {
            if cx.argument_was_omitted(index) {
                continue;
            }
            if !matches!(value.kind, ExprKind::Variable(_)) {
                return Err(CompileError::new(
                    value.span,
                    &format!(
                        "{}() parameter ${output_name} must be passed a variable",
                        cx.name
                    ),
                ));
            }
            continue;
        }
        if name.as_deref() == Some("signals") || (name.is_none() && index == signals_index) {
            let ty = cx.checker.infer_type(value, cx.env)?;
            match ty.codegen_repr() {
                PhpType::Array(element) | PhpType::AssocArray { value: element, .. }
                    if signal_element_type_supported(&element) => {}
                other => {
                    return Err(CompileError::new(
                        value.span,
                        &format!(
                            "{}() parameter $signals must be an array of integer-coercible scalar values, {other:?} given",
                            cx.name
                        ),
                    ));
                }
            }
            if !matches!(call, SignalCall::Mask)
                && matches!(&value.kind, ExprKind::ArrayLiteral(items) if items.is_empty())
            {
                return Err(CompileError::new(
                    value.span,
                    &format!("{}() parameter $signals must not be empty", cx.name),
                ));
            }
            continue;
        }
        cx.checker.infer_type(value, cx.env)?;
    }
    Ok(())
}

/// Returns whether an array element type can follow PHP's weak integer parameter coercion.
fn signal_element_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Str
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Void
            | PhpType::Never
            | PhpType::TaggedScalar
            | PhpType::Object(_)
            | PhpType::Callable
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Finds a positional or named source argument for one canonical parameter.
fn parameter<'a>(args: &'a [Expr], name: &str, index: usize) -> Option<&'a Expr> {
    args.iter().enumerate().find_map(|(position, arg)| {
        let argument_name = argument_name(arg);
        if argument_name.as_deref() == Some(name) || (argument_name.is_none() && position == index)
        {
            Some(argument_value(arg))
        } else {
            None
        }
    })
}

/// Returns an integer literal value when the expression is statically exact.
fn integer_literal(expr: &Expr) -> Option<i64> {
    match expr.kind {
        ExprKind::IntLiteral(value) => Some(value),
        _ => None,
    }
}

/// Returns the normalized name attached to a named argument.
fn argument_name(arg: &Expr) -> Option<String> {
    match &arg.kind {
        ExprKind::NamedArg { name, .. } => Some(php_symbol_key(name)),
        _ => None,
    }
}

/// Unwraps a named argument to the PHP value supplied to its parameter.
fn argument_value(arg: &Expr) -> &Expr {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => value,
        _ => arg,
    }
}
