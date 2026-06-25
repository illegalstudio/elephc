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
//!   that releases each refcounted slot (tag in {1,4,5,6,7,10} = str/array/assoc/object/
//!   mixed/callable) via the kind-dispatched `__rt_decref_any` (so a callable capture
//!   recurses through kind-6), and finally `__rt_heap_free` (unsafe; refcount already 0).
//!   By-ref captures use tag sentinel 0xFF and are skipped (the promoted cell outlives
//!   the closure). P7a0 descriptors have capture_count 0, so the walk is a no-op today;
//!   the full walk is emitted now so P7b only needs `ClosureNew` to populate slots.
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
use super::inst::{data_immediate, operand, store_result};
use super::objects::emit_box_value_into_mixed;
use super::values::WasmRepr;
use super::wat::{ValType, WatModule};
use super::WasmError;
use crate::ir::{Function, Instruction, IrHeapKind, IrType, Module, ValueId};
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
      ;; refcounted tags: 1 (str), 4 (array), 5 (assoc), 6 (object), 7 (mixed), 10 (callable).
      ;; Scalars (0/2/3), null (8), and the by-ref sentinel (0xFF) own no heap storage.
      (if (i32.or (i32.or (i32.eq (local.get $tag) (i32.const 1)) (i32.and (i32.ge_u (local.get $tag) (i32.const 4)) (i32.le_u (local.get $tag) (i32.const 7)))) (i32.eq (local.get $tag) (i32.const 10))) (then  ;; tag in {1,4,5,6,7,10} -> release the slot
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

/// Lowers `Op::ClosureNew` for a no-capture closure: allocates a kind-6 descriptor,
/// stamps its payload (descriptor kind 1, the closure's `entry_index`, capture_count 0),
/// and stores the zero-extended pointer into the result's `I64` local.
///
/// The closure name is carried by an `Immediate::Data` index into the module's string
/// pool (the same pool `ClosureNew` interns the `__eir_closure_<owner>_<n>` name into at
/// lowering time). The `entry_index` is the closure `Function`'s position in
/// `module.closures`, which the `__rt_closure_call` if-ladder keys on.
///
/// P7a1 rejects captures (`operands` non-empty → P7b) and any by-ref/variadic visible
/// parameter (m10): the wrapper forwards Owned by-value args only. The release runtime
/// already walks capture slots, so P7b only needs this path to populate them.
pub(super) fn lower_closure_new(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(WasmError::Unsupported(
            "ClosureNew with captures (P7b) on wasm32-wasi".to_string(),
        ));
    }
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
    // No-capture closures expose every parameter as a visible by-value arg; reject
    // by-ref/variadic params (the wrapper cannot forward them) until P7c/P7-c0.
    for p in &closure_fn.params {
        if p.by_ref || p.variadic {
            return Err(WasmError::Unsupported(format!(
                "ClosureNew by-ref/variadic param {} on wasm32-wasi (P7c)",
                p.name
            )));
        }
    }

    let desc = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!("(call $__rt_heap_alloc (i32.const {}))", DESCRIPTOR_PAYLOAD_BYTES),
        "allocate 32-byte callable descriptor (refcount 1)",
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
        &format!("(i32.store offset=12 (local.get {}) (i32.const 0))", desc),
        "capture_count = 0 (no-capture closure)",
    );
    ctx.fb.ins(
        &format!("(i32.store offset=16 (local.get {}) (i32.const 0))", desc),
        "capture_tags_ptr = 0 (no capture walk)",
    );
    ctx.fb.ins(&format!("local.get {}", desc), "descriptor pointer");
    ctx.fb.ins("i64.extend_i32_u", "zero-extend ptr -> i64 callable value");
    store_result(ctx, inst)
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
/// buffer, call the body, box the result, and return the result cell.
fn build_closure_wrapper(wrapper_symbol: &str, f: &Function) -> Result<String> {
    let body_symbol = wasm_fn_symbol(&f.name);
    let mut wat = String::new();
    wat.push_str(&format!(
        "(func ${} (param $desc i32) (param $args i32) (result i32)\n",
        wrapper_symbol
    ));
    // Shared unbox/box locals (reused per arg/result; each value is pushed before reuse).
    wat.push_str("  (local $ub_tag i64) (local $ub_lo i64) (local $ub_hi i64)\n");
    wat.push_str("  (local $rb_i64 i64) (local $rb_f64 f64) (local $rb_ptr i32) (local $rb_len i64)\n");

    // Unbox each visible parameter and push it for the body call (val-type order).
    for (i, p) in f.params.iter().enumerate() {
        let slot_off = 24 + i * 16;
        wat.push_str(&format!(
            "  ;; unbox arg {} (param {} : {:?}) from slot +{}\n",
            i, p.name, p.ir_type, slot_off
        ));
        wat.push_str(&format!(
            "  (i32.wrap_i64 (i64.load offset={} (local.get $args)))\n",
            slot_off
        ));
        wat.push_str(&unbox_arg_wat(&p.ir_type, &p.php_type)?);
    }

    // Call the closure body with the forwarded args on the stack.
    wat.push_str(&format!("  call ${}\n", body_symbol));
    wat.push_str("  ;; box the body result into a Mixed cell (result i32)\n");

    // Box the body result into a Mixed cell and leave it as the (result i32) return value.
    wat.push_str(&box_result_wat(&f.return_type, &f.return_php_type)?);
    wat.push_str(")\n");
    Ok(wat)
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
    use crate::ir::{Function, FunctionParam, IrType, Module};
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
}