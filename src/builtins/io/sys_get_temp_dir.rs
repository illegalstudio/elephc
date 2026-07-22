//! Purpose:
//! Home of the PHP `sys_get_temp_dir` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `sys_get_temp_dir` is a pure-data builtin whose `Str` return
//!   type is fully determined by its declaration. The registry common path enforces
//!   its 0-argument arity before falling back to `returns`.


builtin! {
    name: "sys_get_temp_dir",
    area: Io,
    params: [],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SysGetTempDir,
    ),
    summary: "Returns the directory path used for temporary files.",
    php_manual: "function.sys-get-temp-dir",
}
