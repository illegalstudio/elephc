//! Purpose:
//! Emits AArch64 structural `json_decode()` Mixed decoder helpers.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen::runtime::system` during runtime emission.
//!
//! Key details:
//! - AArch64 decoder state machine must stay ABI-compatible with shared JSON parser state.

use crate::codegen::emit::Emitter;

/// ARM64 implementation of `__rt_json_decode_mixed`. Emits the structural
/// dispatcher routine; the recursive array/object helpers it calls live
/// in `super::arrays` and `super::objects`.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode_mixed ---");
    emitter.label_global("__rt_json_decode_mixed");

    // Frame layout (112 bytes):
    //   [sp + 0]   = saved input ptr
    //   [sp + 8]   = saved input len
    //   [sp + 16]  = saved first byte (low 8 bits)
    //   [sp + 24]  = decoded ptr (post legacy call)
    //   [sp + 32]  = decoded len
    //   [sp + 40..71] = 32-byte scratch buffer for null-terminated number text
    //   [sp + 72]  = trimmed raw ptr
    //   [sp + 80]  = trimmed raw len
    //   [sp + 88]  = saved result across depth_exit calls
    //   [sp + 96]  = saved x29
    //   [sp + 104] = saved x30
    emitter.instruction("sub sp, sp, #112");                                    // reserve a scratch frame for the checked structural decoder
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish a stable frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the source pointer for downstream classification
    emitter.instruction("str x2, [sp, #8]");                                    // save the source length for downstream classification

    // Skip leading whitespace and capture the first non-whitespace byte.
    emitter.instruction("mov x9, #0");                                          // initialize the source index for the whitespace skip
    emitter.instruction("bl __rt_json_skip_ws");                                // advance to the first non-whitespace source byte
    emitter.instruction("cmp x9, x2");                                          // did the input contain only JSON whitespace?
    emitter.instruction("b.ge __rt_json_decode_mixed_syntax_error");            // empty / all-whitespace input is invalid JSON
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the next byte
    emitter.instruction("strb w10, [sp, #16]");                                 // park the first non-whitespace byte for the post-decode classification

    // Trim the right edge once here so scalar validators and recursive
    // container parsers all see the exact JSON value slice.
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source pointer for raw-slice trimming
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the source length for raw-slice trimming
    emitter.instruction("add x11, x1, x9");                                     // trimmed raw pointer = source pointer + first non-whitespace index
    emitter.instruction("str x11, [sp, #72]");                                  // park the trimmed raw pointer for validators and containers
    emitter.instruction("mov x12, x2");                                         // start the right trim at the original end
    emitter.label("__rt_json_decode_mixed_trim_right");
    emitter.instruction("cmp x12, x9");                                         // has the right edge crossed the first value byte?
    emitter.instruction("b.le __rt_json_decode_mixed_trim_done");               // stop before underflowing an all-whitespace suffix
    emitter.instruction("sub x13, x12, #1");                                    // candidate trailing byte index
    emitter.instruction("ldrb w14, [x1, x13]");                                 // load the candidate trailing byte
    emitter.instruction("cmp w14, #32");                                        // trailing space?
    emitter.instruction("b.eq __rt_json_decode_mixed_trim_step");               // drop trailing spaces
    emitter.instruction("cmp w14, #9");                                         // trailing tab?
    emitter.instruction("b.eq __rt_json_decode_mixed_trim_step");               // drop trailing tabs
    emitter.instruction("cmp w14, #10");                                        // trailing LF?
    emitter.instruction("b.eq __rt_json_decode_mixed_trim_step");               // drop trailing newlines
    emitter.instruction("cmp w14, #13");                                        // trailing CR?
    emitter.instruction("b.ne __rt_json_decode_mixed_trim_done");               // any other byte is part of the value
    emitter.label("__rt_json_decode_mixed_trim_step");
    emitter.instruction("sub x12, x12, #1");                                    // shrink the right edge by one whitespace byte
    emitter.instruction("b __rt_json_decode_mixed_trim_right");                 // continue trimming the raw JSON slice
    emitter.label("__rt_json_decode_mixed_trim_done");
    emitter.instruction("sub x12, x12, x9");                                    // trimmed raw length = right edge - left edge
    emitter.instruction("str x12, [sp, #80]");                                  // park the trimmed raw length

    // Run the legacy decoder on the trimmed slice for string unescaping.
    emitter.instruction("ldr x1, [sp, #72]");                                   // trimmed raw pointer for the legacy decoder
    emitter.instruction("ldr x2, [sp, #80]");                                   // trimmed raw length for the legacy decoder
    emitter.instruction("bl __rt_json_decode");                                 // legacy decoder: returns x1=ptr, x2=len of the decoded slice
    emitter.instruction("str x1, [sp, #24]");                                   // park the decoded pointer for the boxing dispatch
    emitter.instruction("str x2, [sp, #32]");                                   // park the decoded length for the boxing dispatch

    // Classify the value based on the saved first byte.
    emitter.instruction("ldrb w10, [sp, #16]");                                 // reload the saved first byte
    emitter.instruction("cmp w10, #34");                                        // '"' → string
    emitter.instruction("b.eq __rt_json_decode_mixed_string");                  // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #116");                                       // 't' → true
    emitter.instruction("b.eq __rt_json_decode_mixed_true");                    // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #102");                                       // 'f' → false
    emitter.instruction("b.eq __rt_json_decode_mixed_false");                   // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #110");                                       // 'n' → null
    emitter.instruction("b.eq __rt_json_decode_mixed_null");                    // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #91");                                        // '[' → array
    emitter.instruction("b.eq __rt_json_decode_mixed_array");                   // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #123");                                       // '{' → object
    emitter.instruction("b.eq __rt_json_decode_mixed_object");                  // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #45");                                        // '-' → number
    emitter.instruction("b.eq __rt_json_decode_mixed_number");                  // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #48");                                        // '0'..
    emitter.instruction("b.lt __rt_json_decode_mixed_syntax_error");            // garbage is invalid JSON
    emitter.instruction("cmp w10, #57");                                        // ..'9'
    emitter.instruction("b.le __rt_json_decode_mixed_number");                  // branch on the current JSON decoder condition
    emitter.instruction("b __rt_json_decode_mixed_syntax_error");               // anything else is invalid JSON

    // -- malformed input → error signal for the json_decode wrapper --
    emitter.label("__rt_json_decode_mixed_syntax_error");
    emitter.instruction("mov x0, #4");                                          // JSON_ERROR_SYNTAX
    emitter.instruction("bl __rt_json_throw_error");                            // record syntax error and throw when requested
    emitter.instruction("mov x0, #0");                                          // return null-signal to the json_decode wrapper
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_error_done");
    emitter.instruction("mov x0, #0");                                          // propagate an error already recorded by a nested parser
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    // -- validate and box a decoded JSON string --
    emitter.label("__rt_json_decode_mixed_string");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_ptr");
    emitter.instruction("ldr x10, [sp, #72]");                                  // trimmed raw string pointer
    emitter.instruction("str x10, [x9]");                                       // publish validator source pointer
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_len");
    emitter.instruction("ldr x10, [sp, #80]");                                  // trimmed raw string length
    emitter.instruction("str x10, [x9]");                                       // publish validator source length
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("str xzr, [x9]");                                       // validate from the start of the trimmed string
    emitter.instruction("bl __rt_json_validate_string");                        // validate escapes, controls, and UTF-16 before boxing
    emitter.instruction("cbz x0, __rt_json_decode_mixed_error_done");           // validation helper already recorded the JSON error
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x10, [x9]");                                       // validator cursor after the decoded string
    emitter.instruction("ldr x11, [sp, #80]");                                  // trimmed raw string length
    emitter.instruction("cmp x10, x11");                                        // did the string parser consume the whole JSON value?
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // trailing bytes after the string are invalid
    emitter.instruction("mov x0, #1");                                          // tag = string
    emitter.instruction("ldr x1, [sp, #24]");                                   // lo = decoded ptr
    emitter.instruction("ldr x2, [sp, #32]");                                   // hi = decoded len
    emitter.instruction("bl __rt_mixed_from_value");                            // box a Mixed(string) cell (the helper persists the bytes)
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    // -- array dispatch: depth-check, then decode empty or recursive array --
    emitter.label("__rt_json_decode_mixed_array");
    emitter.instruction("bl __rt_json_depth_enter");                            // enforce json_decode depth before parsing the container
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_last_error");
    emitter.instruction("ldr x9, [x9]");                                        // load any depth error recorded by depth_enter
    emitter.instruction("cbnz x9, __rt_json_decode_mixed_error_done");          // depth overflow returns null/throws instead of decoding
    emitter.instruction("ldr x1, [sp, #72]");                                   // trimmed raw slice ptr (with `[`...`]`)
    emitter.instruction("ldr x2, [sp, #80]");                                   // trimmed raw slice length
    emitter.instruction("mov x9, #1");                                          // skip the leading `[`
    emitter.instruction("sub x10, x2, #1");                                     // last meaningful index = len - 1 (the `]`)
    emitter.instruction("mov x2, x10");                                         // skip interior whitespace without consuming the closing bracket
    emitter.instruction("bl __rt_json_skip_ws");                                // advance to array content or the closing bracket
    emitter.instruction("cmp x9, x2");                                          // have we reached the closing bracket?
    emitter.instruction("b.ge __rt_json_decode_mixed_array_empty");             // only whitespace inside → empty array
    emitter.label("__rt_json_decode_mixed_array_invoke");
    emitter.instruction("ldr x1, [sp, #72]");                                   // trimmed raw slice ptr (entire `[...]` slice)
    emitter.instruction("ldr x2, [sp, #80]");                                   // trimmed raw slice length
    emitter.instruction("bl __rt_json_decode_mixed_array_real");                // recursively decode each element; returns x0 = Mixed* or 0 on error
    emitter.instruction("cbz x0, __rt_json_decode_mixed_error_done");           // structural decode failed after recording a JSON error
    emitter.instruction("str x0, [sp, #88]");                                   // save the boxed array across depth_exit
    emitter.instruction("bl __rt_json_depth_exit");                             // leave the current JSON array depth
    emitter.instruction("ldr x0, [sp, #88]");                                   // restore the boxed array result
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_array_empty");
    emitter.instruction("ldr x1, [sp, #72]");                                   // trimmed raw array pointer
    emitter.instruction("ldr x2, [sp, #80]");                                   // trimmed raw array length
    emitter.instruction("cmp x2, #2");                                          // an empty array still needs `[` and `]`
    emitter.instruction("b.lt __rt_json_decode_mixed_syntax_error");            // reject truncated array input
    emitter.instruction("sub x9, x2, #1");                                      // closing bracket index
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the final raw array byte
    emitter.instruction("cmp w10, #93");                                        // expect `]`
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // trailing or malformed bytes after an empty array are invalid
    emitter.instruction("mov x0, #0");                                          // capacity = 0
    emitter.instruction("mov x1, #8");                                          // elem_size = 8 (Mixed-pointer slots)
    emitter.instruction("bl __rt_array_new");                                   // allocate the empty indexed array
    emitter.instruction("mov x1, x0");                                          // payload = array pointer
    emitter.instruction("mov x0, #4");                                          // tag = indexed array
    emitter.instruction("mov x2, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("bl __rt_mixed_from_value");                            // box as Mixed(array)
    emitter.instruction("str x0, [sp, #88]");                                   // save the boxed array across depth_exit
    emitter.instruction("bl __rt_json_depth_exit");                             // leave the current JSON array depth
    emitter.instruction("ldr x0, [sp, #88]");                                   // restore the boxed array result
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    // -- object dispatch: depth-check, then decode empty or recursive object --
    emitter.label("__rt_json_decode_mixed_object");
    emitter.instruction("bl __rt_json_depth_enter");                            // enforce json_decode depth before parsing the container
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_last_error");
    emitter.instruction("ldr x9, [x9]");                                        // load any depth error recorded by depth_enter
    emitter.instruction("cbnz x9, __rt_json_decode_mixed_error_done");          // depth overflow returns null/throws instead of decoding
    emitter.instruction("ldr x1, [sp, #72]");                                   // trimmed raw slice ptr (with `{`...`}`)
    emitter.instruction("ldr x2, [sp, #80]");                                   // trimmed raw slice length
    emitter.instruction("mov x9, #1");                                          // skip the leading `{`
    emitter.instruction("sub x10, x2, #1");                                     // last meaningful index = len - 1 (the `}`)
    emitter.instruction("mov x2, x10");                                         // skip interior whitespace without consuming the closing brace
    emitter.instruction("bl __rt_json_skip_ws");                                // advance to object content or the closing brace
    emitter.instruction("cmp x9, x2");                                          // check the current JSON decoder condition
    emitter.instruction("b.ge __rt_json_decode_mixed_object_empty");            // branch on the current JSON decoder condition
    emitter.label("__rt_json_decode_mixed_object_invoke");
    emitter.instruction("ldr x1, [sp, #72]");                                   // trimmed raw slice ptr (entire `{...}` slice)
    emitter.instruction("ldr x2, [sp, #80]");                                   // trimmed raw slice length
    emitter.instruction("bl __rt_json_decode_mixed_object_real");               // recursively decode each pair; returns x0 = Mixed* or 0 on error
    emitter.instruction("cbz x0, __rt_json_decode_mixed_error_done");           // structural decode failed after recording a JSON error
    emitter.instruction("str x0, [sp, #88]");                                   // save the boxed object across depth_exit
    emitter.instruction("bl __rt_json_depth_exit");                             // leave the current JSON object depth
    emitter.instruction("ldr x0, [sp, #88]");                                   // restore the boxed object result
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_object_empty");
    emitter.instruction("ldr x1, [sp, #72]");                                   // trimmed raw object pointer
    emitter.instruction("ldr x2, [sp, #80]");                                   // trimmed raw object length
    emitter.instruction("cmp x2, #2");                                          // an empty object still needs `{` and `}`
    emitter.instruction("b.lt __rt_json_decode_mixed_syntax_error");            // reject truncated object input
    emitter.instruction("sub x9, x2, #1");                                      // closing brace index
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the final raw object byte
    emitter.instruction("cmp w10, #125");                                       // expect `}`
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // trailing or malformed bytes after an empty object are invalid
    emitter.instruction("mov x0, #0");                                          // capacity = 0
    emitter.instruction("mov x1, #7");                                          // value_type = 7 (boxed mixed slots)
    emitter.instruction("bl __rt_hash_new");                                    // allocate the empty hash
    emitter.instruction("mov x1, x0");                                          // payload = hash pointer
    // Honor the json_decode `$associative` flag: 0 → stdClass, non-zero → assoc.
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_decode_assoc");
    emitter.instruction("ldr x9, [x9]");                                        // load the assoc flag
    emitter.instruction("cbz x9, __rt_json_decode_mixed_object_empty_stdclass"); // 0 → wrap in a stdClass instance
    emitter.instruction("mov x0, #5");                                          // tag = associative array
    emitter.instruction("mov x2, #0");                                          // mixed_from_value high word unused for assoc payload
    emitter.instruction("bl __rt_mixed_from_value");                            // box as Mixed(assoc)
    emitter.instruction("str x0, [sp, #88]");                                   // save the boxed assoc result across depth_exit
    emitter.instruction("bl __rt_json_depth_exit");                             // leave the current JSON object depth
    emitter.instruction("ldr x0, [sp, #88]");                                   // restore the boxed assoc result
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_object_empty_stdclass");
    emitter.instruction("mov x0, x1");                                          // x0 = empty hash pointer for stdclass_from_hash
    emitter.instruction("bl __rt_stdclass_from_hash");                          // x0 = freshly allocated stdClass owning the empty hash
    emitter.instruction("mov x1, x0");                                          // shift the stdClass pointer into the mixed_from_value low-word slot
    emitter.instruction("mov x0, #6");                                          // tag = object
    emitter.instruction("mov x2, #0");                                          // mixed_from_value high word unused for object payload
    emitter.instruction("bl __rt_mixed_from_value");                            // box as Mixed(object)
    emitter.instruction("str x0, [sp, #88]");                                   // save the boxed stdClass result across depth_exit
    emitter.instruction("bl __rt_json_depth_exit");                             // leave the current JSON object depth
    emitter.instruction("ldr x0, [sp, #88]");                                   // restore the boxed stdClass result
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_true");
    emitter.instruction("ldr x1, [sp, #72]");                                   // trimmed raw literal pointer
    emitter.instruction("ldr x2, [sp, #80]");                                   // trimmed raw literal length
    emitter.instruction("cmp x2, #4");                                          // `true` must be exactly four bytes
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject trailing literal junk
    emitter.instruction("ldrb w10, [x1, #1]");                                  // load `true` byte 1
    emitter.instruction("cmp w10, #114");                                       // expect 'r'
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject malformed literal
    emitter.instruction("ldrb w10, [x1, #2]");                                  // load `true` byte 2
    emitter.instruction("cmp w10, #117");                                       // expect 'u'
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject malformed literal
    emitter.instruction("ldrb w10, [x1, #3]");                                  // load `true` byte 3
    emitter.instruction("cmp w10, #101");                                       // expect 'e'
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject malformed literal
    emitter.instruction("mov x0, #3");                                          // tag = bool
    emitter.instruction("mov x1, #1");                                          // lo = 1 (true)
    emitter.instruction("mov x2, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("bl __rt_mixed_from_value");                            // call the mixed from value helper
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_false");
    emitter.instruction("ldr x1, [sp, #72]");                                   // trimmed raw literal pointer
    emitter.instruction("ldr x2, [sp, #80]");                                   // trimmed raw literal length
    emitter.instruction("cmp x2, #5");                                          // `false` must be exactly five bytes
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject trailing literal junk
    emitter.instruction("ldrb w10, [x1, #1]");                                  // load `false` byte 1
    emitter.instruction("cmp w10, #97");                                        // expect 'a'
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject malformed literal
    emitter.instruction("ldrb w10, [x1, #2]");                                  // load `false` byte 2
    emitter.instruction("cmp w10, #108");                                       // expect 'l'
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject malformed literal
    emitter.instruction("ldrb w10, [x1, #3]");                                  // load `false` byte 3
    emitter.instruction("cmp w10, #115");                                       // expect 's'
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject malformed literal
    emitter.instruction("ldrb w10, [x1, #4]");                                  // load `false` byte 4
    emitter.instruction("cmp w10, #101");                                       // expect 'e'
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject malformed literal
    emitter.instruction("mov x0, #3");                                          // tag = bool
    emitter.instruction("mov x1, #0");                                          // lo = 0 (false)
    emitter.instruction("mov x2, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("bl __rt_mixed_from_value");                            // call the mixed from value helper
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_null");
    emitter.instruction("ldr x1, [sp, #72]");                                   // trimmed raw literal pointer
    emitter.instruction("ldr x2, [sp, #80]");                                   // trimmed raw literal length
    emitter.instruction("cmp x2, #4");                                          // `null` must be exactly four bytes
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject trailing literal junk
    emitter.instruction("ldrb w10, [x1, #1]");                                  // load `null` byte 1
    emitter.instruction("cmp w10, #117");                                       // expect 'u'
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject malformed literal
    emitter.instruction("ldrb w10, [x1, #2]");                                  // load `null` byte 2
    emitter.instruction("cmp w10, #108");                                       // expect 'l'
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject malformed literal
    emitter.instruction("ldrb w10, [x1, #3]");                                  // load `null` byte 3
    emitter.instruction("cmp w10, #108");                                       // expect 'l'
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // reject malformed literal
    emitter.instruction("mov x0, #8");                                          // tag = null
    emitter.instruction("mov x1, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("mov x2, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("bl __rt_mixed_from_value");                            // call the mixed from value helper
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    // -- number: scan for '.', 'e', 'E' to choose int vs float --
    emitter.label("__rt_json_decode_mixed_number");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_ptr");
    emitter.instruction("ldr x10, [sp, #72]");                                  // trimmed raw number pointer
    emitter.instruction("str x10, [x9]");                                       // publish validator source pointer
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_len");
    emitter.instruction("ldr x10, [sp, #80]");                                  // trimmed raw number length
    emitter.instruction("str x10, [x9]");                                       // publish validator source length
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("str xzr, [x9]");                                       // validate from the start of the number
    emitter.instruction("bl __rt_json_validate_number");                        // validate RFC 8259 number grammar before numeric conversion
    emitter.instruction("cbz x0, __rt_json_decode_mixed_error_done");           // validation helper already recorded the JSON error
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x10, [x9]");                                       // validator cursor after the number
    emitter.instruction("ldr x11, [sp, #80]");                                  // trimmed raw number length
    emitter.instruction("cmp x10, x11");                                        // did the number parser consume the whole JSON value?
    emitter.instruction("b.ne __rt_json_decode_mixed_syntax_error");            // trailing bytes after the number are invalid
    emitter.instruction("ldr x1, [sp, #24]");                                   // decoded ptr
    emitter.instruction("ldr x2, [sp, #32]");                                   // decoded len
    emitter.instruction("mov x9, #0");                                          // scan index
    emitter.instruction("mov w12, #0");                                         // is_float flag
    emitter.label("__rt_json_decode_mixed_number_scan");
    emitter.instruction("cmp x9, x2");                                          // check the current JSON decoder condition
    emitter.instruction("b.ge __rt_json_decode_mixed_number_decided");          // branch on the current JSON decoder condition
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load or prepare JSON decoder state
    emitter.instruction("cmp w10, #46");                                        // '.'?
    emitter.instruction("b.eq __rt_json_decode_mixed_number_set_float");        // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #101");                                       // 'e'?
    emitter.instruction("b.eq __rt_json_decode_mixed_number_set_float");        // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #69");                                        // 'E'?
    emitter.instruction("b.eq __rt_json_decode_mixed_number_set_float");        // branch on the current JSON decoder condition
    emitter.instruction("add x9, x9, #1");                                      // update the JSON decoder cursor or counter
    emitter.instruction("b __rt_json_decode_mixed_number_scan");                // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_number_set_float");
    emitter.instruction("mov w12, #1");                                         // load or prepare JSON decoder state
    emitter.label("__rt_json_decode_mixed_number_decided");
    emitter.instruction("cbnz w12, __rt_json_decode_mixed_number_float");       // float grammar wins immediately

    // -- integer-grammar overflow detection --
    // Runs unconditionally for integer-grammar tokens so the runtime never
    // wraps through __rt_atoi (its imul-based parser has no overflow flag).
    // Length-then-lex compare against the threshold strings is safe because
    // the fused number validator rejects RFC 8259 leading zeros, which means
    // equal-length leading-zero-free decimal compares lexicographically the
    // same as numerically. On overflow, JSON_BIGINT_AS_STRING selects between
    // the preserved-digit Mixed(string) (flag set) and PHP's default
    // Mixed(float) coercion (flag clear).
    emitter.instruction("ldr x1, [sp, #24]");                                   // decoded ptr (length/lex input)
    emitter.instruction("ldr x2, [sp, #32]");                                   // decoded len
    emitter.instruction("ldrb w10, [x1]");                                      // first byte: '-' selects negative thresholds
    emitter.instruction("cmp w10, #45");                                        // '-' (ASCII 45)
    emitter.instruction("b.eq __rt_json_decode_mixed_number_overflow_neg");     // negative threshold path
    // -- positive: threshold "9223372036854775807" (19 bytes) --
    emitter.instruction("cmp x2, #19");                                         // compare length against 19
    emitter.instruction("b.lt __rt_json_decode_mixed_number_int_atoi");         // < 19 digits → fits in i64
    emitter.instruction("b.gt __rt_json_decode_mixed_number_overflow");         // > 19 digits → guaranteed overflow
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_int_max_str");
    emitter.instruction("mov x9, #0");                                          // lex-compare cursor
    emitter.label("__rt_json_decode_mixed_number_overflow_pos_lex");
    emitter.instruction("cmp x9, #19");                                         // walked all 19 threshold bytes?
    emitter.instruction("b.ge __rt_json_decode_mixed_number_int_atoi");         // exactly == threshold → fits (PHP_INT_MAX)
    emitter.instruction("ldrb w12, [x1, x9]");                                  // token byte
    emitter.instruction("ldrb w13, [x11, x9]");                                 // threshold byte
    emitter.instruction("cmp w12, w13");                                        // lex compare
    emitter.instruction("b.gt __rt_json_decode_mixed_number_overflow");         // token > threshold lex → overflow
    emitter.instruction("b.lt __rt_json_decode_mixed_number_int_atoi");         // token < threshold lex → fits
    emitter.instruction("add x9, x9, #1");                                      // advance cursor
    emitter.instruction("b __rt_json_decode_mixed_number_overflow_pos_lex");    // next byte
    // -- negative: threshold "-9223372036854775808" (20 bytes incl '-') --
    emitter.label("__rt_json_decode_mixed_number_overflow_neg");
    emitter.instruction("cmp x2, #20");                                         // compare length against 20
    emitter.instruction("b.lt __rt_json_decode_mixed_number_int_atoi");         // < 20 chars → fits in i64
    emitter.instruction("b.gt __rt_json_decode_mixed_number_overflow");         // > 20 chars → overflow
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_int_min_str");
    emitter.instruction("mov x9, #0");                                          // lex-compare cursor
    emitter.label("__rt_json_decode_mixed_number_overflow_neg_lex");
    emitter.instruction("cmp x9, #20");                                         // walked all 20 threshold bytes?
    emitter.instruction("b.ge __rt_json_decode_mixed_number_int_atoi");         // exactly == threshold → fits (PHP_INT_MIN)
    emitter.instruction("ldrb w12, [x1, x9]");                                  // token byte
    emitter.instruction("ldrb w13, [x11, x9]");                                 // threshold byte
    emitter.instruction("cmp w12, w13");                                        // lex compare
    emitter.instruction("b.gt __rt_json_decode_mixed_number_overflow");         // |token| > |threshold| → overflow
    emitter.instruction("b.lt __rt_json_decode_mixed_number_int_atoi");         // |token| < |threshold| → fits
    emitter.instruction("add x9, x9, #1");                                      // advance cursor
    emitter.instruction("b __rt_json_decode_mixed_number_overflow_neg_lex");    // next byte
    // -- overflow: JSON_BIGINT_AS_STRING selects between string and float --
    emitter.label("__rt_json_decode_mixed_number_overflow");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x9, [x9]");                                        // load the active flag bitmask
    emitter.instruction("tst x9, #2");                                          // JSON_BIGINT_AS_STRING bit
    emitter.instruction("b.ne __rt_json_decode_mixed_number_bigint_string");    // flag set → preserve digits as string
    emitter.instruction("b __rt_json_decode_mixed_number_float");               // flag clear → fall back to float (PHP default)
    // -- box the saved decoded slice as Mixed(string) --
    emitter.label("__rt_json_decode_mixed_number_bigint_string");
    emitter.instruction("mov x0, #1");                                          // tag = string
    emitter.instruction("ldr x1, [sp, #24]");                                   // ptr = decoded slice (original digits)
    emitter.instruction("ldr x2, [sp, #32]");                                   // len = decoded slice
    emitter.instruction("bl __rt_mixed_from_value");                            // box & persist the digits as a Mixed(string) cell
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    // Integer path: __rt_atoi expects x1=ptr, x2=len → returns x0
    emitter.label("__rt_json_decode_mixed_number_int_atoi");
    emitter.instruction("ldr x1, [sp, #24]");                                   // decoded ptr (atoi input)
    emitter.instruction("ldr x2, [sp, #32]");                                   // decoded len
    emitter.instruction("bl __rt_atoi");                                        // parse the decimal slice as a 64-bit integer
    emitter.instruction("mov x1, x0");                                          // payload = parsed integer
    emitter.instruction("mov x0, #0");                                          // tag = int
    emitter.instruction("mov x2, #0");                                          // mixed_from_value high word unused for int payload
    emitter.instruction("bl __rt_mixed_from_value");                            // box as Mixed(int)
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    // Float path: copy the decoded slice into a 32-byte stack buffer with a
    // trailing NUL, then call libc atof which expects a C string.
    emitter.label("__rt_json_decode_mixed_number_float");
    emitter.instruction("ldr x1, [sp, #24]");                                   // decoded ptr
    emitter.instruction("ldr x2, [sp, #32]");                                   // decoded len
    emitter.instruction("add x11, sp, #40");                                    // scratch buffer base (32 bytes available)
    emitter.instruction("mov x9, #0");                                          // load or prepare JSON decoder state
    emitter.label("__rt_json_decode_mixed_float_copy");
    emitter.instruction("cmp x9, x2");                                          // copied every byte?
    emitter.instruction("b.ge __rt_json_decode_mixed_float_copy_done");         // branch on the current JSON decoder condition
    emitter.instruction("cmp x9, #31");                                         // bound the copy to 31 bytes + NUL terminator
    emitter.instruction("b.ge __rt_json_decode_mixed_float_copy_done");         // branch on the current JSON decoder condition
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load or prepare JSON decoder state
    emitter.instruction("strb w10, [x11, x9]");                                 // store updated JSON decoder state
    emitter.instruction("add x9, x9, #1");                                      // update the JSON decoder cursor or counter
    emitter.instruction("b __rt_json_decode_mixed_float_copy");                 // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_float_copy_done");
    emitter.instruction("strb wzr, [x11, x9]");                                 // append the NUL terminator atof needs
    emitter.instruction("mov x0, x11");                                         // pass the C-string pointer to atof in x0
    emitter.bl_c("atof");                                                       // libc atof → d0 = double
    emitter.instruction("fmov x1, d0");                                         // move the double bits into the integer payload register
    emitter.instruction("mov x0, #2");                                          // tag = float
    emitter.instruction("mov x2, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("bl __rt_mixed_from_value");                            // call the mixed from value helper

    emitter.label("__rt_json_decode_mixed_done");
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the scratch frame
    emitter.instruction("ret");                                                 // return the boxed Mixed pointer in x0
}
