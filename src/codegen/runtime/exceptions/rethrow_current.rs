use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

pub fn emit_rethrow_current(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rethrow_current ---");
    emitter.label_global("__rt_rethrow_current");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("b __rt_throw_current");                         // re-use the ordinary throw helper with the existing active exception state
        }
        Arch::X86_64 => {
            emitter.instruction("jmp __rt_throw_current");                       // re-use the ordinary throw helper with the existing active exception state
        }
    }
}
