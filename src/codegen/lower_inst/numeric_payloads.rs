//! Purpose:
//! Decodes proven integer/float operands without allocating temporary Mixed cells.
//!
//! Called from:
//! - Numeric frame arithmetic and numeric PHP comparison lowering.
//!
//! Key details:
//! - Callers must prove every possible runtime tag is integer or float.
//! - Load both operand homes before decoding because large frame addresses use scratch registers.

use crate::codegen::{abi, platform::Arch, Result};
use crate::codegen::context::FunctionContext;
use crate::ir::ValueId;
use crate::types::PhpType;

/// Separates a proven numeric tag and payload without converting a float back to integer.
pub(super) fn payload(ctx: &mut FunctionContext<'_>, value: ValueId, payload: &str, tag: &str) -> Result<PhpType> {
    let ty = ctx.value_php_type(value)?.codegen_repr();
    match ty {
        PhpType::Int => abi::emit_load_int_immediate(ctx.emitter, tag, 0),
        PhpType::Float => abi::emit_load_int_immediate(ctx.emitter, tag, 2),
        PhpType::Mixed => {
            abi::emit_load_from_address(ctx.emitter, tag, payload, 0);
            abi::emit_load_from_address(ctx.emitter, payload, payload, 8);
        }
        _ => unreachable!("the frame-cell proof rejects nonnumeric input types"),
    }
    Ok(ty)
}

/// Converts an integer with signed semantics or transfers an existing double's exact bits.
pub(super) fn float(ctx: &mut FunctionContext<'_>, ty: &PhpType, payload: &str, tag: &str, dest: &str) {
    let float = ctx.next_label("stack_numeric_float_payload");
    let done = ctx.next_label("stack_numeric_float_ready");
    if *ty == PhpType::Mixed {
        // -- distinguish an integer payload from existing IEEE double bits --
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction(&format!("cbnz {tag}, {float}")), // a nonzero numeric tag already contains a PHP double
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("test {tag}, {tag}"));         // distinguish integer payload bits from a PHP double
                ctx.emitter.instruction(&format!("jnz {float}"));               // preserve existing doubles without another integer conversion
            }
        }
    }
    if *ty != PhpType::Float {
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction(&format!("scvtf {dest}, {payload}")), // convert the original signed integer into a double
            Arch::X86_64 => ctx.emitter.instruction(&format!("cvtsi2sd {dest}, {payload}")), // convert the original signed integer into a double
        }
        if *ty == PhpType::Mixed { ctx.emit_branch(&done); }
    }
    if *ty != PhpType::Int {
        ctx.emitter.label(&float);
        abi::emit_reg_move(ctx.emitter, dest, payload);
    }
    ctx.emitter.label(&done);
}
