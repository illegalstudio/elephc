use super::super::super::abi;
use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::expr::{coerce_result_to_type, emit_expr};
use crate::names::static_property_symbol;
use crate::parser::ast::{Expr, StaticReceiver};
use crate::types::PhpType;

pub(crate) fn emit_static_property_assign_stmt(
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("::${} = ...", property));

    let Some((class_name, declaring_class, prop_ty, declared)) =
        resolve_static_property(receiver, property, ctx, emitter)
    else {
        return;
    };
    let mut val_ty = emit_expr(value, emitter, ctx, data);
    let boxed_to_mixed = declared
        && matches!(prop_ty, PhpType::Mixed | PhpType::Union(_))
        && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_));
    if declared {
        coerce_result_to_type(emitter, ctx, data, &val_ty, &prop_ty);
        val_ty = prop_ty.clone();
    }
    if !boxed_to_mixed {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }

    emitter.comment(&format!("store {}::${}", class_name, property));
    let symbol = static_property_symbol(&declaring_class, property);
    abi::emit_store_result_to_symbol(emitter, &symbol, &val_ty, true);
}

fn resolve_static_property(
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
