//! Purpose:
//! Home of the PHP `clamp` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A shared result resolver is required because the return type depends on all three argument
//!   types: all-Str returns Str, all-Int returns Int, Int/Float mix returns Float,
//!   anything else returns Mixed.

use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::types::PhpType;

builtin! {
    name: "clamp",
    area: Math,
    params: [value: Mixed, min: Mixed, max: Mixed],
    returns: Mixed,
    semantics: clamp_semantics(),
    summary: "Clamps a value to be within a specified range.",
    php_manual: "https://www.php.net/manual/en/function.clamp.php",
}

/// Builds clamp semantics with one result resolver shared by checker and EIR lowering.
const fn clamp_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::Clamp);
    semantics.result_type = BuiltinResultType::Shared(result_type);
    semantics
}

/// Returns the most precise result type for `clamp($value, $min, $max)`.
///
/// All-string operands return `Str`; all-int return `Int`; int/float mix returns
/// `Float`; any other combination returns `Mixed`.
fn result_type(input: &BuiltinSemanticInput<'_>) -> PhpType {
    if input.arg_types.iter().all(|ty| *ty == PhpType::Str) {
        PhpType::Str
    } else if input.arg_types.iter().all(|ty| *ty == PhpType::Int) {
        PhpType::Int
    } else if input
        .arg_types
        .iter()
        .all(|ty| matches!(ty, PhpType::Int | PhpType::Float))
    {
        PhpType::Float
    } else {
        PhpType::Mixed
    }
}
