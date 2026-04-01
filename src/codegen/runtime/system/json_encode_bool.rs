use crate::codegen::emit::Emitter;

/// __rt_json_encode_bool: convert boolean to "true" or "false" JSON string.
/// Input:  x0 = bool value (0 or 1)
/// Output: x1 = string ptr, x2 = string len
pub(crate) fn emit_json_encode_bool(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_bool ---");
    emitter.label_global("__rt_json_encode_bool");

    emitter.instruction("cbnz x0, __rt_json_encode_true");                      // if true, emit "true"

    // -- false --
    emitter.instruction("adrp x1, _json_false@PAGE");                           // load page of "false" string
    emitter.instruction("add x1, x1, _json_false@PAGEOFF");                     // resolve "false" address
    emitter.instruction("mov x2, #5");                                          // length of "false"
    emitter.instruction("ret");                                                 // return

    // -- true --
    emitter.label("__rt_json_encode_true");
    emitter.instruction("adrp x1, _json_true@PAGE");                            // load page of "true" string
    emitter.instruction("add x1, x1, _json_true@PAGEOFF");                      // resolve "true" address
    emitter.instruction("mov x2, #4");                                          // length of "true"
    emitter.instruction("ret");                                                 // return
}
