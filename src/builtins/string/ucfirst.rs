//! Purpose:
//! Home of the PHP `ucfirst` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `ucfirst` is a pure-data builtin whose return
//!   type (`Str`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.


builtin! {
    contract: "ucfirst",
    semantics: semantics(),
}

/// A string source can select `ucfirst` at run time (`$function($value)`), which Laravel's
/// `Str::plural()` does to match the case of the word it inflects.
const fn semantics() -> crate::builtins::semantics::BuiltinSemantics {
    let mut semantics =
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::Ucfirst);
    semantics.callable = crate::builtins::semantics::BuiltinCallablePolicy::Dynamic(
        crate::builtins::semantics::callable_accepts_string_source,
    );
    semantics
}
