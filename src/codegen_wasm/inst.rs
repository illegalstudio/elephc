//! Purpose:
//! Lowers scalar EIR instructions (the `Op` enum subset) to WebAssembly for the
//! wasm32-wasi backend: integer/float arithmetic, comparisons, conversions,
//! truthiness/null predicates, constants, and local-variable access.
//!
//! Called from:
//! - `crate::codegen_wasm::function::emit_dispatch_loop` for each instruction in a
//!   block, before the block's terminator.
//!
//! Key details:
//! - Each value-producing op loads its operands onto the WASM operand stack,
//!   computes the result, then stores it into the result value's local(s).
//! - `IDiv` is PHP `/`, which always yields a float; both i64 operands are widened
//!   with `f64.convert_i64_s` before `f64.div`.
//! - Float constants are emitted bit-exactly (`i64.const <bits>; f64.reinterpret_i64`)
//!   to avoid any float-literal formatting ambiguity.
//! - Borrow rule: `value_repr`/`slot_repr` borrow `ctx`; clone the needed strings
//!   (via `local_refs()` or `.clone()`) before calling a `&mut self` method.

use super::context::{wasm_fn_symbol, FnCtx, Result};
use super::values::WasmRepr;
use super::wat::ValType;
use super::WasmError;
use crate::ir::{
    CmpPredicate, DataId, Immediate, InstId, Instruction, IrHeapKind, IrType, LocalSlotId, Op,
    Ownership, ValueDef, ValueId,
};
use crate::types::PhpType;
use std::collections::HashMap;

/// Lowers one EIR instruction by id. Loads operands, computes the result on the
/// WASM operand stack, and stores it into the result value's local(s). Unsupported
/// ops return `WasmError::Unsupported` so the pipeline can surface a clean diagnostic.
pub(super) fn lower_instruction(ctx: &mut FnCtx, inst_id: InstId) -> Result<()> {
    // Clone the instruction so we can mutate ctx.fb without holding a borrow on ctx.function.
    let inst = ctx
        .function
        .instruction(inst_id)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("missing instruction {:?}", inst_id)))?;

    match inst.op {
        Op::ConstI64 => lower_const_i64(ctx, &inst),
        Op::ConstF64 => lower_const_f64(ctx, &inst),
        Op::ConstBool => lower_const_bool(ctx, &inst),
        Op::ConstNull => lower_const_null(ctx, &inst),
        Op::ConstStr => lower_const_str(ctx, &inst),
        Op::StrLen => lower_strlen(ctx, &inst),
        Op::StrConcat => lower_str_concat(ctx, &inst),
        Op::Nop => lower_nop(ctx),
        Op::ConcatReset => lower_concat_reset(ctx),
        Op::LoadLocal => lower_load_local(ctx, &inst),
        Op::StoreLocal => lower_store_local(ctx, &inst),
        Op::IAdd => lower_int_binop(ctx, &inst, "i64.add"),
        Op::ISub => lower_int_binop(ctx, &inst, "i64.sub"),
        Op::IMul => lower_int_binop(ctx, &inst, "i64.mul"),
        Op::IBitAnd => lower_int_binop(ctx, &inst, "i64.and"),
        Op::IBitOr => lower_int_binop(ctx, &inst, "i64.or"),
        Op::IBitXor => lower_int_binop(ctx, &inst, "i64.xor"),
        Op::IShl => lower_int_binop(ctx, &inst, "i64.shl"),
        Op::IShrA => lower_int_binop(ctx, &inst, "i64.shr_s"),
        Op::ISDiv => lower_int_binop(ctx, &inst, "i64.div_s"),
        Op::ISMod => lower_int_binop(ctx, &inst, "i64.rem_s"),
        Op::INeg => lower_int_neg(ctx, &inst),
        Op::IBitNot => lower_int_bitnot(ctx, &inst),
        Op::IDiv => lower_int_div_to_float(ctx, &inst),
        Op::FAdd => lower_float_binop(ctx, &inst, "f64.add"),
        Op::FSub => lower_float_binop(ctx, &inst, "f64.sub"),
        Op::FMul => lower_float_binop(ctx, &inst, "f64.mul"),
        Op::FDiv => lower_float_binop(ctx, &inst, "f64.div"),
        Op::FNeg => lower_float_neg(ctx, &inst),
        Op::ICmp => lower_int_cmp(ctx, &inst),
        Op::FCmp => lower_float_cmp(ctx, &inst),
        Op::IToF => lower_itof(ctx, &inst),
        Op::FToI => lower_ftoi(ctx, &inst),
        Op::IsTruthy => lower_is_truthy(ctx, &inst),
        Op::IsNull => lower_is_null(ctx, &inst),
        Op::Call => lower_call(ctx, &inst),
        Op::LoadGlobal => lower_load_global(ctx, &inst),
        Op::BuiltinCall => lower_builtin_call(ctx, &inst),
        Op::EchoValue | Op::PrintValue => lower_echo(ctx, &inst),
        Op::Acquire => lower_acquire(ctx, &inst),
        Op::Release => lower_release(ctx, &inst),
        Op::Move | Op::Borrow => lower_forward(ctx, &inst),
        Op::ArrayNew => lower_array_new(ctx, &inst),
        Op::ArrayLen => lower_array_len(ctx, &inst),
        Op::ArrayGet => lower_array_get(ctx, &inst),
        Op::ArrayPush => lower_array_push(ctx, &inst),
        Op::ArraySet => lower_array_set(ctx, &inst),
        Op::HashNew => super::inst_hash::lower_hash_new(ctx, &inst),
        Op::HashGet => super::inst_hash::lower_hash_get(ctx, &inst),
        Op::HashSet => super::inst_hash::lower_hash_set(ctx, &inst),
        Op::HashUnset => super::inst_hash::lower_hash_unset(ctx, &inst),
        Op::HashAppend => super::inst_hash::lower_hash_append(ctx, &inst),
        Op::HashUnion => super::inst_hash::lower_hash_union(ctx, &inst),
        Op::ArrayUnion => super::inst_hash::lower_array_union(ctx, &inst),
        Op::ArrayHashUnion => super::inst_hash::lower_array_hash_union(ctx, &inst),
        Op::HashArrayUnion => super::inst_hash::lower_hash_array_union(ctx, &inst),
        Op::MixedBox => lower_mixed_box(ctx, &inst),
        Op::MixedTagOf => lower_mixed_tag_of(ctx, &inst),
        Op::IterStart => lower_iter_start(ctx, &inst),
        Op::IterNext => lower_iter_next(ctx, &inst),
        Op::IterCurrentKey => lower_iter_current_key(ctx, &inst),
        Op::IterCurrentValue => lower_iter_current_value(ctx, &inst),
        Op::IterEnd => Ok(()),
        Op::ObjectNew => super::objects::lower_object_new(ctx, &inst),
        Op::PropGet => super::objects::lower_prop_get(ctx, &inst),
        Op::PropSet => super::objects::lower_prop_set(ctx, &inst),
        Op::MethodCall => super::methods::lower_method_call(ctx, &inst),
        Op::StaticMethodCall => super::methods::lower_static_method_call(ctx, &inst),
        Op::NullsafeMethodCall => super::methods::lower_nullsafe_method_call(ctx, &inst),
        Op::NullsafePropGet => super::objects::lower_nullsafe_prop_get(ctx, &inst),
        Op::InstanceOf => super::classes::lower_instanceof(ctx, &inst),
        Op::InstanceOfDynamic => super::classes::lower_instanceof_dynamic(ctx, &inst),
        Op::ClosureNew => super::closures::lower_closure_new(ctx, &inst),
        Op::ClosureCall => super::closures::lower_closure_call(ctx, &inst),
        Op::ClosureCapture => super::closures::lower_closure_capture(ctx, &inst),
        Op::FirstClassCallableNew => super::closures::lower_first_class_callable_new(ctx, &inst),
        Op::CallableDescriptorInvoke => {
            super::closures::lower_callable_descriptor_invoke(ctx, &inst)
        }
        Op::LoadRefCell => super::refcell::lower_load_ref_cell(ctx, &inst),
        Op::StoreRefCell => super::refcell::lower_store_ref_cell(ctx, &inst),
        Op::PromoteLocalRefCell => super::refcell::lower_promote_local_ref_cell(ctx, &inst),
        Op::AliasLocalRefCell => super::refcell::lower_alias_local_ref_cell(ctx, &inst),
        Op::ReleaseLocalRefCell => super::refcell::lower_release_local_ref_cell(ctx, &inst),
        Op::IterCurrentValueRef => super::refcell::lower_iter_current_value_ref(ctx, &inst),
        other => Err(WasmError::Unsupported(format!("op {:?}", other))),
    }
}

/// Stores the instruction's result into its value local(s), if it produces one.
pub(super) fn store_result(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    if let Some(r) = inst.result {
        ctx.emit_store_value(r)?;
    }
    Ok(())
}

/// Returns the i-th operand of the instruction, or an error if missing.
pub(super) fn operand(inst: &Instruction, i: usize) -> Result<ValueId> {
    inst.operands
        .get(i)
        .copied()
        .ok_or_else(|| WasmError::Unsupported(format!("missing operand {} in {:?}", i, inst.op)))
}

/// Extracts a `CmpPredicate` from the instruction's immediate, or an error.
fn cmp_immediate(inst: &Instruction) -> Result<CmpPredicate> {
    match &inst.immediate {
        Some(Immediate::CmpPredicate(pred)) => Ok(*pred),
        _ => Err(WasmError::Unsupported(format!(
            "missing CmpPredicate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts an i64 from the instruction's immediate, or an error.
fn i64_immediate(inst: &Instruction) -> Result<i64> {
    match &inst.immediate {
        Some(Immediate::I64(n)) => Ok(*n),
        _ => Err(WasmError::Unsupported(format!(
            "missing i64 immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts an f64 from the instruction's immediate, or an error.
fn f64_immediate(inst: &Instruction) -> Result<f64> {
    match &inst.immediate {
        Some(Immediate::F64(f)) => Ok(*f),
        _ => Err(WasmError::Unsupported(format!(
            "missing f64 immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts a bool from the instruction's immediate, or an error.
fn bool_immediate(inst: &Instruction) -> Result<bool> {
    match &inst.immediate {
        Some(Immediate::Bool(b)) => Ok(*b),
        _ => Err(WasmError::Unsupported(format!(
            "missing bool immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts a `LocalSlotId` from the instruction's immediate, or an error.
fn slot_immediate(inst: &Instruction) -> Result<LocalSlotId> {
    match &inst.immediate {
        Some(Immediate::LocalSlot(slot)) => Ok(*slot),
        _ => Err(WasmError::Unsupported(format!(
            "missing LocalSlot immediate in {:?}",
            inst.op
        ))),
    }
}

/// Extracts a `DataId` from the instruction's immediate, or an error.
pub(super) fn data_immediate(inst: &Instruction) -> Result<DataId> {
    match &inst.immediate {
        Some(Immediate::Data(d)) => Ok(*d),
        _ => Err(WasmError::Unsupported(format!(
            "missing Data immediate in {:?}",
            inst.op
        ))),
    }
}

/// Lowers `Op::Call` to a direct WebAssembly call of a user function.
///
/// The callee is named by an `Immediate::Data` index into the module's function-name
/// pool. Non-by-ref arguments are pushed in source order (matching the callee's value
/// parameter locals). By-ref free-function parameters (P7c0b) are materialized
/// backend-side into a 16-byte ref cell whose pointer is passed as the callee's single
/// i32 parameter, then the cell's final value is written back into the caller's local
/// after the call. This mirrors the native `materialize_ref_arg_address` architecture
/// (backend-side temp cell + writeback) and leaves EIR and native untouched.
///
/// By-ref arg materialization (caller side), per operand:
/// - Already-ref-bound operand (`LoadRefCell(slot)`): the caller's local is already
///   cell-backed (from a prior `=&`/foreach), so its existing cell pointer is passed and
///   shared with the callee — no temp cell, no writeback, no free (the caller's owner
///   epilogue releases the cell).
/// - Fresh local (`LoadLocal(slot)`): a temp cell is heap-allocated and the slot's value
///   is retained into it (persist for strings, incref for refcounted containers and
///   callable descriptors, plain store for scalars/tagged), then the cell pointer is
///   passed. After the call the cell's final value is acquired into the slot and the
///   cell is freed. Cells are grouped by source slot so `f(&$x, &$x)` shares one cell
///   (PHP aliasing, including cross-param reads).
/// - Any other operand (literals, property reads, temporaries) is rejected with a clean
///   diagnostic (non-local by-ref deferred).
fn lower_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = data_immediate(inst)?;
    let name = ctx
        .module
        .data
        .function_names
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("call: unknown function data {:?}", data_id)))?;
    let symbol = wasm_fn_symbol(&name);

    // Resolve the callee once and snapshot the by-ref param flags into owned data, so no
    // `&Function` borrow is held across the mutable `ctx` calls below. Reject a by-ref
    // variadic parameter up front (out of scope for P7c0b).
    let callee = ctx.module.functions.iter().find(|f| f.name == name);
    let return_arity = callee
        .map(|f| WasmRepr::val_types(f.return_type).len())
        .unwrap_or(0);
    let by_ref_params: Vec<bool> = callee
        .map(|f| f.params.iter().map(|p| p.by_ref).collect())
        .unwrap_or_default();
    if let Some(f) = callee {
        if f.params.iter().any(|p| p.by_ref && p.variadic) {
            return Err(WasmError::Unsupported(
                "by-ref variadic parameter (P7c0b)".to_string(),
            ));
        }
    }

    // Pre-call pass: push each argument. By-ref params materialize a cell pointer (a temp
    // cell for a fresh local, the shared pointer for an already-ref-bound local); all
    // other args are pushed unchanged.
    let mut temp_cells: Vec<TempCell> = Vec::new();
    let mut slot_to_cell: HashMap<u32, usize> = HashMap::new();
    for (i, &arg) in inst.operands.iter().enumerate() {
        let is_by_ref = i < by_ref_params.len() && by_ref_params[i];
        if is_by_ref {
            push_by_ref_arg(ctx, arg, &mut temp_cells, &mut slot_to_cell)?;
        } else {
            ctx.emit_load_value(arg)?;
        }
    }

    ctx.fb
        .ins(&format!("call ${}", symbol), &format!("call {}", name));

    if let Some(r) = inst.result {
        ctx.emit_store_value(r)?;
    } else {
        for _ in 0..return_arity {
            ctx.fb.ins("drop", "discard unused call result");
        }
    }

    // Post-call pass: write each temp cell's final value back into its source slot and
    // free the cell. Refcount-balanced for both read-only and mutated cases (see
    // `writeback_temp_cell`).
    for cell in &temp_cells {
        writeback_temp_cell(ctx, cell)?;
    }

    Ok(())
}

/// A temp ref cell synthesized for a by-ref argument whose source is a fresh local.
///
/// One cell per unique source slot (grouped, so `f(&$x, &$x)` shares it). The cell holds
/// a retained copy of the slot's pre-call value; after the call `writeback_temp_cell`
/// acquires the cell's final value into the slot and releases the cell.
struct TempCell {
    /// The source slot raw id (the caller's local that the cell mirrors).
    slot_raw: u32,
    /// The i32 local holding the 16-byte cell pointer.
    ptr_local: String,
}

/// The source of a by-ref argument, resolved by introspecting the operand's defining
/// instruction.
pub(super) enum ByRefSource {
    /// The operand is `LoadRefCell(slot)`: the caller's local is already ref-bound (from
    /// a prior `=&`/foreach), so the existing cell pointer is shared with the callee.
    AlreadyRefBound(u32),
    /// The operand is `LoadLocal(slot)`: a fresh local to mirror into a temp cell.
    FreshLocal(LocalSlotId),
    /// Anything else (literals, property reads, temporaries, block params): non-local
    /// by-ref, currently rejected with a clean diagnostic.
    NonLocal,
}

/// Introspects a by-ref operand's defining instruction to classify its source.
///
/// EIR routes a ref-bound slot read through `LoadRefCell`; a plain local read is
/// `LoadLocal`. Any other defining instruction (or a block-parameter definition) means
/// the argument is not a local, so by-ref is unsupported (clean diagnostic). This keeps
/// the ABI agreement — only a local's storage can be safely mirrored into / shared as a
/// cell — enforced at the lowering edge.
pub(super) fn resolve_by_ref_source(ctx: &FnCtx, arg: ValueId) -> Result<ByRefSource> {
    let val = ctx
        .function
        .value(arg)
        .ok_or_else(|| WasmError::Unsupported(format!("by-ref arg {:?} has no value", arg)))?;
    let inst_id = match val.def {
        ValueDef::Instruction { inst, .. } => inst,
        _ => return Ok(ByRefSource::NonLocal),
    };
    let def = ctx
        .function
        .instruction(inst_id)
        .ok_or_else(|| WasmError::Unsupported(format!("by-ref arg {:?} def missing", arg)))?;
    Ok(match (def.op, &def.immediate) {
        (Op::LoadRefCell, Some(Immediate::LocalSlot(slot))) => {
            ByRefSource::AlreadyRefBound(slot.as_raw())
        }
        (Op::LoadLocal, Some(Immediate::LocalSlot(slot))) => ByRefSource::FreshLocal(*slot),
        _ => ByRefSource::NonLocal,
    })
}

/// Returns the `codegen_repr` payload `PhpType` of a local slot.
///
/// Drives the retain kind (Callable special-case) and the cell's payload release in
/// `emit_ref_cell_release_seq` (`needs_payload_release`).
pub(super) fn slot_payload_type(ctx: &FnCtx, slot: LocalSlotId) -> Result<PhpType> {
    let local = ctx
        .function
        .locals
        .get(slot.as_raw() as usize)
        .ok_or_else(|| WasmError::Unsupported(format!("slot {:?} has no local metadata", slot)))?;
    Ok(local.php_type.codegen_repr())
}

/// Pushes one by-ref argument's cell pointer onto the WASM operand stack.
///
/// For an already-ref-bound local the existing cell pointer is reused (no temp cell, no
/// writeback). For a fresh local a temp cell is synthesized and recorded, grouped by
/// source slot so repeated occurrences of the same slot share one cell (PHP aliasing,
/// including cross-param reads). Non-local operands are rejected.
fn push_by_ref_arg(
    ctx: &mut FnCtx,
    arg: ValueId,
    temp_cells: &mut Vec<TempCell>,
    slot_to_cell: &mut HashMap<u32, usize>,
) -> Result<()> {
    match resolve_by_ref_source(ctx, arg)? {
        ByRefSource::AlreadyRefBound(slot_raw) => {
            let ptr = ctx.ref_cell_ptr(slot_raw)?.to_string();
            ctx.fb.ins(
                &format!("local.get {}", ptr),
                "by-ref arg: existing ref-cell pointer",
            );
        }
        ByRefSource::FreshLocal(slot) => {
            let slot_raw = slot.as_raw();
            if let Some(&idx) = slot_to_cell.get(&slot_raw) {
                ctx.fb.ins(
                    &format!("local.get {}", temp_cells[idx].ptr_local),
                    "by-ref arg: shared temp cell (slot grouping)",
                );
            } else {
                let cell = synthesize_temp_cell(ctx, slot)?;
                ctx.fb.ins(
                    &format!("local.get {}", cell.ptr_local),
                    "by-ref arg: temp cell pointer",
                );
                slot_to_cell.insert(slot_raw, temp_cells.len());
                temp_cells.push(cell);
            }
        }
        ByRefSource::NonLocal => {
            return Err(WasmError::Unsupported(
                "by-ref arg is not a local (P7c0b non-local by-ref deferred)".to_string(),
            ));
        }
    }
    Ok(())
}

/// Synthesizes a temp ref cell mirroring a fresh local's current value.
///
/// Allocates a 16-byte cell and retains the slot's value into it (persist for strings,
/// incref for refcounted containers and callable descriptors, plain store for scalars
/// and tagged values), reusing the promote path's `retain_and_store_slot_value`. The
/// slot's own locals are left untouched (read-only): the writeback later releases the
/// slot's old value, so the slot and the cell each hold an independent reference.
fn synthesize_temp_cell(ctx: &mut FnCtx, slot: LocalSlotId) -> Result<TempCell> {
    let slot_repr = ctx.slot_repr(slot)?.clone();
    let payload = slot_payload_type(ctx, slot)?;
    let ptr_local = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins("i32.const 16", "temp ref cell size (16 bytes)");
    ctx.fb.ins("call $__rt_heap_alloc", "allocate the temp ref cell");
    ctx.fb
        .ins(&format!("local.set {}", ptr_local), "temp cell pointer");
    super::refcell::retain_and_store_slot_value(ctx, &ptr_local, &slot_repr, &payload)?;
    Ok(TempCell {
        slot_raw: slot.as_raw(),
        ptr_local,
    })
}

/// Writes a temp cell's final value back into its source slot and frees the cell.
///
/// Per-slot sequence (refcount-balanced for both read-only and mutated cases):
/// 1. Load the cell's final value (the callee may have mutated it) and retain an owned
///    copy — persist for strings, incref for refcounted containers and callable
///    descriptors, no-op for scalars/tagged — so the slot owns a fresh reference.
/// 2. Release the slot's old (pre-call) value (`release_old_slot_value`), which reads the
///    slot's still-untouched locals.
/// 3. Store the retained value into the slot's locals.
/// 4. Release the cell (`emit_ref_cell_release_seq`: decref the cell's payload by kind,
///    free the 16-byte block).
///
/// Refcount trace (refcounted container, in-place mutation): synth increfs V (R+1,
/// cell+S); writeback increfs V (R+2), releases S old (R+1), stores (S owns 1), releases
/// cell (R). Net: S owns V, refcount R restored. Replacement case: the callee's
/// `store_local` releases the cell's old V and moves its new value into the cell;
/// writeback increfs the new value, releases S's old V (freed at zero), stores the new
/// value, releases the cell (S owns it). Strings use copy-on-acquire (persist to own,
/// `__rt_heap_free_safe` to release) since the runtime frees kind-1 blocks
/// unconditionally, so the slot must own its own copy rather than share the cell's
/// pointer. Scalars carry no refcount; steps 1/2 are no-ops and only the bits move.
fn writeback_temp_cell(ctx: &mut FnCtx, cell: &TempCell) -> Result<()> {
    let ptr_local = cell.ptr_local.clone();
    let slot = LocalSlotId::from_raw(cell.slot_raw);
    let payload = slot_payload_type(ctx, slot)?;
    let slot_repr = ctx.slot_repr(slot)?.clone();

    // Steps 1-3: load + retain + release-old + store, per representation. The release-old
    // runs before the store so the retained copy is not freed as the "old" value.
    match &slot_repr {
        WasmRepr::I64(slot_local) => {
            let tmp = ctx.fresh_temp(ValType::I64);
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=0", "load final value @ cell+0");
            ctx.fb.ins(&format!("local.set {}", tmp), "capture final value");
            if payload == PhpType::Callable {
                ctx.fb.ins(&format!("local.get {}", tmp), "descriptor to retain");
                ctx.fb.ins("i32.wrap_i64", "narrow the descriptor pointer to i32");
                ctx.fb.ins("call $__rt_incref", "retain the descriptor for the slot");
            }
            super::refcell::release_old_slot_value(ctx, &slot_repr, &payload)?;
            ctx.fb.ins(&format!("local.get {}", tmp), "retained value");
            ctx.fb.ins(&format!("local.set {}", slot_local), "store into the slot");
        }
        WasmRepr::F64(slot_local) => {
            let tmp = ctx.fresh_temp(ValType::F64);
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("f64.load offset=0", "load final float @ cell+0");
            ctx.fb.ins(&format!("local.set {}", tmp), "capture final float");
            super::refcell::release_old_slot_value(ctx, &slot_repr, &payload)?;
            ctx.fb.ins(&format!("local.get {}", tmp), "retained float");
            ctx.fb.ins(&format!("local.set {}", slot_local), "store into the slot");
        }
        WasmRepr::Ptr(slot_local) => {
            let tmp = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i32.load offset=0", "load final pointer @ cell+0");
            ctx.fb.ins(&format!("local.set {}", tmp), "capture final pointer");
            ctx.fb.ins(&format!("local.get {}", tmp), "container to retain");
            ctx.fb.ins("call $__rt_incref", "retain the container for the slot");
            super::refcell::release_old_slot_value(ctx, &slot_repr, &payload)?;
            ctx.fb.ins(&format!("local.get {}", tmp), "retained pointer");
            ctx.fb.ins(&format!("local.set {}", slot_local), "store into the slot");
        }
        WasmRepr::Str { ptr, len } => {
            let tmp_ptr = ctx.fresh_temp(ValType::I32);
            let tmp_len = ctx.fresh_temp(ValType::I64);
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i32.load offset=0", "load final string ptr @ cell+0");
            ctx.fb.ins(&format!("local.set {}", tmp_ptr), "capture final string ptr");
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=8", "load final length @ cell+8");
            ctx.fb.ins(&format!("local.set {}", tmp_len), "capture final length");
            // Retain via persist: an owned heap copy safe for the slot (strings use
            // copy-on-acquire; the runtime frees kind-1 blocks unconditionally, so the
            // slot must own its own copy rather than share the cell's pointer).
            ctx.fb.ins(&format!("local.get {}", tmp_ptr), "source string pointer");
            ctx.fb.ins(&format!("local.get {}", tmp_len), "source string length");
            ctx.fb.ins("call $__rt_str_persist", "persist an owned copy for the slot");
            let new_len = ctx.fresh_temp(ValType::I64);
            let new_ptr = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", new_len), "owned string length");
            ctx.fb.ins(&format!("local.set {}", new_ptr), "owned string pointer");
            super::refcell::release_old_slot_value(ctx, &slot_repr, &payload)?;
            ctx.fb.ins(&format!("local.get {}", new_ptr), "owned string pointer");
            ctx.fb.ins(&format!("local.set {}", ptr), "store ptr into the slot");
            ctx.fb.ins(&format!("local.get {}", new_len), "owned string length");
            ctx.fb.ins(&format!("local.set {}", len), "store len into the slot");
        }
        WasmRepr::Tagged {
            payload: pay_local,
            tag: tag_local,
        } => {
            let tmp_pay = ctx.fresh_temp(ValType::I64);
            let tmp_tag = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=0", "load final payload @ cell+0");
            ctx.fb.ins(&format!("local.set {}", tmp_pay), "capture final payload");
            ctx.fb.ins(&format!("local.get {}", ptr_local), "cell address");
            ctx.fb.ins("i64.load offset=8", "load final tag @ cell+8");
            ctx.fb.ins("i32.wrap_i64", "narrow the tag to i32");
            ctx.fb.ins(&format!("local.set {}", tmp_tag), "capture final tag");
            super::refcell::release_old_slot_value(ctx, &slot_repr, &payload)?;
            ctx.fb.ins(&format!("local.get {}", tmp_pay), "retained payload");
            ctx.fb.ins(&format!("local.set {}", pay_local), "store payload into the slot");
            ctx.fb.ins(&format!("local.get {}", tmp_tag), "retained tag");
            ctx.fb.ins(&format!("local.set {}", tag_local), "store tag into the slot");
        }
        WasmRepr::Void => {
            return Err(WasmError::Unsupported("by-ref void slot".to_string()));
        }
    }

    // Step 4: release the cell (payload by kind + free the 16-byte block).
    super::refcell::emit_ref_cell_release_seq(ctx, &ptr_local, &payload)?;
    Ok(())
}

/// Lowers an integer binary op: load both operands, emit the wasm op, store result.
fn lower_int_binop(ctx: &mut FnCtx, inst: &Instruction, wasm_op: &str) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "integer binary op");
    store_result(ctx, inst)
}

/// Lowers a float binary op: load both operands, emit the wasm op, store result.
fn lower_float_binop(ctx: &mut FnCtx, inst: &Instruction, wasm_op: &str) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "float binary op");
    store_result(ctx, inst)
}

/// Lowers `ConstI64`: pushes the immediate integer constant.
fn lower_const_i64(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let n = i64_immediate(inst)?;
    ctx.fb.ins(&format!("i64.const {}", n), "int literal");
    store_result(ctx, inst)
}

/// Lowers `ConstF64` bit-exactly: push the f64's raw bits and reinterpret them as f64.
fn lower_const_f64(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let bits = f64_immediate(inst)?.to_bits() as i64;
    ctx.fb.ins(&format!("i64.const {}", bits), "f64 literal bits");
    ctx.fb.ins("f64.reinterpret_i64", "reinterpret bits as f64");
    store_result(ctx, inst)
}

/// Lowers `ConstBool`: pushes 1 for true, 0 for false (PHP bool is an i64).
fn lower_const_bool(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let val = if bool_immediate(inst)? { 1 } else { 0 };
    ctx.fb.ins(&format!("i64.const {}", val), "bool literal");
    store_result(ctx, inst)
}

/// Lowers `ConstNull`: pushes the i64 null sentinel (0x7fff_ffff_ffff_fffe).
fn lower_const_null(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.fb.ins(
        "i64.const 9223372036854775806",
        "null sentinel (0x7fff_ffff_ffff_fffe)",
    );
    store_result(ctx, inst)
}

/// Lowers `ConstStr`: pushes the literal's linear-memory pointer (i32) and byte
/// length (i64) from the module's string-literal layout.
fn lower_const_str(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let (offset, len) = ctx.str_literal(data_immediate(inst)?)?;
    ctx.fb
        .ins(&format!("i32.const {}", offset), "string literal ptr");
    ctx.fb.ins(&format!("i64.const {}", len), "string literal len");
    store_result(ctx, inst)
}

/// Lowers `StrLen`: reads the length component of a string value.
fn lower_strlen(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let repr = ctx.value_repr(op0)?.clone();
    match repr {
        WasmRepr::Str { len, .. } => {
            ctx.fb.ins(&format!("local.get {}", len), "string length");
        }
        other => return Err(WasmError::Unsupported(format!("strlen of {:?}", other))),
    }
    store_result(ctx, inst)
}

/// Lowers `Nop`: emits a comment; the result local (if any) keeps its default 0.
fn lower_nop(ctx: &mut FnCtx) -> Result<()> {
    ctx.fb.comment("nop");
    Ok(())
}

/// Lowers `ConcatReset`: restores the global concat cursor to this frame's
/// baseline, freeing string temporaries built during the statement.
fn lower_concat_reset(ctx: &mut FnCtx) -> Result<()> {
    ctx.fb
        .ins(&format!("local.get {}", ctx.concat_base_local), "frame concat baseline");
    ctx.fb
        .ins("global.set $__concat_off", "reset concat cursor to baseline");
    Ok(())
}

/// Lowers `StrConcat`: appends two strings into the concat buffer via `__rt_concat`.
///
/// Pushes (a_ptr, a_len, b_ptr, b_len) — matching `__rt_concat`'s parameter order —
/// and stores the returned `(ptr, len)` into the result string value.
fn lower_str_concat(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins("call $__rt_concat", "concatenate two strings");
    store_result(ctx, inst)
}

/// Lowers `LoadLocal`: copies the slot's value into the result value's local(s).
///
/// If the slot stores a ref-cell pointer (a by-ref free-function param per P7c0b, or a
/// caller local promoted by a P7c by-ref closure capture), the value lives in the cell,
/// not the slot's own locals — so the load dereferences the cell. This retroactive
/// routing is what lets the EIR emit a plain `Op::LoadLocal` (it does not mark a
/// by-ref-captured caller local ref-bound) while still reading through the shared cell,
/// mirroring the active native backend's `local_stores_ref_cell_pointer` check in
/// `load_local_to_result`. A preceding `Op::Release(LoadLocal($x))` (emitted before a
/// store_local overwrite) therefore releases the cell's current value, not the slot's
/// stale locals.
fn lower_load_local(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let slot = slot_immediate(inst)?;
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("load_local without result".to_string()))?;
    if let Ok(ptr_local) = ctx.ref_cell_ptr(slot.as_raw()) {
        let ptr_local = ptr_local.to_string();
        let result_repr = ctx.value_repr(result)?.clone();
        super::refcell::emit_cell_load(ctx, &ptr_local, &result_repr)?;
        return ctx.emit_store_value(result);
    }
    let slot_refs = ctx.slot_repr(slot)?.local_refs();
    let result_refs = ctx.value_repr(result)?.local_refs();
    if slot_refs.len() != result_refs.len() {
        return Err(WasmError::Unsupported(format!(
            "load_local repr mismatch: slot has {} local(s), result has {}",
            slot_refs.len(),
            result_refs.len()
        )));
    }
    for r in &slot_refs {
        ctx.fb.ins(&format!("local.get {}", r), "load local slot");
    }
    ctx.emit_store_value(result)
}

/// Lowers `StoreLocal`: stores the operand value into the slot.
///
/// If the slot stores a ref-cell pointer (by-ref param or P7c-promoted caller local),
/// the store writes through the cell. It does NOT release the cell's previous payload:
/// the EIR emits the prior-value release (`Op::Release(LoadLocal($x))`) before this op,
/// and (after the LoadLocal routing above) that release decrefs the cell's current
/// value. Releasing here too would double-free. This matches the contract documented on
/// `lower_store_ref_cell` and native `store_value_to_ref_cell_as`.
fn lower_store_local(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let slot = slot_immediate(inst)?;
    let value = operand(inst, 0)?;
    if let Ok(ptr_local) = ctx.ref_cell_ptr(slot.as_raw()) {
        let ptr_local = ptr_local.to_string();
        let value_repr = ctx.value_repr(value)?.clone();
        super::refcell::emit_cell_store(ctx, &ptr_local, &value_repr)?;
        return Ok(());
    }
    let slot_refs = ctx.slot_repr(slot)?.local_refs();
    let value_refs = ctx.value_repr(value)?.local_refs();
    if slot_refs.len() != value_refs.len() {
        return Err(WasmError::Unsupported(format!(
            "store_local repr mismatch: slot has {} local(s), value has {}",
            slot_refs.len(),
            value_refs.len()
        )));
    }
    ctx.emit_load_value(value)?;
    // Pop in reverse so the first slot local takes the bottom-most stack value.
    for r in slot_refs.iter().rev() {
        ctx.fb.ins(&format!("local.set {}", r), "store local slot");
    }
    Ok(())
}

/// Lowers `INeg`: computes `0 - x`.
fn lower_int_neg(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.fb.ins("i64.const 0", "0 for negation");
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("i64.sub", "0 - x");
    store_result(ctx, inst)
}

/// Lowers `IBitNot`: computes `x ^ -1` (one's complement).
fn lower_int_bitnot(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("i64.const -1", "all-ones mask");
    ctx.fb.ins("i64.xor", "bitwise not");
    store_result(ctx, inst)
}

/// Lowers `IDiv` (PHP `/`): widens both i64 operands to f64 and divides.
fn lower_int_div_to_float(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("f64.convert_i64_s", "lhs to float");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins("f64.convert_i64_s", "rhs to float");
    ctx.fb.ins("f64.div", "php / is float division");
    store_result(ctx, inst)
}

/// Lowers `FNeg`: negates a float.
fn lower_float_neg(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("f64.neg", "negate float");
    store_result(ctx, inst)
}

/// Maps an integer comparison predicate to its signed wasm comparison op.
fn int_cmp_op(pred: CmpPredicate) -> Result<&'static str> {
    Ok(match pred {
        CmpPredicate::Eq => "i64.eq",
        CmpPredicate::Ne => "i64.ne",
        CmpPredicate::Slt => "i64.lt_s",
        CmpPredicate::Sle => "i64.le_s",
        CmpPredicate::Sgt => "i64.gt_s",
        CmpPredicate::Sge => "i64.ge_s",
        other => {
            return Err(WasmError::Unsupported(format!(
                "integer compare predicate {:?}",
                other
            )))
        }
    })
}

/// Maps a float comparison predicate to its (ordered) wasm comparison op.
fn float_cmp_op(pred: CmpPredicate) -> &'static str {
    match pred {
        CmpPredicate::Eq => "f64.eq",
        CmpPredicate::Ne => "f64.ne",
        CmpPredicate::Slt | CmpPredicate::Olt => "f64.lt",
        CmpPredicate::Sle | CmpPredicate::Ole => "f64.le",
        CmpPredicate::Sgt | CmpPredicate::Ogt => "f64.gt",
        CmpPredicate::Sge | CmpPredicate::Oge => "f64.ge",
    }
}

/// Lowers `ICmp`: signed integer comparison yielding an i64 boolean (0/1).
fn lower_int_cmp(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let wasm_op = int_cmp_op(cmp_immediate(inst)?)?;
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "integer comparison");
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    store_result(ctx, inst)
}

/// Lowers `FCmp`: ordered float comparison yielding an i64 boolean (0/1).
fn lower_float_cmp(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let wasm_op = float_cmp_op(cmp_immediate(inst)?);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(wasm_op, "float comparison");
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    store_result(ctx, inst)
}

/// Lowers `IToF`: signed integer to float.
fn lower_itof(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("f64.convert_i64_s", "int to float");
    store_result(ctx, inst)
}

/// Lowers `FToI`: float to signed integer (truncate toward zero; NaN -> 0).
fn lower_ftoi(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb
        .ins("i64.trunc_sat_f64_s", "float to int (truncate, NaN->0)");
    store_result(ctx, inst)
}

/// Lowers `IsTruthy` for i64 (int/bool) and f64 operands; other reprs are unsupported.
fn lower_is_truthy(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let repr = ctx.value_repr(op0)?.clone();
    match repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("i64.const 0", "zero");
            ctx.fb.ins("i64.ne", "truthy = x != 0");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        }
        WasmRepr::F64(_) => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("f64.const 0.0", "zero");
            ctx.fb.ins("f64.ne", "truthy = x != 0.0");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        }
        other => {
            return Err(WasmError::Unsupported(format!("is_truthy of {:?}", other)));
        }
    }
    store_result(ctx, inst)
}

/// Lowers `Op::LoadGlobal` for supported superglobals.
///
/// `$argc` is read via `__rt_argc` (WASI `args_sizes_get`); `$argv` is built as an
/// indexed string array via `__rt_argv` (WASI `args_get`). Other globals are not
/// yet supported.
fn lower_load_global(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = match &inst.immediate {
        Some(Immediate::GlobalName(d)) => *d,
        _ => return Err(WasmError::Unsupported("load_global without a name".to_string())),
    };
    let name = ctx
        .module
        .data
        .global_names
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("load_global: unknown name {:?}", data_id)))?;
    match name.as_str() {
        "argc" => {
            ctx.fb.ins("call $__rt_argc", "load $argc");
            store_result(ctx, inst)
        }
        "argv" => {
            ctx.fb
                .ins("call $__rt_argv", "build $argv (indexed string array)");
            store_result(ctx, inst)
        }
        other => Err(WasmError::Unsupported(format!("global ${}", other))),
    }
}

/// Lowers `Op::BuiltinCall` by dispatching on the builtin's name.
///
/// Only `exit`/`die` and `get_class` are handled so far; other builtins return
/// `Unsupported`.
fn lower_builtin_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = data_immediate(inst)?;
    let name = ctx
        .module
        .data
        .function_names
        .get(data_id.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported(format!("builtin: unknown name data {:?}", data_id)))?;
    match name.as_str() {
        "exit" | "die" => lower_exit(ctx, inst),
        "get_class" => super::classes::lower_get_class(ctx, inst),
        "array_map" => lower_array_map(ctx, inst),
        "array_filter" => lower_array_filter(ctx, inst),
        other => Err(WasmError::Unsupported(format!("builtin {}", other))),
    }
}

/// Lowers `array_map($f, $arr)` where operand 0 `$f` is a `Callable` descriptor
/// (a closure or a free-function first-class callable) and operand 1 `$arr` is an
/// INDEXED `array<int|str|mixed>`, into a `__rt_array_map_callable` runtime call
/// returning a fresh `array<mixed>` of the mapped results. The WASM analogue of the
/// native `lower_array_map_descriptor_callback`.
///
/// Both operands are materialized BORROWED: neither is released here. The EIR owns
/// operands 0 and 1 and releases them at the call site (the source array is borrowed
/// by `array_map`, the descriptor is released after). The runtime returns an Owned
/// array pointer, stored via `store_result`.
///
/// Deferred (returns `Unsupported`, never miscompiled — mirroring the deferral
/// pattern in `closures::lower_callable_descriptor_invoke`): a string/array/object
/// callback (operand 0 not `Callable`); a hash/assoc or otherwise non-indexed source
/// (operand 1 not `Heap(Array)`); and the multi-array zip / 3-arg `array_map(null, …)`
/// shapes (operand count != 2), which need a different runtime contract.
fn lower_array_map(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    // Single callback + single source only. A 3+-operand array_map (multi-array zip
    // or array_map(null, ...)) needs a different runtime contract and is deferred.
    if inst.operands.len() != 2 {
        return Err(WasmError::Unsupported(format!(
            "array_map with {} operands on wasm32-wasi (only single callback + single \
             indexed array supported; multi-array/null-callback deferred)",
            inst.operands.len()
        )));
    }
    let callable = operand(inst, 0)?;
    let array = operand(inst, 1)?;

    // GUARD: operand 0 must be a Callable descriptor (closure or free-fn FCC). A
    // string/array/object callback would not be an i64 descriptor and needs its own
    // runtime callback selection (deferred slice).
    let callable_php = ctx.value_php_type(callable)?.codegen_repr();
    if !matches!(callable_php, PhpType::Callable) {
        return Err(WasmError::Unsupported(format!(
            "array_map with a {:?} callback on wasm32-wasi (only Callable descriptors \
             supported; string/array/object callbacks deferred)",
            callable_php
        )));
    }
    // GUARD: operand 1 must be an INDEXED array (value_type 0/1/7 = int/string/mixed-cell).
    // A HashNew assoc source has a different layout the runtime helper cannot read.
    let array_ir = ctx.function.value(array).map(|v| v.ir_type);
    if !matches!(array_ir, Some(IrType::Heap(IrHeapKind::Array))) {
        return Err(WasmError::Unsupported(format!(
            "array_map over a {:?} source on wasm32-wasi (only indexed array<int|str|mixed> \
             supported; hash/assoc sources deferred)",
            array_ir
        )));
    }

    // operand 0: callable descriptor (i64) -> i32 for __rt_array_map_callable.
    let desc = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(callable)?;
    ctx.fb.ins("i32.wrap_i64", "callable descriptor i64 -> i32");
    ctx.fb.ins(&format!("local.set {}", desc), "save descriptor pointer");

    // operand 1: the indexed source array (a single i32 pointer).
    let src = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(array)?;
    ctx.fb.ins(&format!("local.set {}", src), "save source array pointer");

    // __rt_array_map_callable(desc, src) -> i32 result array pointer (Owned). Neither
    // operand is released here: the EIR owns/releases operands 0 and 1 at the call site.
    ctx.fb.ins(
        &format!(
            "(call $__rt_array_map_callable (local.get {}) (local.get {}))",
            desc, src
        ),
        "map each element through the callback into a fresh array<mixed>",
    );
    store_result(ctx, inst)
}

/// Lowers `array_filter($arr, $f)` where — note the REVERSED operand order vs
/// `array_map` — operand 0 `$arr` is the INDEXED `array<int|str|mixed>` source and
/// operand 1 `$f` is a `Callable` descriptor (a closure or a free-function first-class
/// callable). It lowers to a `__rt_array_filter_callable(desc, src)` runtime call that
/// returns a fresh array of the kept elements. The result is RE-INDEXED to keys
/// `0..kept-1` (it does NOT preserve PHP keys), exactly mirroring the native
/// `__rt_array_filter` divergence — key preservation is deliberately out of scope here.
///
/// Both operands are materialized BORROWED: neither is released here. The EIR owns
/// operands 0 and 1 and releases them at the call site (the source array is borrowed
/// by `array_filter`, the descriptor is released after). The runtime returns an Owned
/// array pointer, stored via `store_result`.
///
/// Deferred (returns `Unsupported`, never miscompiled): the 3-operand
/// `array_filter($arr, $cb, $mode)` and 1-operand falsy-filter `array_filter($arr)`
/// shapes (operand count != 2); a string/array/object callback (operand 1 not
/// `Callable`); and a hash/assoc or otherwise non-indexed source (operand 0 not
/// `Heap(Array)`).
fn lower_array_filter(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    // Single indexed array + single Callable callback only. The 3-arg `mode` form
    // (ARRAY_FILTER_USE_KEY/BOTH) and the 1-arg no-callback falsy filter each need a
    // different runtime contract and are deferred.
    if inst.operands.len() != 2 {
        return Err(WasmError::Unsupported(format!(
            "array_filter with {} operands on wasm32-wasi (only a single indexed array + \
             single Callable callback supported; 3-arg mode / no-callback deferred)",
            inst.operands.len()
        )));
    }
    // REVERSED vs array_map: operand 0 is the ARRAY, operand 1 is the CALLBACK.
    let array = operand(inst, 0)?;
    let callable = operand(inst, 1)?;

    // GUARD: operand 1 must be a Callable descriptor (closure or free-fn FCC). A
    // string/array/object callback would not be an i64 descriptor and needs its own
    // runtime callback selection (deferred slice).
    let callable_php = ctx.value_php_type(callable)?.codegen_repr();
    if !matches!(callable_php, PhpType::Callable) {
        return Err(WasmError::Unsupported(format!(
            "array_filter with a {:?} callback on wasm32-wasi (only Callable descriptors \
             supported; string/array/object callbacks deferred)",
            callable_php
        )));
    }
    // GUARD: operand 0 must be an INDEXED array (value_type 0/1/7 = int/string/mixed-cell).
    // A HashNew assoc source has a different layout the runtime helper cannot read.
    let array_ir = ctx.function.value(array).map(|v| v.ir_type);
    if !matches!(array_ir, Some(IrType::Heap(IrHeapKind::Array))) {
        return Err(WasmError::Unsupported(format!(
            "array_filter over a {:?} source on wasm32-wasi (only indexed array<int|str|mixed> \
             supported; hash/assoc sources deferred)",
            array_ir
        )));
    }

    // operand 1: callable descriptor (i64) -> i32 for __rt_array_filter_callable.
    let desc = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(callable)?;
    ctx.fb.ins("i32.wrap_i64", "callable descriptor i64 -> i32");
    ctx.fb.ins(&format!("local.set {}", desc), "save descriptor pointer");

    // operand 0: the indexed source array (a single i32 pointer).
    let src = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(array)?;
    ctx.fb.ins(&format!("local.set {}", src), "save source array pointer");

    // __rt_array_filter_callable(desc, src) -> i32 result array pointer (Owned). Neither
    // operand is released here: the EIR owns/releases operands 0 and 1 at the call site.
    ctx.fb.ins(
        &format!(
            "(call $__rt_array_filter_callable (local.get {}) (local.get {}))",
            desc, src
        ),
        "keep each element whose callback result is truthy, re-indexed into a fresh array",
    );
    store_result(ctx, inst)
}

/// Lowers `exit`/`die`: an integer argument becomes the WASI exit status; any
/// other argument (a message string) or no argument exits with status 0. Matching
/// the native backend, a string message is NOT printed.
fn lower_exit(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let int_code = inst.operands.first().is_some_and(|arg| {
        ctx.function
            .value(*arg)
            .map(|v| v.php_type.codegen_repr() == PhpType::Int)
            .unwrap_or(false)
    });
    if int_code {
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("i32.wrap_i64", "exit code to i32");
    } else {
        ctx.fb.ins("i32.const 0", "exit status 0");
    }
    ctx.fb.ins("call $wasi_proc_exit", "WASI proc_exit(code)");
    Ok(())
}

/// Lowers `EchoValue`/`PrintValue` by dispatching on the operand's PHP type.
///
/// Integers and booleans share the i64 representation, so the PHP type is used to
/// pick the right runtime helper (booleans print "1"/"" rather than "0"/"1").
/// Floats render as `%.14G` text via `__rt_echo_f64`; mixed values defer to the
/// tag-dispatching `__rt_mixed_write_stdout`. Array and object output still need
/// more runtime support and are not handled yet.
fn lower_echo(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let php = ctx
        .function
        .value(op0)
        .map(|v| v.php_type.codegen_repr())
        .ok_or_else(|| WasmError::Unsupported(format!("echo: unknown operand {:?}", op0)))?;
    match php {
        PhpType::Bool => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_bool", "echo boolean to stdout");
            Ok(())
        }
        PhpType::Int => {
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_i64", "echo integer to stdout");
            Ok(())
        }
        PhpType::Float => {
            // Pushes the f64 value; __rt_echo_f64 reinterprets it to bits for __rt_ftoa.
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_f64", "echo float to stdout");
            Ok(())
        }
        PhpType::Str => {
            // Pushes ptr (i32) then len (i64), matching __rt_echo_str's params.
            ctx.emit_load_value(op0)?;
            ctx.fb.ins("call $__rt_echo_str", "echo string to stdout");
            Ok(())
        }
        PhpType::Mixed => {
            // The Mixed pointer; the runtime dispatches on the cell's tag.
            ctx.emit_load_value(op0)?;
            ctx.fb
                .ins("call $__rt_mixed_write_stdout", "echo mixed value (tag-dispatched)");
            Ok(())
        }
        other => Err(WasmError::Unsupported(format!("echo of {:?}", other))),
    }
}

/// Lowers `Op::Acquire`: makes the operand value safe to store as a new owner.
///
/// A PHP string is copied into an owned heap block (`__rt_str_persist`), matching
/// PHP string value semantics; a heap pointer is increfed (`__rt_incref`); scalars
/// forward unchanged. The result value receives the acquired value. A `Mixed`
/// (tagged) value is not handled yet (its ownership lands with the boxing phase).
///
/// A callable is a heap descriptor carried as `WasmRepr::I64` (a zero-extended i32
/// pointer), so the generic `I64` arm below would forward it without incref'ing the
/// descriptor and leak it. Callables are therefore routed explicitly to `__rt_incref`
/// on the wrapped i32 pointer before forwarding (P7a0).
fn lower_acquire(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    if ctx.value_php_type(value)? == PhpType::Callable {
        ctx.emit_load_value(value)?;
        ctx.fb
            .ins("i32.wrap_i64", "narrow the callable descriptor pointer to i32");
        ctx.fb.ins("call $__rt_incref", "incref the callable descriptor");
        return forward_value(ctx, value, inst);
    }
    let repr = ctx.value_repr(value)?.clone();
    match repr {
        WasmRepr::Str { .. } => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("call $__rt_str_persist", "persist string to an owned heap copy");
            store_result(ctx, inst)
        }
        WasmRepr::Ptr(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb.ins("call $__rt_incref", "incref the owned heap value");
            forward_value(ctx, value, inst)
        }
        WasmRepr::I64(_) | WasmRepr::F64(_) | WasmRepr::Void => forward_value(ctx, value, inst),
        WasmRepr::Tagged { .. } => {
            Err(WasmError::Unsupported("acquire of a Mixed value".to_string()))
        }
    }
}

/// Lowers `Op::Release`: releases storage the value may own.
///
/// No-op for ownership states that cannot own heap storage (non-heap, borrowed,
/// persistent, moved). A string is freed through the bounds/refcount-guarded
/// `__rt_heap_free_safe` (so transient concat/literal pointers are skipped there);
/// a heap pointer is released through the `__rt_decref_any` kind dispatcher. A
/// `Mixed` (tagged) value is not handled yet.
fn lower_release(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let ownership = ctx
        .function
        .value(value)
        .map(|v| v.ownership)
        .unwrap_or(Ownership::NonHeap);
    if matches!(
        ownership,
        Ownership::NonHeap | Ownership::Borrowed | Ownership::Persistent | Ownership::Moved
    ) {
        return Ok(());
    }
    // A callable is a heap descriptor carried as `WasmRepr::I64`, so the generic
    // `I64` arm below is a no-op and an owned callable would leak. Route callables
    // to `__rt_decref_any` on the wrapped i32 pointer; the kind dispatcher resolves
    // heap-header kind 6 to `__rt_callable_descriptor_release` (P7a0).
    if ctx.value_php_type(value)? == PhpType::Callable {
        ctx.emit_load_value(value)?;
        ctx.fb
            .ins("i32.wrap_i64", "narrow the callable descriptor pointer to i32");
        ctx.fb
            .ins("call $__rt_decref_any", "release the callable descriptor (kind 6)");
        return Ok(());
    }
    let repr = ctx.value_repr(value)?.clone();
    match repr {
        WasmRepr::Str { ptr, .. } => {
            ctx.fb
                .ins(&format!("local.get {}", ptr), "string pointer to free");
            ctx.fb
                .ins("call $__rt_heap_free_safe", "free the owned string (skips non-heap)");
            Ok(())
        }
        WasmRepr::Ptr(_) => {
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("call $__rt_decref_any", "release the owned heap value by kind");
            Ok(())
        }
        WasmRepr::I64(_) | WasmRepr::F64(_) | WasmRepr::Void => Ok(()),
        WasmRepr::Tagged { .. } => {
            Err(WasmError::Unsupported("release of a Mixed value".to_string()))
        }
    }
}

/// Lowers `Op::Move` / `Op::Borrow`: pure value forwarding, copying the operand's
/// local(s) into the result's local(s) with no refcount change.
fn lower_forward(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    forward_value(ctx, value, inst)
}

/// Copies `value`'s local(s) into the instruction result's local(s), if the
/// instruction produces a result. Errors if the two reprs differ in local arity.
fn forward_value(ctx: &mut FnCtx, value: ValueId, inst: &Instruction) -> Result<()> {
    let Some(result) = inst.result else {
        return Ok(());
    };
    let value_refs = ctx.value_repr(value)?.local_refs();
    let result_refs = ctx.value_repr(result)?.local_refs();
    if value_refs.len() != result_refs.len() {
        return Err(WasmError::Unsupported(format!(
            "forward repr mismatch: operand has {} local(s), result has {}",
            value_refs.len(),
            result_refs.len()
        )));
    }
    for r in &value_refs {
        ctx.fb
            .ins(&format!("local.get {}", r), "forward operand local");
    }
    ctx.emit_store_value(result)
}

/// Returns the local slot a value was loaded from, if its defining instruction is
/// a `LoadLocal`. Used by `ArrayPush` to write a reallocated array pointer back to
/// the variable's slot (mirroring the native `source_load_local_slot`).
pub(super) fn value_source_slot(ctx: &FnCtx, value: ValueId) -> Option<LocalSlotId> {
    let v = ctx.function.value(value)?;
    let ValueDef::Instruction { inst, .. } = v.def else {
        return None;
    };
    let inst = ctx.function.instruction(inst)?;
    if inst.op == Op::LoadLocal {
        if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
            return Some(slot);
        }
    }
    None
}

/// Lowers `Op::ArrayNew`: allocates an empty indexed array with the immediate
/// capacity. The element size defaults to 16 bytes; `__rt_array_push_int` shrinks
/// it to 8 on the first scalar push, matching the native backend.
fn lower_array_new(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let capacity = match &inst.immediate {
        Some(Immediate::Capacity(c)) => *c as i64,
        _ => return Err(WasmError::Unsupported("array_new without a capacity".to_string())),
    };
    ctx.fb
        .ins(&format!("i64.const {}", capacity), "initial capacity");
    ctx.fb
        .ins("i64.const 16", "default elem_size (specialized on first push)");
    ctx.fb.ins("call $__rt_array_new", "allocate indexed array");
    store_result(ctx, inst)
}

/// Lowers `Op::ArrayLen`: reads the i64 length stored at the array header (A+0).
fn lower_array_len(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins("i64.load", "array length @ +0");
    store_result(ctx, inst)
}

/// Lowers `Op::ArrayGet` for scalar (int) arrays via the bounded runtime getter,
/// which returns the PHP null sentinel for an out-of-range index.
fn lower_array_get(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("array_get without a result".to_string()))?;
    let result_repr = ctx.value_repr(result)?.clone();
    match result_repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(operand(inst, 0)?)?; // array pointer
            ctx.emit_load_value(operand(inst, 1)?)?; // index (i64)
            ctx.fb
                .ins("call $__rt_array_get_int", "indexed array get (int)");
            store_result(ctx, inst)
        }
        WasmRepr::Str { .. } => {
            ctx.emit_load_value(operand(inst, 0)?)?; // array pointer
            ctx.emit_load_value(operand(inst, 1)?)?; // index (i64)
            ctx.fb
                .ins("call $__rt_array_get_str", "indexed array get (string)");
            store_result(ctx, inst)
        }
        other => Err(WasmError::Unsupported(format!("array_get into {:?}", other))),
    }
}

/// Lowers `Op::ArrayPush`. Appends via the runtime (which may reallocate) and
/// writes the returned pointer back into the operand value's local and its source
/// slot, so `$arr[] = v` keeps the variable pointing at the live array — exactly
/// what the native backend does.
fn lower_array_push(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let array = operand(inst, 0)?;
    let value = operand(inst, 1)?;
    let value_repr = ctx.value_repr(value)?.clone();
    match value_repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(value)?;
            ctx.fb
                .ins("call $__rt_array_push_int", "append int (may reallocate)");
        }
        WasmRepr::Str { .. } => {
            ctx.emit_load_value(array)?;
            ctx.emit_load_value(value)?; // string pointer (i32) + length (i64)
            ctx.fb
                .ins("call $__rt_array_push_str", "append string (persists + may reallocate)");
        }
        WasmRepr::Ptr(_) => {
            // A Mixed/Union value is a kind-5 Mixed-cell pointer pushed into a
            // value_type-7 mixed-cell array (16-byte slots, cell at slot+0) — the
            // shape the closure/FCC arg buffer uses. The array shares ownership of
            // the cell (incref) and the EIR releases the operand after the push
            // (`release_indexed_array_write_operand`), mirroring the native
            // `__rt_array_push_refcounted` contract (`__rt_array_push_mixed` stores
            // the cell BORROWED). Other heap kinds (array/hash/object containers)
            // have no WASM append helper yet and stay unsupported.
            let value_ir = ctx.function.value(value).map(|v| v.ir_type);
            if !matches!(
                value_ir,
                Some(IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union))
            ) {
                return Err(WasmError::Unsupported(format!(
                    "array_push of {:?} on wasm32-wasi",
                    value_repr
                )));
            }
            let cell = ctx.fresh_temp(ValType::I32);
            ctx.emit_load_value(value)?; // kind-5 Mixed-cell pointer (i32)
            ctx.fb.ins(&format!("local.set {}", cell), "mixed cell to append");
            ctx.fb.ins(
                &format!("(call $__rt_incref (local.get {}))", cell),
                "array shares the mixed cell (the EIR releases the operand after the push)",
            );
            ctx.emit_load_value(array)?;
            ctx.fb
                .ins(&format!("local.get {}", cell), "mixed cell pointer for the append");
            ctx.fb.ins(
                "call $__rt_array_push_mixed",
                "append mixed cell into a value_type-7 array (may reallocate)",
            );
        }
        other => return Err(WasmError::Unsupported(format!("array_push of {:?}", other))),
    }
    // The runtime returned the (possibly reallocated) pointer: store it back into
    // the array operand value's local.
    ctx.emit_store_value(array)?;
    // And mirror it to the source slot so a later LoadLocal sees the live pointer.
    if let Some(slot) = value_source_slot(ctx, array) {
        let array_ref = ctx.value_repr(array)?.local_refs();
        let slot_ref = ctx.slot_repr(slot)?.local_refs();
        if array_ref.len() == 1 && slot_ref.len() == 1 {
            ctx.fb
                .ins(&format!("local.get {}", array_ref[0]), "reallocated array pointer");
            ctx.fb
                .ins(&format!("local.set {}", slot_ref[0]), "write back to the array slot");
        }
    }
    Ok(())
}

/// Lowers `Op::ArraySet` (`$a[i] = v`). Calls the copy-on-write-aware runtime
/// setter (`__rt_array_set_int`/`__rt_array_set_str`), which may clone or
/// reallocate the array, then writes the returned pointer back into the array
/// operand's value local and its source slot — mirroring `lower_array_push` and
/// the native backend. `ArraySet` produces no result value; the array operand IS
/// the in/out storage.
fn lower_array_set(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let array = operand(inst, 0)?;
    let index = operand(inst, 1)?;
    let value = operand(inst, 2)?;
    // The index must be a single i64 (EIR coerces indexed-array indices to int).
    match ctx.value_repr(index)? {
        WasmRepr::I64(_) => {}
        other => {
            return Err(WasmError::Unsupported(format!(
                "array_set index of {:?}",
                other
            )))
        }
    }
    let value_repr = ctx.value_repr(value)?.clone();
    match value_repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(array)?; // array pointer
            ctx.emit_load_value(index)?; // index (i64)
            ctx.emit_load_value(value)?; // scalar value (i64)
            ctx.fb
                .ins("call $__rt_array_set_int", "set scalar element (COW, may reallocate)");
        }
        WasmRepr::Str { .. } => {
            ctx.emit_load_value(array)?; // array pointer
            ctx.emit_load_value(index)?; // index (i64)
            ctx.emit_load_value(value)?; // string pointer (i32) + length (i64)
            ctx.fb
                .ins("call $__rt_array_set_str", "set string element (COW, persists, may reallocate)");
        }
        other => return Err(WasmError::Unsupported(format!("array_set of {:?}", other))),
    }
    // The runtime returned the (possibly cloned/reallocated) pointer: store it
    // back into the array operand value's local.
    ctx.emit_store_value(array)?;
    // And mirror it to the source slot so a later LoadLocal sees the live pointer.
    if let Some(slot) = value_source_slot(ctx, array) {
        let array_ref = ctx.value_repr(array)?.local_refs();
        let slot_ref = ctx.slot_repr(slot)?.local_refs();
        if array_ref.len() == 1 && slot_ref.len() == 1 {
            ctx.fb
                .ins(&format!("local.get {}", array_ref[0]), "reallocated array pointer");
            ctx.fb
                .ins(&format!("local.set {}", slot_ref[0]), "write back to the array slot");
        }
    }
    Ok(())
}

/// Lowers `Op::MixedBox`: boxes a scalar/string/heap value into a Mixed cell via
/// `__rt_mixed_from_value`, picking the runtime tag from the operand's type. A
/// value that is already a Mixed cell is forwarded unchanged.
fn lower_mixed_box(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let repr = ctx.value_repr(value)?.clone();
    let php = ctx.function.value(value).map(|v| v.php_type.codegen_repr());
    let ir = ctx.function.value(value).map(|v| v.ir_type);
    match repr {
        WasmRepr::I64(local) => {
            // Int -> tag 0, Bool -> tag 3, null (ConstNull, PhpType::Void) -> tag 8
            // (all three are i64-represented; `lower_boxed_null` reaches this arm).
            let tag = match php {
                Some(PhpType::Bool) => 3,
                Some(PhpType::Void) => 8,
                _ => 0,
            };
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (int/bool/null)");
            ctx.fb.ins(&format!("local.get {}", local), "scalar -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb
                .ins("call $__rt_mixed_from_value", "box scalar into a mixed cell");
            store_result(ctx, inst)
        }
        WasmRepr::F64(local) => {
            ctx.fb.ins("i64.const 2", "mixed tag (float)");
            ctx.fb.ins(&format!("local.get {}", local), "float value");
            ctx.fb.ins("i64.reinterpret_f64", "float bits -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box float");
            store_result(ctx, inst)
        }
        WasmRepr::Str { ptr, len } => {
            ctx.fb.ins("i64.const 1", "mixed tag (string)");
            ctx.fb.ins(&format!("local.get {}", ptr), "string pointer");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
            ctx.fb.ins(&format!("local.get {}", len), "string length -> hi");
            ctx.fb
                .ins("call $__rt_mixed_from_value", "box string (persists a copy)");
            store_result(ctx, inst)
        }
        WasmRepr::Ptr(local) => match ir {
            // A value that is already a Mixed cell: forward it unchanged.
            Some(IrType::Heap(IrHeapKind::Mixed)) => forward_value(ctx, value, inst),
            Some(IrType::Heap(kind)) => {
                let tag = match kind {
                    IrHeapKind::Array => 4,
                    IrHeapKind::Hash => 5,
                    IrHeapKind::Object => 6,
                    other => {
                        return Err(WasmError::Unsupported(format!(
                            "mixed_box of heap kind {:?}",
                            other
                        )))
                    }
                };
                ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (heap kind)");
                ctx.fb.ins(&format!("local.get {}", local), "heap pointer");
                ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
                ctx.fb.ins("i64.const 0", "hi unused");
                ctx.fb
                    .ins("call $__rt_mixed_from_value", "box heap value (increfs the child)");
                store_result(ctx, inst)
            }
            _ => Err(WasmError::Unsupported("mixed_box of a non-heap pointer".to_string())),
        },
        WasmRepr::Tagged { .. } => {
            Err(WasmError::Unsupported("mixed_box of a tagged scalar".to_string()))
        }
        WasmRepr::Void => Err(WasmError::Unsupported("mixed_box of void".to_string())),
    }
}

/// Lowers `Op::MixedTagOf`: returns the runtime tag integer of a Mixed value by
/// unboxing it and keeping only the tag result.
fn lower_mixed_tag_of(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb
        .ins("call $__rt_mixed_unbox", "unbox -> (tag, lo, hi)");
    ctx.fb.ins("drop", "discard hi");
    ctx.fb.ins("drop", "discard lo");
    store_result(ctx, inst)
}

/// Lowers `Op::IterStart`: records the iterator's source pointer + cursor locals.
/// Indexed arrays (`PhpType::Array`) iterate by element index; associative arrays
/// (`PhpType::AssocArray`) iterate over the insertion-order entry list (cursor = slot
/// index), with `elem` set to the hash's value type.
fn lower_iter_start(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let iter = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("iter_start without a result".to_string()))?;
    let source = operand(inst, 0)?;
    let src_php = ctx
        .function
        .value(source)
        .map(|v| v.php_type.codegen_repr());
    let (elem, is_hash) = match src_php {
        Some(PhpType::Array(inner)) => (inner.codegen_repr(), false),
        Some(PhpType::AssocArray { value, .. }) => (value.codegen_repr(), true),
        Some(other) => {
            return Err(WasmError::Unsupported(format!("foreach over {:?}", other)))
        }
        None => return Err(WasmError::Unsupported("iter_start source has no type".to_string())),
    };
    ctx.iter_declare(iter, source, elem, is_hash)
}

/// Lowers `Op::IterNext` and pushes the i64 loop-continue boolean the header's `CondBr`
/// consumes. For an indexed array it pre-increments the cursor and tests `cursor <
/// length`. For a hash it calls `__rt_hash_iter_next(source, cursor)`, which advances the
/// slot cursor in insertion order and returns `(new_cursor, has_more)`; the new cursor is
/// stored back and `has_more` becomes the loop condition.
fn lower_iter_next(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let iter = operand(inst, 0)?;
    let slots = ctx.iter_slots(iter)?;
    let (src, cur, is_hash) = (slots.source.clone(), slots.cursor.clone(), slots.is_hash);
    if is_hash {
        ctx.fb.ins(&format!("local.get {}", src), "hash source");
        ctx.fb.ins(&format!("local.get {}", cur), "current slot cursor");
        ctx.fb
            .ins("call $__rt_hash_iter_next", "advance to the next entry in insertion order");
        // Returns (new_cursor, has_more) with has_more on top.
        let has_more = ctx.fresh_temp(ValType::I64);
        ctx.fb.ins(&format!("local.set {}", has_more), "captured has_more");
        ctx.fb.ins(&format!("local.set {}", cur), "store advanced slot cursor");
        ctx.fb.ins(&format!("local.get {}", has_more), "has_more for the loop CondBr");
        return store_result(ctx, inst);
    }
    ctx.fb.ins(&format!("local.get {}", cur), "current cursor");
    ctx.fb.ins("i64.const 1", "advance by one");
    ctx.fb.ins("i64.add", "cursor + 1");
    ctx.fb.ins(&format!("local.set {}", cur), "store advanced cursor");
    ctx.fb.ins(&format!("local.get {}", cur), "cursor");
    ctx.fb.ins(&format!("local.get {}", src), "source array");
    ctx.fb.ins("i64.load", "array length @ +0");
    ctx.fb.ins("i64.lt_s", "cursor < length");
    ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
    store_result(ctx, inst)
}

/// Lowers `Op::IterCurrentKey`. For an indexed array the key is the cursor (boxed into a
/// Mixed int when the result is Mixed, else the raw i64). For a hash it delegates to
/// `inst_hash::lower_hash_iter_key`, which reads the key fields from the current entry.
fn lower_iter_current_key(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let iter = operand(inst, 0)?;
    let slots = ctx.iter_slots(iter)?;
    if slots.is_hash {
        let (src, cur) = (slots.source.clone(), slots.cursor.clone());
        return super::inst_hash::lower_hash_iter_key(ctx, inst, &src, &cur);
    }
    let cur = slots.cursor.clone();
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("iter_current_key without a result".to_string()))?;
    let result_repr = ctx.value_repr(result)?.clone();
    match result_repr {
        WasmRepr::I64(_) => {
            ctx.fb.ins(&format!("local.get {}", cur), "key = cursor");
            store_result(ctx, inst)
        }
        WasmRepr::Ptr(_) | WasmRepr::Tagged { .. } => {
            ctx.fb.ins("i64.const 0", "mixed tag (int key)");
            ctx.fb.ins(&format!("local.get {}", cur), "cursor -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb
                .ins("call $__rt_mixed_from_value", "box the integer key");
            store_result(ctx, inst)
        }
        other => Err(WasmError::Unsupported(format!("iter key into {:?}", other))),
    }
}

/// Lowers `Op::IterCurrentValue`. For an indexed array it reads `source[cursor]` with the
/// element getter picked from the element type, boxing into a Mixed cell when the value
/// variable is Mixed (the usual case). For a hash it delegates to
/// `inst_hash::lower_hash_iter_value`, which reads the value fields from the current
/// entry and reconstructs an owned result.
fn lower_iter_current_value(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let iter = operand(inst, 0)?;
    let slots = ctx.iter_slots(iter)?;
    if slots.is_hash {
        let (src, cur) = (slots.source.clone(), slots.cursor.clone());
        return super::inst_hash::lower_hash_iter_value(ctx, inst, &src, &cur);
    }
    let (src, cur, elem) = (slots.source.clone(), slots.cursor.clone(), slots.elem.clone());
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("iter_current_value without a result".to_string()))?;
    let result_repr = ctx.value_repr(result)?.clone();
    let boxed = matches!(result_repr, WasmRepr::Ptr(_) | WasmRepr::Tagged { .. });
    match &elem {
        PhpType::Int | PhpType::Bool => {
            let tag = if matches!(elem, PhpType::Bool) { 3 } else { 0 };
            if boxed {
                ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag");
            }
            ctx.fb.ins(&format!("local.get {}", src), "source array");
            ctx.fb.ins(&format!("local.get {}", cur), "cursor index");
            ctx.fb
                .ins("call $__rt_array_get_int", "foreach element (int)");
            if boxed {
                ctx.fb.ins("i64.const 0", "hi unused");
                ctx.fb
                    .ins("call $__rt_mixed_from_value", "box the element");
            }
            store_result(ctx, inst)
        }
        PhpType::Str => {
            if boxed {
                let tmp_len = ctx.fresh_temp(ValType::I64);
                let tmp_ptr = ctx.fresh_temp(ValType::I32);
                ctx.fb.ins(&format!("local.get {}", src), "source array");
                ctx.fb.ins(&format!("local.get {}", cur), "cursor index");
                ctx.fb
                    .ins("call $__rt_array_get_str", "foreach element (string)");
                ctx.fb.ins(&format!("local.set {}", tmp_len), "element length");
                ctx.fb.ins(&format!("local.set {}", tmp_ptr), "element pointer");
                ctx.fb.ins("i64.const 1", "mixed tag (string)");
                ctx.fb.ins(&format!("local.get {}", tmp_ptr), "ptr");
                ctx.fb.ins("i64.extend_i32_u", "ptr -> lo");
                ctx.fb.ins(&format!("local.get {}", tmp_len), "len -> hi");
                ctx.fb
                    .ins("call $__rt_mixed_from_value", "box the string element");
                store_result(ctx, inst)
            } else {
                ctx.fb.ins(&format!("local.get {}", src), "source array");
                ctx.fb.ins(&format!("local.get {}", cur), "cursor index");
                ctx.fb
                    .ins("call $__rt_array_get_str", "foreach element (string)");
                store_result(ctx, inst)
            }
        }
        other => Err(WasmError::Unsupported(format!(
            "foreach value of element type {:?}",
            other
        ))),
    }
}

/// Lowers `IsNull` for i64 operands by comparing against the null sentinel.
fn lower_is_null(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let op0 = operand(inst, 0)?;
    let repr = ctx.value_repr(op0)?.clone();
    match repr {
        WasmRepr::I64(_) => {
            ctx.emit_load_value(op0)?;
            ctx.fb
                .ins("i64.const 9223372036854775806", "null sentinel");
            ctx.fb.ins("i64.eq", "is_null = x == sentinel");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
        }
        other => {
            return Err(WasmError::Unsupported(format!("is_null of {:?}", other)));
        }
    }
    store_result(ctx, inst)
}
