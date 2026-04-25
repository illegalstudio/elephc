use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::types::PhpType;

pub(super) struct PropertyAssignTarget {
    pub(super) class_name: String,
    pub(super) offset: usize,
    pub(super) prop_ty: PhpType,
    pub(super) needs_deref: bool,
    pub(super) is_reference: bool,
}

pub(super) enum PropertyAssignResolution {
    Resolved(PropertyAssignTarget),
    UseMagicSet(String),
    Abort,
}

pub(super) fn resolve_property_assign_target(
    obj_ty: &PhpType,
    property: &str,
    magic_set_class: Option<&str>,
    emitter: &mut Emitter,
    ctx: &Context,
) -> PropertyAssignResolution {
    match obj_ty {
        PhpType::Object(class_name) => resolve_object_property_target(
            class_name,
            property,
            magic_set_class,
            emitter,
            ctx,
        ),
        PhpType::Pointer(Some(class_name)) if ctx.extern_classes.contains_key(class_name) => {
            resolve_extern_field_target(class_name, property, emitter, ctx)
        }
        PhpType::Pointer(Some(class_name)) if ctx.packed_classes.contains_key(class_name) => {
            resolve_packed_field_target(class_name, property, emitter, ctx)
        }
        _ => {
            emitter.comment("WARNING: property assign on non-object");
            PropertyAssignResolution::Abort
        }
    }
}

fn resolve_object_property_target(
    class_name: &str,
    property: &str,
    magic_set_class: Option<&str>,
    emitter: &mut Emitter,
    ctx: &Context,
) -> PropertyAssignResolution {
    let class_info = match ctx.classes.get(class_name).cloned() {
        Some(c) => c,
        None => {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return PropertyAssignResolution::Abort;
        }
    };
    let prop_ty = match class_info.properties.iter().find(|(n, _)| n == property) {
        Some((_, ty)) => ty.clone(),
        None => {
            if let Some(magic_class_name) = magic_set_class {
                return PropertyAssignResolution::UseMagicSet(magic_class_name.to_string());
            }
            emitter.comment(&format!("WARNING: undefined property {}", property));
            return PropertyAssignResolution::Abort;
        }
    };
    let offset = match class_info.property_offsets.get(property) {
        Some(offset) => *offset,
        None => {
            emitter.comment(&format!("WARNING: missing property offset {}", property));
            return PropertyAssignResolution::Abort;
        }
    };
    PropertyAssignResolution::Resolved(PropertyAssignTarget {
        class_name: class_name.to_string(),
        offset,
        prop_ty,
        needs_deref: false,
        is_reference: class_info.reference_properties.contains(property),
    })
}

fn resolve_extern_field_target(
    class_name: &str,
    property: &str,
    emitter: &mut Emitter,
    ctx: &Context,
) -> PropertyAssignResolution {
    let class_info = match ctx.extern_classes.get(class_name).cloned() {
        Some(c) => c,
        None => {
            emitter.comment(&format!("WARNING: undefined extern class {}", class_name));
            return PropertyAssignResolution::Abort;
        }
    };
    let field = match class_info.fields.iter().find(|field| field.name == property) {
        Some(field) => field.clone(),
        None => {
            emitter.comment(&format!("WARNING: undefined extern field {}", property));
            return PropertyAssignResolution::Abort;
        }
    };
    PropertyAssignResolution::Resolved(PropertyAssignTarget {
        class_name: class_name.to_string(),
        offset: field.offset,
        prop_ty: field.php_type,
        needs_deref: true,
        is_reference: false,
    })
}

fn resolve_packed_field_target(
    class_name: &str,
    property: &str,
    emitter: &mut Emitter,
    ctx: &Context,
) -> PropertyAssignResolution {
    let class_info = match ctx.packed_classes.get(class_name).cloned() {
        Some(c) => c,
        None => {
            emitter.comment(&format!("WARNING: undefined packed class {}", class_name));
            return PropertyAssignResolution::Abort;
        }
    };
    let field = match class_info.fields.iter().find(|field| field.name == property) {
        Some(field) => field.clone(),
        None => {
            emitter.comment(&format!("WARNING: undefined packed field {}", property));
            return PropertyAssignResolution::Abort;
        }
    };
    PropertyAssignResolution::Resolved(PropertyAssignTarget {
        class_name: class_name.to_string(),
        offset: field.offset,
        prop_ty: field.php_type,
        needs_deref: true,
        is_reference: false,
    })
}
