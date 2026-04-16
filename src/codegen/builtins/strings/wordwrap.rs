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
    emitter.comment("wordwrap()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the input string while evaluating the width and optional break string
            if args.len() >= 2 {
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("mov x3, x0");                              // move the requested wrap width into the scalar runtime argument register
            } else {
                emitter.instruction("mov x3, #75");                             // default to the PHP wordwrap() width of 75 when omitted
            }
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("mov x4, x1");                              // move the break-string pointer into the secondary runtime string-argument pair
                emitter.instruction("mov x5, x2");                              // move the break-string length into the secondary runtime string-argument pair
            } else {
                let (label, len) = data.add_string(b"\n");
                abi::emit_symbol_address(emitter, "x4", &label);                // materialize the default newline break string when the third argument is omitted
                abi::emit_load_int_immediate(emitter, "x5", len as i64);        // materialize the default newline break-string length
            }
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the input string after evaluating the width and optional break string
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the input string while evaluating the width and optional break string
            if args.len() >= 2 {
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the requested wrap width into the scalar x86_64 runtime argument register
            } else {
                emitter.instruction("mov rdi, 75");                             // default to the PHP wordwrap() width of 75 when omitted
            }
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("mov rcx, rax");                            // move the break-string pointer into the secondary x86_64 runtime string-argument pair
                emitter.instruction("mov r8, rdx");                             // move the break-string length into the secondary x86_64 runtime string-argument pair
            } else {
                let (label, len) = data.add_string(b"\n");
                abi::emit_symbol_address(emitter, "rcx", &label);               // materialize the default newline break string when the third argument is omitted
                abi::emit_load_int_immediate(emitter, "r8", len as i64);        // materialize the default newline break-string length
            }
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the input string into the primary x86_64 string-helper input registers
        }
    }
    abi::emit_call_label(emitter, "__rt_wordwrap");                             // wrap the input string at word boundaries through the target-aware runtime helper
    Some(PhpType::Str)
}
