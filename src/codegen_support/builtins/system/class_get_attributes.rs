//! Purpose:
//! Lowers `class_get_attributes()` into an indexed array of populated
//! synthetic `ReflectionAttribute` objects for class-level attributes.
//!
//! Called from:
//! - `crate::codegen_support::builtins::system::emit()`.
//!
//! Key details:
//! - Attribute-object construction is shared with `ReflectionClass`,
//!   `ReflectionMethod`, and `ReflectionProperty` codegen so `getName()`,
//!   `getArguments()`, and `newInstance()` agree across all reflection paths.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits codegen for `class_get_attributes($class)`.
///
/// Returns an indexed array of populated `ReflectionAttribute` instances,
/// one per attribute attached to the class declaration.
///
/// ## Arguments
/// - `$class` must be a compile-time string literal naming the class.
///   At codegen time, `ClassInfo.attribute_names` and `ClassInfo.attribute_args`
///   are walked to fully unroll the construction sequence.
///
/// ## Fallback behavior
/// - If `$class` is not a string literal, returns `Some(Array<Object<ReflectionAttribute>>)`
///   without emitting any instructions.
/// - If the class cannot be resolved, returns `Some(Array<Object<ReflectionAttribute>>)`
///   without emitting any instructions.
///
/// ## Ownership
/// - `class_info` is cloned from `ctx.classes`; no ownership is transferred.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("class_get_attributes()");
    let class_name = match args.first().map(|a| &a.kind) {
        Some(ExprKind::StringLiteral(name)) => name.clone(),
        _ => {
            return Some(PhpType::Array(Box::new(PhpType::Object(
                "ReflectionAttribute".to_string(),
            ))))
        }
    };

    let Some(class_info) = super::resolve_class_name(ctx, &class_name)
        .and_then(|resolved| ctx.classes.get(resolved))
        .cloned()
    else {
        return Some(PhpType::Array(Box::new(PhpType::Object(
            "ReflectionAttribute".to_string(),
        ))));
    };

    Some(crate::codegen_support::reflection::emit_reflection_attribute_array(
        &class_info.attribute_names,
        &class_info.attribute_args,
        emitter,
        ctx,
        data,
    ))
}
