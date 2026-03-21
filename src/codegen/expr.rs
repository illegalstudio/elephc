use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::checker::PhpType;

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
    match expr {
        Expr::StringLiteral(s) => {
            let bytes = s.as_bytes();
            let (label, len) = data.add_string(bytes);
            emitter.comment(&format!("load string \"{}\"", s.escape_default()));
            emitter.instruction(&format!("adrp x1, {}@PAGE", label));
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));
            emitter.instruction(&format!("mov x2, #{}", len));
            PhpType::Str
        }
        Expr::IntLiteral(n) => {
            emitter.comment(&format!("load int {}", n));
            load_immediate(emitter, "x0", *n);
            PhpType::Int
        }
        Expr::Variable(name) => {
            let var = ctx.variables.get(name).expect("undefined variable");
            let offset = var.stack_offset;
            let ty = var.ty.clone();
            match ty {
                PhpType::Int => {
                    emitter.comment(&format!("load ${}", name));
                    emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));
                }
                PhpType::Str => {
                    emitter.comment(&format!("load ${}", name));
                    emitter.instruction(&format!("ldur x1, [x29, #-{}]", offset));
                    emitter.instruction(&format!("ldur x2, [x29, #-{}]", offset - 8));
                }
            }
            ty
        }
        Expr::Negate(inner) => {
            emit_expr(inner, emitter, ctx, data);
            emitter.comment("negate");
            emitter.instruction("neg x0, x0");
            PhpType::Int
        }
        Expr::BinaryOp { .. } => {
            // TODO: implement in Phase 3
            PhpType::Int
        }
    }
}

fn load_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    if value >= 0 && value <= 65535 {
        emitter.instruction(&format!("mov {}, #{}", reg, value));
    } else if value < 0 && value >= -65536 {
        emitter.instruction(&format!("mov {}, #{}", reg, value));
    } else {
        // Use movz/movk for larger values
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
