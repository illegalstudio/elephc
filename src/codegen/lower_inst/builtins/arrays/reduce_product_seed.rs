//! Purpose:
//! Lowers the seeded raw integer `array_reduce()` product selected by the AST callback fusion.
//!
//! Called from:
//! - The typed ArrayReduceProductSeed and ArrayReduceProductFloatSeed dispatchers.
//!
//! Key details:
//! - The source is a packed `array<int>`; the initial value is a raw int or a raw double.
//! - `__rt_array_product_int_i64` multiplies with PHP's checked integer semantics and
//!   switches to double at the first overflow, boxing only the final result.
//! - `__rt_array_product_float_i64` starts in double mode from the raw IEEE-754 seed bits,
//!   so no element is boxed either way.

use super::*;

/// Lowers `array_reduce($ints, fn($c, $v) => $c * $v, C)` to the raw integer-seeded product.
pub(crate) fn lower_array_reduce_product_seed(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_reduce_product_seed", 2)?;
    let array = expect_operand(inst, 0)?;
    let seed = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_load_int_immediate(ctx.emitter, "x1", 0);
            ctx.emitter.instruction("ldr x2, [x0]");                            // x2 = exclusive end = array length
            ctx.load_value_to_reg(seed, "x3")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 0);
            ctx.emitter.instruction("mov rdx, QWORD PTR [rdi]");                // rdx = exclusive end = array length
            ctx.load_value_to_reg(seed, "rcx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_product_int_i64");
    store_if_result(ctx, inst)
}

/// Lowers a float-seeded `array_reduce()` product to the raw double reducer.
pub(crate) fn lower_array_reduce_product_float_seed(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_reduce_product_float_seed", 2)?;
    let array = expect_operand(inst, 0)?;
    let seed = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_load_int_immediate(ctx.emitter, "x1", 0);
            ctx.emitter.instruction("ldr x2, [x0]");                            // x2 = exclusive end = array length
            ctx.load_value_to_reg(seed, "x3")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 0);
            ctx.emitter.instruction("mov rdx, QWORD PTR [rdi]");                // rdx = exclusive end = array length
            ctx.load_value_to_reg(seed, "rcx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_product_float_i64");
    store_if_result(ctx, inst)
}
