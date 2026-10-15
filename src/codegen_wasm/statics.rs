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
/// `self::` and `parent::` are COMPILE-TIME scopes: they name the class the method is written
/// in, and its parent, so `scope` — the enclosing function's own class — resolves them here.
///
/// `static::` is the one that cannot be: late static binding picks the slot from the CALLED
/// class at run time, which one compile-time address cannot follow. `late_static_slots` is what
/// serves that form, with a ladder over the classes the call could have named.
pub(super) fn resolve_label<'a>(
    module: &Module,
    slots: &'a StaticSlots,
    label: &str,
    scope: Option<&str>,
) -> Option<&'a StaticSlot> {
    let (receiver, property) = label.split_once("::")?;
    let receiver = receiver.trim_start_matches('\\');
    let property = property.trim_start_matches('$');
    let receiver = match receiver {
        "static" => return None,
        "self" => scope?.to_string(),
        "parent" => module
            .class_infos
            .get(scope?)
            .and_then(|info| info.parent.clone())?,
        named => named.to_string(),
    };
    let owner = declaring_class(&module.class_infos, &receiver, property);
    slots.get(&slot_key(&owner, property))
}

/// The class a function is written IN, which is what `self::` and `static::` resolve against.
///
/// The EIR names a method `Class::method`, so the scope is the part before the separator; a free
/// function has none, and a `self::` inside one is a program the checker already rejected.
pub(super) fn enclosing_class(function_name: &str) -> Option<&str> {
    function_name.rsplit_once("::").map(|(class, _)| class)
}

/// Every slot a `static::$prop` could name, paired with the class id that selects it.
///
/// Late static binding resolves against the CALLED class, which every static call on this target
/// already forwards as hidden parameter 0 — so the answer is a ladder over the classes that could
/// have been called, exactly as `new static()` is. Sorted by class id so the emitted ladder is
/// stable across builds.
pub(super) fn late_static_slots<'a>(
    module: &Module,
    slots: &'a StaticSlots,
    property: &str,
    scope: &str,
) -> Vec<(u64, &'a StaticSlot)> {
    let mut arms: Vec<(u64, &StaticSlot)> = Vec::new();
    for (class_name, info) in &module.class_infos {
        if !class_is_scope_or_descendant(module, class_name, scope) {
            continue;
        }
        let owner = declaring_class(&module.class_infos, class_name, property);
        if let Some(slot) = slots.get(&slot_key(&owner, property)) {
            arms.push((info.class_id, slot));
        }
    }
    arms.sort_by_key(|(class_id, _)| *class_id);
    arms.dedup_by_key(|(class_id, _)| *class_id);
    arms
}

/// Whether `class_name` IS `scope` or inherits from it.
fn class_is_scope_or_descendant(module: &Module, class_name: &str, scope: &str) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        if name == scope {
            return true;
        }
        current = module
            .class_infos
            .get(&name)
            .and_then(|info| info.parent.clone());
    }
    false
}

/// The container-typed static properties whose default has to be BUILT rather than stamped.
///
/// Static data can express bytes, and a scalar or a string literal is bytes — an array is not.
/// Its slot therefore starts as a null pointer and `main`'s prologue puts a real container there,
/// which is the earliest point any PHP code can observe it. The literal comes back whole, so a
/// non-empty default like `public static $values = [3, 5];` carries its elements along.
pub(super) fn container_static_defaults(
    module: &Module,
    slots: &StaticSlots,
) -> Vec<(u32, PhpType, Option<LiteralDefaultValue>)> {
    let mut initializers: Vec<(u32, PhpType, Option<LiteralDefaultValue>)> = Vec::new();
    let mut seen: Vec<u32> = Vec::new();
    let mut class_names: Vec<&String> = module.class_infos.keys().collect();
    class_names.sort();
    for class_name in class_names {
        let Some(info) = module.class_infos.get(class_name) else {
            continue;
        };
        for (index, (property, php_type)) in info.static_properties.iter().enumerate() {
            // A declared `array` is a boxed cell, built here like a concrete container is.
            if !php_type.is_php_array()
                && !matches!(
                    php_type.codegen_repr(),
                    PhpType::Array(_) | PhpType::AssocArray { .. }
                )
            {
                continue;
            }
            let owner = declaring_class(&module.class_infos, class_name, property);
            let Some(slot) = slots.get(&slot_key(&owner, property)) else {
                continue;
            };
            if seen.contains(&slot.address) {
                continue;
            }
            seen.push(slot.address);
            let literal = info
                .static_defaults
                .get(index)
                .cloned()
                .flatten()
                .and_then(|default| {
                    literal_default_value(
                        &format!("static property ${property}"),
                        php_type,
                        &default.kind,
                        "StaticInit",
                    )
                    .ok()
                });
            initializers.push((slot.address, php_type.codegen_repr(), literal));
        }
    }
    initializers.sort_by_key(|(address, _, _)| *address);
    initializers
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
        // A TAGGED slot is the `int|null` pair written flat: the payload in the first word and
        // the Mixed tag in the second. Its null is NOT sixteen zero bytes — tag 0 is the
        // INTEGER tag, so a zeroed tagged slot reads as `0` rather than as null, which is the
        // one way this layout can be got wrong silently.
        LiteralDefaultValue::TaggedNull => (0, 8),
        LiteralDefaultValue::TaggedInt(value) => (*value, 0),
        // A BOXED slot holds one cell pointer, and the null pointer is php's null here — the
        // same value an unwritten slot carries, which is why `__rt_mixed_tag_of` reads address
        // zero as tag 8. A non-null boxed default would need a heap cell before main runs, and
        // the audit refuses those rather than laying down a zero that reads as null.
        LiteralDefaultValue::BoxedNull => (0, 0),
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
/// Keys a PHP GLOBAL's slot inside the same map the static properties use.
///
/// The `::` in a static-property key is a legal PHP class/property separator, so a plain global
/// name could never be confused with one — but the prefix makes that explicit rather than
/// relying on it, and keeps both kinds in one map so no extra plumbing threads a second one
/// through every function lowering.
pub(super) fn global_slot_key(name: &str) -> String {
    format!("global ${name}")
}

/// Reserves one 16-byte slot per PHP global the module reads or writes, and returns the cursor.
///
/// `argc`/`argv` are answered by WASI helpers rather than storage, so they get no slot. Every
/// other name is sized like a static property: an int or float is one word, a string is a
/// pointer and a length. The TYPE comes from the instructions themselves — a store's operand or
/// a load's result — since the checker gives a global one type across the module.
pub(super) fn plan_global_slots(
    module: &Module,
    slots: &mut StaticSlots,
    mut cursor: u32,
) -> u32 {
    cursor = (cursor + 7) & !7;
    let mut named: Vec<(String, PhpType)> = Vec::new();
    for function in super::function::all_functions(module) {
        for inst in &function.instructions {
            let Some(crate::ir::Immediate::GlobalName(data_id)) = inst.immediate else {
                continue;
            };
            if !matches!(inst.op, crate::ir::Op::LoadGlobal | crate::ir::Op::StoreGlobal) {
                continue;
            }
            let Some(name) = module.data.global_names.get(data_id.as_raw() as usize) else {
                continue;
            };
            if name == "argc" || name == "argv" || named.iter().any(|(seen, _)| seen == name) {
                continue;
            }
            let php_type = if inst.op == crate::ir::Op::StoreGlobal {
                inst.operands
                    .first()
                    .and_then(|value| function.value(*value))
                    .map(|value| value.php_type.clone())
            } else {
                Some(inst.result_php_type.clone())
            };
            let Some(php_type) = php_type else { continue };
            named.push((name.clone(), php_type));
        }
    }
    named.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (name, php_type) in named {
        slots.insert(
            global_slot_key(&name),
            StaticSlot {
                address: cursor,
                php_type,
            },
        );
        cursor += 16;
    }
    cursor
}

/// Keys a function's STATIC LOCAL inside the same map the static properties use.
///
/// The key carries the function as well as the variable, because `static $n` in two different
/// functions is two different cells — php keys them by the declaration site, not the name. The
/// native backend says the same thing with `names::static_local_symbol`, which exists for the
/// same reason: building the symbol by concatenation once merged unrelated statics onto one
/// cell.
pub(super) fn static_local_slot_key(function: &str, name: &str) -> String {
    format!("static {function}::${name}")
}

/// Bytes reserved per static local: the 16-byte value slot, then the initialized flag.
///
/// The flag cannot be folded into the value. `static $x = null;` is initialized and holds
/// nothing, and `static $x = 0;` is initialized and holds zero — neither is distinguishable
/// from an untouched slot, so the "has the initializer already run" question needs storage of
/// its own. Linear memory starts zeroed, which makes "not yet" the default without an
/// initializer pass.
pub(super) const STATIC_LOCAL_BYTES: u32 = 24;

/// Offset of the initialized flag inside a static local's reservation.
pub(super) const STATIC_LOCAL_INIT_OFFSET: u32 = 16;

/// Reserves one slot per function static local the module declares, and returns the cursor.
///
/// The TYPE comes from the instruction itself, the way `plan_global_slots` takes a global's:
/// a store or an initializer names it through its operand and a load through its result. All
/// three ops carry the same `LocalSlotId`, so any one of them settles the shape.
pub(super) fn plan_static_local_slots(
    module: &Module,
    slots: &mut StaticSlots,
    mut cursor: u32,
) -> u32 {
    cursor = (cursor + 7) & !7;
    let mut named: Vec<(String, PhpType)> = Vec::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
    {
        for inst in &function.instructions {
            if !matches!(
                inst.op,
                crate::ir::Op::LoadStaticLocal
                    | crate::ir::Op::StoreStaticLocal
                    | crate::ir::Op::InitStaticLocal
                    | crate::ir::Op::StaticLocalUninitialized
            ) {
                continue;
            }
            let Some(crate::ir::Immediate::LocalSlot(slot_id)) = inst.immediate else {
                continue;
            };
            let Some(local) = function.locals.get(slot_id.as_raw() as usize) else {
                continue;
            };
            let Some(name) = local.name.as_deref() else {
                continue;
            };
            let key = static_local_slot_key(&function.name, name);
            if named.iter().any(|(seen, _)| *seen == key) {
                continue;
            }
            named.push((key, local.php_type.codegen_repr()));
        }
    }
    named.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (key, php_type) in named {
        slots.insert(
            key,
            StaticSlot {
                address: cursor,
                php_type,
            },
        );
        cursor += STATIC_LOCAL_BYTES;
    }
    cursor
}

/// Plans the static-property slots against a scratch module, so the capability audit can ask
/// where a static would live without touching the real one.
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
    use crate::ir::{Builder, Function, IrType, LocalKind, Ownership, Terminator};
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
            constant_deprecations: HashMap::new(),
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
            is_internal: false,
            eval_property_storage: false,
            clone_override_property_storage: false,
            scope_dynamic_property_storage: false,
            constant_order: Vec::new(),
            property_hooks: HashMap::new(),
            abstract_methods: HashSet::new(),
            abstract_static_methods: HashSet::new(),
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
    /// A PHP global gets its own 16-byte slot, keyed by name, and `argc`/`argv` get none —
    /// those are answered by WASI helpers, so a slot for them would be storage nothing reads.
    #[test]
    fn global_slots_are_placed_by_name() {
        for op in [crate::ir::Op::LoadGlobal, crate::ir::Op::StoreGlobal] {
            assert!(
                super::super::capability::op_is_supported(op),
                "{op:?} must stay admitted for these slots to serve a lowering"
            );
        }

        let mut module = crate::ir::Module::new(Target::wasm());
        let greeting = module.data.intern_global_name("GREETING");
        let argc = module.data.intern_global_name("argc");
        let mut function =
            crate::ir::Function::new("main".to_string(), crate::ir::IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = crate::ir::Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let text = builder.emit_const_str(crate::ir::DataId::from_raw(0));
            let _ = builder.emit(
                crate::ir::Op::StoreGlobal,
                vec![text],
                Some(crate::ir::Immediate::GlobalName(greeting)),
                crate::ir::IrType::Void,
                PhpType::Void,
                crate::ir::Ownership::NonHeap,
            );
            let _ = builder.emit(
                crate::ir::Op::LoadGlobal,
                Vec::new(),
                Some(crate::ir::Immediate::GlobalName(argc)),
                crate::ir::IrType::I64,
                PhpType::Int,
                crate::ir::Ownership::NonHeap,
            );
            builder.terminate(crate::ir::Terminator::Return { value: None });
        }
        module.add_function(function);

        let mut slots: StaticSlots = HashMap::new();
        let end = plan_global_slots(&module, &mut slots, 64);
        assert!(
            slots.contains_key(&global_slot_key("GREETING")),
            "a stored global needs storage: {slots:?}"
        );
        assert!(
            !slots.contains_key(&global_slot_key("argc")),
            "argc is answered by WASI and must not take a slot"
        );
        assert_eq!(end, 64 + 16, "exactly one 16-byte slot was reserved");
    }

    /// Verifies an inherited static property is ONE slot shared by the parent and its children.
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

        let from_parent = resolve_label(&module, &slots, "Parent::$n", None).expect("parent resolves");
        let from_child = resolve_label(&module, &slots, "Child::$n", None).expect("child resolves");
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

    /// `self::` and `parent::` are COMPILE-TIME scopes and resolve from the enclosing class;
    /// `static::` is the one that cannot, because it picks its slot from the CALLED class at run
    /// time, which a compile-time address cannot follow.
    #[test]
    fn self_and_parent_resolve_from_the_enclosing_class() {
        let module = inheriting_module();
        let slots = plan_static_slots_for_audit(&module).expect("placement");
        let shared = resolve_label(&module, &slots, "Parent::$n", None)
            .expect("the declaring class resolves")
            .address;
        for (label, scope) in [("self::$n", "Child"), ("parent::$n", "Child")] {
            let resolved = resolve_label(&module, &slots, label, Some(scope))
                .unwrap_or_else(|| panic!("{label} in {scope} must resolve"));
            assert_eq!(
                resolved.address, shared,
                "{label} must bind the one slot the hierarchy shares"
            );
        }
        assert!(
            resolve_label(&module, &slots, "static::$n", Some("Child")).is_none(),
            "static:: must not bind a compile-time slot"
        );
    }

    /// The late-bound ladder names one arm per class the call could have been made on, so the
    /// slot is picked at run time from the id its caller forwarded.
    #[test]
    fn late_static_slots_cover_the_scope_and_its_descendants() {
        let module = inheriting_module();
        let slots = plan_static_slots_for_audit(&module).expect("placement");
        let arms = late_static_slots(&module, &slots, "n", "Parent");
        assert_eq!(arms.len(), 2, "Parent and Child each select a slot");
    }

    /// A NULLABLE scalar static's `= null` default is not sixteen zero bytes.
    ///
    /// The slot is the `int|null` pair written flat — payload in the first word, Mixed tag in
    /// the second — and tag 0 is the INTEGER tag. So a zeroed tagged slot reads back as the
    /// integer `0`, which is the one way this layout goes wrong without saying so: the
    /// program prints a number where php prints nothing. The bytes must carry tag 8.
    #[test]
    fn a_nullable_scalar_static_defaults_to_the_null_tag_not_to_zero() {
        let mut module = Module::new(Target::wasm());
        let mut info = minimal_class_info(1);
        info.static_properties = vec![("maybe".to_string(), PhpType::TaggedScalar)];
        info.static_defaults = vec![Some(crate::parser::ast::Expr {
            kind: crate::parser::ast::ExprKind::Null,
            span: Span::dummy(),
        })];
        module.class_infos.insert("C".to_string(), info);

        let mut wm = WatModule::new();
        let (slots, _) = plan_static_slots(&mut wm, &module, &HashMap::new(), 0);
        assert_eq!(slots.len(), 1, "one slot for the one static property");

        let laid_out = wm
            .data_segments()
            .iter()
            .find(|segment| segment.bytes.len() == 16)
            .map(|segment| segment.bytes.clone())
            .expect("the slot's sixteen bytes");
        assert_eq!(&laid_out[..8], &0i64.to_le_bytes(), "the payload is zero");
        assert_eq!(
            &laid_out[8..],
            &8i64.to_le_bytes(),
            "and the tag is php null, not the integer tag a zeroed slot would read as"
        );
    }

    /// Builds a function that declares one `static $name` and reads it.
    ///
    /// The three static-local ops all name the variable the same way, so one read is enough to
    /// make the planner place the cell.
    fn function_with_a_static_local(name: &str, variable: &str, php_type: PhpType) -> Function {
        let mut function = Function::new(name.to_string(), IrType::I64, PhpType::Int);
        let slot = function.add_local(
            Some(variable.to_string()),
            IrType::from_php(&php_type),
            php_type.clone(),
            LocalKind::StaticLocal,
        );
        {
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let read = b
                .emit(
                    crate::ir::Op::LoadStaticLocal,
                    Vec::new(),
                    Some(crate::ir::Immediate::LocalSlot(slot)),
                    IrType::from_php(&php_type),
                    php_type,
                    Ownership::MaybeOwned,
                )
                .expect("the read emits");
            b.terminate(Terminator::Return { value: Some(read) });
        }
        function
    }

    /// Two functions can each declare `static $n`, and they are two variables.
    ///
    /// The cell is keyed by `(function, variable)` for exactly this reason: keying it by the
    /// NAME alone would have `a()` and `b()` share one counter, which is the first thing a
    /// static local must not do. The flag is checked at the same time, because a shared flag
    /// would make the second function's initializer never run.
    #[test]
    fn two_functions_declaring_the_same_static_name_get_two_cells() {
        for op in [
            crate::ir::Op::LoadStaticLocal,
            crate::ir::Op::StoreStaticLocal,
            crate::ir::Op::InitStaticLocal,
        ] {
            assert!(super::super::capability::op_is_supported(op));
        }

        let mut module = Module::new(Target::wasm());
        module.add_function(function_with_a_static_local("a", "n", PhpType::Int));
        module.add_function(function_with_a_static_local("b", "n", PhpType::Int));

        let mut slots: StaticSlots = HashMap::new();
        let end = plan_static_local_slots(&module, &mut slots, 0);
        assert_eq!(slots.len(), 2, "one cell per declaring function");

        let a = slots[&static_local_slot_key("a", "n")].address;
        let b = slots[&static_local_slot_key("b", "n")].address;
        assert_ne!(a, b, "a() and b() must not share one counter");
        assert_eq!(
            b.abs_diff(a),
            STATIC_LOCAL_BYTES,
            "each cell is 16 bytes of value plus the initializer flag"
        );
        assert!(
            STATIC_LOCAL_INIT_OFFSET >= 16 && STATIC_LOCAL_INIT_OFFSET < STATIC_LOCAL_BYTES,
            "the flag lives past the value words and inside the cell"
        );
        assert!(
            a + STATIC_LOCAL_INIT_OFFSET != b && b + STATIC_LOCAL_INIT_OFFSET != a,
            "neither flag may land on the other cell's value"
        );
        assert_eq!(end, 2 * STATIC_LOCAL_BYTES, "the cursor covers both cells");
    }

    /// A static local's cell never overlaps a static PROPERTY's.
    ///
    /// They are two keyspaces in one map and one region: `static $n` inside a method of a class
    /// that also declares `public static $n` would collide if either side built its key by
    /// concatenation alone.
    #[test]
    fn static_locals_are_placed_after_the_static_properties() {
        let module = inheriting_module();
        let mut slots = plan_static_slots_for_audit(&module).expect("placement");
        let occupied: Vec<u32> = slots.values().map(|slot| slot.address).collect();
        let property_end = occupied.iter().copied().max().expect("one property") + 16;

        let mut with_local = Module::new(Target::wasm());
        with_local.class_infos = module.class_infos.clone();
        with_local.add_function(function_with_a_static_local("Parent::bump", "n", PhpType::Int));
        plan_static_local_slots(&with_local, &mut slots, property_end);

        let local = slots[&static_local_slot_key("Parent::bump", "n")].address;
        assert!(
            !occupied.contains(&local),
            "the static local took a static property's address"
        );
        assert!(
            local >= property_end,
            "the static local must be placed past the properties"
        );
    }
}
