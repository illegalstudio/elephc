//! Purpose:
//! Compares proven integer/float PHP operands without allocating temporary boxes.
//!
//! Called from:
//! - `comparisons::lower_php_rel_cmp` after the closed numeric origin proof.
//!
//! Key details:
//! - Integer pairs compare exactly, including values beyond double precision.
//! - Mixed float comparisons convert signed integers and reject every unordered NaN predicate.

use crate::codegen::{platform::Arch, Result};
use crate::codegen::context::FunctionContext;
use crate::ir::{CmpPredicate, Instruction};
use super::{expect_cmp_predicate, expect_operand, numeric_payloads, store_if_result};

/// Dispatches on numeric tags while preserving exact signed integer ordering.
pub(super) fn lower(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let (left, right, left_tag, right_tag) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x10", "x11", "x9"),
        Arch::X86_64 => ("rax", "r10", "r11", "r9"),
    };
    ctx.load_value_to_reg(lhs, left)?;
    ctx.load_value_to_reg(rhs, right)?;
    let left_type = numeric_payloads::payload(ctx, lhs, left, left_tag)?;
    let right_type = numeric_payloads::payload(ctx, rhs, right, right_tag)?;
    let integer = ctx.next_label("numeric_compare_integer");
    let done = ctx.next_label("numeric_compare_done");
    let predicate = expect_cmp_predicate(inst)?;
    // -- compare two integer tags exactly before considering floating-point coercion --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("orr x12, x11, x9");                        // detect whether either runtime operand is a float
            ctx.emitter.instruction(&format!("cbz x12, {integer}"));            // keep signed integer precision when both tags are zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r8, r11");                             // retain individual tags for signed float conversion
            ctx.emitter.instruction("or r8, r9");                               // detect whether either runtime operand is a float
            ctx.emitter.instruction(&format!("jz {integer}"));                  // keep signed integer precision when both tags are zero
        }
    }
    let (left_float, right_float) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("d0", "d1"),
        Arch::X86_64 => ("xmm0", "xmm1"),
    };
    numeric_payloads::float(ctx, &left_type, left, left_tag, left_float);
    numeric_payloads::float(ctx, &right_type, right, right_tag, right_float);
    let (integer_condition, float_condition) = conditions(predicate);
    // -- unordered float operands make every relational predicate false in PHP --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d0, d1");                             // compare promoted numeric operands using IEEE double semantics
            ctx.emitter.instruction(&format!("cset x0, {integer_condition}"));  // materialize the requested ordered predicate
            ctx.emitter.instruction("cset x10, vc");                            // reject unordered comparisons involving NaN
            ctx.emitter.instruction("and x0, x0, x10");                         // publish false whenever either operand was NaN
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare promoted numeric operands using IEEE double semantics
            ctx.emitter.instruction(&format!("set{float_condition} al"));       // materialize the requested ordered predicate
            ctx.emitter.instruction("setnp r10b");                              // reject unordered comparisons involving NaN
            ctx.emitter.instruction("and al, r10b");                            // publish false whenever either operand was NaN
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean result without preserving stale high bits
        }
    }
    ctx.emit_branch(&done);
    ctx.emitter.label(&integer);
    // -- signed integer pairs do not round through double precision --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, x10");                             // compare both original signed integer payloads exactly
            ctx.emitter.instruction(&format!("cset x0, {integer_condition}"));  // materialize the requested signed integer predicate
        }
        Arch::X86_64 => {
            let signed = match predicate { CmpPredicate::Slt => "l", CmpPredicate::Sle => "le", CmpPredicate::Sgt => "g", CmpPredicate::Sge => "ge", _ => unreachable!() };
            ctx.emitter.instruction("cmp rax, r10");                            // compare both original signed integer payloads exactly
            ctx.emitter.instruction(&format!("set{signed} al"));                // materialize the requested signed integer predicate
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean result without preserving stale high bits
        }
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Maps the four admitted predicates to architecture-specific ordered condition codes.
fn conditions(predicate: CmpPredicate) -> (&'static str, &'static str) {
    match predicate {
        CmpPredicate::Slt => ("lt", "b"),
        CmpPredicate::Sle => ("le", "be"),
        CmpPredicate::Sgt => ("gt", "a"),
        CmpPredicate::Sge => ("ge", "ae"),
        _ => unreachable!("the numeric comparison proof accepts ordered relational predicates only"),
    }
}
