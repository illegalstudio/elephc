//! Purpose:
//! Home of the internal `__elephc_reflection_class_build` builtin: builds a fresh, complete
//! `ReflectionClass` by calling the compiled per-class builder whose address a member
//! reflector stored.
//!
//! Called from:
//! - The synthetic `getDeclaringClass()` / `ReflectionParameter::getClass()` getters
//!   (`crate::types::checker::builtin_types::reflection`).
//!
//! Key details:
//! - `internal: true`: never PHP-visible.
//! - Member reflectors store the builder ADDRESS, not a `ReflectionClass`: building one per
//!   member would cost the class's default and static property arrays per member, and
//!   sharing one complete reflector would form a cycle between it and its own members.
//!   Building at call time gives php's fresh object per call with no cycle.
//! - AOT only: eval-built reflectors keep storing the object itself, so eval never needs
//!   an execution route for this builtin.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "__elephc_reflection_class_build",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcReflectionClassBuild,
    ),
}

/// Types the builder call as the `ReflectionClass` it returns.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Object("ReflectionClass".to_string()))
}
