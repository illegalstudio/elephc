//! Purpose:
//! Owns the class-metadata runtime tables (`__class_parent_ids`,
//! `__class_interface_ptrs`, `__class_name_entries`, `__class_name_missing`) and
//! lowers the EIR `Op::InstanceOf` / `Op::InstanceOfDynamic` instructions plus the
//! `get_class` builtin for the wasm32-wasi backend.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_instruction` dispatches `InstanceOf`,
//!   `InstanceOfDynamic`, and the `get_class` builtin arm here.
//! - `crate::codegen_wasm::generate()` calls `emit_class_metadata_tables` after
//!   `emit_gc_desc_table` (threading the static-data cursor) and
//!   `emit_class_runtime` alongside the other runtime emitters.
//!
//! Key details:
//! - The tables are laid out in static memory below the heap base, the same
//!   cursor-threaded region as `emit_gc_desc_table`, and are indexed by runtime
//!   `class_id`. `__gc_desc_count` (already emitted by `emit_gc_desc_table` as
//!   `max_class_id + 1`) is reused as the shared bounds for every class-indexed
//!   table, so no parallel count global is introduced.
//! - `ClassInfo.interfaces` is the full transitive set (the frontend flattens
//!   directly-implemented interfaces, their interface-ancestors, and class-parent
//!   interfaces), so each class's interface block is self-contained for
//!   instanceof-interface. The parent-chain walk in `__rt_instanceof` mirrors the
//!   native `__rt_exception_matches` and is belt-and-suspenders.
//! - The runtime helpers are borrow-only: they never decref their operands. The
//!   instanceof/get_class operands are released by the EIR ownership pass; the
//!   `get_class` result is a data-segment pointer with no persist.
//! - Dynamic instanceof supports object and Mixed/null targets; a string target
//!   needs a dedicated name->id lookup table and is deferred to P6g, emitting a
//!   clear `Unsupported` diagnostic before any dereference.

use super::context::{FnCtx, Result};
use super::inst::{data_immediate, operand, store_result};
use super::wat::{DataSegment, Global, ValType, WatModule};
use super::WasmError;
use crate::ir::{Instruction, Module, ValueId};
use crate::types::{ClassInfo, PhpType};
use std::collections::HashMap;

/// `__rt_instanceof`: returns 1 iff the object at `obj` is an instance of the
/// named target (`target_id`, `target_kind` where 0 = class, 1 = interface).
///
/// Walks the class parent chain via `__class_parent_ids` (i64 each, -1 = root). At
/// each class: a class target matches on `cid == target_id`; an interface target
/// scans the class's interface block (`__class_interface_ptrs[cid]` -> `[i64
/// count][i64 iface_id, i64 impl_ptr] * count`, 16-byte stride) for `iface_id ==
/// target_id`, skipping the scan when the block pointer is 0 (the class implements
/// no interfaces, which would otherwise read address 0 and trap). Out-of-range
/// `cid` and a root parent return false. Borrows the object (never frees it).
const RT_INSTANCEOF: &str = r#"(func $__rt_instanceof (param $obj i32) (param $target_id i64) (param $target_kind i32) (result i32)
  (local $cid i64) (local $ptr i32) (local $n i64) (local $i i64) (local $iid i64) (local $parent i64)
  local.get $obj                        ;; null receiver -> false
  i32.eqz
  if
    i32.const 0
    return
  end
  local.get $obj                        ;; read the runtime class id at +0
  i64.load
  local.set $cid
  block $done
    loop $walk
      local.get $cid                     ;; out of range -> false
      i32.wrap_i64
      global.get $__gc_desc_count
      i32.ge_u
      br_if $done
      local.get $target_kind             ;; class target (kind 0)?
      i32.eqz
      if
        local.get $cid                   ;; exact class match -> true
        local.get $target_id
        i64.eq
        if
          i32.const 1
          return
        end
      else
        global.get $__class_interface_ptrs  ;; this class's interface block ptr
        local.get $cid
        i64.const 4
        i64.mul
        i32.wrap_i64
        i32.add
        i32.load
        local.set $ptr
        local.get $ptr                   ;; no interfaces on this class -> skip the scan
        i32.eqz
        if
        else
          local.get $ptr                 ;; interface count
          i64.load
          local.set $n
          i64.const 0
          local.set $i
          block $scan_done
            loop $scan
              local.get $i               ;; i < count
              local.get $n
              i64.lt_u
              if
                local.get $ptr           ;; entry[i].iface_id at ptr + 8 + i*16
                local.get $i
                i64.const 16
                i64.mul
                i32.wrap_i64
                i32.add
                i64.load offset=8
                local.set $iid
                local.get $iid           ;; interface match -> true
                local.get $target_id
                i64.eq
                if
                  i32.const 1
                  return
                end
                local.get $i             ;; i++
                i64.const 1
                i64.add
                local.set $i
                br $scan
              else
                br $scan_done
              end
            end
          end
        end
      end
      global.get $__class_parent_ids     ;; parent class id
      local.get $cid
      i64.const 8
      i64.mul
      i32.wrap_i64
      i32.add
      i64.load
      local.set $parent
      local.get $parent                  ;; root -> false
      i64.const -1
      i64.eq
      br_if $done
      local.get $parent                  ;; walk up the parent chain
      local.set $cid
      br $walk
    end
  end
  i32.const 0)                           ;; no match -> false
"#;

/// `__rt_mixed_instanceof`: unboxes a Mixed cell and delegates an object payload
/// to `__rt_instanceof`; any other tag (null, scalar, container) is false. Borrows
/// the cell (never frees it).
const RT_MIXED_INSTANCEOF: &str = r#"(func $__rt_mixed_instanceof (param $mixed i32) (param $target_id i64) (param $target_kind i32) (result i32)
  (local $tag i64) (local $lo i64) (local $hi i64)
  (call $__rt_mixed_unbox (local.get $mixed))                           ;; unbox -> stack: tag, lo, hi
  (local.set $hi)                                                       ;; pop value high word
  (local.set $lo)                                                       ;; pop value low word
  (local.set $tag)                                                      ;; pop runtime tag
  (if (i64.eq (local.get $tag) (i64.const 6))                           ;; tag 6 = object -> delegate
    (then (return (call $__rt_instanceof (i32.wrap_i64 (local.get $lo)) (local.get $target_id) (local.get $target_kind)))))
  (i32.const 0))                                                        ;; non-object -> false
"#;

/// `__rt_class_name_by_cid`: returns the `(ptr, len)` of the class name for the
/// runtime class id `cid`, or the empty `__class_name_missing` row when the id is
/// out of range. The result points into static memory (no persist).
const RT_CLASS_NAME_BY_CID: &str = r#"(func $__rt_class_name_by_cid (param $cid i64) (result i32) (result i64)
  (local $base i32)
  (if (i32.ge_u (i32.wrap_i64 (local.get $cid)) (global.get $__gc_desc_count))  ;; out of range -> ""
    (then (return (global.get $__class_name_missing) (i64.const 0))))
  (local.set $base (i32.add (global.get $__class_name_entries) (i32.wrap_i64 (i64.mul (local.get $cid) (i64.const 16)))))  ;; row base = entries + cid*16
  (i32.load offset=0 (local.get $base))                                 ;; name pointer
  (i64.load offset=8 (local.get $base)))                               ;; name length
"#;

/// `__rt_class_name_by_obj`: returns the `(ptr, len)` of the runtime class name
/// of the object at `obj`, or `("", 0)` for a null receiver. Borrows the object.
const RT_CLASS_NAME_BY_OBJ: &str = r#"(func $__rt_class_name_by_obj (param $obj i32) (result i32) (result i64)
  (if (i32.eqz (local.get $obj))                                        ;; null -> ""
    (then (return (global.get $__class_name_missing) (i64.const 0))))
  (call $__rt_class_name_by_cid (i64.load (local.get $obj))))           ;; cid = [obj+0], lookup
"#;

/// `__rt_instanceof_lookup`: resolves a runtime class/interface name string to
/// `(success, target_id, target_kind)` via a linear scan of `__instanceof_target_entries`
/// (32-byte rows: `[name_ptr i32][pad4][name_len i64][target_id i64][target_kind i32][pad4]`),
/// case-insensitively (A-Z folded to a-z; other bytes compared verbatim). Returns
/// `(1, id, kind)` on the first match — kind 0 = class, 1 = interface — and `(0, 0, 0)`
/// when no row matches or the table is empty. Borrows the query string (never frees it),
/// mirroring the native `__rt_instanceof_lookup` exactly. The empty-table case is handled
/// by `$count == 0`, so unit-test harnesses (`emit_class_metadata_stub`) no-op safely.
const RT_INSTANCEOF_LOOKUP: &str = r#"(func $__rt_instanceof_lookup (param $qptr i32) (param $qlen i64) (result i32) (result i64) (result i32)
  (local $count i32) (local $base i32) (local $i i32) (local $row i32) (local $nlen i64) (local $b i32) (local $qb i32) (local $nb i32) (local $nptr i32)
  (local.set $count (global.get $__instanceof_target_count))                          ;; row count (0 -> no match)
  (local.set $base (global.get $__instanceof_target_entries))                         ;; row table base
  (local.set $i (i32.const 0))                                                        ;; row index = 0
  (block $no_match                                                                   ;; break target = no match
    (loop $scan
      (br_if $no_match (i32.ge_u (local.get $i) (local.get $count)))                 ;; all rows scanned -> no match
      (local.set $row (i32.add (local.get $base) (i32.mul (local.get $i) (i32.const 32))))  ;; row = base + i*32
      (local.set $nlen (i64.load offset=8 (local.get $row)))                          ;; candidate name length
      (if (i64.eq (local.get $qlen) (local.get $nlen))                                ;; same length -> compare bytes
        (then
          (local.set $nptr (i32.load offset=0 (local.get $row)))                      ;; candidate name pointer
          (local.set $b (i32.const 0))                                                ;; byte index = 0
          (block $next_row                                                            ;; break target = next row
            (loop $byte_loop
              (if (i64.ge_u (i64.extend_i32_u (local.get $b)) (local.get $qlen))     ;; all bytes matched -> return
                (then
                  (return (i32.const 1) (i64.load offset=16 (local.get $row)) (i32.load offset=24 (local.get $row)))  ;; (1, target_id, target_kind)
                )
              )
              (local.set $qb (i32.load8_u (i32.add (local.get $qptr) (local.get $b))))  ;; query byte
              (local.set $nb (i32.load8_u (i32.add (local.get $nptr) (local.get $b))))  ;; candidate byte
              (local.set $qb (select (i32.add (local.get $qb) (i32.const 32)) (local.get $qb) (i32.and (i32.ge_u (local.get $qb) (i32.const 65)) (i32.le_u (local.get $qb) (i32.const 90)))))  ;; fold A-Z -> a-z
              (local.set $nb (select (i32.add (local.get $nb) (i32.const 32)) (local.get $nb) (i32.and (i32.ge_u (local.get $nb) (i32.const 65)) (i32.le_u (local.get $nb) (i32.const 90)))))  ;; fold A-Z -> a-z
              (br_if $next_row (i32.ne (local.get $qb) (local.get $nb)))               ;; mismatch -> next row
              (local.set $b (i32.add (local.get $b) (i32.const 1)))                   ;; b++
              (br $byte_loop)                                                         ;; continue byte loop
            )
          )
        )
      )
      (local.set $i (i32.add (local.get $i) (i32.const 1)))                           ;; i++
      (br $scan)                                                                      ;; next row
    )
  )
  (i32.const 0) (i64.const 0) (i32.const 0)                                          ;; no match -> (0, 0, 0)
)
"#;

/// Registers the import-free class runtime helpers on `wm`.
///
/// Emits `__rt_instanceof`, `__rt_mixed_instanceof`, `__rt_class_name_by_cid`,
/// `__rt_class_name_by_obj`, and `__rt_instanceof_lookup`. They reference the
/// `$__gc_desc_count`,
/// `$__class_parent_ids`, `$__class_interface_ptrs`, `$__class_name_entries`, and
/// `$__class_name_missing` globals, so every module emitting them must also emit
/// either `emit_class_metadata_tables` (real programs) or `emit_class_metadata_stub`
/// (unit-test harnesses) so the globals exist and the WAT validates. The helpers are
/// borrow-only and safely return false/empty when `__gc_desc_count == 0` or
/// `__instanceof_target_count == 0`.
pub(super) fn emit_class_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_INSTANCEOF);
    wm.add_raw_func(RT_MIXED_INSTANCEOF);
    wm.add_raw_func(RT_CLASS_NAME_BY_CID);
    wm.add_raw_func(RT_CLASS_NAME_BY_OBJ);
    wm.add_raw_func(RT_INSTANCEOF_LOOKUP);
}

/// Declares the class-metadata globals at zero/empty for unit-test harnesses that
/// register no classes.
///
/// With `__gc_desc_count == 0` (from `emit_gc_desc_stub`) the instanceof bounds
/// check fails for every `cid`, the name lookup returns the missing row, and
/// `__rt_instanceof_lookup` sees `__instanceof_target_count == 0` so its scan is
/// empty, so the helpers no-op safely. `$__class_name_missing` still needs a real
/// address, so a single null byte is laid out at offset 0 (linear memory always has a
/// valid address 0 region in the runtime scratch space).
#[cfg(test)]
pub(super) fn emit_class_metadata_stub(wm: &mut WatModule) {
    wm.add_data(DataSegment {
        offset: 0,
        bytes: vec![0u8],
    });
    wm.add_global(Global {
        name: "__class_name_missing".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: 0,
    });
    wm.add_global(Global {
        name: "__class_parent_ids".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: 0,
    });
    wm.add_global(Global {
        name: "__class_interface_ptrs".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: 0,
    });
    wm.add_global(Global {
        name: "__class_name_entries".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: 0,
    });
    // P6g: empty instanceof target table so `__rt_instanceof_lookup` (now registered by
    // `emit_class_runtime`) validates and no-ops with `count == 0`.
    wm.add_global(Global {
        name: "__instanceof_target_entries".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: 0,
    });
    wm.add_global(Global {
        name: "__instanceof_target_count".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: 0,
    });
}

/// Emits the class-metadata tables and globals, advancing the static-data cursor,
/// and returns the new cursor.
///
/// Mirrors `emit_gc_desc_table`: all data is laid out in static memory below the
/// heap base, indexed by runtime `class_id`, and the cursor is threaded through
/// `generate()`. Reuses `$__gc_desc_count` (already `max_class_id + 1` from
/// `emit_gc_desc_table`) as the shared bounds. Layout, in emission order:
/// - `__class_name_missing` (1 null byte).
/// - Per-class ascii name labels (recorded for the name table).
/// - `__class_parent_ids` (i64 array, `count` entries, 8-aligned; -1 = root/gap).
/// - Per-class interface blocks `[i64 count][i64 iface_id, i64 impl_ptr=0] * count`
///   (8-aligned, 16-byte stride; only for classes that implement interfaces),
///   then `__class_interface_ptrs` (i32 array, `count` entries; 0 = no interfaces).
/// - `__class_name_entries` (16-byte rows `[i32 ptr][i32 pad][i64 len]`, 8-aligned).
pub(super) fn emit_class_metadata_tables(wm: &mut WatModule, module: &Module, cursor: u32) -> u32 {
    let mut cursor = (cursor + 7) & !7; // 8-align the class-metadata region

    // A single null byte for the missing-name row, always present.
    let missing_off = cursor;
    wm.add_data(DataSegment {
        offset: missing_off,
        bytes: vec![0u8],
    });
    cursor += 1;

    if module.class_infos.is_empty() {
        // No classes: declare the globals at 0 (the helpers no-op via __gc_desc_count==0).
        wm.add_global(Global {
            name: "__class_name_missing".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: missing_off as i64,
        });
        for name in ["__class_parent_ids", "__class_interface_ptrs", "__class_name_entries"] {
            wm.add_global(Global {
                name: name.to_string(),
                ty: ValType::I32,
                mutable: false,
                init: 0,
            });
        }
        return cursor;
    }

    let id_to_ci: HashMap<u64, &ClassInfo> =
        module.class_infos.values().map(|ci| (ci.class_id, ci)).collect();
    // class_id -> canonical class name (the `class_infos` key).
    let id_to_name: HashMap<u64, &str> = module
        .class_infos
        .iter()
        .map(|(name, ci)| (ci.class_id, name.as_str()))
        .collect();
    let max_id = module.class_infos.values().map(|ci| ci.class_id).max().unwrap_or(0);
    let count = max_id + 1;

    // Per-class ascii name labels (1-aligned; the i64 arrays below re-align).
    let mut label_off: HashMap<u64, u32> = HashMap::new();
    let mut label_len: HashMap<u64, u64> = HashMap::new();
    for cid in 0..=max_id {
        if let Some(name) = id_to_name.get(&cid) {
            let bytes = name.as_bytes().to_vec();
            label_len.insert(cid, bytes.len() as u64);
            label_off.insert(cid, cursor);
            wm.add_data(DataSegment {
                offset: cursor,
                bytes,
            });
            cursor += label_len[&cid] as u32;
        }
    }

    // __class_parent_ids: i64 array (8-aligned).
    cursor = (cursor + 7) & !7;
    let parent_ids_off = cursor;
    let mut parent_bytes = Vec::with_capacity(count as usize * 8);
    for cid in 0..=max_id {
        let parent_id = id_to_ci
            .get(&cid)
            .and_then(|ci| ci.parent.as_ref())
            .and_then(|p| module.class_infos.get(p))
            .map(|pci| pci.class_id as i64)
            .unwrap_or(-1);
        parent_bytes.extend_from_slice(&parent_id.to_le_bytes());
    }
    wm.add_data(DataSegment {
        offset: parent_ids_off,
        bytes: parent_bytes,
    });
    cursor += (count * 8) as u32;

    // Per-class interface blocks (8-aligned), recorded for the pointer table.
    cursor = (cursor + 7) & !7;
    let mut block_off: HashMap<u64, u32> = HashMap::new();
    for cid in 0..=max_id {
        let Some(ci) = id_to_ci.get(&cid) else { continue };
        if ci.interfaces.is_empty() {
            continue;
        }
        // Resolve each interface name to its interface_id; skip names not in the table.
        let ifaces: Vec<u64> = ci
            .interfaces
            .iter()
            .filter_map(|name| module.interface_infos.get(name).map(|ii| ii.interface_id))
            .collect();
        if ifaces.is_empty() {
            continue;
        }
        block_off.insert(cid, cursor);
        let mut bytes = Vec::with_capacity(8 + ifaces.len() * 16);
        bytes.extend_from_slice(&(ifaces.len() as i64).to_le_bytes()); // count
        for iface_id in &ifaces {
            bytes.extend_from_slice(&(*iface_id as i64).to_le_bytes()); // iface_id
            bytes.extend_from_slice(&0i64.to_le_bytes()); // impl_ptr (reserved)
        }
        wm.add_data(DataSegment {
            offset: cursor,
            bytes,
        });
        cursor += (8 + ifaces.len() as u32 * 16) as u32;
    }

    // __class_interface_ptrs: i32 array (4-aligned); 0 = no interfaces.
    cursor = (cursor + 3) & !3;
    let interface_ptrs_off = cursor;
    let mut ptr_bytes = Vec::with_capacity(count as usize * 4);
    for cid in 0..=max_id {
        let off = block_off.get(&cid).copied().unwrap_or(0);
        ptr_bytes.extend_from_slice(&off.to_le_bytes());
    }
    wm.add_data(DataSegment {
        offset: interface_ptrs_off,
        bytes: ptr_bytes,
    });
    cursor += (count * 4) as u32;

    // __class_name_entries: 16-byte rows [i32 ptr][i32 pad][i64 len] (8-aligned for len).
    cursor = (cursor + 7) & !7;
    let name_entries_off = cursor;
    let mut entry_bytes = Vec::with_capacity(count as usize * 16);
    for cid in 0..=max_id {
        let (ptr, len) = match (label_off.get(&cid), label_len.get(&cid)) {
            (Some(&off), Some(&len)) => (off as i32, len as i64),
            _ => (missing_off as i32, 0i64),
        };
        entry_bytes.extend_from_slice(&ptr.to_le_bytes()); // ptr
        entry_bytes.extend_from_slice(&0u32.to_le_bytes()); // pad
        entry_bytes.extend_from_slice(&len.to_le_bytes()); // len
    }
    wm.add_data(DataSegment {
        offset: name_entries_off,
        bytes: entry_bytes,
    });
    cursor += (count * 16) as u32;

    wm.add_global(Global {
        name: "__class_name_missing".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: missing_off as i64,
    });
    wm.add_global(Global {
        name: "__class_parent_ids".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: parent_ids_off as i64,
    });
    wm.add_global(Global {
        name: "__class_interface_ptrs".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: interface_ptrs_off as i64,
    });
    wm.add_global(Global {
        name: "__class_name_entries".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: name_entries_off as i64,
    });
    cursor
}

/// Emits the `__instanceof_target_entries` lookup table — one row per class/interface name
/// spelling (plain and leading-backslash absolute) — plus the `$__instanceof_target_entries`
/// (base) and `$__instanceof_target_count` (row count) globals, advancing the static-data
/// cursor and returning the new cursor.
///
/// Each row is 32 bytes: `[name_ptr i32][pad4][name_len i64][target_id i64][target_kind
/// i32][pad4]`, 8-aligned so the i64 fields load cleanly. `__rt_instanceof_lookup` scans
/// this table case-insensitively to resolve a dynamic string instanceof target. Mirrors
/// the native `_instanceof_target_entries` table (two spellings per target: plain + `\Name`,
/// so `count == (classes + interfaces) * 2`). `generate()` threads this after
/// `emit_class_metadata_tables`, before `heap_base` is computed, so the table sits in
/// static memory below the heap. With no classes or interfaces the globals are zeroed so
/// the lookup no-ops with `count == 0`.
pub(super) fn emit_instanceof_target_table(wm: &mut WatModule, module: &Module, cursor: u32) -> u32 {
    // Collect (name, id, kind): classes (kind 0) then interfaces (kind 1). Class and
    // interface id spaces are distinct; `target_kind` disambiguates them at lookup time.
    let mut targets: Vec<(String, u64, i32)> = Vec::new();
    for (name, ci) in &module.class_infos {
        targets.push((name.clone(), ci.class_id, 0));
    }
    for (name, ii) in &module.interface_infos {
        targets.push((name.clone(), ii.interface_id, 1));
    }
    targets.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)));

    let mut cursor = (cursor + 7) & !7; // 8-align the name bytes and the i64-bearing rows

    if targets.is_empty() {
        wm.add_global(Global {
            name: "__instanceof_target_entries".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: 0,
        });
        wm.add_global(Global {
            name: "__instanceof_target_count".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: 0,
        });
        return cursor;
    }

    // Emit the plain and absolute (`\Name`) name bytes, recording each spelling's offset.
    let mut name_off: HashMap<(u64, i32, bool), u32> = HashMap::new();
    let mut name_len: HashMap<(u64, i32, bool), u64> = HashMap::new();
    for (name, id, kind) in &targets {
        for is_abs in [false, true] {
            let bytes: Vec<u8> = if is_abs {
                let mut b = vec![b'\\'];
                b.extend_from_slice(name.as_bytes());
                b
            } else {
                name.as_bytes().to_vec()
            };
            name_len.insert((*id, *kind, is_abs), bytes.len() as u64);
            name_off.insert((*id, *kind, is_abs), cursor);
            wm.add_data(DataSegment {
                offset: cursor,
                bytes,
            });
            cursor += name_len[&(*id, *kind, is_abs)] as u32;
        }
    }

    // 8-align for the i64-bearing rows, then emit one 32-byte row per spelling.
    cursor = (cursor + 7) & !7;
    let rows_off = cursor;
    let mut row_bytes: Vec<u8> = Vec::with_capacity(targets.len() * 2 * 32);
    for (name, id, kind) in &targets {
        let _ = name; // name bytes already emitted above; only offsets are referenced here.
        for is_abs in [false, true] {
            let ptr = name_off[&(*id, *kind, is_abs)] as i32;
            let len = name_len[&(*id, *kind, is_abs)] as i64;
            row_bytes.extend_from_slice(&ptr.to_le_bytes()); // name_ptr (i32)
            row_bytes.extend_from_slice(&0u32.to_le_bytes()); // pad (i32)
            row_bytes.extend_from_slice(&len.to_le_bytes()); // name_len (i64)
            row_bytes.extend_from_slice(&(*id as i64).to_le_bytes()); // target_id (i64)
            row_bytes.extend_from_slice(&(*kind as i32).to_le_bytes()); // target_kind (i32)
            row_bytes.extend_from_slice(&0u32.to_le_bytes()); // pad (i32)
        }
    }
    wm.add_data(DataSegment {
        offset: rows_off,
        bytes: row_bytes,
    });
    cursor += (targets.len() as u32 * 2 * 32) as u32;

    wm.add_global(Global {
        name: "__instanceof_target_entries".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: rows_off as i64,
    });
    wm.add_global(Global {
        name: "__instanceof_target_count".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: (targets.len() as i64) * 2,
    });
    cursor
}

/// Reads the class-name immediate of an instanceof instruction.
///
/// `Op::InstanceOf` carries `Immediate::Data` indexing into `module.data.class_names`
/// (the pool `intern_class_name` interns into), NOT `module.data.strings`. Mirrors the
/// native `class_name_immediate` helper.
fn class_name_immediate(ctx: &FnCtx, inst: &Instruction) -> Result<String> {
    let data_id = data_immediate(inst)?;
    ctx.module
        .data
        .class_names
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("instanceof: unknown class name {:?}", data_id)))
}

/// Resolves a named instanceof target to `(target_id, target_kind)`.
///
/// A class name resolves to `(class_id, 0)`; an interface name to `(interface_id, 1)`.
/// Both the raw and backslash-trimmed spellings are tried so FQN immediates match the
/// canonical `class_infos`/`interface_infos` keys. Returns `None` for an unknown name.
fn classify_named_target(ctx: &FnCtx, name: &str) -> Option<(u64, i32)> {
    let trimmed = name.trim_start_matches('\\');
    if let Some(ci) = ctx
        .module
        .class_infos
        .get(name)
        .or_else(|| ctx.module.class_infos.get(trimmed))
    {
        return Some((ci.class_id, 0));
    }
    if let Some(ii) = ctx
        .module
        .interface_infos
        .get(name)
        .or_else(|| ctx.module.interface_infos.get(trimmed))
    {
        return Some((ii.interface_id, 1));
    }
    None
}

/// Emits a `("", 0)` string result into the instruction's Str result slot.
///
/// Pushes the `$__class_name_missing` pointer (i32) and a zero length (i64), then
/// stores them as the Str result. The pointer is a static data address, so the
/// release is a no-op.
fn emit_empty_class_name(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.fb
        .ins("global.get $__class_name_missing", "empty class name pointer");
    ctx.fb.ins("i64.const 0", "empty class name length");
    store_result(ctx, inst)
}

/// Lowers `Op::InstanceOf` (a named class/interface target).
///
/// A non-object receiver (null/scalar) is a compile-time false. The target name is
/// resolved to `(target_id, target_kind)`; an unknown name is false (not a trap). An
/// `Object` receiver calls `__rt_instanceof`; a `Mixed`/`Union` receiver calls
/// `__rt_mixed_instanceof`. The result is widened from i32 to the i64 Bool slot.
pub(super) fn lower_instanceof(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?;

    let name = class_name_immediate(ctx, inst)?;
    let Some((target_id, kind)) = classify_named_target(ctx, &name) else {
        // Unknown target name -> false (PHP would warn + false; never a trap).
        ctx.fb
            .ins("i64.const 0", "instanceof unknown target -> false");
        return store_result(ctx, inst);
    };

    match value_ty {
        PhpType::Object(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins(&format!("i64.const {}", target_id), "instanceof target id");
            ctx.fb
                .ins(&format!("i32.const {}", kind), "instanceof target kind");
            ctx.fb
                .ins("call $__rt_instanceof", "runtime instanceof (object receiver)");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
            store_result(ctx, inst)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins(&format!("i64.const {}", target_id), "instanceof target id");
            ctx.fb
                .ins(&format!("i32.const {}", kind), "instanceof target kind");
            ctx.fb.ins(
                "call $__rt_mixed_instanceof",
                "runtime instanceof (mixed/union receiver)",
            );
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
            store_result(ctx, inst)
        }
        _ => {
            // null/scalar receiver -> false
            ctx.fb
                .ins("i64.const 0", "instanceof non-object receiver -> false");
            store_result(ctx, inst)
        }
    }
}

/// Emits the code that normalizes an instanceof value receiver to a raw object pointer
/// in local `vp` (0 when the value is not an object).
///
/// Shared by the object/mixed-target and string-target paths of `lower_instanceof_dynamic`:
/// both end in `__rt_instanceof(vp, target_id, target_kind)`, which returns false for a
/// null receiver (`vp == 0`). A `Mixed`/`Union` value is unboxed and the object payload
/// extracted via a `select` (`lo_i32` when tag == 6, else 0); a scalar/null value yields 0.
fn normalize_receiver_ptr(
    ctx: &mut FnCtx,
    value: ValueId,
    value_ty: &PhpType,
    vp: &str,
) -> Result<()> {
    match value_ty {
        PhpType::Object(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins(&format!("local.set {}", vp), "value object pointer");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let hi = ctx.fresh_temp(ValType::I64);
            let lo = ctx.fresh_temp(ValType::I64);
            let tag = ctx.fresh_temp(ValType::I64);
            ctx.emit_load_value(value)?;
            ctx.fb.ins("call $__rt_mixed_unbox", "unbox value -> (tag, lo, hi)");
            ctx.fb.ins(&format!("local.set {}", hi), "discard value high word");
            ctx.fb.ins(&format!("local.set {}", lo), "value low word");
            ctx.fb.ins(&format!("local.set {}", tag), "value runtime tag");
            // vp = (tag == 6) ? lo_i32 : 0  (select: val1 if cond!=0 else val2)
            ctx.fb.ins(&format!("local.get {}", lo), "value object ptr (if object)");
            ctx.fb.ins("i32.wrap_i64", "value low word -> i32 ptr");
            ctx.fb.ins("i32.const 0", "non-object value ptr");
            ctx.fb.ins(&format!("local.get {}", tag), "value runtime tag");
            ctx.fb.ins("i64.const 6", "object tag");
            ctx.fb.ins("i64.eq", "is value an object?");
            ctx.fb.ins("select", "vp = object ? lo : 0");
            ctx.fb
                .ins(&format!("local.set {}", vp), "value receiver pointer");
        }
        _ => {
            ctx.fb.ins("i32.const 0", "non-object value receiver");
            ctx.fb
                .ins(&format!("local.set {}", vp), "value receiver pointer");
        }
    }
    Ok(())
}

/// String-target arm of `Op::InstanceOfDynamic` (`$x instanceof "SomeClass"`).
///
/// Resolves the runtime target name via `__rt_instanceof_lookup` (case-insensitive scan
/// of `__instanceof_target_entries`), then runs `__rt_instanceof` with the resolved
/// `(target_id, target_kind)` against the value's object pointer. An unknown name yields
/// `(0, 0, 0)` -> false (PHP warns + false, never a trap). The value is normalized to a
/// receiver pointer exactly like the object-target path, so a non-object value
/// (`vp == 0`) is false. Borrows the target string (the lookup never frees it).
fn lower_instanceof_dynamic_string(
    ctx: &mut FnCtx,
    inst: &Instruction,
    value: ValueId,
    target: ValueId,
    value_ty: PhpType,
) -> Result<()> {
    let vp = ctx.fresh_temp(ValType::I32);
    normalize_receiver_ptr(ctx, value, &value_ty, &vp)?;

    // Load the target string (ptr i32, len i64) and look it up. `emit_load_value` pushes
    // the pair [ptr, len] with len on top, so capture len then ptr.
    let qlen = ctx.fresh_temp(ValType::I64);
    let qptr = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(target)?;
    ctx.fb
        .ins(&format!("local.set {}", qlen), "target string length (top of str pair)");
    ctx.fb
        .ins(&format!("local.set {}", qptr), "target string pointer");
    let tid = ctx.fresh_temp(ValType::I64);
    let tkind = ctx.fresh_temp(ValType::I32);
    let tvalid = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.get {}", qptr), "target string pointer");
    ctx.fb.ins(&format!("local.get {}", qlen), "target string length");
    ctx.fb.ins(
        "call $__rt_instanceof_lookup",
        "look up target name -> (success, id, kind)",
    );
    ctx.fb
        .ins(&format!("local.set {}", tkind), "resolved target kind (top)");
    ctx.fb
        .ins(&format!("local.set {}", tid), "resolved target id");
    ctx.fb
        .ins(&format!("local.set {}", tvalid), "lookup success flag");

    // if success: __rt_instanceof(vp, id, kind) else 0; widen to i64 Bool.
    ctx.fb.ins(&format!("local.get {}", tvalid), "lookup success flag");
    ctx.fb.ins("if (result i32)", "resolved target -> check, else false");
    ctx.fb
        .ins(&format!("local.get {}", vp), "value receiver pointer");
    ctx.fb
        .ins(&format!("local.get {}", tid), "resolved target id");
    ctx.fb
        .ins(&format!("local.get {}", tkind), "resolved target kind");
    ctx.fb.ins(
        "call $__rt_instanceof",
        "runtime instanceof (string target)",
    );
    ctx.fb.ins("else", "unknown name -> false");
    ctx.fb.ins("i32.const 0", "false");
    ctx.fb.ins("end", "end string-target instanceof");
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    store_result(ctx, inst)
}

/// Lowers `Op::InstanceOfDynamic` (a runtime target operand).
///
/// Supported targets: an `Object` (read its runtime class id), a `Mixed`/`Union`
/// (unbox; an object payload uses the object path, anything else is false), and a `Str`
/// (resolved via `__rt_instanceof_lookup` against the `__instanceof_target_entries`
/// table — P6g). A null/scalar target is false. The value is normalized to a receiver
/// pointer (0 = non-object) shared by every target arm.
pub(super) fn lower_instanceof_dynamic(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let target = operand(inst, 1)?;
    let value_ty = ctx.value_php_type(value)?;
    let target_ty = ctx.value_php_type(target)?;

    // Type-gate the target FIRST (before any dereference of it).
    match target_ty {
        PhpType::Str => {
            return lower_instanceof_dynamic_string(ctx, inst, value, target, value_ty);
        }
        PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_) => {}
        _ => {
            // scalar/null target -> always false
            ctx.fb
                .ins("i64.const 0", "instanceof scalar/null target -> false");
            return store_result(ctx, inst);
        }
    }

    // Shared temps: the value receiver pointer (0 = non-object) and the resolved
    // target class id + validity flag.
    let vp = ctx.fresh_temp(ValType::I32);
    let tcid = ctx.fresh_temp(ValType::I64);
    let tvalid = ctx.fresh_temp(ValType::I32);

    // Normalize the value to a receiver pointer (0 = not an object).
    normalize_receiver_ptr(ctx, value, &value_ty, &vp)?;

    // Resolve the target to (target_cid, valid). kind is always 0 (class).
    match target_ty {
        PhpType::Object(_) => {
            let tobj = ctx.fresh_temp(ValType::I32);
            ctx.emit_load_value(target)?;
            ctx.fb.ins(&format!("local.set {}", tobj), "target object pointer");
            ctx.fb.ins(&format!("local.get {}", tobj), "target object pointer");
            ctx.fb.ins("if", "target non-null?");
            ctx.fb.ins(&format!("local.get {}", tobj), "target object pointer");
            ctx.fb.ins("i64.load offset=0", "target class id");
            ctx.fb.ins(&format!("local.set {}", tcid), "target class id");
            ctx.fb.ins("i32.const 1", "target valid");
            ctx.fb.ins(&format!("local.set {}", tvalid), "target valid flag");
            ctx.fb.ins("else", "null target");
            ctx.fb.ins("i64.const 0", "no target class id");
            ctx.fb.ins(&format!("local.set {}", tcid), "no target class id");
            ctx.fb.ins("i32.const 0", "target invalid");
            ctx.fb.ins(&format!("local.set {}", tvalid), "target valid flag");
            ctx.fb.ins("end", "end target null test");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let thi = ctx.fresh_temp(ValType::I64);
            let tlo = ctx.fresh_temp(ValType::I64);
            let ttag = ctx.fresh_temp(ValType::I64);
            ctx.emit_load_value(target)?;
            ctx.fb.ins("call $__rt_mixed_unbox", "unbox target -> (tag, lo, hi)");
            ctx.fb.ins(&format!("local.set {}", thi), "discard target high word");
            ctx.fb.ins(&format!("local.set {}", tlo), "target low word");
            ctx.fb.ins(&format!("local.set {}", ttag), "target runtime tag");
            ctx.fb.ins(&format!("local.get {}", ttag), "target runtime tag");
            ctx.fb.ins("i64.const 6", "object tag");
            ctx.fb.ins("i64.eq", "is target an object?");
            ctx.fb.ins("if", "target object?");
            ctx.fb.ins(&format!("local.get {}", tlo), "target object pointer");
            ctx.fb.ins("i32.wrap_i64", "target low word -> i32 ptr");
            ctx.fb.ins("i64.load offset=0", "target runtime class id");
            ctx.fb.ins(&format!("local.set {}", tcid), "target class id");
            ctx.fb.ins("i32.const 1", "target valid");
            ctx.fb.ins(&format!("local.set {}", tvalid), "target valid flag");
            ctx.fb.ins("else", "target not an object");
            ctx.fb.ins("i64.const 0", "no target class id");
            ctx.fb.ins(&format!("local.set {}", tcid), "no target class id");
            ctx.fb.ins("i32.const 0", "target invalid");
            ctx.fb.ins(&format!("local.set {}", tvalid), "target valid flag");
            ctx.fb.ins("end", "end target object test");
        }
        _ => unreachable!("target type gate handled object/mixed/union only"),
    }

    // if valid: __rt_instanceof(vp, tcid, 0) else 0; widen to i64 Bool.
    ctx.fb.ins(&format!("local.get {}", tvalid), "target valid flag");
    ctx.fb.ins("if (result i32)", "valid target -> check, else false");
    ctx.fb
        .ins(&format!("local.get {}", vp), "value receiver pointer");
    ctx.fb.ins(&format!("local.get {}", tcid), "target class id");
    ctx.fb.ins("i32.const 0", "target kind (class)");
    ctx.fb
        .ins("call $__rt_instanceof", "runtime instanceof (dynamic object target)");
    ctx.fb.ins("else", "invalid target -> false");
    ctx.fb.ins("i32.const 0", "false");
    ctx.fb.ins("end", "end dynamic instanceof");
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    store_result(ctx, inst)
}

/// Lowers the `get_class` builtin.
///
/// Zero operands: the lexical class (`rsplit_once("::")` on the current function
/// name). A known class looks up its name via `__rt_class_name_by_cid`; an unknown
/// lexical class (trait/builtin/closure) or no enclosing method yields `""`. One
/// operand: an `Object` looks up its runtime class name via `__rt_class_name_by_obj`;
/// a `Mixed`/`Union` operand is `Unsupported` (mirrors native); any other concrete
/// type yields `""` (the native-vs-PHP divergence, to be fixed cross-target later).
pub(super) fn lower_get_class(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() {
        let lexical = ctx.function.name.rsplit_once("::").map(|(c, _)| c);
        return match lexical.and_then(|c| ctx.module.class_infos.get(c)) {
            Some(ci) => {
                ctx.fb
                    .ins(&format!("i64.const {}", ci.class_id), "lexical class id");
                ctx.fb
                    .ins("call $__rt_class_name_by_cid", "lookup lexical class name");
                store_result(ctx, inst)
            }
            None => emit_empty_class_name(ctx, inst),
        };
    }

    let value = operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?;
    match value_ty {
        PhpType::Object(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("call $__rt_class_name_by_obj", "lookup runtime class name");
            store_result(ctx, inst)
        }
        PhpType::Mixed | PhpType::Union(_) => Err(WasmError::Unsupported(
            "get_class on Mixed/Union (mirrors native lower_class_name_lookup)".to_string(),
        )),
        _ => emit_empty_class_name(ctx, inst),
    }
}

/// Builds the mixed-method-call candidate list for a method name + operand count.
///
/// Scans every class whose `methods[method_key]` matches the call arity (`params`
/// plus the receiver). `ClassInfo.methods` is flattened with inherited signatures,
/// so a subclass that inherited the method is a candidate and dispatches to the
/// inherited impl via `method_impl_classes`. Sorted by `class_id` for a stable
/// if-ladder. Returns the `(class_id, runtime_class_name, impl_class)` triples: the
/// runtime class name drives vtable-slot / introducer resolution exactly like the
/// single-class `lower_method_call` path, and `impl_class` names the implementation.
pub(super) fn mixed_method_candidates(
    module: &Module,
    method_key: &str,
    operand_count: usize,
) -> Vec<(u64, String, String)> {
    let mut out: Vec<(u64, String, String)> = Vec::new();
    for (class_name, ci) in &module.class_infos {
        let Some(sig) = ci.methods.get(method_key) else { continue };
        if sig.params.len() + 1 != operand_count {
            continue;
        }
        let impl_class = ci
            .method_impl_classes
            .get(method_key)
            .cloned()
            .unwrap_or_else(|| class_name.clone());
        out.push((ci.class_id, class_name.clone(), impl_class));
    }
    out.sort_by_key(|(cid, _, _)| *cid);
    out
}

/// Computes the runtime mixed-cell tag for a concrete callee return PHP type.
///
/// Mirrors the tags `__rt_mixed_from_value` consumes: int 0, bool 3, float 2, string
/// 1, array 4, assoc 5, object 6. Other types are not boxed here.
pub(super) fn mixed_tag_for_php_type(php: &PhpType) -> Option<i64> {
    match php {
        PhpType::Int => Some(0),
        PhpType::Bool => Some(3),
        PhpType::Float => Some(2),
        PhpType::Str => Some(1),
        PhpType::Array(_) => Some(4),
        PhpType::AssocArray { .. } => Some(5),
        PhpType::Object(_) => Some(6),
        // A callable is a kind-6 descriptor carried as `WasmRepr::I64`; boxing it into a
        // Mixed cell stores the descriptor pointer under tag 10 so `__rt_mixed_from_value`
        // increfs the descriptor (shared ownership) and the tag-10 release arm dispatches
        // kind-6 -> `__rt_callable_descriptor_release` (P7a1 closure-call arg/result path).
        PhpType::Callable => Some(10),
        _ => None,
    }
}