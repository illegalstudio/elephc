//! Purpose:
//! Binds prelude INI operations to the shared request engine and protected mbstring coordinator.
//!
//! Called from:
//! - The AOT builtin registry and injected INI wrappers.
//!
//! Key details:
//! - The internal neutral contract owns all arguments; the engine owns identity and mutation semantics.

builtin! {
    contract: "__elephc_shared_ini",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::SharedIni),
}
