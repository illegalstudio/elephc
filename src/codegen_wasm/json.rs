//! Purpose:
//! `json_encode` for the wasm32-wasi backend: scalars, arrays, hashes, and objects.
//!
//! Called from:
//! - `plan::lower_module` — `emit_json_property_table` lays out the per-class public-property
//!   metadata, and `emit_json_runtime` adds the encoder, both only for a module that calls
//!   `json_encode`.
//! - `builtins` — the shape check and the lowering.
//!
//! Key details:
//! - The encoder appends into ONE growable buffer held in globals rather than concatenating
//!   returned strings, so encoding a nested structure costs amortised appends instead of a copy
//!   per level.
//! - An object is encoded the way php does it: a class implementing `JsonSerializable` has its
//!   `jsonSerialize()` called and the RESULT encoded; any other class has its PUBLIC properties
//!   walked in declaration order. Both need per-class metadata, which is laid out here in the
//!   same shape `objects::emit_gc_desc_table` uses — a pointer table indexed by `class_id`.
//! - Strings are escaped with php's DEFAULT flags: `"` `\` `/` and the control characters get
//!   their short forms, everything else below 0x20 becomes `\u00XX`, and a non-ASCII sequence is
//!   decoded from UTF-8 and re-emitted as `\uXXXX` — with a surrogate pair above U+FFFF, which
//!   is what php produces without `JSON_UNESCAPED_UNICODE`.
//! - A FLOAT is written with serialize_precision -1 (`__rt_ftoa_shortest`); INF and NAN have no
//!   JSON form and abandon the encode, as do invalid UTF-8, a resource, a pure enum, an
//!   unencodable property slot, and nesting beyond php's default depth of 512.
//! - A backed enum encodes as its `value`; dynamic properties follow the declared ones.

use std::collections::{HashMap, HashSet};

use crate::ir::Module;
use crate::types::{ClassInfo, PhpType};

use super::wat::{DataSegment, Global, ValType, WatModule};

/// One public property, as the encoder reads it.
struct JsonProperty {
    name: String,
    /// Byte offset of the property's 16-byte slot inside the object payload.
    offset: u32,
    /// The runtime tag the slot's bytes carry, matching `objects::gc_desc_tag`.
    tag: u32,
}

/// The runtime tag for a property type, mirroring `objects::gc_desc_tag` — the same numbering a
/// Mixed cell uses, so a slot can be boxed by pairing its bytes with this tag.
fn json_property_tag(ty: &PhpType) -> Option<u32> {
    match ty.codegen_repr() {
        PhpType::Int => Some(0),
        PhpType::Str => Some(1),
        PhpType::Float => Some(2),
        PhpType::Bool | PhpType::False => Some(3),
        PhpType::Array(_) => Some(4),
        PhpType::AssocArray { .. } => Some(5),
        PhpType::Object(_) => Some(6),
        PhpType::Mixed | PhpType::Union(_) => Some(7),
        // Anything else has no JSON form the audit is willing to claim.
        _ => None,
    }
}

/// Row flag in the high bits of a class's entry count: a BACKED enum, encoded as its `value`.
const ROW_BACKED_ENUM: u32 = 0x8000_0000;
/// Row flag: a PURE enum, which php refuses to encode, so the whole call answers `false`.
const ROW_PURE_ENUM: u32 = 0x4000_0000;
/// Property tag of a slot whose type has no JSON form: reaching it abandons the encode.
const TAG_UNENCODABLE: u32 = 255;

/// Every other class a value statically typed `class_name` can be at runtime: its descendants,
/// or for an interface the classes implementing it.
///
/// The property table is indexed by the RUNTIME class id, so a `B $b` parameter holding a `D`
/// walks `D`'s row; the audit has to have proven that row, not just `B`'s.
fn related_classes(module: &Module, class_name: &str) -> Vec<String> {
    let key = crate::names::php_symbol_key(class_name);
    let is_interface = crate::ir::class_relations::lookup_interface(module, class_name).is_some();
    let mut related = Vec::new();
    for (name, info) in &module.class_infos {
        if crate::names::php_symbol_key(name) == key {
            continue;
        }
        let mut hit = is_interface
            && crate::ir::class_relations::class_implements_interface(module, name, class_name);
        let mut parent = info.parent.clone();
        for _ in 0..module.class_infos.len() {
            if hit {
                break;
            }
            let Some(next) = parent.clone() else { break };
            if crate::names::php_symbol_key(&next) == key {
                hit = true;
            }
            parent = module.class_infos.get(&next).and_then(|info| info.parent.clone());
        }
        if hit {
            related.push(name.clone());
        }
    }
    related.sort();
    related
}

/// Whether a class, and every class a value of its type can be at runtime, can be encoded.
///
/// Walks nested classes with a visited set, so a self-referential type is answered rather than
/// followed forever.
fn class_is_encodable(module: &Module, class_name: &str, seen: &mut HashSet<String>) -> bool {
    if !seen.insert(crate::names::php_symbol_key(class_name)) {
        return true;
    }
    if !exact_class_is_encodable(module, class_name, seen) {
        return false;
    }
    related_classes(module, class_name)
        .iter()
        .all(|related| {
            !seen.insert(crate::names::php_symbol_key(related))
                || exact_class_is_encodable(module, related, seen)
        })
}

/// The per-class half of `class_is_encodable`: this class's own public properties.
///
/// A typed property that may still be uninitialized is refused by name: php skips it, and this
/// target's zeroed slot is indistinguishable from a real `0`, so encoding it would invent a
/// member.
fn exact_class_is_encodable(module: &Module, class_name: &str, seen: &mut HashSet<String>) -> bool {
    let Some(info) = crate::ir::class_relations::lookup_class(module, class_name) else {
        return false;
    };
    // A `JsonSerializable` answers through its own method, so its properties are read only when
    // that method hands the object (or another one) back — `return $this` — and then php walks
    // them like any other object. The method body is not analysed, so a class that could expose a
    // possibly-uninitialized property that way is refused by name here too.
    if implements_json_serializable(module, class_name) {
        return jsonserialize_target(module, class_name).is_some()
            && !has_possibly_uninitialized_public_property(module, class_name, info);
    }
    public_properties(info).iter().all(|(name, ty)| {
        if super::capability::class_property_may_be_uninitialized(module, class_name, name) {
            return false;
        }
        match json_property_tag(ty) {
            None => false,
            Some(_) => match ty.codegen_repr() {
                PhpType::Object(nested) => class_is_encodable(module, &nested, seen),
                _ => true,
            },
        }
    })
}

/// Whether some public property of `class_name` may still be uninitialized when an outside walk
/// reads it: php skips such a property, while this target's zeroed slot reads as a real `0`.
fn has_possibly_uninitialized_public_property(
    module: &Module,
    class_name: &str,
    info: &ClassInfo,
) -> bool {
    public_properties(info).iter().any(|(name, _)| {
        super::capability::class_property_may_be_uninitialized(module, class_name, name)
    })
}

/// Whether any class in the module can hold a public property uninitialized.
///
/// An object reaching `json_encode` through a `mixed` value has no static class, so the encoder
/// walks whatever class it turns out to be; without a known class the audit proves the property
/// for the whole class set instead. Enums are not asked: their
/// properties are always initialized.
fn any_class_has_possibly_uninitialized_public_property(module: &Module) -> bool {
    module.class_infos.iter().any(|(name, info)| {
        !module.enum_infos.contains_key(name)
            && has_possibly_uninitialized_public_property(module, name, info)
    })
}

/// Whether `php` and everything it can contain has a JSON form this encoder produces.
pub(super) fn type_is_encodable(module: &Module, php: &PhpType) -> bool {
    let mut seen = HashSet::new();
    type_is_encodable_inner(module, php, &mut seen)
}

/// The recursive half of `type_is_encodable`.
fn type_is_encodable_inner(module: &Module, php: &PhpType, seen: &mut HashSet<String>) -> bool {
    match php.codegen_repr() {
        // A float is encodable now that the shortest-round-trip printer exists: php writes it
        // with serialize_precision = -1, which `__rt_ftoa_shortest` reproduces.
        PhpType::Int
        | PhpType::Str
        | PhpType::Bool
        | PhpType::False
        | PhpType::Float
        | PhpType::Void => true,
        PhpType::Array(element) => type_is_encodable_inner(module, &element, seen),
        // A hash key is written as a STRING whichever kind it is — a JSON object has no other —
        // so a `mixed` key needs no proof beyond the two tags a php array key can carry.
        PhpType::AssocArray { key, value } => {
            matches!(key.codegen_repr(), PhpType::Int | PhpType::Str | PhpType::Mixed)
                && type_is_encodable_inner(module, &value, seen)
        }
        // A bare `mixed` is encodable now that every runtime tag has an answer: the scalars and
        // containers walk as themselves, a closure writes `{}`, and a RESOURCE — the one thing
        // JSON cannot express — abandons the encode and answers `false`, which is what php does.
        PhpType::Mixed => !any_class_has_possibly_uninitialized_public_property(module),
        PhpType::Object(class_name) => class_is_encodable(module, &class_name, seen),
        _ => false,
    }
}

/// The public properties of a class, in declaration order.
fn public_properties(info: &ClassInfo) -> Vec<(String, PhpType)> {
    info.properties
        .iter()
        .filter(|(name, _)| {
            matches!(
                info.property_visibilities.get(name),
                Some(crate::parser::ast::Visibility::Public) | None
            )
        })
        .cloned()
        .collect()
}

/// Whether a class implements `JsonSerializable`, following its ancestors.
fn implements_json_serializable(module: &Module, class_name: &str) -> bool {
    crate::ir::class_relations::class_implements_interface(module, class_name, "JsonSerializable")
}

/// The compiled `jsonSerialize` body for a class, and whether it dispatches virtually.
fn jsonserialize_target(module: &Module, class_name: &str) -> Option<(String, bool)> {
    let key = crate::names::php_symbol_key("jsonSerialize");
    let info = crate::ir::class_relations::lookup_class(module, class_name)?;
    let impl_class = info
        .method_impl_classes
        .get(&key)
        .cloned()
        .unwrap_or_else(|| class_name.to_string());
    let want = crate::names::php_symbol_key(&format!("{impl_class}::jsonSerialize"));
    let body = module
        .class_methods
        .iter()
        .find(|f| crate::names::php_symbol_key(&f.name) == want)?;
    let dynamic = info.vtable_slots.contains_key(&key) && !info.final_methods.contains(&key);
    Some((body.name.clone(), dynamic))
}

/// The encoder's row for one class: the properties it writes and the enum flag bits.
///
/// A backed enum writes only its `value`, a pure enum writes nothing and flags the failure, and
/// a property whose type has no JSON form keeps its entry with `TAG_UNENCODABLE` rather than
/// vanishing, so the encode fails instead of silently dropping a member.
fn json_row(module: &Module, class_name: &str, ci: &ClassInfo) -> (Vec<JsonProperty>, u32) {
    let all = public_properties(ci);
    let (selected, flags) = match module.enum_infos.get(class_name) {
        Some(info) if info.backing_type.is_some() => (
            all.into_iter().filter(|(name, _)| name == "value").collect::<Vec<_>>(),
            ROW_BACKED_ENUM,
        ),
        Some(_) => (Vec::new(), ROW_PURE_ENUM),
        None => (all, 0),
    };
    let properties = selected
        .into_iter()
        .filter_map(|(property, ty)| {
            let index = ci.properties.iter().position(|(p, _)| *p == property)?;
            let offset = ci
                .property_offsets
                .get(&property)
                .copied()
                .unwrap_or(8 + index * 16) as u32;
            Some(JsonProperty {
                name: property,
                offset,
                tag: json_property_tag(&ty).unwrap_or(TAG_UNENCODABLE),
            })
        })
        .collect();
    (properties, flags)
}

/// Lays out the per-class public-property metadata and declares its globals.
///
/// Same shape as the gc-descriptor table: one row per class id, reached through a pointer table,
/// so the encoder resolves a class in two loads. Returns the new data cursor.
pub(super) fn emit_json_property_table(
    wm: &mut WatModule,
    module: &Module,
    mut cursor: u32,
) -> u32 {
    cursor = (cursor + 3) & !3;
    let class_infos = &module.class_infos;
    if class_infos.is_empty() {
        for name in ["__json_prop_ptrs", "__json_prop_counts", "__json_prop_count", "__json_dyn_offs"] {
            wm.add_global(Global {
                name: name.to_string(),
                ty: ValType::I32,
                mutable: false,
                init: 0,
            });
        }
        return cursor;
    }
    let mut ordered: Vec<_> = class_infos.iter().collect();
    ordered.sort_by(|(left, left_ci), (right, right_ci)| {
        left_ci.class_id.cmp(&right_ci.class_id).then_with(|| left.cmp(right))
    });
    let max_id = ordered.last().map(|(_, ci)| ci.class_id).unwrap_or(0);
    let count = max_id + 1;
    let mut id_to_name: HashMap<u64, &str> = HashMap::new();
    for (name, ci) in &ordered {
        id_to_name.entry(ci.class_id).or_insert(name.as_str());
    }

    // The property NAMES, laid out once each and shared by every row that mentions them.
    let mut name_offsets: HashMap<String, (u32, u32)> = HashMap::new();
    for (_, ci) in &ordered {
        for (name, _) in public_properties(ci) {
            if !name_offsets.contains_key(&name) {
                let bytes = name.as_bytes().to_vec();
                let len = bytes.len() as u32;
                wm.add_data(DataSegment { offset: cursor, bytes });
                name_offsets.insert(name.clone(), (cursor, len));
                cursor += len;
            }
        }
    }

    // One 16-byte entry per public property: name pointer, name length, slot offset, tag.
    cursor = (cursor + 3) & !3;
    let mut row_off: HashMap<u64, u32> = HashMap::new();
    let mut row_len: HashMap<u64, u32> = HashMap::new();
    let mut dyn_offsets: HashMap<u64, u32> = HashMap::new();
    for cid in 0..=max_id {
        let Some(name) = id_to_name.get(&cid) else {
            continue;
        };
        let Some(ci) = class_infos.get(*name) else {
            continue;
        };
        let (properties, flags) = json_row(module, name, ci);
        let mut bytes = Vec::with_capacity(properties.len() * 16);
        for property in &properties {
            let (name_ptr, name_len) = name_offsets[&property.name];
            bytes.extend_from_slice(&name_ptr.to_le_bytes());
            bytes.extend_from_slice(&name_len.to_le_bytes());
            bytes.extend_from_slice(&property.offset.to_le_bytes());
            bytes.extend_from_slice(&property.tag.to_le_bytes());
        }
        row_len.insert(cid, properties.len() as u32 | flags);
        if let Some(offset) = super::objects::dynamic_property_hash_offset_for_class(ci, name, "") {
            dyn_offsets.insert(cid, offset as u32);
        }
        if bytes.is_empty() {
            row_off.insert(cid, cursor);
            continue;
        }
        row_off.insert(cid, cursor);
        let written = bytes.len() as u32;
        wm.add_data(DataSegment { offset: cursor, bytes });
        cursor += written;
    }

    cursor = (cursor + 3) & !3;
    let ptrs_off = cursor;
    let mut ptrs = Vec::with_capacity(count as usize * 4);
    for cid in 0..=max_id {
        ptrs.extend_from_slice(&row_off.get(&cid).copied().unwrap_or(0).to_le_bytes());
    }
    wm.add_data(DataSegment { offset: ptrs_off, bytes: ptrs });
    cursor += count as u32 * 4;

    let counts_off = cursor;
    let mut counts = Vec::with_capacity(count as usize * 4);
    for cid in 0..=max_id {
        counts.extend_from_slice(&row_len.get(&cid).copied().unwrap_or(0).to_le_bytes());
    }
    wm.add_data(DataSegment { offset: counts_off, bytes: counts });
    cursor += count as u32 * 4;

    let dyn_off = cursor;
    let mut dyns = Vec::with_capacity(count as usize * 4);
    for cid in 0..=max_id {
        dyns.extend_from_slice(&dyn_offsets.get(&cid).copied().unwrap_or(0).to_le_bytes());
    }
    wm.add_data(DataSegment { offset: dyn_off, bytes: dyns });
    cursor += count as u32 * 4;

    wm.add_global(Global {
        name: "__json_dyn_offs".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: dyn_off as i64,
    });
    wm.add_global(Global {
        name: "__json_prop_ptrs".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: ptrs_off as i64,
    });
    wm.add_global(Global {
        name: "__json_prop_counts".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: counts_off as i64,
    });
    wm.add_global(Global {
        name: "__json_prop_count".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: count as i64,
    });
    cursor
}

/// The `(class_id, callee_symbol)` arms of the `jsonSerialize` ladder.
///
/// The dispatch decision is the one an ordinary method call makes — a method with a vtable slot
/// that is not final goes through its introducer's stub, so an overriding subclass still wins —
/// because a different rule here would answer with a different body than `$obj->jsonSerialize()`
/// would.
pub(super) fn jsonserialize_ladder(module: &Module) -> Vec<(u64, String)> {
    let key = crate::names::php_symbol_key("jsonSerialize");
    let mut arms = Vec::new();
    let mut classes: Vec<_> = module.class_infos.iter().collect();
    classes.sort_by(|left, right| left.1.class_id.cmp(&right.1.class_id));
    for (class_name, info) in classes {
        if !implements_json_serializable(module, class_name) {
            continue;
        }
        let Some((body_name, dynamic)) = jsonserialize_target(module, class_name) else {
            continue;
        };
        let symbol = if dynamic {
            let introducer = info
                .method_declaring_classes
                .get(&key)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            super::symbols::method_dispatch_symbol(&introducer, &key)
        } else {
            super::symbols::method_symbol(&body_name)
        };
        arms.push((info.class_id, symbol));
    }
    arms
}

/// Whether this module calls `json_encode`.
pub(super) fn module_uses_json_encode(module: &Module) -> bool {
    use crate::ir::{Immediate, RuntimeCallTarget, RuntimeFnId};
    super::function::all_functions(module)
        .any(|function| {
            function.instructions.iter().any(|inst| {
                let Some(Immediate::RuntimeCall(target)) = inst.immediate.as_ref() else {
                    return false;
                };
                matches!(
                    target,
                    RuntimeCallTarget::Function(RuntimeFnId::JsonEncode)
                        | RuntimeCallTarget::ProfiledFunction {
                            target: RuntimeFnId::JsonEncode,
                            ..
                        }
                )
            })
        })
}

/// Adds the encoder.
///
/// `serialize_targets` are the `(class_id, callee_symbol)` arms of the `jsonSerialize` ladder,
/// which is what lets an object answer through its own method. The ladder is a class-id if-chain
/// for the same reason every other dynamic dispatch here is one: this backend has no `funcref`
/// table to index.
pub(super) fn emit_json_runtime(wm: &mut WatModule, serialize_targets: &[(u64, String)]) {
    for (name, init) in [("__json_buf", 0), ("__json_len", 0), ("__json_cap", 0), ("__json_failed", 0), ("__json_depth", 0)] {
        wm.add_global(Global {
            name: name.to_string(),
            ty: ValType::I32,
            mutable: true,
            init,
        });
    }
    wm.add_raw_func(RT_JSON_PUT);
    wm.add_raw_func(RT_JSON_PUT_BYTE);
    wm.add_raw_func(RT_JSON_PUT_HEX4);
    wm.add_raw_func(RT_JSON_PUT_STRING);
    wm.add_raw_func(&rt_json_serialize_call(serialize_targets));
    wm.add_raw_func(RT_JSON_SLOT);
    wm.add_raw_func(RT_JSON_DYN_PROPS);
    wm.add_raw_func(RT_JSON_OBJECT);
    wm.add_raw_func(RT_JSON_HASH_IS_LIST);
    wm.add_raw_func(RT_JSON_VALUE);
    wm.add_raw_func(RT_JSON_ENCODE);
}

/// `__rt_json_put`: appends bytes to the encode buffer, doubling it when full.
const RT_JSON_PUT: &str = r#"(func $__rt_json_put (param $ptr i32) (param $len i32)
  (local $need i32) (local $grown i32) (local $i i32)
  (if (i32.eqz (local.get $len))
    (then (return)))
  (local.set $need (i32.add (global.get $__json_len) (local.get $len)))
  (if (i32.gt_u (local.get $need) (global.get $__json_cap))
    (then
      (global.set $__json_cap (i32.mul (i32.add (global.get $__json_cap) (i32.const 64)) (i32.const 2)))
      (if (i32.gt_u (local.get $need) (global.get $__json_cap))
        (then (global.set $__json_cap (local.get $need))))
      (local.set $grown (call $__rt_heap_alloc (global.get $__json_cap)))
      (block $copied (loop $byte
        (br_if $copied (i32.ge_u (local.get $i) (global.get $__json_len)))
        (i32.store8 (i32.add (local.get $grown) (local.get $i))
                    (i32.load8_u (i32.add (global.get $__json_buf) (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $byte)))
      (if (global.get $__json_buf)
        (then (call $__rt_heap_free (global.get $__json_buf))))
      (global.set $__json_buf (local.get $grown))))
  (local.set $i (i32.const 0))
  (block $written (loop $byte
    (br_if $written (i32.ge_u (local.get $i) (local.get $len)))
    (i32.store8 (i32.add (global.get $__json_buf) (i32.add (global.get $__json_len) (local.get $i)))
                (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $byte)))
  (global.set $__json_len (local.get $need)))
"#;

/// `__rt_json_put_byte`: one literal byte, written through the float scratch.
const RT_JSON_PUT_BYTE: &str = r#"(func $__rt_json_put_byte (param $byte i32)
  (i32.store8 (i32.add (global.get $__float_scratch) (i32.const 8300)) (local.get $byte))
  (call $__rt_json_put (i32.add (global.get $__float_scratch) (i32.const 8300)) (i32.const 1)))
"#;

/// `__rt_json_put_hex4`: `\uXXXX` for one code unit, lower-case hex as php writes it.
const RT_JSON_PUT_HEX4: &str = r#"(func $__rt_json_put_hex4 (param $unit i32)
  (local $i i32) (local $nibble i32)
  (call $__rt_json_put_byte (i32.const 92))                        ;; '\'
  (call $__rt_json_put_byte (i32.const 117))                       ;; 'u'
  (local.set $i (i32.const 3))
  (block $done (loop $digit
    (br_if $done (i32.lt_s (local.get $i) (i32.const 0)))
    (local.set $nibble (i32.and (i32.shr_u (local.get $unit) (i32.mul (local.get $i) (i32.const 4))) (i32.const 15)))
    (call $__rt_json_put_byte
      (select (i32.add (local.get $nibble) (i32.const 87))          ;; 'a'..'f'
              (i32.add (local.get $nibble) (i32.const 48))          ;; '0'..'9'
              (i32.gt_u (local.get $nibble) (i32.const 9))))
    (local.set $i (i32.sub (local.get $i) (i32.const 1)))
    (br $digit))))
"#;

/// `__rt_json_put_string`: a quoted, escaped JSON string.
///
/// php's DEFAULT flags: the short forms for `"` `\` `/` and the five named control characters,
/// `\u00XX` for the rest below 0x20, and `\uXXXX` for everything above 0x7f — decoded from UTF-8,
/// with a surrogate pair beyond U+FFFF. Invalid UTF-8 (bad lead or continuation bytes, a truncated
/// sequence, an overlong form, a surrogate, a point above U+10FFFF) raises `__json_failed`, so the
/// whole `json_encode` answers `false` as php does without `JSON_INVALID_UTF8_*` flags.
const RT_JSON_PUT_STRING: &str = r#"(func $__rt_json_put_string (param $ptr i32) (param $len i64)
  (local $i i32) (local $n i32) (local $b i32) (local $cp i32) (local $extra i32) (local $j i32)
  (local $cont i32) (local $min i32)
  (local.set $n (i32.wrap_i64 (local.get $len)))
  (call $__rt_json_put_byte (i32.const 34))                        ;; '"'
  (block $done (loop $byte
    (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
    (local.set $b (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (block $emitted
      ;; The short forms, in the order php writes them.
      (if (i32.eq (local.get $b) (i32.const 34))                   ;; '"'
        (then (call $__rt_json_put_byte (i32.const 92))
              (call $__rt_json_put_byte (i32.const 34)) (br $emitted)))
      (if (i32.eq (local.get $b) (i32.const 92))                   ;; '\'
        (then (call $__rt_json_put_byte (i32.const 92))
              (call $__rt_json_put_byte (i32.const 92)) (br $emitted)))
      (if (i32.eq (local.get $b) (i32.const 47))                   ;; '/'
        (then (call $__rt_json_put_byte (i32.const 92))
              (call $__rt_json_put_byte (i32.const 47)) (br $emitted)))
      (if (i32.eq (local.get $b) (i32.const 8))                    ;; backspace
        (then (call $__rt_json_put_byte (i32.const 92))
              (call $__rt_json_put_byte (i32.const 98)) (br $emitted)))
      (if (i32.eq (local.get $b) (i32.const 12))                   ;; form feed
        (then (call $__rt_json_put_byte (i32.const 92))
              (call $__rt_json_put_byte (i32.const 102)) (br $emitted)))
      (if (i32.eq (local.get $b) (i32.const 10))                   ;; newline
        (then (call $__rt_json_put_byte (i32.const 92))
              (call $__rt_json_put_byte (i32.const 110)) (br $emitted)))
      (if (i32.eq (local.get $b) (i32.const 13))                   ;; carriage return
        (then (call $__rt_json_put_byte (i32.const 92))
              (call $__rt_json_put_byte (i32.const 114)) (br $emitted)))
      (if (i32.eq (local.get $b) (i32.const 9))                    ;; tab
        (then (call $__rt_json_put_byte (i32.const 92))
              (call $__rt_json_put_byte (i32.const 116)) (br $emitted)))
      (if (i32.lt_u (local.get $b) (i32.const 32))                 ;; other control byte
        (then (call $__rt_json_put_hex4 (local.get $b)) (br $emitted)))
      (if (i32.lt_u (local.get $b) (i32.const 128))                ;; plain ASCII
        (then (call $__rt_json_put_byte (local.get $b)) (br $emitted)))
      ;; A UTF-8 sequence, validated strictly the way php does: the length comes from the lead
      ;; byte (0xC0, 0xC1 and 0xF5.. never start one), the continuation bytes must all be
      ;; 10xxxxxx, and the decoded point must not be overlong, a surrogate or above U+10FFFF.
      ;; Anything else abandons the whole encode, which php answers with `false`
      ;; (JSON_ERROR_UTF8) — the same flag a resource raises. A valid sequence is re-emitted as
      ;; escaped UTF-16, a surrogate pair above U+FFFF.
      (local.set $extra (i32.const 0))
      (local.set $min (i32.const 0))
      (local.set $cp (i32.const 0))
      (if (i32.and (i32.ge_u (local.get $b) (i32.const 194)) (i32.le_u (local.get $b) (i32.const 223)))
        (then (local.set $extra (i32.const 1)) (local.set $min (i32.const 128))
              (local.set $cp (i32.and (local.get $b) (i32.const 31)))))
      (if (i32.eq (i32.and (local.get $b) (i32.const 240)) (i32.const 224))
        (then (local.set $extra (i32.const 2)) (local.set $min (i32.const 2048))
              (local.set $cp (i32.and (local.get $b) (i32.const 15)))))
      (if (i32.and (i32.ge_u (local.get $b) (i32.const 240)) (i32.le_u (local.get $b) (i32.const 244)))
        (then (local.set $extra (i32.const 3)) (local.set $min (i32.const 65536))
              (local.set $cp (i32.and (local.get $b) (i32.const 7)))))
      (if (i32.or (i32.eqz (local.get $extra))                     ;; a stray continuation, 0xC0/0xC1, 0xF5..
                  (i32.gt_u (i32.add (i32.add (local.get $i) (local.get $extra)) (i32.const 1)) (local.get $n)))
        (then (global.set $__json_failed (i32.const 1)) (return)))
      (local.set $j (i32.const 0))
      (block $gathered (loop $cbyte
        (br_if $gathered (i32.ge_u (local.get $j) (local.get $extra)))
        (local.set $cont (i32.load8_u
          (i32.add (local.get $ptr) (i32.add (i32.add (local.get $i) (local.get $j)) (i32.const 1)))))
        (if (i32.ne (i32.and (local.get $cont) (i32.const 192)) (i32.const 128))
          (then (global.set $__json_failed (i32.const 1)) (return)))
        (local.set $cp (i32.or (i32.shl (local.get $cp) (i32.const 6))
                               (i32.and (local.get $cont) (i32.const 63))))
        (local.set $j (i32.add (local.get $j) (i32.const 1)))
        (br $cbyte)))
      (if (i32.or (i32.lt_u (local.get $cp) (local.get $min))      ;; overlong
          (i32.or (i32.gt_u (local.get $cp) (i32.const 1114111))   ;; beyond U+10FFFF
                  (i32.and (i32.ge_u (local.get $cp) (i32.const 55296))
                           (i32.le_u (local.get $cp) (i32.const 57343)))))  ;; a surrogate
        (then (global.set $__json_failed (i32.const 1)) (return)))
      (local.set $i (i32.add (local.get $i) (local.get $extra)))
      (if (i32.gt_u (local.get $cp) (i32.const 65535))
        (then
          (local.set $cp (i32.sub (local.get $cp) (i32.const 65536)))
          (call $__rt_json_put_hex4
            (i32.add (i32.const 55296) (i32.shr_u (local.get $cp) (i32.const 10))))
          (call $__rt_json_put_hex4
            (i32.add (i32.const 56320) (i32.and (local.get $cp) (i32.const 1023)))))
        (else (call $__rt_json_put_hex4 (local.get $cp)))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $byte)))
  (call $__rt_json_put_byte (i32.const 34)))
"#;

/// `__rt_json_serialize_call`: dispatches an object to its own `jsonSerialize`.
///
/// Answers 0 for a class that does not implement `JsonSerializable`, which is how the value
/// encoder chooses between the method and the property walk.
fn rt_json_serialize_call(targets: &[(u64, String)]) -> String {
    let mut wat = String::from(
        "(func $__rt_json_serialize_call (param $obj i32) (result i32)\n  (local $cid i32)\n",
    );
    if targets.is_empty() {
        wat.push_str("  (i32.const 0))\n");
        return wat;
    }
    wat.push_str("  (local.set $cid (i32.wrap_i64 (i64.load (local.get $obj))))\n");
    for (class_id, callee) in targets {
        wat.push_str(&format!(
            "  (if (i32.eq (local.get $cid) (i32.const {class_id}))\n    (then (return (call ${callee} (local.get $obj)))))\n"
        ));
    }
    wat.push_str("  (i32.const 0))\n");
    wat
}

/// `__rt_json_slot`: one property slot, encoded according to the tag its class row records.
///
/// A boxed slot (tag 7) already holds a cell and is read directly; any other tag pairs the
/// slot's two words with it and boxes them for the value encoder. `TAG_UNENCODABLE` is a slot
/// whose declared type has no JSON form, which abandons the encode rather than dropping it.
const RT_JSON_SLOT: &str = r#"(func $__rt_json_slot (param $slot i32) (param $tag i32)
  (local $cell i32)
  (if (i32.eq (local.get $tag) (i32.const 255))
    (then (global.set $__json_failed (i32.const 1)) (return)))
  (if (i32.eq (local.get $tag) (i32.const 7))
    (then
      (call $__rt_json_value (i32.wrap_i64 (i64.load (local.get $slot))))
      (return)))
  (local.set $cell (call $__rt_mixed_from_value (i64.extend_i32_u (local.get $tag))
    (i64.load (local.get $slot)) (i64.load offset=8 (local.get $slot))))
  (call $__rt_json_value (local.get $cell))
  (call $__rt_decref_any (local.get $cell)))
"#;

/// `__rt_json_dyn_props`: the dynamic properties of an object, after its declared ones.
///
/// The hash holds boxed cells in insertion order, which is the order php writes them. `$wrote`
/// says whether the declared properties already wrote a member, so the first dynamic one knows
/// whether it needs a leading comma.
const RT_JSON_DYN_PROPS: &str = r#"(func $__rt_json_dyn_props (param $src i32) (param $wrote i32)
  (local $cursor i64) (local $more i64) (local $next i64) (local $kcell i32) (local $vcell i32)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $tptr i32) (local $tlen i32)
  (local.set $cursor (i64.const -2))
  (block $done (loop $entry
    (br_if $done (global.get $__json_failed))
    (call $__rt_mixed_iter_next (local.get $src) (local.get $cursor) (i32.const 1))
    (local.set $more)
    (local.set $next)
    (br_if $done (i64.eqz (local.get $more)))
    (local.set $cursor (local.get $next))
    (if (local.get $wrote)
      (then (call $__rt_json_put_byte (i32.const 44))))
    (local.set $wrote (i32.const 1))
    (local.set $kcell (call $__rt_mixed_iter_key (local.get $src) (local.get $cursor) (i32.const 1)))
    (call $__rt_mixed_unbox (local.get $kcell))
    (local.set $hi)
    (local.set $lo)
    (local.set $tag)
    (if (i64.eq (local.get $tag) (i64.const 1))
      (then (call $__rt_json_put_string (i32.wrap_i64 (local.get $lo)) (local.get $hi)))
      (else
        (call $__rt_itoa (local.get $lo) (global.get $__float_scratch))
        (local.set $tlen)
        (local.set $tptr)
        (call $__rt_json_put_string (local.get $tptr) (i64.extend_i32_u (local.get $tlen)))))
    (call $__rt_decref_any (local.get $kcell))
    (call $__rt_json_put_byte (i32.const 58))                      ;; ':'
    (local.set $vcell (call $__rt_mixed_iter_value (local.get $src) (local.get $cursor) (i32.const 1)))
    (call $__rt_json_value (local.get $vcell))
    (call $__rt_decref_any (local.get $vcell))
    (br $entry))))
"#;

/// `__rt_json_object`: an object as a JSON object.
///
/// A `JsonSerializable` answers through its method and the RESULT is encoded. A backed enum
/// encodes as its `value` and a pure enum abandons the encode, both read off the flag bits in
/// the row's count word. Any other class has its public properties walked in declaration
/// order, then its dynamic properties in insertion order. Nesting deeper than php's default
/// depth of 512 abandons the encode too, which is also what ends a cyclic object graph.
const RT_JSON_OBJECT: &str = r#"(func $__rt_json_object (param $obj i32)
  (local $cid i32) (local $row i32) (local $n i32) (local $i i32) (local $entry i32)
  (local $res i32) (local $flags i32) (local $dyn i32) (local $hash i32)
  (local $rtag i64) (local $rlo i64) (local $rhi i64)
  (local.set $res (call $__rt_json_serialize_call (local.get $obj)))
  (if (local.get $res)
    (then
      (call $__rt_mixed_unbox (local.get $res))
      (local.set $rhi)
      (local.set $rlo)
      (local.set $rtag)
      ;; `return $this;` is php's way of asking for the property walk: encoding the answer
      ;; would call the method again, forever. Any other answer is encoded, and counts as one
      ;; level of nesting so that two objects answering each other also end at the depth limit.
      (if (i32.and (i64.eq (local.get $rtag) (i64.const 6))
                   (i64.eq (local.get $rlo) (i64.extend_i32_u (local.get $obj))))
        (then (call $__rt_decref_any (local.get $res)))
        (else
          (global.set $__json_depth (i32.add (global.get $__json_depth) (i32.const 1)))
          (if (i32.gt_u (global.get $__json_depth) (i32.const 512))
            (then
              (global.set $__json_failed (i32.const 1))
              (call $__rt_decref_any (local.get $res))
              (return)))
          (call $__rt_json_value (local.get $res))
          (global.set $__json_depth (i32.sub (global.get $__json_depth) (i32.const 1)))
          (call $__rt_decref_any (local.get $res))
          (return)))))
  (local.set $cid (i32.wrap_i64 (i64.load (local.get $obj))))
  (if (i32.lt_u (local.get $cid) (global.get $__json_prop_count))
    (then
      (local.set $row (i32.load (i32.add (global.get $__json_prop_ptrs) (i32.mul (local.get $cid) (i32.const 4)))))
      (local.set $n (i32.load (i32.add (global.get $__json_prop_counts) (i32.mul (local.get $cid) (i32.const 4)))))
      (local.set $dyn (i32.load (i32.add (global.get $__json_dyn_offs) (i32.mul (local.get $cid) (i32.const 4)))))
      (local.set $flags (i32.and (local.get $n) (i32.const -1073741824)))
      (local.set $n (i32.and (local.get $n) (i32.const 1073741823)))))
  (if (i32.and (local.get $flags) (i32.const -2147483648))         ;; backed enum: its value
    (then
      (call $__rt_json_slot (i32.add (local.get $obj) (i32.load offset=8 (local.get $row)))
                            (i32.load offset=12 (local.get $row)))
      (return)))
  (if (i32.and (local.get $flags) (i32.const 1073741824))          ;; pure enum: no JSON form
    (then (global.set $__json_failed (i32.const 1)) (return)))
  (global.set $__json_depth (i32.add (global.get $__json_depth) (i32.const 1)))
  (if (i32.gt_u (global.get $__json_depth) (i32.const 512))
    (then (global.set $__json_failed (i32.const 1)) (return)))
  (call $__rt_json_put_byte (i32.const 123))                       ;; '{'
  (block $walked (loop $property
    (br_if $walked (i32.ge_u (local.get $i) (local.get $n)))
    (br_if $walked (global.get $__json_failed))
    (if (local.get $i)
      (then (call $__rt_json_put_byte (i32.const 44))))            ;; ','
    (local.set $entry (i32.add (local.get $row) (i32.mul (local.get $i) (i32.const 16))))
    (call $__rt_json_put_string (i32.load (local.get $entry))
                                (i64.extend_i32_u (i32.load offset=4 (local.get $entry))))
    (call $__rt_json_put_byte (i32.const 58))                      ;; ':'
    (call $__rt_json_slot (i32.add (local.get $obj) (i32.load offset=8 (local.get $entry)))
                          (i32.load offset=12 (local.get $entry)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $property)))
  (if (local.get $dyn)
    (then
      (local.set $hash (i32.load (i32.add (local.get $obj) (local.get $dyn))))
      (if (local.get $hash)
        (then (call $__rt_json_dyn_props (local.get $hash) (i32.ne (local.get $n) (i32.const 0)))))))
  (call $__rt_json_put_byte (i32.const 125))                       ;; '}'
  (global.set $__json_depth (i32.sub (global.get $__json_depth) (i32.const 1))))
"#;

/// `__rt_json_value`: one value, appended to the encode buffer.
///
/// An INDEXED array becomes a JSON array and a HASH a JSON object, which is php's rule: the
/// storage decides, and a hash key is written as a string whichever kind it is.
/// `__rt_json_hash_is_list`: whether a keyed array's keys are `0..n-1`, in that order.
///
/// php's `json_encode` does not ask whether a value is a HASH, it asks whether the keys it
/// holds form a list — `[0 => "a", 1 => "b"]` encodes as `["a","b"]` and only a gap or an
/// out-of-order key makes it an object. Measured on php-src 8.5.10, where `[1 => "a", 0 => "b"]`
/// is `{"1":"a","0":"b"}`: the ORDER counts, not just the set of keys.
///
/// This target stores that array as a hash whatever its keys are, so the question has to be
/// asked at runtime. Without it every keyed array encoded as an object, which is how a
/// duplicate-free `array_unique` — whose keys are still `0..n-1` — came out `{"0":100,...}`
/// where php answers `[100,...]`.
const RT_JSON_HASH_IS_LIST: &str = r#"(func $__rt_json_hash_is_list (param $src i32) (result i32)
  (local $cursor i64) (local $more i64) (local $next i64) (local $kcell i32)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $n i64)
  (local.set $cursor (i64.const -2))
  (block $done (loop $entry
    (call $__rt_mixed_iter_next (local.get $src) (local.get $cursor) (i32.const 1))
    (local.set $more)
    (local.set $next)
    (br_if $done (i64.eqz (local.get $more)))
    (local.set $cursor (local.get $next))
    (local.set $kcell (call $__rt_mixed_iter_key (local.get $src) (local.get $cursor) (i32.const 1)))
    (call $__rt_mixed_unbox (local.get $kcell))
    (local.set $hi)
    (local.set $lo)
    (local.set $tag)
    (call $__rt_decref_any (local.get $kcell))
    (if (i64.ne (local.get $tag) (i64.const 0))
      (then (return (i32.const 0))))                               ;; a string key is never a list
    (if (i64.ne (local.get $lo) (local.get $n))
      (then (return (i32.const 0))))                               ;; a gap, or out of order
    (local.set $n (i64.add (local.get $n) (i64.const 1)))
    (br $entry)))
  (i32.const 1))
"#;

const RT_JSON_VALUE: &str = r#"(func $__rt_json_value (param $cell i32)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $tptr i32) (local $tlen i32)
  (local $src i32) (local $kind i32) (local $cursor i64) (local $more i64) (local $next i64)
  (local $kcell i32) (local $vcell i32) (local $first i32) (local $as_object i32)
  (if (global.get $__json_failed) (then (return)))                 ;; the encode is already abandoned
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eqz (local.get $tag))                                   ;; int
    (then
      (call $__rt_itoa (local.get $lo) (global.get $__float_scratch))
      (local.set $tlen)
      (local.set $tptr)
      (call $__rt_json_put (local.get $tptr) (local.get $tlen))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 1))                      ;; string
    (then
      (call $__rt_json_put_string (i32.wrap_i64 (local.get $lo)) (local.get $hi))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 3))                      ;; bool
    (then
      (if (i64.eqz (local.get $lo))
        (then (call $__rt_json_put (i32.add (global.get $__float_scratch) (i32.const 8310)) (i32.const 5)))
        (else (call $__rt_json_put (i32.add (global.get $__float_scratch) (i32.const 8320)) (i32.const 4))))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 2))                      ;; float
    (then
      ;; INF and NAN have no JSON form: php abandons the encode (JSON_ERROR_INF_OR_NAN).
      (if (i64.eq (i64.and (local.get $lo) (i64.const 9218868437227405312))
                  (i64.const 9218868437227405312))
        (then (global.set $__json_failed (i32.const 1)) (return)))
      ;; php writes a JSON float with serialize_precision = -1 — the fewest digits that read back
      ;; as the same double — and with a LOWER-CASE exponent, where its own text uses `E`.
      (call $__rt_ftoa_shortest (local.get $lo)
        (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80)
        (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 792)
        (i32.add (global.get $__float_scratch) (i32.const 4096)) (i32.const 101))
      (local.set $tlen)
      (local.set $tptr)
      (call $__rt_json_put (local.get $tptr) (local.get $tlen))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 6))                      ;; object
    (then
      (call $__rt_json_object (i32.wrap_i64 (local.get $lo)))
      (return)))
  (if (i32.or (i64.eq (local.get $tag) (i64.const 4))
              (i64.eq (local.get $tag) (i64.const 5)))
    (then
      (global.set $__json_depth (i32.add (global.get $__json_depth) (i32.const 1)))
      (if (i32.gt_u (global.get $__json_depth) (i32.const 512))
        (then (global.set $__json_failed (i32.const 1)) (return)))
      (local.set $src (i32.wrap_i64 (local.get $lo)))
      (local.set $kind (select (i32.const 1) (i32.const 0) (i64.eq (local.get $tag) (i64.const 5))))
      ;; `$kind` picks the ITERATOR; `$as_object` picks the brackets and whether keys are
      ;; written. They used to be one variable, which is what made every keyed array an
      ;; object — php decides by the keys, not by the storage.
      (local.set $as_object (i32.const 0))
      (if (i32.eq (local.get $kind) (i32.const 1)) (then
        (local.set $as_object (i32.eqz (call $__rt_json_hash_is_list (local.get $src))))))
      (call $__rt_json_put_byte (select (i32.const 123) (i32.const 91) (local.get $as_object)))
      (local.set $cursor (select (i64.const -2) (i64.const -1) (i32.eq (local.get $kind) (i32.const 1))))
      (block $walked (loop $entry
        (call $__rt_mixed_iter_next (local.get $src) (local.get $cursor) (local.get $kind))
        (local.set $more)
        (local.set $next)
        (br_if $walked (i64.eqz (local.get $more)))
        (local.set $cursor (local.get $next))
        (if (local.get $first)
          (then (call $__rt_json_put_byte (i32.const 44))))
        (local.set $first (i32.const 1))
        (if (i32.eq (local.get $as_object) (i32.const 1))
          (then
            (local.set $kcell (call $__rt_mixed_iter_key (local.get $src) (local.get $cursor) (local.get $kind)))
            (call $__rt_mixed_unbox (local.get $kcell))
            (local.set $hi)
            (local.set $lo)
            (local.set $tag)
            (if (i64.eq (local.get $tag) (i64.const 1))
              (then (call $__rt_json_put_string (i32.wrap_i64 (local.get $lo)) (local.get $hi)))
              (else
                ;; An integer key is written as a STRING: a JSON object has no other kind.
                (call $__rt_itoa (local.get $lo) (global.get $__float_scratch))
                (local.set $tlen)
                (local.set $tptr)
                (call $__rt_json_put_string (local.get $tptr) (i64.extend_i32_u (local.get $tlen)))))
            (call $__rt_decref_any (local.get $kcell))
            (call $__rt_json_put_byte (i32.const 58))))
        (local.set $vcell (call $__rt_mixed_iter_value (local.get $src) (local.get $cursor) (local.get $kind)))
        (call $__rt_json_value (local.get $vcell))
        (call $__rt_decref_any (local.get $vcell))
        (br_if $walked (global.get $__json_failed))
        (br $entry)))
      (call $__rt_json_put_byte (select (i32.const 125) (i32.const 93) (local.get $as_object)))
      (global.set $__json_depth (i32.sub (global.get $__json_depth) (i32.const 1)))
      (return)))
  ;; A RESOURCE has no JSON form at all: php abandons the whole encode and answers `false` with
  ;; "Type is not supported", which is why this sets a flag rather than writing a placeholder.
  ;; A CLOSURE does have one — php walks its (empty) public properties and writes `{}`. Both
  ;; measured on 8.5.6.
  (if (i64.eq (local.get $tag) (i64.const 9))
    (then
      (global.set $__json_failed (i32.const 1))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 10))
    (then
      (call $__rt_json_put_byte (i32.const 123))                    ;; '{'
      (call $__rt_json_put_byte (i32.const 125))                    ;; '}'
      (return)))
  ;; null is what is left.
  (call $__rt_json_put (i32.add (global.get $__float_scratch) (i32.const 8330)) (i32.const 4)))
"#;

/// `__rt_json_encode`: the builtin, boxed as php's `string|false`.
///
/// The literals `true`, `false` and `null` are written into the scratch here rather than laid
/// out as data, because the encoder is the only reader and three fixed words do not justify
/// moving the whole region's offsets.
const RT_JSON_ENCODE: &str = r#"(func $__rt_json_encode (param $cell i32) (result i32)
  (local $out i32) (local $start i32) (local $outer_failed i32) (local $outer_depth i32)
  (i32.store8 offset=8310 (global.get $__float_scratch) (i32.const 102))   ;; 'f'
  (i32.store8 offset=8311 (global.get $__float_scratch) (i32.const 97))    ;; 'a'
  (i32.store8 offset=8312 (global.get $__float_scratch) (i32.const 108))   ;; 'l'
  (i32.store8 offset=8313 (global.get $__float_scratch) (i32.const 115))   ;; 's'
  (i32.store8 offset=8314 (global.get $__float_scratch) (i32.const 101))   ;; 'e'
  (i32.store8 offset=8320 (global.get $__float_scratch) (i32.const 116))   ;; 't'
  (i32.store8 offset=8321 (global.get $__float_scratch) (i32.const 114))   ;; 'r'
  (i32.store8 offset=8322 (global.get $__float_scratch) (i32.const 117))   ;; 'u'
  (i32.store8 offset=8323 (global.get $__float_scratch) (i32.const 101))   ;; 'e'
  (i32.store8 offset=8330 (global.get $__float_scratch) (i32.const 110))   ;; 'n'
  (i32.store8 offset=8331 (global.get $__float_scratch) (i32.const 117))   ;; 'u'
  (i32.store8 offset=8332 (global.get $__float_scratch) (i32.const 108))   ;; 'l'
  (i32.store8 offset=8333 (global.get $__float_scratch) (i32.const 108))   ;; 'l'
  ;; A `jsonSerialize()` may call `json_encode` itself, so this call owns only the bytes after
  ;; `$start` (0 when nothing is encoding) and restores the length and failure flag it found on
  ;; the way out; the buffer pointer is re-read after every growth, so an outer call's bytes
  ;; below `$start` survive a nested call that reallocates.
  (local.set $start (global.get $__json_len))
  (local.set $outer_failed (global.get $__json_failed))
  (local.set $outer_depth (global.get $__json_depth))
  (global.set $__json_failed (i32.const 0))
  (global.set $__json_depth (i32.const 0))                         ;; php counts depth per call
  (call $__rt_json_value (local.get $cell))
  (if (global.get $__json_failed)
    (then
      (global.set $__json_len (local.get $start))
      (global.set $__json_failed (local.get $outer_failed))
      (global.set $__json_depth (local.get $outer_depth))
      (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))  ;; php's `false`
  ;; `__rt_mixed_from_value` persists its own copy of a string, so the bytes go in straight from
  ;; the buffer: persisting here first stranded one block per call.
  (local.set $out (call $__rt_mixed_from_value (i64.const 1)
    (i64.extend_i32_u (i32.add (global.get $__json_buf) (local.get $start)))
    (i64.extend_i32_u (i32.sub (global.get $__json_len) (local.get $start)))))
  (global.set $__json_len (local.get $start))
  (global.set $__json_failed (local.get $outer_failed))
  (global.set $__json_depth (local.get $outer_depth))
  (local.get $out))
"#;
