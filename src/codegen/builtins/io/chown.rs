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
    emitter.comment("chown()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve path ptr/len while uid is evaluated
            let principal_ty = emit_expr(&args[1], emitter, ctx, data);
            if principal_ty == PhpType::Str {
                emitter.instruction("mov x3, x1");                              // user-name pointer → runtime string slot
                emitter.instruction("mov x4, x2");                              // user-name length → runtime string slot
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown_user");               // resolve user name and call libc chown()
            } else {
                emitter.instruction("mov x3, x0");                              // uid → runtime uid register
                emitter.instruction("mov x4, #-1");                             // gid = -1 (leave group unchanged)
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown");                    // call libc chown(path, uid, -1)
            }
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve path ptr/len
            let principal_ty = emit_expr(&args[1], emitter, ctx, data);
            if principal_ty == PhpType::Str {
                emitter.instruction("mov rdi, rax");                            // user-name pointer → runtime string slot
                emitter.instruction("mov rsi, rdx");                            // user-name length → runtime string slot
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown_user");               // resolve user name and call libc chown()
            } else {
                emitter.instruction("mov rdi, rax");                            // uid → secondary integer arg slot
                emitter.instruction("mov rsi, -1");                             // gid = -1 (leave group unchanged)
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown");                    // call libc chown(path, uid, -1)
            }
        }
    }
    Some(PhpType::Bool)
}
