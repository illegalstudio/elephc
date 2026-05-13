use crate::codegen::emit::Emitter;

/// ARM64 implementation of `__rt_json_decode_mixed`. Emits the structural
/// dispatcher routine; the recursive array/object helpers it calls live
/// in `super::arrays` and `super::objects`.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode_mixed ---");
    emitter.label_global("__rt_json_decode_mixed");

    // Frame layout (96 bytes):
    //   [sp + 0]   = saved input ptr
    //   [sp + 8]   = saved input len
    //   [sp + 16]  = saved first byte (low 8 bits)
    //   [sp + 24]  = decoded ptr (post legacy call)
    //   [sp + 32]  = decoded len
    //   [sp + 40..71] = 32-byte scratch buffer for null-terminated number text
    //   [sp + 72..79] = padding for alignment
    //   [sp + 80]  = saved x29
    //   [sp + 88]  = saved x30
    emitter.instruction("sub sp, sp, #96");                                     // reserve a small scratch frame for the structural decoder
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a stable frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the source pointer for downstream classification
    emitter.instruction("str x2, [sp, #8]");                                    // save the source length for downstream classification

    // Skip leading whitespace and capture the first non-whitespace byte.
    emitter.instruction("mov x9, #0");                                          // initialize the source index for the whitespace skip
    emitter.label("__rt_json_decode_mixed_skip_ws");
    emitter.instruction("cmp x9, x2");                                          // are we past the end of the input?
    emitter.instruction("b.ge __rt_json_decode_mixed_empty");                   // empty / all-whitespace input → Mixed(null)
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the next byte
    emitter.instruction("cmp w10, #32");                                        // space?
    emitter.instruction("b.eq __rt_json_decode_mixed_skip_step");               // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #9");                                         // tab?
    emitter.instruction("b.eq __rt_json_decode_mixed_skip_step");               // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #10");                                        // LF?
    emitter.instruction("b.eq __rt_json_decode_mixed_skip_step");               // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #13");                                        // CR?
    emitter.instruction("b.ne __rt_json_decode_mixed_skip_done");               // any other byte stops the scan
    emitter.label("__rt_json_decode_mixed_skip_step");
    emitter.instruction("add x9, x9, #1");                                      // consume the whitespace byte
    emitter.instruction("b __rt_json_decode_mixed_skip_ws");                    // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_skip_done");
    emitter.instruction("strb w10, [sp, #16]");                                 // park the first non-whitespace byte for the post-decode classification

    // Run the legacy decoder which handles trimming + string escape decoding.
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source pointer for the legacy decoder
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the source length for the legacy decoder
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
    emitter.instruction("cmp w10, #91");                                        // '[' → array (empty check, else passthrough)
    emitter.instruction("b.eq __rt_json_decode_mixed_array");                   // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #123");                                       // '{' → object (empty check, else passthrough)
    emitter.instruction("b.eq __rt_json_decode_mixed_object");                  // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #45");                                        // '-' → number
    emitter.instruction("b.eq __rt_json_decode_mixed_number");                  // branch on the current JSON decoder condition
    emitter.instruction("cmp w10, #48");                                        // '0'..
    emitter.instruction("b.lt __rt_json_decode_mixed_passthrough");             // garbage → return as string
    emitter.instruction("cmp w10, #57");                                        // ..'9'
    emitter.instruction("b.le __rt_json_decode_mixed_number");                  // branch on the current JSON decoder condition
    emitter.instruction("b __rt_json_decode_mixed_passthrough");                // anything else → string

    // -- empty / all-whitespace input → Mixed(null) --
    emitter.label("__rt_json_decode_mixed_empty");
    emitter.instruction("mov x0, #8");                                          // tag = null
    emitter.instruction("mov x1, #0");                                          // lo = 0
    emitter.instruction("mov x2, #0");                                          // hi = 0
    emitter.instruction("bl __rt_mixed_from_value");                            // box a Mixed(null) cell
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    // -- string and non-empty container passthrough → Mixed(str, ptr, len) --
    emitter.label("__rt_json_decode_mixed_string");
    emitter.label("__rt_json_decode_mixed_passthrough");
    emitter.instruction("mov x0, #1");                                          // tag = string
    emitter.instruction("ldr x1, [sp, #24]");                                   // lo = decoded ptr
    emitter.instruction("ldr x2, [sp, #32]");                                   // hi = decoded len
    emitter.instruction("bl __rt_mixed_from_value");                            // box a Mixed(string) cell (the helper persists the bytes)
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    // -- array dispatch: empty → Mixed(array=[]); else passthrough as string --
    // Walks the trimmed slice between the `[` and `]` brackets, accepting only
    // JSON whitespace bytes. If any non-whitespace byte appears, the array
    // contains elements that the structural decoder does not yet handle, so
    // we fall through to the legacy passthrough. Empty arrays box as a
    // genuine Mixed(array) so type observations (gettype, is_array) match
    // PHP semantics.
    emitter.label("__rt_json_decode_mixed_array");
    emitter.instruction("ldr x1, [sp, #24]");                                   // decoded slice ptr (with `[`...`]`)
    emitter.instruction("ldr x2, [sp, #32]");                                   // decoded slice length
    emitter.instruction("mov x9, #1");                                          // skip the leading `[`
    emitter.instruction("sub x10, x2, #1");                                     // last meaningful index = len - 1 (the `]`)
    emitter.label("__rt_json_decode_mixed_array_scan");
    emitter.instruction("cmp x9, x10");                                         // have we reached the closing bracket?
    emitter.instruction("b.ge __rt_json_decode_mixed_array_empty");             // only whitespace inside → empty array
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load the next interior byte
    emitter.instruction("cmp w11, #32");                                        // space?
    emitter.instruction("b.eq __rt_json_decode_mixed_array_step");              // branch on the current JSON decoder condition
    emitter.instruction("cmp w11, #9");                                         // tab?
    emitter.instruction("b.eq __rt_json_decode_mixed_array_step");              // branch on the current JSON decoder condition
    emitter.instruction("cmp w11, #10");                                        // LF?
    emitter.instruction("b.eq __rt_json_decode_mixed_array_step");              // branch on the current JSON decoder condition
    emitter.instruction("cmp w11, #13");                                        // CR?
    emitter.instruction("b.ne __rt_json_decode_mixed_array_invoke");            // any other byte → contents present → invoke the recursive parser
    emitter.label("__rt_json_decode_mixed_array_step");
    emitter.instruction("add x9, x9, #1");                                      // update the JSON decoder cursor or counter
    emitter.instruction("b __rt_json_decode_mixed_array_scan");                 // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_array_invoke");
    emitter.instruction("ldr x1, [sp, #24]");                                   // decoded slice ptr (entire `[...]` slice)
    emitter.instruction("ldr x2, [sp, #32]");                                   // decoded slice length
    emitter.instruction("bl __rt_json_decode_mixed_array_real");                // recursively decode each element; returns x0 = Mixed* or 0 on error
    emitter.instruction("cbz x0, __rt_json_decode_mixed_passthrough");          // structural decode failed → fall back to the legacy string passthrough
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_array_empty");
    emitter.instruction("mov x0, #0");                                          // capacity = 0
    emitter.instruction("mov x1, #8");                                          // elem_size = 8 (Mixed-pointer slots)
    emitter.instruction("bl __rt_array_new");                                   // allocate the empty indexed array
    emitter.instruction("mov x1, x0");                                          // payload = array pointer
    emitter.instruction("mov x0, #4");                                          // tag = indexed array
    emitter.instruction("mov x2, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("bl __rt_mixed_from_value");                            // box as Mixed(array)
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    // -- object dispatch: empty `{}` → Mixed(assoc=hash); else passthrough --
    emitter.label("__rt_json_decode_mixed_object");
    emitter.instruction("ldr x1, [sp, #24]");                                   // decoded slice ptr (with `{`...`}`)
    emitter.instruction("ldr x2, [sp, #32]");                                   // decoded slice length
    emitter.instruction("mov x9, #1");                                          // skip the leading `{`
    emitter.instruction("sub x10, x2, #1");                                     // last meaningful index = len - 1 (the `}`)
    emitter.label("__rt_json_decode_mixed_object_scan");
    emitter.instruction("cmp x9, x10");                                         // check the current JSON decoder condition
    emitter.instruction("b.ge __rt_json_decode_mixed_object_empty");            // branch on the current JSON decoder condition
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load or prepare JSON decoder state
    emitter.instruction("cmp w11, #32");                                        // check the current JSON decoder condition
    emitter.instruction("b.eq __rt_json_decode_mixed_object_step");             // branch on the current JSON decoder condition
    emitter.instruction("cmp w11, #9");                                         // check the current JSON decoder condition
    emitter.instruction("b.eq __rt_json_decode_mixed_object_step");             // branch on the current JSON decoder condition
    emitter.instruction("cmp w11, #10");                                        // check the current JSON decoder condition
    emitter.instruction("b.eq __rt_json_decode_mixed_object_step");             // branch on the current JSON decoder condition
    emitter.instruction("cmp w11, #13");                                        // check the current JSON decoder condition
    emitter.instruction("b.ne __rt_json_decode_mixed_object_invoke");           // any other byte → invoke recursive parser
    emitter.label("__rt_json_decode_mixed_object_step");
    emitter.instruction("add x9, x9, #1");                                      // update the JSON decoder cursor or counter
    emitter.instruction("b __rt_json_decode_mixed_object_scan");                // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_object_invoke");
    emitter.instruction("ldr x1, [sp, #24]");                                   // decoded slice ptr (entire `{...}` slice)
    emitter.instruction("ldr x2, [sp, #32]");                                   // decoded slice length
    emitter.instruction("bl __rt_json_decode_mixed_object_real");               // recursively decode each pair; returns x0 = Mixed* or 0 on error
    emitter.instruction("cbz x0, __rt_json_decode_mixed_passthrough");          // structural decode failed → fall back to the legacy string passthrough
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_object_empty");
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
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_object_empty_stdclass");
    emitter.instruction("mov x0, x1");                                          // x0 = empty hash pointer for stdclass_from_hash
    emitter.instruction("bl __rt_stdclass_from_hash");                          // x0 = freshly allocated stdClass owning the empty hash
    emitter.instruction("mov x1, x0");                                          // shift the stdClass pointer into the mixed_from_value low-word slot
    emitter.instruction("mov x0, #6");                                          // tag = object
    emitter.instruction("mov x2, #0");                                          // mixed_from_value high word unused for object payload
    emitter.instruction("bl __rt_mixed_from_value");                            // box as Mixed(object)
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_true");
    emitter.instruction("mov x0, #3");                                          // tag = bool
    emitter.instruction("mov x1, #1");                                          // lo = 1 (true)
    emitter.instruction("mov x2, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("bl __rt_mixed_from_value");                            // call the mixed from value helper
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_false");
    emitter.instruction("mov x0, #3");                                          // tag = bool
    emitter.instruction("mov x1, #0");                                          // lo = 0 (false)
    emitter.instruction("mov x2, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("bl __rt_mixed_from_value");                            // call the mixed from value helper
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_null");
    emitter.instruction("mov x0, #8");                                          // tag = null
    emitter.instruction("mov x1, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("mov x2, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("bl __rt_mixed_from_value");                            // call the mixed from value helper
    emitter.instruction("b __rt_json_decode_mixed_done");                       // continue in the JSON decoder control path

    // -- number: scan for '.', 'e', 'E' to choose int vs float --
    emitter.label("__rt_json_decode_mixed_number");
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
    // the validator pre-pass rejects RFC 8259 leading zeros, which means
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
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the scratch frame
    emitter.instruction("ret");                                                 // return the boxed Mixed pointer in x0
}
