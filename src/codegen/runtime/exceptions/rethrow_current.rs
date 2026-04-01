use crate::codegen::emit::Emitter;

pub fn emit_rethrow_current(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rethrow_current ---");
    emitter.label_global("__rt_rethrow_current");
    emitter.instruction("b __rt_throw_current");                                // re-use the ordinary throw helper with the existing active exception state
}
