//! Purpose:
//! Home of the PHP `get_declared_interfaces` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Check hook returns `Array<Str>` unconditionally (zero-arg builtin).


builtin! {
    name: "get_declared_interfaces",
    area: Callables,
    params: [],
    returns: Mixed,
    check: crate::builtins::callables::support::check_declared_names,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::GetDeclaredInterfaces,
    ),
    summary: "Returns an array of all declared interfaces.",
    php_manual: "function.get-declared-interfaces",
}
