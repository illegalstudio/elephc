use super::emit::Emitter;

pub fn emit_runtime(emitter: &mut Emitter) {
    // itoa: convert integer in x0 to string
    // Returns pointer in x1, length in x2
    emitter.blank();
    emitter.comment("--- runtime: itoa ---");
    emitter.label("__rt_itoa");
    emitter.comment("Input: x0 = integer value");
    emitter.comment("Output: x1 = pointer to string, x2 = length");
    emitter.comment("TODO: implement in Phase 2");
    emitter.instruction("ret");
}
