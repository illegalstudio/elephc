//! Purpose:
//! Lowers property target resolution for object, extern, and packed-field writes.
//! Shares receiver and property metadata with object expression lowering.
//!
//! Called from:
//! - `crate::codegen_support::stmt::assignments::properties`
//!
//! Key details:
//! - Property writes must respect declared types, visibility checks, and runtime object layout.

use crate::codegen_support::context::Context;
use crate::codegen_support::emit::Emitter;
use crate::types::PhpType;

/// The resolved physical slot for a property assignment: the class that
/// owns the property, the byte offset of the slot within the object,
/// the PHP type of the property, whether the slot must be dereferenced
/// before use (for extern/packed pointers), and whether the property
/// is a PHP reference (so the cell address is stored rather than the value).
pub(super) struct PropertyAssignTarget {
    /// The fully-qualified class name that declares this property.
    pub(super) class_name: String,
    /// Byte offset of the property's slot within the object layout.
    pub(super) offset: usize,
    /// PHP type of the stored value (controls register/Stack slot selection).
    pub(super) prop_ty: PhpType,
    /// True for extern/packed pointers: load through the pointer before reading
    /// or writing the field value.
    pub(super) needs_deref: bool,
    /// True when the property is declared `&$name`; the slot holds a `PhpRef`
    /// cell pointer rather than a direct value.
    pub(super) is_reference: bool,
}

/// The result of resolving a property assignment target. `Resolved` means
/// the property has a concrete storage slot. `UseMagicSet` means the
/// property is undeclared but the class has a `__set` magic method. `UseDynamicProperty`
/// means the class has `#[\AllowDynamicProperties]` and the write should route
/// through the per-object hashtable. `Abort` means resolution failed and
/// assignment cannot proceed.
pub(super) enum PropertyAssignResolution {
    Resolved(PropertyAssignTarget),
    UseMagicSet(String),
    /// Class declares `#[\AllowDynamicProperties]`: route the write through
    /// the per-object hashtable side-table at the given byte offset.
    UseDynamicProperty {
        #[allow(dead_code)] // reserved for future diagnostics
        class_name: String,
        dyn_slot_offset: usize,
    },
    Abort,
}

/// Resolves the assignment target for a property write. Returns
/// `Resolved(PropertyAssignTarget)` for concrete slots, `UseMagicSet(class_name)`
/// when the property is unset but the class has `__set`, `UseDynamicProperty`
/// when the class has `#[\AllowDynamicProperties]`, or `Abort` for invalid
/// or unsupported targets. Handles `Object`, `Pointer<extern>` (FFI fields),
/// and `Pointer<packed>` (packed struct fields).
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

/// Resolves a property assignment target for a declared `Object` class.
/// Looks up the property in `class_info.properties`. If not found and
/// `magic_set_class` is provided, returns `UseMagicSet`. If the class
/// has `#[\AllowDynamicProperties]`, returns `UseDynamicProperty` with the
/// slot offset computed as `8 + num_props * 16` (pointing past the object
/// header and existing named-property slots). Otherwise emits a warning
/// and returns `Abort`. On success returns `Resolved` with `needs_deref=false`
/// and `is_reference` set from `class_info.reference_properties`.
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
            if class_info.allow_dynamic_properties {
                let dyn_slot_offset = 8 + class_info.properties.len() * 16;
                return PropertyAssignResolution::UseDynamicProperty {
                    class_name: class_name.to_string(),
                    dyn_slot_offset,
                };
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

/// Resolves a property assignment target for an `extern` (FFI) class field.
/// Looks up the field by name in `ctx.extern_classes[class_name].fields`.
/// Returns `Resolved` with `needs_deref=true` and the field's offset and type,
/// or `Abort` if the class or field is not found.
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

/// Resolves a property assignment target for a `packed` struct field.
/// Looks up the field by name in `ctx.packed_classes[class_name].fields`.
/// Returns `Resolved` with `needs_deref=true` and the field's offset and type,
/// or `Abort` if the class or field is not found.
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
