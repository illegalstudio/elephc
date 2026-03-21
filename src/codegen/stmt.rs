use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use super::expr::emit_expr;
use crate::parser::ast::Stmt;
use crate::types::checker::PhpType;

pub fn emit_stmt(
    stmt: &Stmt,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match stmt {
        Stmt::Echo(expr) => {
            emitter.blank();
            emitter.comment("echo");
            let ty = emit_expr(expr, emitter, ctx, data);
            match ty {
                PhpType::Str => {
                    // x1 = ptr, x2 = len already set by emit_expr
                    emitter.instruction("mov x0, #1");
                    emitter.instruction("mov x16, #4");
                    emitter.instruction("svc #0x80");
                }
                PhpType::Int => {
                    // x0 = value, call itoa then write
                    emitter.instruction("bl __rt_itoa");
                    // itoa returns x1 = ptr, x2 = len
                    emitter.instruction("mov x0, #1");
                    emitter.instruction("mov x16, #4");
                    emitter.instruction("svc #0x80");
                }
            }
        }
        Stmt::Assign { name, value } => {
            emitter.blank();
            emitter.comment(&format!("${} = ...", name));
            let ty = emit_expr(value, emitter, ctx, data);

            // Allocate if not already allocated
            if !ctx.variables.contains_key(name) {
                ctx.alloc_var(name, ty.clone());
            }

            let var = ctx.variables.get(name).unwrap();
            let offset = var.stack_offset;

            match ty {
                PhpType::Int => {
                    emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));
                }
                PhpType::Str => {
                    emitter.instruction(&format!("stur x1, [x29, #-{}]", offset));
                    emitter.instruction(&format!("stur x2, [x29, #-{}]", offset - 8));
                }
            }
        }
    }
}
