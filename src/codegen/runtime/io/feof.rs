use crate::codegen::emit::Emitter;

/// feof: check if EOF has been reached for a file descriptor.
/// Input:  x0=fd
/// Output: x0=1 if EOF, 0 if not
pub fn emit_feof(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: feof ---");
    emitter.label_global("__rt_feof");

    // -- load eof flag for this fd from _eof_flags array --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("ldrb w0, [x9, x0]");                                   // load _eof_flags[fd] into return register
    emitter.instruction("ret");                                                 // return to caller
}
