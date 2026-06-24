//! Purpose:
//! Lowers EIR object instructions (`ObjectNew`, `PropGet`, `PropSet`) for the
//! wasm32-wasi backend and emits the kind-4 (object) refcount runtime
//! `__rt_decref_object` referenced by `__rt_decref_any`.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_instruction` dispatches the three object ops here.
//! - `crate::codegen_wasm::generate()` emits `emit_object_runtime` after the refcount runtime.
//!
//! Key details:
//! - P6a scope: scalar declared properties (int/float/bool) and no-ctor objects only.
//!   Dynamic properties, constructors, method dispatch, destructors, and refcounted
//!  property values are later sub-phases and return `WasmError::Unsupported` here so a
//!  clean diagnostic surfaces instead of silently-wrong code.
//! - Objects are heap blocks whose 16-byte header (`__rt_heap_alloc`) is stamped with
//!   heap-kind 4 at `[ptr-8]`; the payload holds `class_id` at `+0` and one 16-byte
//!   `(value_lo i64, value_hi i64)` slot per declared property (parent-first).
//! - `__rt_decref_object` is the P6a simplified release: it decrements the refcount
//!   and frees the block at zero without a property walk. This is safe only because the
//!  lowering rejects every class that could hold a refcounted property value; the
//!  non-scalar-property `Unsupported` paths and the negative scoping test lock that.

use super::context::{FnCtx, Result};
use super::inst::{data_immediate, operand, store_result};
use super::values::WasmRepr;
use super::wat::{ValType, WatModule};
use super::WasmError;
use crate::codegen_ir::{literal_default_value, LiteralDefaultValue};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

/// Registers the P6a object refcount runtime (`__rt_decref_object`) on `wm`.
///
/// Must be emitted alongside `refcount::emit_refcount_runtime`, whose
/// `__rt_decref_any` calls `__rt_decref_object` from its kind-4 branch. WAT resolves
/// `(call $name)` across the whole module regardless of definition order, so the
/// relative placement of this emitter and `emit_refcount_runtime` is cosmetic; what
/// matters is that every harness emitting `__rt_decref_any` also emits this.
pub(super) fn emit_object_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_DECREF_OBJECT);
}

/// `__rt_decref_object`: the P6a simplified object release.
///
/// Mirrors the three range guards of `__rt_decref_any` (null / below first payload /
/// at-or-after the bump cursor), then decrements the refcount held in a local. When
/// it reaches zero the block is freed via `__rt_heap_free_safe` — which itself skips
/// blocks whose header refcount is already 0, so the decremented value must NOT be
/// stored back before the free call (the free path marks refcount 0 itself). When the
/// refcount stays above zero the decremented value is stored back and the block stays
/// live. P6a objects hold only scalar properties, so no property walk and no destructor
/// are needed; P6b replaces this with the full `__rt_object_free_deep` + gc_desc walk.
const RT_DECREF_OBJECT: &str = r#"(func $__rt_decref_object (param $ptr i32)
  (local $rc i32)
  (if (i32.eqz (local.get $ptr))                  ;; guard: null pointer
    (then (return)))
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                              ;; guard: below first payload (borrowed/literal)
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))
    (then (return)))                              ;; guard: at/after bump cursor (not live)
  (local.set $rc                                  ;; rc = refcount[ptr-12] - 1 (held in local)
    (i32.add (i32.load (i32.sub (local.get $ptr) (i32.const 12))) (i32.const -1)))
  (if (i32.eqz (local.get $rc))                   ;; refcount reached zero?
    (then
      (call $__rt_heap_free_safe (local.get $ptr)) ;; free (header refcount still 1 -> free proceeds)
      (return)))                                  ;; P6a: no property walk, no destructor
  (i32.store (i32.sub (local.get $ptr) (i32.const 12)) (local.get $rc)) ;; rc > 0: store decremented refcount
  (return))
"#;

/// Returns the PHP type attached to an SSA value, read from the function's value table.
///
/// The receiver class of a `PropGet`/`PropSet` is resolved from the object operand's
/// `PhpType::Object(name)` here (not from `WasmRepr::Ptr`, which carries only the WAT
/// local name). Clones the type so no borrow on `ctx.function` is held across later
/// `&mut self` lowering calls.
fn value_php_type(ctx: &FnCtx, v: ValueId) -> Result<PhpType> {
    Ok(ctx
        .function
        .value(v)
        .ok_or_else(|| WasmError::Unsupported(format!("no SSA value {:?}", v)))?
        .php_type
        .clone())
}

/// Resolves the `ClassInfo` for a receiver's PHP object type, rejecting non-object
/// receivers with a clean `Unsupported` error.
fn receiver_class_info(ctx: &FnCtx, object: ValueId) -> Result<crate::types::ClassInfo> {
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
fn resolve_property_slot(
    ci: &crate::types::ClassInfo,
    property: &str,
) -> Result<(usize, usize, PhpType)> {
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

/// Lowers `Op::ObjectNew` (P6a: no constructor) to an inline heap allocation.
///
/// Allocates `8 + n*16` payload bytes via `__rt_heap_alloc`, stamps heap-kind 4 at
/// `[obj-8]`, writes the compile-time `class_id` at `[obj+0]`, zeroes every property
/// slot, then emits scalar (int/float/bool/null) property defaults. Rejects
/// `#[AllowDynamicProperties]` classes and constructor calls with `Unsupported`.
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
/// Int/Bool store the value as i64 and zero the tag word; Float stores the raw f64
/// bits as i64 (read back by `f64.load`) and zeroes the tag; Null leaves the slot
/// zeroed (already written by the zeroing loop). Any other `LiteralDefaultValue`
/// variant (string, boxed, array, sentinel) is rejected as a P6b concern.
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

/// Lowers `Op::PropGet` of a declared scalar property to a direct memory load.
///
/// Pushes the object pointer, then loads the property's value_lo as i64 (int/bool) or
/// f64 (float) using the `offset=` immediate. Non-scalar properties, undeclared
/// properties, and non-object receivers return `Unsupported`. The loaded scalar is
/// stored into the result value's local; scalars are unrefcounted, so no incref.
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
        other => {
            return Err(WasmError::Unsupported(format!(
                "non-scalar property ${} of type {:?} not yet supported on wasm32-wasi",
                property, other
            )))
        }
    }

    store_result(ctx, inst)?;
    Ok(())
}

/// Lowers `Op::PropSet` of a declared scalar property to a direct memory store.
///
/// `PropSet` is void with operands `[object, value]`. Pushes the object pointer, the
/// value (i64 for int/bool, f64 for float), and stores at the property offset, then
/// zeroes the tag word. Non-scalar properties and non-object receivers return
/// `Unsupported`. The object pointer is unchanged by a scalar write, so no write-back
/// to the object value local is needed (unlike `ArraySet`/`HashSet`).
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
        other => {
            return Err(WasmError::Unsupported(format!(
                "non-scalar property ${} of type {:?} not yet supported on wasm32-wasi",
                property, other
            )))
        }
    }

    Ok(())
}