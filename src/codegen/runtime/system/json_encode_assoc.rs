use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_encode_assoc: encode an assoc array as JSON '{"key":"value",...}'.
/// Input:  x0 = hash table pointer
/// Output: x1 = result ptr (in concat_buf), x2 = result len
///
/// Uses __rt_hash_iter_next to iterate the hash table entries in insertion order.
/// Hash table iter yields: x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi, x5=val_tag per entry.
pub(crate) fn emit_json_encode_assoc(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_assoc_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_encode_assoc ---");
    emitter.label_global("__rt_json_encode_assoc");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #112");                                    // allocate 112 bytes
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // set new frame pointer

    // -- save hash table pointer --
    emitter.instruction("str x0, [sp, #0]");                                    // save hash ptr

    // -- get output position in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // output position
    emitter.instruction("str x11, [sp, #8]");                                   // save output start
    emitter.instruction("str x11, [sp, #16]");                                  // save output write pos

    // -- write opening brace --
    emitter.instruction("mov w12, #123");                                       // ASCII '{'
    emitter.instruction("strb w12, [x11]");                                     // write '{'
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- get hash table count --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload hash ptr
    emitter.instruction("bl __rt_hash_count");                                  // get count → x0
    emitter.instruction("str x0, [sp, #24]");                                   // save count
    emitter.instruction("str xzr, [sp, #32]");                                  // iterator cursor = 0 (start from hash header head)
    emitter.instruction("str xzr, [sp, #40]");                                  // items written = 0

    // -- iterate hash table entries --
    emitter.label("__rt_json_assoc_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                   // load items written
    emitter.instruction("ldr x3, [sp, #24]");                                   // load total count
    emitter.instruction("cmp x4, x3");                                          // check if all items written
    emitter.instruction("b.ge __rt_json_assoc_close");                          // done

    // -- get next entry via hash_iter --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload hash ptr
    emitter.instruction("ldr x1, [sp, #32]");                                   // load iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // get entry → x0=next_cursor, x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi
    emitter.instruction("str x0, [sp, #32]");                                   // save next iterator cursor

    // -- save key and value on stack --
    emitter.instruction("str x1, [sp, #48]");                                   // save key ptr
    emitter.instruction("str x2, [sp, #56]");                                   // save key len
    emitter.instruction("str x3, [sp, #64]");                                   // save val_lo
    emitter.instruction("str x4, [sp, #72]");                                   // save val_hi
    emitter.instruction("str x5, [sp, #88]");                                   // save val_tag

    // -- add comma if not first entry --
    emitter.instruction("ldr x5, [sp, #40]");                                   // load items written
    emitter.instruction("cbz x5, __rt_json_assoc_key");                         // skip comma for first
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #44");                                        // ASCII ','
    emitter.instruction("strb w12, [x11]");                                     // write ','
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- write key as quoted string --
    emitter.label("__rt_json_assoc_key");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // write opening quote
    emitter.instruction("add x11, x11, #1");                                    // advance

    // -- copy key bytes --
    emitter.instruction("ldr x1, [sp, #48]");                                   // load key ptr
    emitter.instruction("ldr x2, [sp, #56]");                                   // load key len
    emitter.instruction("mov x10, #0");                                         // copy index
    emitter.label("__rt_json_assoc_key_copy");
    emitter.instruction("cmp x10, x2");                                         // check if done
    emitter.instruction("b.ge __rt_json_assoc_key_done");                       // done
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load key byte
    emitter.instruction("strb w12, [x11, x10]");                                // write to output
    emitter.instruction("add x10, x10, #1");                                    // increment
    emitter.instruction("b __rt_json_assoc_key_copy");                          // continue
    emitter.label("__rt_json_assoc_key_done");
    emitter.instruction("add x11, x11, x2");                                    // advance write pos
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // write closing quote
    emitter.instruction("add x11, x11, #1");                                    // advance

    // -- write colon --
    emitter.instruction("mov w12, #58");                                        // ASCII ':'
    emitter.instruction("strb w12, [x11]");                                     // write ':'
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos after emitting the JSON key prefix

    // -- move concat_off to the current write position so nested encoders append safely --
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current output write position
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x11, x10");                                   // x12 = absolute concat offset for the current write position
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x12, [x9]");                                       // nested JSON/string encoders append after the existing key prefix

    // -- encode the value according to its per-entry runtime tag --
    emitter.instruction("ldr x12, [sp, #88]");                                  // load the saved per-entry value_tag
    emitter.instruction("cmp x12, #0");                                         // is this value an integer?
    emitter.instruction("b.eq __rt_json_assoc_value_int");                      // encode integers via itoa
    emitter.instruction("cmp x12, #1");                                         // is this value a string?
    emitter.instruction("b.eq __rt_json_assoc_value_str");                      // encode strings with JSON escaping
    emitter.instruction("cmp x12, #2");                                         // is this value a float?
    emitter.instruction("b.eq __rt_json_assoc_value_float");                    // encode floats via ftoa
    emitter.instruction("cmp x12, #3");                                         // is this value a bool?
    emitter.instruction("b.eq __rt_json_assoc_value_bool");                     // encode bools via json_encode_bool
    emitter.instruction("cmp x12, #4");                                         // is this value an indexed array?
    emitter.instruction("b.eq __rt_json_assoc_value_array");                    // encode arrays via the indexed-array JSON helpers
    emitter.instruction("cmp x12, #5");                                         // is this value an associative array?
    emitter.instruction("b.eq __rt_json_assoc_value_assoc");                    // encode nested associative arrays recursively
    emitter.instruction("cmp x12, #8");                                         // is this value null?
    emitter.instruction("b.eq __rt_json_assoc_value_null");                     // encode null via json_encode_null
    emitter.instruction("b __rt_json_assoc_value_null");                        // unsupported mixed/object payloads currently encode as null

    emitter.label("__rt_json_assoc_value_int");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load integer payload from value_lo
    emitter.instruction("bl __rt_itoa");                                        // encode integer payload as decimal digits
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_str");
    emitter.instruction("ldr x1, [sp, #64]");                                   // load string pointer from value_lo
    emitter.instruction("ldr x2, [sp, #72]");                                   // load string length from value_hi
    emitter.instruction("bl __rt_json_encode_str");                             // encode string payload with JSON escaping and quotes
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_float");
    emitter.instruction("ldr x9, [sp, #64]");                                   // load float bits from value_lo
    emitter.instruction("fmov d0, x9");                                         // move float bits into the FP argument register
    emitter.instruction("bl __rt_ftoa");                                        // encode float payload as decimal digits
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_bool");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load bool payload from value_lo
    emitter.instruction("bl __rt_json_encode_bool");                            // encode bool payload as true/false
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_array");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load nested array pointer from value_lo
    emitter.instruction("bl __rt_json_encode_array_dynamic");                   // encode nested indexed arrays through the dynamic array JSON helper
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded nested array into concat_buf

    emitter.label("__rt_json_assoc_value_assoc");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load nested associative array pointer from value_lo
    emitter.instruction("bl __rt_json_encode_assoc");                           // encode the nested associative array recursively
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded nested associative array into concat_buf

    emitter.label("__rt_json_assoc_value_null");
    emitter.instruction("bl __rt_json_encode_null");                            // encode null or unsupported payloads as JSON null

    emitter.label("__rt_json_assoc_value_copy");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current concat_buf write position
    emitter.instruction("mov x10, #0");                                         // copy index
    emitter.label("__rt_json_assoc_val_copy");
    emitter.instruction("cmp x10, x2");                                         // check if done
    emitter.instruction("b.ge __rt_json_assoc_val_done");                       // done
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load val byte
    emitter.instruction("strb w12, [x11, x10]");                                // write to output
    emitter.instruction("add x10, x10, #1");                                    // increment
    emitter.instruction("b __rt_json_assoc_val_copy");                          // continue
    emitter.label("__rt_json_assoc_val_done");
    emitter.instruction("add x11, x11, x2");                                    // advance write pos
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- increment items written --
    emitter.instruction("ldr x5, [sp, #40]");                                   // load items written
    emitter.instruction("add x5, x5, #1");                                      // increment
    emitter.instruction("str x5, [sp, #40]");                                   // save items written
    emitter.instruction("b __rt_json_assoc_loop");                              // continue loop

    // -- write closing brace --
    emitter.label("__rt_json_assoc_close");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #125");                                       // ASCII '}'
    emitter.instruction("strb w12, [x11]");                                     // write '}'
    emitter.instruction("add x11, x11, #1");                                    // advance

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = output start
    emitter.instruction("sub x2, x11, x1");                                     // x2 = total length

    // -- update concat_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x10, x11, x10");                                   // compute the absolute concat offset after the closing brace
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_json_encode_assoc_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_assoc ---");
    emitter.label_global("__rt_json_encode_assoc");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving JSON-assoc scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for hash iteration state and concat-buffer cursors
    emitter.instruction("sub rsp, 80");                                         // reserve local slots for the hash pointer, output pointers, cursor, item count, and entry scratch payload
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source associative-array pointer across nested JSON helper calls
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer offset before appending the JSON object
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the current JSON append
    emitter.instruction("add r11, r10");                                        // compute the current concat-buffer write pointer from the base plus offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the encoded-object start pointer for the final result slice
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the current concat-buffer write pointer for the hash iteration loop
    emitter.instruction("mov BYTE PTR [r11], 123");                             // write the opening JSON brace before any encoded key/value pair
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the opening brace
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer before entering the hash iteration loop
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the hash iterator cursor to the insertion-order start sentinel
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the number of encoded key/value pairs to zero

    emitter.label("__rt_json_assoc_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source associative-array pointer for the next insertion-order iteration step
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the current hash iterator cursor for the next insertion-order iteration step
    emitter.instruction("call __rt_hash_iter_next");                            // advance one insertion-order hash entry and return its key plus payload in the x86_64 result registers
    emitter.instruction("cmp rax, -1");                                         // has associative-array iteration reached the done sentinel?
    emitter.instruction("je __rt_json_assoc_close");                            // finish by writing the closing brace once every hash entry has been encoded
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the updated hash iterator cursor for the next insertion-order loop step
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the current associative-array key pointer for the JSON key copy loop
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save the current associative-array key length for the JSON key copy loop
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // save the current associative-array value low payload word for runtime JSON dispatch
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // save the current associative-array value high payload word for runtime JSON dispatch
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");                        // save the current associative-array runtime value tag for runtime JSON dispatch
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // is this the first encoded associative-array entry in the JSON object?
    emitter.instruction("je __rt_json_assoc_key");                              // skip the comma separator before the first encoded associative-array entry
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer before appending the comma separator
    emitter.instruction("mov BYTE PTR [r11], 44");                              // write the JSON comma separator between encoded associative-array entries
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the comma separator
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer after appending the comma separator

    emitter.label("__rt_json_assoc_key");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer before emitting the JSON key prefix
    emitter.instruction("mov BYTE PTR [r11], 34");                              // write the opening JSON key quote before copying the raw key bytes
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the opening key quote
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the associative-array key pointer for the JSON key copy loop
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the associative-array key length for the JSON key copy loop
    emitter.instruction("xor rdx, rdx");                                        // initialize the JSON key copy index to the beginning of the associative-array key slice

    emitter.label("__rt_json_assoc_key_copy");
    emitter.instruction("cmp rdx, rcx");                                        // have we copied every byte of the associative-array key slice?
    emitter.instruction("jae __rt_json_assoc_key_done");                        // finish the key prefix once every key byte has been copied into concat_buf
    emitter.instruction("mov r8b, BYTE PTR [r10 + rdx]");                       // load the next associative-array key byte from the borrowed hash-entry key slice
    emitter.instruction("mov BYTE PTR [r11 + rdx], r8b");                       // copy the associative-array key byte into concat_buf after the opening quote
    emitter.instruction("add rdx, 1");                                          // advance the JSON key copy index to the next borrowed key byte
    emitter.instruction("jmp __rt_json_assoc_key_copy");                        // continue copying the associative-array key bytes into concat_buf

    emitter.label("__rt_json_assoc_key_done");
    emitter.instruction("add r11, rcx");                                        // advance the concat-buffer write pointer by the copied associative-array key length
    emitter.instruction("mov BYTE PTR [r11], 34");                              // write the closing JSON key quote after the copied key bytes
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the closing key quote
    emitter.instruction("mov BYTE PTR [r11], 58");                              // write the JSON colon separator between the encoded key and encoded value
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the colon separator
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer after emitting the full JSON key prefix
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update before nested value encoding
    emitter.instruction("mov rcx, r11");                                        // copy the current write pointer before turning it into an absolute concat offset
    emitter.instruction("sub rcx, r10");                                        // compute the concat-buffer absolute offset for the current JSON value write position
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the concat-buffer offset so nested JSON helpers append after the existing key prefix
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the saved associative-array runtime value tag for runtime JSON dispatch
    emitter.instruction("cmp r10, 0");                                          // is the associative-array value an integer?
    emitter.instruction("je __rt_json_assoc_value_int");                        // encode integer payloads through the decimal integer helper
    emitter.instruction("cmp r10, 1");                                          // is the associative-array value a string?
    emitter.instruction("je __rt_json_assoc_value_str");                        // encode string payloads through the JSON string helper
    emitter.instruction("cmp r10, 2");                                          // is the associative-array value a float?
    emitter.instruction("je __rt_json_assoc_value_float");                      // encode float payloads through the decimal float helper
    emitter.instruction("cmp r10, 3");                                          // is the associative-array value a bool?
    emitter.instruction("je __rt_json_assoc_value_bool");                       // encode bool payloads through the JSON bool helper
    emitter.instruction("cmp r10, 4");                                          // is the associative-array value a nested indexed array?
    emitter.instruction("je __rt_json_assoc_value_array");                      // encode nested indexed arrays recursively
    emitter.instruction("cmp r10, 5");                                          // is the associative-array value a nested associative array?
    emitter.instruction("je __rt_json_assoc_value_assoc");                      // encode nested associative arrays recursively
    emitter.instruction("cmp r10, 7");                                          // is the associative-array value a boxed mixed payload?
    emitter.instruction("je __rt_json_assoc_value_mixed");                      // encode boxed mixed payloads through the mixed JSON helper
    emitter.instruction("cmp r10, 8");                                          // is the associative-array value explicit null?
    emitter.instruction("je __rt_json_assoc_value_null");                       // encode explicit null payloads through the shared helper
    emitter.instruction("jmp __rt_json_assoc_value_null");                      // unsupported payload families currently degrade to JSON null

    emitter.label("__rt_json_assoc_value_int");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the integer payload from the saved hash-entry low payload word
    emitter.instruction("call __rt_itoa");                                      // encode the integer payload as a decimal JSON slice
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_str");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the string pointer from the saved hash-entry low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // load the string length from the saved hash-entry high payload word
    emitter.instruction("call __rt_json_encode_str");                           // encode the string payload with JSON escaping and quotes
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_float");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // load the raw float bit-pattern from the saved hash-entry low payload word
    emitter.instruction("movq xmm0, r10");                                      // move the raw float bit-pattern into the x86_64 floating-point argument register
    emitter.instruction("call __rt_ftoa");                                      // encode the float payload as a decimal JSON slice
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_bool");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the bool payload from the saved hash-entry low payload word
    emitter.instruction("call __rt_json_encode_bool");                          // encode the bool payload as the JSON literals true/false
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_array");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the nested indexed-array pointer from the saved hash-entry low payload word
    emitter.instruction("call __rt_json_encode_array_dynamic");                 // encode the nested indexed array recursively into a JSON slice
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_assoc");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the nested associative-array pointer from the saved hash-entry low payload word
    emitter.instruction("call __rt_json_encode_assoc");                         // encode the nested associative array recursively into a JSON slice
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_mixed");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the boxed mixed pointer from the saved hash-entry low payload word
    emitter.instruction("call __rt_json_encode_mixed");                         // encode the boxed mixed payload recursively into a JSON slice
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_null");
    emitter.instruction("call __rt_json_encode_null");                          // encode null or unsupported payload families as the JSON null literal

    emitter.label("__rt_json_assoc_value_copy");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer before copying the encoded JSON value bytes
    emitter.instruction("xor rcx, rcx");                                        // initialize the encoded-value copy index to the beginning of the returned JSON slice
    emitter.label("__rt_json_assoc_val_copy");
    emitter.instruction("cmp rcx, rdx");                                        // have we copied every byte of the returned encoded JSON value slice?
    emitter.instruction("jae __rt_json_assoc_val_done");                        // finish copying once the entire returned JSON value slice has been appended
    emitter.instruction("mov r10b, BYTE PTR [rax + rcx]");                      // load the next encoded JSON value byte from the returned slice
    emitter.instruction("mov BYTE PTR [r11 + rcx], r10b");                      // copy the encoded JSON value byte into concat_buf at the current write position
    emitter.instruction("add rcx, 1");                                          // advance the encoded-value copy index to the next byte
    emitter.instruction("jmp __rt_json_assoc_val_copy");                        // continue copying until the whole returned JSON value slice has been appended

    emitter.label("__rt_json_assoc_val_done");
    emitter.instruction("add r11, rdx");                                        // advance the concat-buffer write pointer by the copied encoded-value length
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer after appending the encoded JSON value
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // increment the number of encoded associative-array entries after appending one full key/value pair
    emitter.instruction("jmp __rt_json_assoc_loop");                            // continue iterating the remaining associative-array entries in insertion order

    emitter.label("__rt_json_assoc_close");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the concat-buffer write pointer after the final encoded JSON entry
    emitter.instruction("mov BYTE PTR [r11], 125");                             // append the closing JSON brace to complete the encoded object slice
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the closing brace
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the encoded-object start pointer in the leading x86_64 string result register
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer write pointer before turning it into a slice length
    emitter.instruction("sub rdx, rax");                                        // compute the final encoded-object length from write_end - write_start
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update
    emitter.instruction("mov rcx, r11");                                        // copy the final concat-buffer write pointer before converting it into an absolute offset
    emitter.instruction("sub rcx, r10");                                        // compute the new absolute concat-buffer offset after the encoded JSON object
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the updated concat-buffer offset so later writers append after this JSON object
    emitter.instruction("add rsp, 80");                                         // release the local JSON-assoc scratch frame before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the encoded JSON object slice in the x86_64 string result registers
}
