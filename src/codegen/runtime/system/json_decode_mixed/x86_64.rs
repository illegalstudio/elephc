use crate::codegen::emit::Emitter;

/// x86_64 implementation of `__rt_json_decode_mixed`. Mirrors the ARM64
/// dispatcher in `super::aarch64`; the recursive array/object helpers
/// live in `super::arrays` and `super::objects`.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode_mixed ---");
    emitter.label_global("__rt_json_decode_mixed");

    // Frame layout (rbp-relative, 80 bytes reserved):
    //   [rbp - 8]  = saved input ptr (rax)
    //   [rbp - 16] = saved input len (rdx)
    //   [rbp - 24] = saved first byte (in low 8 bits)
    //   [rbp - 32] = decoded ptr
    //   [rbp - 40] = decoded len
    //   [rbp - 72] = 32-byte scratch buffer (rbp - 72 .. rbp - 41)
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the structural decoder
    emitter.instruction("sub rsp, 80");                                         // reserve local slots; 80 keeps the call site 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source pointer for downstream classification
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source length for downstream classification

    // Skip leading whitespace and capture the first non-whitespace byte.
    emitter.instruction("xor rcx, rcx");                                        // initialize the source index for the whitespace skip
    emitter.label("__rt_json_decode_mixed_skip_ws");
    emitter.instruction("cmp rcx, rdx");                                        // are we past the end of the input?
    emitter.instruction("jge __rt_json_decode_mixed_empty");                    // empty / all-whitespace input → Mixed(null)
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load the next byte
    emitter.instruction("cmp r8, 32");                                          // space?
    emitter.instruction("je __rt_json_decode_mixed_skip_step");                 // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 9");                                           // tab?
    emitter.instruction("je __rt_json_decode_mixed_skip_step");                 // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 10");                                          // LF?
    emitter.instruction("je __rt_json_decode_mixed_skip_step");                 // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 13");                                          // CR?
    emitter.instruction("jne __rt_json_decode_mixed_skip_done");                // any other byte stops the scan
    emitter.label("__rt_json_decode_mixed_skip_step");
    emitter.instruction("add rcx, 1");                                          // consume the whitespace byte
    emitter.instruction("jmp __rt_json_decode_mixed_skip_ws");                  // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_skip_done");
    emitter.instruction("mov BYTE PTR [rbp - 24], r8b");                        // park the first non-whitespace byte for the post-decode classification

    // Run the legacy decoder which handles trimming + string escape decoding.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source pointer for the legacy decoder
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the source length for the legacy decoder
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
    emitter.instruction("cmp r8, 91");                                          // '[' → array (empty check, else passthrough)
    emitter.instruction("je __rt_json_decode_mixed_array");                     // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 123");                                         // '{' → object (empty check, else passthrough)
    emitter.instruction("je __rt_json_decode_mixed_object");                    // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 45");                                          // '-' → number
    emitter.instruction("je __rt_json_decode_mixed_number");                    // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 48");                                          // '0'..
    emitter.instruction("jl __rt_json_decode_mixed_passthrough");               // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 57");                                          // ..'9'
    emitter.instruction("jle __rt_json_decode_mixed_number");                   // branch on the current JSON decoder condition
    emitter.instruction("jmp __rt_json_decode_mixed_passthrough");              // continue in the JSON decoder control path

    // -- empty input → Mixed(null) --
    emitter.label("__rt_json_decode_mixed_empty");
    emitter.instruction("mov rax, 8");                                          // tag = null
    emitter.instruction("xor rdi, rdi");                                        // lo = 0
    emitter.instruction("xor rsi, rsi");                                        // hi = 0
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    // -- string and non-empty container passthrough --
    emitter.label("__rt_json_decode_mixed_string");
    emitter.label("__rt_json_decode_mixed_passthrough");
    emitter.instruction("mov rax, 1");                                          // tag = string
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // lo = decoded ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // hi = decoded len
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    // -- array dispatch: empty `[]` → Mixed(array=[]); else passthrough --
    emitter.label("__rt_json_decode_mixed_array");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // decoded slice ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // decoded slice len
    emitter.instruction("mov rcx, 1");                                          // skip the leading `[`
    emitter.instruction("mov r9, rdx");                                         // copy length for offset comparison
    emitter.instruction("sub r9, 1");                                           // last meaningful index = len - 1 (the `]`)
    emitter.label("__rt_json_decode_mixed_array_scan");
    emitter.instruction("cmp rcx, r9");                                         // check the current JSON decoder condition
    emitter.instruction("jge __rt_json_decode_mixed_array_empty");              // branch on the current JSON decoder condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON decoder state
    emitter.instruction("cmp r8, 32");                                          // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_mixed_array_step");                // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 9");                                           // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_mixed_array_step");                // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 10");                                          // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_mixed_array_step");                // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 13");                                          // check the current JSON decoder condition
    emitter.instruction("jne __rt_json_decode_mixed_array_invoke");             // non-whitespace inside → invoke recursive parser
    emitter.label("__rt_json_decode_mixed_array_step");
    emitter.instruction("add rcx, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_mixed_array_scan");               // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_array_invoke");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // decoded slice ptr (entire `[...]` slice)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // decoded slice length
    emitter.instruction("call __rt_json_decode_mixed_array_real");              // recursively decode each element; returns rax = Mixed* or 0 on error
    emitter.instruction("test rax, rax");                                       // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_mixed_passthrough");               // structural decode failed → fall back to the legacy string passthrough
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_array_empty");
    emitter.instruction("mov rdi, 0");                                          // capacity = 0
    emitter.instruction("mov rsi, 8");                                          // elem_size = 8 (Mixed-pointer slots)
    emitter.instruction("call __rt_array_new");                                 // returns rax = array pointer
    emitter.instruction("mov rdi, rax");                                        // payload = array pointer
    emitter.instruction("mov rax, 4");                                          // tag = indexed array
    emitter.instruction("xor rsi, rsi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    // -- object dispatch: empty `{}` → Mixed(assoc=hash); else passthrough --
    emitter.label("__rt_json_decode_mixed_object");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // decoded slice ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // decoded slice len
    emitter.instruction("mov rcx, 1");                                          // skip the leading `{`
    emitter.instruction("mov r9, rdx");                                         // load or prepare JSON decoder state
    emitter.instruction("sub r9, 1");                                           // last meaningful index = len - 1 (the `}`)
    emitter.label("__rt_json_decode_mixed_object_scan");
    emitter.instruction("cmp rcx, r9");                                         // check the current JSON decoder condition
    emitter.instruction("jge __rt_json_decode_mixed_object_empty");             // branch on the current JSON decoder condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON decoder state
    emitter.instruction("cmp r8, 32");                                          // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_mixed_object_step");               // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 9");                                           // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_mixed_object_step");               // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 10");                                          // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_mixed_object_step");               // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 13");                                          // check the current JSON decoder condition
    emitter.instruction("jne __rt_json_decode_mixed_object_invoke");            // non-whitespace inside → invoke recursive parser
    emitter.label("__rt_json_decode_mixed_object_step");
    emitter.instruction("add rcx, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_mixed_object_scan");              // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_object_invoke");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // decoded slice ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // decoded slice length
    emitter.instruction("call __rt_json_decode_mixed_object_real");             // recursively decode each pair
    emitter.instruction("test rax, rax");                                       // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_mixed_passthrough");               // structural decode failed → fallback
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_mixed_object_empty");
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
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_object_empty_stdclass_x");
    // rdi already holds the empty hash pointer (SysV first arg) for stdclass_from_hash.
    emitter.instruction("call __rt_stdclass_from_hash");                        // rax = freshly allocated stdClass owning the empty hash
    emitter.instruction("mov rdi, rax");                                        // shift the stdClass pointer into the mixed_from_value low-word slot
    emitter.instruction("mov rax, 6");                                          // tag = object
    emitter.instruction("xor rsi, rsi");                                        // mixed_from_value high word unused for object payload
    emitter.instruction("call __rt_mixed_from_value");                          // box as Mixed(object)
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_true");
    emitter.instruction("mov rax, 3");                                          // tag = bool
    emitter.instruction("mov rdi, 1");                                          // lo = 1
    emitter.instruction("xor rsi, rsi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_false");
    emitter.instruction("mov rax, 3");                                          // tag = bool
    emitter.instruction("xor rdi, rdi");                                        // lo = 0
    emitter.instruction("xor rsi, rsi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_mixed_null");
    emitter.instruction("mov rax, 8");                                          // tag = null
    emitter.instruction("xor rdi, rdi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("xor rsi, rsi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("jmp __rt_json_decode_mixed_done");                     // continue in the JSON decoder control path

    // -- number: scan for '.', 'e', 'E' to choose int vs float --
    emitter.label("__rt_json_decode_mixed_number");
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
    // the validator pre-pass rejects RFC 8259 leading zeros, so equal-length
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
