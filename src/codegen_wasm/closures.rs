//! Purpose:
//! Lowers EIR closure / callable instructions for the wasm32-wasi backend and emits
//! the kind-6 (callable descriptor) refcount runtime `__rt_callable_descriptor_release`
//! referenced by `__rt_decref_any`, plus the P7a1 no-capture create/call surface
//! (`ClosureNew` / `ClosureCall` / per-closure wrappers / `__rt_closure_call`).
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` emits `emit_closure_runtime` right after the
//!   refcount runtime, because `__rt_decref_any`'s kind-6 branch calls
//!   `__rt_callable_descriptor_release` and WAT requires every call target to be
//!   defined for the module to validate.
//! - `crate::codegen_wasm::generate()` emits `emit_closure_dispatch` after the closure
//!   bodies are lowered, so each closure's wrapper and the `__rt_closure_call` if-ladder
//!   are present before the module is assembled.
//! - `crate::codegen_wasm::inst::lower_instruction` dispatches `ClosureNew` /
//!   `ClosureCall` / `ClosureCapture` here.
//! - Unit-test harnesses that emit `emit_refcount_runtime` must also emit
//!   `emit_closure_runtime` for the same reason (see the M5 ripple).
//!
//! Key details:
//! - A callable is a single heap pointer (carried as `WasmRepr::I64`, a zero-extended
//!   i32) to a callable descriptor: a generic heap block whose 16-byte header is
//!   stamped with heap-kind 6 at `[ptr-8]`. The descriptor payload (P7a0 layout) is:
//!   `[ptr+0]` i64 descriptor kind (Closure=1; reserved for FirstClass/Static/Instance
//!   variants later), `[ptr+8]` i32 entry_index (the if-ladder key), `[ptr+12]` i32
//!   capture_count, `[ptr+16]` i32 capture_tags_ptr (a static per-closure tag-byte
//!   array), `[ptr+20]` pad, and capture slots at `[ptr+32 + i*16]` (low 8 = value/ptr,
//!   high 8 = string length). Slot base 32 (not native's 64) because WASM needs no
//!   signature/environment/invocation symbol records.
//! - `__rt_callable_descriptor_release` mirrors `__rt_decref_object`: null / below-payload
//!   / at-cursor guards, a refcount==0 re-entrancy guard, mark-zero, then a capture walk
//!   that releases each refcounted slot (tag in {1,4,5,6,7,10,12} = str/array/assoc/
//!   object/mixed/callable/iterable) via the kind-dispatched `__rt_decref_any` (so a
//!   callable capture recurses through kind-6), and finally `__rt_heap_free` (unsafe;
//!   refcount already 0). By-ref captures use tag sentinel 0xFF and are skipped (the
//!   promoted cell outlives the closure). P7a0 descriptors have capture_count 0, so the
//!   walk is a no-op today; the full walk is emitted now so P7b only needs `ClosureNew`
//!   to populate slots.
//! - P7a1 closure call uses a uniform Mixed-cell arg buffer. `ClosureCall` boxes each
//!   argument into a kind-5 cell (via `objects::emit_box_value_into_mixed`), pushes the
//!   cell pointer into a `value_type`-7 array (`__rt_array_push_mixed`), and calls
//!   `__rt_closure_call(desc, args)`, which if-ladders on the descriptor's `entry_index`
//!   to the per-closure wrapper. The wrapper unboxes each slot to the body's declared
//!   parameter type (acquiring containers/callables so the body's Owned params balance),
//!   calls the body, boxes the body's result into a Mixed cell, and returns the cell.
//!   The caller unboxes the result cell to the instruction's result type and releases
//!   the arg array (whose `free_deep` releases every cell). This is refcount-balanced
//!   whether or not the EIR ownership pass acquires ClosureCall arguments: the wrapper's
//!   acquire gives the body an owned ref, and the array's deep free releases the cell
//!   that `__rt_mixed_from_value` incref'd/persisted.

use super::context::{wasm_fn_symbol, FnCtx, Result};
use super::inst::{
    ByRefSource, data_immediate, operand, resolve_by_ref_source, slot_payload_type, store_result,
};
use super::objects::emit_box_value_into_mixed;
use super::values::WasmRepr;
use super::wat::{DataSegment, ValType, WatModule};
use super::WasmError;
use crate::ir::{Function, Instruction, IrHeapKind, IrType, LocalSlotId, Module, Ownership, ValueId};
use crate::types::PhpType;

/// Registers the callable-descriptor refcount runtime (`__rt_callable_descriptor_release`)
/// on `wm`.
///
/// Must be emitted alongside `refcount::emit_refcount_runtime`, whose `__rt_decref_any`
/// calls this from its kind-6 branch. The function references only `__rt_decref_any`
/// (for the capture walk) and `__rt_heap_free`, both always present alongside the
/// refcount runtime, so — unlike `emit_object_runtime` — no extra globals are required
/// and the same emitter serves production modules and unit-test harnesses.
pub(super) fn emit_closure_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_CALLABLE_DESCRIPTOR_RELEASE);
}

/// `__rt_callable_descriptor_release`: the kind-6 release entry. Decrements the
/// descriptor refcount; at zero, walks the capture slots (releasing each refcounted
/// child via the kind-dispatched `__rt_decref_any`, so callable captures recurse) and
/// frees the descriptor. No-ops on null or non-heap pointers. Mirrors
/// `__rt_decref_object` (objects.rs) in guard shape and walk structure, but reads the
/// slot count/tags from the descriptor payload (`[ptr+12]` / `[ptr+16]`) instead of a
/// class gc_desc, since a closure's capture layout is per-descriptor, not per-class.
const RT_CALLABLE_DESCRIPTOR_RELEASE: &str = r#"(func $__rt_callable_descriptor_release (param $ptr i32)
  (local $rc i32) (local $n i32) (local $tags i32) (local $i i32) (local $tag i32) (local $slot i32)
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
  (local.set $n (i32.load offset=12 (local.get $ptr)))             ;; capture_count = [ptr+12]
  (local.set $tags (i32.load offset=16 (local.get $ptr)))          ;; capture_tags_ptr = [ptr+16]
  (local.set $i (i32.const 0))                                     ;; capture index = 0
  (block $walk_end
    (loop $walk
      (br_if $walk_end (i32.ge_u (local.get $i) (local.get $n)))   ;; i >= n -> end walk
      (local.set $tag (i32.load8_u (i32.add (local.get $tags) (local.get $i))))  ;; tag = tags[i]
      ;; refcounted tags: 1 (str), 4 (array), 5 (assoc), 6 (object), 7 (mixed), 10 (callable), 12 (iterable).
      ;; Scalars (0/2/3), null (8), and the by-ref sentinel (0xFF) own no heap storage; 13 (buffer) is non-refcounted.
      (if (i32.or (i32.or (i32.or (i32.eq (local.get $tag) (i32.const 1)) (i32.and (i32.ge_u (local.get $tag) (i32.const 4)) (i32.le_u (local.get $tag) (i32.const 7)))) (i32.eq (local.get $tag) (i32.const 10))) (i32.eq (local.get $tag) (i32.const 12))) (then  ;; tag in {1,4,5,6,7,10,12} -> release the slot
        (local.set $slot (i32.wrap_i64 (i64.load offset=32 (i32.add (local.get $ptr) (i32.mul (local.get $i) (i32.const 16))))))  ;; slot ptr = low 8 bytes of [ptr+32+i*16]
        (call $__rt_decref_any (local.get $slot))                  ;; release the child (kind-dispatched; callable recurses via kind 6)
      )                                                            ;; close then (tag check)
      )                                                            ;; close if (tag check)
      (local.set $i (i32.add (local.get $i) (i32.const 1)))        ;; i++
      (br $walk)                                                   ;; loop back
    )                                                              ;; close loop $walk
  )                                                                ;; close block $walk_end
  (call $__rt_heap_free (local.get $ptr))                          ;; free the descriptor (unsafe: refcount already 0)
  (return)                                                         ;; top-level return
)
"#;

// ---------------------------------------------------------------------------
// P7a1: no-capture closure create / call lowering + dispatch wrappers.
// ---------------------------------------------------------------------------

/// The fixed 32-byte descriptor payload size for a no-capture closure (header fields
/// only; capture slots begin at `+32` and are absent when `capture_count` is 0).
const DESCRIPTOR_PAYLOAD_BYTES: i32 = 32;

/// The byte offset of the first capture slot within a callable descriptor payload.
/// Each capture slot is 16 bytes (low 8 = value/ptr, high 8 = string length).
const CAPTURE_SLOT_OFFSET: i32 = 32;

/// The size of one capture slot in the descriptor payload.
const CAPTURE_SLOT_BYTES: i32 = 16;

// ---------------------------------------------------------------------------
// P7b: by-value capture tag tables + capture-aware ClosureNew / wrapper unbox.
// ---------------------------------------------------------------------------

/// The runtime release-tag byte for a capture of `php` type, mirroring the native
/// `type_tag` table (`src/codegen/callable_descriptor.rs:584`) with the by-ref
/// override. The release runtime (`__rt_callable_descriptor_release`) releases a
/// slot iff its tag is in `{1,4,5,6,7,10,12}` (str/array/assoc/object/mixed/callable/
/// iterable); scalars (`0/2/3`), null (`8`), the by-ref sentinel (`0xFF`), and
/// `13` (buffer, non-refcounted) own no heap storage and are skipped. Only the
/// wrapper-supported set is reachable for P7b (see `lower_closure_new`), but the
/// full table is emitted for forward-compat with P7c (by-ref), P7d1 (Mixed/Union),
/// and P7d1b (Iterable) captures.
fn capture_tag_for_php_type(php: &PhpType, by_ref: bool) -> u8 {
    if by_ref {
        return 0xFF;
    }
    match php {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed | PhpType::Union(_) => 7,
        PhpType::Void => 8,
        PhpType::Resource(_) => 9,
        PhpType::Callable => 10,
        PhpType::Pointer(_) => 11,
        PhpType::Iterable => 12,
        PhpType::Buffer(_) => 13,
        PhpType::Packed(_) => 14,
        PhpType::Never => 15,
        PhpType::TaggedScalar => 7,
    }
}

/// Emits one static per-closure capture-tag byte array into `wm` (in static memory
/// below the heap) and returns `(advanced_cursor, tag_base_per_closure)`. The
/// returned vec is indexed by the closure's position in `module.closures` (its
/// `entry_index`); a no-capture closure gets a `0` sentinel (no segment emitted),
/// so indexing by `entry_index` is uniform. Each array holds one tag byte per
/// capture param (the trailing `flags.closure_capture_count` params, source order),
/// computed via `capture_tag_for_php_type`. `generate()` calls this after the
/// instanceof target table and before `heap_base` is computed.
pub(super) fn emit_closure_capture_tag_tables(
    wm: &mut WatModule,
    module: &Module,
    mut cursor: u32,
) -> Result<(u32, Vec<u32>)> {
    let mut tag_ptrs: Vec<u32> = Vec::with_capacity(module.closures.len());
    for f in &module.closures {
        let cap = f.flags.closure_capture_count;
        if cap == 0 {
            tag_ptrs.push(0);
            continue;
        }
        // 4-align the cursor so a multi-byte tag array starts on a clean boundary.
        cursor = (cursor + 3) & !3;
        let base = cursor;
        // Defensive: `closure_capture_count` is set by `lower_closure_function_with_signature`
        // to exactly the appended capture count, so `cap <= params.len()` always holds for
        // well-formed modules. Guard anyway so a hand-crafted malformed `Module` surfaces an
        // error instead of panicking on slice underflow.
        let visible = f
            .params
            .len()
            .checked_sub(cap)
            .ok_or_else(|| WasmError::Unsupported(format!(
                "closure {} capture_count {} > params {}",
                f.name, cap, f.params.len()
            )))?;
        let mut bytes = Vec::with_capacity(cap);
        for p in &f.params[visible..] {
            bytes.push(capture_tag_for_php_type(&p.php_type, p.by_ref));
        }
        wm.add_data(DataSegment {
            offset: base,
            bytes,
        });
        cursor = base + cap as u32;
        tag_ptrs.push(base);
    }
    Ok((cursor, tag_ptrs))
}

/// Lowers `Op::ClosureNew`: allocates a kind-6 callable descriptor, stamps its
/// payload (descriptor kind 1, the closure's `entry_index`, `capture_count`, and the
/// per-closure `capture_tags_ptr` from the static tag array), stamps each by-value
/// capture into its slot, and stores the zero-extended pointer into the result's
/// `I64` local.
///
/// The closure name is carried by an `Immediate::Data` index into the module's string
/// pool (the same pool `ClosureNew` interns the `__eir_closure_<owner>_<n>` name into
/// at lowering time). The `entry_index` is the closure `Function`'s position in
/// `module.closures`, which the `__rt_closure_call` if-ladder keys on and which
/// indexes `ctx.closure_tag_ptrs`.
///
/// P7b supports by-value captures of `Int`/`Bool`/`Float`/`Str`/`Array`/`AssocArray`/
/// `Object`/`Callable`; P7d1 extends that to `Mixed`/`Union` (both a kind-5 Mixed cell,
/// capture tag 7); P7d1b extends that to `Iterable` (single-i32 Ptr, capture tag 12,
/// now in the release set `{1,4,5,6,7,10,12}`). The capture list is recovered as the
/// trailing `closure_capture_count` params of the closure body (parity with native
/// `closure_capture_params_from_eir`); the operand count is cross-checked against it.
/// The slot layout (tag + store shape) is derived from the **capture param** type
/// (not the operand type), with an explicit operand/param type-drift cross-check so a
/// future lowering divergence is a compile error, not a silent miscompile. By-ref
/// captures (P7c/P7c0), by-ref/variadic visible params (m10), and
/// `Buffer`/`TaggedScalar`/`Pointer`/`Resource`/`Packed`/`Never` captures are rejected.
/// Ownership: a non-`Owned` refcounted capture is `incref`'d (or `__rt_str_persist`'d
/// for strings) so the descriptor owns a ref; an `Owned` operand's ref transfers
/// directly (no incref), mirroring native
/// `emit_runtime_closure_descriptor_with_captures`.
pub(super) fn lower_closure_new(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = data_immediate(inst)?;
    let name = ctx
        .module
        .data
        .strings
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("closure new: unknown name {:?}", data_id)))?;
    let (entry_index, closure_fn) = ctx
        .module
        .closures
        .iter()
        .enumerate()
        .find(|(_, f)| f.name == name)
        .ok_or_else(|| WasmError::Unsupported(format!("closure new: no body for {}", name)))?;
    let capture_count = inst.operands.len();
    let visible_count = closure_fn.params.len().saturating_sub(capture_count);
    if capture_count != closure_fn.flags.closure_capture_count {
        return Err(WasmError::Unsupported(format!(
            "closure {}: operand count {} != capture_count {}",
            name, capture_count, closure_fn.flags.closure_capture_count
        )));
    }
    // Visible params must be by-value non-variadic (the wrapper forwards them as-is).
    for p in &closure_fn.params[..visible_count] {
        if p.by_ref || p.variadic {
            return Err(WasmError::Unsupported(format!(
                "ClosureNew by-ref/variadic visible param {} on wasm32-wasi (P7c)",
                p.name
            )));
        }
    }
    // Validate every capture param up front so an unsupported capture fails before any
    // descriptor allocation (no half-stamped descriptor leaks on the error path).
    for p in &closure_fn.params[visible_count..] {
        reject_unsupported_capture(&name, p)?;
    }

    let total = DESCRIPTOR_PAYLOAD_BYTES + capture_count as i32 * CAPTURE_SLOT_BYTES;
    let desc = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!("(call $__rt_heap_alloc (i32.const {}))", total),
        "allocate callable descriptor (refcount 1) + capture slots",
    );
    ctx.fb.ins(&format!("local.set {}", desc), "save descriptor pointer");
    ctx.fb.ins(
        &format!("(i64.store (i32.sub (local.get {}) (i32.const 8)) (i64.const 6))", desc),
        "stamp heap-header kind = 6 (callable)",
    );
    ctx.fb.ins(
        &format!("(i64.store (local.get {}) (i64.const 1))", desc),
        "descriptor kind = 1 (Closure)",
    );
    ctx.fb.ins(
        &format!("(i32.store offset=8 (local.get {}) (i32.const {}))", desc, entry_index),
        "entry_index = position in module.closures",
    );
    ctx.fb.ins(
        &format!("(i32.store offset=12 (local.get {}) (i32.const {}))", desc, capture_count),
        "capture_count = number of capture slots",
    );
    let tag_base = ctx.closure_tag_base(entry_index);
    ctx.fb.ins(
        &format!("(i32.store offset=16 (local.get {}) (i32.const {}))", desc, tag_base),
        "capture_tags_ptr = static per-closure tag array (0 if no captures)",
    );

    // Stamp each capture into its 16-byte slot at [desc + 32 + i*16]. A by-ref
    // capture (`use (&$x)`) stamps the caller's ref-cell pointer (P7c); a by-value
    // capture stamps the retained value (P7b).
    for i in 0..capture_count {
        let operand = operand(inst, i)?;
        let cap_p = &closure_fn.params[visible_count + i];
        if cap_p.by_ref {
            stamp_by_ref_capture_slot(ctx, &desc, i, operand, cap_p)?;
        } else {
            stamp_capture_slot(ctx, &desc, i, operand, cap_p)?;
        }
    }

    ctx.fb.ins(&format!("local.get {}", desc), "descriptor pointer");
    ctx.fb.ins("i64.extend_i32_u", "zero-extend ptr -> i64 callable value");
    store_result(ctx, inst)
}

/// Rejects a capture param whose kind is outside the supported set.
/// By-ref captures are supported (P7c) for the by-value value-type set: the caller
/// local is promoted into a persistent ref-cell and the cell pointer is stamped into
/// the descriptor. P7d1 extends the by-value set to `Mixed`/`Union` (both
/// `WasmRepr::Ptr`, boxed as a kind-5 Mixed cell, capture tag 7 already in the
/// release set `{1,4,5,6,7,10,12}`); the by-ref path reuses the same Ptr-repr promote
/// + cell-ptr stamp, so by-ref `Mixed`/`Union` only needs this reject lift.
/// P7d1b accepts `IrHeapKind::Iterable` by-value and by-ref (single-i32 Ptr, tag 12,
/// now in the release set). `Buffer` stays REJECTED — `BufferNew` is not lowered in
/// WASM; a Buffer capture path would be dead-code scaffolding until that lands.
/// `TaggedScalar` (P7d1c) is a 2-word `WasmRepr::Tagged` cell that diverges from the
/// release walk's single-i32-ptr read. `Pointer`/`Resource`/`Packed`/`Never` would
/// stamp a slot whose tag is not in the release set `{1,4,5,6,7,10,12}` (an unleakable
/// ptr) — and a by-ref capture of them has no sound promote either. `Void` carries no
/// value. Each rejected kind carries a phase tag so the caller knows where it lands.
fn reject_unsupported_capture(name: &str, p: &crate::ir::FunctionParam) -> Result<()> {
    // Reject by php_type first: Pointer/Resource lower to a raw I64 and Packed to
    // Heap(Object), so the ir_type match below would otherwise accept them even
    // though their capture tags (9/11, or a Packed-as-object ref) are outside the
    // release set or semantically wrong for a by-value capture. Never carries no
    // value to stamp.
    match &p.php_type {
        PhpType::Pointer(_)
        | PhpType::Resource(_)
        | PhpType::Packed(_)
        | PhpType::Never => {
            return Err(WasmError::Unsupported(format!(
                "ClosureNew {:?} capture {} on wasm32-wasi (unsupported capture kind)",
                p.php_type, p.name
            )));
        }
        _ => {}
    }
    match p.ir_type {
        IrType::I64 | IrType::F64 | IrType::Str => Ok(()),
        IrType::Heap(
            IrHeapKind::Array | IrHeapKind::Hash | IrHeapKind::Object
            | IrHeapKind::Mixed | IrHeapKind::Union
            | IrHeapKind::Iterable,
        ) => Ok(()),
        IrType::Heap(IrHeapKind::Buffer) => Err(WasmError::Unsupported(format!(
            "ClosureNew {} Buffer capture on wasm32-wasi (BufferNew not yet lowered)", name,
        ))),
        IrType::TaggedScalar | IrType::Void => Err(WasmError::Unsupported(format!(
            "ClosureNew {:?} capture {} on wasm32-wasi",
            p.ir_type, p.name
        ))),
    }
}

/// Stamps one by-value capture `operand` into descriptor slot `i` (`[desc + 32 + i*16]`)
/// using the capture **param**'s type for the tag/store shape. Loads the operand,
/// applies ownership-aware retain (`incref` for refcounted, `__rt_str_persist` for a
/// non-owned string; an `Owned` operand transfers its ref with no retain), then stores
/// the value into the slot. Cross-checks the operand's php_type against the param's so
/// a stamp/unbox type drift is a compile error rather than a silent miscompile.
fn stamp_capture_slot(
    ctx: &mut FnCtx,
    desc: &str,
    i: usize,
    operand: ValueId,
    cap_p: &crate::ir::FunctionParam,
) -> Result<()> {
    let off = CAPTURE_SLOT_OFFSET + i as i32 * CAPTURE_SLOT_BYTES;
    let operand_php = ctx.value_php_type(operand)?;
    if operand_php != cap_p.php_type {
        return Err(WasmError::Unsupported(format!(
            "closure capture {}: operand type {:?} != param type {:?}",
            cap_p.name, operand_php, cap_p.php_type
        )));
    }
    let ownership = ctx
        .function
        .value(operand)
        .map(|v| v.ownership)
        .unwrap_or(Ownership::NonHeap);
    let not_owned = !matches!(ownership, Ownership::Owned);
    ctx.emit_load_value(operand)?;
    match cap_p.ir_type {
        IrType::I64 => {
            // Int/Bool/Callable: one i64 (the value, or the descriptor pointer for Callable).
            let v = ctx.fresh_temp(ValType::I64);
            ctx.fb.ins(&format!("local.set {}", v), "capture i64 value");
            if matches!(cap_p.php_type, PhpType::Callable) && not_owned {
                ctx.fb.ins(
                    &format!("(call $__rt_incref (i32.wrap_i64 (local.get {})))", v),
                    "share the captured callable descriptor (descriptor owns a ref)",
                );
            }
            ctx.fb.ins(
                &format!(
                    "(i64.store (i32.add (local.get {}) (i32.const {})) (local.get {}))",
                    desc, off, v
                ),
                "store the i64 capture into its slot",
            );
        }
        IrType::F64 => {
            let v = ctx.fresh_temp(ValType::F64);
            ctx.fb.ins(&format!("local.set {}", v), "capture f64 value");
            ctx.fb.ins(
                &format!(
                    "(f64.store (i32.add (local.get {}) (i32.const {})) (local.get {}))",
                    desc, off, v
                ),
                "store the f64 capture into its slot (no refcount)",
            );
        }
        IrType::Str => {
            // Str repr on the stack: [ptr i32, len i64] (len on top).
            let len = ctx.fresh_temp(ValType::I64);
            let ptr = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", len), "capture string length");
            ctx.fb.ins(&format!("local.set {}", ptr), "capture string pointer");
            if not_owned {
                // __rt_str_persist(ptr i32, len i64) -> (owned ptr i32, owned len i64).
                ctx.fb.ins(&format!("local.get {}", ptr), "string pointer for persist");
                ctx.fb.ins(&format!("local.get {}", len), "string length for persist");
                ctx.fb.ins("call $__rt_str_persist", "persist an owned copy for the descriptor");
                ctx.fb.ins(&format!("local.set {}", len), "owned copy length");
                ctx.fb.ins(&format!("local.set {}", ptr), "owned copy pointer");
            }
            ctx.fb.ins(
                &format!(
                    "(i32.store (i32.add (local.get {}) (i32.const {})) (local.get {}))",
                    desc, off, ptr
                ),
                "store the string pointer in the slot low 4 bytes",
            );
            ctx.fb.ins(
                &format!(
                    "(i32.store (i32.add (local.get {}) (i32.const {})) (i32.wrap_i64 (local.get {})))",
                    desc,
                    off + 8,
                    len
                ),
                "store the string length (i32) in the slot high 4 bytes",
            );
        }
        IrType::Heap(
            IrHeapKind::Array | IrHeapKind::Hash | IrHeapKind::Object | IrHeapKind::Mixed | IrHeapKind::Union
            | IrHeapKind::Iterable,
        ) => {
            // Container/Mixed-cell/Iterable ptr on the stack as a single i32. Mixed and Union
            // are both `WasmRepr::Ptr` (a kind-5 Mixed cell); Iterable is a type-erased
            // array/hash/object ptr (kind 2/3/4) freed via `__rt_decref_any`. All stamp
            // as one i32 ptr that the release walk reads back.
            let ptr = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", ptr), "capture container/cell pointer");
            if not_owned {
                ctx.fb.ins(
                    &format!("(call $__rt_incref (local.get {}))", ptr),
                    "share the captured container/cell (descriptor owns a ref)",
                );
            }
            ctx.fb.ins(
                &format!(
                    "(i64.store (i32.add (local.get {}) (i32.const {})) (i64.extend_i32_u (local.get {})))",
                    desc, off, ptr
                ),
                "store the container/cell pointer (i64) for the release walk's i64.load",
            );
        }
        // Unsupported kinds are rejected up front by `reject_unsupported_capture`; the
        // remaining ir types are unreachable here.
        _ => {
            return Err(WasmError::Unsupported(format!(
                "closure capture {:?} stamp on wasm32-wasi",
                cap_p.ir_type
            )))
        }
    }
    Ok(())
}

/// Promotes a caller local into a persistent ref-cell for a by-ref closure capture, or
/// returns the existing cell pointer if the slot is already ref-bound.
///
/// Unlike P7c0b's transient temp cell (synthesized per call, written back + freed after),
/// a by-ref closure capture's cell outlives the `ClosureNew`: the closure holds the cell
/// pointer in its descriptor, so the cell must persist for the closure's lifetime. The
/// cell is released once by the `Return` epilogue via `ref_cell_owners` (the descriptor's
/// release walk skips the 0xFF by-ref tag), and the slot's old value is released here
/// (WASM has no PhpLocal-exit-release epilogue, so the lingering slot reference must drop
/// now). Mirrors the active native backend's `promote_local_slot_for_ref_capture`.
///
/// If the slot already stores a ref-cell pointer (a prior `use(&$x)`, a `=&` alias, or a
/// by-ref free-function param), the existing cell is shared — no re-alloc, no second
/// owner (`add_ref_cell_owner` dedups by slot). After this, the caller's subsequent
/// `LoadLocal`/`StoreLocal` route through the cell (see `inst::lower_load_local` /
/// `lower_store_local`).
fn promote_local_for_by_ref_capture(ctx: &mut FnCtx, slot: LocalSlotId) -> Result<String> {
    let slot_raw = slot.as_raw();
    if let Some(ptr) = ctx.ref_cell_ptrs.get(&slot_raw) {
        return Ok(ptr.clone());
    }
    let slot_repr = ctx.slot_repr(slot)?.clone();
    let payload = slot_payload_type(ctx, slot)?;
    let ptr_local = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins("i32.const 16", "ref cell size (16 bytes)");
    ctx.fb.ins("call $__rt_heap_alloc", "allocate the by-ref capture ref cell");
    ctx.fb.ins(&format!("local.set {}", ptr_local), "by-ref capture cell pointer");
    super::refcell::retain_and_store_slot_value(ctx, &ptr_local, &slot_repr, &payload)?;
    ctx.register_ref_cell_ptr(slot_raw, ptr_local.clone());
    ctx.add_ref_cell_owner(slot_raw, payload.clone());
    super::refcell::release_old_slot_value(ctx, &slot_repr, &payload)?;
    Ok(ptr_local)
}

/// Stamps one by-ref capture `operand` into descriptor slot `i` (`[desc + 32 + i*16]`).
///
/// Resolves the operand's source local (a `LoadLocal`/`LoadRefCell` of a php-visible
/// local; non-locals are rejected, matching P7c0b's restriction), promotes it into a
/// ref-cell (or reuses its existing cell), and stores the cell pointer (i32) into the
/// slot's low word. The capture tag is 0xFF (`capture_tag_for_php_type` with `by_ref`),
/// stamped by `emit_closure_capture_tag_tables`, so the descriptor's release walk skips
/// it — the caller owns the cell, not the descriptor.
fn stamp_by_ref_capture_slot(
    ctx: &mut FnCtx,
    desc: &str,
    i: usize,
    operand: ValueId,
    cap_p: &crate::ir::FunctionParam,
) -> Result<()> {
    let off = CAPTURE_SLOT_OFFSET + i as i32 * CAPTURE_SLOT_BYTES;
    let slot = match resolve_by_ref_source(ctx, operand)? {
        ByRefSource::AlreadyRefBound(raw) => LocalSlotId::from_raw(raw),
        ByRefSource::FreshLocal(slot) => slot,
        ByRefSource::NonLocal => {
            return Err(WasmError::Unsupported(format!(
                "by-ref capture {} of a non-local on wasm32-wasi (P7c: deferred)",
                cap_p.name
            )));
        }
    };
    let cell_ptr = promote_local_for_by_ref_capture(ctx, slot)?;
    ctx.fb.ins(&format!("local.get {}", desc), "descriptor address");
    ctx.fb.ins(&format!("local.get {}", cell_ptr), "by-ref capture cell pointer");
    ctx.fb.ins(
        &format!("i32.store offset={}", off),
        "stamp the cell pointer @ capture slot+0 (tag 0xFF, release walk skips)",
    );
    Ok(())
}

/// Lowers `Op::ClosureCapture`, a no-op marker the EIR emits (with an `Immediate::I64(1)`
/// when the capture is by-ref). P7a1 handles only no-capture closures, so capture
/// operands never reach here; the marker is honored as a bare pass-through.
pub(super) fn lower_closure_capture(_ctx: &mut FnCtx, _inst: &Instruction) -> Result<()> {
    Ok(())
}

/// Lowers `Op::ClosureCall`: builds the uniform Mixed-cell arg buffer, calls
/// `__rt_closure_call(desc, args)`, unboxes the result cell to the instruction's result
/// type, and releases the arg array (whose deep free releases every arg cell).
///
/// Operand 0 is the callable (an `I64` descriptor); operands 1.. are the arguments in
/// source order. Each argument is boxed via `objects::emit_box_value_into_mixed`, which
/// shares ownership of containers/callables (`__rt_mixed_from_value` increfs them) and
/// persists strings, so the array's `__rt_array_free_deep` releases exactly those refs.
/// The result cell is unboxed to the inst result's `WasmRepr` shape; container/callable
/// results are incref'd before store so the inst owns a ref, and the cell is then
/// released. A `Mixed`/`Union` result forwards the cell directly (no release: ownership
/// transfers to the inst). A void call (no result) releases the null cell the wrapper
/// returned.
pub(super) fn lower_closure_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let callable = operand(inst, 0)?;
    let arg_count = inst.operands.len().saturating_sub(1);

    // callable descriptor (i64) -> i32 for __rt_closure_call.
    let desc = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(callable)?;
    ctx.fb.ins("i32.wrap_i64", "callable descriptor i64 -> i32");
    ctx.fb.ins(&format!("local.set {}", desc), "save descriptor pointer");

    // arg buffer: a value_type-7 Mixed-cell array, pre-sized to arg_count (16-byte slots).
    let args = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!("(call $__rt_array_new (i64.const {}) (i64.const 16))", arg_count),
        "allocate the closure arg buffer (16-byte Mixed-cell slots)",
    );
    ctx.fb.ins(&format!("local.set {}", args), "save arg array pointer");

    for i in 0..arg_count {
        let arg = operand(inst, 1 + i)?;
        let cell = emit_box_value_into_mixed(ctx, arg)?;
        ctx.fb.ins(
            &format!(
                "(local.set {} (call $__rt_array_push_mixed (local.get {}) (local.get {})))",
                args, args, cell
            ),
            &format!("box arg {} into a Mixed cell and append it to the buffer", i),
        );
    }

    // call $__rt_closure_call(desc, args) -> i32 result cell pointer.
    let rcell = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!(
            "(local.set {} (call $__rt_closure_call (local.get {}) (local.get {})))",
            rcell, desc, args
        ),
        "dispatch through the closure if-ladder and capture the result cell",
    );

    // Unbox the result cell to the inst result shape, or release it if the call is void.
    if let Some(result) = inst.result {
        unbox_result_cell(ctx, &rcell, result)?;
    } else {
        ctx.fb.ins(
            &format!("local.get {}", rcell),
            "void closure result cell (wrapper boxed null)",
        );
        ctx.fb.ins("call $__rt_decref_any", "release the void-result cell");
    }

    // Release the arg buffer: kind-2 -> __rt_decref_array -> free_deep releases each cell.
    ctx.fb.ins(&format!("local.get {}", args), "arg buffer pointer");
    ctx.fb.ins("call $__rt_decref_any", "release the arg buffer (deep-frees each cell)");
    Ok(())
}

/// Unboxes a result Mixed cell (`$rcell`) into the inst result value's `WasmRepr` shape
/// and stores it, releasing the cell afterward — except for a `Mixed`/`Union` result,
/// where the cell IS the value and ownership transfers to the inst (no release).
fn unbox_result_cell(ctx: &mut FnCtx, rcell: &str, result: ValueId) -> Result<()> {
    let repr = ctx.value_repr(result)?.clone();
    let php = ctx.value_php_type(result)?;
    let is_mixed_result = matches!(php, PhpType::Mixed | PhpType::Union(_));
    match &repr {
        WasmRepr::I64(_) => {
            if matches!(php, PhpType::Callable) {
                // callable result: unbox lo (descriptor i64), incref to own, store i64.
                ctx.fb.ins(&format!("local.get {}", rcell), "result cell");
                ctx.fb.ins("call $__rt_mixed_unbox", "unbox -> (tag, lo, hi)");
                let lo = capture_unbox_lo(ctx)?;
                ctx.fb.ins(&format!("local.get {}", lo), "descriptor lo");
                ctx.fb.ins("i32.wrap_i64", "descriptor -> i32 for incref");
                ctx.fb.ins("call $__rt_incref", "own the descriptor (inst owns a ref)");
                ctx.fb.ins(&format!("local.get {}", lo), "descriptor i64 -> store");
                ctx.emit_store_value(result)?;
                release_cell(ctx, rcell);
            } else {
                // int/bool result: cast_int -> i64, store, release cell.
                ctx.fb.ins(&format!("local.get {}", rcell), "result cell");
                ctx.fb.ins("call $__rt_mixed_cast_int", "cast cell -> i64");
                ctx.emit_store_value(result)?;
                release_cell(ctx, rcell);
            }
        }
        WasmRepr::F64(_) => {
            ctx.fb.ins(&format!("local.get {}", rcell), "result cell");
            ctx.fb.ins("call $__rt_mixed_cast_float", "cast cell -> f64 bits (i64)");
            ctx.fb.ins("f64.reinterpret_i64", "f64 bits -> f64");
            ctx.emit_store_value(result)?;
            release_cell(ctx, rcell);
        }
        WasmRepr::Str { .. } => {
            // cast_string persists an owned copy (ptr i32, len i32); widen len to i64.
            ctx.fb.ins(&format!("local.get {}", rcell), "result cell");
            ctx.fb.ins("call $__rt_mixed_cast_string", "cast cell -> owned (ptr, len i32)");
            ctx.fb.ins("i64.extend_i32_u", "string length i32 -> i64 for the Str repr");
            ctx.emit_store_value(result)?;
            release_cell(ctx, rcell);
        }
        WasmRepr::Ptr(_) => {
            if is_mixed_result {
                // The cell IS the Mixed value: forward it directly, no release.
                ctx.fb.ins(&format!("local.get {}", rcell), "result Mixed cell pointer");
                ctx.emit_store_value(result)?;
            } else {
                // array/hash/object result: unbox lo (ptr), incref to own, store ptr.
                ctx.fb.ins(&format!("local.get {}", rcell), "result cell");
                ctx.fb.ins("call $__rt_mixed_unbox", "unbox -> (tag, lo, hi)");
                let lo = capture_unbox_lo(ctx)?;
                ctx.fb.ins(&format!("local.get {}", lo), "child pointer lo");
                ctx.fb.ins("i32.wrap_i64", "child -> i32 for incref");
                ctx.fb.ins("call $__rt_incref", "own the child (inst owns a ref)");
                ctx.fb.ins(&format!("local.get {}", lo), "child pointer -> store");
                ctx.fb.ins("i32.wrap_i64", "lo -> i32 Ptr value");
                ctx.emit_store_value(result)?;
                release_cell(ctx, rcell);
            }
        }
        _ => {
            return Err(WasmError::Unsupported(format!(
                "ClosureCall result repr {:?} on wasm32-wasi",
                repr
            )))
        }
    }
    Ok(())
}

/// Captures the `lo` component of an `__rt_mixed_unbox` result (which leaves
/// `(tag, lo, hi)` on the stack) into a fresh i64 temp and returns the temp name.
/// `hi` and `tag` are dropped; only `lo` (the payload pointer/value) is kept.
fn capture_unbox_lo(ctx: &mut FnCtx) -> Result<String> {
    let hi = ctx.fresh_temp(ValType::I64);
    let lo = ctx.fresh_temp(ValType::I64);
    let tag = ctx.fresh_temp(ValType::I64);
    ctx.fb.ins(&format!("local.set {}", hi), "drop unbox hi");
    ctx.fb.ins(&format!("local.set {}", lo), "capture unbox lo");
    ctx.fb.ins(&format!("local.set {}", tag), "drop unbox tag");
    Ok(lo)
}

/// Releases a result cell via `__rt_decref_any` (kind-5 -> `__rt_decref_mixed` -> frees
/// the cell and its incref'd child). Used after the result has been copied/incref'd out.
fn release_cell(ctx: &mut FnCtx, rcell: &str) {
    ctx.fb.ins(&format!("local.get {}", rcell), "result cell");
    ctx.fb.ins("call $__rt_decref_any", "release the result cell");
}

/// Emits one wrapper per closure body plus the `__rt_closure_call` if-ladder that keys
/// on the descriptor's `entry_index`. Called from `generate()` after the closure bodies
/// are lowered, so each `fn___eir_closure_<owner>_<n>` body symbol is already defined.
///
/// A wrapper `(func $fn_closure_wrap_<owner>_<n> (param $desc i32) (param $args i32)
/// (result i32))` unboxes each arg slot to the body's declared parameter type (acquiring
/// containers/callables so the body's Owned params balance), calls the body, boxes the
/// body's result into a Mixed cell, and returns the cell. `__rt_closure_call` reads the
/// `entry_index` from `[desc+8]` and tail-calls the matching wrapper; the fall-through is
/// `unreachable` (a valid descriptor always carries a known index). No wrappers are
/// emitted when the module has no closures.
pub(super) fn emit_closure_dispatch(wm: &mut WatModule, module: &Module) -> Result<()> {
    if module.closures.is_empty() {
        return Ok(());
    }
    let mut arms: Vec<(u32, String)> = Vec::new();
    for (idx, f) in module.closures.iter().enumerate() {
        let wrapper_symbol = wrapper_symbol(&f.name);
        let wat = build_closure_wrapper(&wrapper_symbol, f)?;
        wm.add_raw_func(&wat);
        arms.push((idx as u32, wrapper_symbol));
    }
    wm.add_raw_func(&build_closure_call_ladder(&arms));
    Ok(())
}

/// Formats one raw WAT instruction line (2-space indented) with a trailing `;;` comment
/// aligned at column 60, matching the hand-authored runtime/test WAT in this file. Lines
/// that reach past column 58 get a single separating space before `;;`. Used by the
/// closure wrapper / dispatch builders so the generated WAT stays readable.
fn wat_ins(code: &str, comment: &str) -> String {
    let prefix = format!("  {}", code);
    let pad = if prefix.len() >= 58 { 1 } else { 58 - prefix.len() };
    format!("{}{};; {}\n", prefix, " ".repeat(pad), comment)
}

/// Builds the raw WAT `__rt_closure_call` if-ladder from the (entry_index, wrapper) arms.
fn build_closure_call_ladder(arms: &[(u32, String)]) -> String {
    let mut wat = String::new();
    wat.push_str("(func $__rt_closure_call (param $desc i32) (param $args i32) (result i32)\n");
    wat.push_str("  (local $idx i32)\n");
    wat.push_str(&wat_ins("local.get $desc", "descriptor pointer"));
    wat.push_str(&wat_ins("i32.load offset=8", "entry_index = [desc+8]"));
    wat.push_str(&wat_ins("local.set $idx", "save the dispatch key"));
    for (idx, wrapper) in arms {
        wat.push_str(&format!(
            "  ;; dispatch arm for closure entry_index {}\n",
            idx
        ));
        wat.push_str(&wat_ins("local.get $idx", "load the dispatch key"));
        wat.push_str(&wat_ins(&format!("i32.const {}", idx), "the arm's entry_index"));
        wat.push_str(&wat_ins("i32.eq", "key == entry_index ?"));
        wat.push_str("  (if (then\n");
        wat.push_str(&wat_ins("local.get $desc", "forward the descriptor"));
        wat.push_str(&wat_ins("local.get $args", "forward the arg buffer"));
        wat.push_str(&format!("    call ${}\n", wrapper));
        wat.push_str("    return))\n");
    }
    wat.push_str("  ;; a valid descriptor always carries a known entry_index\n");
    wat.push_str(&wat_ins("unreachable", "fall-through: unknown entry_index traps"));
    wat.push_str(")\n");
    wat
}

/// Builds the raw WAT body of a closure wrapper: unbox each visible param from the arg
/// buffer, unbox each capture param from the descriptor's capture slots, call the body
/// (visible args then capture args, in EIR `Function` param order), box the result, and
/// return the result cell. The visible/capture split is read from
/// `f.flags.closure_capture_count` (captures are the trailing params); captures are
/// stamped into the descriptor by `lower_closure_new` and read here from
/// `[desc + 32 + j*16]` (NOT from the arg buffer, which carries only visible args).
fn build_closure_wrapper(wrapper_symbol: &str, f: &Function) -> Result<String> {
    let body_symbol = wasm_fn_symbol(&f.name);
    let cap = f.flags.closure_capture_count;
    // Defensive: `cap` is the trailing capture count set at lowering time, so
    // `cap <= params.len()` for well-formed modules. Guard so a malformed `Function`
    // surfaces an error instead of panicking on slice underflow.
    let vis = f
        .params
        .len()
        .checked_sub(cap)
        .ok_or_else(|| WasmError::Unsupported(format!(
            "closure {} capture_count {} > params {}",
            f.name, cap, f.params.len()
        )))?;
    let mut wat = String::new();
    wat.push_str(&format!(
        "(func ${} (param $desc i32) (param $args i32) (result i32)\n",
        wrapper_symbol
    ));
    // Shared unbox/box locals (reused per arg/result; each value is pushed before reuse).
    wat.push_str("  (local $ub_tag i64) (local $ub_lo i64) (local $ub_hi i64)\n");
    wat.push_str("  (local $rb_i64 i64) (local $rb_f64 f64) (local $rb_ptr i32) (local $rb_len i64)\n");

    // Unbox each visible parameter from the arg buffer and push it for the body call.
    for (i, p) in f.params[..vis].iter().enumerate() {
        let slot_off = 24 + i * 16;
        wat.push_str(&format!(
            "  ;; unbox visible arg {} (param {} : {:?}) from arg slot +{}\n",
            i, p.name, p.ir_type, slot_off
        ));
        wat.push_str(&format!(
            "  (i32.wrap_i64 (i64.load offset={} (local.get $args)))\n",
            slot_off
        ));
        wat.push_str(&unbox_arg_wat(&p.ir_type, &p.php_type)?);
    }

    // Unbox each capture from the descriptor and push it for the body call. Captures sit
    // at [desc + 32 + j*16] (raw slots, NOT Mixed cells). A by-ref capture stores the
    // caller's ref-cell pointer (i32) at slot+0; the body's by-ref capture param is a
    // single i32 (`WasmRepr::Ptr`, declared by P7c0b's `lower_function`), so the wrapper
    // pushes the cell pointer with no incref — the body borrows the caller's cell. A
    // by-value capture uses `unbox_capture_wat`.
    for (j, p) in f.params[vis..].iter().enumerate() {
        let slot_off = CAPTURE_SLOT_OFFSET as usize + j * CAPTURE_SLOT_BYTES as usize;
        if p.by_ref {
            wat.push_str(&format!(
                "  ;; unbox by-ref capture {} (param {} : &{:?}) cell ptr from descriptor slot +{}\n",
                j, p.name, p.ir_type, slot_off
            ));
            wat.push_str(&unbox_by_ref_capture_wat(slot_off)?);
        } else {
            wat.push_str(&format!(
                "  ;; unbox capture {} (param {} : {:?}) from descriptor slot +{}\n",
                j, p.name, p.ir_type, slot_off
            ));
            wat.push_str(&unbox_capture_wat(slot_off, &p.ir_type, &p.php_type)?);
        }
    }

    // Call the closure body with the forwarded visible + capture args on the stack.
    wat.push_str(&format!("  call ${}\n", body_symbol));
    wat.push_str("  ;; box the body result into a Mixed cell (result i32)\n");

    // Box the body result into a Mixed cell and leave it as the (result i32) return value.
    wat.push_str(&box_result_wat(&f.return_type, &f.return_php_type)?);
    wat.push_str(")\n");
    Ok(wat)
}

/// Returns the raw WAT sequence that unboxes one by-ref capture from a descriptor slot
/// at `[desc + slot_off]`: a single `i32.load` of the cell pointer stored by
/// `stamp_by_ref_capture_slot`. The body's by-ref capture parameter is a single i32
/// (`WasmRepr::Ptr`, declared by P7c0b's `lower_function`), so exactly one i32 is pushed.
/// No incref: the body borrows the caller's cell (the caller owns it; the descriptor's
/// release walk skips the 0xFF by-ref tag).
fn unbox_by_ref_capture_wat(slot_off: usize) -> Result<String> {
    let off = slot_off as i32;
    let mut s = String::new();
    s.push_str(&wat_ins(
        &format!("(i32.load offset={} (local.get $desc))", off),
        "load the by-ref capture cell pointer (single i32, body borrows the caller's cell)",
    ));
    Ok(s)
}

/// Returns the raw WAT sequence that unboxes one capture from a raw descriptor slot at
/// `[desc + slot_off]` (NOT a Mixed cell) to the body capture parameter's `IrType` /
/// `PhpType`, pushing exactly the param's `WasmRepr::val_types` for the body call.
/// Refcounted captures (containers, callables, strings) are pushed as BORROWS (no incref),
/// matching native's Model A: the descriptor retains its single ref (released by the
/// tag-walk at descriptor free), and a captured value returned by the body is promoted to
/// an owned ref at the EIR return boundary (`acquire_borrowed_return_value`) instead of by
/// the wrapper. Scalars (int/bool/float) are pushed with no refcount.
fn unbox_capture_wat(slot_off: usize, ir: &IrType, php: &PhpType) -> Result<String> {
    let off = slot_off as i32;
    let mut s = String::new();
    match ir {
        IrType::I64 => {
            if matches!(php, PhpType::Callable) {
                s.push_str(&wat_ins(
                    &format!("(local.set $ub_lo (i64.load offset={} (local.get $desc)))", off),
                    "load the captured callable descriptor (i64 lo)",
                ));
                s.push_str(&wat_ins("local.get $ub_lo", "push the descriptor i64 for the body (borrow)"));
            } else {
                s.push_str(&wat_ins(
                    &format!("(i64.load offset={} (local.get $desc))", off),
                    "load the captured int/bool i64 for the body",
                ));
            }
        }
        IrType::F64 => {
            s.push_str(&wat_ins(
                &format!("(f64.load offset={} (local.get $desc))", off),
                "load the captured f64 for the body",
            ));
        }
        IrType::Str => {
            s.push_str(&wat_ins(
                &format!("(local.set $rb_ptr (i32.load offset={} (local.get $desc)))", off),
                "load the captured string pointer",
            ));
            s.push_str(&wat_ins(
                &format!(
                    "(local.set $rb_len (i64.extend_i32_u (i32.load offset={} (local.get $desc))))",
                    off + 8
                ),
                "load the captured string length (i32 -> i64 for the Str repr)",
            ));
            s.push_str(&wat_ins("local.get $rb_ptr", "push the string pointer for the body (borrow)"));
            s.push_str(&wat_ins("local.get $rb_len", "push the string length for the body"));
        }
        IrType::Heap(kind) => match kind {
            IrHeapKind::Array | IrHeapKind::Hash | IrHeapKind::Object | IrHeapKind::Mixed | IrHeapKind::Union
            | IrHeapKind::Iterable => {
                s.push_str(&wat_ins(
                    &format!("(local.set $rb_ptr (i32.load offset={} (local.get $desc)))", off),
                    "load the captured container/cell/Iterable pointer (single-i32 borrow)",
                ));
                s.push_str(&wat_ins("local.get $rb_ptr", "push the container/cell/Iterable pointer for the body (borrow)"));
            }
            IrHeapKind::Buffer => {
                return Err(WasmError::Unsupported(format!(
                    "closure heap capture kind {:?} on wasm32-wasi (BufferNew not yet lowered)",
                    kind
                )));
            }
        },
        IrType::TaggedScalar | IrType::Void => {
            return Err(WasmError::Unsupported(format!(
                "closure capture ir {:?} on wasm32-wasi",
                ir
            )));
        }
    }
    Ok(s)
}

/// Returns the raw WAT sequence that unboxes one arg cell (already loaded on the stack
/// as an i32 cell pointer) to the body parameter's `IrType` / `PhpType`, pushing exactly
/// the param's `WasmRepr::val_types` for the body call. Containers and callables are
/// incref'd so the body's Owned parameter owns a fresh ref.
fn unbox_arg_wat(ir: &IrType, php: &PhpType) -> Result<String> {
    let mut s = String::new();
    match ir {
        IrType::I64 => {
            if matches!(php, PhpType::Callable) {
                s.push_str(&wat_ins("call $__rt_mixed_unbox", "unbox cell -> (tag, lo, hi)"));
                s.push_str(&wat_ins("local.set $ub_hi", "save hi"));
                s.push_str(&wat_ins("local.set $ub_lo", "save lo (descriptor i64)"));
                s.push_str(&wat_ins("local.set $ub_tag", "save tag"));
                s.push_str(&wat_ins("local.get $ub_lo", "descriptor lo"));
                s.push_str(&wat_ins("i32.wrap_i64", "descriptor -> i32 for incref"));
                s.push_str(&wat_ins("call $__rt_incref", "own a ref for the body param"));
                s.push_str(&wat_ins("local.get $ub_lo", "push descriptor i64 for the body"));
            } else {
                s.push_str(&wat_ins("call $__rt_mixed_cast_int", "cast cell -> i64 (int/bool)"));
            }
        }
        IrType::F64 => {
            s.push_str(&wat_ins("call $__rt_mixed_cast_float", "cast cell -> f64 bits (i64)"));
            s.push_str(&wat_ins("f64.reinterpret_i64", "f64 bits -> f64 for the body"));
        }
        IrType::Str => {
            s.push_str(&wat_ins("call $__rt_mixed_cast_string", "cast cell -> (ptr i32, len i32)"));
            s.push_str(&wat_ins("i64.extend_i32_u", "widen len to i64 (Str repr)"));
        }
        IrType::Heap(kind) => {
            match kind {
                IrHeapKind::Array | IrHeapKind::Hash | IrHeapKind::Object => {
                    s.push_str(&wat_ins("call $__rt_mixed_unbox", "unbox cell -> (tag, lo, hi)"));
                    s.push_str(&wat_ins("local.set $ub_hi", "save hi"));
                    s.push_str(&wat_ins("local.set $ub_lo", "save lo (cell ptr i64)"));
                    s.push_str(&wat_ins("local.set $ub_tag", "save tag"));
                    s.push_str(&wat_ins("local.get $ub_lo", "cell lo"));
                    s.push_str(&wat_ins("i32.wrap_i64", "ptr -> i32 for incref"));
                    s.push_str(&wat_ins("call $__rt_incref", "own a ref for the body param"));
                    s.push_str(&wat_ins("local.get $ub_lo", "cell lo"));
                    s.push_str(&wat_ins("i32.wrap_i64", "push ptr i32 for the body"));
                }
                IrHeapKind::Mixed => {
                    return Err(WasmError::Unsupported(
                        "closure Mixed visible param on wasm32-wasi (caller-side box rejects it)"
                            .to_string(),
                    ));
                }
                IrHeapKind::Iterable | IrHeapKind::Union | IrHeapKind::Buffer => {
                    return Err(WasmError::Unsupported(format!(
                        "closure heap param kind {:?} on wasm32-wasi",
                        kind
                    )));
                }
            }
        }
        IrType::TaggedScalar | IrType::Void => {
            return Err(WasmError::Unsupported(format!(
                "closure param ir {:?} on wasm32-wasi",
                ir
            )));
        }
    }
    Ok(s)
}

/// Returns the raw WAT sequence that boxes the body's on-stack result (in
/// `WasmRepr::val_types(return_type)` order) into a Mixed cell, releasing the body's
/// owned source for string/container/callable results (mirroring
/// `methods::box_call_result_into_mixed`), and leaves the cell pointer as the wrapper's
/// `(result i32)` return value. A `Heap(Mixed)` result is forwarded directly (the body's
/// cell is already a Mixed cell; ownership transfers, no re-box, no release). A void body
/// boxes a null cell so the wrapper always returns a well-defined cell.
fn box_result_wat(ir: &IrType, php: &PhpType) -> Result<String> {
    let mut s = String::new();
    match ir {
        IrType::I64 => {
            let tag = match php {
                PhpType::Bool => 3,
                PhpType::Callable => 10,
                _ => 0,
            };
            s.push_str(&wat_ins("local.set $rb_i64", "save the body's i64 result"));
            s.push_str(&wat_ins(&format!("i64.const {}", tag), "mixed tag (int/bool/callable)"));
            s.push_str(&wat_ins("local.get $rb_i64", "result lo"));
            s.push_str(&wat_ins("i64.const 0", "hi unused"));
            s.push_str(&wat_ins("call $__rt_mixed_from_value", "box into a Mixed cell (increfs callable)"));
            if matches!(php, PhpType::Callable) {
                // from_value incref'd the descriptor; release the body's owned source.
                s.push_str(&wat_ins("local.get $rb_i64", "body's owned descriptor"));
                s.push_str(&wat_ins("i32.wrap_i64", "descriptor -> i32 for decref"));
                s.push_str(&wat_ins("call $__rt_decref_any", "release the body's source ref"));
            }
        }
        IrType::F64 => {
            s.push_str(&wat_ins("local.set $rb_f64", "save the body's f64 result"));
            s.push_str(&wat_ins("i64.const 2", "mixed tag = float"));
            s.push_str(&wat_ins("local.get $rb_f64", "result f64"));
            s.push_str(&wat_ins("i64.reinterpret_f64", "f64 -> i64 bits (lo)"));
            s.push_str(&wat_ins("i64.const 0", "hi unused"));
            s.push_str(&wat_ins("call $__rt_mixed_from_value", "box into a Mixed cell"));
        }
        IrType::Str => {
            s.push_str(&wat_ins("local.set $rb_len", "save result len (top of Str repr)"));
            s.push_str(&wat_ins("local.set $rb_ptr", "save result ptr"));
            s.push_str(&wat_ins("i64.const 1", "mixed tag = string"));
            s.push_str(&wat_ins("local.get $rb_ptr", "result ptr"));
            s.push_str(&wat_ins("i64.extend_i32_u", "widen ptr to i64 (lo)"));
            s.push_str(&wat_ins("local.get $rb_len", "result len (hi)"));
            s.push_str(&wat_ins("call $__rt_mixed_from_value", "persist + box into a Mixed cell"));
            s.push_str(&wat_ins("local.get $rb_ptr", "body's owned source ptr"));
            s.push_str(&wat_ins("call $__rt_decref_any", "release the body's source string"));
        }
        IrType::Heap(kind) => match kind {
            IrHeapKind::Array | IrHeapKind::Hash | IrHeapKind::Object => {
                let tag = match kind {
                    IrHeapKind::Array => 4,
                    IrHeapKind::Hash => 5,
                    _ => 6,
                };
                s.push_str(&wat_ins("local.set $rb_ptr", "save the body's container ptr"));
                s.push_str(&wat_ins(&format!("i64.const {}", tag), "mixed tag (array/assoc/object)"));
                s.push_str(&wat_ins("local.get $rb_ptr", "container ptr"));
                s.push_str(&wat_ins("i64.extend_i32_u", "widen ptr to i64 (lo)"));
                s.push_str(&wat_ins("i64.const 0", "hi unused"));
                s.push_str(&wat_ins("call $__rt_mixed_from_value", "box into a Mixed cell (increfs child)"));
                s.push_str(&wat_ins("local.get $rb_ptr", "body's owned source ptr"));
                s.push_str(&wat_ins("call $__rt_decref_any", "release the body's source container"));
            }
            IrHeapKind::Mixed => {
                // The body's result is already a Mixed cell pointer (i32); forward it.
            }
            IrHeapKind::Iterable | IrHeapKind::Union | IrHeapKind::Buffer => {
                return Err(WasmError::Unsupported(format!(
                    "closure heap result kind {:?} on wasm32-wasi",
                    kind
                )));
            }
        },
        IrType::TaggedScalar => {
            return Err(WasmError::Unsupported(
                "closure tagged-scalar result on wasm32-wasi".to_string(),
            ));
        }
        IrType::Void => {
            s.push_str(&wat_ins("i64.const 8", "mixed tag = null"));
            s.push_str(&wat_ins("i64.const 0", "lo unused"));
            s.push_str(&wat_ins("i64.const 0", "hi unused"));
            s.push_str(&wat_ins("call $__rt_mixed_from_value", "box a null cell for the void result"));
        }
    }
    Ok(s)
}

/// Derives the wrapper symbol for a closure body name `__eir_closure_<owner>_<n>`:
/// `__closure_wrap_<owner>_<n>`, then sanitizes through `wasm_fn_symbol`.
fn wrapper_symbol(closure_name: &str) -> String {
    let tail = closure_name
        .strip_prefix("__eir_closure_")
        .unwrap_or(closure_name);
    wasm_fn_symbol(&format!("__closure_wrap_{}", tail))
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the P7a0 callable-descriptor release runtime, exercised
    //! end-to-end under `wasmer` via hand-written driver functions and `--invoke`.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test builds a reactor module containing the heap + refcount + closure
    //!   runtime (plus the array/mixed/hash/float/object runtimes that `__rt_decref_any`
    //!   may dispatch to), validates it with `wasmparser`, and runs the driver under
    //!   `wasmer`. Runs skip silently when `wasmer` is absent (validation always runs).
    //! - P7a0 covers only the release contract: kind-6 dispatch through
    //!   `__rt_decref_any`, the refcount keep/free paths, and a callable boxed in a
    //!   Mixed cell releasing through the tag-10 arm. Create/call lowering is P7a1.

    use super::emit_closure_runtime;
    use super::super::arrays::emit_array_runtime;
    use super::super::classes::{emit_class_metadata_stub, emit_class_runtime};
    use super::super::heap::emit_heap_runtime;
    use super::super::mixed::emit_mixed_runtime;
    use super::super::objects::{emit_destructor_dispatch_stub, emit_gc_desc_stub, emit_object_runtime};
    use super::super::refcount::emit_refcount_runtime;
    use super::super::wat::WatModule;
    use crate::codegen::platform::Target;
    use crate::ir::{Function, FunctionParam, IrHeapKind, IrType, Module};
    use crate::types::PhpType;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir() -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_p7_{}_{}", std::process::id(), n))
    }

    /// Returns whether the `wasmer` CLI is available.
    fn wasmer_available() -> bool {
        std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Builds a 3-page reactor module with the heap + refcount + closure runtime and the
    /// full `__rt_decref_any` dispatch surface, validates it, and runs `export` under
    /// `wasmer`, returning trimmed stdout. `None` if wasmer is absent; validation
    /// always runs.
    fn run_driver(driver: &str, export: &str) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_refcount_runtime(&mut wm);
        // `__rt_decref_any` dispatches to the array/hash/mixed/object/closure runtimes,
        // so all of them must be present to validate (generate() emits all of them).
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        super::super::hashes::emit_hash_runtime(&mut wm);
        emit_object_runtime(&mut wm);
        emit_gc_desc_stub(&mut wm);
        emit_destructor_dispatch_stub(&mut wm);
        emit_class_metadata_stub(&mut wm);
        emit_class_runtime(&mut wm);
        // `__rt_decref_any` kind-6 dispatches to `__rt_callable_descriptor_release`.
        emit_closure_runtime(&mut wm);
        wm.add_raw_func(driver);
        let wat = wm.render();
        let bytes = ::wat::parse_str(&wat)
            .unwrap_or_else(|e| panic!("WAT did not assemble: {e}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|e| panic!("wasm did not validate: {e}\n{wat}"));
        if !wasmer_available() {
            return None;
        }
        let dir = unique_tmp_dir();
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.wasm");
        std::fs::write(&path, &bytes).expect("write wasm");
        let out = std::process::Command::new("wasmer")
            .arg("run")
            .arg("--invoke")
            .arg(export)
            .arg(&path)
            .output()
            .expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "wasmer --invoke {export} failed: {}\n{}",
            String::from_utf8_lossy(&out.stderr),
            wat
        );
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// A kind-6 descriptor at refcount 1, released through `__rt_decref_any` (the
    /// kind-6 branch), frees the block, so `_gc_live` returns to "0". Proves the
    /// kind-6 dispatch routes to `__rt_callable_descriptor_release` and the rc 1 -> 0
    /// path frees (no captures, so the walk is empty).
    #[test]
    fn decref_any_kind6_frees_descriptor() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $d i32)
  (local.set $d (call $__rt_heap_alloc (i32.const 32)))            ;; 32-byte descriptor (no captures)
  (i64.store (i32.sub (local.get $d) (i32.const 8)) (i64.const 6)) ;; stamp heap-header kind = 6 (callable)
  (i64.store (local.get $d) (i64.const 1))                         ;; descriptor kind = 1 (Closure)
  (i32.store offset=8 (local.get $d) (i32.const 0))               ;; entry_index = 0
  (i32.store offset=12 (local.get $d) (i32.const 0))              ;; capture_count = 0
  (i32.store offset=16 (local.get $d) (i32.const 0))              ;; capture_tags_ptr = 0 (no walk)
  (call $__rt_decref_any (local.get $d))                          ;; kind 6 -> release -> rc 1 -> 0 -> free
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }

    /// A kind-6 descriptor incref'd to refcount 2, then released once via
    /// `__rt_decref_any` (rc 2 -> 1, NOT freed), stays live — proving the above-zero
    /// keep path holds the descriptor (a shared callable is never freed early).
    #[test]
    fn decref_any_kind6_above_zero_keeps_descriptor() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $d i32)
  (local.set $d (call $__rt_heap_alloc (i32.const 32)))
  (i64.store (i32.sub (local.get $d) (i32.const 8)) (i64.const 6)) ;; stamp kind 6
  (call $__rt_incref (local.get $d))                              ;; rc 1 -> 2
  (call $__rt_decref_any (local.get $d))                          ;; rc 2 -> 1, not freed
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_ne!(o, "0");
        }
    }

    /// A callable descriptor boxed in a Mixed cell (tag 10). `__rt_mixed_from_value`
    /// shares ownership (it increfs the refcounted child), so the balanced sequence is:
    /// the caller owns the descriptor (rc 1), boxes it (cell increfs -> rc 2), the
    /// caller releases its own ref (rc 1), then releasing the cell (kind 5 ->
    /// `__rt_decref_mixed` -> tag-10 arm -> `__rt_decref_any` on the child -> kind 6 ->
    /// descriptor release) drops the last ref and frees both the cell and the
    /// descriptor, so `_gc_live` returns to "0". Proves the Mixed-tag-10 path releases a
    /// callable correctly with NO change to `mixed.rs` (the tag-10 arm already calls
    /// `__rt_decref_any`, which now dispatches kind 6).
    #[test]
    fn mixed_tag10_releases_callable_descriptor() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $d i32) (local $c i32)
  (local.set $d (call $__rt_heap_alloc (i32.const 32)))            ;; descriptor
  (i64.store (i32.sub (local.get $d) (i32.const 8)) (i64.const 6)) ;; stamp heap-header kind = 6 (callable)
  (i32.store offset=12 (local.get $d) (i32.const 0))               ;; capture_count = 0
  (i64.const 10)                                                   ;; tag = 10 (callable)
  (i64.extend_i32_u (local.get $d))                                ;; lo = descriptor pointer
  (i64.const 0)                                                    ;; hi = 0
  (call $__rt_mixed_from_value)                                    ;; box: cell increfs the descriptor (rc 1 -> 2)
  (local.set $c)
  (call $__rt_decref_any (local.get $d))                           ;; caller releases its own ref (rc 2 -> 1)
  (call $__rt_decref_any (local.get $c))                           ;; cell kind 5 -> tag-10 -> child kind 6 -> rc 1 -> 0 -> free both
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0");
        }
    }

    // -------------------------------------------------------------------------
    // P7a1: no-capture closure create/call through the wrapper + if-ladder.
    // -------------------------------------------------------------------------

    /// Builds a one-int-param, int-return closure body `__eir_closure_main_0` for the
    /// P7a1 driver tests. The wrapper generated by `emit_closure_dispatch` unboxes the
    /// arg cell via `__rt_mixed_cast_int`, calls this body, and boxes the int result.
    fn int_closure_body_wat() -> &'static str {
        r#"(func $fn___eir_closure_main_0 (param $p1 i64) (result i64)
  (i64.mul (local.get $p1) (i64.const 2)))                              ;; body: return arg * 2
"#
    }

    /// Builds the closure `Function` (name, one int param, int return) that
    /// `emit_closure_dispatch` reads to generate the wrapper + ladder arms.
    fn int_closure_fn() -> Function {
        let mut f = Function::new(
            "__eir_closure_main_0".to_string(),
            IrType::I64,
            PhpType::Int,
        );
        f.params.push(FunctionParam {
            name: "x".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: false,
            variadic: false,
        });
        f
    }

    /// Builds a reactor module with the full runtime surface (so `__rt_decref_any`
    /// validates), the hand-written closure body, the wrappers + `__rt_closure_call`
    /// ladder generated from `closure_fn` via `emit_closure_dispatch`, and the driver.
    /// Validates with `wasmparser` and runs `export` under `wasmer`; `None` if wasmer
    /// is absent (validation always runs).
    fn run_p7a1_driver(
        closure_fn: Function,
        body_wat: &str,
        driver: &str,
        export: &str,
    ) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_refcount_runtime(&mut wm);
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        super::super::hashes::emit_hash_runtime(&mut wm);
        emit_object_runtime(&mut wm);
        emit_gc_desc_stub(&mut wm);
        emit_destructor_dispatch_stub(&mut wm);
        emit_class_metadata_stub(&mut wm);
        emit_class_runtime(&mut wm);
        emit_closure_runtime(&mut wm);
        wm.add_raw_func(body_wat);
        let mut module = Module::new(Target::wasm());
        module.closures.push(closure_fn);
        super::emit_closure_dispatch(&mut wm, &module).expect("emit_closure_dispatch");
        wm.add_raw_func(driver);
        let wat = wm.render();
        let bytes = ::wat::parse_str(&wat)
            .unwrap_or_else(|e| panic!("WAT did not assemble: {e}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|e| panic!("wasm did not validate: {e}\n{wat}"));
        if !wasmer_available() {
            return None;
        }
        let dir = unique_tmp_dir();
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.wasm");
        std::fs::write(&path, &bytes).expect("write wasm");
        let out = std::process::Command::new("wasmer")
            .arg("run")
            .arg("--invoke")
            .arg(export)
            .arg(&path)
            .output()
            .expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "wasmer --invoke {export} failed: {}\n{}",
            String::from_utf8_lossy(&out.stderr),
            wat
        );
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// A no-capture closure `(int $x) -> int { $x * 2 }`, called with arg 42 through the
    /// full P7a1 path (descriptor alloc -> Mixed-cell arg buffer -> `__rt_closure_call`
    /// if-ladder -> wrapper unbox -> body -> wrapper box -> caller unbox), returns 84.
    /// Proves the wrapper + ladder + arg-buffer lowering produce the correct result.
    #[test]
    fn closure_call_int_doubles_arg() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $desc i32) (local $args i32) (local $cell i32) (local $rcell i32)
  (local.set $desc (call $__rt_heap_alloc (i32.const 32)))            ;; descriptor (rc 1)
  (i64.store (i32.sub (local.get $desc) (i32.const 8)) (i64.const 6)) ;; stamp kind 6 (callable)
  (i64.store (local.get $desc) (i64.const 1))                         ;; descriptor kind = 1 (Closure)
  (i32.store offset=8 (local.get $desc) (i32.const 0))               ;; entry_index = 0
  (i32.store offset=12 (local.get $desc) (i32.const 0))              ;; capture_count = 0
  (i32.store offset=16 (local.get $desc) (i32.const 0))              ;; capture_tags_ptr = 0
  (local.set $args (call $__rt_array_new (i64.const 1) (i64.const 16)))  ;; 1-slot arg buffer
  (i64.const 0) (i64.const 42) (i64.const 0) (call $__rt_mixed_from_value)  ;; box int 42 -> cell
  (local.set $cell)
  (local.set $args (call $__rt_array_push_mixed (local.get $args) (local.get $cell)))  ;; append cell
  (local.set $rcell (call $__rt_closure_call (local.get $desc) (local.get $args)))     ;; dispatch -> result cell
  (call $__rt_mixed_cast_int (local.get $rcell)))                    ;; unbox result int -> 84
"#;
        if let Some(o) = run_p7a1_driver(int_closure_fn(), int_closure_body_wat(), driver, "t") {
            assert_eq!(o, "84");
        }
    }

    /// The same closure call, fully balanced: after unboxing the result, the driver
    /// releases the result cell, the arg buffer (whose deep free releases the arg cell),
    /// and the descriptor, leaving `_gc_live` at "0". Proves the P7a1 create/call path
    /// is refcount-balanced end-to-end (no descriptor/cell/array leak).
    #[test]
    fn closure_call_int_balanced_gc() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $desc i32) (local $args i32) (local $cell i32) (local $rcell i32)
  (local.set $desc (call $__rt_heap_alloc (i32.const 32)))
  (i64.store (i32.sub (local.get $desc) (i32.const 8)) (i64.const 6))
  (i64.store (local.get $desc) (i64.const 1))
  (i32.store offset=8 (local.get $desc) (i32.const 0))
  (i32.store offset=12 (local.get $desc) (i32.const 0))
  (i32.store offset=16 (local.get $desc) (i32.const 0))
  (local.set $args (call $__rt_array_new (i64.const 1) (i64.const 16)))
  (i64.const 0) (i64.const 42) (i64.const 0) (call $__rt_mixed_from_value)
  (local.set $cell)
  (local.set $args (call $__rt_array_push_mixed (local.get $args) (local.get $cell)))
  (local.set $rcell (call $__rt_closure_call (local.get $desc) (local.get $args)))
  (call $__rt_mixed_cast_int (local.get $rcell))                     ;; 84 (borrowed read of the cell)
  drop                                                                ;; discard the result value
  (call $__rt_decref_any (local.get $rcell))                         ;; release the result cell
  (call $__rt_decref_any (local.get $args))                          ;; release the arg buffer (frees the arg cell)
  (call $__rt_decref_any (local.get $desc))                          ;; release the descriptor
  (global.get $_gc_live))                                             ;; expect 0 (balanced)
"#;
        if let Some(o) = run_p7a1_driver(int_closure_fn(), int_closure_body_wat(), driver, "t") {
            assert_eq!(o, "0");
        }
    }

    // -------------------------------------------------------------------------
    // P7b: by-value capture refcount balance (string capture, explicit release).
    // -------------------------------------------------------------------------

    /// Builds a one-Str-capture closure body `__eir_closure_cap_gc_0` for the P7b
    /// balance driver: it acquires the captured string (modeling EIR Edit 1's
    /// return-boundary `Op::Acquire`) and returns it unchanged. The wrapper now passes the
    /// capture as a borrow (no unbox incref), so the body's own incref is the owned ref
    /// `box_result_wat` releases after persisting a copy for the result cell. Body params
    /// are `(ptr i32, len i64)` — the wrapper's `unbox_capture_wat` Str arm pushes them in
    /// that order.
    fn str_capture_body_wat() -> &'static str {
        r#"(func $fn___eir_closure_cap_gc_0 (param $cp i32) (param $cl i64) (result i32) (result i64)
  (call $__rt_incref (local.get $cp))                                   ;; EDIT 1: acquire the capture for the return value
  (local.get $cp)                                                       ;; return the captured string pointer
  (local.get $cl))                                                      ;; return the captured string length
"#
    }

    /// Builds the closure `Function` (name, one Str capture param, Str return) that
    /// `emit_closure_dispatch` reads to generate the capture-aware wrapper + ladder arm.
    fn str_capture_fn() -> Function {
        let mut f = Function::new(
            "__eir_closure_cap_gc_0".to_string(),
            IrType::Str,
            PhpType::Str,
        );
        f.flags.is_closure = true;
        f.flags.closure_capture_count = 1;
        f.params.push(FunctionParam {
            name: "s".to_string(),
            ir_type: IrType::Str,
            php_type: PhpType::Str,
            by_ref: false,
            variadic: false,
        });
        f
    }

    /// Like `run_p7a1_driver`, but also emits the per-closure capture-tag byte array
    /// (at offset 512) and an optional literal string (at offset 600) into static memory
    /// below the heap (heap_base = 1024), so a capture-bearing driver can stamp a real
    /// `capture_tags_ptr` and persist a real capture value. `tag_byte` is the single
    /// capture's tag (1 = string, 10 = callable, ...); `literal` is the optional string
    /// literal body for string-capture drivers. Validates with `wasmparser` and runs
    /// `export` under `wasmer`; `None` if wasmer is absent.
    fn run_p7b_capture_driver(
        closure_fn: Function,
        body_wat: &str,
        driver: &str,
        export: &str,
        tag_byte: u8,
        literal: Option<&[u8]>,
    ) -> Option<String> {
        use super::super::wat::DataSegment;
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_refcount_runtime(&mut wm);
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        super::super::hashes::emit_hash_runtime(&mut wm);
        emit_object_runtime(&mut wm);
        emit_gc_desc_stub(&mut wm);
        emit_destructor_dispatch_stub(&mut wm);
        emit_class_metadata_stub(&mut wm);
        emit_class_runtime(&mut wm);
        emit_closure_runtime(&mut wm);
        // Static data for the capture driver: the single capture's tag byte at 512,
        // and an optional string literal at 600 (used by string-capture drivers).
        wm.add_data(DataSegment {
            offset: 512,
            bytes: vec![tag_byte],
        });
        if let Some(lit) = literal {
            wm.add_data(DataSegment {
                offset: 600,
                bytes: lit.to_vec(),
            });
        }
        wm.add_raw_func(body_wat);
        let mut module = Module::new(Target::wasm());
        module.closures.push(closure_fn);
        super::emit_closure_dispatch(&mut wm, &module).expect("emit_closure_dispatch");
        wm.add_raw_func(driver);
        let wat = wm.render();
        let bytes = ::wat::parse_str(&wat)
            .unwrap_or_else(|e| panic!("WAT did not assemble: {e}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|e| panic!("wasm did not validate: {e}\n{wat}"));
        if !wasmer_available() {
            return None;
        }
        let dir = unique_tmp_dir();
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.wasm");
        std::fs::write(&path, &bytes).expect("write wasm");
        let out = std::process::Command::new("wasmer")
            .arg("run")
            .arg("--invoke")
            .arg(export)
            .arg(&path)
            .output()
            .expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "wasmer --invoke {export} failed: {}\n{}",
            String::from_utf8_lossy(&out.stderr),
            wat
        );
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// A one-Str-capture closure, created and called through the full P7b path with
    /// explicit release of the result cell, the arg buffer, and the descriptor, leaves
    /// `_gc_live` at "0". The driver manually stamps the descriptor (mirroring
    /// `lower_closure_new`): `__rt_str_persist` makes an owned copy for the descriptor,
    /// stored in slot 0 with `capture_tags_ptr` pointing at the static tag array. The
    /// generated wrapper unboxes the capture (incref for the body), the body returns it,
    /// the wrapper boxes the result (persist + release the body's source), and the
    /// driver releases the result cell, arg buffer, and descriptor (whose tag-1 walk
    /// frees the descriptor's persisted copy). Proves the by-value string capture is
    /// refcount-balanced end-to-end (no descriptor/cell/string leak).
    #[test]
    fn closure_capture_str_balanced_gc() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $desc i32) (local $args i32) (local $rcell i32) (local $cp i32) (local $cl i64)
  (local.set $desc (call $__rt_heap_alloc (i32.const 48)))            ;; descriptor (32 + 1 capture slot)
  (i64.store (i32.sub (local.get $desc) (i32.const 8)) (i64.const 6)) ;; stamp heap-header kind = 6 (callable)
  (i64.store (local.get $desc) (i64.const 1))                         ;; descriptor kind = 1 (Closure)
  (i32.store offset=8 (local.get $desc) (i32.const 0))               ;; entry_index = 0
  (i32.store offset=12 (local.get $desc) (i32.const 1))              ;; capture_count = 1
  (i32.store offset=16 (local.get $desc) (i32.const 512))            ;; capture_tags_ptr = static tag array [1]
  (call $__rt_str_persist (i32.const 600) (i64.const 2))             ;; persist "hi" -> (ptr i32, len i64)
  (local.set $cl)                                                    ;; save owned copy length
  (local.set $cp)                                                    ;; save owned copy pointer
  (i32.store offset=32 (local.get $desc) (local.get $cp))            ;; store capture string ptr in slot 0 low 4
  (i32.store offset=40 (local.get $desc) (i32.wrap_i64 (local.get $cl))) ;; store capture string len in slot 0 high 4
  (local.set $args (call $__rt_array_new (i64.const 0) (i64.const 16))) ;; empty arg buffer (no visible args)
  (local.set $rcell (call $__rt_closure_call (local.get $desc) (local.get $args))) ;; dispatch -> result cell
  (call $__rt_decref_any (local.get $rcell))                         ;; release the result cell (frees its persisted copy)
  (call $__rt_decref_any (local.get $args))                          ;; release the empty arg buffer
  (call $__rt_decref_any (local.get $desc))                          ;; release the descriptor (tag-1 walk frees its copy)
  (global.get $_gc_live))                                            ;; expect 0 (balanced)
"#;
        if let Some(o) =
            run_p7b_capture_driver(str_capture_fn(), str_capture_body_wat(), driver, "t", 1, Some(b"hi"))
        {
            assert_eq!(o, "0");
        }
    }

    // -------------------------------------------------------------------------
    // P7b: by-value callable capture refcount balance (tag 10, the release-walk
    // recursion path that the string test does not exercise). Verifies by execution
    // the audit's "no double-free" claim for callable captures: the descriptor's
    // tag-10 walk releases the captured descriptor exactly once.
    // -------------------------------------------------------------------------

    /// Builds the one-Callable-capture closure body `__eir_closure_cap_call_gc_0` for the
    /// callable balance driver: it acquires the captured descriptor (modeling EIR Edit 1's
    /// return-boundary `Op::Acquire`) and returns it unchanged (an I64). The wrapper now
    /// passes the capture as a borrow (no unbox incref), so the body's own incref is the
    /// owned ref `box_result_wat`'s Callable arm releases after increfing again for the
    /// result cell. Body param is `(param $cap i64)` — the wrapper's `unbox_capture_wat`
    /// Callable arm pushes the descriptor i64.
    fn callable_capture_body_wat() -> &'static str {
        r#"(func $fn___eir_closure_cap_call_gc_0 (param $cap i64) (result i64)
  (call $__rt_incref (i32.wrap_i64 (local.get $cap)))                   ;; EDIT 1: acquire the capture for the return value
  (local.get $cap))                                                     ;; return the captured callable descriptor (i64)
"#
    }

    /// Builds the closure `Function` (name, one Callable capture param, Callable return)
    /// that `emit_closure_dispatch` reads to generate the capture-aware wrapper + ladder
    /// arm. Callable carries as `IrType::I64` with `PhpType::Callable`.
    fn callable_capture_fn() -> Function {
        let mut f = Function::new(
            "__eir_closure_cap_call_gc_0".to_string(),
            IrType::I64,
            PhpType::Callable,
        );
        f.flags.is_closure = true;
        f.flags.closure_capture_count = 1;
        f.params.push(FunctionParam {
            name: "c".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Callable,
            by_ref: false,
            variadic: false,
        });
        f
    }

    /// A one-Callable-capture closure, created and called through the full P7b path with
    /// explicit release of the result cell, the arg buffer, the outer descriptor, and the
    /// captured inner descriptor, leaves `_gc_live` at "0". The driver hand-stamps a
    /// minimal inner callable descriptor (kind 6, no captures) and an outer descriptor
    /// that captures it (tag 10), mirroring `lower_closure_new`'s MaybeOwned-incref stamp
    /// arm: the inner is `incref`'d before being stored in the slot, so the outer owns one
    /// ref and the driver retains its allocation ref. The generated wrapper unboxes the
    /// capture (incref for the body), the body returns it, the wrapper boxes the result
    /// (Callable: incref for the cell + release the body's source), and the driver
    /// releases the result cell, the arg buffer, the outer descriptor (whose tag-10 walk
    /// recurses through `__rt_callable_descriptor_release` to release the captured ref),
    /// and finally its own inner ref. Proves the by-value callable capture is
    /// refcount-balanced end-to-end with no double-free and no leak.
    #[test]
    fn closure_capture_callable_balanced_gc() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $inner i32) (local $desc i32) (local $args i32) (local $rcell i32)
  (local.set $inner (call $__rt_heap_alloc (i32.const 32)))           ;; inner callable descriptor (no captures)
  (i64.store (i32.sub (local.get $inner) (i32.const 8)) (i64.const 6)) ;; stamp inner heap-header kind = 6 (callable)
  (i64.store (local.get $inner) (i64.const 1))                         ;; inner descriptor kind = 1 (Closure)
  (i32.store offset=8 (local.get $inner) (i32.const 0))               ;; inner entry_index = 0 (unused here)
  (i32.store offset=12 (local.get $inner) (i32.const 0))              ;; inner capture_count = 0
  (i32.store offset=16 (local.get $inner) (i32.const 0))              ;; inner capture_tags_ptr = 0 (no tags)
  (local.set $desc (call $__rt_heap_alloc (i32.const 48)))            ;; outer descriptor (32 + 1 capture slot)
  (i64.store (i32.sub (local.get $desc) (i32.const 8)) (i64.const 6)) ;; stamp outer heap-header kind = 6 (callable)
  (i64.store (local.get $desc) (i64.const 1))                         ;; outer descriptor kind = 1 (Closure)
  (i32.store offset=8 (local.get $desc) (i32.const 0))               ;; outer entry_index = 0 (only closure)
  (i32.store offset=12 (local.get $desc) (i32.const 1))              ;; outer capture_count = 1
  (i32.store offset=16 (local.get $desc) (i32.const 512))            ;; outer capture_tags_ptr = static tag array [10]
  (call $__rt_incref (local.get $inner))                              ;; retain a ref for the descriptor (MaybeOwned stamp arm)
  (i64.store offset=32 (local.get $desc) (i64.extend_i32_u (local.get $inner))) ;; store captured callable ptr in slot 0
  (local.set $args (call $__rt_array_new (i64.const 0) (i64.const 16))) ;; empty arg buffer (no visible args)
  (local.set $rcell (call $__rt_closure_call (local.get $desc) (local.get $args))) ;; dispatch -> result cell
  (call $__rt_decref_any (local.get $rcell))                         ;; release the result cell (frees its ref on inner)
  (call $__rt_decref_any (local.get $args))                          ;; release the empty arg buffer
  (call $__rt_decref_any (local.get $desc))                          ;; release the outer descriptor (tag-10 walk releases its inner ref)
  (call $__rt_decref_any (local.get $inner))                         ;; release the driver's own inner allocation ref
  (global.get $_gc_live))                                            ;; expect 0 (balanced)
"#;
        if let Some(o) = run_p7b_capture_driver(
            callable_capture_fn(),
            callable_capture_body_wat(),
            driver,
            "t",
            10,
            None,
        ) {
            assert_eq!(o, "0");
        }
    }

    // -------------------------------------------------------------------------
    // P7d0: by-value callable capture non-return path — refcount leak guard.
    // Proves Edit 2 (unbox incref removal) for the case where the capture is NOT
    // returned: any spurious wrapper incref permanently elevates the captured
    // descriptor's refcount and leaves it un-freed after all explicit releases.
    // -------------------------------------------------------------------------

    /// Builds the closure body `__eir_closure_cap_nr_0` for the non-return
    /// callable capture driver: ignores the captured callable descriptor and
    /// returns a constant int 42. No incref here — Edit 1 only acquires a capture
    /// that IS returned by the body; since the capture is discarded, a spurious
    /// wrapper unbox incref (pre-Edit-2) would be uncompensated, leaking the
    /// captured descriptor's refcount permanently.
    fn nr_capture_body_wat() -> &'static str {
        r#"(func $fn___eir_closure_cap_nr_0 (param $cap i64) (result i64)
  (i64.const 42))                                                       ;; ignore the capture; return an int (non-return path)
"#
    }

    /// Builds the closure `Function` (name, one Callable capture param, Int return)
    /// that `emit_closure_dispatch` reads to generate the capture-aware wrapper +
    /// ladder arm. The return type is `IrType::I64` / `PhpType::Int` — the body
    /// returns a plain int rather than the captured callable, so `box_result_wat`
    /// emits a kind-5 int cell that carries no reference to the inner descriptor.
    fn nr_capture_fn() -> Function {
        let mut f = Function::new(
            "__eir_closure_cap_nr_0".to_string(),
            IrType::I64,
            PhpType::Int,
        );
        f.flags.is_closure = true;
        f.flags.closure_capture_count = 1;
        f.params.push(FunctionParam {
            name: "c".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Callable,
            by_ref: false,
            variadic: false,
        });
        f
    }

    /// A one-Callable-capture closure that does NOT return the capture, called
    /// through the full P7d0 path, leaves `_gc_live` at "0". The driver stamps an
    /// inner callable descriptor (no captures, alloc rc = 1) and an outer descriptor
    /// that captures it (tag 10), increfing `inner` before storing it in the slot
    /// so the outer owns one ref and the driver retains its own alloc ref (inner
    /// rc = 2 after setup). The call dispatches through the generated wrapper; the
    /// wrapper unboxes the capture as a borrow (Edit 2: no unbox incref), the body
    /// ignores it and returns the int 42, and the wrapper boxes it into a kind-5 int
    /// result cell. The driver then releases: the int result cell (inner untouched),
    /// the arg buffer, the outer descriptor (tag-10 walk releases the captured inner
    /// ref, rc 2→1), and the driver's own inner ref (rc 1→0, freed). Pre-Edit-2 the
    /// wrapper would incref inner during unbox (rc 2→3), the non-return body would
    /// not compensate, the tag-10 walk brings rc 3→2, and the driver decref brings
    /// rc 2→1 — inner is never freed, `_gc_live != 0`.
    #[test]
    fn closure_capture_callable_nonreturn_gc() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $inner i32) (local $desc i32) (local $args i32) (local $rcell i32)
  (local.set $inner (call $__rt_heap_alloc (i32.const 32)))           ;; inner callable descriptor (no captures)
  (i64.store (i32.sub (local.get $inner) (i32.const 8)) (i64.const 6)) ;; stamp inner heap-header kind = 6 (callable)
  (i64.store (local.get $inner) (i64.const 1))                         ;; inner descriptor kind = 1 (Closure)
  (i32.store offset=8 (local.get $inner) (i32.const 0))               ;; inner entry_index = 0 (unused here)
  (i32.store offset=12 (local.get $inner) (i32.const 0))              ;; inner capture_count = 0
  (i32.store offset=16 (local.get $inner) (i32.const 0))              ;; inner capture_tags_ptr = 0 (no tags)
  (local.set $desc (call $__rt_heap_alloc (i32.const 48)))            ;; outer descriptor (32 + 1 capture slot)
  (i64.store (i32.sub (local.get $desc) (i32.const 8)) (i64.const 6)) ;; stamp outer heap-header kind = 6 (callable)
  (i64.store (local.get $desc) (i64.const 1))                         ;; outer descriptor kind = 1 (Closure)
  (i32.store offset=8 (local.get $desc) (i32.const 0))               ;; outer entry_index = 0 (only closure)
  (i32.store offset=12 (local.get $desc) (i32.const 1))              ;; outer capture_count = 1
  (i32.store offset=16 (local.get $desc) (i32.const 512))            ;; outer capture_tags_ptr = static tag array [10]
  (call $__rt_incref (local.get $inner))                              ;; retain a ref for the descriptor (MaybeOwned stamp arm)
  (i64.store offset=32 (local.get $desc) (i64.extend_i32_u (local.get $inner))) ;; store captured callable ptr in slot 0
  (local.set $args (call $__rt_array_new (i64.const 0) (i64.const 16))) ;; empty arg buffer (no visible args)
  (local.set $rcell (call $__rt_closure_call (local.get $desc) (local.get $args))) ;; dispatch -> int result cell (kind 5)
  (call $__rt_decref_any (local.get $rcell))                         ;; release the int result cell (does NOT touch inner)
  (call $__rt_decref_any (local.get $args))                          ;; release the empty arg buffer
  (call $__rt_decref_any (local.get $desc))                          ;; release the outer descriptor (tag-10 walk releases inner ref)
  (call $__rt_decref_any (local.get $inner))                         ;; release the driver's own inner allocation ref
  (global.get $_gc_live))                                            ;; expect 0 (inner rc reaches 0 and is freed)
"#;
        if let Some(o) = run_p7b_capture_driver(
            nr_capture_fn(),
            nr_capture_body_wat(),
            driver,
            "t",
            10,
            None,
        ) {
            assert_eq!(o, "0");
        }
    }

    // -------------------------------------------------------------------------
    // P7d1b: Iterable by-value capture refcount balance — Edit 4 load-bearing
    // guard. Proves that tag 12 in the release condition is necessary: without
    // it the captured array is never released and `_gc_live` is non-zero.
    // -------------------------------------------------------------------------

    /// Builds the one-Iterable-capture closure body `__eir_closure_cap_iter_gc_0`:
    /// receives the captured array pointer as a borrow (single i32, `WasmRepr::Ptr`
    /// for `IrType::Heap(IrHeapKind::Iterable)`) and returns a constant int 42,
    /// ignoring the capture. The non-return path is the sensitive one: the wrapper
    /// passes the Iterable as a borrow (no incref), the body does not touch it, and
    /// the only release is via the descriptor's tag-12 release walk (Edit 4).
    fn iterable_gc_body_wat() -> &'static str {
        r#"(func $fn___eir_closure_cap_iter_gc_0 (param $arr i32) (result i64)
  (i64.const 42))                                                     ;; ignore the capture; return a constant int
"#
    }

    /// Builds the closure `Function` (name, one Iterable capture param, Int return)
    /// that `emit_closure_dispatch` reads to generate the capture-aware wrapper +
    /// ladder arm. Iterable carries as `IrType::Heap(IrHeapKind::Iterable)` with
    /// `PhpType::Iterable`; the wrapper's `unbox_capture_wat` Heap arm (Edit 3)
    /// pushes the raw i32 ptr as a borrow for the body.
    fn iterable_gc_fn() -> Function {
        let mut f = Function::new(
            "__eir_closure_cap_iter_gc_0".to_string(),
            IrType::I64,
            PhpType::Int,
        );
        f.flags.is_closure = true;
        f.flags.closure_capture_count = 1;
        f.params.push(FunctionParam {
            name: "arr".to_string(),
            ir_type: IrType::Heap(IrHeapKind::Iterable),
            php_type: PhpType::Iterable,
            by_ref: false,
            variadic: false,
        });
        f
    }

    /// A one-Iterable-capture closure, created and called through the full P7d1b
    /// path with explicit release of the result cell, the arg buffer, the
    /// descriptor, and the driver's own array ref, leaves `_gc_live` at "0". The
    /// driver allocates a real `__rt_array_new` pointer (kind 2, rc = 1), increfs
    /// it before storing into the descriptor slot so the descriptor owns one ref
    /// (rc = 2), calls the closure (wrapper borrows the array, body ignores it,
    /// no incref), then releases: int result cell, empty arg buffer, the descriptor
    /// (tag-12 walk calls `__rt_decref_any(arr)` → rc 2→1), and finally the
    /// driver's own array ref (rc 1→0, freed). Proves Edit 4 (tag 12 in the release
    /// set) actually frees the captured array.
    ///
    /// NEGATIVE CONTROL: temporarily reverting Edit 4 (removing tag 12 from the
    /// condition) causes the descriptor release walk to skip the Iterable slot →
    /// array rc stays at 1 after the driver decref → array not freed → `_gc_live`
    /// returns a non-zero value. Report: pass=0, Edit-4-reverted=1.
    #[test]
    fn closure_capture_iterable_balanced_gc() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $arr i32) (local $desc i32) (local $args i32) (local $rcell i32)
  (local.set $arr (call $__rt_array_new (i64.const 0) (i64.const 16)))  ;; real array (kind 2), rc = 1 from alloc
  (local.set $desc (call $__rt_heap_alloc (i32.const 48)))              ;; descriptor (32 + 1 capture slot), rc = 1
  (i64.store (i32.sub (local.get $desc) (i32.const 8)) (i64.const 6))  ;; stamp heap-header kind = 6 (callable)
  (i64.store (local.get $desc) (i64.const 1))                          ;; descriptor kind = 1 (Closure)
  (i32.store offset=8 (local.get $desc) (i32.const 0))                 ;; entry_index = 0 (only closure)
  (i32.store offset=12 (local.get $desc) (i32.const 1))                ;; capture_count = 1
  (i32.store offset=16 (local.get $desc) (i32.const 512))              ;; capture_tags_ptr = static tag array [12]
  (call $__rt_incref (local.get $arr))                                  ;; retain a ref for the descriptor (MaybeOwned stamp arm), array rc = 2
  (i64.store offset=32 (local.get $desc) (i64.extend_i32_u (local.get $arr))) ;; store captured Iterable ptr in slot 0
  (local.set $args (call $__rt_array_new (i64.const 0) (i64.const 16))) ;; empty arg buffer (no visible args)
  (local.set $rcell (call $__rt_closure_call (local.get $desc) (local.get $args))) ;; dispatch -> int result cell (kind 5)
  (call $__rt_decref_any (local.get $rcell))                           ;; release the int result cell (does NOT touch arr)
  (call $__rt_decref_any (local.get $args))                            ;; release the empty arg buffer
  (call $__rt_decref_any (local.get $desc))                            ;; release the descriptor (tag-12 walk releases arr ref: rc 2->1)
  (call $__rt_decref_any (local.get $arr))                             ;; release the driver's own array ref (rc 1->0, freed)
  (global.get $_gc_live))                                              ;; expect 0 (array freed, no leak)
"#;
        if let Some(o) = run_p7b_capture_driver(
            iterable_gc_fn(),
            iterable_gc_body_wat(),
            driver,
            "t",
            12,
            None,
        ) {
            assert_eq!(o, "0");
        }
    }
}