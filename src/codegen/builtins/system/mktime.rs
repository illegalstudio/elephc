use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("mktime()");

    // -- evaluate all 6 arguments: hour, min, sec, month, day, year --
    // Push them on stack in reverse order so they come off in order
    for i in (0..6).rev() {
        emit_expr(&args[i], emitter, ctx, data);
        emitter.instruction("str x0, [sp, #-16]!");                             // push argument onto stack
    }

    // -- pop args into registers: x0=hour, x1=min, x2=sec, x3=month, x4=day, x5=year --
    emitter.instruction("ldr x0, [sp], #16");                                   // pop hour
    emitter.instruction("ldr x1, [sp], #16");                                   // pop minute
    emitter.instruction("ldr x2, [sp], #16");                                   // pop second
    emitter.instruction("ldr x3, [sp], #16");                                   // pop month
    emitter.instruction("ldr x4, [sp], #16");                                   // pop day
    emitter.instruction("ldr x5, [sp], #16");                                   // pop year

    // -- call runtime to build struct tm and call mktime --
    emitter.instruction("bl __rt_mktime");                                      // mktime(h,m,s,mon,day,yr) → x0=timestamp

    Some(PhpType::Int)
}
