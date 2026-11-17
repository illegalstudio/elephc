//! Purpose:
//! Home of the internal `__elephc_eval_reflection_class_build` builtin: builds a fresh,
//! complete `ReflectionClass` through the eval bridge for the class name an eval-built member
//! reflector stored.
//!
//! Called from:
//! - The synthetic `getDeclaringClass()` / `ReflectionParameter::getClass()` getters
//!   (`crate::types::checker::builtin_types::reflection`).
//!
//! Key details:
//! - `internal: true`: never PHP-visible.
//! - The eval twin of `__elephc_reflection_class_build`: compiled reflectors store a builder
//!   address, eval-built ones the declaring class's name, and the getters dispatch on which.
//! - Lowers to a call into libelephc-magician only when the program links the eval bridge;
//!   without it no reflector can hold a class name and the call yields `false`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "__elephc_eval_reflection_class_build",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcEvalReflectionClassBuild,
    ),
}

/// Types the eval build call as the `ReflectionClass` it returns.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Object("ReflectionClass".to_string()))
}
