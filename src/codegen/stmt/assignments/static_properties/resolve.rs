//! Purpose:
//! Lowers static receiver and property metadata resolution.
//! Works with static property symbols and class metadata instead of local frame slots.
//!
//! Called from:
//! - `crate::codegen::stmt::assignments::static_properties`
//!
//! Key details:
//! - Late-bound receivers and visibility checks must match PHP inheritance semantics before storage is updated.

use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::parser::ast::StaticReceiver;
use crate::types::PhpType;

pub(super) fn resolve_static_property(
    receiver: &StaticReceiver,
    property: &str,
    ctx: &Context,
    emitter: &mut Emitter,
) -> Option<(String, String, PhpType, bool)> {
    let class_name = match receiver {
        StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
        StaticReceiver::Self_ | StaticReceiver::Static => match &ctx.current_class {
            Some(class_name) => class_name.clone(),
            None => {
                emitter.comment("WARNING: self::/static:: used outside class scope");
                return None;
            }
        },
        StaticReceiver::Parent => {
            let current_class = match &ctx.current_class {
                Some(class_name) => class_name.clone(),
                None => {
                    emitter.comment("WARNING: parent:: used outside class scope");
                    return None;
                }
            };
            match ctx.classes.get(&current_class).and_then(|info| info.parent.clone()) {
                Some(parent_name) => parent_name,
                None => {
                    emitter.comment(&format!("WARNING: class {} has no parent", current_class));
                    return None;
                }
            }
        }
    };

    let class_info = match ctx.classes.get(&class_name) {
        Some(class_info) => class_info,
        None => {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return None;
        }
    };
    let prop_ty = match class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
    {
        Some(prop_ty) => prop_ty,
        None => {
            emitter.comment(&format!(
                "WARNING: undefined static property {}::${}",
                class_name, property
            ));
            return None;
        }
    };
    let declaring_class = class_info
        .static_property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.clone());
    let declared = class_info.declared_static_properties.contains(property);
    Some((class_name, declaring_class, prop_ty, declared))
}
