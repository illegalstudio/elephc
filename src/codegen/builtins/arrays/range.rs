use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("range()");
    if emitter.target.arch == Arch::X86_64 {
        emit_expr(&args[0], emitter, ctx, data);
        abi::emit_push_reg(emitter, "rax");                                     // preserve the range start value while evaluating the range end value expression
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov rsi, rax");                                    // place the inclusive range end value in the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the inclusive range start value into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_range");                            // build the integer range array through the x86_64 runtime helper
        return Some(PhpType::Array(Box::new(PhpType::Int)));
    }

    emit_expr(&args[0], emitter, ctx, data);
    // -- save start value, evaluate end value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push start value onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to create array from start to end --
    emitter.instruction("mov x1, x0");                                          // move end value to x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop start value into x0 (first arg)
    emitter.instruction("bl __rt_range");                                       // call runtime: create range → x0=new array

    Some(PhpType::Array(Box::new(PhpType::Int)))
}
