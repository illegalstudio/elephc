//! Purpose:
//! Home of the PHP `join` builtin: the registry-visible alias of `implode`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `join` retains its own typed target so PHP diagnostics name the called alias.
//!   Its argument roles and renderer are shared with `implode`.
//! - Reference PHP declares `join(string|array $separator, ?array $array = null)`, which
//!   is what makes BOTH accepted call forms work: `join($separator, $array)` and the
//!   single-argument `join($array)` that joins with an empty separator. `separator` is
//!   therefore declared `Mixed` here, not `Str`, and the backend picks the operand roles
//!   from the argument count. (The legacy reversed `join($array, $separator)` order was
//!   REMOVED in PHP 8.0 and is deliberately not accepted.)
//! - Unlike `implode`, no `check` hook narrows the arity: the one-argument form is the
//!   whole reason this alias carries its own declaration.


builtin! {
    contract: "join",
    semantics: super::implode::join_semantics(crate::ir::RuntimeFnId::Join),
}
