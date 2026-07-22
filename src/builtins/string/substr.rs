//! Purpose:
//! Home of the PHP `substr` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Arity is validated by the registry's `check_arity` before the check hook fires;
//!   the inline arity check from the legacy arm is therefore not reproduced here.


builtin! {
    name: "substr",
    area: String,
    params: [string: Str, offset: Int, length: Int = crate::builtins::spec::DefaultSpec::Null],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Substr,
    ),
    summary: "Returns a portion of a string specified by the offset and length.",
    php_manual: "https://www.php.net/manual/en/function.substr.php",
}
