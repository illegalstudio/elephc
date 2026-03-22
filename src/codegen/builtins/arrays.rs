use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "count" => {
            emitter.comment("count()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("ldr x0, [x0]");
            Some(PhpType::Int)
        }
        // @todo: add support for array_push() with floats, booleans and other types
        "array_push" => {
            emitter.comment("array_push()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");
            let val_ty = emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("ldr x9, [sp], #16");
            match &val_ty {
                PhpType::Int => {
                    emitter.instruction("mov x1, x0");
                    emitter.instruction("mov x0, x9");
                    emitter.instruction("bl __rt_array_push_int");
                }
                PhpType::Str => {
                    emitter.instruction("mov x0, x9");
                    emitter.instruction("bl __rt_array_push_str");
                }
                _ => {}
            }
            Some(PhpType::Void)
        }
        "array_pop" => {
            emitter.comment("array_pop()");
            let arr_ty = emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("ldr x9, [x0]");
            emitter.instruction("sub x9, x9, #1");
            emitter.instruction("str x9, [x0]");
            let elem_ty = match &arr_ty {
                PhpType::Array(t) => *t.clone(),
                _ => PhpType::Int,
            };
            match &elem_ty {
                PhpType::Int => {
                    emitter.instruction("add x0, x0, #24");
                    emitter.instruction("ldr x0, [x0, x9, lsl #3]");
                }
                PhpType::Str => {
                    emitter.instruction("lsl x10, x9, #4");
                    emitter.instruction("add x0, x0, x10");
                    emitter.instruction("add x0, x0, #24");
                    emitter.instruction("ldr x1, [x0]");
                    emitter.instruction("ldr x2, [x0, #8]");
                }
                _ => {}
            }
            Some(elem_ty)
        }
        "in_array" => {
            emitter.comment("in_array()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            let found_label = ctx.next_label("in_array_found");
            let end_label = ctx.next_label("in_array_end");
            let done_label = ctx.next_label("in_array_done");
            emitter.instruction("ldr x9, [x0]");
            emitter.instruction("add x10, x0, #24");
            emitter.instruction("ldr x11, [sp], #16");
            emitter.instruction("mov x12, #0");
            let loop_label = ctx.next_label("in_array_loop");
            emitter.label(&loop_label);
            emitter.instruction("cmp x12, x9");
            emitter.instruction(&format!("b.ge {}", end_label));
            emitter.instruction("ldr x13, [x10, x12, lsl #3]");
            emitter.instruction("cmp x13, x11");
            emitter.instruction(&format!("b.eq {}", found_label));
            emitter.instruction("add x12, x12, #1");
            emitter.instruction(&format!("b {}", loop_label));
            emitter.label(&found_label);
            emitter.instruction("mov x0, #1");
            emitter.instruction(&format!("b {}", done_label));
            emitter.label(&end_label);
            emitter.instruction("mov x0, #0");
            emitter.label(&done_label);
            Some(PhpType::Int)
        }
        "array_keys" => {
            emitter.comment("array_keys()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("ldr x9, [x0]");
            emitter.instruction("str x9, [sp, #-16]!");
            emitter.instruction("mov x0, x9");
            emitter.instruction("mov x1, #8");
            emitter.instruction("bl __rt_array_new");
            emitter.instruction("str x0, [sp, #-16]!");
            emitter.instruction("str xzr, [sp, #-16]!");
            let loop_label = ctx.next_label("akeys_loop");
            let end_label = ctx.next_label("akeys_end");
            emitter.label(&loop_label);
            emitter.instruction("ldr x12, [sp]");
            emitter.instruction("ldr x9, [sp, #32]");
            emitter.instruction("cmp x12, x9");
            emitter.instruction(&format!("b.ge {}", end_label));
            emitter.instruction("ldr x0, [sp, #16]");
            emitter.instruction("mov x1, x12");
            emitter.instruction("bl __rt_array_push_int");
            emitter.instruction("ldr x12, [sp]");
            emitter.instruction("add x12, x12, #1");
            emitter.instruction("str x12, [sp]");
            emitter.instruction(&format!("b {}", loop_label));
            emitter.label(&end_label);
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ldr x0, [sp], #16");
            emitter.instruction("add sp, sp, #16");
            Some(PhpType::Array(Box::new(PhpType::Int)))
        }
        "array_values" => {
            emitter.comment("array_values()");
            emit_expr(&args[0], emitter, ctx, data);
            Some(PhpType::Array(Box::new(PhpType::Int)))
        }
        "sort" => {
            emitter.comment("sort()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_sort_int");
            Some(PhpType::Void)
        }
        "rsort" => {
            emitter.comment("rsort()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_rsort_int");
            Some(PhpType::Void)
        }
        "isset" => {
            emitter.comment("isset()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("mov x0, #1");
            Some(PhpType::Int)
        }
        _ => None,
    }
}
