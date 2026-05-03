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
    emitter.comment("chgrp()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve path ptr/len while gid is evaluated
            let principal_ty = emit_expr(&args[1], emitter, ctx, data);
            if principal_ty == PhpType::Str {
                emitter.instruction("mov x3, x1");                              // group-name pointer → runtime string slot
                emitter.instruction("mov x4, x2");                              // group-name length → runtime string slot
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chgrp_group");              // resolve group name and call libc chown()
            } else {
                emitter.instruction("mov x4, x0");                              // gid → runtime gid register
                emitter.instruction("mov x3, #-1");                             // uid = -1 (leave owner unchanged)
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown");                    // call libc chown(path, -1, gid)
            }
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve path ptr/len
            let principal_ty = emit_expr(&args[1], emitter, ctx, data);
            if principal_ty == PhpType::Str {
                emitter.instruction("mov rdi, rax");                            // group-name pointer → runtime string slot
                emitter.instruction("mov rsi, rdx");                            // group-name length → runtime string slot
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chgrp_group");              // resolve group name and call libc chown()
            } else {
                emitter.instruction("mov rsi, rax");                            // gid → tertiary integer arg slot
                emitter.instruction("mov rdi, -1");                             // uid = -1 (leave owner unchanged)
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown");                    // call libc chown(path, -1, gid)
            }
        }
    }
    Some(PhpType::Bool)
}
