//! Purpose:
//! Lowers typed BCMath EIR calls into target-aware shared runtime helper arguments.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13` for all `RuntimeFnId::Bc*` targets.
//!
//! Key details:
//! - String coercions preserve PHP source values before ABI reordering.
//! - Omitted, literal-null, and dynamically boxed-null scales select process scale distinctly from zero.
//! - Call sites publish bridge entries before invoking late-bound `__rt_bc*` helpers.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::Result;
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::strings::{load_as_int, load_string_arg_to_regs};
use super::{ensure_arg_count, ensure_arg_count_between, store_if_result};

/// Lowers `bcadd()` through the shared binary BCMath helper.
pub(crate) fn lower_bcadd(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_scaled(ctx, inst, "bcadd", "__rt_bcadd")
}

/// Lowers `bcsub()` through the shared binary BCMath helper.
pub(crate) fn lower_bcsub(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_scaled(ctx, inst, "bcsub", "__rt_bcsub")
}

/// Lowers `bcmul()` through the shared binary BCMath helper.
pub(crate) fn lower_bcmul(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_scaled(ctx, inst, "bcmul", "__rt_bcmul")
}

/// Lowers `bcdiv()` through the shared binary BCMath helper.
pub(crate) fn lower_bcdiv(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_scaled(ctx, inst, "bcdiv", "__rt_bcdiv")
}

/// Lowers `bcmod()` through the shared binary BCMath helper.
pub(crate) fn lower_bcmod(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_scaled(ctx, inst, "bcmod", "__rt_bcmod")
}

/// Lowers `bcdivmod()` through the shared two-string-result helper.
pub(crate) fn lower_bcdivmod(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_scaled(ctx, inst, "bcdivmod", "__rt_bcdivmod")
}

/// Lowers `bcpow()` through the shared binary-shaped BCMath helper.
pub(crate) fn lower_bcpow(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_scaled(ctx, inst, "bcpow", "__rt_bcpow")
}

/// Lowers `bccomp()` through the shared comparison helper.
pub(crate) fn lower_bccomp(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_scaled(ctx, inst, "bccomp", "__rt_bccomp")
}

/// Lowers `bcsqrt()` through the shared unary scaled helper.
pub(crate) fn lower_bcsqrt(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "bcsqrt", 1, 2)?;
    publish(ctx);
    load_string_arg_to_regs(ctx, inst, 0, "bcsqrt", string_ptr(ctx), string_len(ctx))?;
    push_string_result(ctx);
    load_optional_scale(ctx, inst.operands.get(1).copied(), 5, 6, 3, 4, "bcsqrt scale")?;
    pop_primary_string(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_bcsqrt");
    store_if_result(ctx, inst)
}

/// Lowers `bcceil()` through the shared unary integer-boundary helper.
pub(crate) fn lower_bcceil(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_string(ctx, inst, "bcceil", "__rt_bcceil")
}

/// Lowers `bcfloor()` through the shared unary integer-boundary helper.
pub(crate) fn lower_bcfloor(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_string(ctx, inst, "bcfloor", "__rt_bcfloor")
}

/// Lowers `bcpowmod()` while preserving all three source strings and nullable scale.
pub(crate) fn lower_bcpowmod(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "bcpowmod", 3, 4)?;
    publish(ctx);
    load_string_arg_to_regs(ctx, inst, 0, "bcpowmod", string_ptr(ctx), string_len(ctx))?;
    push_string_result(ctx);
    load_string_arg_to_regs(ctx, inst, 1, "bcpowmod", string_ptr(ctx), string_len(ctx))?;
    push_string_result(ctx);
    load_string_arg_to_regs(ctx, inst, 2, "bcpowmod", string_ptr(ctx), string_len(ctx))?;
    push_string_result(ctx);
    load_optional_scale(
        ctx,
        inst.operands.get(3).copied(),
        7,
        8,
        10,
        11,
        "bcpowmod scale",
    )?;
    pop_powmod_strings(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_bcpowmod");
    store_if_result(ctx, inst)
}

/// Lowers `bcround()` with default precision zero and default mode one.
pub(crate) fn lower_bcround(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "bcround", 1, 3)?;
    publish(ctx);
    load_string_arg_to_regs(ctx, inst, 0, "bcround", string_ptr(ctx), string_len(ctx))?;
    push_string_result(ctx);
    load_optional_int(ctx, inst.operands.get(1).copied(), 0, "bcround precision")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_optional_int(ctx, inst.operands.get(2).copied(), 1, "bcround mode")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x4, x0");                              // pass the rounding mode in the helper's fourth value register
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the rounding mode in the helper's secondary integer register
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    pop_primary_string(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_bcround");
    store_if_result(ctx, inst)
}

/// Lowers `bcscale()` into the getter or setter helper after nullable-scale discrimination.
pub(crate) fn lower_bcscale(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "bcscale", 0, 1)?;
    publish(ctx);
    load_optional_scale(
        ctx,
        inst.operands.first().copied(),
        5,
        6,
        8,
        9,
        "bcscale scale",
    )?;
    let getter = ctx.next_label("bcscale_get");
    let done = ctx.next_label("bcscale_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x6, {}", getter));           // null selects the process-scale getter
            ctx.emitter.instruction("mov x0, x5");                              // pass the explicit scale to the setter helper
            abi::emit_call_label(ctx.emitter, "__rt_bcscale_set");
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the getter after setting the scale
            ctx.emitter.label(&getter);
            abi::emit_call_label(ctx.emitter, "__rt_bcscale_get");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test r9, r9");                             // check whether null selected the process-scale getter
            ctx.emitter.instruction(&format!("jnz {}", getter));                // branch to the getter for omitted or null scale
            ctx.emitter.instruction("mov rax, r8");                             // pass the explicit scale to the setter helper
            abi::emit_call_label(ctx.emitter, "__rt_bcscale_set");
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the getter after setting the scale
            ctx.emitter.label(&getter);
            abi::emit_call_label(ctx.emitter, "__rt_bcscale_get");
        }
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers one two-string operation with an optional scale.
fn lower_binary_scaled(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    helper: &str,
) -> Result<()> {
    ensure_arg_count_between(inst, name, 2, 3)?;
    publish(ctx);
    load_string_arg_to_regs(ctx, inst, 0, name, string_ptr(ctx), string_len(ctx))?;
    push_string_result(ctx);
    load_string_arg_to_regs(ctx, inst, 1, name, string_ptr(ctx), string_len(ctx))?;
    push_string_result(ctx);
    load_optional_scale(ctx, inst.operands.get(2).copied(), 5, 6, 8, 9, name)?;
    pop_binary_strings(ctx);
    abi::emit_call_label(ctx.emitter, helper);
    store_if_result(ctx, inst)
}

/// Lowers one exact-arity unary string operation.
fn lower_unary_string(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    helper: &str,
) -> Result<()> {
    ensure_arg_count(inst, name, 1)?;
    publish(ctx);
    load_string_arg_to_regs(ctx, inst, 0, name, string_ptr(ctx), string_len(ctx))?;
    abi::emit_call_label(ctx.emitter, helper);
    store_if_result(ctx, inst)
}

/// Publishes every bridge entry before argument registers are materialized.
fn publish(ctx: &mut FunctionContext<'_>) {
    crate::codegen::bcmath::publish_elephc_bcmath_function_pointers(ctx.emitter);
}

/// Materializes a nullable optional scale into target-selected scale/null registers.
fn load_optional_scale(
    ctx: &mut FunctionContext<'_>,
    value: Option<ValueId>,
    aarch64_scale_reg: usize,
    aarch64_null_reg: usize,
    x86_scale_reg: usize,
    x86_null_reg: usize,
    context: &str,
) -> Result<()> {
    let Some(value) = value else {
        set_scale_regs(
            ctx,
            0,
            1,
            aarch64_scale_reg,
            aarch64_null_reg,
            x86_scale_reg,
            x86_null_reg,
        );
        return Ok(());
    };
    let ty = ctx.value_php_type(value)?.codegen_repr();
    if matches!(ty, PhpType::Void | PhpType::Never) {
        set_scale_regs(
            ctx,
            0,
            1,
            aarch64_scale_reg,
            aarch64_null_reg,
            x86_scale_reg,
            x86_null_reg,
        );
        return Ok(());
    }
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        return load_dynamic_optional_scale(
            ctx,
            value,
            aarch64_scale_reg,
            aarch64_null_reg,
            x86_scale_reg,
            x86_null_reg,
        );
    }
    if ty == PhpType::TaggedScalar {
        return load_tagged_optional_scale(
            ctx,
            value,
            aarch64_scale_reg,
            aarch64_null_reg,
            x86_scale_reg,
            x86_null_reg,
        );
    }
    load_as_int(ctx, value, context)?;
    move_scale_result(
        ctx,
        aarch64_scale_reg,
        aarch64_null_reg,
        x86_scale_reg,
        x86_null_reg,
    );
    Ok(())
}

/// Splits an inline nullable integer into an explicit scale and a null-selection flag.
fn load_tagged_optional_scale(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    aarch64_scale_reg: usize,
    aarch64_null_reg: usize,
    x86_scale_reg: usize,
    x86_null_reg: usize,
) -> Result<()> {
    let null_case = ctx.next_label("bcmath_tagged_scale_null");
    let done = ctx.next_label("bcmath_tagged_scale_done");
    ctx.load_value_to_result(value)?;
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &null_case);
    move_scale_result(
        ctx,
        aarch64_scale_reg,
        aarch64_null_reg,
        x86_scale_reg,
        x86_null_reg,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", done)),       // skip null materialization for an integer scale
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", done)),      // skip null materialization for an integer scale
    }
    ctx.emitter.label(&null_case);
    set_scale_regs(
        ctx,
        0,
        1,
        aarch64_scale_reg,
        aarch64_null_reg,
        x86_scale_reg,
        x86_null_reg,
    );
    ctx.emitter.label(&done);
    Ok(())
}

/// Discriminates a boxed null from a boxed coercible integer without losing the source cell.
fn load_dynamic_optional_scale(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    aarch64_scale_reg: usize,
    aarch64_null_reg: usize,
    x86_scale_reg: usize,
    x86_null_reg: usize,
) -> Result<()> {
    let null_case = ctx.next_label("bcmath_scale_null");
    let done = ctx.next_label("bcmath_scale_done");
    crate::codegen::lower_inst::load_value_to_first_int_arg(ctx, value)?;
    let source_reg = abi::runtime_helper_int_arg_reg(ctx.emitter, 0);
    abi::emit_push_reg(ctx.emitter, source_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // mixed tag 8 identifies a dynamic null scale
            ctx.emitter.instruction(&format!("b.eq {}", null_case));            // use process scale for dynamic null
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // mixed tag 8 identifies a dynamic null scale
            ctx.emitter.instruction(&format!("je {}", null_case));              // use process scale for dynamic null
        }
    }
    abi::emit_pop_reg(ctx.emitter, source_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
    move_scale_result(
        ctx,
        aarch64_scale_reg,
        aarch64_null_reg,
        x86_scale_reg,
        x86_null_reg,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", done)),       // skip the null-scale materialization
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", done)),      // skip the null-scale materialization
    }
    ctx.emitter.label(&null_case);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    set_scale_regs(
        ctx,
        0,
        1,
        aarch64_scale_reg,
        aarch64_null_reg,
        x86_scale_reg,
        x86_null_reg,
    );
    ctx.emitter.label(&done);
    Ok(())
}

/// Copies an integer result into the selected scale register and clears its null flag.
fn move_scale_result(
    ctx: &mut FunctionContext<'_>,
    aarch64_scale_reg: usize,
    aarch64_null_reg: usize,
    x86_scale_reg: usize,
    x86_null_reg: usize,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x{}, x0", aarch64_scale_reg));// preserve the explicit scale for the BCMath helper
            ctx.emitter.instruction(&format!("mov x{}, xzr", aarch64_null_reg));// mark the explicit scale as non-null
        }
        Arch::X86_64 => {
            let scale_reg = x86_reg(x86_scale_reg);
            let null_reg = x86_reg(x86_null_reg);
            ctx.emitter.instruction(&format!("mov {}, rax", scale_reg));        // preserve the explicit scale for the BCMath helper
            ctx.emitter.instruction(&format!("xor {}, {}", null_reg, null_reg));// mark the explicit scale as non-null
        }
    }
}

/// Loads immediate scale/null values into target-selected registers.
fn set_scale_regs(
    ctx: &mut FunctionContext<'_>,
    scale: i64,
    null: i64,
    aarch64_scale_reg: usize,
    aarch64_null_reg: usize,
    x86_scale_reg: usize,
    x86_null_reg: usize,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, &format!("x{}", aarch64_scale_reg), scale);
            abi::emit_load_int_immediate(ctx.emitter, &format!("x{}", aarch64_null_reg), null);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, x86_reg(x86_scale_reg), scale);
            abi::emit_load_int_immediate(ctx.emitter, x86_reg(x86_null_reg), null);
        }
    }
}

/// Maps the compact x86 helper-register selector onto an architectural register name.
fn x86_reg(selector: usize) -> &'static str {
    match selector {
        3 => "rdi",
        4 => "rsi",
        8 => "r8",
        9 => "r9",
        10 => "r10",
        11 => "r11",
        _ => unreachable!("unsupported BCMath x86 register selector"),
    }
}

/// Materializes an optional integer argument, substituting one default immediate.
fn load_optional_int(
    ctx: &mut FunctionContext<'_>,
    value: Option<ValueId>,
    default: i64,
    context: &str,
) -> Result<()> {
    match value {
        Some(value) if !matches!(ctx.value_php_type(value)?, PhpType::Void | PhpType::Never) => {
            load_as_int(ctx, value, context)
        }
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), default);
            Ok(())
        }
    }
}

/// Returns the current target's canonical PHP string pointer result register.
fn string_ptr(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rax",
    }
}

/// Returns the current target's canonical PHP string length result register.
fn string_len(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x2",
        Arch::X86_64 => "rdx",
    }
}

/// Pushes the canonical string result pair without changing its order.
fn push_string_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_reg_pair(ctx.emitter, string_ptr(ctx), string_len(ctx));
}

/// Restores one staged string into the primary helper registers.
fn pop_primary_string(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2"),
        Arch::X86_64 => abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx"),
    }
}

/// Restores two staged strings into the binary-helper register contract.
fn pop_binary_strings(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg_pair(ctx.emitter, "x3", "x4");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
}

/// Restores three staged strings into the modular-power helper register contract.
fn pop_powmod_strings(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg_pair(ctx.emitter, "x5", "x6");
            abi::emit_pop_reg_pair(ctx.emitter, "x3", "x4");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg_pair(ctx.emitter, "r8", "r9");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
}
