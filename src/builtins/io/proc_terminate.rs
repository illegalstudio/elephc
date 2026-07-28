//! Purpose:
//! Home of the PHP `proc_terminate` builtin and its signal argument validation.
//!
//! Called from:
//! - The builtin registry, checker, and typed EIR runtime-call lowering.
//!
//! Key details:
//! - Weak PHP scalar coercion accepts numeric strings, floats, booleans, and
//!   null for the integer signal while rejecting arrays and objects.
//! - Windows intentionally ignores the optional signal and follows php-src by
//!   terminating the retained process HANDLE with exit code 255.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "proc_terminate",
    area: Io,
    params: [process: Mixed, signal: Int = DefaultSpec::Int(15)],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ProcTerminate,
    ),
    summary: "Terminates a process opened by proc_open.",
    php_manual: "function.proc-terminate",
}

/// Validates PHP's weak scalar-to-int signal surface and returns `Bool`.
///
/// The registry pre-infers regular arguments. The hook re-infers the optional
/// signal to obtain its resolved type. Numeric string literals are checked
/// eagerly, while other weakly coercible scalar types are converted by codegen.
/// The process remains `Mixed` because resources have no finer type in the
/// current checker.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(signal) = cx.args.get(1) {
        let signal_ty = cx.checker.infer_type(signal, cx.env)?;
        let invalid_string_literal = matches!(
            &signal.kind,
            ExprKind::StringLiteral(value) if !is_php_int_parameter_string(value)
        );
        if invalid_string_literal || !accepts_weak_int_signal(&signal_ty) {
            return Err(CompileError::new(
                signal.span,
                "proc_terminate() signal must be int",
            ));
        }
    }
    Ok(PhpType::Bool)
}

/// Returns whether a statically inferred type can use PHP's weak `int` parameter coercion.
fn accepts_weak_int_signal(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int
        | PhpType::Float
        | PhpType::Bool
        | PhpType::False
        | PhpType::Str
        | PhpType::TaggedScalar
        | PhpType::Void
        | PhpType::Never => true,
        PhpType::Union(members) => {
            ty.codegen_repr() == PhpType::TaggedScalar
                && members.iter().all(accepts_weak_int_signal)
        }
        _ => false,
    }
}

/// Recognizes PHP numeric strings accepted by a weakly typed `int` parameter.
fn is_php_int_parameter_string(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() && is_php_numeric_whitespace(bytes[cursor]) {
        cursor += 1;
    }
    if cursor < bytes.len() && matches!(bytes[cursor], b'+' | b'-') {
        cursor += 1;
    }

    let mut digits = 0;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
        digits += 1;
    }
    if cursor < bytes.len() && bytes[cursor] == b'.' {
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return false;
    }

    if cursor < bytes.len() && matches!(bytes[cursor], b'e' | b'E') {
        cursor += 1;
        if cursor < bytes.len() && matches!(bytes[cursor], b'+' | b'-') {
            cursor += 1;
        }
        let exponent_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == exponent_start {
            return false;
        }
    }

    while cursor < bytes.len() && is_php_numeric_whitespace(bytes[cursor]) {
        cursor += 1;
    }
    cursor == bytes.len()
}

/// Returns whether a byte is whitespace accepted around a PHP numeric string.
fn is_php_numeric_whitespace(byte: u8) -> bool {
    byte == b' ' || (b'\t'..=b'\r').contains(&byte)
}
