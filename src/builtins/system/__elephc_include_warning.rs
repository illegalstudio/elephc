//! Purpose:
//! Home of the internal `__elephc_include_warning` builtin: raises PHP's `E_WARNING` with the
//! given message, for the missing-file path of a dynamic include.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The dispatch `crate::resolver::dynamic_includes` lowers a manifest-closed include site to.
//!
//! Key details:
//! - PHP reports a failed `include` as `E_WARNING` (errno 2), which `trigger_error()` cannot
//!   raise (it only accepts the `E_USER_*` levels). The message goes through
//!   `__rt_diag_warning` with a `Warning: ` prefix, so `set_error_handler`, `error_reporting`
//!   and `@` see exactly what reference PHP produces.

builtin! {
    contract: "__elephc_include_warning",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IncludeWarning,
    ),
}
