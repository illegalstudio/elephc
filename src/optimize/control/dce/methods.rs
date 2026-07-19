//! Purpose:
//! Handles DCE methods cases.
//! Preserves observable effects while removing unreachable tails, redundant branches, or dead writes.
//!
//! Called from:
//! - `crate::optimize::control::dce`
//!
//! Key details:
//! - The pass must remain conservative around throws, finally blocks, switch fallthrough, method calls, and variable writes.
//! - The generated PDO constructor and attribute dispatchers bypass DCE because their large
//!   branch chains are trusted compiler input and provide no useful optimization opportunity.

use super::*;

/// Applies DCE to a class method, recording the class context for effect tracking.
/// `class_name` is used for effect correlation; `parent_name` tracks inheritance
/// when present. Preserves observable effects (throws, calls, writes) while
/// removing unreachable tails and dead branches within the method body. The
/// compiler-owned PDO constructor and attribute dispatchers are retained verbatim because their
/// large branch chains make tail-sensitive DCE expensive while offering no semantic benefit.
pub(crate) fn dce_method(method: ClassMethod, class_name: &str, parent_name: Option<&str>) -> ClassMethod {
    if class_name.eq_ignore_ascii_case("PDO")
        && (method.name.eq_ignore_ascii_case("__construct")
            || method.name.eq_ignore_ascii_case("setAttribute")
            || method.name.eq_ignore_ascii_case("getAttribute"))
    {
        return method;
    }
    let context = ClassEffectContext {
        class_name: class_name.to_string(),
        parent_name: parent_name.map(str::to_string),
    };
    ClassMethod {
        body: with_class_effect_context(Some(context), || dce_block(method.body)),
        ..method
    }
}

/// Applies DCE to a class method without recording class context.
/// Used for methods where the class hierarchy is irrelevant to effect analysis,
/// such as methods defined outside class bodies or during incremental passes.
pub(crate) fn dce_method_without_context(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        body: with_class_effect_context(None, || dce_block(method.body)),
        ..method
    }
}
