//! Purpose:
//! Home of the PHP `version_compare` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The result type depends on the call shape, as in PHP: `int` (-1/0/1) without an operator
//!   or with a `null` one, `bool` with a string operator, and the boxed `int|bool` when the
//!   operator may be null at run time (a boxed or nullable scalar operator).
//! - An unknown operator raises php-src's catchable `ValueError`, so the runtime function is
//!   `MAY_THROW`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "version_compare",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::VersionCompare,
    ),
}

/// Returns `int` for the comparison form, `bool` for the operator form, and `int|bool` when
/// the operator may be null at run time.
///
/// Both versions are checked like PHP's `string` parameters and the operator like `?string`
/// before the result type is chosen: argument lowering would otherwise turn an array into the
/// string `"Array"` and compare it.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_string_parameter(cx, 0, "version1", false)?;
    check_string_parameter(cx, 1, "version2", false)?;
    check_string_parameter(cx, 2, "operator", true)?;
    let Some(operator) = cx.args.get(2) else {
        return Ok(PhpType::Int);
    };
    match cx.checker.infer_type(operator, cx.env)?.codegen_repr() {
        PhpType::Void | PhpType::Never => Ok(PhpType::Int),
        PhpType::Mixed | PhpType::Union(_) | PhpType::TaggedScalar => {
            Ok(cx.checker.normalize_union_type(vec![PhpType::Int, PhpType::Bool]))
        }
        _ => Ok(PhpType::Bool),
    }
}

/// Rejects an argument PHP's coercive `string` (or, with `nullable`, `?string`) parameter
/// refuses: an array, an object, or another container shape, with PHP's `TypeError` wording.
///
/// `null` is accepted for every parameter: PHP's coercive mode still converts it to `""` for a
/// non-nullable scalar parameter (only deprecated since PHP 8.1), so `nullable` only picks the
/// declared type named in the message.
fn check_string_parameter(
    cx: &mut BuiltinCheckCtx,
    index: usize,
    parameter: &str,
    nullable: bool,
) -> Result<(), CompileError> {
    let Some(argument) = cx.args.get(index) else {
        return Ok(());
    };
    let span = argument.span;
    let inferred = cx.checker.infer_type(argument, cx.env)?;
    // A bare `array` declaration is represented as a boxed union, so ask for PHP arrays first.
    let accepted = !inferred.is_php_array() && match inferred.codegen_repr() {
        PhpType::Str
        | PhpType::Int
        | PhpType::Float
        | PhpType::Bool
        | PhpType::False
        | PhpType::Mixed
        | PhpType::TaggedScalar
        | PhpType::Union(_)
        | PhpType::Void
        | PhpType::Never => true,
        _ => false,
    };
    if accepted {
        return Ok(());
    }
    let expected = if nullable { "?string" } else { "string" };
    Err(CompileError::new(
        span,
        &format!(
            "version_compare(): Argument #{} (${parameter}) must be of type {expected}, {} given",
            index + 1,
            crate::builtins::math::random_bytes::given_type_name(&inferred),
        ),
    ))
}
