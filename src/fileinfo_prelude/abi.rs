//! Purpose:
//! Keeps the fileinfo prelude's PHP-visible signatures separate from its runtime storage.
//!
//! Called from:
//! - The checker's free-function and method signature builders and method body pass.
//!
//! Key details:
//! - Reflection reports the declaration: `finfo`, `?string`, `int` and untyped parameters.
//! - The runtime ABI widens only the parameters whose invalid values must reach PHP's own
//!   coercion or TypeError path instead of being rejected while compiling.

use crate::names::php_symbol_key;
use crate::types::PhpType;

/// Returns the runtime storage type for one fileinfo prelude function parameter.
///
/// The declaration keeps its annotations in `param_type_exprs`, so reflection and dynamic eval
/// continue to report the PHP signature. Only the ABI slot becomes boxed `mixed`.
pub(crate) fn widened_function_param(name: &str, index: usize) -> Option<PhpType> {
    let widened: &[usize] = match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "finfo_open" => &[0, 1],
        // `$context` is untyped in PHP but must carry either null or a stream context.
        "finfo_buffer" | "finfo_file" => &[0, 3],
        "finfo_close" | "finfo_set_flags" => &[0],
        // PHP leaves `$filename` untyped, but the slot carries resources and strings.
        "mime_content_type" => &[0],
        _ => return None,
    };
    widened.contains(&index).then_some(PhpType::Mixed)
}

/// Replaces a prelude function's runtime parameter types with the storage PHP semantics need.
pub(crate) fn widen_function_params(name: &str, params: &mut [(String, PhpType)]) {
    for (index, (_, ty)) in params.iter_mut().enumerate() {
        if let Some(widened) = widened_function_param(name, index) {
            *ty = widened;
        }
    }
}

/// Returns PHP's literal `true` return type for the functions that always succeed.
///
/// `FunctionSig` stores runtime types, where `true` is `Bool`; reflection and eval metadata
/// consult this helper so the declared literal survives.
pub(crate) fn reflection_return_type(owner: &str) -> Option<&'static str> {
    let owner = php_symbol_key(owner.trim_start_matches('\\'));
    matches!(
        owner.as_str(),
        "finfo_close" | "finfo_set_flags" | "finfo::set_flags"
    )
    .then_some("true")
}

/// Returns the runtime storage type for one fileinfo prelude method parameter.
///
/// `finfo::__construct` mirrors `finfo_open`'s coercion and TypeError behavior, while the
/// public query methods carry an untyped `$context` that still accepts a stream context.
pub(crate) fn widened_method_param(class: &str, method: &str, index: usize) -> Option<PhpType> {
    if !class.trim_start_matches('\\').eq_ignore_ascii_case("finfo") {
        return None;
    }
    let widened: &[usize] = match php_symbol_key(method).as_str() {
        "__construct" => &[0, 1],
        // `$context` is untyped and accepts a stream context or null.
        "buffer" | "file" => &[2],
        _ => return None,
    };
    widened.contains(&index).then_some(PhpType::Mixed)
}

/// Replaces a prelude method's runtime parameter types with their PHP-compatible storage.
pub(crate) fn widen_method_params(class: &str, method: &str, params: &mut [(String, PhpType)]) {
    for (index, (_, ty)) in params.iter_mut().enumerate() {
        if let Some(widened) = widened_method_param(class, method, index) {
            *ty = widened;
        }
    }
}
