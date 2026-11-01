//! Purpose:
//! Home of the PHP `class_exists` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook accepts literal and runtime string names; literals may seed autoload
//!   discovery while runtime names query the emitted closed-world class metadata.
//! - Arguments are pre-inferred by the registry common path before the hook runs.


builtin! {
    contract: "class_exists",
    check: crate::builtins::callables::support::check_class_like_exists,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ClassExists,
    ),
}
