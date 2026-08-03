//! Purpose:
//! Lays out and initializes PHP static properties for the wasm32-wasi backend, and
//! resolves `Class::$prop` labels to their slot addresses.
//!
//! Called from:
//! - `crate::codegen_wasm::plan::generate()` to reserve the region and emit the
//!   initializer, before `heap_base` is computed.
//! - `crate::codegen_wasm::inst` for `Op::LoadStaticProperty` / `Op::StoreStaticProperty`.
//!
//! Key details:
//! - A static property is ONE slot for the whole program, shaped exactly like an object
//!   property slot: `value_lo` at +0 and `value_hi`/tag at +8. That is what lets the
//!   loads, stores and string handling be the same code the instance path already uses.
//! - The slot is keyed by the DECLARING class, not by the class named at the use site:
//!   `Child::$count` and `Parent::$count` are the same storage when `Child` inherits it,
//!   which is what PHP does.
//! - The region lives in static memory below the heap, so it is never reallocated and a
//!   slot address is a compile-time constant.
//! - Initialization runs once at module entry rather than from data-segment bytes,
//!   because a string default has to become an OWNED heap copy — the same reason the
//!   instance path persists rather than storing the literal's address.

use std::collections::HashMap;

use crate::codegen::{literal_default_value, LiteralDefaultValue};
use crate::ir::Module;
use crate::types::{ClassInfo, PhpType};

use super::wat::{DataSegment, WatModule};

/// One static property's compile-time placement.
#[derive(Clone, Debug)]
pub(super) struct StaticSlot {
    /// Absolute byte address of the 16-byte slot in linear memory.
    pub(super) address: u32,
    /// The property's declared PHP type, which decides the slot's shape.
    pub(super) php_type: PhpType,
}

/// The module's static-property placement, keyed by `"DeclaringClass::name"`.
///
/// Enum case singletons share this map under the same `"Enum::CASE"` key shape: a case is
/// PHP's own class constant holding an object, so one placement serves both.
pub(super) type StaticSlots = HashMap<String, StaticSlot>;

/// Returns the declaring class of `property` as seen from `class_name`.
///
/// A class that inherits a static shares its parent's storage, so the key must be the
/// declaring class or `Child::$n` and `Parent::$n` would get two slots and diverge.
fn declaring_class(class_infos: &HashMap<String, ClassInfo>, class_name: &str, property: &str) -> String {
    class_infos
        .get(class_name)
        .and_then(|info| info.static_property_declaring_classes.get(property))
        .cloned()
        .unwrap_or_else(|| class_name.to_string())
}

/// Reserves one 16-byte slot per distinct static property and returns the placement plus
/// the advanced static-data cursor.
///
/// The walk is over `class_infos` sorted by name so the layout is deterministic across
/// builds; a property already placed under its declaring class is not placed again.
pub(super) fn plan_static_slots(
    wm: &mut WatModule,
    module: &Module,
    default_strings: &HashMap<String, (u32, u32)>,
    mut cursor: u32,
) -> (StaticSlots, u32) {
    cursor = (cursor + 7) & !7;
    let mut slots: StaticSlots = HashMap::new();
    let mut class_names: Vec<&String> = module.class_infos.keys().collect();
    class_names.sort();
    for class_name in class_names {
        let Some(info) = module.class_infos.get(class_name) else {
            continue;
        };
        for (property, php_type) in &info.static_properties {
            let owner = declaring_class(&module.class_infos, class_name, property);
            let key = slot_key(&owner, property);
            if slots.contains_key(&key) {
                continue;
            }
            let index = info
                .static_properties
                .iter()
                .position(|(name, _)| name == property);
            let bytes = index
                .and_then(|index| info.static_defaults.get(index).cloned().flatten())
                .and_then(|default| {
                    literal_default_value(
                        &format!("static property ${property}"),
                        php_type,
                        &default.kind,
                        "StaticInit",
                    )
                    .ok()
                })
                .and_then(|literal| slot_bytes(&literal, default_strings))
                .unwrap_or_else(|| vec![0u8; 16]);
            wm.add_data(DataSegment {
                offset: cursor,
                bytes,
            });
            slots.insert(
                key,
                StaticSlot {
                    address: cursor,
                    php_type: php_type.clone(),
                },
            );
            cursor += 16;
        }
    }
    // One pointer slot per enum case. A case is a SINGLETON object PHP builds once, so the
    // slot starts at zero and the first read materializes it — the same lazy shape the
    // native uses, and the reason no initializer has to run before `main`.
    let mut enum_names: Vec<&String> = module.enum_infos.keys().collect();
    enum_names.sort();
    for enum_name in enum_names {
        let Some(info) = module.enum_infos.get(enum_name) else {
            continue;
        };
        for case in &info.cases {
            let key = slot_key(enum_name, &case.name);
            if slots.contains_key(&key) {
                continue;
            }
            wm.add_data(DataSegment {
                offset: cursor,
                bytes: vec![0u8; 16],
            });
            slots.insert(
                key,
                StaticSlot {
                    address: cursor,
                    php_type: PhpType::Object(enum_name.clone()),
                },
            );
            cursor += 16;
        }
    }
    (slots, cursor)
}

/// Resolves an `Enum::CASE` label to the case's singleton slot, and the case itself.
pub(super) fn resolve_enum_case<'a>(
    module: &'a Module,
    slots: &'a StaticSlots,
    label: &'a str,
) -> Option<(&'a StaticSlot, &'a str, &'a crate::types::EnumCaseInfo)> {
    let (enum_name, case_name) = label.split_once("::")?;
    let enum_name = enum_name.trim_start_matches('\\');
    let info = module.enum_infos.get(enum_name)?;
    let case = info.cases.iter().find(|case| case.name == case_name)?;
    let slot = slots.get(&slot_key(enum_name, case_name))?;
    Some((slot, enum_name, case))
}

/// Builds the placement key for one static property.
pub(super) fn slot_key(declaring_class: &str, property: &str) -> String {
    format!("{}::{}", declaring_class, property)
}

/// Resolves a `Class::$prop` label — the form the EIR interns for these ops — to its slot.
///
/// `static::` is deliberately unresolved: late static binding picks the slot from the
/// CALLED class at runtime, which this placement cannot express.
pub(super) fn resolve_label<'a>(
    module: &Module,
    slots: &'a StaticSlots,
    label: &str,
) -> Option<&'a StaticSlot> {
    let (receiver, property) = label.split_once("::")?;
    let receiver = receiver.trim_start_matches('\\');
    if receiver == "static" || receiver == "self" || receiver == "parent" {
        return None;
    }
    let property = property.trim_start_matches('$');
    let owner = declaring_class(&module.class_infos, receiver, property);
    slots.get(&slot_key(&owner, property))
}

/// Renders one static property's initial 16 bytes: `value_lo` then `value_hi`.
///
/// A string default carries the LITERAL's static-data address rather than an owned heap
/// copy, which is safe precisely because the refcount helpers no-op below the heap: a
/// later assignment releases the old value and the literal is simply left alone, and a
/// read acquires its own persisted copy. That is what lets the whole region be data
/// bytes with no initializer to run.
///
/// `None` means the default has no byte form — a Mixed slot needs a heap cell, which
/// static data cannot express — and the caller refuses the property rather than
/// silently zeroing it.
fn slot_bytes(
    literal: &LiteralDefaultValue,
    default_strings: &HashMap<String, (u32, u32)>,
) -> Option<Vec<u8>> {
    let (lo, hi): (i64, i64) = match literal {
        LiteralDefaultValue::Int(value) => (*value, 0),
        LiteralDefaultValue::Bool(value) => (i64::from(*value), 0),
        LiteralDefaultValue::Float(value) => (value.to_bits() as i64, 0),
        LiteralDefaultValue::Null => (0, 0),
        LiteralDefaultValue::Str(value) => {
            let (pointer, length) = default_strings.get(value)?;
            (i64::from(*pointer), i64::from(*length))
        }
        _ => return None,
    };
    let mut bytes = Vec::with_capacity(16);
    bytes.extend_from_slice(&lo.to_le_bytes());
    bytes.extend_from_slice(&hi.to_le_bytes());
    Some(bytes)
}

/// Builds the same placement the emitter will, for the capability audit.
///
/// The audit runs before any WAT exists, so it cannot pass a module to write data into;
/// the addresses it sees are therefore placeholders. Only the KEY SET and the slot types
/// matter to it — which property resolves, and whether its type has a slot shape — and
/// those are identical either way.
pub(super) fn plan_static_slots_for_audit(module: &Module) -> Option<StaticSlots> {
    let mut probe = WatModule::new();
    Some(plan_static_slots(&mut probe, module, &HashMap::new(), 0).0)
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the static-property placement.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.

    use super::*;
    use crate::codegen_support::platform::Target;
    use crate::types::PhpType;
    use crate::span::Span;
    use std::collections::{HashMap, HashSet};

    /// A `ClassInfo` with every field at its empty value, so a fixture states only what it
    /// is testing.
    fn minimal_class_info(class_id: u64) -> ClassInfo {
        ClassInfo {
            class_id,
            declaration_span: Span::dummy(),
            parent: None,
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            allow_dynamic_properties: false,
            constants: HashMap::new(),
            constant_types: HashMap::new(),
            constant_visibilities: HashMap::new(),
            final_constants: HashSet::new(),
            attribute_names: Vec::new(),
            attribute_args: Vec::new(),
            method_attribute_names: HashMap::new(),
            method_attribute_args: HashMap::new(),
            property_attribute_names: HashMap::new(),
            property_attribute_args: HashMap::new(),
            constant_attribute_names: HashMap::new(),
            constant_attribute_args: HashMap::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
            properties: Vec::new(),
            property_offsets: HashMap::new(),
            property_declaring_classes: HashMap::new(),
            defaults: Vec::new(),
            property_visibilities: HashMap::new(),
            property_set_visibilities: HashMap::new(),
            declared_properties: HashSet::new(),
            property_declared_slots: Vec::new(),
            final_properties: HashSet::new(),
            readonly_properties: HashSet::new(),
            reference_properties: HashSet::new(),
            owned_reference_properties: HashSet::new(),
            promoted_properties: HashSet::new(),
            property_reference_slots: Vec::new(),
            abstract_properties: HashSet::new(),
            abstract_property_hooks: HashMap::new(),
            static_properties: Vec::new(),
            static_defaults: Vec::new(),
            static_property_declaring_classes: HashMap::new(),
            static_property_visibilities: HashMap::new(),
            declared_static_properties: HashSet::new(),
            final_static_properties: HashSet::new(),
            method_decls: Vec::new(),
            methods: HashMap::new(),
            static_methods: HashMap::new(),
            late_static_method_returns: HashMap::new(),
            late_static_static_method_returns: HashMap::new(),
            callable_method_return_sigs: HashMap::new(),
            callable_array_method_return_sigs: HashMap::new(),
            method_visibilities: HashMap::new(),
            final_methods: HashSet::new(),
            method_declaring_classes: HashMap::new(),
            method_impl_classes: HashMap::new(),
            vtable_methods: Vec::new(),
            vtable_slots: HashMap::new(),
            static_method_visibilities: HashMap::new(),
            final_static_methods: HashSet::new(),
            static_method_declaring_classes: HashMap::new(),
            static_method_impl_classes: HashMap::new(),
            static_vtable_methods: Vec::new(),
            static_vtable_slots: HashMap::new(),
            interfaces: Vec::new(),
            constructor_param_to_prop: Vec::new(),
        }
    }

    /// Builds a module with one class declaring `$n`, and one child inheriting it.
    fn inheriting_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let mut parent = minimal_class_info(1);
        parent.static_properties = vec![("n".to_string(), PhpType::Int)];
        parent.static_defaults = vec![None];
        module.class_infos.insert("Parent".to_string(), parent);

        let mut child = minimal_class_info(2);
        child.parent = Some("Parent".to_string());
        child.static_properties = vec![("n".to_string(), PhpType::Int)];
        child.static_defaults = vec![None];
        child
            .static_property_declaring_classes
            .insert("n".to_string(), "Parent".to_string());
        module.class_infos.insert("Child".to_string(), child);
        module
    }

    /// An inherited static is ONE storage: PHP has `Child::$n` and `Parent::$n` name the same
    /// slot, so `Op::LoadStaticProperty` on either must resolve to the same address. Keying the
    /// placement by the USE-SITE class instead would give two slots that silently diverge.
    #[test]
    fn inherited_statics_share_one_slot() {
        // This placement is what `Op::LoadStaticProperty` and `Op::StoreStaticProperty` read;
        // if either stops being admitted, the addresses below serve no lowering.
        for op in [
            crate::ir::Op::LoadStaticProperty,
            crate::ir::Op::StoreStaticProperty,
        ] {
            assert!(super::super::capability::op_is_supported(op));
        }

        let module = inheriting_module();
        let slots = plan_static_slots_for_audit(&module).expect("placement");
        assert_eq!(slots.len(), 1, "the inherited static must not be placed twice");

        let from_parent = resolve_label(&module, &slots, "Parent::$n").expect("parent resolves");
        let from_child = resolve_label(&module, &slots, "Child::$n").expect("child resolves");
        assert_eq!(
            from_parent.address, from_child.address,
            "Child::$n and Parent::$n must be the same storage"
        );
    }

    /// Every enum case gets its OWN singleton slot: `Op::ScopedConstantGet` reads a pointer
    /// that starts at zero and materializes once, so two cases sharing a slot would make
    /// `Suit::Hearts === Suit::Spades` answer true.
    #[test]
    fn enum_cases_get_one_singleton_slot_each() {
        assert!(super::super::capability::op_is_supported(
            crate::ir::Op::ScopedConstantGet
        ));

        let mut module = Module::new(Target::wasm());
        module.enum_infos.insert(
            "Suit".to_string(),
            crate::types::EnumInfo {
                backing_type: Some(PhpType::Str),
                cases: vec![
                    crate::types::EnumCaseInfo {
                        name: "Hearts".to_string(),
                        value: Some(crate::types::EnumCaseValue::Str("H".to_string())),
                        attribute_names: Vec::new(),
                        attribute_args: Vec::new(),
                    },
                    crate::types::EnumCaseInfo {
                        name: "Spades".to_string(),
                        value: Some(crate::types::EnumCaseValue::Str("S".to_string())),
                        attribute_names: Vec::new(),
                        attribute_args: Vec::new(),
                    },
                ],
            },
        );
        let slots = plan_static_slots_for_audit(&module).expect("placement");
        let hearts = resolve_enum_case(&module, &slots, "Suit::Hearts").expect("Hearts resolves");
        let spades = resolve_enum_case(&module, &slots, "Suit::Spades").expect("Spades resolves");
        assert_ne!(
            hearts.0.address, spades.0.address,
            "two cases must not share one singleton"
        );
        assert!(
            resolve_enum_case(&module, &slots, "Suit::Missing").is_none(),
            "an unknown case must not resolve"
        );
    }

    /// `static::`, `self::` and `parent::` pick their slot from the CALLED class at runtime,
    /// which a compile-time address cannot follow — so they resolve to nothing and the audit
    /// refuses them rather than binding the wrong storage.
    #[test]
    fn late_bound_receivers_do_not_resolve() {
        let module = inheriting_module();
        let slots = plan_static_slots_for_audit(&module).expect("placement");
        for label in ["static::$n", "self::$n", "parent::$n"] {
            assert!(
                resolve_label(&module, &slots, label).is_none(),
                "{label} must not bind a compile-time slot"
            );
        }
    }
}
