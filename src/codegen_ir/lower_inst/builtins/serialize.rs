//! Purpose:
//! Lowers the `serialize()` and `unserialize()` builtins on the EIR backend.
//! Bridges already-evaluated EIR operands to the shared serialize runtime helpers.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - `serialize()` prepares a `(tag, lo, hi)` triple for the static argument type and
//!   tail-calls `__rt_serialize_value`; a Mixed argument is unboxed by
//!   `__rt_serialize_mixed`. The result is a borrowed `_concat_buf` string slice that
//!   `store_if_result` persists on store, like other string builtins.
//! - `unserialize()` passes the source string to `__rt_unserialize_mixed` and boxes a
//!   PHP `false` when the parser reports failure (a null result pointer).
//! - Array (`a:`) serialization is added in a later increment; until then a non-scalar
//!   static argument type is rejected at lowering time rather than mis-serialized.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::load_value_to_first_int_arg;
use super::{expect_operand, store_if_result};

/// Lowers `serialize($value)` into the shared serialize runtime helper.
///
/// Scalar static types are formatted directly through `__rt_serialize_value`; a
/// Mixed/Union argument is unboxed and dispatched by `__rt_serialize_mixed`.
/// Non-scalar static types (arrays/objects) are not yet supported and are rejected.
pub(super) fn lower_serialize(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "serialize", 1)?;
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?;
    let is_x86 = ctx.emitter.target.arch == Arch::X86_64;

    // Reset the reference-tracking state (value counter + seen-objects map) so this
    // top-level serialize() assigns r:/R: indices from scratch, matching PHP.
    abi::emit_call_label(ctx.emitter, "__rt_serialize_begin");

    match value_ty.codegen_repr() {
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax"); // value_lo = integer payload
                ctx.emitter.instruction("mov rdi, 0"); // value_tag = int
                ctx.emitter.instruction("mov rdx, 0"); // value_hi unused
            } else {
                ctx.emitter.instruction("mov x1, x0"); // value_lo = integer payload
                ctx.emitter.instruction("mov x0, #0"); // value_tag = int
                ctx.emitter.instruction("mov x2, #0"); // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_value");
        }
        PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax"); // value_lo = bool payload
                ctx.emitter.instruction("mov rdi, 3"); // value_tag = bool
                ctx.emitter.instruction("mov rdx, 0"); // value_hi unused
            } else {
                ctx.emitter.instruction("mov x1, x0"); // value_lo = bool payload
                ctx.emitter.instruction("mov x0, #3"); // value_tag = bool
                ctx.emitter.instruction("mov x2, #0"); // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_value");
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("movq rsi, xmm0"); // value_lo = float bit pattern
                ctx.emitter.instruction("mov rdi, 2"); // value_tag = float
                ctx.emitter.instruction("mov rdx, 0"); // value_hi unused
            } else {
                ctx.emitter.instruction("fmov x1, d0"); // value_lo = float bit pattern
                ctx.emitter.instruction("mov x0, #2"); // value_tag = float
                ctx.emitter.instruction("mov x2, #0"); // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_value");
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax"); // value_lo = string pointer (len stays in rdx)
                ctx.emitter.instruction("mov rdi, 1"); // value_tag = string
            } else {
                ctx.emitter.instruction("mov x0, #1"); // value_tag = string (lo/hi already in x1/x2)
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_value");
        }
        PhpType::Void | PhpType::Never => {
            if is_x86 {
                ctx.emitter.instruction("mov rdi, 8"); // value_tag = null
                ctx.emitter.instruction("mov rsi, 0"); // value_lo unused
                ctx.emitter.instruction("mov rdx, 0"); // value_hi unused
            } else {
                ctx.emitter.instruction("mov x0, #8"); // value_tag = null
                ctx.emitter.instruction("mov x1, #0"); // value_lo unused
                ctx.emitter.instruction("mov x2, #0"); // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_value");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_serialize_mixed");
        }
        PhpType::Array(_) => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax"); // value_lo = indexed array pointer
                ctx.emitter.instruction("mov rdi, 4"); // value_tag = indexed array
                ctx.emitter.instruction("mov rdx, 0"); // value_hi unused
            } else {
                ctx.emitter.instruction("mov x1, x0"); // value_lo = indexed array pointer
                ctx.emitter.instruction("mov x0, #4"); // value_tag = indexed array
                ctx.emitter.instruction("mov x2, #0"); // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_value");
        }
        PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax"); // value_lo = hash pointer
                ctx.emitter.instruction("mov rdi, 5"); // value_tag = associative array
                ctx.emitter.instruction("mov rdx, 0"); // value_hi unused
            } else {
                ctx.emitter.instruction("mov x1, x0"); // value_lo = hash pointer
                ctx.emitter.instruction("mov x0, #5"); // value_tag = associative array
                ctx.emitter.instruction("mov x2, #0"); // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_value");
        }
        PhpType::Object(_) => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax"); // value_lo = object pointer
                ctx.emitter.instruction("mov rdi, 6"); // value_tag = object
                ctx.emitter.instruction("mov rdx, 0"); // value_hi unused
            } else {
                ctx.emitter.instruction("mov x1, x0"); // value_lo = object pointer
                ctx.emitter.instruction("mov x0, #6"); // value_tag = object
                ctx.emitter.instruction("mov x2, #0"); // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_value");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "serialize() of {:?} is not yet supported",
                other
            )));
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `unserialize($data, $options?)` into the shared unserialize runtime helper.
///
/// The source string is parsed by `__rt_unserialize_mixed`; a null result pointer
/// (parse error or unsupported wire form) is boxed as PHP `false`. The optional
/// `$options` argument is accepted but currently ignored.
pub(super) fn lower_unserialize(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "unserialize expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    let data = expect_operand(inst, 0)?;
    // Reset the per-call value registry so r:/R: back-references resolve against
    // this unserialize() call's own pre-order value indices.
    abi::emit_call_label(ctx.emitter, "__rt_unserialize_begin");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    match ctx.value_php_type(data)?.codegen_repr() {
        PhpType::Str => {
            ctx.load_string_value_to_regs(data, ptr_reg, len_reg)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, data)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "unserialize() of {:?} is not supported",
                other
            )));
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_unserialize_mixed");
    box_false_on_unserialize_failure(ctx);
    store_if_result(ctx, inst)
}

/// Replaces a null `__rt_unserialize_mixed` result with a boxed PHP `false`.
///
/// PHP's `unserialize()` returns `false` on malformed or unsupported input; the
/// runtime helper signals that with a null Mixed pointer, which this boxes into a
/// `Mixed(bool=false)` cell so the EIR result stays a valid boxed value.
fn box_false_on_unserialize_failure(ctx: &mut FunctionContext<'_>) {
    let done = ctx.next_label("unserialize_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", done)); // success returns a boxed Mixed
            ctx.emitter.instruction("mov x0, #3"); // tag = bool
            ctx.emitter.instruction("mov x1, #0"); // value_lo = false
            ctx.emitter.instruction("mov x2, #0"); // value_hi unused
            ctx.emitter.instruction("bl __rt_mixed_from_value"); // box the PHP false result
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax"); // success returns a non-null Mixed pointer
            ctx.emitter.instruction(&format!("jne {}", done)); // skip false boxing on success
            ctx.emitter.instruction("mov rax, 3"); // tag = bool
            ctx.emitter.instruction("mov rdi, 0"); // value_lo = false
            ctx.emitter.instruction("mov rsi, 0"); // value_hi unused
            ctx.emitter.instruction("call __rt_mixed_from_value"); // box the PHP false result
            ctx.emitter.label(&done);
        }
    }
}
