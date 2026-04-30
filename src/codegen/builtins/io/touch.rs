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
    emitter.comment("touch()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            // x1/x2 = path. Now set up x3 = mtime, x4 = atime.
            match args.len() {
                1 => {
                    emitter.instruction("mov x3, #-1");                         // mtime = -1 → current time
                    emitter.instruction("mov x4, #-1");                         // atime = -1 → current time
                }
                2 => {
                    emitter.instruction("stp x1, x2, [sp, #-16]!");             // preserve path while mtime is evaluated
                    emit_expr(&args[1], emitter, ctx, data);
                    emitter.instruction("mov x3, x0");                          // mtime
                    emitter.instruction("mov x4, x0");                          // atime defaults to mtime
                    emitter.instruction("ldp x1, x2, [sp], #16");               // restore path ptr/len
                }
                _ => {
                    // 3 args: explicit mtime + atime
                    emitter.instruction("stp x1, x2, [sp, #-16]!");             // preserve path
                    emit_expr(&args[1], emitter, ctx, data);
                    emitter.instruction("str x0, [sp, #-16]!");                 // save mtime on stack
                    emit_expr(&args[2], emitter, ctx, data);
                    emitter.instruction("mov x4, x0");                          // atime = third arg
                    emitter.instruction("ldr x3, [sp], #16");                   // pop saved mtime → x3
                    emitter.instruction("ldp x1, x2, [sp], #16");               // restore path ptr/len
                }
            }
        }
        Arch::X86_64 => {
            match args.len() {
                1 => {
                    emitter.instruction("mov rdi, -1");                         // mtime = -1
                    emitter.instruction("mov rsi, -1");                         // atime = -1
                }
                2 => {
                    abi::emit_push_reg_pair(emitter, "rax", "rdx");             // preserve path
                    emit_expr(&args[1], emitter, ctx, data);
                    emitter.instruction("mov rdi, rax");                        // mtime
                    emitter.instruction("mov rsi, rax");                        // atime defaults to mtime
                    abi::emit_pop_reg_pair(emitter, "rax", "rdx");              // restore path
                }
                _ => {
                    abi::emit_push_reg_pair(emitter, "rax", "rdx");             // preserve path
                    emit_expr(&args[1], emitter, ctx, data);
                    emitter.instruction("push rax");                            // save mtime
                    emit_expr(&args[2], emitter, ctx, data);
                    emitter.instruction("mov rsi, rax");                        // atime
                    emitter.instruction("pop rdi");                             // pop saved mtime → rdi
                    abi::emit_pop_reg_pair(emitter, "rax", "rdx");              // restore path
                }
            }
        }
    }
    abi::emit_call_label(emitter, "__rt_touch");                                // call the target-aware runtime helper
    Some(PhpType::Bool)
}
