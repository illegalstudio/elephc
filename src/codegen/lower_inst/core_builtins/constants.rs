//! Purpose:
//! Materializes AOT global constant inventories from the prescanned module metadata.
//!
//! Called from:
//! - `super::lower_core_builtin()` for `get_defined_constants()`.
//!
//! Key details:
//! - The flat result and categorized `Core`/`user` result share the same scalar materializer.
//! - Hash entries own boxed Mixed cells whose payload types match the constant declarations.

use std::collections::HashSet;

use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed};
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};

/// Returns every prescanned global constant, optionally grouped by origin.
pub(super) fn lower_get_defined_constants(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let categorize = super::expect_operand(inst, 0)?;
    let categorized = ctx.next_label("get_defined_constants_categorized");
    let done = ctx.next_label("get_defined_constants_done");
    ctx.load_value_to_result(categorize)?;
    emit_branch_if_nonzero(ctx, &categorized);
    let entries = sorted_constant_entries(ctx);
    emit_constant_hash(ctx, &entries)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&categorized);
    emit_categorized_constants(ctx, &entries)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Branches when the current integer result is truthy.
fn emit_branch_if_nonzero(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test the categorized boolean argument
            ctx.emitter.instruction(&format!("b.ne {label}"));                  // select the categorized inventory when true
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test the categorized boolean argument
            ctx.emitter.instruction(&format!("jnz {label}"));                   // select the categorized inventory when true
        }
    }
}

/// Returns a stable copy of constant metadata so emission can mutate the data section.
fn sorted_constant_entries(ctx: &FunctionContext<'_>) -> Vec<(String, ExprKind, PhpType)> {
    let mut entries = ctx
        .module
        .global_constants
        .iter()
        .map(|(name, (value, ty))| (name.clone(), value.clone(), ty.clone()))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

/// Builds the categorized outer hash with independent Core and user maps.
fn emit_categorized_constants(
    ctx: &mut FunctionContext<'_>,
    entries: &[(String, ExprKind, PhpType)],
) -> Result<()> {
    let user_names = ctx
        .module
        .user_defined_constants
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let core = entries
        .iter()
        .filter(|(name, _, _)| !user_names.contains(name))
        .cloned()
        .collect::<Vec<_>>();
    let user = entries
        .iter()
        .filter(|(name, _, _)| user_names.contains(name))
        .cloned()
        .collect::<Vec<_>>();

    allocate_mixed_hash(ctx, 2);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_constant_hash(ctx, &core)?;
    box_owned_mixed_hash(ctx);
    insert_boxed_hash_value(ctx, "Core");
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_constant_hash(ctx, &user)?;
    box_owned_mixed_hash(ctx);
    insert_boxed_hash_value(ctx, "user");
    Ok(())
}

/// Boxes the current owned string-to-Mixed hash as one Mixed value.
fn box_owned_mixed_hash(ctx: &mut FunctionContext<'_>) {
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        },
    );
}

/// Builds one string-keyed Mixed hash from static constant values.
fn emit_constant_hash(
    ctx: &mut FunctionContext<'_>,
    entries: &[(String, ExprKind, PhpType)],
) -> Result<()> {
    allocate_mixed_hash(ctx, entries.len().saturating_mul(2).max(16));
    for (name, value, ty) in entries {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_boxed_constant_value(ctx, value, ty)?;
        insert_boxed_hash_value(ctx, name);
    }
    Ok(())
}

/// Allocates a string-keyed hash whose entries are owned boxed Mixed cells.
fn allocate_mixed_hash(ctx: &mut FunctionContext<'_>, capacity: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity.max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 7);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity.max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 7);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
}

/// Materializes one supported scalar constant as an owned Mixed cell.
fn emit_boxed_constant_value(
    ctx: &mut FunctionContext<'_>,
    value: &ExprKind,
    declared_type: &PhpType,
) -> Result<()> {
    match value {
        ExprKind::IntLiteral(value) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                *value,
            );
            emit_box_current_value_as_mixed(ctx.emitter, declared_type);
        }
        ExprKind::BoolLiteral(value) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                i64::from(*value),
            );
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
        }
        ExprKind::FloatLiteral(value) => {
            let label = ctx.data.add_float(*value);
            abi::emit_load_symbol_to_reg(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                &label,
                0,
            );
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
        }
        ExprKind::StringLiteral(value) => {
            let (label, len) = ctx.data.add_string(value.as_bytes());
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        }
        ExprKind::Null => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
        }
        ExprKind::Negate(inner) => emit_boxed_negative_constant(ctx, &inner.kind)?,
        other => return Err(unsupported_constant_value(other)),
    }
    Ok(())
}

/// Materializes a negated integer or float constant as an owned Mixed cell.
fn emit_boxed_negative_constant(
    ctx: &mut FunctionContext<'_>,
    value: &ExprKind,
) -> Result<()> {
    match value {
        ExprKind::IntLiteral(value) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                value.wrapping_neg(),
            );
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        }
        ExprKind::FloatLiteral(value) => {
            let label = ctx.data.add_float(-value);
            abi::emit_load_symbol_to_reg(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                &label,
                0,
            );
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
        }
        other => return Err(unsupported_constant_value(other)),
    }
    Ok(())
}

/// Constructs the explicit codegen diagnostic for an unsupported constant expression.
fn unsupported_constant_value(value: &ExprKind) -> CodegenIrError {
    CodegenIrError::unsupported(format!(
        "get_defined_constants value expression {:?}",
        value
    ))
}

/// Inserts the current boxed value into the hash pointer saved on the stack.
fn insert_boxed_hash_value(ctx: &mut FunctionContext<'_>, key: &str) {
    let (key_label, key_len) = ctx.data.add_string(key.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the boxed constant value to hash_set
            ctx.emitter.instruction("mov x4, xzr");                             // boxed Mixed cells have no high payload word
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x5", 7);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the boxed constant value to hash_set
            ctx.emitter.instruction("xor r8, r8");                              // boxed Mixed cells have no high payload word
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_load_int_immediate(ctx.emitter, "r9", 7);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
}
