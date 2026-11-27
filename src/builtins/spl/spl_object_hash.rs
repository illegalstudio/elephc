//! Purpose:
//! Home of the PHP `spl_object_hash` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook accepts any value that may hold an object (a boxed `mixed`, a union, a
//!   callable); the backend raises PHP's argument `TypeError` for anything else; returns `Str`.
//! - The hash is derived from the object's heap pointer stringified via `__rt_itoa`.

use crate::builtins::semantics::BuiltinSemanticInput;
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "spl_object_hash",
    check: check,
    semantics: super::object_argument::object_argument_semantics(
        crate::ir::RuntimeFnId::SplObjectHash,
        effects,
    ),
}

/// Accepts any argument that may hold an object and returns `Str`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::object_argument::check_object_argument(cx, "spl_object_hash", PhpType::Str)
}

/// Keeps the runtime target's effects, adding `MAY_THROW` for a runtime-checked argument.
fn effects(input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    super::object_argument::object_argument_effects(crate::ir::RuntimeFnId::SplObjectHash.effects(), input)
}
