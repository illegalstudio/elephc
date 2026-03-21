use super::abi;
use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::PhpType;

/// Emits code to evaluate an expression.
/// Returns the type of the result.
/// - Strings: x1 = pointer, x2 = length
/// - Integers: x0 = value
pub fn emit_expr(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    match &expr.kind {
        ExprKind::StringLiteral(s) => {
            let bytes = s.as_bytes();
            let (label, len) = data.add_string(bytes);
            emitter.comment(&format!("load string \"{}\"", s.escape_default()));
            emitter.instruction(&format!("adrp x1, {}@PAGE", label));
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));
            emitter.instruction(&format!("mov x2, #{}", len));
            PhpType::Str
        }
        ExprKind::IntLiteral(n) => {
            emitter.comment(&format!("load int {}", n));
            load_immediate(emitter, "x0", *n);
            PhpType::Int
        }
        ExprKind::Variable(name) => {
            let var = ctx.variables.get(name).expect("undefined variable");
            let offset = var.stack_offset;
            let ty = var.ty.clone();
            emitter.comment(&format!("load ${}", name));
            abi::emit_load(emitter, &ty, offset);
            ty
        }
        ExprKind::Negate(inner) => {
            emit_expr(inner, emitter, ctx, data);
            emitter.comment("negate");
            emitter.instruction("neg x0, x0");
            PhpType::Int
        }
        ExprKind::BinaryOp { left, op, right } => emit_binop(left, op, right, emitter, ctx, data),
    }
}

fn emit_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
            emit_expr(left, emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");
            emit_expr(right, emitter, ctx, data);
            emitter.instruction("ldr x1, [sp], #16");
            // x1 = left, x0 = right
            let instr = match op {
                BinOp::Add => "add x0, x1, x0",
                BinOp::Sub => "sub x0, x1, x0",
                BinOp::Mul => "mul x0, x1, x0",
                BinOp::Div => "sdiv x0, x1, x0",
                _ => unreachable!(),
            };
            emitter.instruction(instr);
            PhpType::Int
        }
        BinOp::Concat => {
            // Evaluate left → x1 (ptr), x2 (len)
            emit_expr(left, emitter, ctx, data);
            // Save left on stack
            emitter.instruction("stp x1, x2, [sp, #-16]!");
            // Evaluate right → x1 (ptr), x2 (len)
            emit_expr(right, emitter, ctx, data);
            // Move right to x3, x4
            emitter.instruction("mov x3, x1");
            emitter.instruction("mov x4, x2");
            // Restore left to x1, x2
            emitter.instruction("ldp x1, x2, [sp], #16");
            // __rt_concat(x1=left_ptr, x2=left_len, x3=right_ptr, x4=right_len)
            // returns x1=result_ptr, x2=result_len
            emitter.instruction("bl __rt_concat");
            PhpType::Str
        }
    }
}

fn load_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    if value >= 0 && value <= 65535 {
        emitter.instruction(&format!("mov {}, #{}", reg, value));
    } else if value < 0 && value >= -65536 {
        emitter.instruction(&format!("mov {}, #{}", reg, value));
    } else {
        let uval = value as u64;
        emitter.instruction(&format!("movz {}, #0x{:x}", reg, uval & 0xFFFF));
        if (uval >> 16) & 0xFFFF != 0 {
            emitter.instruction(&format!(
                "movk {}, #0x{:x}, lsl #16",
                reg,
                (uval >> 16) & 0xFFFF
            ));
        }
        if (uval >> 32) & 0xFFFF != 0 {
            emitter.instruction(&format!(
                "movk {}, #0x{:x}, lsl #32",
                reg,
                (uval >> 32) & 0xFFFF
            ));
        }
        if (uval >> 48) & 0xFFFF != 0 {
            emitter.instruction(&format!(
                "movk {}, #0x{:x}, lsl #48",
                reg,
                (uval >> 48) & 0xFFFF
            ));
        }
    }
}
