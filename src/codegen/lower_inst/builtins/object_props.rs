//! Purpose:
//! Lowers the four internal `__elephc_object_*` introspection builtins the injected
//! `var_export` prelude uses to walk an object: `__elephc_object_is_enum`,
//! `__elephc_object_prop_count`, `__elephc_object_prop_name` and
//! `__elephc_object_prop_value`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::runtime_functions` dispatch, through
//!   the `RuntimeFnId::ElephcObject*` targets declared by the builtin registry.
//!
//! Key details:
//! - Every one of them starts by materializing a RAW OBJECT POINTER (or 0) in the
//!   integer result register, so the runtime helpers never have to know whether the
//!   caller had a statically typed object or a boxed `Mixed`. `Mixed` operands are
//!   unboxed here and a non-object payload collapses to 0, which each helper
//!   already treats as "no object".
//! - The helpers themselves live in
//!   `codegen_support::runtime::objects::{enum_debug, export_props}` and read the
//!   same `_class_prop_desc_*` rows `print_r` and `var_dump` walk.
//! - There is no target-specific behavior beyond register naming; both supported
//!   architectures go through the same sequence.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::Result;
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `__elephc_object_is_enum(value)` to a bounded per-class table probe.
///
/// Returns PHP `true` only for an instance whose class is an enum; a non-object
/// value, a null instance and an unknown class id all report `false`.
pub(crate) fn lower_object_is_enum(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_object_is_enum", 1)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_object_is_enum()");
    emit_object_pointer_from_operand(ctx, expect_operand(inst, 0)?)?;
    let skip_label = ctx.next_label("obj_is_enum_not_object");
    let done_label = ctx.next_label("obj_is_enum_done");
    emit_branch_if_result_zero(ctx, &skip_label);
    abi::emit_call_label(ctx.emitter, "__rt_obj_enum_kind");
    // The kind is 0 for a plain class and 1/2/3 for a pure / int-backed /
    // string-backed enum, so PHP's boolean is "kind is non-zero".
    emit_normalize_result_to_bool(ctx);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&skip_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_object_prop_count(value)` to `__rt_obj_prop_count`.
///
/// A non-object operand reaches the helper as a null pointer, which reports 0.
pub(crate) fn lower_object_prop_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_object_prop_count", 1)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_object_prop_count()");
    emit_object_pointer_from_operand(ctx, expect_operand(inst, 0)?)?;
    abi::emit_call_label(ctx.emitter, "__rt_obj_prop_count");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_object_prop_name(value, index)` to `__rt_obj_prop_name`.
///
/// The result is the platform string result pair; an absent or uninitialized
/// property yields a zero-length string.
pub(crate) fn lower_object_prop_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_object_prop_name", 2)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_object_prop_name()");
    emit_object_and_index_arguments(ctx, inst)?;
    abi::emit_call_label(ctx.emitter, "__rt_obj_prop_name");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_object_prop_value(value, index)` to `__rt_obj_prop_value`.
///
/// The helper always returns a freshly allocated Mixed cell (boxed PHP null when
/// there is no such property), matching the `Fresh` ownership the registry
/// declares for this target.
pub(crate) fn lower_object_prop_value(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_object_prop_value", 2)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_object_prop_value()");
    emit_object_and_index_arguments(ctx, inst)?;
    abi::emit_call_label(ctx.emitter, "__rt_obj_prop_value");
    store_if_result(ctx, inst)
}

/// Materializes the object pointer in the first argument register and the property
/// index in the second, evaluating the index FIRST so the object pointer is not
/// clobbered by the index load.
fn emit_object_and_index_arguments(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let index = expect_operand(inst, 1)?;
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(index, index_reg)?;
    abi::emit_push_reg(ctx.emitter, index_reg);
    emit_object_pointer_from_operand(ctx, expect_operand(inst, 0)?)?;
    abi::emit_pop_reg(ctx.emitter, index_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov x1, {}", index_reg));                // property index → second helper argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // object pointer → SysV first argument register
            ctx.emitter
                .instruction(&format!("mov rsi, {}", index_reg));               // property index → SysV second argument register
        }
    }
    Ok(())
}

/// Leaves a raw object pointer (or 0) in the integer result register.
///
/// A statically typed object operand is already a pointer. A `Mixed` operand is
/// unboxed and only a tag-6 payload survives; every other shape — including PHP
/// null — collapses to 0, which the runtime helpers read as "no object".
pub(super) fn emit_object_pointer_from_operand(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let loaded = ctx.load_value_to_result(value)?.codegen_repr();
    match loaded {
        PhpType::Object(_) => Ok(()),
        PhpType::Mixed | PhpType::Union(_) => {
            let not_object = ctx.next_label("obj_props_not_object");
            let done = ctx.next_label("obj_props_unboxed");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #6");                      // only a boxed object payload carries properties
                    ctx.emitter
                        .instruction(&format!("b.ne {}", not_object));          // every other Mixed shape reports "no object"
                    ctx.emitter.instruction("mov x0, x1");                      // unboxed object pointer → integer result register
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 6");                      // only a boxed object payload carries properties
                    ctx.emitter
                        .instruction(&format!("jne {}", not_object));           // every other Mixed shape reports "no object"
                    ctx.emitter.instruction("mov rax, rdi");                    // unboxed object pointer → integer result register
                }
            }
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&not_object);
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            ctx.emitter.label(&done);
            Ok(())
        }
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
    }
}

/// Branches to `label` when the integer result register holds zero.
fn emit_branch_if_result_zero(ctx: &mut FunctionContext<'_>, label: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", result_reg, label));        // a null object pointer takes the caller's empty path
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", result_reg, result_reg));  // is the object pointer zero?
            ctx.emitter.instruction(&format!("jz {}", label));                  // a null object pointer takes the caller's empty path
        }
    }
}

/// Collapses a non-zero integer result to PHP `true` (1) and zero to `false` (0).
fn emit_normalize_result_to_bool(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // compare the enum kind against zero
            ctx.emitter.instruction("cset x0, ne");                             // any non-zero enum kind is PHP true
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // compare the enum kind against zero
            ctx.emitter.instruction("setne al");                                // any non-zero enum kind is PHP true
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean to the full result register
        }
    }
}
