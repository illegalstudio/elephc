use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_encode_null: produce the "null" JSON string.
/// Output: x1 = string ptr, x2 = string len
pub(crate) fn emit_json_encode_null(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_null_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_encode_null ---");
    emitter.label_global("__rt_json_encode_null");

    emitter.adrp("x1", "_json_null");                            // load page of "null" string
    emitter.add_lo12("x1", "x1", "_json_null");                      // resolve "null" address
    emitter.instruction("mov x2, #4");                                          // length of "null"
    emitter.instruction("ret");                                                 // return
}

fn emit_json_encode_null_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_null ---");
    emitter.label_global("__rt_json_encode_null");

    emitter.instruction("lea rax, [rip + _json_null]");                         // materialize the address of the static JSON null literal
    emitter.instruction("mov rdx, 4");                                          // return the byte length of the JSON null literal
    emitter.instruction("ret");                                                 // return the borrowed JSON literal slice in the x86_64 string result registers
}
