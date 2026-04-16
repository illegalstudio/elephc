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
    emitter.comment("str_pad()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the input string while evaluating the target length and optional pad arguments
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the requested target length while evaluating the optional pad string and pad type
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the requested pad string while evaluating the optional pad type
            } else {
                let (label, len) = data.add_string(b" ");
                abi::emit_symbol_address(emitter, "x1", &label);                // materialize the default single-space pad string when the third argument is omitted
                abi::emit_load_int_immediate(emitter, "x2", len as i64);        // materialize the default single-space pad-string length
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the synthesized default pad string while evaluating the optional pad type
            }
            if args.len() >= 4 {
                emit_expr(&args[3], emitter, ctx, data);
                emitter.instruction("mov x7, x0");                              // move the requested pad type into the extra AArch64 runtime argument register
            } else {
                emitter.instruction("mov x7, #1");                              // default to STR_PAD_RIGHT when the fourth argument is omitted
            }
            emitter.instruction("ldp x3, x4, [sp], #16");                       // restore the pad string into the secondary AArch64 string-helper argument pair
            emitter.instruction("ldr x5, [sp], #16");                           // restore the requested target length into the scalar runtime argument register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the input string into the primary AArch64 string-helper argument pair
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the input string while evaluating the target length and optional pad arguments
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_push_reg(emitter, "rax");                                 // preserve the requested target length while evaluating the optional pad string and pad type
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                abi::emit_push_reg_pair(emitter, "rax", "rdx");                 // preserve the requested pad string while evaluating the optional pad type
            } else {
                let (label, len) = data.add_string(b" ");
                abi::emit_symbol_address(emitter, "rax", &label);               // materialize the default single-space pad string when the third argument is omitted
                abi::emit_load_int_immediate(emitter, "rdx", len as i64);       // materialize the default single-space pad-string length
                abi::emit_push_reg_pair(emitter, "rax", "rdx");                 // preserve the synthesized default pad string while evaluating the optional pad type
            }
            if args.len() >= 4 {
                emit_expr(&args[3], emitter, ctx, data);
                emitter.instruction("mov r8, rax");                             // move the requested pad type into the extra x86_64 runtime argument register
            } else {
                emitter.instruction("mov r8, 1");                               // default to STR_PAD_RIGHT when the fourth argument is omitted
            }
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the pad string into the secondary x86_64 string-helper argument pair
            abi::emit_pop_reg(emitter, "rcx");                                  // restore the requested target length into the scalar runtime argument register
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the input string into the primary x86_64 string-helper input registers
        }
    }
    abi::emit_call_label(emitter, "__rt_str_pad");                              // pad the input string to the requested width through the target-aware runtime helper
    Some(PhpType::Str)
}
