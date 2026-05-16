//! Purpose:
//! Emits x86_64 structural `json_decode()` Mixed decoder helpers.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen::runtime::system` during runtime emission.
//!
//! Key details:
//! - The SysV decoder path must mirror the AArch64 parser contract and shared JSON state slots.

use crate::codegen::emit::Emitter;

/// x86_64 implementation of `__rt_json_decode_mixed`. Mirrors the ARM64
/// dispatcher in `super::aarch64`; the recursive array/object helpers
/// live in `super::arrays` and `super::objects`.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode_mixed ---");
    emitter.label_global("__rt_json_decode_mixed");

    // Frame layout (rbp-relative, 96 bytes reserved):
    //   [rbp - 8]  = saved input ptr (rax)
    //   [rbp - 16] = saved input len (rdx)
    //   [rbp - 24] = saved first byte (in low 8 bits)
    //   [rbp - 32] = decoded ptr
    //   [rbp - 40] = decoded len
    //   [rbp - 72] = 32-byte scratch buffer (rbp - 72 .. rbp - 41)
    //   [rbp - 80] = trimmed raw ptr
    //   [rbp - 88] = trimmed raw len
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the structural decoder
    emitter.instruction("sub rsp, 96");                                         // reserve local slots while keeping runtime calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source pointer for downstream classification
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source length for downstream classification

    // Skip leading whitespace and capture the first non-whitespace byte.
    emitter.instruction("xor rcx, rcx");                                        // initialize the source index for the whitespace skip
    emitter.instruction("call __rt_json_skip_ws");                              // advance to the first non-whitespace source byte
    emitter.instruction("cmp rcx, rdx");                                        // did the input contain only JSON whitespace?
    emitter.instruction("jge __rt_json_decode_mixed_syntax_error");             // empty / all-whitespace input is invalid JSON
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load the next byte
    emitter.instruction("mov BYTE PTR [rbp - 24], r8b");                        // park the first non-whitespace byte for the post-decode classification

    // Trim the right edge once here so scalar validators and recursive
    // container parsers all see the exact JSON value slice.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source pointer for raw-slice trimming
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the source length for raw-slice trimming
    emitter.instruction("lea r10, [rax + rcx]");                                // trimmed raw pointer = source pointer + first value byte
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // park the trimmed raw pointer for validators and containers
    emitter.instruction("mov r11, rdx");                                        // start the right trim at the original end
    emitter.label("__rt_json_decode_mixed_trim_right_x");
    emitter.instruction("cmp r11, rcx");                                        // has the right edge crossed the first value byte?
    emitter.instruction("jle __rt_json_decode_mixed_trim_done_x");              // stop before underflowing an all-whitespace suffix
    emitter.instruction("lea r10, [r11 - 1]");                                  // candidate trailing byte index
    emitter.instruction("movzx r8, BYTE PTR [rax + r10]");                      // load the candidate trailing byte
    emitter.instruction("cmp r8, 32");                                          // trailing space?
    emitter.instruction("je __rt_json_decode_mixed_trim_step_x");               // drop trailing spaces
    emitter.instruction("cmp r8, 9");                                           // trailing tab?
    emitter.instruction("je __rt_json_decode_mixed_trim_step_x");               // drop trailing tabs
    emitter.instruction("cmp r8, 10");                                          // trailing LF?
    emitter.instruction("je __rt_json_decode_mixed_trim_step_x");               // drop trailing newlines
    emitter.instruction("cmp r8, 13");                                          // trailing CR?
    emitter.instruction("jne __rt_json_decode_mixed_trim_done_x");              // any other byte is part of the value
    emitter.label("__rt_json_decode_mixed_trim_step_x");
    emitter.instruction("sub r11, 1");                                          // shrink the right edge by one whitespace byte
    emitter.instruction("jmp __rt_json_decode_mixed_trim_right_x");             // continue trimming the raw JSON slice
    emitter.label("__rt_json_decode_mixed_trim_done_x");
    emitter.instruction("sub r11, rcx");                                        // trimmed raw length = right edge - left edge
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // park the trimmed raw length

    // Run the legacy decoder on the trimmed slice for string unescaping.
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw pointer for the legacy decoder
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // trimmed raw length for the legacy decoder
    emitter.instruction("call __rt_json_decode");                               // legacy decoder: returns rax=ptr, rdx=len of the decoded slice
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // park the decoded pointer for the boxing dispatch
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // park the decoded length for the boxing dispatch

    // Classify the value based on the saved first byte.
    emitter.instruction("movzx r8, BYTE PTR [rbp - 24]");                       // reload the saved first byte
    emitter.instruction("cmp r8, 34");                                          // '"' → string
    emitter.instruction("je __rt_json_decode_mixed_string");                    // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 116");                                         // 't' → true
    emitter.instruction("je __rt_json_decode_mixed_true");                      // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 102");                                         // 'f' → false
    emitter.instruction("je __rt_json_decode_mixed_false");                     // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 110");                                         // 'n' → null
    emitter.instruction("je __rt_json_decode_mixed_null");                      // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 91");                                          // '[' → array
    emitter.instruction("je __rt_json_decode_mixed_array");                     // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 123");                                         // '{' → object
    emitter.instruction("je __rt_json_decode_mixed_object");                    // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 45");                                          // '-' → number
    emitter.instruction("je __rt_json_decode_mixed_number");                    // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 48");                                          // '0'..
    emitter.instruction("jl __rt_json_decode_mixed_syntax_error");              // garbage is invalid JSON
    emitter.instruction("cmp r8, 57");                                          // ..'9'
    emitter.instruction("jle __rt_json_decode_mixed_number");                   // branch on the current JSON decoder condition
    emitter.instruction("jmp __rt_json_decode_mixed_syntax_error");             // anything else is invalid JSON

    // -- malformed input → error signal for the json_decode wrapper --
    emitter.label("__rt_json_decode_mixed_syntax_error");
    emitter.instruction("mov rax, 4");                                          // JSON_ERROR_SYNTAX
    emitter.instruction("call __rt_json_throw_error");                          // record syntax error and throw when requested
    emitter.instruction("xor rax, rax");                                        // return null-signal to the json_decode wrapper
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_error_done");
    emitter.instruction("xor rax, rax");                                        // propagate an error already recorded by a nested parser
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    // -- validate and box a decoded JSON string --
    emitter.label("__rt_json_decode_mixed_string");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw string pointer
    emitter.instruction("mov QWORD PTR [rip + _json_validate_ptr], rax");       // publish validator source pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // trimmed raw string length
    emitter.instruction("mov QWORD PTR [rip + _json_validate_len], rax");       // publish validator source length
    emitter.instruction("mov QWORD PTR [rip + _json_validate_idx], 0");         // validate from the start of the trimmed string
    emitter.instruction("call __rt_json_validate_string_x");                    // validate escapes, controls, and UTF-16 before boxing
    emitter.instruction("test rax, rax");                                       // non-zero means the string syntax is valid
    emitter.instruction("je __rt_json_decode_mixed_error_done");                // validation helper already recorded the JSON error
    emitter.instruction("mov r10, QWORD PTR [rip + _json_validate_idx]");       // validator cursor after the decoded string
    emitter.instruction("cmp r10, QWORD PTR [rbp - 88]");                       // did the string parser consume the whole JSON value?
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // trailing bytes after the string are invalid
    emitter.instruction("mov rax, 1");                                          // tag = string
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // lo = decoded ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // hi = decoded len
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    // -- array dispatch: depth-check, then decode empty or recursive array --
    emitter.label("__rt_json_decode_mixed_array");
    emitter.instruction("call __rt_json_depth_enter");                          // enforce json_decode depth before parsing the container
    emitter.instruction("mov r10, QWORD PTR [rip + _json_last_error]");         // load any depth error recorded by depth_enter
    emitter.instruction("test r10, r10");                                       // non-zero means the container exceeded depth
    emitter.instruction("jne __rt_json_decode_mixed_error_done");               // depth overflow returns null/throws instead of decoding
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw slice ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // trimmed raw slice len
    emitter.instruction("mov rcx, 1");                                          // skip the leading `[`
    emitter.instruction("sub rdx, 1");                                          // last meaningful index = len - 1 (the `]`)
    emitter.instruction("call __rt_json_skip_ws");                              // advance to array content or the closing bracket
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON decoder condition
    emitter.instruction("jge __rt_json_decode_mixed_array_empty");              // branch on the current JSON decoder condition
    emitter.label("__rt_json_decode_mixed_array_invoke");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw slice ptr (entire `[...]` slice)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // trimmed raw slice length
    emitter.instruction("call __rt_json_decode_mixed_array_real");              // recursively decode each element; returns rax = Mixed* or 0 on error
    emitter.instruction("test rax, rax");                                       // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_mixed_error_done");                // structural decode failed after recording a JSON error
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the boxed array across depth_exit
    emitter.instruction("call __rt_json_depth_exit");                           // leave the current JSON array depth
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // restore the boxed array result
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_array_empty");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw array pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // trimmed raw array length
    emitter.instruction("cmp rdx, 2");                                          // an empty array still needs `[` and `]`
    emitter.instruction("jl __rt_json_decode_mixed_syntax_error");              // reject truncated array input
    emitter.instruction("lea rcx, [rdx - 1]");                                  // closing bracket index
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load the final raw array byte
    emitter.instruction("cmp r8, 93");                                          // expect `]`
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // trailing or malformed bytes after an empty array are invalid
    emitter.instruction("mov rdi, 0");                                          // capacity = 0
    emitter.instruction("mov rsi, 8");                                          // elem_size = 8 (Mixed-pointer slots)
    emitter.instruction("call __rt_array_new");                                 // returns rax = array pointer
    emitter.instruction("mov rdi, rax");                                        // payload = array pointer
    emitter.instruction("mov rax, 4");                                          // tag = indexed array
    emitter.instruction("xor rsi, rsi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the boxed array across depth_exit
    emitter.instruction("call __rt_json_depth_exit");                           // leave the current JSON array depth
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // restore the boxed array result
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    // -- object dispatch: depth-check, then decode empty or recursive object --
    emitter.label("__rt_json_decode_mixed_object");
    emitter.instruction("call __rt_json_depth_enter");                          // enforce json_decode depth before parsing the container
    emitter.instruction("mov r10, QWORD PTR [rip + _json_last_error]");         // load any depth error recorded by depth_enter
    emitter.instruction("test r10, r10");                                       // non-zero means the container exceeded depth
    emitter.instruction("jne __rt_json_decode_mixed_error_done");               // depth overflow returns null/throws instead of decoding
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw slice ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // trimmed raw slice len
    emitter.instruction("mov rcx, 1");                                          // skip the leading `{`
    emitter.instruction("sub rdx, 1");                                          // last meaningful index = len - 1 (the `}`)
    emitter.instruction("call __rt_json_skip_ws");                              // advance to object content or the closing brace
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON decoder condition
    emitter.instruction("jge __rt_json_decode_mixed_object_empty");             // branch on the current JSON decoder condition
    emitter.label("__rt_json_decode_mixed_object_invoke");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw slice ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // trimmed raw slice length
    emitter.instruction("call __rt_json_decode_mixed_object_real");             // recursively decode each pair
    emitter.instruction("test rax, rax");                                       // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_mixed_error_done");                // structural decode failed after recording a JSON error
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the boxed object across depth_exit
    emitter.instruction("call __rt_json_depth_exit");                           // leave the current JSON object depth
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // restore the boxed object result
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_object_empty");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw object pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // trimmed raw object length
    emitter.instruction("cmp rdx, 2");                                          // an empty object still needs `{` and `}`
    emitter.instruction("jl __rt_json_decode_mixed_syntax_error");              // reject truncated object input
    emitter.instruction("lea rcx, [rdx - 1]");                                  // closing brace index
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load the final raw object byte
    emitter.instruction("cmp r8, 125");                                         // expect `}`
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // trailing or malformed bytes after an empty object are invalid
    emitter.instruction("mov rdi, 0");                                          // capacity = 0
    emitter.instruction("mov rsi, 7");                                          // value_type = 7 (boxed mixed slots)
    emitter.instruction("call __rt_hash_new");                                  // returns rax = hash pointer
    emitter.instruction("mov rdi, rax");                                        // payload = hash pointer
    // Honor the json_decode `$associative` flag: 0 → stdClass, non-zero → assoc.
    emitter.instruction("mov r10, QWORD PTR [rip + _json_decode_assoc]");       // load the assoc flag
    emitter.instruction("test r10, r10");                                       // zero → stdClass dispatch
    emitter.instruction("je __rt_json_decode_mixed_object_empty_stdclass_x");   // wrap the empty hash in a stdClass instance
    emitter.instruction("mov rax, 5");                                          // tag = associative array
    emitter.instruction("xor rsi, rsi");                                        // mixed_from_value high word unused for assoc payload
    emitter.instruction("call __rt_mixed_from_value");                          // box as Mixed(assoc)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the boxed assoc result across depth_exit
    emitter.instruction("call __rt_json_depth_exit");                           // leave the current JSON object depth
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // restore the boxed assoc result
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_object_empty_stdclass_x");
    // rdi already holds the empty hash pointer (SysV first arg) for stdclass_from_hash.
    emitter.instruction("call __rt_stdclass_from_hash");                        // rax = freshly allocated stdClass owning the empty hash
    emitter.instruction("mov rdi, rax");                                        // shift the stdClass pointer into the mixed_from_value low-word slot
    emitter.instruction("mov rax, 6");                                          // tag = object
    emitter.instruction("xor rsi, rsi");                                        // mixed_from_value high word unused for object payload
    emitter.instruction("call __rt_mixed_from_value");                          // box as Mixed(object)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the boxed stdClass result across depth_exit
    emitter.instruction("call __rt_json_depth_exit");                           // leave the current JSON object depth
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // restore the boxed stdClass result
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_true");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw literal pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // trimmed raw literal length
    emitter.instruction("cmp rdx, 4");                                          // `true` must be exactly four bytes
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject trailing literal junk
    emitter.instruction("movzx r8, BYTE PTR [rax + 1]");                        // load `true` byte 1
    emitter.instruction("cmp r8, 114");                                         // expect 'r'
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject malformed literal
    emitter.instruction("movzx r8, BYTE PTR [rax + 2]");                        // load `true` byte 2
    emitter.instruction("cmp r8, 117");                                         // expect 'u'
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject malformed literal
    emitter.instruction("movzx r8, BYTE PTR [rax + 3]");                        // load `true` byte 3
    emitter.instruction("cmp r8, 101");                                         // expect 'e'
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject malformed literal
    emitter.instruction("mov rax, 3");                                          // tag = bool
    emitter.instruction("mov rdi, 1");                                          // lo = 1
    emitter.instruction("xor rsi, rsi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_false");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw literal pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // trimmed raw literal length
    emitter.instruction("cmp rdx, 5");                                          // `false` must be exactly five bytes
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject trailing literal junk
    emitter.instruction("movzx r8, BYTE PTR [rax + 1]");                        // load `false` byte 1
    emitter.instruction("cmp r8, 97");                                          // expect 'a'
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject malformed literal
    emitter.instruction("movzx r8, BYTE PTR [rax + 2]");                        // load `false` byte 2
    emitter.instruction("cmp r8, 108");                                         // expect 'l'
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject malformed literal
    emitter.instruction("movzx r8, BYTE PTR [rax + 3]");                        // load `false` byte 3
    emitter.instruction("cmp r8, 115");                                         // expect 's'
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject malformed literal
    emitter.instruction("movzx r8, BYTE PTR [rax + 4]");                        // load `false` byte 4
    emitter.instruction("cmp r8, 101");                                         // expect 'e'
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject malformed literal
    emitter.instruction("mov rax, 3");                                          // tag = bool
    emitter.instruction("xor rdi, rdi");                                        // lo = 0
    emitter.instruction("xor rsi, rsi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_null");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw literal pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // trimmed raw literal length
    emitter.instruction("cmp rdx, 4");                                          // `null` must be exactly four bytes
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject trailing literal junk
    emitter.instruction("movzx r8, BYTE PTR [rax + 1]");                        // load `null` byte 1
    emitter.instruction("cmp r8, 117");                                         // expect 'u'
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject malformed literal
    emitter.instruction("movzx r8, BYTE PTR [rax + 2]");                        // load `null` byte 2
    emitter.instruction("cmp r8, 108");                                         // expect 'l'
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject malformed literal
    emitter.instruction("movzx r8, BYTE PTR [rax + 3]");                        // load `null` byte 3
    emitter.instruction("cmp r8, 108");                                         // expect 'l'
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // reject malformed literal
    emitter.instruction("mov rax, 8");                                          // tag = null
    emitter.instruction("xor rdi, rdi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("xor rsi, rsi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    // -- number: scan for '.', 'e', 'E' to choose int vs float --
    emitter.label("__rt_json_decode_mixed_number");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // trimmed raw number pointer
    emitter.instruction("mov QWORD PTR [rip + _json_validate_ptr], rax");       // publish validator source pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // trimmed raw number length
    emitter.instruction("mov QWORD PTR [rip + _json_validate_len], rax");       // publish validator source length
    emitter.instruction("mov QWORD PTR [rip + _json_validate_idx], 0");         // validate from the start of the number
    emitter.instruction("call __rt_json_validate_number_x");                    // validate RFC 8259 number grammar before numeric conversion
    emitter.instruction("test rax, rax");                                       // non-zero means the number syntax is valid
    emitter.instruction("je __rt_json_decode_mixed_error_done");                // validation helper already recorded the JSON error
    emitter.instruction("mov r10, QWORD PTR [rip + _json_validate_idx]");       // validator cursor after the number
    emitter.instruction("cmp r10, QWORD PTR [rbp - 88]");                       // did the number parser consume the whole JSON value?
    emitter.instruction("jne __rt_json_decode_mixed_syntax_error");             // trailing bytes after the number are invalid
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // decoded ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // decoded len
    emitter.instruction("xor rcx, rcx");                                        // scan index
    emitter.instruction("xor r9, r9");                                          // is_float flag
    emitter.label("__rt_json_decode_mixed_number_scan");
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON decoder condition
    emitter.instruction("jge __rt_json_decode_mixed_number_decided");           // branch on the current JSON decoder condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON decoder state
    emitter.instruction("cmp r8, 46");                                          // '.'
    emitter.instruction("je __rt_json_decode_mixed_number_set_float");          // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 101");                                         // 'e'
    emitter.instruction("je __rt_json_decode_mixed_number_set_float");          // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 69");                                          // 'E'
    emitter.instruction("je __rt_json_decode_mixed_number_set_float");          // branch on the current JSON decoder condition
    emitter.instruction("add rcx, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_mixed_number_scan");              // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_number_set_float");
    emitter.instruction("mov r9, 1");                                           // load or prepare JSON decoder state
    emitter.label("__rt_json_decode_mixed_number_decided");
    emitter.instruction("test r9, r9");                                         // is_float flag
    emitter.instruction("jne __rt_json_decode_mixed_number_float");             // float grammar wins immediately

    // -- integer-grammar overflow detection --
    // Runs unconditionally so the runtime never wraps through __rt_atoi.
    // Length-then-lex compare against the threshold strings is safe because
    // the fused number validator rejects RFC 8259 leading zeros, so equal-length
    // leading-zero-free decimal compares lexicographically the same as
    // numerically. On overflow, JSON_BIGINT_AS_STRING selects between the
    // preserved-digit Mixed(string) (flag set) and PHP's default Mixed(float)
    // coercion (flag clear).
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // decoded ptr (length/lex input)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // decoded len
    emitter.instruction("movzx r8, BYTE PTR [rax]");                            // first byte: '-' selects negative thresholds
    emitter.instruction("cmp r8, 45");                                          // '-' (ASCII 45)
    emitter.instruction("je __rt_json_decode_mixed_number_overflow_neg");       // negative threshold path
    // -- positive: threshold "9223372036854775807" (19 bytes) --
    emitter.instruction("cmp rdx, 19");                                         // compare length against 19
    emitter.instruction("jl __rt_json_decode_mixed_number_int_atoi");           // < 19 digits → fits in i64
    emitter.instruction("jg __rt_json_decode_mixed_number_overflow");           // > 19 digits → guaranteed overflow
    emitter.instruction("lea r11, [rip + _json_int_max_str]");                  // threshold address
    emitter.instruction("xor rcx, rcx");                                        // lex-compare cursor
    emitter.label("__rt_json_decode_mixed_number_overflow_pos_lex");
    emitter.instruction("cmp rcx, 19");                                         // walked all 19 threshold bytes?
    emitter.instruction("jge __rt_json_decode_mixed_number_int_atoi");          // exactly == threshold → fits (PHP_INT_MAX)
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // token byte
    emitter.instruction("movzx r9, BYTE PTR [r11 + rcx]");                      // threshold byte
    emitter.instruction("cmp r8, r9");                                          // lex compare
    emitter.instruction("jg __rt_json_decode_mixed_number_overflow");           // token > threshold lex → overflow
    emitter.instruction("jl __rt_json_decode_mixed_number_int_atoi");           // token < threshold lex → fits
    emitter.instruction("add rcx, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_json_decode_mixed_number_overflow_pos_lex");  // next byte
    // -- negative: threshold "-9223372036854775808" (20 bytes incl '-') --
    emitter.label("__rt_json_decode_mixed_number_overflow_neg");
    emitter.instruction("cmp rdx, 20");                                         // compare length against 20
    emitter.instruction("jl __rt_json_decode_mixed_number_int_atoi");           // < 20 chars → fits in i64
    emitter.instruction("jg __rt_json_decode_mixed_number_overflow");           // > 20 chars → overflow
    emitter.instruction("lea r11, [rip + _json_int_min_str]");                  // threshold address
    emitter.instruction("xor rcx, rcx");                                        // lex-compare cursor
    emitter.label("__rt_json_decode_mixed_number_overflow_neg_lex");
    emitter.instruction("cmp rcx, 20");                                         // walked all 20 threshold bytes?
    emitter.instruction("jge __rt_json_decode_mixed_number_int_atoi");          // exactly == threshold → fits (PHP_INT_MIN)
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // token byte
    emitter.instruction("movzx r9, BYTE PTR [r11 + rcx]");                      // threshold byte
    emitter.instruction("cmp r8, r9");                                          // lex compare
    emitter.instruction("jg __rt_json_decode_mixed_number_overflow");           // |token| > |threshold| → overflow
    emitter.instruction("jl __rt_json_decode_mixed_number_int_atoi");           // |token| < |threshold| → fits
    emitter.instruction("add rcx, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_json_decode_mixed_number_overflow_neg_lex");  // next byte
    // -- overflow: JSON_BIGINT_AS_STRING selects between string and float --
    emitter.label("__rt_json_decode_mixed_number_overflow");
    emitter.instruction("mov r10, QWORD PTR [rip + _json_active_flags]");       // load the active flag bitmask
    emitter.instruction("test r10, 2");                                         // JSON_BIGINT_AS_STRING bit
    emitter.instruction("jne __rt_json_decode_mixed_number_bigint_string");     // flag set → preserve digits as string
    emitter.instruction("jmp __rt_json_decode_mixed_number_float");             // flag clear → fall back to float (PHP default)
    // -- box the saved decoded slice as Mixed(string) --
    emitter.label("__rt_json_decode_mixed_number_bigint_string");
    emitter.instruction("mov rax, 1");                                          // tag = string
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // ptr = decoded slice (original digits)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // len = decoded slice
    emitter.instruction("call __rt_mixed_from_value");                          // box & persist the digits as a Mixed(string) cell
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    // Integer path: __rt_atoi expects rax=ptr, rdx=len → returns rax
    emitter.label("__rt_json_decode_mixed_number_int_atoi");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // decoded ptr (atoi input)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // decoded len
    emitter.instruction("call __rt_atoi");                                      // parse the decimal slice as a 64-bit integer
    emitter.instruction("mov rdi, rax");                                        // payload = parsed integer
    emitter.instruction("mov rax, 0");                                          // tag = int
    emitter.instruction("xor rsi, rsi");                                        // mixed_from_value high word unused for int payload
    emitter.instruction("call __rt_mixed_from_value");                          // box as Mixed(int)
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    // Float path: copy the decoded slice into a 32-byte stack buffer with a
    // trailing NUL, then call libc atof which expects a C string.
    emitter.label("__rt_json_decode_mixed_number_float");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // decoded ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // decoded len
    emitter.instruction("lea r10, [rbp - 72]");                                 // scratch buffer base (32 bytes)
    emitter.instruction("xor rcx, rcx");                                        // update the JSON decoder cursor or counter
    emitter.label("__rt_json_decode_mixed_float_copy");
    emitter.instruction("cmp rcx, rdx");                                        // copied every byte?
    emitter.instruction("jge __rt_json_decode_mixed_float_copy_done");          // branch on the current JSON decoder condition
    emitter.instruction("cmp rcx, 31");                                         // bound to 31 + NUL
    emitter.instruction("jge __rt_json_decode_mixed_float_copy_done");          // branch on the current JSON decoder condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON decoder state
    emitter.instruction("mov BYTE PTR [r10 + rcx], r8b");                       // load or prepare JSON decoder state
    emitter.instruction("add rcx, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_mixed_float_copy");               // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_float_copy_done");
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0");                         // append the NUL terminator atof needs
    emitter.instruction("mov rdi, r10");                                        // pass the C-string pointer to atof in rdi
    emitter.bl_c("atof");                                                       // libc atof → xmm0 = double
    emitter.instruction("movq rdi, xmm0");                                      // move the double bits into the integer payload register
    emitter.instruction("mov rax, 2");                                          // tag = float
    emitter.instruction("xor rsi, rsi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper

    emitter.label("__rt_json_decode_mixed_done");
    emitter.instruction("mov rsp, rbp");                                        // unwind the scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed pointer in rax
}
