//! Purpose:
//! Home of the internal `__elephc_internal_warning` builtin: raises one php-level `E_WARNING`
//! from an elephc-PHP prelude through the runtime's diagnostic channel.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - Preludes implementing a php builtin whose failure path warns, such as
//!   `__elephc_highlight_file` in `crate::highlight_prelude`.
//!
//! Key details:
//! - The message is the complete diagnostic line, level word included (`"Warning: f(): ...\n"`),
//!   which `__rt_diag_warning` honours like every other runtime warning: `@` and
//!   `error_reporting()` suppression, the user error handler, and `error_get_last()`.
//! - `trigger_error()` cannot do this: php refuses every non-user level there.

builtin! {
    contract: "__elephc_internal_warning",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcInternalWarning,
    ),
}
