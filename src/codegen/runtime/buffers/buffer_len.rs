use crate::codegen::emit::Emitter;

pub fn emit_buffer_len(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_len ---");
    emitter.label("__rt_buffer_len");
    emitter.instruction("ldr x0, [x0]");                                        // load the logical element count from the buffer header
    emitter.instruction("ret");                                                 // return length in x0
}
