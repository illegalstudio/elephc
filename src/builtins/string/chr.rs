//! Purpose:
//! Home of the PHP `chr` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `chr` is a pure-data builtin whose return type
//!   (`Str`) is fully determined by its declaration. The registry derives the return
//!   type from the `returns:` field without calling a check hook.
//! - The parameter is named `codepoint` (matching the parity golden) and typed `Int`,
//!   reflecting PHP's `chr(int $codepoint): string`. The dedicated `lower_chr` emitter
//!   coerces the operand to an integer via `load_as_int`, so the declared `Int` type
//!   is consistent with the existing lowering.


builtin! {
    name: "chr",
    area: String,
    params: [codepoint: Int],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Chr,
    ),
    summary: "Returns a one-character string from the given byte code point.",
    php_manual: "https://www.php.net/manual/en/function.chr.php",
}
