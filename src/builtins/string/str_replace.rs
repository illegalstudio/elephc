//! Purpose:
//! Home of the PHP `str_replace` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The by-reference `$count` (fourth argument) is routed to
//!   `crate::str_replace_count_prelude`, which counts as it replaces.
//! - The declared `string` return is only right for a STRING `$subject`. php answers an array
//!   for an array `$subject`, and `subject_shaped_result_type` is what tells the checker and
//!   EIR that — see its own comment for the miscompile it closes. `str_ireplace` shares it.

use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinEffects, BuiltinResultType, BuiltinSemanticInput,
    BuiltinSemantics,
};
use crate::types::PhpType;

builtin! {
    contract: "str_replace",
    semantics: str_replace_semantics(),
}

/// Builds runtime semantics whose result follows php's subject-shaped return type.
const fn str_replace_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::StrReplace);
    semantics.result_type = BuiltinResultType::Shared(subject_shaped_result_type);
    semantics.effects = BuiltinEffects::Shared(str_replace_effects);
    semantics
}

/// The three-argument form is pure; the by-reference `$count` form writes a caller variable.
///
/// A static, empty effect set would call `str_replace($s, $r, $subject, $count)` pure, and the
/// optimizer substitutes known values into pure calls and may drop their unused results, which
/// would lose the write to `$count`. Only the count form carries the write.
fn str_replace_effects(input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    let base = crate::ir::RuntimeFnId::StrReplace.effects();
    if input.args.len() >= 4 {
        base | crate::ir::Effects::WRITES_HEAP | crate::ir::Effects::WRITES_LOCAL
    } else {
        base
    }
}

/// Returns `array<string>` for an array `$subject` and `string` for every other subject.
///
/// php: `str_replace(array|string $search, array|string $replace, string|array $subject,
/// int &$count = null): string|array` — "if `$subject` is an array, then the search and replace
/// is performed with every entry of `$subject`, and the return value is an array as well."
/// Measured on 8.5.10: `str_replace(["a"], ["1"], ["ab", "ba"])` is
/// `array(0 => '1b', 1 => 'b1')` and `gettype()` of it is `array`; the subject's KEYS are
/// preserved (`["k" => "aa"]` answers `["k" => "11"]`).
///
/// This existed only in the backend. `lower_string_replace` has had an array-subject path
/// calling `__rt_str_replace_array_subject_arrays` for a while, but the contract declared
/// `returns: Str` and the semantics took it verbatim, so the call site stored an ARRAY pointer
/// into a string slot. Measured before this resolver, on the identical source php answers the
/// two-element array above with:
///
/// ```text
/// $r = str_replace(["a"], ["1"], ["ab", "ba"]); var_dump($r);   // string(2) "b1"
/// ```
///
/// — a silent wrong answer, not a diagnostic. The same wrong type is what refused
/// `[$a, $b] = str_replace($search, $replace, $subjects)` with "List unpacking requires an array
/// on the right-hand side": the checker was told the right-hand side was a string.
///
/// The array arm deliberately matches the backend's own predicate in `lower_string_replace`
/// exactly — `Array(Str | Mixed)`, nothing else. An `AssocArray` subject is NOT claimed here:
/// the backend has no array-subject path for one and falls through to its string handling, so
/// promising an array result would re-create the mismatch this resolver removes. Every other
/// subject, gradual ones included, keeps the declared `string` it has always had.
pub(crate) fn subject_shaped_result_type(input: &BuiltinSemanticInput<'_>) -> PhpType {
    match input.arg_types.get(2).map(PhpType::codegen_repr) {
        Some(PhpType::Array(element))
            if matches!(element.codegen_repr(), PhpType::Str | PhpType::Mixed) =>
        {
            PhpType::Array(Box::new(PhpType::Str))
        }
        _ => PhpType::Str,
    }
}
