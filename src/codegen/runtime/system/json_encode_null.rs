use crate::codegen::emit::Emitter;

/// __rt_json_encode_null: produce the "null" JSON string.
/// Output: x1 = string ptr, x2 = string len
pub(crate) fn emit_json_encode_null(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_null ---");
    emitter.label_global("__rt_json_encode_null");

    emitter.adrp("x1", "_json_null");                            // load page of "null" string
    emitter.add_lo12("x1", "x1", "_json_null");                      // resolve "null" address
    emitter.instruction("mov x2, #4");                                          // length of "null"
    emitter.instruction("ret");                                                 // return
}
