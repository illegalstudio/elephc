//! Purpose:
//! Lowers explicit EIR ownership operations for the Phase 04 backend.
//! Handles string persistence, heap retains, releases, and pure forwarding ops.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - `Acquire` turns PHP strings into heap-owned storage so local slots do not
//!   alias transient concat buffers or immutable data-section literals.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{
    Immediate, Instruction, Op, RuntimeCallTarget, RuntimeFnId, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
use crate::codegen::{CodegenIrError, Result};

/// Publishes a temporary owner's frame slot above the current PHP catch boundary.
///
/// The record's cleanup discipline follows the SLOT, not its PHP type. A reference-cell owner
/// slot carries its cell's PAYLOAD type, so a cell whose payload is a callable would otherwise
/// select the descriptor release and hand a cell address to `__rt_callable_descriptor_release`.
/// Every owned cell uses `REFERENCE_CELL_HEAP_KIND`, which the generic `__rt_decref_any` dispatcher already
/// routes to `__rt_reference_cell_release`, so a cell slot always uses the generic entry.
pub(super) fn lower_push_call_operand_owner(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = super::expect_local_slot(inst)?;
    let offset = ctx.local_offset(slot)?;
    let holds_reference_cell = matches!(
        ctx.local_kind(slot)?,
        crate::ir::LocalKind::RefCell | crate::ir::LocalKind::ReturnRefCell
    );
    let callable = !holds_reference_cell
        && ctx.local_php_type(slot)?.codegen_repr() == PhpType::Callable;
    let address = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_frame_slot_address(ctx.emitter, address, offset);
    abi::emit_push_call_operand_owner(ctx.emitter, address, callable);
    Ok(())
}

/// Removes the innermost operand scope before the owning slot is retired normally.
pub(super) fn lower_pop_call_operand_owner(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::expect_local_slot(inst)?;
    abi::emit_pop_call_operand_owner(ctx.emitter);
    Ok(())
}

/// Lowers an ownership acquire by making the operand safe to store as a new owner.
pub(super) fn lower_acquire(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    if crate::codegen::stack_numeric::is_stack_value(ctx.function, &ctx.numeric_cell_offsets, value) {
        return store_if_result(ctx, inst);
    }
    match ty {
        PhpType::Str => {
            persist_loaded_string(ctx, value)?;
        }
        PhpType::Callable => {
            abi::emit_incref_if_refcounted(ctx.emitter, &ty);
        }
        PhpType::Buffer(_) => {}
        other if other.is_refcounted() => {
            abi::emit_incref_if_refcounted(ctx.emitter, &other);
        }
        PhpType::Void | PhpType::Never => {}
        // Scalar types (Int, Float) arise when a checked op's result is narrowed
        // to a scalar by constant folding. The acquire instruction's result is
        // still typed Heap(Mixed), so box the scalar into a Mixed cell to match
        // the expected storage type.
        PhpType::Int | PhpType::Float => {
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &ty);
        }
        other => {
            if inst.result.is_some() {
                return Err(CodegenIrError::unsupported(format!(
                    "acquire for PHP type {:?}",
                    other
                )));
            }
        }
    }
    store_if_result(ctx, inst)
}

/// Reports whether a string value is a direct view of one selected source byte.
fn is_string_byte_view(ctx: &FunctionContext<'_>, value: ValueId) -> bool {
    ctx.function.value(value).and_then(|value| match value.def {
        ValueDef::Instruction { inst, .. } => ctx.function.instruction(inst),
        _ => None,
    }).is_some_and(|inst| inst.op == Op::StrCharAt)
}

/// Copies a selected byte through stack storage before a helper can persist the string.
/// Empty offset results keep their original pointer and length so missing sentinels survive.
pub(super) fn stage_string_byte_view(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    pointer_reg: &str,
    length_reg: &str,
) -> bool {
    if !is_string_byte_view(ctx, value) {
        return false;
    }
    let ready = ctx.next_label("str_byte_persist_ready");
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- stage a present offset byte before string persistence --
            ctx.emitter.instruction(&format!("cmp {length_reg}, #1"));          // only a present offset has one byte to stage
            ctx.emitter.instruction(&format!("b.ne {ready}"));                  // preserve the original empty or missing string pair
            ctx.emitter.instruction(&format!("ldrb w9, [{pointer_reg}]"));      // read the selected byte before its receiver can be retired
            ctx.emitter.instruction("strb w9, [sp]");                           // stage the byte outside the managed heap
            ctx.emitter.instruction(&format!("mov {pointer_reg}, sp"));         // persistence must duplicate stack storage instead of taking over a heap view
        }
        Arch::X86_64 => {
            // -- stage a present offset byte before string persistence --
            ctx.emitter.instruction(&format!("cmp {length_reg}, 1"));           // only a present offset has one byte to stage
            ctx.emitter.instruction(&format!("jne {ready}"));                   // preserve the original empty or missing string pair
            ctx.emitter.instruction(&format!("movzx r10d, BYTE PTR [{pointer_reg}]")); // read the selected byte before its receiver can be retired
            ctx.emitter.instruction("mov BYTE PTR [rsp], r10b");                // stage the byte outside the managed heap
            ctx.emitter.instruction(&format!("mov {pointer_reg}, rsp"));        // persistence must duplicate stack storage instead of taking over a heap view
        }
    }
    ctx.emitter.label(&ready);
    true
}

/// Releases stack storage reserved by [`stage_string_byte_view`] after its consumer returns.
pub(super) fn release_string_byte_stage(ctx: &mut FunctionContext<'_>, staged: bool) {
    if staged {
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
}

/// Copies offset bytes through stack storage so payload bytes cannot impersonate heap headers.
/// Empty offset results bypass staging so the persistence helper preserves missing sentinels.
pub(super) fn persist_loaded_string(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let (pointer_reg, length_reg) = abi::string_result_regs(ctx.emitter);
    let staged = stage_string_byte_view(ctx, value, pointer_reg, length_reg);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    release_string_byte_stage(ctx, staged);
    Ok(())
}

/// Lowers `ReleaseUnlessAliases`: releases operand 0 only when it is not the same payload the
/// call returned in operand 1.
///
/// A callee summarized as *possibly* returning a parameter (`if ($c) return $x; return 7;`)
/// makes the caller suppress that argument's release on every path, because releasing it on the
/// branch that hands the box back would free it twice (issue #604). Suppressing it on the other
/// branches leaks one block per call (issue #619). Comparing the two payloads at runtime picks
/// the right behaviour per call: identical pointers mean ownership moved into the result and the
/// caller must keep its hands off, different pointers mean the callee dropped the argument and
/// the caller still owns it.
pub(super) fn lower_release_unless_aliases(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let result = expect_operand(inst, 1)?;
    let ownership = ctx.value_ownership(value)?;
    if !ownership.may_require_release() {
        return Ok(());
    }
    let transient = value_is_transient_string(ctx, value)?;
    if value_is_scratch_string(ctx, value)? && !transient {
        return Ok(());
    }

    let skip_label = ctx.next_label("release_unless_aliases_skip");
    let result_reg = abi::symbol_scratch_reg(ctx.emitter);
    let ty = ctx.load_value_to_result(value)?;
    // A string loads into the string result pair, whose pointer register is not the integer
    // result register on AArch64 (`x1` against `x0`); boxed payloads stay single-word values.
    let value_reg = if ty == PhpType::Str {
        abi::string_result_regs(ctx.emitter).0
    } else {
        abi::int_result_reg(ctx.emitter)
    };
    ctx.load_value_to_reg(result, result_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", value_reg, result_reg));    // compare the argument payload with the value the callee returned
            ctx.emitter.instruction(&format!("b.eq {}", skip_label));           // ownership moved into the result: the caller must not release it
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", value_reg, result_reg));    // compare the argument payload with the value the callee returned
            ctx.emitter.instruction(&format!("je {}", skip_label));             // ownership moved into the result: the caller must not release it
        }
    }
    match ty {
        PhpType::Str => {
            if transient { release_transient_string(ctx, value)?; }
            else { release_loaded_string(ctx); }
        }
        PhpType::Callable => {
            abi::emit_decref_if_refcounted(ctx.emitter, &ty);
        }
        PhpType::Buffer(_) => {}
        other if other.is_refcounted() => {
            abi::emit_decref_if_refcounted(ctx.emitter, &other);
        }
        _ => {}
    }
    ctx.emitter.label(&skip_label);
    Ok(())
}

/// Lowers `EnsureOwned` on a string: claims a transient heap block for the value's owners.
///
/// `__rt_str_claim` retags a `CONCAT_TEMP_HEAP_KIND` block as an owned string in place and
/// leaves every other pointer (concat scratch, literals, already owned blocks) unchanged, so the
/// result shares the operand's payload without a copy. The result is released like any owned
/// string, which `__rt_heap_free_safe` turns into a no-op for non-heap pointers. Once claimed, a
/// later `__rt_str_persist` duplicates the block instead of adopting it, so a callee that
/// persists or hands back the argument can no longer take over a block its caller still frees.
pub(super) fn lower_ensure_owned(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    if ty != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "ensure_owned for PHP type {:?}",
            ty
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_claim");
    store_if_result(ctx, inst)
}

/// Lowers a release only for values that own or may own runtime-managed storage.
pub(super) fn lower_release(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    release_value_if_owned(ctx, value)
}

/// Emits the release of one SSA value when it owns or may own runtime-managed storage.
///
/// Shared by `release` and by instructions that consume their operand on one path only,
/// such as a static-local initializer that is dropped once the slot is already initialized.
pub(super) fn release_value_if_owned(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let ownership = ctx.value_ownership(value)?;
    if !ownership.may_require_release() {
        return Ok(());
    }
    if crate::codegen::stack_numeric::is_stack_value(ctx.function, &ctx.numeric_cell_offsets, value) {
        return Ok(());
    }
    if crate::codegen::stack_numeric::numeric_local_read(ctx.function, &ctx.numeric_local_slots, value) {
        ctx.load_value_to_result(value)?;
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
        return Ok(());
    }
    if value_is_transient_string(ctx, value)? {
        release_normalized_string_cast_copies(ctx, value)?;
        return release_transient_string(ctx, value);
    }
    if value_is_scratch_string(ctx, value)? {
        // `trim` and `md5` may answer with a view or a fresh string and then
        // skip releasing that answer. A normalized `(string)` cast still owns
        // the copy it passed in, so that copy is released with the answer,
        // after the caller has read it.
        return release_normalized_string_cast_copies(ctx, value);
    }

    let ty = ctx.load_value_to_result(value)?;
    match ty {
        PhpType::Str => {
            // Free this string before nested cast copies. The nested release
            // reloads its own pointer, so doing it first would free that copy
            // twice and leak this one.
            release_loaded_string(ctx);
            release_normalized_string_cast_copies(ctx, value)?;
        }
        PhpType::Callable => {
            abi::emit_decref_if_refcounted(ctx.emitter, &ty);
        }
        PhpType::Buffer(_) => {}
        other if other.is_refcounted() => {
            abi::emit_decref_if_refcounted(ctx.emitter, &other);
        }
        PhpType::Void | PhpType::Never => {}
        // Scalar types (Int, Float) arise when a checked op's result is narrowed
        // to a scalar by constant folding; release is a no-op for non-refcounted
        // scalars.
        PhpType::Int | PhpType::Float => {}
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "release for PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Returns whether a producer can return scratch or an unadopted transient heap block.
fn value_is_transient_string(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    if ctx.value_php_type(value)? != PhpType::Str { return Ok(false); }
    Ok(ctx.function.value(value).and_then(|value| match value.def {
        ValueDef::Instruction { inst, .. } => ctx.function.instruction(inst),
        _ => None,
    }).is_some_and(Instruction::creates_transient_string))
}

/// Frees only an unadopted transient block, preserving scratch and already adopted strings.
fn release_transient_string(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    ctx.load_value_to_result(value)?;
    let skip = ctx.next_label("release_transient_skip");
    let (pointer, _) = abi::string_result_regs(ctx.emitter);
    let result = abi::int_result_reg(ctx.emitter);
    // -- classify the producer's exact base before releasing an unadopted temporary --
    if pointer != result {
        ctx.emitter.instruction(&format!("mov {result}, {pointer}"));           // pass the produced string pointer to the heap classifier
    }
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    let kind = crate::codegen_support::sentinels::CONCAT_TEMP_HEAP_KIND;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {result}, #{kind}"));         // does the producer still own an unadopted heap temporary?
            ctx.emitter.instruction(&format!("b.ne {skip}"));                   // scratch and adopted full-string stores must remain untouched
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {result}, {kind}"));          // does the producer still own an unadopted heap temporary?
            ctx.emitter.instruction(&format!("jne {skip}"));                    // scratch and adopted full-string stores must remain untouched
        }
    }
    ctx.load_value_to_result(value)?;
    release_loaded_string(ctx);
    ctx.emitter.label(&skip);
    Ok(())
}

/// Returns whether the producer's ordinary string release is skipped.
/// Transient heap producers additionally require the conditional retirement above.
fn value_is_scratch_string(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    if ctx.value_php_type(value)? != PhpType::Str {
        return Ok(false);
    }
    let value_metadata = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_metadata.def else {
        return Ok(false);
    };
    let inst = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(inst.string_release_is_noop())
}

/// Lowers a pure ownership forwarding opcode by copying the operand into the result slot.
pub(super) fn lower_forward(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    ctx.load_value_to_result(value)?;
    store_if_result(ctx, inst)
}

/// Releases owned `(string)` copies passed into a runtime call whose own result is not freed.
///
/// The copy exists only so a mixed local or a `??` temporary can be released
/// before the call. `trim` may return a view of that copy, so the copy stays
/// alive until the call's result is released, which is after the caller reads it.
/// Only calls that do not keep the argument are included. `ini_set` stores the
/// pointer and must not have it freed here.
fn release_normalized_string_cast_copies(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let persists = {
        let Some(inst) = ctx.function.value(value).and_then(|value| match value.def {
            ValueDef::Instruction { inst, .. } => ctx.function.instruction(inst),
            _ => None,
        }) else {
            return Ok(());
        };
        let subject = if inst.op == Op::StrPersist {
            let Some(operand) = inst.operands.first().copied() else {
                return Ok(());
            };
            let Some(subject) = ctx.function.value(operand).and_then(|value| match value.def {
                ValueDef::Instruction { inst, .. } => ctx.function.instruction(inst),
                _ => None,
            }) else {
                return Ok(());
            };
            subject
        } else {
            inst
        };
        if subject.op != Op::RuntimeCall || !runtime_call_discards_string_argument(subject) {
            return Ok(());
        }
        subject
            .operands
            .iter()
            .copied()
            .filter(|operand| {
                ctx.function.value(*operand).and_then(|value| match value.def {
                    ValueDef::Instruction { inst, .. } => ctx.function.instruction(inst),
                    _ => None,
                }).is_some_and(|producer| {
                    producer.op == Op::StrPersist && producer.immediate == Some(Immediate::Bool(true))
                })
            })
            .collect::<Vec<_>>()
    };
    for operand in persists {
        let ty = ctx.load_value_to_result(operand)?;
        if ty == PhpType::Str {
            release_loaded_string(ctx);
        }
    }
    Ok(())
}

/// Returns whether this runtime call reads its string arguments without storing them.
fn runtime_call_discards_string_argument(inst: &Instruction) -> bool {
    let Some(Immediate::RuntimeCall(target)) = &inst.immediate else {
        return false;
    };
    let id = match target {
        RuntimeCallTarget::Function(id)
        | RuntimeCallTarget::ProfiledFunction { target: id, .. } => *id,
        _ => return false,
    };
    matches!(
        id,
        RuntimeFnId::Trim
            | RuntimeFnId::Ltrim
            | RuntimeFnId::Rtrim
            | RuntimeFnId::Chop
            | RuntimeFnId::Md5
            | RuntimeFnId::Sha1
            | RuntimeFnId::Sprintf
            | RuntimeFnId::Vsprintf
            | RuntimeFnId::Printf
    )
}

/// Releases a loaded string result through the validating heap-free helper.
///
/// `__rt_heap_free_safe` skips non-heap pointers (null, .rodata, out-of-range) and
/// only frees plausible live heap blocks, so it safely handles the zero-length owned
/// strings that `__rt_str_persist` now allocates as independent blocks. The previous
/// `cbz len` guard skipped them and leaked every owned empty string on reassignment.
fn release_loaded_string(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, _) = abi::string_result_regs(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(                                            // pass the loaded string pointer to validated heap release
                &format!("mov {}, {}", result_reg, ptr_reg)
            );
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
        Arch::X86_64 => {
            if ptr_reg != result_reg {
                ctx.emitter.instruction(                                        // pass the loaded string pointer to validated heap release
                    &format!("mov {}, {}", result_reg, ptr_reg)
                );
            }
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
    }
}
