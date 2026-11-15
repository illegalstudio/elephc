//! Purpose:
//! Shared semantic descriptor for the eleven PHP `ctype_*` character-class builtins.
//!
//! Called from:
//! - The `src/builtins/string/ctype_*.rs` home files.
//!
//! Key details:
//! - PHP declares `mixed $text`. A string is classified byte by byte and stays pure.
//! - Every other argument raises PHP 8.1's "will be interpreted as string in the future"
//!   deprecation, so its call can warn and, through a throwing error handler, throw.
//! - The backend accepts every source representation, so runtime-selected callables wrap
//!   the builtin over a boxed `mixed` parameter.

use crate::builtins::semantics::{
    callable_accepts_any_source, runtime_fn_semantics, BuiltinCallablePolicy, BuiltinEffects,
    BuiltinSemanticInput, BuiltinSemantics,
};
use crate::ir::{Effects, RuntimeFnId};
use crate::types::PhpType;

/// Builds the descriptor of one `ctype_*` builtin around its typed runtime target.
pub const fn ctype_semantics(target: RuntimeFnId) -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(target);
    semantics.effects = BuiltinEffects::Shared(ctype_effects);
    semantics.callable = BuiltinCallablePolicy::Dynamic(callable_accepts_any_source);
    semantics
}

/// Resolves the effects of one call from the static type of its argument.
///
/// Only a string avoids the deprecation. A boxed `mixed` value also reads the heap cell
/// that holds it.
fn ctype_effects(input: &BuiltinSemanticInput<'_>) -> Effects {
    match input.arg_types.first().map(PhpType::codegen_repr) {
        Some(PhpType::Str) => Effects::PURE,
        Some(PhpType::Mixed | PhpType::Union(_)) => {
            Effects::READS_HEAP | Effects::MAY_WARN | Effects::MAY_THROW
        }
        _ => Effects::MAY_WARN | Effects::MAY_THROW,
    }
}
