use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
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

    match emitter.target.arch {
        Arch::AArch64 => {
            // -- evaluate all 6 arguments: hour, min, sec, month, day, year --
            // Push them on stack in reverse order so they come off in order
            for i in (0..6).rev() {
                emit_expr(&args[i], emitter, ctx, data);
                emitter.instruction("str x0, [sp, #-16]!");                     // push the evaluated integer argument onto the temporary stack
            }

            // -- pop args into registers: x0=hour, x1=min, x2=sec, x3=month, x4=day, x5=year --
            emitter.instruction("ldr x0, [sp], #16");                           // restore the hour argument into the first integer argument register
            emitter.instruction("ldr x1, [sp], #16");                           // restore the minute argument into the second integer argument register
            emitter.instruction("ldr x2, [sp], #16");                           // restore the second argument into the third integer argument register
            emitter.instruction("ldr x3, [sp], #16");                           // restore the month argument into the fourth integer argument register
            emitter.instruction("ldr x4, [sp], #16");                           // restore the day argument into the fifth integer argument register
            emitter.instruction("ldr x5, [sp], #16");                           // restore the year argument into the sixth integer argument register
        }
        Arch::X86_64 => {
            // -- evaluate all 6 arguments: hour, min, sec, month, day, year --
            // Push them on stack in reverse order so they come off in order
            for i in (0..6).rev() {
                emit_expr(&args[i], emitter, ctx, data);
                abi::emit_push_reg(emitter, "rax");                             // push the evaluated integer argument onto the temporary x86_64 stack slot
            }

            // -- pop args into SysV integer registers: rdi=hour, rsi=min, rdx=sec, rcx=month, r8=day, r9=year --
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the hour argument into the first SysV integer argument register
            abi::emit_pop_reg(emitter, "rsi");                                  // restore the minute argument into the second SysV integer argument register
            abi::emit_pop_reg(emitter, "rdx");                                  // restore the second argument into the third SysV integer argument register
            abi::emit_pop_reg(emitter, "rcx");                                  // restore the month argument into the fourth SysV integer argument register
            abi::emit_pop_reg(emitter, "r8");                                   // restore the day argument into the fifth SysV integer argument register
            abi::emit_pop_reg(emitter, "r9");                                   // restore the year argument into the sixth SysV integer argument register
        }
    }

    // -- call runtime to build struct tm and call mktime --
    abi::emit_call_label(emitter, "__rt_mktime");                               // build a libc struct tm and return the resulting Unix timestamp through the active target ABI

    Some(PhpType::Int)
}
