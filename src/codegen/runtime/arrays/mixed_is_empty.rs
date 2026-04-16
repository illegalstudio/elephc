use crate::codegen::{emit::Emitter, platform::Arch};

/// mixed_is_empty: implement PHP empty() semantics for boxed mixed values.
/// Input:  x0 = boxed mixed pointer
/// Output: x0 = 1 when the payload is empty, else 0
pub fn emit_mixed_is_empty(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_is_empty_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_is_empty ---");
    emitter.label_global("__rt_mixed_is_empty");

    // -- null boxed pointers behave like null --
    emitter.instruction("cbz x0, __rt_mixed_is_empty_yes");                     // missing mixed boxes are empty like null

    // -- unwrap nested mixed boxes until we reach a concrete payload tag --
    emitter.label("__rt_mixed_is_empty_unbox");
    emitter.instruction("ldr x9, [x0]");                                        // x9 = boxed payload tag
    emitter.instruction("cmp x9, #7");                                          // does this mixed box wrap another mixed value?
    emitter.instruction("b.ne __rt_mixed_is_empty_dispatch");                   // stop once we reach a concrete payload tag
    emitter.instruction("ldr x0, [x0, #8]");                                    // follow the nested mixed pointer stored in value_lo
    emitter.instruction("cbz x0, __rt_mixed_is_empty_yes");                     // null nested boxes collapse to empty
    emitter.instruction("b __rt_mixed_is_empty_unbox");                         // keep unboxing nested mixed wrappers

    // -- dispatch on the concrete payload tag --
    emitter.label("__rt_mixed_is_empty_dispatch");
    emitter.instruction("cmp x9, #0");                                          // is the payload an int?
    emitter.instruction("b.eq __rt_mixed_is_empty_int");                        // ints are empty when equal to zero
    emitter.instruction("cmp x9, #1");                                          // is the payload a string?
    emitter.instruction("b.eq __rt_mixed_is_empty_string");                     // strings are empty when their length is zero
    emitter.instruction("cmp x9, #2");                                          // is the payload a float?
    emitter.instruction("b.eq __rt_mixed_is_empty_float");                      // floats are empty when equal to 0.0
    emitter.instruction("cmp x9, #3");                                          // is the payload a bool?
    emitter.instruction("b.eq __rt_mixed_is_empty_bool");                       // bool false is empty
    emitter.instruction("cmp x9, #4");                                          // is the payload an indexed array?
    emitter.instruction("b.eq __rt_mixed_is_empty_array");                      // arrays are empty when their element count is zero
    emitter.instruction("cmp x9, #5");                                          // is the payload an associative array?
    emitter.instruction("b.eq __rt_mixed_is_empty_array");                      // hashes are empty when their element count is zero
    emitter.instruction("cmp x9, #6");                                          // is the payload an object?
    emitter.instruction("b.eq __rt_mixed_is_empty_no");                         // objects are never empty in PHP
    emitter.instruction("b __rt_mixed_is_empty_yes");                           // null and unknown payloads are treated as empty

    emitter.label("__rt_mixed_is_empty_int");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the integer payload from value_lo
    emitter.instruction("cmp x10, #0");                                         // compare the integer payload against zero
    emitter.instruction("cset x0, eq");                                         // return 1 when the integer payload is zero
    emitter.instruction("ret");                                                 // finish integer empty() evaluation

    emitter.label("__rt_mixed_is_empty_string");
    emitter.instruction("ldr x10, [x0, #16]");                                  // load the string length from value_hi
    emitter.instruction("cbz x10, __rt_mixed_is_empty_yes");                    // empty strings are empty
    emitter.instruction("cmp x10, #1");                                         // check whether the string length is exactly one byte
    emitter.instruction("b.ne __rt_mixed_is_empty_no");                         // longer strings are not empty
    emitter.instruction("ldr x11, [x0, #8]");                                   // load the string pointer from value_lo
    emitter.instruction("ldrb w12, [x11]");                                     // load the first byte of the string payload
    emitter.instruction("cmp w12, #48");                                        // compare against ASCII '0'
    emitter.instruction("cset x0, eq");                                         // the one-byte string \"0\" is empty, anything else is not
    emitter.instruction("ret");                                                 // finish string empty() evaluation

    emitter.label("__rt_mixed_is_empty_float");
    emitter.instruction("ldr d0, [x0, #8]");                                    // load the float payload from value_lo
    emitter.instruction("fcmp d0, #0.0");                                       // compare the float payload against 0.0
    emitter.instruction("cset x0, eq");                                         // return 1 when the float payload is 0.0
    emitter.instruction("ret");                                                 // finish float empty() evaluation

    emitter.label("__rt_mixed_is_empty_bool");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the boolean payload from value_lo
    emitter.instruction("cmp x10, #0");                                         // compare the boolean payload against false
    emitter.instruction("cset x0, eq");                                         // return 1 when the boolean payload is false
    emitter.instruction("ret");                                                 // finish bool empty() evaluation

    emitter.label("__rt_mixed_is_empty_array");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the array/hash pointer from value_lo
    emitter.instruction("cbz x10, __rt_mixed_is_empty_yes");                    // null containers behave like empty/null
    emitter.instruction("ldr x10, [x10]");                                      // load the container element count from the header
    emitter.instruction("cmp x10, #0");                                         // compare the element count against zero
    emitter.instruction("cset x0, eq");                                         // return 1 when the container has no elements
    emitter.instruction("ret");                                                 // finish array/hash empty() evaluation

    emitter.label("__rt_mixed_is_empty_yes");
    emitter.instruction("mov x0, #1");                                          // return true for empty payloads
    emitter.instruction("ret");                                                 // finish empty() evaluation

    emitter.label("__rt_mixed_is_empty_no");
    emitter.instruction("mov x0, #0");                                          // return false for non-empty payloads
    emitter.instruction("ret");                                                 // finish empty() evaluation
}

fn emit_mixed_is_empty_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_is_empty ---");
    emitter.label_global("__rt_mixed_is_empty");

    emitter.instruction("call __rt_mixed_unbox");                               // normalize nested mixed boxes to a concrete runtime tag plus payload words for x86_64
    emitter.instruction("cmp rax, 0");                                          // check whether the unboxed payload is an integer
    emitter.instruction("je __rt_mixed_is_empty_int_linux_x86_64");             // integers are empty when the unboxed payload is zero
    emitter.instruction("cmp rax, 1");                                          // check whether the unboxed payload is a string
    emitter.instruction("je __rt_mixed_is_empty_string_linux_x86_64");          // strings are empty when their length is zero or when they equal the single byte \"0\"
    emitter.instruction("cmp rax, 2");                                          // check whether the unboxed payload is a float
    emitter.instruction("je __rt_mixed_is_empty_float_linux_x86_64");           // floats are empty when they compare equal to 0.0
    emitter.instruction("cmp rax, 3");                                          // check whether the unboxed payload is a bool
    emitter.instruction("je __rt_mixed_is_empty_bool_linux_x86_64");            // bool false is empty under PHP empty() semantics
    emitter.instruction("cmp rax, 4");                                          // check whether the unboxed payload is an indexed array
    emitter.instruction("je __rt_mixed_is_empty_array_linux_x86_64");           // arrays are empty when their element count is zero
    emitter.instruction("cmp rax, 5");                                          // check whether the unboxed payload is an associative array
    emitter.instruction("je __rt_mixed_is_empty_array_linux_x86_64");           // hashes are empty when their element count is zero
    emitter.instruction("cmp rax, 6");                                          // check whether the unboxed payload is an object
    emitter.instruction("je __rt_mixed_is_empty_no_linux_x86_64");              // objects are never empty under PHP empty() semantics
    emitter.instruction("jmp __rt_mixed_is_empty_yes_linux_x86_64");            // null and unknown payload tags are treated as empty for the current mixed contract

    emitter.label("__rt_mixed_is_empty_int_linux_x86_64");
    emitter.instruction("cmp rdi, 0");                                          // compare the unboxed integer payload against zero in the low mixed payload word
    emitter.instruction("sete al");                                             // materialize the x86_64 integer empty() result in the low byte when the payload is zero
    emitter.instruction("movzx eax, al");                                       // widen the x86_64 boolean byte back into the canonical integer result register
    emitter.instruction("ret");                                                 // finish integer empty() evaluation on x86_64

    emitter.label("__rt_mixed_is_empty_string_linux_x86_64");
    emitter.instruction("cmp rdx, 0");                                          // check whether the unboxed string payload has zero length
    emitter.instruction("je __rt_mixed_is_empty_yes_linux_x86_64");             // empty strings are empty under PHP empty() semantics
    emitter.instruction("cmp rdx, 1");                                          // only one-byte strings can still be empty after the zero-length fast path
    emitter.instruction("jne __rt_mixed_is_empty_no_linux_x86_64");             // longer strings are never empty under PHP empty() semantics
    emitter.instruction("mov al, BYTE PTR [rdi]");                              // load the single byte stored by the one-byte unboxed string payload
    emitter.instruction("cmp al, 48");                                          // compare the single string byte against ASCII '0'
    emitter.instruction("sete al");                                             // materialize the x86_64 string empty() result in the low byte when the single byte equals '0'
    emitter.instruction("movzx eax, al");                                       // widen the x86_64 boolean byte back into the canonical integer result register
    emitter.instruction("ret");                                                 // finish string empty() evaluation on x86_64

    emitter.label("__rt_mixed_is_empty_float_linux_x86_64");
    emitter.instruction("movq xmm0, rdi");                                      // move the unboxed float payload bits into the native x86_64 scalar-double result register
    emitter.instruction("xorpd xmm1, xmm1");                                    // materialize a canonical 0.0 comparison operand in a scratch SIMD register
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare the unboxed float payload against 0.0 using the native x86_64 scalar-double compare
    emitter.instruction("sete al");                                             // materialize the x86_64 floating-point empty() result in the low byte when the payload equals 0.0
    emitter.instruction("movzx eax, al");                                       // widen the x86_64 boolean byte back into the canonical integer result register
    emitter.instruction("ret");                                                 // finish float empty() evaluation on x86_64

    emitter.label("__rt_mixed_is_empty_bool_linux_x86_64");
    emitter.instruction("cmp rdi, 0");                                          // compare the unboxed boolean payload against false in the low mixed payload word
    emitter.instruction("sete al");                                             // materialize the x86_64 boolean empty() result in the low byte when the payload is false
    emitter.instruction("movzx eax, al");                                       // widen the x86_64 boolean byte back into the canonical integer result register
    emitter.instruction("ret");                                                 // finish bool empty() evaluation on x86_64

    emitter.label("__rt_mixed_is_empty_array_linux_x86_64");
    emitter.instruction("test rdi, rdi");                                       // null array or hash payload pointers behave like empty/null under the mixed contract
    emitter.instruction("je __rt_mixed_is_empty_yes_linux_x86_64");             // treat missing array or hash payload pointers as empty
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the unboxed array or hash element count from the container header
    emitter.instruction("cmp rax, 0");                                          // compare the container element count against zero
    emitter.instruction("sete al");                                             // materialize the x86_64 container empty() result in the low byte when the element count is zero
    emitter.instruction("movzx eax, al");                                       // widen the x86_64 boolean byte back into the canonical integer result register
    emitter.instruction("ret");                                                 // finish array/hash empty() evaluation on x86_64

    emitter.label("__rt_mixed_is_empty_yes_linux_x86_64");
    emitter.instruction("mov eax, 1");                                          // return true for payloads that are empty under the current mixed empty() contract
    emitter.instruction("ret");                                                 // finish empty() evaluation on x86_64

    emitter.label("__rt_mixed_is_empty_no_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // return false for payloads that are not empty under the current mixed empty() contract
    emitter.instruction("ret");                                                 // finish empty() evaluation on x86_64
}
