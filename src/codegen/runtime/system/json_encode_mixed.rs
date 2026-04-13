use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

pub(crate) fn emit_json_encode_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_encode_mixed ---");
    emitter.label_global("__rt_json_encode_mixed");

    emitter.instruction("cbz x0, __rt_json_encode_mixed_null");                 // null mixed pointers encode as JSON null
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed runtime value_tag
    emitter.instruction("cmp x9, #0");                                          // is the boxed value an integer?
    emitter.instruction("b.eq __rt_json_encode_mixed_int");                     // encode integers via itoa
    emitter.instruction("cmp x9, #1");                                          // is the boxed value a string?
    emitter.instruction("b.eq __rt_json_encode_mixed_str");                     // encode strings with JSON escaping
    emitter.instruction("cmp x9, #2");                                          // is the boxed value a float?
    emitter.instruction("b.eq __rt_json_encode_mixed_float");                   // encode floats via ftoa
    emitter.instruction("cmp x9, #3");                                          // is the boxed value a bool?
    emitter.instruction("b.eq __rt_json_encode_mixed_bool");                    // encode bools via json_encode_bool
    emitter.instruction("cmp x9, #4");                                          // is the boxed value an indexed array?
    emitter.instruction("b.eq __rt_json_encode_mixed_array");                   // encode nested arrays via the indexed-array helpers
    emitter.instruction("cmp x9, #5");                                          // is the boxed value an associative array?
    emitter.instruction("b.eq __rt_json_encode_mixed_assoc");                   // encode nested associative arrays recursively
    emitter.instruction("cmp x9, #8");                                          // is the boxed value null?
    emitter.instruction("b.eq __rt_json_encode_mixed_null");                    // encode null via json_encode_null
    emitter.instruction("b __rt_json_encode_mixed_null");                       // unsupported object/mixed payloads currently encode as null

    emitter.label("__rt_json_encode_mixed_int");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed integer payload
    emitter.instruction("b __rt_itoa");                                         // tail-call to integer JSON encoding

    emitter.label("__rt_json_encode_mixed_str");
    emitter.instruction("ldr x1, [x0, #8]");                                    // load the boxed string pointer
    emitter.instruction("ldr x2, [x0, #16]");                                   // load the boxed string length
    emitter.instruction("b __rt_json_encode_str");                              // tail-call to string JSON encoding

    emitter.label("__rt_json_encode_mixed_float");
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the boxed float bits
    emitter.instruction("fmov d0, x9");                                         // move the boxed float bits into the FP argument register
    emitter.instruction("b __rt_ftoa");                                         // tail-call to float JSON encoding

    emitter.label("__rt_json_encode_mixed_bool");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed bool payload
    emitter.instruction("b __rt_json_encode_bool");                             // tail-call to bool JSON encoding

    emitter.label("__rt_json_encode_mixed_array");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed array pointer
    emitter.instruction("b __rt_json_encode_array_dynamic");                    // tail-call to the dynamic indexed-array JSON encoder

    emitter.label("__rt_json_encode_mixed_assoc");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed associative-array pointer
    emitter.instruction("b __rt_json_encode_assoc");                            // tail-call to associative-array JSON encoding

    emitter.label("__rt_json_encode_mixed_null");
    emitter.instruction("b __rt_json_encode_null");                             // tail-call to JSON null encoding
}

fn emit_json_encode_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_mixed ---");
    emitter.label_global("__rt_json_encode_mixed");

    emitter.instruction("test rax, rax");                                       // null mixed boxes encode as the JSON null literal immediately
    emitter.instruction("jz __rt_json_encode_mixed_null");                      // branch to the shared null encoder when no mixed box exists
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the boxed runtime value tag from the mixed cell header
    emitter.instruction("cmp r10, 0");                                          // is the boxed payload an integer?
    emitter.instruction("je __rt_json_encode_mixed_int");                       // encode integers through the decimal integer helper
    emitter.instruction("cmp r10, 1");                                          // is the boxed payload a string?
    emitter.instruction("je __rt_json_encode_mixed_str");                       // encode strings through the JSON string helper
    emitter.instruction("cmp r10, 2");                                          // is the boxed payload a float?
    emitter.instruction("je __rt_json_encode_mixed_float");                     // encode floats through the decimal float helper
    emitter.instruction("cmp r10, 3");                                          // is the boxed payload a bool?
    emitter.instruction("je __rt_json_encode_mixed_bool");                      // encode bools through the JSON bool helper
    emitter.instruction("cmp r10, 4");                                          // is the boxed payload an indexed array?
    emitter.instruction("je __rt_json_encode_mixed_array");                     // encode nested indexed arrays recursively
    emitter.instruction("cmp r10, 5");                                          // is the boxed payload an associative array?
    emitter.instruction("je __rt_json_encode_mixed_assoc");                     // encode nested associative arrays recursively
    emitter.instruction("cmp r10, 8");                                          // is the boxed payload null?
    emitter.instruction("je __rt_json_encode_mixed_null");                      // encode explicit null payloads through the shared helper
    emitter.instruction("jmp __rt_json_encode_mixed_null");                     // unsupported payload families currently degrade to JSON null on x86_64 too

    emitter.label("__rt_json_encode_mixed_int");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed integer payload into the standard integer result register
    emitter.instruction("jmp __rt_itoa");                                       // tail-call to the integer JSON encoder

    emitter.label("__rt_json_encode_mixed_str");
    emitter.instruction("mov rdx, QWORD PTR [rax + 16]");                       // load the boxed string length into the paired x86_64 string result register
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed string pointer into the leading x86_64 string result register
    emitter.instruction("jmp __rt_json_encode_str");                            // tail-call to the JSON string encoder

    emitter.label("__rt_json_encode_mixed_float");
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the boxed float bit-pattern from the mixed cell payload
    emitter.instruction("movq xmm0, r10");                                      // move the boxed float bits into the x86_64 floating-point argument register
    emitter.instruction("jmp __rt_ftoa");                                       // tail-call to the float JSON encoder

    emitter.label("__rt_json_encode_mixed_bool");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed bool payload into the standard integer result register
    emitter.instruction("jmp __rt_json_encode_bool");                           // tail-call to the JSON bool encoder

    emitter.label("__rt_json_encode_mixed_array");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed indexed-array pointer from the mixed cell payload
    emitter.instruction("jmp __rt_json_encode_array_dynamic");                  // tail-call to the dynamic indexed-array JSON encoder

    emitter.label("__rt_json_encode_mixed_assoc");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed associative-array pointer from the mixed cell payload
    emitter.instruction("jmp __rt_json_encode_assoc");                          // tail-call to the associative-array JSON encoder

    emitter.label("__rt_json_encode_mixed_null");
    emitter.instruction("jmp __rt_json_encode_null");                           // tail-call to the shared JSON null encoder
}
