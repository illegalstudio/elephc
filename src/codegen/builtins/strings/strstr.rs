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
    emitter.comment("strstr()");
    emit_expr(&args[0], emitter, ctx, data);
    let found = ctx.next_label("strstr_found");
    let end = ctx.next_label("strstr_end");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the haystack pointer and length while evaluating the needle string
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the needle pointer into the third string-helper argument register
            emitter.instruction("mov x4, x2");                                  // move the needle length into the fourth string-helper argument register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the haystack pointer and length after evaluating the needle
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the haystack again so strstr() can rebuild the matching suffix after strpos()
            abi::emit_call_label(emitter, "__rt_strpos");                       // find the first match position inside the haystack through the shared runtime helper
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the haystack pointer and length after the strpos() helper returns the match index
            emitter.instruction("cmp x0, #0");                                  // check whether strpos() returned a valid match position
            emitter.instruction(&format!("b.ge {}", found));                    // branch to the matching-suffix path when the needle was found
            emitter.instruction("mov x2, #0");                                  // return an empty-string length when strstr() does not find the needle
            emitter.instruction(&format!("b {}", end));                         // skip the suffix-construction path when strstr() does not find the needle
            emitter.label(&found);
            emitter.instruction("add x1, x1, x0");                              // advance the haystack pointer to the start of the matching suffix
            emitter.instruction("sub x2, x2, x0");                              // shrink the haystack length down to the matching suffix length
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the haystack pointer and length while evaluating the needle string
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov r8, rax");                                 // preserve the needle pointer while restoring the haystack pointer and length
            emitter.instruction("mov r9, rdx");                                 // preserve the needle length while restoring the haystack pointer and length
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the haystack pointer and length into the standard string result registers
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // push the haystack again so strstr() can rebuild the matching suffix after strpos()
            emitter.instruction("mov rdi, rax");                                // move the haystack pointer into the first SysV string-helper argument register
            emitter.instruction("mov rsi, rdx");                                // move the haystack length into the second SysV string-helper argument register
            emitter.instruction("mov rdx, r8");                                 // move the preserved needle pointer into the third SysV string-helper argument register
            emitter.instruction("mov rcx, r9");                                 // move the preserved needle length into the fourth SysV string-helper argument register
            abi::emit_call_label(emitter, "__rt_strpos");                       // find the first match position inside the haystack through the shared runtime helper
            emitter.instruction("mov r8, rax");                                 // preserve the signed strpos() result across restoring the saved haystack pointer and length
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the haystack pointer and length after the strpos() helper returns the match index
            emitter.instruction("cmp r8, 0");                                   // check whether strpos() returned a valid match position
            emitter.instruction(&format!("jge {}", found));                     // branch to the matching-suffix path when the needle was found
            emitter.instruction("xor eax, eax");                                // return an empty-string pointer when strstr() does not find the needle
            emitter.instruction("xor edx, edx");                                // return an empty-string length when strstr() does not find the needle
            emitter.instruction(&format!("jmp {}", end));                       // skip the suffix-construction path when strstr() does not find the needle
            emitter.label(&found);
            emitter.instruction("add rax, r8");                                 // advance the haystack pointer to the start of the matching suffix
            emitter.instruction("sub rdx, r8");                                 // shrink the haystack length down to the matching suffix length
        }
    }
    emitter.label(&end);

    Some(PhpType::Str)
}
