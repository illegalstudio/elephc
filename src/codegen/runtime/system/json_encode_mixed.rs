use crate::codegen::emit::Emitter;

pub(crate) fn emit_json_encode_mixed(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_mixed ---");
    emitter.label("__rt_json_encode_mixed");

    emitter.instruction("cbz x0, __rt_json_encode_mixed_null");                   // null mixed pointers encode as JSON null
    emitter.instruction("ldr x9, [x0]");                                          // load the boxed runtime value_tag
    emitter.instruction("cmp x9, #0");                                            // is the boxed value an integer?
    emitter.instruction("b.eq __rt_json_encode_mixed_int");                       // encode integers via itoa
    emitter.instruction("cmp x9, #1");                                            // is the boxed value a string?
    emitter.instruction("b.eq __rt_json_encode_mixed_str");                       // encode strings with JSON escaping
    emitter.instruction("cmp x9, #2");                                            // is the boxed value a float?
    emitter.instruction("b.eq __rt_json_encode_mixed_float");                     // encode floats via ftoa
    emitter.instruction("cmp x9, #3");                                            // is the boxed value a bool?
    emitter.instruction("b.eq __rt_json_encode_mixed_bool");                      // encode bools via json_encode_bool
    emitter.instruction("cmp x9, #4");                                            // is the boxed value an indexed array?
    emitter.instruction("b.eq __rt_json_encode_mixed_array");                     // encode nested arrays via the indexed-array helpers
    emitter.instruction("cmp x9, #5");                                            // is the boxed value an associative array?
    emitter.instruction("b.eq __rt_json_encode_mixed_assoc");                     // encode nested associative arrays recursively
    emitter.instruction("cmp x9, #8");                                            // is the boxed value null?
    emitter.instruction("b.eq __rt_json_encode_mixed_null");                      // encode null via json_encode_null
    emitter.instruction("b __rt_json_encode_mixed_null");                         // unsupported object/mixed payloads currently encode as null

    emitter.label("__rt_json_encode_mixed_int");
    emitter.instruction("ldr x0, [x0, #8]");                                      // load the boxed integer payload
    emitter.instruction("b __rt_itoa");                                           // tail-call to integer JSON encoding

    emitter.label("__rt_json_encode_mixed_str");
    emitter.instruction("ldr x1, [x0, #8]");                                      // load the boxed string pointer
    emitter.instruction("ldr x2, [x0, #16]");                                     // load the boxed string length
    emitter.instruction("b __rt_json_encode_str");                                // tail-call to string JSON encoding

    emitter.label("__rt_json_encode_mixed_float");
    emitter.instruction("ldr x9, [x0, #8]");                                      // load the boxed float bits
    emitter.instruction("fmov d0, x9");                                           // move the boxed float bits into the FP argument register
    emitter.instruction("b __rt_ftoa");                                           // tail-call to float JSON encoding

    emitter.label("__rt_json_encode_mixed_bool");
    emitter.instruction("ldr x0, [x0, #8]");                                      // load the boxed bool payload
    emitter.instruction("b __rt_json_encode_bool");                               // tail-call to bool JSON encoding

    emitter.label("__rt_json_encode_mixed_array");
    emitter.instruction("ldr x0, [x0, #8]");                                      // load the boxed array pointer
    emitter.instruction("b __rt_json_encode_array_dynamic");                      // tail-call to the dynamic indexed-array JSON encoder

    emitter.label("__rt_json_encode_mixed_assoc");
    emitter.instruction("ldr x0, [x0, #8]");                                      // load the boxed associative-array pointer
    emitter.instruction("b __rt_json_encode_assoc");                              // tail-call to associative-array JSON encoding

    emitter.label("__rt_json_encode_mixed_null");
    emitter.instruction("b __rt_json_encode_null");                               // tail-call to JSON null encoding
}
