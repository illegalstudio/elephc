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
    emitter.comment("array_fill()");
    if emitter.target.arch == Arch::X86_64 {
        return emit_array_fill_linux_x86_64(args, emitter, ctx, data);
    }

    emit_expr(&args[0], emitter, ctx, data);
    // -- save start index, evaluate count --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push start index onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- save count, evaluate fill value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push count onto stack
    let value_ty = emit_expr(&args[2], emitter, ctx, data);
    let uses_refcounted_runtime = value_ty.is_refcounted();
    // -- set up three-arg call: start, count, value --
    emitter.instruction("mov x2, x0");                                          // move fill value to x2 (third arg)
    emitter.instruction("ldr x1, [sp], #16");                                   // pop count into x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop start index into x0 (first arg)
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_fill_refcounted"
    } else {
        "bl __rt_array_fill"
    };
    emitter.instruction(runtime_call);                                          // call runtime: fill array → x0=new array

    Some(PhpType::Array(Box::new(value_ty)))
}

fn emit_array_fill_linux_x86_64(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, "rax");                                         // preserve the start index while evaluating the count and fill value arguments
    emit_expr(&args[1], emitter, ctx, data);
    abi::emit_push_reg(emitter, "rax");                                         // preserve the count while evaluating the fill value argument
    let value_ty = emit_expr(&args[2], emitter, ctx, data);
    if matches!(value_ty, PhpType::Float) {
        emitter.instruction("movq rdx, xmm0");                                  // move the floating-point fill payload bits into the third x86_64 runtime argument register
    } else {
        emitter.instruction("mov rdx, rax");                                    // place the fill payload in the third x86_64 runtime argument register
    }
    abi::emit_pop_reg(emitter, "rsi");                                          // restore the requested count into the second x86_64 runtime argument register
    abi::emit_pop_reg(emitter, "rdi");                                          // restore the start index into the first x86_64 runtime argument register
    if value_ty.is_refcounted() {
        abi::emit_call_label(emitter, "__rt_array_fill_refcounted");            // build an indexed array by repeatedly retaining the borrowed heap payload
    } else {
        abi::emit_call_label(emitter, "__rt_array_fill");                       // build a scalar indexed array through the plain fill runtime helper
    }

    Some(PhpType::Array(Box::new(value_ty)))
}
