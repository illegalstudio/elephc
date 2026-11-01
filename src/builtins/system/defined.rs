//! Purpose:
//! Home of the PHP `defined` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Constant names may be computed at runtime and are probed against the emitted registry.
//! - The semantic effect includes `MAY_THROW` because invalid relative class
//!   receivers raise catchable PHP `Error`s during static lowering.

use crate::builtins::semantics::{runtime_fn_semantics, BuiltinEffects, BuiltinSemantics};

builtin! {
    contract: "defined",
    semantics: defined_semantics(),
}

/// Adds static-lowering access errors to the typed runtime target's global-read effect.
const fn defined_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::Defined);
    semantics.effects = BuiltinEffects::Static(crate::ir::Effects::from_bits_retain(
        crate::ir::RuntimeFnId::Defined.effects().bits() | crate::ir::Effects::MAY_THROW.bits(),
    ));
    semantics
}
