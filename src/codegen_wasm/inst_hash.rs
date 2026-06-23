//! Purpose:
//! Lowers the EIR associative-array (hash) instructions — `HashNew`, `HashSet`,
//! and `HashGet` — to WebAssembly for the wasm32-wasi backend, materializing PHP
//! keys/values into the `(key_lo, key_hi)` / `(val_lo, val_hi, val_tag)` shapes the
//! `__rt_hash_*` runtime expects and reconstructing reads back into typed locals.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_instruction` for `Op::HashNew/HashGet/HashSet`.
//!
//! Key details:
//! - The hash runtime (`crate::codegen_wasm::hashes`) owns inbound values itself:
//!   `__rt_hash_set` persists string values and increfs container values per the
//!   per-entry `val_tag`. So this lowering passes BORROWED representations (raw
//!   ptr/len, raw container pointer) rather than persisting/increfing here — the
//!   opposite of the native backend, whose `__rt_hash_set` does not take ownership.
//! - Each entry stores its value concretely plus a per-entry runtime tag (the same
//!   `crate::codegen::runtime_value_tag` scheme as the native targets). A Mixed-valued
//!   hash therefore stores concrete entries with their own tag; reads box back into a
//!   Mixed cell on demand. Only `Mixed`/`Iterable`/`TaggedScalar` source values need a
//!   real boxing step, which lands with string keys in P5d-2.
//! - KEYS: integer/bool/float keys pass through inline; string keys are classified by
//!   `__rt_hash_normalize_key` (integer-like strings collapse to int keys); Mixed-cell
//!   keys go through `__rt_hash_key_from_mixed` (unbox + per-tag classification).
//! - REFCOUNTED READS: a `HashGet` result is EIR `MaybeOwned`, so the consumer may
//!   release it. To stay safe against that release the wasm reads return OWNED values
//!   even though the native backend returns a borrowed payload: a string element is
//!   copied with `__rt_str_persist`, a Mixed element is (re)boxed with
//!   `__rt_mixed_from_value` (which also yields a null cell on a miss, tag 8), and a
//!   container element is increfed (null-safe) before being returned. All read paths
//!   stay branchless via `select` plus null-safe runtime calls.

use super::context::{FnCtx, Result};
use super::inst::{operand, store_result, value_source_slot};
use super::values::WasmRepr;
use super::wat::ValType;
use super::WasmError;
use crate::ir::{Immediate, Instruction, IrHeapKind, IrType};
use crate::types::PhpType;

/// The in-band i64 null marker (`PHP_INT_MAX - 1`) the wasm32-wasi backend uses for
/// PHP null in scalar slots; a `HashGet` miss on a scalar-typed element yields it.
const NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;

/// Lowers `Op::HashNew`: allocates an empty ordered-map hash with the immediate
/// capacity hint and the storage value tag derived from the result's `AssocArray`
/// element type (a Mixed-valued hash uses tag 7). `__rt_hash_set`/`__rt_hash_get`
/// tolerate a 0 capacity (set resizes first, get short-circuits), so the literal
/// pair count is passed through unclamped, matching the native backend.
pub(super) fn lower_hash_new(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let capacity = match &inst.immediate {
        Some(Immediate::Capacity(c)) => *c as i64,
        _ => return Err(WasmError::Unsupported("hash_new without a capacity".to_string())),
    };
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("hash_new without a result".to_string()))?;
    let storage_value = ctx
        .function
        .value(result)
        .map(|v| v.php_type.codegen_repr())
        .map(assoc_storage_value_type)
        .unwrap_or(PhpType::Mixed);
    let value_tag = crate::codegen::runtime_value_tag(&storage_value) as i64;
    ctx.fb
        .ins(&format!("i64.const {}", capacity), "initial hash capacity");
    ctx.fb
        .ins(&format!("i64.const {}", value_tag), "hash storage value tag");
    ctx.fb
        .ins("call $__rt_hash_new", "allocate ordered-map hash");
    store_result(ctx, inst)
}

/// Lowers `Op::HashSet` (`$h[k] = v`). Materializes the key and value into temp
/// locals, computes the per-entry runtime tag, calls the copy-on-write-aware
/// `__rt_hash_set` (which may clone/resize and which persists/increfs the value),
/// then writes the returned pointer back into the hash operand's value local and its
/// source slot — mirroring `lower_array_set`. Produces no result value; the hash
/// operand IS the in/out storage.
pub(super) fn lower_hash_set(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let hash = operand(inst, 0)?;
    let key = operand(inst, 1)?;
    let value = operand(inst, 2)?;

    let storage_value = hash_storage_value_type(ctx, hash);
    let value_ty = ctx
        .function
        .value(value)
        .map(|v| v.php_type.codegen_repr())
        .unwrap_or(PhpType::Mixed);

    // A Mixed/Union/Tagged source value flowing into a concretely-typed hash needs a
    // runtime cast (`__rt_mixed_cast_*`); that lands in P5d-2. Reject it cleanly here
    // rather than mis-tagging a Mixed-cell pointer as an inline scalar.
    if !matches!(storage_value, PhpType::Mixed | PhpType::Iterable)
        && matches!(
            value_ty,
            PhpType::Mixed | PhpType::Union(_) | PhpType::TaggedScalar
        )
    {
        return Err(WasmError::Unsupported(
            "hash_set of a mixed value into a concretely-typed hash".to_string(),
        ));
    }

    let (key_lo, key_hi) = materialize_hash_key(ctx, key)?;
    let (val_lo, val_hi) = materialize_hash_value(ctx, value)?;
    let val_tag = hash_value_tag(&value_ty, &storage_value);

    // __rt_hash_set(hash, key_lo, key_hi, val_lo, val_hi, val_tag) -> hash'
    ctx.emit_load_value(hash)?;
    ctx.fb
        .ins(&format!("local.get {}", key_lo), "hash key low word");
    ctx.fb
        .ins(&format!("local.get {}", key_hi), "hash key high word");
    ctx.fb
        .ins(&format!("local.get {}", val_lo), "hash value low word");
    ctx.fb
        .ins(&format!("local.get {}", val_hi), "hash value high word");
    ctx.fb
        .ins(&format!("i64.const {}", val_tag), "hash value runtime tag");
    ctx.fb
        .ins("call $__rt_hash_set", "set hash element (COW, persists/increfs value)");

    // The runtime returned the (possibly cloned/resized) pointer: store it back into
    // the hash operand value's local and mirror it to the source slot so a later
    // LoadLocal sees the live pointer.
    ctx.emit_store_value(hash)?;
    if let Some(slot) = value_source_slot(ctx, hash) {
        let hash_ref = ctx.value_repr(hash)?.local_refs();
        let slot_ref = ctx.slot_repr(slot)?.local_refs();
        if hash_ref.len() == 1 && slot_ref.len() == 1 {
            ctx.fb
                .ins(&format!("local.get {}", hash_ref[0]), "reallocated hash pointer");
            ctx.fb
                .ins(&format!("local.set {}", slot_ref[0]), "write back to the hash slot");
        }
    }
    Ok(())
}

/// Lowers `Op::HashGet` (`$h[k]`). Materializes the key, calls `__rt_hash_get` (which
/// returns `(found, value_lo, value_hi, value_tag)`), captures all four results, then
/// reconstructs the typed result:
/// - an integer/bool element yields the raw i64 payload (the null sentinel on a miss);
/// - a float element reinterprets the payload bits (0.0 on a miss);
/// - a string element is copied into an OWNED heap string via `__rt_str_persist` (an
///   empty owned string on a miss, which the runtime handles safely);
/// - a Mixed element is (re)boxed via `__rt_mixed_from_value`: a fresh OWNED cell on a
///   hit of any tag, and a null cell on a miss (the runtime returns tag 8, so
///   `$h[missing]` boxes to PHP null);
/// - a container element (array/hash/object) is increfed (null-safe) and returned, or
///   null on a miss.
///
/// All refcounted results are returned OWNED to honor the EIR `MaybeOwned` contract on a
/// `HashGet` result, so a consumer that releases the value cannot use-after-free the
/// hash's own stored reference. Every path stays branchless via `select` plus null-safe
/// runtime calls. Tagged-scalar reads are not yet supported and return `Unsupported`.
pub(super) fn lower_hash_get(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let hash = operand(inst, 0)?;
    let key = operand(inst, 1)?;
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("hash_get without a result".to_string()))?;
    let result_repr = ctx.value_repr(result)?.clone();
    let result_ir = ctx.function.value(result).map(|v| v.ir_type);

    // Reject still-unsupported reprs before emitting the call so the stack stays balanced.
    if matches!(result_repr, WasmRepr::Tagged { .. } | WasmRepr::Void) {
        return Err(WasmError::Unsupported(format!(
            "hash_get into {:?}",
            result_repr
        )));
    }

    let (key_lo, key_hi) = materialize_hash_key(ctx, key)?;

    // __rt_hash_get(hash, key_lo, key_hi) -> (found i32, vlo i64, vhi i64, vtag i64)
    ctx.emit_load_value(hash)?;
    ctx.fb
        .ins(&format!("local.get {}", key_lo), "hash key low word");
    ctx.fb
        .ins(&format!("local.get {}", key_hi), "hash key high word");
    ctx.fb
        .ins("call $__rt_hash_get", "look up hash element");

    // Capture all four results; the value tag is on top of the stack.
    let vtag = ctx.fresh_temp(ValType::I64);
    ctx.fb.ins(&format!("local.set {}", vtag), "captured value tag");
    let vhi = ctx.fresh_temp(ValType::I64);
    ctx.fb.ins(&format!("local.set {}", vhi), "captured value high word");
    let vlo = ctx.fresh_temp(ValType::I64);
    ctx.fb.ins(&format!("local.set {}", vlo), "captured value low word");
    let found = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {}", found), "captured found flag");

    match result_repr {
        WasmRepr::I64(_) => {
            // Scalar element: raw payload on a hit, the null sentinel on a miss.
            ctx.fb.ins(&format!("local.get {}", vlo), "element payload");
            ctx.fb
                .ins(&format!("i64.const {}", NULL_SENTINEL), "null sentinel for a miss");
            ctx.fb.ins(&format!("local.get {}", found), "found flag (i32 cond)");
            ctx.fb.ins("select", "found ? payload : null sentinel");
            store_result(ctx, inst)
        }
        WasmRepr::F64(_) => {
            // Float element: reinterpret the payload bits; a miss yields 0.0.
            ctx.fb.ins(&format!("local.get {}", vlo), "element payload bits");
            ctx.fb.ins("i64.const 0", "zero bits for a miss");
            ctx.fb.ins(&format!("local.get {}", found), "found flag (i32 cond)");
            ctx.fb.ins("select", "found ? bits : 0");
            ctx.fb.ins("f64.reinterpret_i64", "payload bits -> float");
            store_result(ctx, inst)
        }
        WasmRepr::Str { .. } => {
            // Own a copy of the borrowed hash string; a miss (vlo=0, vhi=0) persists an
            // empty owned string, which __rt_str_persist handles safely.
            ctx.fb
                .ins(&format!("local.get {}", vlo), "stored string ptr (extended)");
            ctx.fb.ins("i32.wrap_i64", "string ptr -> i32");
            ctx.fb
                .ins(&format!("local.get {}", vhi), "stored string length");
            ctx.fb
                .ins("call $__rt_str_persist", "own a copy of the hash string");
            // Returns (ptr i32, len i64) — exactly the Str repr's component order.
            store_result(ctx, inst)
        }
        WasmRepr::Ptr(_) => match result_ir {
            Some(IrType::Heap(IrHeapKind::Mixed)) => {
                // Box-on-read: a fresh owned Mixed cell on any hit tag; on a miss the
                // runtime returns tag 8, so this boxes a null cell ($h[missing] is null).
                ctx.fb.ins(&format!("local.get {}", vtag), "element tag");
                ctx.fb.ins(&format!("local.get {}", vlo), "element low word");
                ctx.fb.ins(&format!("local.get {}", vhi), "element high word");
                ctx.fb
                    .ins("call $__rt_mixed_from_value", "box hash element into a Mixed cell");
                store_result(ctx, inst)
            }
            Some(IrType::Heap(_)) => {
                // Container element: incref the borrowed pointer (null-safe, a no-op on a
                // miss) so the caller owns a reference, then select null on a miss.
                let ptr = ctx.fresh_temp(ValType::I32);
                ctx.fb
                    .ins(&format!("local.get {}", vlo), "stored container ptr (extended)");
                ctx.fb.ins("i32.wrap_i64", "container ptr -> i32");
                ctx.fb
                    .ins(&format!("local.set {}", ptr), "captured container pointer");
                ctx.fb
                    .ins(&format!("local.get {}", ptr), "container pointer to retain");
                ctx.fb
                    .ins("call $__rt_incref", "retain the borrowed container (null-safe)");
                ctx.fb.ins(&format!("local.get {}", ptr), "container pointer");
                ctx.fb.ins("i32.const 0", "null pointer for a miss");
                ctx.fb.ins(&format!("local.get {}", found), "found flag (i32 cond)");
                ctx.fb.ins("select", "found ? container : null");
                store_result(ctx, inst)
            }
            _ => Err(WasmError::Unsupported(
                "hash_get into a non-heap pointer".to_string(),
            )),
        },
        // Tagged/Void were rejected above.
        _ => Err(WasmError::Unsupported(
            "hash_get result reconstruction".to_string(),
        )),
    }
}

/// Materializes a hash key into two i64 temp locals `(key_lo, key_hi)`.
///
/// Integer and boolean keys pass through with `key_hi = -1` (the int-key marker);
/// float keys truncate toward zero to an int key, matching PHP's array-key coercion.
/// String keys are classified by `__rt_hash_normalize_key` (integer-like strings
/// collapse to int keys, others keep `key_hi = len`). A Mixed-cell key is unboxed and
/// classified by `__rt_hash_key_from_mixed`. Other representations (e.g. a tagged
/// scalar, or a non-Mixed heap pointer used as an offset) return a clean `Unsupported`.
fn materialize_hash_key(ctx: &mut FnCtx, key: crate::ir::ValueId) -> Result<(String, String)> {
    let key_lo = ctx.fresh_temp(ValType::I64);
    let key_hi = ctx.fresh_temp(ValType::I64);
    let key_ir = ctx.function.value(key).map(|v| v.ir_type);
    let repr = ctx.value_repr(key)?.clone();
    match repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(key)?;
            ctx.fb
                .ins(&format!("local.set {}", key_lo), "int hash key low word");
            ctx.fb.ins("i64.const -1", "int-key marker (key_hi = -1)");
            ctx.fb
                .ins(&format!("local.set {}", key_hi), "int hash key high word");
        }
        WasmRepr::F64(_) => {
            ctx.emit_load_value(key)?;
            ctx.fb
                .ins("i64.trunc_sat_f64_s", "float key truncated toward zero to int");
            ctx.fb
                .ins(&format!("local.set {}", key_lo), "float hash key low word");
            ctx.fb.ins("i64.const -1", "int-key marker (key_hi = -1)");
            ctx.fb
                .ins(&format!("local.set {}", key_hi), "float hash key high word");
        }
        WasmRepr::Str { .. } => {
            // String key: integer-like strings collapse to int keys, others stay strings.
            ctx.emit_load_value(key)?; // ptr (i32), len (i64) — len on top
            ctx.fb
                .ins("call $__rt_hash_normalize_key", "classify string key (int-like -> int key)");
            ctx.fb
                .ins(&format!("local.set {}", key_hi), "normalized hash key high word");
            ctx.fb
                .ins(&format!("local.set {}", key_lo), "normalized hash key low word");
        }
        WasmRepr::Ptr(_) if matches!(key_ir, Some(IrType::Heap(IrHeapKind::Mixed))) => {
            // Mixed-cell key: unbox + per-tag classification in the runtime helper.
            ctx.emit_load_value(key)?; // i32 Mixed cell pointer
            ctx.fb
                .ins("call $__rt_hash_key_from_mixed", "classify a Mixed array key");
            ctx.fb
                .ins(&format!("local.set {}", key_hi), "Mixed hash key high word");
            ctx.fb
                .ins(&format!("local.set {}", key_lo), "Mixed hash key low word");
        }
        other => {
            return Err(WasmError::Unsupported(format!("hash key of {:?}", other)))
        }
    }
    Ok((key_lo, key_hi))
}

/// Materializes a hash value into two i64 temp locals `(val_lo, val_hi)`.
///
/// Scalars use the low word (the high word is unused); a float reinterprets its bits;
/// a string passes its borrowed `(ptr, len)` (the runtime persists an owned copy); a
/// heap pointer passes its borrowed pointer (the runtime increfs). The per-entry tag
/// (computed separately) tells `__rt_hash_set` which ownership step to take.
fn materialize_hash_value(ctx: &mut FnCtx, value: crate::ir::ValueId) -> Result<(String, String)> {
    let val_lo = ctx.fresh_temp(ValType::I64);
    let val_hi = ctx.fresh_temp(ValType::I64);
    let repr = ctx.value_repr(value)?.clone();
    match repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins(&format!("local.set {}", val_lo), "scalar hash value low word");
            ctx.fb.ins("i64.const 0", "scalar hash value high word unused");
            ctx.fb
                .ins(&format!("local.set {}", val_hi), "store value high word");
        }
        WasmRepr::F64(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("i64.reinterpret_f64", "float bits -> value low word");
            ctx.fb
                .ins(&format!("local.set {}", val_lo), "float hash value low word");
            ctx.fb.ins("i64.const 0", "float hash value high word unused");
            ctx.fb
                .ins(&format!("local.set {}", val_hi), "store value high word");
        }
        WasmRepr::Str { .. } => {
            // Pass the borrowed (ptr, len); __rt_hash_set persists an owned copy.
            ctx.emit_load_value(value)?; // ptr (i32), len (i64) — len on top
            ctx.fb
                .ins(&format!("local.set {}", val_hi), "string length -> value high word");
            ctx.fb.ins("i64.extend_i32_u", "string ptr -> i64 value low word");
            ctx.fb
                .ins(&format!("local.set {}", val_lo), "string ptr -> value low word");
        }
        WasmRepr::Ptr(_) => {
            // Pass the borrowed heap pointer; __rt_hash_set increfs per the tag.
            ctx.emit_load_value(value)?; // i32 pointer
            ctx.fb
                .ins("i64.extend_i32_u", "heap ptr -> i64 value low word");
            ctx.fb
                .ins(&format!("local.set {}", val_lo), "heap hash value low word");
            ctx.fb.ins("i64.const 0", "heap hash value high word unused");
            ctx.fb
                .ins(&format!("local.set {}", val_hi), "store value high word");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "hash value of {:?}",
                other
            )))
        }
    }
    Ok((val_lo, val_hi))
}

/// Returns the per-entry runtime tag for a hash-set payload, mirroring the native
/// `hash_set_value_tag`. For a Mixed/Iterable-valued hash the tag describes the source
/// value's own type (so a heterogeneous hash stores concrete entries); for a
/// concretely-typed hash the tag is the storage element type's tag.
fn hash_value_tag(value_ty: &PhpType, storage_value_ty: &PhpType) -> i64 {
    if matches!(storage_value_ty, PhpType::Mixed | PhpType::Iterable) {
        crate::codegen::runtime_value_tag(value_ty) as i64
    } else {
        crate::codegen::runtime_value_tag(storage_value_ty) as i64
    }
}

/// Extracts the storage value element type of an `AssocArray`, defaulting to Mixed for
/// any other (defensive) shape so an unknown hash is treated as heterogeneous.
fn assoc_storage_value_type(ty: PhpType) -> PhpType {
    match ty {
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        _ => PhpType::Mixed,
    }
}

/// Reads the storage value element type of the hash operand value, defaulting to Mixed.
fn hash_storage_value_type(ctx: &FnCtx, hash: crate::ir::ValueId) -> PhpType {
    ctx.function
        .value(hash)
        .map(|v| v.php_type.codegen_repr())
        .map(assoc_storage_value_type)
        .unwrap_or(PhpType::Mixed)
}

/// Emits the byte address of hash entry `cursor` — `hash + 40 + cursor*64` (a 40-byte
/// header then 64-byte entries) — into a fresh i32 temp local and returns its name.
/// `src` is the hash pointer local; `cursor` is the i64 slot-index local.
fn hash_entry_addr(ctx: &mut FnCtx, src: &str, cursor: &str) -> String {
    let entry = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.get {}", src), "hash base pointer");
    ctx.fb.ins("i32.const 40", "skip the 40-byte header");
    ctx.fb.ins("i32.add", "address of the first entry");
    ctx.fb.ins(&format!("local.get {}", cursor), "current slot index");
    ctx.fb.ins("i64.const 64", "entry stride (bytes)");
    ctx.fb.ins("i64.mul", "slot * 64");
    ctx.fb.ins("i32.wrap_i64", "byte offset -> i32");
    ctx.fb.ins("i32.add", "entry = first_entry + slot*64");
    ctx.fb.ins(&format!("local.set {}", entry), "hash entry address");
    entry
}

/// Emits a load of the i64 field at `offset` from the entry whose address is in `entry`.
fn load_entry_field(ctx: &mut FnCtx, entry: &str, offset: i32, comment: &str) {
    ctx.fb.ins(&format!("local.get {}", entry), "hash entry address");
    ctx.fb.ins(&format!("i32.const {}", offset), "field offset");
    ctx.fb.ins("i32.add", "field address");
    ctx.fb.ins("i64.load", comment);
}

/// Lowers a hash `foreach` VALUE read (`Op::IterCurrentValue` over an associative array).
/// Reads `value_lo@+24` / `value_hi@+32` / `value_tag@+40` from the entry at (`src`,
/// `cursor`) and reconstructs the result exactly as `lower_hash_get` does on a hit — no
/// miss is possible, since the cursor points at a live entry: a scalar yields the raw
/// payload, a float reinterprets it, a string is copied owned via `__rt_str_persist`, a
/// Mixed value is (re)boxed via `__rt_mixed_from_value`, and a container is increfed.
/// Refcounted results are OWNED (a foreach value variable is `MaybeOwned`).
pub(super) fn lower_hash_iter_value(
    ctx: &mut FnCtx,
    inst: &Instruction,
    src: &str,
    cursor: &str,
) -> Result<()> {
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("iter value without a result".to_string()))?;
    let result_repr = ctx.value_repr(result)?.clone();
    let result_ir = ctx.function.value(result).map(|v| v.ir_type);
    let entry = hash_entry_addr(ctx, src, cursor);
    match result_repr {
        WasmRepr::I64(_) => {
            load_entry_field(ctx, &entry, 24, "scalar element payload");
            store_result(ctx, inst)
        }
        WasmRepr::F64(_) => {
            load_entry_field(ctx, &entry, 24, "float element payload bits");
            ctx.fb.ins("f64.reinterpret_i64", "payload bits -> float");
            store_result(ctx, inst)
        }
        WasmRepr::Str { .. } => {
            load_entry_field(ctx, &entry, 24, "stored string ptr (extended)");
            ctx.fb.ins("i32.wrap_i64", "string ptr -> i32");
            load_entry_field(ctx, &entry, 32, "stored string length");
            ctx.fb
                .ins("call $__rt_str_persist", "own a copy of the hash string");
            store_result(ctx, inst)
        }
        WasmRepr::Ptr(_) => match result_ir {
            Some(IrType::Heap(IrHeapKind::Mixed)) => {
                load_entry_field(ctx, &entry, 40, "element tag");
                load_entry_field(ctx, &entry, 24, "element low word");
                load_entry_field(ctx, &entry, 32, "element high word");
                ctx.fb
                    .ins("call $__rt_mixed_from_value", "box hash element into a Mixed cell");
                store_result(ctx, inst)
            }
            Some(IrType::Heap(_)) => {
                let ptr = ctx.fresh_temp(ValType::I32);
                load_entry_field(ctx, &entry, 24, "stored container ptr (extended)");
                ctx.fb.ins("i32.wrap_i64", "container ptr -> i32");
                ctx.fb
                    .ins(&format!("local.set {}", ptr), "captured container pointer");
                ctx.fb
                    .ins(&format!("local.get {}", ptr), "container pointer to retain");
                ctx.fb
                    .ins("call $__rt_incref", "retain the borrowed container (null-safe)");
                ctx.fb.ins(&format!("local.get {}", ptr), "owned container pointer");
                store_result(ctx, inst)
            }
            _ => Err(WasmError::Unsupported(
                "iter value into a non-heap pointer".to_string(),
            )),
        },
        other => Err(WasmError::Unsupported(format!("iter value into {:?}", other))),
    }
}

/// Lowers a hash `foreach` KEY read (`Op::IterCurrentKey` over an associative array).
/// Reads `key_lo@+8` / `key_hi@+16` from the entry at (`src`, `cursor`) and reconstructs
/// the result: an int-typed key yields the raw `key_lo`; a string-typed key is copied
/// owned via `__rt_str_persist`; a Mixed/boxed key is boxed via `__rt_mixed_from_value`
/// with the tag chosen at runtime (`key_hi == -1` ⇒ int tag 0, else string tag 1), so a
/// heterogeneous int/string key set boxes correctly.
pub(super) fn lower_hash_iter_key(
    ctx: &mut FnCtx,
    inst: &Instruction,
    src: &str,
    cursor: &str,
) -> Result<()> {
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("iter key without a result".to_string()))?;
    let result_repr = ctx.value_repr(result)?.clone();
    let entry = hash_entry_addr(ctx, src, cursor);
    match result_repr {
        WasmRepr::I64(_) => {
            load_entry_field(ctx, &entry, 8, "int key value");
            store_result(ctx, inst)
        }
        WasmRepr::Str { .. } => {
            load_entry_field(ctx, &entry, 8, "stored key ptr (extended)");
            ctx.fb.ins("i32.wrap_i64", "key ptr -> i32");
            load_entry_field(ctx, &entry, 16, "stored key length");
            ctx.fb
                .ins("call $__rt_str_persist", "own a copy of the string key");
            store_result(ctx, inst)
        }
        WasmRepr::Ptr(_) | WasmRepr::Tagged { .. } => {
            // Box the key: tag = (key_hi != -1) ? 1 (string) : 0 (int).
            load_entry_field(ctx, &entry, 16, "key high word (-1 marks an int key)");
            ctx.fb.ins("i64.const -1", "int-key sentinel");
            ctx.fb.ins("i64.ne", "string key? (key_hi != -1)");
            ctx.fb
                .ins("i64.extend_i32_u", "tag = 1 (string) or 0 (int)");
            load_entry_field(ctx, &entry, 8, "key low word (int value or string ptr)");
            load_entry_field(ctx, &entry, 16, "key high word (length, ignored for int)");
            ctx.fb
                .ins("call $__rt_mixed_from_value", "box the hash key into a Mixed cell");
            store_result(ctx, inst)
        }
        other => Err(WasmError::Unsupported(format!("iter key into {:?}", other))),
    }
}
