//! Purpose:
//! Home of the PHP `random_bytes` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The bytes come from the operating system's CSPRNG (`arc4random_buf` on macOS,
//!   `getrandom` on Linux); there is no weak fallback.
//! - A `$length` below 1 raises php-src's catchable `ValueError`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "random_bytes",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::RandomBytes,
    ),
}

/// Rejects a `$length` PHP's coercive `int` parameter refuses (an array, an object, or another
/// container shape) and types the call as the `string` it returns.
///
/// `null` is accepted: PHP's coercive mode converts it to `0` (only deprecated since PHP 8.1),
/// which then raises the catchable `ValueError` for a length below 1.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(length) = cx.args.first() {
        let span = length.span;
        let inferred = cx.checker.infer_type(length, cx.env)?;
        // A bare `array` declaration is represented as a boxed union, so ask for PHP arrays first.
        let accepted = !inferred.is_php_array() && matches!(
            inferred.codegen_repr(),
            PhpType::Int
                | PhpType::Float
                | PhpType::Bool
                | PhpType::False
                | PhpType::Str
                | PhpType::Mixed
                | PhpType::TaggedScalar
                | PhpType::Union(_)
                | PhpType::Void
                | PhpType::Never
        );
        if !accepted {
            return Err(CompileError::new(
                span,
                &format!(
                    "random_bytes(): Argument #1 ($length) must be of type int, {} given",
                    given_type_name(&inferred)
                ),
            ));
        }
    }
    Ok(PhpType::Str)
}

/// Names a statically known argument type the way PHP's argument `TypeError` does.
pub(crate) fn given_type_name(ty: &PhpType) -> String {
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => "array".to_string(),
        PhpType::Object(name) => name.trim_start_matches('\\').to_string(),
        PhpType::Callable => "Closure".to_string(),
        PhpType::Void | PhpType::Never => "null".to_string(),
        other => other.to_string(),
    }
}
