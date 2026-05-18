//! Purpose:
//! Emits shared JSON object decoding helpers for assoc arrays and stdClass results.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen::runtime::system` during runtime emission.
//!
//! Key details:
//! - Object decoding must honor the caller associative flag and keep property values boxed as Mixed.

use crate::codegen::emit::Emitter;

/// __rt_json_decode_mixed_object_real (ARM64): recursive-descent parser for
/// non-empty JSON objects. Walks the slice between the leading `{` and
/// trailing `}`, parses each key (a JSON string) and value (any JSON
/// value, recursively decoded), and inserts the pair into a hash via
/// __rt_hash_set. Result boxes as Mixed(tag=5, lo=hash_ptr).
///
/// Input:  x1 = slice ptr (with leading `{` and trailing `}`),
///         x2 = slice length
/// Output: x0 = Mixed* on success, 0 on parse error after recording JSON state
pub(super) fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode_mixed_object_real ---");
    emitter.label_global("__rt_json_decode_mixed_object_real");

    // Frame layout (80 bytes):
    //   [sp + 0]  = slice_ptr
    //   [sp + 8]  = slice_len
    //   [sp + 16] = cursor
    //   [sp + 24] = hash_ptr
    //   [sp + 32] = key_start (saved across the recursive key decode)
    //   [sp + 40] = key Mixed* (saved across the recursive value decode)
    //   [sp + 48] = value_start / value Mixed* during insertion
    //   [sp + 56] = after_comma flag
    //   [sp + 64] = saved x29
    //   [sp + 72] = saved x30
    emitter.instruction("sub sp, sp, #80");                                     // update the JSON decoder cursor or counter
    emitter.instruction("stp x29, x30, [sp, #64]");                             // store updated JSON decoder state
    emitter.instruction("add x29, sp, #64");                                    // update the JSON decoder cursor or counter
    emitter.instruction("str x1, [sp, #0]");                                    // store updated JSON decoder state
    emitter.instruction("str x2, [sp, #8]");                                    // store updated JSON decoder state

    // Allocate the destination hash with capacity 4 and value_type 7
    // (boxed mixed slots — every value is a Mixed pointer).
    emitter.instruction("mov x0, #4");                                          // initial capacity
    emitter.instruction("mov x1, #7");                                          // value_type = 7 (boxed mixed)
    emitter.instruction("bl __rt_hash_new");                                    // call the hash new helper
    emitter.instruction("str x0, [sp, #24]");                                   // park hash ptr

    emitter.instruction("mov x9, #1");                                          // cursor = 1 (skip leading `{`)
    emitter.instruction("str x9, [sp, #16]");                                   // store updated JSON decoder state
    emitter.instruction("str xzr, [sp, #56]");                                  // no trailing comma seen yet

    emitter.label("__rt_json_decode_object_real_loop");

    // Skip whitespace before the key.
    emitter.instruction("ldr x1, [sp, #0]");                                    // load or prepare JSON decoder state
    emitter.instruction("ldr x2, [sp, #8]");                                    // load or prepare JSON decoder state
    emitter.instruction("ldr x9, [sp, #16]");                                   // load or prepare JSON decoder state
    emitter.instruction("sub x10, x2, #1");                                     // update the JSON decoder cursor or counter
    emitter.instruction("mov x2, x10");                                         // skip whitespace up to, but not past, the closing brace
    emitter.instruction("bl __rt_json_skip_ws");                                // advance to the next key or closing brace
    emitter.instruction("cmp x9, x2");                                          // did the scan reach the closing brace?
    emitter.instruction("b.ge __rt_json_decode_object_real_close");             // branch on the current JSON decoder condition
    emitter.instruction("str x9, [sp, #16]");                                   // store updated JSON decoder state

    // After whitespace skip: if `}` we're done.
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load or prepare JSON decoder state
    emitter.instruction("cmp w11, #125");                                       // '}'
    emitter.instruction("b.eq __rt_json_decode_object_real_close");             // branch on the current JSON decoder condition

    // Key MUST be a JSON string starting with `"`.
    emitter.instruction("cmp w11, #34");                                        // '"'
    emitter.instruction("b.ne __rt_json_decode_object_real_fail");              // branch on the current JSON decoder condition
    emitter.instruction("str xzr, [sp, #56]");                                  // a real key clears the trailing-comma guard

    // Save key_start, then scan to the closing `"` (with backslash-escape
    // awareness so `\\\"` doesn't end the key prematurely).
    emitter.instruction("str x9, [sp, #32]");                                   // key_start (points at opening `"`)
    emitter.instruction("add x9, x9, #1");                                      // step past the opening `"`
    emitter.instruction("ldr x10, [sp, #8]");                                   // slice_len
    emitter.instruction("mov x12, #0");                                         // escape flag
    emitter.label("__rt_json_decode_object_real_key_scan");
    emitter.instruction("cmp x9, x10");                                         // check the current JSON decoder condition
    emitter.instruction("b.ge __rt_json_decode_object_real_fail");              // branch on the current JSON decoder condition
    emitter.instruction("ldrb w13, [x1, x9]");                                  // load or prepare JSON decoder state
    emitter.instruction("cbnz x12, __rt_json_decode_object_real_key_after_escape"); // branch on the current JSON decoder condition
    emitter.instruction("cmp w13, #92");                                        // '\\'
    emitter.instruction("b.eq __rt_json_decode_object_real_key_set_escape");    // branch on the current JSON decoder condition
    emitter.instruction("cmp w13, #34");                                        // '"' → key end
    emitter.instruction("b.eq __rt_json_decode_object_real_key_done");          // branch on the current JSON decoder condition
    emitter.instruction("b __rt_json_decode_object_real_key_advance");          // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_key_set_escape");
    emitter.instruction("mov x12, #1");                                         // load or prepare JSON decoder state
    emitter.instruction("b __rt_json_decode_object_real_key_advance");          // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_key_after_escape");
    emitter.instruction("mov x12, #0");                                         // load or prepare JSON decoder state
    emitter.label("__rt_json_decode_object_real_key_advance");
    emitter.instruction("add x9, x9, #1");                                      // update the JSON decoder cursor or counter
    emitter.instruction("b __rt_json_decode_object_real_key_scan");             // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_key_done");
    emitter.instruction("add x9, x9, #1");                                      // include the closing `"` in the sub-slice
    emitter.instruction("str x9, [sp, #16]");                                   // cursor at byte after closing `"`

    // Recursively decode the key sub-slice — must produce Mixed(str).
    emitter.instruction("ldr x11, [sp, #0]");                                   // slice_ptr
    emitter.instruction("ldr x10, [sp, #32]");                                  // key_start
    emitter.instruction("add x1, x11, x10");                                    // sub_ptr
    emitter.instruction("sub x2, x9, x10");                                     // sub_len
    emitter.instruction("bl __rt_json_decode_mixed");                           // call the json decode mixed helper
    emitter.instruction("cbz x0, __rt_json_decode_object_real_propagate");      // recursion already recorded the JSON error
    emitter.instruction("str x0, [sp, #40]");                                   // park key Mixed*

    // Skip whitespace, expect `:`, skip whitespace.
    emitter.instruction("ldr x9, [sp, #16]");                                   // load or prepare JSON decoder state
    emitter.instruction("ldr x1, [sp, #0]");                                    // load or prepare JSON decoder state
    emitter.instruction("ldr x2, [sp, #8]");                                    // load or prepare JSON decoder state
    emitter.instruction("bl __rt_json_skip_ws");                                // advance to the colon after the key
    emitter.instruction("cmp x9, x2");                                          // check the current JSON decoder condition
    emitter.instruction("b.ge __rt_json_decode_object_real_fail");              // branch on the current JSON decoder condition
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load or prepare JSON decoder state
    emitter.label("__rt_json_decode_object_real_at_colon");
    emitter.instruction("cmp w11, #58");                                        // ':'
    emitter.instruction("b.ne __rt_json_decode_object_real_fail");              // branch on the current JSON decoder condition
    emitter.instruction("add x9, x9, #1");                                      // consume the colon
    emitter.instruction("str x9, [sp, #16]");                                   // store updated JSON decoder state

    // Skip whitespace before the value.
    emitter.instruction("bl __rt_json_skip_ws");                                // advance to the first byte of the value
    emitter.instruction("cmp x9, x2");                                          // check the current JSON decoder condition
    emitter.instruction("b.ge __rt_json_decode_object_real_fail");              // branch on the current JSON decoder condition
    emitter.instruction("str x9, [sp, #16]");                                   // store updated JSON decoder state
    emitter.instruction("str x9, [sp, #48]");                                   // value_start

    // Boundary scanner for the value: advance to ',' or '}' at depth 0.
    emitter.instruction("ldr x10, [sp, #8]");                                   // slice_len
    emitter.instruction("mov x12, #0");                                         // depth
    emitter.instruction("mov x13, #0");                                         // in_string
    emitter.instruction("mov x14, #0");                                         // escape
    emitter.label("__rt_json_decode_object_real_value_scan");
    emitter.instruction("cmp x9, x10");                                         // check the current JSON decoder condition
    emitter.instruction("b.ge __rt_json_decode_object_real_value_done");        // branch on the current JSON decoder condition
    emitter.instruction("ldrb w15, [x1, x9]");                                  // load or prepare JSON decoder state
    emitter.instruction("cbnz x14, __rt_json_decode_object_real_value_after_escape"); // branch on the current JSON decoder condition
    emitter.instruction("cbnz x13, __rt_json_decode_object_real_value_in_string"); // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #34");                                        // check the current JSON decoder condition
    emitter.instruction("b.eq __rt_json_decode_object_real_value_enter_string"); // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #91");                                        // check the current JSON decoder condition
    emitter.instruction("b.eq __rt_json_decode_object_real_value_open");        // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #123");                                       // check the current JSON decoder condition
    emitter.instruction("b.eq __rt_json_decode_object_real_value_open");        // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #93");                                        // check the current JSON decoder condition
    emitter.instruction("b.eq __rt_json_decode_object_real_value_close_inner"); // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #125");                                       // check the current JSON decoder condition
    emitter.instruction("b.eq __rt_json_decode_object_real_value_close_inner"); // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #44");                                        // check the current JSON decoder condition
    emitter.instruction("b.ne __rt_json_decode_object_real_value_advance");     // branch on the current JSON decoder condition
    emitter.instruction("cbz x12, __rt_json_decode_object_real_value_done");    // branch on the current JSON decoder condition
    emitter.instruction("b __rt_json_decode_object_real_value_advance");        // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_open");
    emitter.instruction("add x12, x12, #1");                                    // update the JSON decoder cursor or counter
    emitter.instruction("b __rt_json_decode_object_real_value_advance");        // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_close_inner");
    emitter.instruction("cbz x12, __rt_json_decode_object_real_value_done");    // branch on the current JSON decoder condition
    emitter.instruction("sub x12, x12, #1");                                    // update the JSON decoder cursor or counter
    emitter.instruction("b __rt_json_decode_object_real_value_advance");        // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_enter_string");
    emitter.instruction("mov x13, #1");                                         // load or prepare JSON decoder state
    emitter.instruction("b __rt_json_decode_object_real_value_advance");        // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_in_string");
    emitter.instruction("cmp w15, #92");                                        // check the current JSON decoder condition
    emitter.instruction("b.eq __rt_json_decode_object_real_value_set_escape");  // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #34");                                        // check the current JSON decoder condition
    emitter.instruction("b.ne __rt_json_decode_object_real_value_advance");     // branch on the current JSON decoder condition
    emitter.instruction("mov x13, #0");                                         // load or prepare JSON decoder state
    emitter.instruction("b __rt_json_decode_object_real_value_advance");        // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_set_escape");
    emitter.instruction("mov x14, #1");                                         // load or prepare JSON decoder state
    emitter.instruction("b __rt_json_decode_object_real_value_advance");        // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_after_escape");
    emitter.instruction("mov x14, #0");                                         // load or prepare JSON decoder state
    emitter.label("__rt_json_decode_object_real_value_advance");
    emitter.instruction("add x9, x9, #1");                                      // update the JSON decoder cursor or counter
    emitter.instruction("b __rt_json_decode_object_real_value_scan");           // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_done");
    emitter.instruction("str x9, [sp, #16]");                                   // cursor at separator

    // Recursively decode the value sub-slice.
    emitter.instruction("ldr x11, [sp, #0]");                                   // slice_ptr
    emitter.instruction("ldr x10, [sp, #48]");                                  // value_start
    emitter.instruction("add x1, x11, x10");                                    // update the JSON decoder cursor or counter
    emitter.instruction("sub x2, x9, x10");                                     // update the JSON decoder cursor or counter
    emitter.instruction("bl __rt_json_decode_mixed");                           // call the json decode mixed helper
    emitter.instruction("cbz x0, __rt_json_decode_object_real_propagate");      // recursion already recorded the JSON error

    // Insert (key, value) into the hash.
    //   __rt_hash_set: x0=hash, x1=key_lo, x2=key_hi, x3=value_lo,
    //                  x4=value_hi, x5=value_tag → returns x0=updated hash
    emitter.instruction("str x0, [sp, #48]");                                   // park value Mixed* while the key may be normalized
    emitter.instruction("ldr x10, [sp, #40]");                                  // key Mixed*
    emitter.instruction("ldr x1, [x10, #8]");                                   // key_lo = ptr (offset 8 in Mixed cell)
    emitter.instruction("ldr x2, [x10, #16]");                                  // key_hi = len (offset 16)
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_decode_assoc");
    emitter.instruction("ldr x9, [x9]");                                        // load the assoc flag to choose array-key versus property-name semantics
    emitter.instruction("cbz x9, __rt_json_decode_object_real_key_ready");      // stdClass mode keeps numeric-looking property names as strings
    emitter.instruction("bl __rt_hash_normalize_key");                          // normalize integer-string JSON object keys for assoc-array mode
    emitter.label("__rt_json_decode_object_real_key_ready");
    emitter.instruction("ldr x0, [sp, #24]");                                   // hash ptr
    emitter.instruction("ldr x3, [sp, #48]");                                   // value_lo = Mixed*
    emitter.instruction("mov x4, #0");                                          // value_hi
    emitter.instruction("mov x5, #7");                                          // value_tag = boxed mixed
    emitter.instruction("bl __rt_hash_set");                                    // call the hash set helper
    emitter.instruction("str x0, [sp, #24]");                                   // updated hash ptr

    // Look at the separator.
    emitter.instruction("ldr x1, [sp, #0]");                                    // load or prepare JSON decoder state
    emitter.instruction("ldr x9, [sp, #16]");                                   // load or prepare JSON decoder state
    emitter.instruction("ldr x10, [sp, #8]");                                   // load or prepare JSON decoder state
    emitter.instruction("cmp x9, x10");                                         // check the current JSON decoder condition
    emitter.instruction("b.ge __rt_json_decode_object_real_fail");              // branch on the current JSON decoder condition
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load or prepare JSON decoder state
    emitter.instruction("cmp w11, #44");                                        // ','
    emitter.instruction("b.eq __rt_json_decode_object_real_after_comma");       // branch on the current JSON decoder condition
    emitter.instruction("cmp w11, #125");                                       // '}'
    emitter.instruction("b.eq __rt_json_decode_object_real_close");             // branch on the current JSON decoder condition
    emitter.instruction("b __rt_json_decode_object_real_fail");                 // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_object_real_after_comma");
    emitter.instruction("add x9, x9, #1");                                      // update the JSON decoder cursor or counter
    emitter.instruction("str x9, [sp, #16]");                                   // store updated JSON decoder state
    emitter.instruction("mov x11, #1");                                         // mark that the next token must be a key
    emitter.instruction("str x11, [sp, #56]");                                  // remember a comma was just consumed
    emitter.instruction("b __rt_json_decode_object_real_loop");                 // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_object_real_close");
    emitter.instruction("ldr x2, [sp, #8]");                                    // slice length
    emitter.instruction("sub x10, x2, #1");                                     // final byte index must be the closing brace
    emitter.instruction("cmp x9, x10");                                         // did parsing stop exactly at the final brace?
    emitter.instruction("b.ne __rt_json_decode_object_real_fail");              // trailing bytes after the object are invalid
    emitter.instruction("ldr x11, [sp, #56]");                                  // trailing-comma guard
    emitter.instruction("cbnz x11, __rt_json_decode_object_real_fail");         // trailing commas are invalid JSON
    emitter.instruction("ldr x1, [sp, #24]");                                   // hash ptr
    // PHP json_decode default returns stdClass; assoc=true returns hash.
    // Read the runtime flag set by the json_decode codegen to decide which.
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_decode_assoc");
    emitter.instruction("ldr x9, [x9]");                                        // load the assoc flag (0 → stdClass, non-zero → assoc array)
    emitter.instruction("cbz x9, __rt_json_decode_object_real_close_stdclass"); // 0 means PHP's default → wrap hash in a stdClass

    emitter.instruction("mov x0, #5");                                          // tag = associative array
    emitter.instruction("mov x2, #0");                                          // mixed_from_value high word unused for assoc payload
    emitter.instruction("bl __rt_mixed_from_value");                            // box the hash as Mixed(assoc)
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the local frame before returning
    emitter.instruction("ret");                                                 // return Mixed* (assoc array) in x0

    emitter.label("__rt_json_decode_object_real_close_stdclass");
    emitter.instruction("mov x0, x1");                                          // x0 = hash pointer for stdclass_from_hash
    emitter.instruction("bl __rt_stdclass_from_hash");                          // x0 = freshly allocated stdClass adopting the decoded hash
    emitter.instruction("mov x1, x0");                                          // shift the stdClass pointer into the mixed_from_value low-word slot
    emitter.instruction("mov x0, #6");                                          // tag = object
    emitter.instruction("mov x2, #0");                                          // mixed_from_value high word unused for object payload
    emitter.instruction("bl __rt_mixed_from_value");                            // box the stdClass as Mixed(object)
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the local frame before returning
    emitter.instruction("ret");                                                 // return Mixed* (stdClass) in x0

    emitter.label("__rt_json_decode_object_real_fail");
    emitter.instruction("mov x0, #4");                                          // JSON_ERROR_SYNTAX
    emitter.instruction("bl __rt_json_throw_error");                            // record or throw the syntax error
    emitter.label("__rt_json_decode_object_real_propagate");
    emitter.instruction("mov x0, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // load or prepare JSON decoder state
    emitter.instruction("add sp, sp, #80");                                     // update the JSON decoder cursor or counter
    emitter.instruction("ret");                                                 // return from the JSON decoder helper
}

/// __rt_json_decode_mixed_object_real (x86_64): mirrors the ARM64 recursive
/// object parser. See the ARM64 docstring for the parser's semantics.
pub(super) fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode_mixed_object_real ---");
    emitter.label_global("__rt_json_decode_mixed_object_real");

    // Frame layout (rbp-relative, 64 bytes reserved):
    //   [rbp - 8]  = slice_ptr
    //   [rbp - 16] = slice_len
    //   [rbp - 24] = cursor
    //   [rbp - 32] = hash_ptr
    //   [rbp - 40] = key_start
    //   [rbp - 48] = key Mixed*
    //   [rbp - 56] = value_start / value Mixed* during insertion
    //   [rbp - 64] = after_comma flag
    emitter.instruction("push rbp");                                            // preserve or restore JSON decoder scratch state
    emitter.instruction("mov rbp, rsp");                                        // load or prepare JSON decoder state
    emitter.instruction("sub rsp, 64");                                         // update the JSON decoder cursor or counter
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // load or prepare JSON decoder state
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // load or prepare JSON decoder state

    emitter.instruction("mov rdi, 4");                                          // initial capacity
    emitter.instruction("mov rsi, 7");                                          // value_type = boxed mixed
    emitter.instruction("call __rt_hash_new");                                  // call the hash new helper
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // load or prepare JSON decoder state

    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // cursor past `{`
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // no trailing comma seen yet

    emitter.label("__rt_json_decode_object_real_loop_x");

    // Skip whitespace before key.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // load or prepare JSON decoder state
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // load or prepare JSON decoder state
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // load or prepare JSON decoder state
    emitter.instruction("sub rdx, 1");                                          // closing brace index is the exclusive whitespace limit
    emitter.instruction("call __rt_json_skip_ws");                              // advance to the next key or closing brace
    emitter.instruction("cmp rcx, rdx");                                        // did the scan reach the closing brace?
    emitter.instruction("jge __rt_json_decode_object_real_close_x");            // branch on the current JSON decoder condition
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // load or prepare JSON decoder state

    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore slice length after the whitespace helper limit
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON decoder state
    emitter.instruction("cmp r8, 125");                                         // '}'
    emitter.instruction("je __rt_json_decode_object_real_close_x");             // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 34");                                          // '"' — key must be JSON string
    emitter.instruction("jne __rt_json_decode_object_real_fail_x");             // branch on the current JSON decoder condition
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // a real key clears the trailing-comma guard

    // Save key_start, scan to closing `"`.
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // load or prepare JSON decoder state
    emitter.instruction("add rcx, 1");                                          // step past opening `"`
    emitter.instruction("push r12");                                            // preserve callee-saved
    emitter.instruction("xor r12, r12");                                        // escape flag
    emitter.label("__rt_json_decode_object_real_key_scan_x");
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON decoder condition
    emitter.instruction("jge __rt_json_decode_object_real_key_fail_x");         // branch on the current JSON decoder condition
    emitter.instruction("movzx r10, BYTE PTR [rax + rcx]");                     // load or prepare JSON decoder state
    emitter.instruction("test r12, r12");                                       // check the current JSON decoder condition
    emitter.instruction("jne __rt_json_decode_object_real_key_after_escape_x"); // branch on the current JSON decoder condition
    emitter.instruction("cmp r10, 92");                                         // '\\'
    emitter.instruction("je __rt_json_decode_object_real_key_set_escape_x");    // branch on the current JSON decoder condition
    emitter.instruction("cmp r10, 34");                                         // '"'
    emitter.instruction("je __rt_json_decode_object_real_key_done_x");          // branch on the current JSON decoder condition
    emitter.instruction("jmp __rt_json_decode_object_real_key_advance_x");      // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_key_set_escape_x");
    emitter.instruction("mov r12, 1");                                          // load or prepare JSON decoder state
    emitter.instruction("jmp __rt_json_decode_object_real_key_advance_x");      // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_key_after_escape_x");
    emitter.instruction("xor r12, r12");                                        // update the JSON decoder cursor or counter
    emitter.label("__rt_json_decode_object_real_key_advance_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_object_real_key_scan_x");         // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_key_done_x");
    emitter.instruction("pop r12");                                             // preserve or restore JSON decoder scratch state
    emitter.instruction("add rcx, 1");                                          // include the closing `"`
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // load or prepare JSON decoder state

    // Recursively decode the key sub-slice.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // load or prepare JSON decoder state
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // load or prepare JSON decoder state
    emitter.instruction("add rax, r10");                                        // update the JSON decoder cursor or counter
    emitter.instruction("mov rdx, rcx");                                        // load or prepare JSON decoder state
    emitter.instruction("sub rdx, r10");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_json_decode_mixed");                         // call the json decode mixed helper
    emitter.instruction("test rax, rax");                                       // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_object_real_propagate_x");         // recursion already recorded the JSON error
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // park key Mixed*

    // Skip whitespace, expect `:`, skip whitespace.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // load or prepare JSON decoder state
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // load or prepare JSON decoder state
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // load or prepare JSON decoder state
    emitter.instruction("call __rt_json_skip_ws");                              // advance to the colon after the key
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON decoder condition
    emitter.instruction("jge __rt_json_decode_object_real_fail_x");             // branch on the current JSON decoder condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON decoder state
    emitter.label("__rt_json_decode_object_real_at_colon_x");
    emitter.instruction("cmp r8, 58");                                          // ':'
    emitter.instruction("jne __rt_json_decode_object_real_fail_x");             // branch on the current JSON decoder condition
    emitter.instruction("add rcx, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // load or prepare JSON decoder state

    // Skip whitespace before value.
    emitter.instruction("call __rt_json_skip_ws");                              // advance to the first byte of the value
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON decoder condition
    emitter.instruction("jge __rt_json_decode_object_real_fail_x");             // branch on the current JSON decoder condition
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // load or prepare JSON decoder state
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // load or prepare JSON decoder state

    // Boundary scanner for the value.
    emitter.instruction("push r12");                                            // preserve or restore JSON decoder scratch state
    emitter.instruction("xor r10, r10");                                        // depth
    emitter.instruction("xor r11, r11");                                        // in_string
    emitter.instruction("xor r12, r12");                                        // escape
    emitter.label("__rt_json_decode_object_real_value_scan_x");
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON decoder condition
    emitter.instruction("jge __rt_json_decode_object_real_value_done_x");       // branch on the current JSON decoder condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON decoder state
    emitter.instruction("test r12, r12");                                       // check the current JSON decoder condition
    emitter.instruction("jne __rt_json_decode_object_real_value_after_escape_x"); // branch on the current JSON decoder condition
    emitter.instruction("test r11, r11");                                       // check the current JSON decoder condition
    emitter.instruction("jne __rt_json_decode_object_real_value_in_string_x");  // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 34");                                          // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_object_real_value_enter_string_x"); // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 91");                                          // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_object_real_value_open_x");        // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 123");                                         // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_object_real_value_open_x");        // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 93");                                          // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_object_real_value_close_inner_x"); // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 125");                                         // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_object_real_value_close_inner_x"); // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 44");                                          // check the current JSON decoder condition
    emitter.instruction("jne __rt_json_decode_object_real_value_advance_x");    // branch on the current JSON decoder condition
    emitter.instruction("test r10, r10");                                       // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_object_real_value_done_x");        // branch on the current JSON decoder condition
    emitter.instruction("jmp __rt_json_decode_object_real_value_advance_x");    // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_open_x");
    emitter.instruction("add r10, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_object_real_value_advance_x");    // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_close_inner_x");
    emitter.instruction("test r10, r10");                                       // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_object_real_value_done_x");        // branch on the current JSON decoder condition
    emitter.instruction("sub r10, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_object_real_value_advance_x");    // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_enter_string_x");
    emitter.instruction("mov r11, 1");                                          // load or prepare JSON decoder state
    emitter.instruction("jmp __rt_json_decode_object_real_value_advance_x");    // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_in_string_x");
    emitter.instruction("cmp r8, 92");                                          // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_object_real_value_set_escape_x");  // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 34");                                          // check the current JSON decoder condition
    emitter.instruction("jne __rt_json_decode_object_real_value_advance_x");    // branch on the current JSON decoder condition
    emitter.instruction("xor r11, r11");                                        // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_object_real_value_advance_x");    // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_set_escape_x");
    emitter.instruction("mov r12, 1");                                          // load or prepare JSON decoder state
    emitter.instruction("jmp __rt_json_decode_object_real_value_advance_x");    // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_after_escape_x");
    emitter.instruction("xor r12, r12");                                        // update the JSON decoder cursor or counter
    emitter.label("__rt_json_decode_object_real_value_advance_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_object_real_value_scan_x");       // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_object_real_value_done_x");
    emitter.instruction("pop r12");                                             // preserve or restore JSON decoder scratch state
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // load or prepare JSON decoder state

    // Recursively decode value sub-slice.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // load or prepare JSON decoder state
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // load or prepare JSON decoder state
    emitter.instruction("add rax, r10");                                        // update the JSON decoder cursor or counter
    emitter.instruction("mov rdx, rcx");                                        // load or prepare JSON decoder state
    emitter.instruction("sub rdx, r10");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_json_decode_mixed");                         // call the json decode mixed helper
    emitter.instruction("test rax, rax");                                       // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_object_real_propagate_x");         // recursion already recorded the JSON error

    // hash_set on x86_64: rdi=hash, rsi=key_lo, rdx=key_hi, rcx=value_lo,
    // r8=value_hi, r9=value_tag -> returns rax=updated hash.
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // park value Mixed* while the key may be normalized
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // key Mixed*
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // key_lo = key pointer for the normalizer ABI
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // key_hi = key length for the normalizer ABI
    emitter.instruction("mov r10, QWORD PTR [rip + _json_decode_assoc]");       // load the assoc flag to choose array-key versus property-name semantics
    emitter.instruction("test r10, r10");                                       // determine whether this object becomes an assoc array
    emitter.instruction("je __rt_json_decode_object_real_key_ready_x");         // stdClass mode keeps numeric-looking property names as strings
    emitter.instruction("call __rt_hash_normalize_key");                        // normalize integer-string JSON object keys for assoc-array mode
    emitter.label("__rt_json_decode_object_real_key_ready_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // value_lo = value Mixed*
    emitter.instruction("mov rsi, rax");                                        // key_lo = normalized key low word
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // hash ptr
    emitter.instruction("xor r8, r8");                                          // value_hi
    emitter.instruction("mov r9, 7");                                           // value_tag = boxed mixed
    emitter.instruction("call __rt_hash_set");                                  // call the hash set helper
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // load or prepare JSON decoder state

    // Look at the separator.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // load or prepare JSON decoder state
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // load or prepare JSON decoder state
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // load or prepare JSON decoder state
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON decoder condition
    emitter.instruction("jge __rt_json_decode_object_real_fail_x");             // branch on the current JSON decoder condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON decoder state
    emitter.instruction("cmp r8, 44");                                          // ','
    emitter.instruction("je __rt_json_decode_object_real_after_comma_x");       // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 125");                                         // '}'
    emitter.instruction("je __rt_json_decode_object_real_close_x");             // branch on the current JSON decoder condition
    emitter.instruction("jmp __rt_json_decode_object_real_fail_x");             // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_object_real_after_comma_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // load or prepare JSON decoder state
    emitter.instruction("mov QWORD PTR [rbp - 64], 1");                         // remember a comma was just consumed
    emitter.instruction("jmp __rt_json_decode_object_real_loop_x");             // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_object_real_close_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // cursor at the candidate closing brace
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // slice length
    emitter.instruction("sub rdx, 1");                                          // final byte index must be the closing brace
    emitter.instruction("cmp rcx, rdx");                                        // did parsing stop exactly at the final brace?
    emitter.instruction("jne __rt_json_decode_object_real_fail_x");             // trailing bytes after the object are invalid
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // was the last parsed token a comma?
    emitter.instruction("jne __rt_json_decode_object_real_fail_x");             // trailing commas are invalid JSON
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // rdi = hash pointer
    // PHP json_decode default returns stdClass; assoc=true returns hash.
    // Read the runtime flag set by the json_decode codegen to decide which.
    emitter.instruction("mov r10, QWORD PTR [rip + _json_decode_assoc]");       // load the assoc flag (0 → stdClass, non-zero → assoc array)
    emitter.instruction("test r10, r10");                                       // zero means PHP's default
    emitter.instruction("je __rt_json_decode_object_real_close_stdclass_x");    // dispatch to stdClass wrapping

    emitter.instruction("mov rax, 5");                                          // tag = associative array
    emitter.instruction("xor rsi, rsi");                                        // mixed_from_value high word unused for assoc payload
    emitter.instruction("call __rt_mixed_from_value");                          // box the hash as Mixed(assoc)
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* (assoc array) in rax

    emitter.label("__rt_json_decode_object_real_close_stdclass_x");
    // rdi already holds the hash pointer (SysV first arg) for stdclass_from_hash.
    emitter.instruction("call __rt_stdclass_from_hash");                        // rax = freshly allocated stdClass adopting the decoded hash
    emitter.instruction("mov rdi, rax");                                        // shift the stdClass pointer into the mixed_from_value low-word slot
    emitter.instruction("mov rax, 6");                                          // tag = object
    emitter.instruction("xor rsi, rsi");                                        // mixed_from_value high word unused for object payload
    emitter.instruction("call __rt_mixed_from_value");                          // box the stdClass as Mixed(object)
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* (stdClass) in rax

    emitter.label("__rt_json_decode_object_real_key_fail_x");
    emitter.instruction("pop r12");                                             // preserve or restore JSON decoder scratch state
    emitter.label("__rt_json_decode_object_real_fail_x");
    emitter.instruction("mov rax, 4");                                          // JSON_ERROR_SYNTAX
    emitter.instruction("call __rt_json_throw_error");                          // record or throw the syntax error
    emitter.label("__rt_json_decode_object_real_propagate_x");
    emitter.instruction("xor rax, rax");                                        // update the JSON decoder cursor or counter
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON decoder state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON decoder scratch state
    emitter.instruction("ret");                                                 // return from the JSON decoder helper
}
