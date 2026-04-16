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
    emitter.comment("intdiv()");
    let zero_label = ctx.next_label("intdiv_zero");
    let done_label = ctx.next_label("intdiv_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- integer division: dividend / divisor --
            emit_expr(&args[0], emitter, ctx, data);
            abi::emit_push_reg(emitter, "x0");                                  // preserve the dividend while evaluating the divisor expression
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_pop_reg(emitter, "x1");                                   // restore the dividend into the left-hand division register

            // -- division by zero guard --
            emitter.instruction(&format!("cbz x0, {zero_label}"));              // if the divisor is 0, branch to the fatal error path
            emitter.instruction("sdiv x0, x1, x0");                             // divide the saved dividend by the current divisor
            emitter.instruction(&format!("b {done_label}"));                    // skip the fatal error path after a successful division
        }
        Arch::X86_64 => {
            // -- integer division: dividend / divisor --
            emit_expr(&args[0], emitter, ctx, data);
            abi::emit_push_reg(emitter, "rax");                                 // preserve the dividend while evaluating the divisor expression
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_pop_reg(emitter, "r11");                                  // restore the dividend into a scratch register before idiv clobbers rax/rdx

            // -- division by zero guard --
            emitter.instruction("test rax, rax");                               // check whether the divisor expression evaluated to zero
            emitter.instruction(&format!("je {}", zero_label));                 // branch to the fatal error path when the divisor is zero
            emitter.instruction("mov r10, rax");                                // preserve the divisor because idiv requires the dividend in rax
            emitter.instruction("mov rax, r11");                                // move the saved dividend into the mandatory idiv accumulator register
            emitter.instruction("cqo");                                         // sign-extend the dividend into rdx:rax for signed division
            emitter.instruction("idiv r10");                                    // divide the dividend by the preserved divisor and leave the quotient in rax
            emitter.instruction(&format!("jmp {}", done_label));                // skip the fatal error path after a successful division
        }
    }

    // -- fatal error: division by zero --
    emitter.label(&zero_label);
    let (err_label, err_len) = data.add_string(b"Fatal error: division by zero\n");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // fd = stderr
            emitter.adrp("x1", &err_label);                                     // load the page that contains the fatal error string
            emitter.add_lo12("x1", "x1", &err_label);                           // resolve the fatal error string address within that page
            emitter.instruction(&format!("mov x2, #{}", err_len));              // pass the fatal error string length to write()
            emitter.syscall(4);
            emitter.instruction("mov x0, #1");                                  // exit code 1
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea rsi, [rip + {}]", err_label));    // point the Linux write() buffer register at the fatal error string
            emitter.instruction(&format!("mov edx, {}", err_len));              // pass the fatal error string length to write()
            emitter.instruction("mov edi, 2");                                  // fd = stderr
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal division-by-zero message before terminating
            emitter.instruction("mov edi, 1");                                  // exit code 1
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate the process after reporting division by zero
        }
    }

    emitter.label(&done_label);
    Some(PhpType::Int)
}
