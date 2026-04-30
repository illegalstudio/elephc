use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("umask()");
    if args.is_empty() {
        // PHP allows umask() with no args to read the current umask without
        // changing it. The portable libc trick is to set umask(0) then
        // immediately set it back. Here we approximate by setting umask(0)
        // and then setting the returned value back, leaving the umask
        // unchanged on the way out.
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction("mov x0, #0");                              // probe with mask = 0
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction("xor eax, eax");                            // probe with mask = 0
            }
        }
        abi::emit_call_label(emitter, "__rt_umask");                            // first call → returns previous mask
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction("stp x0, xzr, [sp, #-16]!");                // save the probed previous mask
                // Restore the original umask immediately.
                // x0 now holds the previous mask; pass it back to umask().
                // The second call also returns the previous mask (which is the
                // probed-zero value), so we ignore that return and restore x0.
                emitter.instruction("ldr x0, [sp]");                            // reload previous mask
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction("push rax");                                // save the probed previous mask
                emitter.instruction("mov rax, QWORD PTR [rsp]");                // reload previous mask
            }
        }
        abi::emit_call_label(emitter, "__rt_umask");                            // restore the original umask
        // Discard whatever the second call returned and restore the saved value.
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction("ldp x0, xzr, [sp], #16");                  // pop the saved previous mask back into x0
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction("pop rax");                                 // pop the saved previous mask back into rax
            }
        }
        return Some(PhpType::Int);
    }
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_umask");                                // umask(mask) — returns previous mask
    Some(PhpType::Int)
}
