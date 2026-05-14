//! Purpose:
//! Lowers `class_get_attributes()` into an indexed array of populated
//! synthetic `ReflectionAttribute` objects for class-level attributes.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Attribute-object construction is shared with `ReflectionClass`,
//!   `ReflectionMethod`, and `ReflectionProperty` codegen so `getName()`,
//!   `getArguments()`, and `newInstance()` agree across all reflection paths.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// `class_get_attributes($class)`: return an indexed array of populated
/// `ReflectionAttribute` instances, one per attribute attached to the
/// class declaration. The class argument must be a compile-time string
/// literal — at codegen time we walk `ClassInfo.attribute_names` and
/// `ClassInfo.attribute_args` to fully unroll the construction sequence.
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

    Some(crate::codegen::reflection::emit_reflection_attribute_array(
        &class_info.attribute_names,
        &class_info.attribute_args,
        emitter,
        ctx,
        data,
    ))
}
