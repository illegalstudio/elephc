use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::names::static_property_symbol;
use crate::parser::ast::StaticReceiver;
use crate::types::PhpType;

pub(super) fn emit_static_property_access(
    receiver: &StaticReceiver,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let Some((class_name, declaring_class, prop_ty)) =
        resolve_static_property(receiver, property, ctx, emitter)
    else {
        return PhpType::Int;
    };

    emitter.comment(&format!("{}::${}", class_name, property));
    let symbol = static_property_symbol(&declaring_class, property);
    abi::emit_load_symbol_to_result(emitter, &symbol, &prop_ty);
    prop_ty
}

pub(crate) fn resolve_static_property(
    receiver: &StaticReceiver,
    property: &str,
    ctx: &Context,
    emitter: &mut Emitter,
) -> Option<(String, String, PhpType)> {
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
    Some((class_name, declaring_class, prop_ty))
}
