//! Purpose:
//! Emits shared JSON array decoding helpers for boxed Mixed values.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen::runtime::system` during runtime emission.
//!
//! Key details:
//! - Array decoding must preserve element order, depth checks, and boxed Mixed payload ownership.

use crate::codegen::emit::Emitter;

/// Emits ARM64 runtime assembly for `__rt_json_decode_mixed_array_real`, a
/// recursive-descent parser for non-empty JSON arrays. Walks the slice between
/// the leading `[` and trailing `]`, finds each element boundary using a
/// depth-and-string-aware scanner, then recursively calls `__rt_json_decode_mixed`
/// on each element sub-slice. Mixed pointers are pushed into a fresh array
/// created by `__rt_array_new(cap=4, elem_size=8)`; the array is finally boxed
/// as `Mixed(tag=4)` and returned.
///
/// # Frame layout (64 bytes, sp-relative)
/// - `[sp + 0..8]`   = slice_ptr
/// - `[sp + 8..16]`  = slice_len
/// - `[sp + 16..24]` = cursor (running scan position)
/// - `[sp + 24..32]` = arr_ptr (allocated via `__rt_array_new`, may grow on push)
/// - `[sp + 32..40]` = elem_start (saved across the recursive decode call)
/// - `[sp + 40..48]` = after_comma flag
/// - `[sp + 48..56]` = saved x29
/// - `[sp + 56..64]` = saved x30
///
/// # Input registers
/// - `x1` = slice ptr (with leading `[` and trailing `]`)
/// - `x2` = slice length
///
/// # Output registers
/// - `x0` = `Mixed*` on success; `0` on parse error (error recorded via `__rt_json_throw_error`)
///
/// # Error handling
/// - Unterminated values, trailing bytes after `]`, and trailing commas all
///   yield `JSON_ERROR_SYNTAX` via `__rt_json_throw_error`.
pub(super) fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode_mixed_array_real ---");
    emitter.label_global("__rt_json_decode_mixed_array_real");

    // Frame layout (64 bytes):
    //   [sp + 0]  = slice_ptr
    //   [sp + 8]  = slice_len
    //   [sp + 16] = cursor (running scan position)
    //   [sp + 24] = arr_ptr (allocated via __rt_array_new, may grow on push)
    //   [sp + 32] = elem_start (saved across the recursive decode call)
    //   [sp + 40] = after_comma flag
    //   [sp + 48] = saved x29
    //   [sp + 56] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // update the JSON decoder cursor or counter
    emitter.instruction("stp x29, x30, [sp, #48]");                             // store updated JSON decoder state
    emitter.instruction("add x29, sp, #48");                                    // update the JSON decoder cursor or counter
    emitter.instruction("str x1, [sp, #0]");                                    // park the array slice ptr for downstream loads
    emitter.instruction("str x2, [sp, #8]");                                    // park the array slice length for the boundary scanner

    // Allocate the destination array up-front so failure paths can free
    // it consistently. capacity=4 is a reasonable starting point; the
    // push_refcounted helper grows the array if needed.
    emitter.instruction("mov x0, #4");                                          // initial capacity
    emitter.instruction("mov x1, #8");                                          // elem_size = 8 (Mixed-pointer slots)
    emitter.instruction("bl __rt_array_new");                                   // call the array new helper
    emitter.instruction("str x0, [sp, #24]");                                   // park the array pointer

    // Initialize the cursor past the leading `[`.
    emitter.instruction("mov x9, #1");                                          // load or prepare JSON decoder state
    emitter.instruction("str x9, [sp, #16]");                                   // store updated JSON decoder state
    emitter.instruction("str xzr, [sp, #40]");                                  // no comma has been consumed before the first element

    // Outer element loop. Each iteration: skip ws → scan element boundary
    // → recursively decode → push → look at separator.
    emitter.label("__rt_json_decode_array_real_loop");

    // Skip whitespace before the element.
    emitter.instruction("ldr x1, [sp, #0]");                                    // slice_ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // slice_len
    emitter.instruction("ldr x9, [sp, #16]");                                   // cursor
    emitter.instruction("sub x10, x2, #1");                                     // last meaningful index = len - 1 (the `]`)
    emitter.instruction("mov x2, x10");                                         // skip whitespace up to, but not past, the closing bracket
    emitter.instruction("bl __rt_json_skip_ws");                                // advance to the next element or closing bracket
    emitter.instruction("cmp x9, x2");                                          // did the scan reach the closing bracket?
    emitter.instruction("b.ge __rt_json_decode_array_real_close");              // ran past the close → finalize
    emitter.instruction("str x9, [sp, #16]");                                   // store updated JSON decoder state

    // After whitespace skip: peek at the byte. If it's the closing `]`
    // (e.g., trailing whitespace before close, or empty array — though
    // that case is also handled by the caller) we're done.
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load or prepare JSON decoder state
    emitter.instruction("cmp w11, #93");                                        // ']'
    emitter.instruction("b.eq __rt_json_decode_array_real_close");              // branch on the current JSON decoder condition

    // Save elem_start, then run the boundary scanner.
    emitter.instruction("str xzr, [sp, #40]");                                  // seeing an element clears the trailing-comma guard
    emitter.instruction("str x9, [sp, #32]");                                   // elem_start

    // Boundary scanner: advance cursor until we hit ',' or ']' at depth 0,
    // accounting for nested `[`/`]` and `{`/`}` plus JSON string state.
    // Registers (caller-saved, all live within this scan):
    //   x9  = cursor
    //   x10 = end (slice_ptr + slice_len)
    //   x11 = slice_ptr
    //   x12 = depth
    //   x13 = in_string flag
    //   x14 = escape flag
    //   w15 = current byte
    emitter.instruction("ldr x10, [sp, #8]");                                   // slice_len
    emitter.instruction("ldr x11, [sp, #0]");                                   // slice_ptr
    emitter.instruction("mov x12, #0");                                         // depth
    emitter.instruction("mov x13, #0");                                         // in_string
    emitter.instruction("mov x14, #0");                                         // escape
    emitter.label("__rt_json_decode_array_real_scan");
    emitter.instruction("cmp x9, x10");                                         // hit slice end?
    emitter.instruction("b.ge __rt_json_decode_array_real_scan_done");          // unterminated value → end
    emitter.instruction("ldrb w15, [x11, x9]");                                 // load or prepare JSON decoder state
    emitter.instruction("cbnz x14, __rt_json_decode_array_real_scan_after_escape"); // branch on the current JSON decoder condition
    emitter.instruction("cbnz x13, __rt_json_decode_array_real_scan_in_string"); // branch on the current JSON decoder condition
    // Outside string state.
    emitter.instruction("cmp w15, #34");                                        // '"'
    emitter.instruction("b.eq __rt_json_decode_array_real_scan_enter_string");  // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #91");                                        // '['
    emitter.instruction("b.eq __rt_json_decode_array_real_scan_open");          // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #123");                                       // '{'
    emitter.instruction("b.eq __rt_json_decode_array_real_scan_open");          // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #93");                                        // ']'
    emitter.instruction("b.eq __rt_json_decode_array_real_scan_close");         // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #125");                                       // '}'
    emitter.instruction("b.eq __rt_json_decode_array_real_scan_close");         // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #44");                                        // ','
    emitter.instruction("b.ne __rt_json_decode_array_real_scan_advance");       // branch on the current JSON decoder condition
    // Comma at depth 0 means element separator; bail.
    emitter.instruction("cbz x12, __rt_json_decode_array_real_scan_done");      // branch on the current JSON decoder condition
    emitter.instruction("b __rt_json_decode_array_real_scan_advance");          // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_open");
    emitter.instruction("add x12, x12, #1");                                    // update the JSON decoder cursor or counter
    emitter.instruction("b __rt_json_decode_array_real_scan_advance");          // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_close");
    // Closing bracket at depth 0 → element ends, leave for outer loop.
    emitter.instruction("cbz x12, __rt_json_decode_array_real_scan_done");      // branch on the current JSON decoder condition
    emitter.instruction("sub x12, x12, #1");                                    // update the JSON decoder cursor or counter
    emitter.instruction("b __rt_json_decode_array_real_scan_advance");          // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_enter_string");
    emitter.instruction("mov x13, #1");                                         // load or prepare JSON decoder state
    emitter.instruction("b __rt_json_decode_array_real_scan_advance");          // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_in_string");
    emitter.instruction("cmp w15, #92");                                        // '\\'
    emitter.instruction("b.eq __rt_json_decode_array_real_scan_set_escape");    // branch on the current JSON decoder condition
    emitter.instruction("cmp w15, #34");                                        // '"' → close string
    emitter.instruction("b.ne __rt_json_decode_array_real_scan_advance");       // branch on the current JSON decoder condition
    emitter.instruction("mov x13, #0");                                         // load or prepare JSON decoder state
    emitter.instruction("b __rt_json_decode_array_real_scan_advance");          // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_set_escape");
    emitter.instruction("mov x14, #1");                                         // load or prepare JSON decoder state
    emitter.instruction("b __rt_json_decode_array_real_scan_advance");          // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_after_escape");
    emitter.instruction("mov x14, #0");                                         // load or prepare JSON decoder state
    emitter.label("__rt_json_decode_array_real_scan_advance");
    emitter.instruction("add x9, x9, #1");                                      // update the JSON decoder cursor or counter
    emitter.instruction("b __rt_json_decode_array_real_scan");                  // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_done");
    emitter.instruction("str x9, [sp, #16]");                                   // cursor now at the separator (',' or ']')

    // Recursively decode the element sub-slice.
    emitter.instruction("ldr x11, [sp, #0]");                                   // slice_ptr
    emitter.instruction("ldr x10, [sp, #32]");                                  // elem_start
    emitter.instruction("ldr x9, [sp, #16]");                                   // elem_end
    emitter.instruction("add x1, x11, x10");                                    // sub_ptr = slice_ptr + elem_start
    emitter.instruction("sub x2, x9, x10");                                     // sub_len = elem_end - elem_start
    emitter.instruction("bl __rt_json_decode_mixed");                           // x0 = Mixed* for the element
    emitter.instruction("cbz x0, __rt_json_decode_array_real_propagate");       // recursion already recorded the JSON error

    // Push the Mixed pointer into the destination array.
    emitter.instruction("ldr x1, [sp, #24]");                                   // array ptr
    emitter.instruction("mov x9, x1");                                          // copy for arg-order swap
    emitter.instruction("mov x1, x0");                                          // child = Mixed*
    emitter.instruction("mov x0, x9");                                          // x0 = array ptr
    emitter.instruction("bl __rt_array_push_refcounted");                       // returns x0 = updated array
    emitter.instruction("str x0, [sp, #24]");                                   // store updated JSON decoder state

    // Look at the separator.
    emitter.instruction("ldr x1, [sp, #0]");                                    // slice_ptr
    emitter.instruction("ldr x9, [sp, #16]");                                   // cursor at separator
    emitter.instruction("ldr x10, [sp, #8]");                                   // slice_len
    emitter.instruction("cmp x9, x10");                                         // check the current JSON decoder condition
    emitter.instruction("b.ge __rt_json_decode_array_real_fail");               // branch on the current JSON decoder condition
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load or prepare JSON decoder state
    emitter.instruction("cmp w11, #44");                                        // ','
    emitter.instruction("b.eq __rt_json_decode_array_real_after_comma");        // branch on the current JSON decoder condition
    emitter.instruction("cmp w11, #93");                                        // ']'
    emitter.instruction("b.eq __rt_json_decode_array_real_close");              // branch on the current JSON decoder condition
    emitter.instruction("b __rt_json_decode_array_real_fail");                  // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_array_real_after_comma");
    emitter.instruction("add x9, x9, #1");                                      // update the JSON decoder cursor or counter
    emitter.instruction("str x9, [sp, #16]");                                   // store updated JSON decoder state
    emitter.instruction("mov x10, #1");                                         // mark that the next token must be another element
    emitter.instruction("str x10, [sp, #40]");                                  // publish the trailing-comma guard
    emitter.instruction("b __rt_json_decode_array_real_loop");                  // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_array_real_close");
    emitter.instruction("ldr x9, [sp, #16]");                                   // cursor should point at the closing bracket
    emitter.instruction("ldr x10, [sp, #8]");                                   // slice length
    emitter.instruction("sub x10, x10, #1");                                    // required closing bracket index
    emitter.instruction("cmp x9, x10");                                         // did the parser stop at the final byte?
    emitter.instruction("b.ne __rt_json_decode_array_real_fail");               // trailing bytes after the array are invalid
    emitter.instruction("ldr x11, [sp, #40]");                                  // was the close reached right after a comma?
    emitter.instruction("cbnz x11, __rt_json_decode_array_real_fail");          // trailing commas are invalid JSON
    // Box the populated array as Mixed(tag=4).
    emitter.instruction("ldr x1, [sp, #24]");                                   // array ptr
    emitter.instruction("mov x0, #4");                                          // tag = indexed array
    emitter.instruction("mov x2, #0");                                          // load or prepare JSON decoder state
    emitter.instruction("bl __rt_mixed_from_value");                            // call the mixed from value helper
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // load or prepare JSON decoder state
    emitter.instruction("add sp, sp, #64");                                     // update the JSON decoder cursor or counter
    emitter.instruction("ret");                                                 // return from the JSON decoder helper

    emitter.label("__rt_json_decode_array_real_fail");
    emitter.instruction("mov x0, #4");                                          // JSON_ERROR_SYNTAX
    emitter.instruction("bl __rt_json_throw_error");                            // record syntax error and throw when requested
    emitter.label("__rt_json_decode_array_real_propagate");
    emitter.instruction("mov x0, #0");                                          // signal decode failure to the caller
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // load or prepare JSON decoder state
    emitter.instruction("add sp, sp, #64");                                     // update the JSON decoder cursor or counter
    emitter.instruction("ret");                                                 // return from the JSON decoder helper
}

/// Emits x86_64 (System V ABI) runtime assembly for `__rt_json_decode_mixed_array_real`,
/// a recursive-descent parser for non-empty JSON arrays. Identical semantics to the
/// ARM64 version; see `emit_aarch64` for the full parser description.
///
/// # Frame layout (48 bytes, rbp-relative)
/// - `[rbp - 8]`   = slice_ptr
/// - `[rbp - 16]`  = slice_len
/// - `[rbp - 24]`  = cursor
/// - `[rbp - 32]`  = arr_ptr
/// - `[rbp - 40]`  = elem_start
/// - `[rbp - 48]`  = after_comma flag
///
/// # Input registers (System V ABI)
/// - `rax` = slice ptr
/// - `rdx` = slice length
///
/// # Output registers
/// - `rax` = `Mixed*` on success; `0` on parse error (error recorded via `__rt_json_throw_error`)
///
/// # Error handling
/// - Unterminated values, trailing bytes after `]`, and trailing commas all
///   yield `JSON_ERROR_SYNTAX` via `__rt_json_throw_error`.
pub(super) fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode_mixed_array_real ---");
    emitter.label_global("__rt_json_decode_mixed_array_real");

    // Frame layout (rbp-relative, 48 bytes reserved):
    //   [rbp - 8]  = slice_ptr
    //   [rbp - 16] = slice_len
    //   [rbp - 24] = cursor
    //   [rbp - 32] = arr_ptr
    //   [rbp - 40] = elem_start
    //   [rbp - 48] = after_comma flag
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 48");                                         // reserve aligned scratch
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // park slice_ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // park slice_len

    emitter.instruction("mov rdi, 4");                                          // initial capacity
    emitter.instruction("mov rsi, 8");                                          // elem_size = 8 (Mixed-pointer slots)
    emitter.instruction("call __rt_array_new");                                 // rax = array pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // park array pointer

    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // cursor = 1 (skip leading `[`)
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // no comma has been consumed before the first element

    emitter.label("__rt_json_decode_array_real_loop_x");

    // Skip whitespace before the element.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // slice_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // slice_len
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // cursor
    emitter.instruction("sub rdx, 1");                                          // last meaningful index = len - 1 (the `]`)
    emitter.instruction("call __rt_json_skip_ws");                              // advance to the next element or closing bracket
    emitter.instruction("cmp rcx, rdx");                                        // did the scan reach the closing bracket?
    emitter.instruction("jge __rt_json_decode_array_real_close_x");             // ran past the close
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // load or prepare JSON decoder state

    // After whitespace skip: peek at the byte. If it's `]` we're done.
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON decoder state
    emitter.instruction("cmp r8, 93");                                          // ']'
    emitter.instruction("je __rt_json_decode_array_real_close_x");              // branch on the current JSON decoder condition

    // Save elem_start, then run the boundary scanner.
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // seeing an element clears the trailing-comma guard
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // elem_start

    // Boundary scanner. Registers:
    //   rcx = cursor
    //   rdx = slice_len
    //   rax = slice_ptr (kept reloadable but we use [rbp-8] when needed)
    //   r10 = depth
    //   r11 = in_string flag
    //   r12 = escape flag (callee-saved — we save before clobbering)
    //   r8  = current byte
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore slice_len after the whitespace helper used an exclusive limit
    emitter.instruction("push r12");                                            // preserve callee-saved (System V keeps r12)
    emitter.instruction("xor r10, r10");                                        // depth
    emitter.instruction("xor r11, r11");                                        // in_string
    emitter.instruction("xor r12, r12");                                        // escape
    emitter.label("__rt_json_decode_array_real_scan_x");
    emitter.instruction("cmp rcx, rdx");                                        // hit slice end?
    emitter.instruction("jge __rt_json_decode_array_real_scan_done_x");         // branch on the current JSON decoder condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON decoder state
    emitter.instruction("test r12, r12");                                       // check the current JSON decoder condition
    emitter.instruction("jne __rt_json_decode_array_real_scan_after_escape_x"); // branch on the current JSON decoder condition
    emitter.instruction("test r11, r11");                                       // check the current JSON decoder condition
    emitter.instruction("jne __rt_json_decode_array_real_scan_in_string_x");    // branch on the current JSON decoder condition
    // Outside string state.
    emitter.instruction("cmp r8, 34");                                          // '"'
    emitter.instruction("je __rt_json_decode_array_real_scan_enter_string_x");  // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 91");                                          // '['
    emitter.instruction("je __rt_json_decode_array_real_scan_open_x");          // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 123");                                         // '{'
    emitter.instruction("je __rt_json_decode_array_real_scan_open_x");          // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 93");                                          // ']'
    emitter.instruction("je __rt_json_decode_array_real_scan_close_x");         // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 125");                                         // '}'
    emitter.instruction("je __rt_json_decode_array_real_scan_close_x");         // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 44");                                          // ','
    emitter.instruction("jne __rt_json_decode_array_real_scan_advance_x");      // branch on the current JSON decoder condition
    emitter.instruction("test r10, r10");                                       // depth zero?
    emitter.instruction("je __rt_json_decode_array_real_scan_done_x");          // comma at depth 0 → element separator
    emitter.instruction("jmp __rt_json_decode_array_real_scan_advance_x");      // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_open_x");
    emitter.instruction("add r10, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_array_real_scan_advance_x");      // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_close_x");
    emitter.instruction("test r10, r10");                                       // depth zero?
    emitter.instruction("je __rt_json_decode_array_real_scan_done_x");          // closing bracket at depth 0 → done
    emitter.instruction("sub r10, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_array_real_scan_advance_x");      // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_enter_string_x");
    emitter.instruction("mov r11, 1");                                          // load or prepare JSON decoder state
    emitter.instruction("jmp __rt_json_decode_array_real_scan_advance_x");      // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_in_string_x");
    emitter.instruction("cmp r8, 92");                                          // '\\'
    emitter.instruction("je __rt_json_decode_array_real_scan_set_escape_x");    // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 34");                                          // '"' → close string
    emitter.instruction("jne __rt_json_decode_array_real_scan_advance_x");      // branch on the current JSON decoder condition
    emitter.instruction("xor r11, r11");                                        // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_array_real_scan_advance_x");      // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_set_escape_x");
    emitter.instruction("mov r12, 1");                                          // load or prepare JSON decoder state
    emitter.instruction("jmp __rt_json_decode_array_real_scan_advance_x");      // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_after_escape_x");
    emitter.instruction("xor r12, r12");                                        // update the JSON decoder cursor or counter
    emitter.label("__rt_json_decode_array_real_scan_advance_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("jmp __rt_json_decode_array_real_scan_x");              // continue in the JSON decoder control path
    emitter.label("__rt_json_decode_array_real_scan_done_x");
    emitter.instruction("pop r12");                                             // restore callee-saved register
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // cursor at separator

    // Recursively decode the element sub-slice.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // slice_ptr
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // elem_start
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // elem_end
    emitter.instruction("add rax, r10");                                        // sub_ptr = slice_ptr + elem_start
    emitter.instruction("mov rdx, rcx");                                        // sub_len = elem_end - elem_start
    emitter.instruction("sub rdx, r10");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_json_decode_mixed");                         // rax = Mixed* for the element
    emitter.instruction("test rax, rax");                                       // check the current JSON decoder condition
    emitter.instruction("je __rt_json_decode_array_real_propagate_x");          // recursion already recorded the JSON error

    // Push the Mixed pointer into the destination array.
    emitter.instruction("mov rsi, rax");                                        // child = Mixed*
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // array ptr
    emitter.instruction("call __rt_array_push_refcounted");                     // rax = updated array
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // load or prepare JSON decoder state

    // Look at the separator.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // load or prepare JSON decoder state
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // load or prepare JSON decoder state
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // load or prepare JSON decoder state
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON decoder condition
    emitter.instruction("jge __rt_json_decode_array_real_fail_x");              // branch on the current JSON decoder condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON decoder state
    emitter.instruction("cmp r8, 44");                                          // ','
    emitter.instruction("je __rt_json_decode_array_real_after_comma_x");        // branch on the current JSON decoder condition
    emitter.instruction("cmp r8, 93");                                          // ']'
    emitter.instruction("je __rt_json_decode_array_real_close_x");              // branch on the current JSON decoder condition
    emitter.instruction("jmp __rt_json_decode_array_real_fail_x");              // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_array_real_after_comma_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON decoder cursor or counter
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // load or prepare JSON decoder state
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // next token must be another element
    emitter.instruction("jmp __rt_json_decode_array_real_loop_x");              // continue in the JSON decoder control path

    emitter.label("__rt_json_decode_array_real_close_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // cursor should point at the closing bracket
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // slice length
    emitter.instruction("sub rdx, 1");                                          // required closing bracket index
    emitter.instruction("cmp rcx, rdx");                                        // did the parser stop at the final byte?
    emitter.instruction("jne __rt_json_decode_array_real_fail_x");              // trailing bytes after the array are invalid
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // was the close reached right after a comma?
    emitter.instruction("jne __rt_json_decode_array_real_fail_x");              // trailing commas are invalid JSON
    // Box the populated array as Mixed(tag=4).
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // array ptr
    emitter.instruction("mov rax, 4");                                          // tag = indexed array
    emitter.instruction("xor rsi, rsi");                                        // update the JSON decoder cursor or counter
    emitter.instruction("call __rt_mixed_from_value");                          // call the mixed from value helper
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON decoder state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON decoder scratch state
    emitter.instruction("ret");                                                 // return from the JSON decoder helper

    emitter.label("__rt_json_decode_array_real_fail_x");
    emitter.instruction("mov rax, 4");                                          // JSON_ERROR_SYNTAX
    emitter.instruction("call __rt_json_throw_error");                          // record syntax error and throw when requested
    emitter.label("__rt_json_decode_array_real_propagate_x");
    emitter.instruction("xor rax, rax");                                        // signal decode failure to the caller
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON decoder state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON decoder scratch state
    emitter.instruction("ret");                                                 // return from the JSON decoder helper
}
