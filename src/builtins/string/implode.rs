//! Purpose:
//! Home of the PHP `implode` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Reference PHP declares `implode(string|array $separator, ?array $array = null)`, which is
//!   what makes BOTH accepted call forms work: `implode($separator, $array)` and the
//!   single-argument `implode($array)` that joins with an empty separator. `separator` is
//!   therefore declared `Mixed`, not `Str`, and the backend picks the operand roles from the
//!   argument count (`lower_implode` in `crate::codegen::lower_inst::builtins::strings`).
//! - The legacy reversed `implode($array, $separator)` order was REMOVED in PHP 8.0 and is
//!   deliberately not accepted.
//! - This declaration mirrors the `join` alias exactly; the registry's alias arity gate requires
//!   the two to agree on their enforced bounds (one required parameter, at most two).
//! - No `check` hook narrows the arity: `returns: Str` is authoritative for the checker.


builtin! {
    contract: "implode",
    semantics: implode_semantics(),
}

/// Reports whether this call can raise `implode()`'s `TypeError`.
///
/// PHP throws when the array argument is not an array, and in elephc that can only happen when
/// the operand arrived BOXED -- a `mixed` or union slot such as `?array`, whose payload the
/// backend checks at run time. Declaring `MAY_THROW` for exactly those calls is what keeps them
/// inside an enclosing `try`: without it the guard's diagnostic was raised and never caught,
/// and an unused call could be eliminated along with its diagnostic (issue #689).
///
/// A statically typed array operand cannot fail that check, so it keeps the pure summary and
/// stays eliminable. The array is the LAST argument in both accepted call forms.
pub(crate) fn implode_effects(
    input: &crate::builtins::semantics::BuiltinSemanticInput<'_>,
) -> crate::ir::Effects {
    let base = crate::ir::RuntimeFnId::Implode.effects();
    let Some(array_ty) = input.arg_types.last() else {
        return base;
    };
    match array_ty.codegen_repr() {
        crate::types::PhpType::Mixed | crate::types::PhpType::Union(_) => {
            base | crate::ir::Effects::MAY_THROW
        }
        _ => base,
    }
}

/// Builds the shared `implode`/`join` descriptor with the call-dependent throw summary.
pub(crate) const fn implode_semantics() -> crate::builtins::semantics::BuiltinSemantics {
    let mut semantics =
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::Implode);
    semantics.effects = crate::builtins::semantics::BuiltinEffects::Shared(implode_effects);
    semantics
}
