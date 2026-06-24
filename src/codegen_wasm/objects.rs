//! Purpose:
//! Lowers EIR object instructions (`ObjectNew`, `PropGet`, `PropSet`) for the
//! wasm32-wasi backend and emits the kind-4 (object) refcount runtime
//! `__rt_decref_object` referenced by `__rt_decref_any`.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_instruction` dispatches the three object ops here.
//! - `crate::codegen_wasm::generate()` emits `emit_object_runtime` after the refcount runtime
//!   and `emit_gc_desc_table` before the heap base is computed.
//!
//! Key details:
//! - P6b scope: declared properties of scalar, string, container (array/hash/object), and
//!   mixed/union/iterable type; no-ctor objects only. Dynamic properties, constructors,
//!   method dispatch, and destructors are later sub-phases and return `WasmError::Unsupported`.
//! - Objects are heap blocks whose 16-byte header (`__rt_heap_alloc`) is stamped with
//!   heap-kind 4 at `[ptr-8]`; the payload holds `class_id` at `+0` and one 16-byte
//!   `(value_lo i64, value_hi i64)` slot per declared property (parent-first). The hi word is
//!   the runtime value tag for refcounted slots (4/5/6/7) or the string length for Str, and 0
//!   for scalars.
//! - `__rt_decref_object` performs the full release: a refcount==0 re-entrancy guard, mark-zero,
//!   then a gc_desc-driven property walk that releases each refcounted slot value (desc tag in
//!   {1,4,5,6,7}) before freeing the block via `__rt_heap_free` (unsafe; refcount is already 0).
//!   The property count is derived from the object's own size header (`n = (size-8) >> 4`), not
//!   from a terminator, so a scalar-then-refcounted property ordering is handled correctly.
//!   The gc_desc table is emitted by `emit_gc_desc_table` (one tag byte per property, indexed by
//!   `class_id`); `emit_gc_desc_stub` declares empty-table globals for unit-test harnesses that
//!   register no classes (the `cid < count` check is then false for every cid and the walk is
//!   skipped, which is correct for harness blocks holding no refcounted property values).
//! - PropGet returns an OWNED value (persist/incref) so the MaybeOwned read result is
//!   independent of the object; PropSet releases the previous slot value (null-safe), retains or
//!   persists the incoming value, and stores lo + hi. Mixed slots split into MOVE (incoming is
//!   already a Mixed cell) and BOX (`emit_box_value_into_mixed`).

use super::context::{FnCtx, Result};
use super::inst::{data_immediate, operand, store_result};
use super::values::WasmRepr;
use super::wat::{DataSegment, Global, ValType, WatModule};
use super::WasmError;
use crate::codegen_ir::{literal_default_value, LiteralDefaultValue};
use crate::ir::{Instruction, IrHeapKind, IrType, ValueId};
use crate::types::{ClassInfo, PhpType};
use std::collections::HashMap;

/// Registers the object refcount runtime (`__rt_decref_object`) on `wm`.
///
/// Must be emitted alongside `refcount::emit_refcount_runtime`, whose `__rt_decref_any` calls
/// `__rt_decref_object` from its kind-4 branch. The function references the `$__gc_desc_ptrs`
/// and `$__gc_desc_count` globals, so every module emitting this runtime must also emit either
/// `emit_gc_desc_table` (real programs, via `generate()`) or `emit_gc_desc_stub` (unit-test
/// harnesses) so the globals exist and the WAT validates. WAT resolves `(call $name)` across
/// the whole module regardless of definition order, so the relative placement of this emitter
/// and `emit_refcount_runtime` is cosmetic; what matters is that every harness emitting
/// `__rt_decref_any` also emits this plus a gc_desc global pair.
pub(super) fn emit_object_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_DECREF_OBJECT);
}

/// Declares empty-table `$__gc_desc_ptrs` / `$__gc_desc_count` globals for unit-test harnesses
/// that register no classes.
///
/// With `count = 0` the `cid < count` range check in `__rt_decref_object` is false for every
/// class id, so the property walk is skipped and the object is freed shallowly. That is correct
/// for harness blocks that hold no refcounted property values (the P6a object tests). Real
/// programs use `emit_gc_desc_table` instead, which emits the actual per-class descriptors.
#[cfg(test)]
pub(super) fn emit_gc_desc_stub(wm: &mut WatModule) {
    wm.add_global(Global {
        name: "__gc_desc_ptrs".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: 0,
    });
    wm.add_global(Global {
        name: "__gc_desc_count".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: 0,
    });
}

/// Emits the per-class gc_desc data (one runtime tag byte per property), the class-indexed
/// pointer table, and the `$__gc_desc_ptrs` / `$__gc_desc_count` globals, then returns the
/// advanced static-data cursor.
///
/// Mirrors the native `_class_gc_desc_ptrs` / `_class_gc_desc_<id>` tables. Each known class id
/// gets a descriptor of exactly `n_properties` tag bytes (a single `0x00` for an empty class,
/// never read since `n = 0`); gap ids in `0..=max_id` point at a generous zero-filled "missing"
/// descriptor so a freed object whose class id lands on a gap reads tag 0 (skip) for every slot
/// without an out-of-bounds load. The pointer table is 4-aligned so its i32 entries load cleanly.
/// `generate()` calls this after the string-literal data and before computing `heap_base`, so
/// the descriptor data lives in static memory below the heap and is never overwritten by
/// allocation.
pub(super) fn emit_gc_desc_table(
    wm: &mut WatModule,
    class_infos: &HashMap<String, ClassInfo>,
    mut cursor: u32,
) -> u32 {
    // 4-align the cursor for the descriptor and pointer-table data that follows.
    cursor = (cursor + 3) & !3;
    if class_infos.is_empty() {
        // No classes: declare empty-table globals so the walk's `cid < count` check is false
        // for every cid and the property walk is skipped.
        wm.add_global(Global {
            name: "__gc_desc_ptrs".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: 0,
        });
        wm.add_global(Global {
            name: "__gc_desc_count".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: 0,
        });
        return cursor;
    }
    let id_to_ci: HashMap<u64, &ClassInfo> =
        class_infos.values().map(|ci| (ci.class_id, ci)).collect();
    let max_id = class_infos.values().map(|ci| ci.class_id).max().unwrap_or(0);
    let count = max_id + 1;
    // Missing/gap descriptor: a zero run so a gap class id reads tag 0 (skip) for every slot.
    let missing_off = cursor;
    wm.add_data(DataSegment {
        offset: missing_off,
        bytes: vec![0u8; 64],
    });
    cursor += 64;
    // Per-class descriptor bytes (one runtime tag per property, in declared/parent-first order).
    let mut desc_off: HashMap<u64, u32> = HashMap::new();
    for cid in 0..=max_id {
        match id_to_ci.get(&cid) {
            Some(ci) => {
                let mut bytes = Vec::with_capacity(ci.properties.len().max(1));
                for (name, ty) in &ci.properties {
                    bytes.push(if ci.reference_properties.contains(name) {
                        0
                    } else {
                        gc_desc_tag(ty)
                    });
                }
                if bytes.is_empty() {
                    // Empty class: a single 0x00 byte (n = 0, so the walk never reads it).
                    bytes.push(0);
                }
                wm.add_data(DataSegment {
                    offset: cursor,
                    bytes: bytes.clone(),
                });
                desc_off.insert(cid, cursor);
                cursor += bytes.len() as u32;
            }
            None => {
                desc_off.insert(cid, missing_off);
            }
        }
    }
    // 4-align for the i32 pointer table, then emit one entry per class id in 0..=max_id.
    cursor = (cursor + 3) & !3;
    let ptrs_off = cursor;
    let mut ptrs_bytes = Vec::with_capacity(count as usize * 4);
    for cid in 0..=max_id {
        let off = desc_off.get(&cid).copied().unwrap_or(missing_off);
        ptrs_bytes.extend_from_slice(&off.to_le_bytes());
    }
    wm.add_data(DataSegment {
        offset: ptrs_off,
        bytes: ptrs_bytes,
    });
    cursor += (count * 4) as u32;
    wm.add_global(Global {
        name: "__gc_desc_ptrs".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: ptrs_off as i64,
    });
    wm.add_global(Global {
        name: "__gc_desc_count".to_string(),
        ty: ValType::I32,
        mutable: false,
        init: count as i64,
    });
    cursor
}

/// Maps a property's PHP type to the single-byte runtime tag stored in its gc_desc entry.
///
/// Matches the native `_class_gc_desc_<id>` tag mapping exactly: Int=0, Str=1, Float=2, Bool=3,
/// Array=4, AssocArray=5, Object=6, Mixed/Union/Iterable=7, Resource=9, and 0 for the remaining
/// non-refcounted kinds (Callable/Pointer/Buffer/Packed/Never/Void/Null). `TaggedScalar` maps to
/// 7 (nullable scalars are boxed as Mixed on the wasm target) rather than panicking like the
/// native emitter, which never observes `TaggedScalar` as a declared property type.
fn gc_desc_tag(ty: &PhpType) -> u8 {
    match ty {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable | PhpType::TaggedScalar => 7,
        PhpType::Resource(_) => 9,
        _ => 0,
    }
}

/// `__rt_decref_object`: the full object release with a gc_desc-driven property walk.
///
/// Mirrors `__rt_object_free_deep` from the native backend (minus the Fiber/Generator/SPL special
/// cases and the destructor hook, which are later sub-phases). Guards null / below-first-payload
/// / at-or-after-cursor like `__rt_decref_any`, then a refcount==0 re-entrancy guard. On reaching
/// zero it marks the refcount 0 (so any nested release during the walk is a no-op), derives the
/// property count `n = (size[ptr-16] - 8) >> 4` from the object's own size header, and — when
/// `class_id < $__gc_desc_count` — walks `i in 0..n` releasing each slot whose desc tag is in
/// {1,4,5,6,7} (str/array/hash/object/mixed) via the null-safe, kind-dispatched `__rt_decref_any`.
/// Resource slots (tag 9) and scalars (tag 0/2/3) are deliberately skipped. Finally the block is
/// freed with `__rt_heap_free` (unsafe, no refcount guard) since the refcount is already 0.
const RT_DECREF_OBJECT: &str = r#"(func $__rt_decref_object (param $ptr i32)
  (local $rc i32) (local $n i32) (local $cid i32) (local $desc i32) (local $i i32) (local $tag i32) (local $slot i32)
  (if (i32.eqz (local.get $ptr)) (then (return)))                    ;; guard: null pointer
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                                                  ;; guard: below first payload (borrowed/literal)
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))
    (then (return)))                                                  ;; guard: at/after bump cursor (not live)
  (local.set $rc (i32.load (i32.sub (local.get $ptr) (i32.const 12))))  ;; refcount = [ptr-12]
  (if (i32.eqz (local.get $rc)) (then (return)))                    ;; guard: refcount == 0 (re-entrancy)
  (local.set $rc (i32.add (local.get $rc) (i32.const -1)))          ;; rc = rc - 1
  (if (i32.ne (local.get $rc) (i32.const 0)) (then                  ;; keep path: rc > 0, store and return
    (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (local.get $rc))  ;; store decremented refcount
    (return)))                                                       ;; keep live and return
  (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (i32.const 0))  ;; mark refcount 0 (re-entrancy guard)
  (local.set $cid (i32.wrap_i64 (i64.load (local.get $ptr))))    ;; class_id = [ptr+0] (i64 -> i32)
  (local.set $n (i32.shr_u (i32.sub (i32.load (i32.sub (local.get $ptr) (i32.const 16))) (i32.const 8)) (i32.const 4)))  ;; n = (size-8) >> 4
  (if (i32.lt_u (local.get $cid) (global.get $__gc_desc_count)) (then  ;; class_id within the descriptor table?
    (local.set $desc (i32.load (i32.add (global.get $__gc_desc_ptrs) (i32.mul (local.get $cid) (i32.const 4)))))  ;; desc = ptrs[cid]
    (local.set $i (i32.const 0))                                ;; property index = 0
    (block $walk_end
      (loop $walk
        (br_if $walk_end (i32.ge_u (local.get $i) (local.get $n)))  ;; i >= n -> end walk
        (local.set $tag (i32.load8_u (i32.add (local.get $desc) (local.get $i))))  ;; tag = desc[i]
        (if (i32.or (i32.eq (local.get $tag) (i32.const 1)) (i32.and (i32.ge_u (local.get $tag) (i32.const 4)) (i32.le_u (local.get $tag) (i32.const 7)))) (then  ;; tag in {1,4,5,6,7} -> release the slot
          (local.set $slot (i32.load offset=8 (i32.add (local.get $ptr) (i32.mul (local.get $i) (i32.const 16)))))  ;; slot ptr = [ptr + 8 + i*16]
          (call $__rt_decref_any (local.get $slot))            ;; release the child cell (null-safe, kind-dispatched)
        )                                                      ;; close then
        )                                                      ;; close if (tag check)
        (local.set $i (i32.add (local.get $i) (i32.const 1)))   ;; i++
        (br $walk)                                             ;; loop back
      )                                                        ;; close loop $walk
    )                                                          ;; close block $walk_end
  )                                                            ;; close then (cid < count)
  )                                                            ;; close if (cid < count)
  (call $__rt_heap_free (local.get $ptr))                         ;; free the object storage (unsafe: refcount already 0)
  (return)                                                    ;; top-level return
)                                                              ;; close func
"#;

/// Returns the PHP type attached to an SSA value, read from the function's value table.
///
/// The receiver class of a `PropGet`/`PropSet` is resolved from the object operand's
/// `PhpType::Object(name)` here (not from `WasmRepr::Ptr`, which carries only the WAT local
/// name). Clones the type so no borrow on `ctx.function` is held across later `&mut self`
/// lowering calls.
fn value_php_type(ctx: &FnCtx, v: ValueId) -> Result<PhpType> {
    Ok(ctx
        .function
        .value(v)
        .ok_or_else(|| WasmError::Unsupported(format!("no SSA value {:?}", v)))?
        .php_type
        .clone())
}

/// Resolves the `ClassInfo` for a receiver's PHP object type, rejecting non-object receivers
/// with a clean `Unsupported` error.
fn receiver_class_info(ctx: &FnCtx, object: ValueId) -> Result<ClassInfo> {
    let php_type = value_php_type(ctx, object)?;
    let class_name = match php_type {
        PhpType::Object(name) => name,
        other => {
            return Err(WasmError::Unsupported(format!(
                "property access on non-object receiver {:?}",
                other
            )))
        }
    };
    ctx.module
        .class_infos
        .get(&class_name)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", class_name)))
}

/// Looks up a declared property's `(index, byte_offset, php_type)` on `ci`, rejecting
/// undeclared properties (dynamic/`__get` are later sub-phases).
fn resolve_property_slot(ci: &ClassInfo, property: &str) -> Result<(usize, usize, PhpType)> {
    let index = ci
        .properties
        .iter()
        .position(|(p, _)| p == property)
        .ok_or_else(|| WasmError::Unsupported(format!("property ${} not declared", property)))?;
    let offset = ci
        .property_offsets
        .get(property)
        .copied()
        .unwrap_or(8 + index * 16);
    Ok((index, offset, ci.properties[index].1.clone()))
}

/// Loads the object pointer local ref for `object`, rejecting a non-pointer repr.
fn object_ptr_ref(ctx: &FnCtx, object: ValueId) -> Result<String> {
    let repr = ctx.value_repr(object)?.clone();
    match repr {
        WasmRepr::Ptr(name) => Ok(name),
        other => Err(WasmError::Unsupported(format!(
            "object value is not a pointer: {:?}",
            other
        ))),
    }
}

/// Lowers `Op::ObjectNew` (no constructor) to an inline heap allocation.
///
/// Allocates `8 + n*16` payload bytes via `__rt_heap_alloc`, stamps heap-kind 4 at `[obj-8]`,
/// writes the compile-time `class_id` at `[obj+0]`, zeroes every property slot, then emits
/// scalar (int/float/bool/null) property defaults. Rejects `#[AllowDynamicProperties]` classes
/// and constructor calls with `Unsupported`. Non-scalar property defaults (string/container/
/// mixed) are a follow-up sub-phase and surface as `Unsupported` from `emit_scalar_default`.
pub(super) fn lower_object_new(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let class_data = data_immediate(inst)?;
    let class_name = ctx
        .module
        .data
        .class_names
        .get(class_data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("invalid class data id {:?}", class_data)))?;
    let ci = ctx
        .module
        .class_infos
        .get(&class_name)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", class_name)))?;
    if ci.allow_dynamic_properties {
        return Err(WasmError::Unsupported(format!(
            "dynamic properties not yet supported on wasm32-wasi for class {}",
            class_name
        )));
    }
    if !inst.operands.is_empty() {
        return Err(WasmError::Unsupported(format!(
            "object constructor not yet supported on wasm32-wasi for class {}",
            class_name
        )));
    }

    let n = ci.properties.len();
    let payload_size: i32 = 8 + (n as i32) * 16;
    let class_id = ci.class_id as i64;

    // -- allocate the object block --
    ctx.fb.ins(&format!("i32.const {}", payload_size), "object payload size in bytes");
    ctx.fb.ins("call $__rt_heap_alloc", "allocate object block -> ptr (i32)");
    let obj = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", obj), "capture fresh object pointer");

    // -- stamp the heap header kind word (kind 4 = object) at [obj-8] --
    ctx.fb.ins(&format!("local.get {}", obj), "object base address");
    ctx.fb.ins("i32.const 8", "header kind word offset (ptr - 8)");
    ctx.fb.ins("i32.sub", "address of the kind word");
    ctx.fb.ins("i64.const 4", "heap kind 4 = object instance");
    ctx.fb.ins("i64.store", "stamp the heap header kind word");

    // -- write the compile-time class id at [obj+0] --
    ctx.fb.ins(&format!("local.get {}", obj), "object base address");
    ctx.fb.ins(&format!("i64.const {}", class_id), "compile-time class id");
    ctx.fb.ins("i64.store", "store class id at object payload offset zero");

    // -- zero every property slot (value_lo and the tag value_hi) --
    for i in 0..n {
        let off = 8 + i * 16;
        ctx.fb.ins(&format!("local.get {}", obj), "object base address");
        ctx.fb.ins("i64.const 0", "zero");
        ctx.fb.ins(&format!("i64.store offset={}", off), "zero property slot value_lo");
        ctx.fb.ins(&format!("local.get {}", obj), "object base address");
        ctx.fb.ins("i64.const 0", "zero");
        ctx.fb.ins(&format!("i64.store offset={}", off + 8), "zero property slot value_hi (tag)");
    }

    // -- emit scalar property defaults (int/float/bool/null); non-scalar -> Unsupported --
    for i in 0..n {
        let Some(Some(expr)) = ci.defaults.get(i) else {
            continue;
        };
        let prop_name = ci.properties[i].0.clone();
        let prop_type = ci.properties[i].1.clone();
        let off = ci
            .property_offsets
            .get(&prop_name)
            .copied()
            .unwrap_or(8 + i * 16);
        let lit = literal_default_value(
            &format!("property ${}", prop_name),
            &prop_type,
            &expr.kind,
            "ObjectNew",
        )
        .map_err(|e| WasmError::Unsupported(e.to_string()))?;
        emit_scalar_default(ctx, &obj, off, &lit, &prop_name)?;
    }

    // -- store the object pointer into the result value's local --
    ctx.fb.ins(&format!("local.get {}", obj), "reload object pointer for result store");
    store_result(ctx, inst)?;
    Ok(())
}

/// Writes one scalar property default into the object slot at `offset`.
///
/// Int/Bool store the value as i64 and zero the tag word; Float stores the raw f64 bits as i64
/// (read back by `f64.load`) and zeroes the tag; Null leaves the slot zeroed (already written by
/// the zeroing loop). Any other `LiteralDefaultValue` variant (string, boxed, array, sentinel) is
/// rejected as a follow-up sub-phase concern.
fn emit_scalar_default(
    ctx: &mut FnCtx,
    obj: &str,
    offset: usize,
    lit: &LiteralDefaultValue,
    prop_name: &str,
) -> Result<()> {
    match lit {
        LiteralDefaultValue::Int(v) => {
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i64.const {}", v), "int default value");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store int default value_lo");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 0", "zero tag word");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store int default value_hi (tag = 0)");
        }
        LiteralDefaultValue::Bool(b) => {
            let v: i64 = i64::from(*b);
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i64.const {}", v), "bool default value");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store bool default value_lo");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 0", "zero tag word");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store bool default value_hi (tag = 0)");
        }
        LiteralDefaultValue::Float(f) => {
            let bits = f.to_bits() as i64;
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins(&format!("i64.const {}", bits), "float default bits");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store float default value_lo");
            ctx.fb.ins(&format!("local.get {}", obj), "object base address");
            ctx.fb.ins("i64.const 0", "zero tag word");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store float default value_hi (tag = 0)");
        }
        LiteralDefaultValue::Null => {
            // The zeroing loop already wrote (0, 0); skip.
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "non-scalar default for property ${} on wasm32-wasi (kind {:?})",
                prop_name, std::mem::discriminant(other)
            )))
        }
    }
    Ok(())
}

/// Boxes a scalar / string / container value into a fresh owned kind-5 Mixed cell via
/// `__rt_mixed_from_value`, leaving the cell pointer (i32) on the WASM operand stack and
/// returning the temp local name holding it.
///
/// Reused by the Mixed-property BOX sub-case of `lower_prop_set`. A value that is already a
/// Mixed cell (`Ptr` with `ir_type == Heap(Mixed)`) is NOT handled here — the MOVE path stores it
/// directly. Returns `Unsupported` for Tagged/Void/non-heap pointers.
fn emit_box_value_into_mixed(ctx: &mut FnCtx, value: ValueId) -> Result<String> {
    let repr = ctx.value_repr(value)?.clone();
    let php = ctx.function.value(value).map(|v| v.php_type.codegen_repr());
    let ir = ctx.function.value(value).map(|v| v.ir_type);
    match &repr {
        WasmRepr::I64(local) => {
            let tag: i64 = if matches!(php, Some(PhpType::Bool)) { 3 } else { 0 };
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (int/bool)");
            ctx.fb.ins(&format!("local.get {}", local), "scalar value -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box scalar into a mixed cell");
        }
        WasmRepr::F64(local) => {
            ctx.fb.ins("i64.const 2", "mixed tag (float)");
            ctx.fb.ins(&format!("local.get {}", local), "float value (f64)");
            ctx.fb.ins("i64.reinterpret_f64", "float bits -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box float into a mixed cell");
        }
        WasmRepr::Str { ptr, len } => {
            ctx.fb.ins("i64.const 1", "mixed tag (string)");
            ctx.fb.ins(&format!("local.get {}", ptr), "string pointer");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
            ctx.fb.ins(&format!("local.get {}", len), "string length -> hi");
            ctx.fb.ins("call $__rt_mixed_from_value", "box string (persists a copy)");
        }
        WasmRepr::Ptr(local) => {
            let tag: i64 = match ir {
                Some(IrType::Heap(IrHeapKind::Array)) => 4,
                Some(IrType::Heap(IrHeapKind::Hash)) => 5,
                Some(IrType::Heap(IrHeapKind::Object)) => 6,
                Some(IrType::Heap(IrHeapKind::Mixed)) => {
                    return Err(WasmError::Unsupported(
                        "box of already-mixed value not handled by emit_box_value_into_mixed (MOVE path stores directly)".to_string(),
                    ))
                }
                _ => {
                    return Err(WasmError::Unsupported(
                        "box of a non-heap pointer into a mixed cell not supported on wasm32-wasi"
                            .to_string(),
                    ))
                }
            };
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (heap kind)");
            ctx.fb.ins(&format!("local.get {}", local), "heap pointer");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box heap value (increfs the child)");
        }
        _ => {
            return Err(WasmError::Unsupported(
                "box of a tagged/void value into a mixed cell not supported on wasm32-wasi".to_string(),
            ))
        }
    }
    let cell_tmp = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", cell_tmp), "save fresh kind-5 cell ptr");
    Ok(cell_tmp)
}

/// Lowers `Op::PropGet` of a declared property to a direct memory load that returns an OWNED value.
///
/// Scalar arms (Int/Bool/Float) load directly with no incref (unrefcounted). Refcounted arms
/// (Str, Array/AssocArray/Object, Mixed/Union/Iterable) load the slot's child pointer, incref it
/// (so the read result is independent of the object and may be Released by the ownership pass),
/// then `store_result`. Str reads the persisted-string copy via `__rt_str_persist`. Non-object
/// receivers, undeclared properties, Resource, and Tagged/Void results return `Unsupported`.
pub(super) fn lower_prop_get(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let object = operand(inst, 0)?;
    let prop_data = data_immediate(inst)?;
    let property = ctx
        .module
        .data
        .strings
        .get(prop_data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("invalid string data id {:?}", prop_data)))?;

    let ci = receiver_class_info(ctx, object)?;
    let (_, offset, prop_type) = resolve_property_slot(&ci, &property)?;
    let obj_ref = object_ptr_ref(ctx, object)?;

    match &prop_type {
        PhpType::Int | PhpType::Bool => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i64.load offset={}", offset), "load scalar property value_lo");
        }
        PhpType::Float => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("f64.load offset={}", offset), "load float property value_lo");
        }
        PhpType::Str => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load string property ptr (lo)");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i64.load offset={}", offset + 8), "load string property len (hi)");
            ctx.fb.ins("call $__rt_str_persist", "persist string copy (ptr,len) -> (new_ptr,new_len)");
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load container property ptr (lo)");
            let ptr_tmp = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", ptr_tmp), "save container cell ptr");
            ctx.fb.ins(&format!("local.get {}", ptr_tmp), "container cell ptr");
            ctx.fb.ins("call $__rt_incref", "retain returned container cell (refcount++)");
            ctx.fb.ins(&format!("local.get {}", ptr_tmp), "container cell ptr for result");
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load mixed property cell ptr (lo)");
            let cell_tmp = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", cell_tmp), "save mixed cell ptr");
            ctx.fb.ins(&format!("local.get {}", cell_tmp), "mixed cell ptr");
            ctx.fb.ins("call $__rt_incref", "retain returned mixed cell (refcount++)");
            ctx.fb.ins(&format!("local.get {}", cell_tmp), "mixed cell ptr for result");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "property ${} of type {:?} not yet supported on wasm32-wasi",
                property, other
            )))
        }
    }

    store_result(ctx, inst)?;
    Ok(())
}

/// Lowers `Op::PropSet` of a declared property slot to a direct memory store.
///
/// `PropSet` is void with operands `[object, value]`. Scalar arms (Int/Bool/Float) write the
/// value directly and zero the tag word. Refcounted arms (Str, Array/AssocArray/Object,
/// Mixed/Union/Iterable) first release the previous slot value (null-safe via
/// `__rt_decref_any`), then retain + persist the incoming value and store lo (as i64) plus the
/// hi-word (runtime tag, or string length). The Mixed/Union/Iterable slot splits into MOVE (the
/// incoming value is already a Mixed cell, stored without incref) and BOX (anything else, boxed
/// via `emit_box_value_into_mixed`). The ownership pass handles the source temp release; the
/// backend just stores. Non-object receivers, undeclared properties, and Resource values return
/// `Unsupported`.
pub(super) fn lower_prop_set(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let object = operand(inst, 0)?;
    let value = operand(inst, 1)?;
    let prop_data = data_immediate(inst)?;
    let property = ctx
        .module
        .data
        .strings
        .get(prop_data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("invalid string data id {:?}", prop_data)))?;

    let ci = receiver_class_info(ctx, object)?;
    let (_, offset, prop_type) = resolve_property_slot(&ci, &property)?;
    let obj_ref = object_ptr_ref(ctx, object)?;

    match &prop_type {
        PhpType::Int | PhpType::Bool => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.emit_load_value(value)?;
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store scalar property value_lo");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins("i64.const 0", "zero tag word");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store property value_hi (tag = 0)");
        }
        PhpType::Float => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.emit_load_value(value)?;
            ctx.fb.ins(&format!("f64.store offset={}", offset), "store float property value_lo");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins("i64.const 0", "zero tag word");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store property value_hi (tag = 0)");
        }
        PhpType::Str => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load previous slot value ptr");
            ctx.fb.ins("call $__rt_decref_any", "release previous slot value (null-safe)");
            ctx.emit_load_value(value)?;
            ctx.fb.ins("call $__rt_str_persist", "persist string copy (ptr,len) -> (new_ptr,new_len)");
            let len_tmp = ctx.fresh_temp(ValType::I64);
            let ptr_tmp = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", len_tmp), "save persisted string len");
            ctx.fb.ins(&format!("local.set {}", ptr_tmp), "save persisted string ptr");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("local.get {}", ptr_tmp), "persisted string ptr");
            ctx.fb.ins("i64.extend_i32_u", "widen string ptr to i64 lo");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store string property value_lo (ptr)");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("local.get {}", len_tmp), "persisted string len");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store string property value_hi (len)");
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load previous slot value ptr");
            ctx.fb.ins("call $__rt_decref_any", "release previous slot value (null-safe)");
            ctx.emit_load_value(value)?;
            let ptr_tmp = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", ptr_tmp), "save container cell ptr");
            ctx.fb.ins(&format!("local.get {}", ptr_tmp), "container cell ptr");
            ctx.fb.ins("call $__rt_incref", "retain new container cell (refcount++)");
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("local.get {}", ptr_tmp), "container cell ptr");
            ctx.fb.ins("i64.extend_i32_u", "widen cell ptr to i64 lo");
            ctx.fb.ins(&format!("i64.store offset={}", offset), "store container property value_lo (ptr)");
            let tag = crate::codegen::runtime_value_tag(&prop_type) as i64;
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i64.const {}", tag), "container runtime tag (hi word)");
            ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store container property value_hi (tag)");
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => {
            ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
            ctx.fb.ins(&format!("i32.load offset={}", offset), "load previous slot value ptr");
            ctx.fb.ins("call $__rt_decref_any", "release previous slot value (null-safe)");
            let value_repr = ctx.value_repr(value)?.clone();
            let value_ir_type = ctx.function.value(value).map(|v| v.ir_type);
            let is_move = matches!(&value_repr, WasmRepr::Ptr(_))
                && matches!(value_ir_type, Some(IrType::Heap(IrHeapKind::Mixed)));
            if is_move {
                ctx.emit_load_value(value)?;
                let cell_tmp = ctx.fresh_temp(ValType::I32);
                ctx.fb.ins(&format!("local.set {}", cell_tmp), "save mixed cell ptr (MOVE)");
                ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
                ctx.fb.ins(&format!("local.get {}", cell_tmp), "moved mixed cell ptr");
                ctx.fb.ins("i64.extend_i32_u", "widen cell ptr to i64 lo");
                ctx.fb.ins(&format!("i64.store offset={}", offset), "store mixed property value_lo (ptr)");
                ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
                ctx.fb.ins("i64.const 7", "mixed runtime tag (hi word)");
                ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store mixed property value_hi (tag)");
            } else {
                let cell_tmp = emit_box_value_into_mixed(ctx, value)?;
                ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
                ctx.fb.ins(&format!("local.get {}", cell_tmp), "fresh kind-5 cell ptr");
                ctx.fb.ins("i64.extend_i32_u", "widen cell ptr to i64 lo");
                ctx.fb.ins(&format!("i64.store offset={}", offset), "store mixed property value_lo (ptr)");
                ctx.fb.ins(&format!("local.get {}", obj_ref), "object base address");
                ctx.fb.ins("i64.const 7", "mixed runtime tag (hi word)");
                ctx.fb.ins(&format!("i64.store offset={}", offset + 8), "store mixed property value_hi (tag)");
            }
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "property ${} of type {:?} not yet supported on wasm32-wasi",
                property, other
            )))
        }
    }

    Ok(())
}