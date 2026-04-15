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
    emitter.comment("ctype_digit()");
    emit_expr(&args[0], emitter, ctx, data);
    let loop_label = ctx.next_label("ctype_loop");
    let fail_label = ctx.next_label("ctype_fail");
    let pass_label = ctx.next_label("ctype_pass");
    let end_label = ctx.next_label("ctype_end");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x2, {}", fail_label));                    // empty strings fail the ctype_digit() contract
            emitter.instruction("mov x3, #0");                                          // x3 = current byte index inside the checked string
            emitter.label(&loop_label);
            emitter.instruction("cmp x3, x2");                                          // check whether the loop index reached the string length
            emitter.instruction(&format!("b.ge {}", pass_label));                       // all bytes matched the digit predicate, so the string passes
            emitter.instruction("ldrb w4, [x1, x3]");                                   // load the current byte from the string payload
            emitter.instruction("sub w5, w4, #48");                                     // normalize the byte against '0' for the decimal-digit range check
            emitter.instruction("cmp w5, #9");                                          // test whether the normalized byte falls inside 0-9
            emitter.instruction(&format!("b.hi {}", fail_label));                       // fail immediately when the byte is outside the decimal-digit range
            emitter.instruction("add x3, x3, #1");                                      // advance the byte index after accepting the current digit
            emitter.instruction(&format!("b {}", loop_label));                          // continue scanning until a byte fails or the string ends
            emitter.label(&fail_label);
            emitter.instruction("mov x0, #0");                                          // return false once an empty string or non-digit byte is observed
            emitter.instruction(&format!("b {}", end_label));                           // skip the success materialization after setting the false result
            emitter.label(&pass_label);
            emitter.instruction("mov x0, #1");                                          // return true once every byte in the string satisfied the digit predicate
        }
        Arch::X86_64 => {
            emitter.instruction("test rdx, rdx");                                       // empty strings fail the ctype_digit() contract
            emitter.instruction(&format!("je {}", fail_label));                         // jump to the false result when the checked string is empty
            emitter.instruction("xor rcx, rcx");                                        // rcx = current byte index inside the checked string
            emitter.label(&loop_label);
            emitter.instruction("cmp rcx, rdx");                                        // check whether the loop index reached the string length
            emitter.instruction(&format!("jge {}", pass_label));                        // all bytes matched the digit predicate, so the string passes
            emitter.instruction("movzx r8d, BYTE PTR [rax + rcx]");                     // load the current byte from the string payload into a zero-extended scratch register
            emitter.instruction("sub r8d, 48");                                         // normalize the byte against '0' for the decimal-digit range check
            emitter.instruction("cmp r8d, 9");                                          // test whether the normalized byte falls inside 0-9
            emitter.instruction(&format!("ja {}", fail_label));                         // fail immediately when the byte is outside the decimal-digit range
            emitter.instruction("add rcx, 1");                                          // advance the byte index after accepting the current digit
            emitter.instruction(&format!("jmp {}", loop_label));                        // continue scanning until a byte fails or the string ends
            emitter.label(&fail_label);
            emitter.instruction("mov rax, 0");                                          // return false once an empty string or non-digit byte is observed
            emitter.instruction(&format!("jmp {}", end_label));                         // skip the success materialization after setting the false result
            emitter.label(&pass_label);
            emitter.instruction("mov rax, 1");                                          // return true once every byte in the string satisfied the digit predicate
        }
    }
    emitter.label(&end_label);
    Some(PhpType::Bool)
}
