//! Purpose:
//! Home of the PHP `iconv_strlen` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - An omitted or `null` `$encoding` counts in `iconv.internal_encoding`, while an
//!   explicitly empty one counts in PHP's `default_charset`.
//! - This file also owns the argument validation the whole iconv family shares.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "iconv_strlen",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IconvStrlen,
    ),
}

/// Validates `iconv_strlen()`'s arguments and returns `PhpType::Union([Int, False])`.
///
/// The hook infers every argument itself so a container passed where PHP declares a
/// string is rejected here instead of reaching the backend.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_string_argument(cx, 0, "iconv_strlen", "string")?;
    check_nullable_string_argument(cx, 1, "iconv_strlen", "encoding")?;
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::False]))
}

/// Validates one iconv argument that PHP declares as `string`.
///
/// PHP coerces scalars to string but raises a `TypeError` for arrays, objects, and other
/// container shapes, so those are rejected here instead of reaching the backend.
pub(super) fn check_string_argument(
    cx: &mut BuiltinCheckCtx,
    index: usize,
    function: &str,
    parameter: &str,
) -> Result<(), CompileError> {
    let Some(argument) = cx.args.get(index) else {
        return Ok(());
    };
    let span = argument.span;
    let inferred = cx.checker.infer_type(argument, cx.env)?;
    if string_coercible(&inferred) {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        &format!("{function}() {parameter} argument must be string"),
    ))
}

/// Validates one iconv argument that PHP declares as a nullable `string`.
pub(super) fn check_nullable_string_argument(
    cx: &mut BuiltinCheckCtx,
    index: usize,
    function: &str,
    parameter: &str,
) -> Result<(), CompileError> {
    let Some(argument) = cx.args.get(index) else {
        return Ok(());
    };
    let span = argument.span;
    let inferred = cx.checker.infer_type(argument, cx.env)?;
    if matches!(inferred, PhpType::Void | PhpType::Never) || string_coercible(&inferred) {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        &format!("{function}() {parameter} argument must be string or null"),
    ))
}

/// Reports whether PHP would accept one inferred type where a string is declared.
fn string_coercible(inferred: &PhpType) -> bool {
    matches!(
        inferred.codegen_repr(),
        PhpType::Str
            | PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Mixed
            | PhpType::TaggedScalar
            | PhpType::Union(_)
    )
}
