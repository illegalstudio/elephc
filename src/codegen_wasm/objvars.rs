//! Purpose:
//! `get_object_vars()` on `wasm32-wasi`: the per-class public-property table and the walk that
//! projects one object into an `array<string, mixed>`.
//!
//! Called from:
//! - `plan` lays the table out and registers the runtime, for a module that calls the builtin.
//! - `builtins` lowers `RuntimeFnId::GetObjectVars` to `__rt_object_vars`.
//! - `capability` gates the call against what this table can answer.
//!
//! Key details:
//! - Same shape as the gc-descriptor and json tables: one row per class id, reached through a
//!   pointer table, so a class resolves in two loads.
//! - Only the PUBLIC properties are laid out, which is what php projects for a call made outside
//!   the object's class. The audit refuses a call made from inside one rather than under-report
//!   the protected and private properties php would include there.

use std::collections::HashMap;

use crate::ir::{Immediate, Module, Op, RuntimeCallTarget, RuntimeFnId};
use crate::types::{ClassInfo, PhpType};

use super::wat::{DataSegment, Global, ValType, WatModule};

/// Base address of the per-class property rows, indexed by class id.
const PROP_PTRS_GLOBAL: &str = "__objvars_prop_ptrs";

/// Entry count of each class's row, indexed by class id.
const PROP_COUNTS_GLOBAL: &str = "__objvars_prop_counts";

/// Byte offset of each class's dynamic-property hash pointer, indexed by class id; 0 for none.
const DYN_OFFS_GLOBAL: &str = "__objvars_dyn_offs";

/// Number of class-id slots the two tables above carry.
const PROP_COUNT_GLOBAL: &str = "__objvars_prop_count";

/// Bytes per property row entry: name pointer, name length, slot offset, runtime tag.
const ENTRY_BYTES: u32 = 16;

/// The public properties of one class, in declaration order.
///
/// An absent visibility is php's implicit `public`, the same reading `json_encode`'s own table
/// takes. Order matters: php returns the properties in declaration order, and a hash preserves
/// insertion order, so walking the row in order is what makes the answer match.
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

/// The runtime value tag for a property's declared type, or `None` when it has no `mixed` form.
///
/// These are the tags `__rt_hash_set` understands as a VALUE: a scalar is stored raw, a string is
/// persisted into an owned copy, and a container or already-boxed cell is increfed. A float is
/// included here where `json_encode`'s table drops it — there is no serialization precision to
/// decide, the cell just carries the double's bits.
pub(super) fn property_value_tag(ty: &PhpType) -> Option<u32> {
    match ty.codegen_repr() {
        PhpType::Int => Some(0),
        PhpType::Str => Some(1),
        PhpType::Float => Some(2),
        PhpType::Bool | PhpType::False => Some(3),
        PhpType::Array(_) => Some(4),
        PhpType::AssocArray { .. } => Some(5),
        PhpType::Object(_) => Some(6),
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => Some(7),
        _ => None,
    }
}

/// Whether every public property of `class_name` (and of every class a value of its type can
/// be at runtime) has a tag the projection can carry and can never be left uninitialized.
///
/// A typed property with no default that no constructor writes is skipped by php, and this
/// target's zeroed slot is indistinguishable from a real `0`, so such a class is refused by name.
pub(super) fn class_is_projectable(module: &Module, class_name: &str) -> bool {
    let exact = |name: &str| {
        module.class_infos.get(name).is_some_and(|info| {
            public_properties(info).iter().all(|(property, ty)| {
                property_value_tag(ty).is_some()
                    && !super::capability::class_property_may_be_uninitialized(
                        module, name, property,
                    )
            })
        })
    };
    exact(class_name)
        && module
            .class_infos
            .iter()
            .filter(|(name, info)| {
                let mut parent = info.parent.clone();
                for _ in 0..module.class_infos.len() {
                    let Some(next) = parent.clone() else { return false };
                    if crate::names::php_symbol_key(&next)
                        == crate::names::php_symbol_key(class_name)
                    {
                        return name.as_str() != class_name;
                    }
                    parent = module.class_infos.get(&next).and_then(|info| info.parent.clone());
                }
                false
            })
            .all(|(name, _)| exact(name))
}

/// Whether any instruction in the module calls `get_object_vars()`.
///
/// The gate for laying the table out, so a module that never asks keeps the data segments it had.
pub(super) fn module_uses_get_object_vars(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .any(|function| {
            function.instructions.iter().any(|inst| {
                inst.op == Op::RuntimeCall
                    && matches!(
                        inst.immediate.as_ref(),
                        Some(Immediate::RuntimeCall(
                            RuntimeCallTarget::Function(RuntimeFnId::GetObjectVars)
                                | RuntimeCallTarget::ProfiledFunction {
                                    target: RuntimeFnId::GetObjectVars,
                                    ..
                                }
                        ))
                    )
            })
        })
}

/// Lays out the per-class public-property metadata and declares its globals.
///
/// Must run before `heap_base` is computed, like every other class-indexed table, so the rows sit
/// in static memory below the bump allocator. Returns the advanced data cursor.
pub(super) fn emit_object_vars_table(wm: &mut WatModule, module: &Module, mut cursor: u32) -> u32 {
    cursor = (cursor + 3) & !3;
    let class_infos = &module.class_infos;
    if class_infos.is_empty() {
        for name in [PROP_PTRS_GLOBAL, PROP_COUNTS_GLOBAL, PROP_COUNT_GLOBAL, DYN_OFFS_GLOBAL] {
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
    ordered.sort_by(|(left, left_info), (right, right_info)| {
        left_info
            .class_id
            .cmp(&right_info.class_id)
            .then_with(|| left.cmp(right))
    });
    let max_id = ordered.last().map(|(_, info)| info.class_id).unwrap_or(0);
    let count = max_id + 1;
    let mut id_to_name: HashMap<u64, &str> = HashMap::new();
    for (name, info) in &ordered {
        id_to_name.entry(info.class_id).or_insert(name.as_str());
    }

    // The property NAMES, laid out once each and shared by every row that mentions them.
    let mut name_offsets: HashMap<String, (u32, u32)> = HashMap::new();
    for (_, info) in &ordered {
        for (name, _) in public_properties(info) {
            if name_offsets.contains_key(&name) {
                continue;
            }
            let bytes = name.as_bytes().to_vec();
            let len = bytes.len() as u32;
            wm.add_data(DataSegment {
                offset: cursor,
                bytes,
            });
            name_offsets.insert(name, (cursor, len));
            cursor += len;
        }
    }

    // One entry per public property, in declaration order.
    cursor = (cursor + 3) & !3;
    let mut row_off: HashMap<u64, u32> = HashMap::new();
    let mut row_len: HashMap<u64, u32> = HashMap::new();
    let mut dyn_offsets: HashMap<u64, u32> = HashMap::new();
    for cid in 0..=max_id {
        let row_start = cursor;
        row_off.insert(cid, row_start);
        let Some(info) = id_to_name.get(&cid).and_then(|name| class_infos.get(*name)) else {
            row_len.insert(cid, 0);
            continue;
        };
        if let Some(name) = id_to_name.get(&cid) {
            if let Some(offset) =
                super::objects::dynamic_property_hash_offset_for_class(info, name, "")
            {
                dyn_offsets.insert(cid, offset as u32);
            }
        }
        let mut bytes = Vec::new();
        let mut entries = 0u32;
        for (property, ty) in public_properties(info) {
            let Some(tag) = property_value_tag(&ty) else {
                // The audit refuses such a class, so no module that reaches the walk has one.
                continue;
            };
            let index = info
                .properties
                .iter()
                .position(|(name, _)| *name == property)
                .unwrap_or(0);
            let offset = info
                .property_offsets
                .get(&property)
                .copied()
                .unwrap_or(8 + index * 16) as u32;
            let (name_ptr, name_len) = name_offsets[&property];
            bytes.extend_from_slice(&name_ptr.to_le_bytes());
            bytes.extend_from_slice(&name_len.to_le_bytes());
            bytes.extend_from_slice(&offset.to_le_bytes());
            bytes.extend_from_slice(&tag.to_le_bytes());
            debug_assert_eq!(bytes.len() as u32 % ENTRY_BYTES, 0, "row entry stride drifted");
            entries += 1;
        }
        row_len.insert(cid, entries);
        if !bytes.is_empty() {
            let written = bytes.len() as u32;
            wm.add_data(DataSegment {
                offset: row_start,
                bytes,
            });
            cursor += written;
        }
    }

    cursor = (cursor + 3) & !3;
    let ptrs_off = cursor;
    let mut ptrs = Vec::with_capacity(count as usize * 4);
    for cid in 0..=max_id {
        ptrs.extend_from_slice(&row_off.get(&cid).copied().unwrap_or(0).to_le_bytes());
    }
    wm.add_data(DataSegment {
        offset: ptrs_off,
        bytes: ptrs,
    });
    cursor += count as u32 * 4;

    let counts_off = cursor;
    let mut counts = Vec::with_capacity(count as usize * 4);
    for cid in 0..=max_id {
        counts.extend_from_slice(&row_len.get(&cid).copied().unwrap_or(0).to_le_bytes());
    }
    wm.add_data(DataSegment {
        offset: counts_off,
        bytes: counts,
    });
    cursor += count as u32 * 4;

    let dyn_off = cursor;
    let mut dyns = Vec::with_capacity(count as usize * 4);
    for cid in 0..=max_id {
        dyns.extend_from_slice(&dyn_offsets.get(&cid).copied().unwrap_or(0).to_le_bytes());
    }
    wm.add_data(DataSegment {
        offset: dyn_off,
        bytes: dyns,
    });
    cursor += count as u32 * 4;

    for (name, init) in [
        (DYN_OFFS_GLOBAL, dyn_off),
        (PROP_PTRS_GLOBAL, ptrs_off),
        (PROP_COUNTS_GLOBAL, counts_off),
        (PROP_COUNT_GLOBAL, count as u32),
    ] {
        wm.add_global(Global {
            name: name.to_string(),
            ty: ValType::I32,
            mutable: false,
            init: init as i64,
        });
    }
    cursor
}

/// Registers `__rt_object_vars` on `wm`.
pub(super) fn emit_object_vars_runtime(wm: &mut WatModule) {
    wm.add_raw_func(&rt_object_vars());
}

/// `__rt_object_vars`: one object projected into a fresh `array<string, mixed>`.
///
/// php returns the properties in DECLARATION order, and a hash preserves insertion order, so the
/// row is walked front to back and each entry inserted once.
///
/// Ownership follows `__rt_hash_set`'s contract exactly, which is why nothing is released here: a
/// string value is PERSISTED into a copy the hash owns, a container or boxed cell is INCREFED, and
/// a scalar is stored raw. The object keeps every reference it had, which is what makes
/// `get_object_vars($o)` leave `$o` untouched.
///
/// The high word is the slot's only for a STRING, where it is the length. A property slot's second
/// word is its runtime tag for a container and zero for a scalar, and neither is a value the hash
/// should carry — passing the tag through would file a container entry whose high word reads back
/// as a length.
fn rt_object_vars() -> String {
    format!(
        r#"(func $__rt_object_vars (param $obj i32) (result i32)
  (local $cid i32) (local $row i32) (local $n i32) (local $i i32)
  (local $entry i32) (local $slot i32) (local $tag i64) (local $out i32)
  (local $dyn i32) (local $hash i32) (local $cursor i64) (local $more i64) (local $next i64)
  (local $kcell i32) (local $vcell i32) (local $ktag i64) (local $klo i64) (local $khi i64)
  (local.set $out (call $__rt_hash_new (i64.const 8) (i64.const 7)))  ;; mixed-valued result hash
  (if (i32.eqz (local.get $obj)) (then (return (local.get $out))))    ;; guard: null receiver
  (local.set $cid (i32.wrap_i64 (i64.load (local.get $obj))))         ;; class_id @ O+0
  (if (i32.ge_u (local.get $cid) (global.get $__objvars_prop_count))
    (then (return (local.get $out))))                                 ;; no row for this class id
  (local.set $row
    (i32.load (i32.add (global.get $__objvars_prop_ptrs) (i32.mul (local.get $cid) (i32.const 4)))))
  (local.set $n
    (i32.load (i32.add (global.get $__objvars_prop_counts) (i32.mul (local.get $cid) (i32.const 4)))))
  (block $done (loop $property
    (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
    (local.set $entry (i32.add (local.get $row) (i32.mul (local.get $i) (i32.const {stride}))))
    (local.set $slot (i32.add (local.get $obj) (i32.load offset=8 (local.get $entry))))
    (local.set $tag (i64.extend_i32_u (i32.load offset=12 (local.get $entry))))
    (local.set $out (call $__rt_hash_set (local.get $out)
      (i64.extend_i32_u (i32.load (local.get $entry)))                ;; key: the name's bytes
      (i64.extend_i32_u (i32.load offset=4 (local.get $entry)))       ;; key length
      (i64.load (local.get $slot))                                    ;; value_lo
      (select (i64.load offset=8 (local.get $slot)) (i64.const 0)
        (i64.eq (local.get $tag) (i64.const 1)))                      ;; value_hi: a string's length only
      (local.get $tag)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $property)))
  ;; The dynamic properties follow the declared ones, in insertion order.
  (local.set $dyn
    (i32.load (i32.add (global.get $__objvars_dyn_offs) (i32.mul (local.get $cid) (i32.const 4)))))
  (if (local.get $dyn)
    (then
      (local.set $hash (i32.load (i32.add (local.get $obj) (local.get $dyn))))
      (if (local.get $hash)
        (then
          (local.set $cursor (i64.const -2))
          (block $dyn_done (loop $dyn_entry
            (call $__rt_mixed_iter_next (local.get $hash) (local.get $cursor) (i32.const 1))
            (local.set $more)
            (local.set $next)
            (br_if $dyn_done (i64.eqz (local.get $more)))
            (local.set $cursor (local.get $next))
            (local.set $kcell (call $__rt_mixed_iter_key (local.get $hash) (local.get $cursor) (i32.const 1)))
            (call $__rt_mixed_unbox (local.get $kcell))
            (local.set $khi)
            (local.set $klo)
            (local.set $ktag)
            (if (i64.ne (local.get $ktag) (i64.const 1))              ;; an integer key becomes its text
              (then
                (call $__rt_itoa (local.get $klo) (global.get $__float_scratch))
                (local.set $khi (i64.extend_i32_u))
                (local.set $klo (i64.extend_i32_u))))
            (local.set $vcell (call $__rt_mixed_iter_value (local.get $hash) (local.get $cursor) (i32.const 1)))
            (local.set $out (call $__rt_hash_set (local.get $out)
              (local.get $klo) (local.get $khi)
              (i64.extend_i32_u (local.get $vcell)) (i64.const 0) (i64.const 7)))
            (call $__rt_decref_any (local.get $vcell))                  ;; the set kept its own reference
            (call $__rt_decref_any (local.get $kcell))
            (br $dyn_entry)))))))
  (local.get $out))
"#,
        stride = ENTRY_BYTES
    )
}
