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
use crate::ir::{Instruction, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
use crate::codegen::{CodegenIrError, Result};

/// Lowers an ownership acquire by making the operand safe to store as a new owner.
pub(super) fn lower_acquire(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    match ty {
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
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

/// Lowers a release only for values that own or may own runtime-managed storage.
pub(super) fn lower_release(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let ownership = ctx.value_ownership(value)?;
    if !ownership.may_require_release() {
        return Ok(());
    }
    if value_is_scratch_string(ctx, value)? {
        return Ok(());
    }

    let ty = ctx.load_value_to_result(value)?;
    match ty {
        PhpType::Str => {
            release_loaded_string(ctx);
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

/// Returns whether a runtime call hands back storage the caller may have to release.
///
/// `fread()` returns a slice of the 64 KiB concat arena while the read fits in it, and
/// an owned `__rt_heap_alloc` block when it does not. `sys_get_temp_dir()` splits the
/// same way by target rather than by size: the Windows arm hands back an owned block
/// holding the converted path, where the POSIX arm returns borrowed storage. Either
/// way the caller cannot know at compile time which it got, so neither may be skipped
/// here. `__rt_heap_free_safe` then tells them apart exactly rather than
/// heuristically: `_concat_buf` and `_heap_buf` are separate `.comm` objects, so an
/// arena pointer can never satisfy its managed-heap window test and passes through
/// untouched, leaving only the owned block to be reclaimed.
///
/// Their `result_ownership()` stays `MayAliasArguments` deliberately. Calling them
/// `Fresh` would be untrue — the borrowed case is not owned storage, and other passes
/// read that contract — whereas this predicate only decides whether a release is
/// emitted, which is safe to answer conservatively. None of them takes a string
/// argument, so the aliasing the conservative classification guards against cannot
/// arise for them.
///
/// Without this, the release the EIR already emits for these values was dropped in the
/// backend, and every such call leaked its block for the life of the process.
///
/// `stream_get_contents()` is deliberately NOT listed. It would be unreachable here:
/// its checker signature is the union `string|false`, so the value is never
/// `PhpType::Str` and [`value_is_scratch_string`] returns before this predicate is
/// consulted. Its release always takes the `__rt_decref_mixed` arm instead, and the
/// block its accumulation buffer strands is a separate defect that lives in the
/// boxing path — see the note in `io/fread.rs`.
fn returns_arena_slice_or_owned_block(target: crate::ir::RuntimeFnId) -> bool {
    matches!(
        target,
        crate::ir::RuntimeFnId::Fread | crate::ir::RuntimeFnId::SysGetTempDir
    )
}

/// Returns whether a value is a transient string backed by concat scratch storage.
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
    if inst.op == Op::RuntimeCall {
        let result_is_fresh = match inst.immediate {
            Some(crate::ir::Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::ArrayFetchForWrite,
            )) => false,
            Some(crate::ir::Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::Function(target),
            )) => {
                matches!(
                    target.result_ownership(),
                    crate::builtins::semantics::BuiltinResultOwnership::Fresh
                ) || returns_arena_slice_or_owned_block(target)
            }
            Some(crate::ir::Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::UnaryString(_),
            )) => true,
            _ => false,
        };
        return Ok(!result_is_fresh);
    }
    Ok(matches!(
        inst.op,
        Op::IToStr
            | Op::FToStr
            | Op::BoolToStr
            | Op::ResourceToStr
            | Op::MixedCastString
            | Op::StrConcat
            | Op::StrCharAt
            | Op::StrInterpolate
    ))
}

/// Lowers a pure ownership forwarding opcode by copying the operand into the result slot.
pub(super) fn lower_forward(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    ctx.load_value_to_result(value)?;
    store_if_result(ctx, inst)
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
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, ptr_reg)); // pass the loaded string pointer to the validating heap-free helper
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
        Arch::X86_64 => {
            if ptr_reg != result_reg {
                ctx.emitter.instruction(&format!("mov {}, {}", result_reg, ptr_reg)); // pass the loaded string pointer to the validating heap-free helper
            }
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
    }
}
