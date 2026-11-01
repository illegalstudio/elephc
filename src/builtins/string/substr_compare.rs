//! Purpose:
//! Home of the PHP `substr_compare` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Matches php-src's signature `substr_compare(string $haystack, string $needle,
//!   int $offset, ?int $length = null, bool $case_insensitive = false): int`. `$length` is
//!   declared `Mixed` rather than `?int` for the same reason `substr_count` does it: the
//!   nullable default has to survive as a RUN-TIME null the backend can still recognize,
//!   because a null `$length` means "compare to the end of the longer operand" and not `0`.
//! - The typed runtime target carries `MAY_THROW`: an `$offset` past the end of `$haystack`
//!   and a negative `$length` each raise a catchable `ValueError`, so the call must not be
//!   removable by dead-code elimination nor hoisted out of a `try`.


builtin! {
    contract: "substr_compare",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SubstrCompare,
    ),
}
