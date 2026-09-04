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
//! - Resources retain and release their opaque handles through the authoritative
//!   runtime resource registry rather than heap-block reference counts.

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
    let raw_ty = ctx.raw_value_php_type(value)?;
    let ty = ctx.load_value_to_result(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        retain_loaded_resource(ctx);
        return store_if_result(ctx, inst);
    }
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
    if value_is_scratch_string(ctx, value)? {
        return Ok(());
    }

    let skip_label = ctx.next_label("release_unless_aliases_skip");
    let value_reg = abi::int_result_reg(ctx.emitter);
    let result_reg = abi::symbol_scratch_reg(ctx.emitter);
    let ty = ctx.load_value_to_result(value)?;
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
            release_loaded_string(ctx);
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

    let raw_ty = ctx.raw_value_php_type(value)?;
    let ty = ctx.load_value_to_result(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        release_loaded_resource(ctx);
        return Ok(());
    }
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
            )) => matches!(
                target.result_ownership(),
                crate::builtins::semantics::BuiltinResultOwnership::Fresh
            ),
            Some(crate::ir::Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::ProfiledFunction { target, .. },
            )) => matches!(
                target.result_ownership(),
                crate::builtins::semantics::BuiltinResultOwnership::Fresh
            ),
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
            ctx.emitter.instruction(
                &format!("mov {}, {}", result_reg, ptr_reg)
            );                                                                  // pass the loaded string pointer to the validating heap-free helper
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
        Arch::X86_64 => {
            if ptr_reg != result_reg {
                ctx.emitter.instruction(
                    &format!("mov {}, {}", result_reg, ptr_reg)
                );                                                              // pass the loaded string pointer to the validating heap-free helper
            }
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
    }
}

/// Retains the loaded opaque resource handle and leaves it as the acquire result.
///
/// The runtime helper accepts the normal target integer argument and returns the
/// same handle, allowing `store_if_result` to forward the acquired value.
fn retain_loaded_resource(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the loaded opaque resource handle to the registry retain helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_resource_retain");
}

/// Releases the loaded opaque resource handle through the runtime registry.
fn release_loaded_resource(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the loaded opaque resource handle to the registry release helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_resource_release");
}
