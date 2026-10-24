//! Purpose:
//! Emits the `__rt_json_encode_array_int`, `__rt_json_arr_int_loop` runtime helper assembly for json encode array int.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - JSON encoders are emitted formatter state machines; escaping, type tags, and buffer growth are observable PHP behavior.
//! - The output goes through a `__rt_strbuf_*` descriptor in the frame, which starts in the
//!   scratch tail and grows onto the heap. Every write is preceded by a `__rt_json_buf_ensure`
//!   or `__rt_json_buf_append` reservation, and the cursor is reloaded from the descriptor
//!   after each one because growth relocates it. Encoding `range(1, 20000)` used to stream past
//!   the end of the 64 KiB `_concat_buf` into the adjacent globals.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;
use crate::codegen_support::runtime::strings::{STRBUF_SIZE, STRBUF_WRITE};

/// Frame offset of the AArch64 output-buffer descriptor.
const DESC: usize = 48;
/// Frame size of the AArch64 encoder: array, length, index, descriptor, and the frame record.
const FRAME: usize = DESC + STRBUF_SIZE + 16;

/// Emits `__rt_json_encode_array_int` to encode an int array as JSON "[1,2,3]" with optional pretty-printing.
///
/// Input:  x0 = array pointer (header: capacity[8], length[8], then elements[8 each])
/// Output: x1 = result ptr, x2 = result len (scratch-backed, or a transient heap block once the
///         output outgrew the scratch tail)
///
/// Behavior:
/// - Redirects to `__rt_json_encode_array_dynamic` when `JSON_FORCE_OBJECT` (bit 16) is set in `_json_active_flags`.
/// - Enters one pretty-print indentation level via `__rt_json_pretty_push` after the opening bracket.
/// - Each element gets optional newline + indentation via `__rt_json_pretty_line`.
/// - Recursion depth is tracked via `__rt_json_depth_enter`/`__rt_json_depth_exit`.
/// - `__rt_strbuf_finish` publishes `_concat_off` past a scratch-backed result on return.
pub(crate) fn emit_json_encode_array_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_array_int_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_int ---");
    emitter.label_global("__rt_json_encode_array_int");

    // -- hand a run-time promoted array to the object-form encoder --
    // A statically `Array(...)` value can hold HASH storage at run time:
    // `__rt_array_set_mixed_key` promotes the destination when a key does not fit the packed
    // layout, and the static type does not move with it. Encoding that as a JSON array reads
    // hash internals. The probe is the uniform heap-kind byte, the same one
    // `__rt_array_edge_key` dispatches on; `__rt_json_encode_assoc` takes the container in x0
    // and returns the slice in x1/x2 exactly as this entry does, so it is a tail jump — and it
    // is the encoder that renders `[5 => 3]` the way PHP does, `{"5":3}` rather than `[3]`.
    // The conditional branch skips OVER an unconditional one because AArch64 cannot name an
    // external label in a conditional branch.
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the uniform heap-kind header word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low-byte heap kind
    emitter.instruction("cmp x9, #3");                                          // kind 3 = associative hash storage
    emitter.instruction("b.ne __rt_json_arr_int_packed");                       // packed storage encodes inline below
    emitter.instruction("b __rt_json_encode_assoc");                            // hash storage is the object-form encoder's job
    emitter.label("__rt_json_arr_int_packed");
    super::json_integer_tree::emit_json_integer_tree_entry(
        emitter,
        "__rt_json_arr_int_integer_tree",
    );

    // -- redirect to the dynamic encoder when JSON_FORCE_OBJECT is set --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x9, [x9]");                                        // load the active flag bitmask
    emitter.instruction("tst x9, #16");                                         // is JSON_FORCE_OBJECT (bit 16) set?
    emitter.instruction("b.eq __rt_json_encode_array_int_static");              // flag clear: encode as a static JSON array
    emitter.instruction("b __rt_json_encode_array_dynamic");                    // flag set: tail-call the object-aware encoder (uncond → cross-atom safe)
    emitter.label("__rt_json_encode_array_int_static");

    // -- set up stack frame --
    // [sp+0]=array, [sp+24]=length, [sp+32]=index, [sp+DESC..+32]=output buffer descriptor
    emitter.instruction(&format!("sub sp, sp, #{}", FRAME));                    // allocate the encoder frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", FRAME - 16));       // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", FRAME - 16));              // set new frame pointer

    // -- save array pointer --
    emitter.instruction("str x0, [sp, #0]");                                    // save array ptr

    // -- enter the recursion-depth check --
    emitter.instruction("bl __rt_json_depth_enter");                            // increment _json_active_depth and throw on overflow when requested

    // -- open the output buffer at the scratch tail and write the opening bracket --
    emitter.instruction(&format!("add x0, sp, #{}", DESC));                     // address of the output-buffer descriptor
    emitter.instruction("bl __rt_strbuf_init");                                 // open the buffer at the current scratch offset
    emitter.instruction(&format!("add x0, sp, #{}", DESC));                     // address of the output-buffer descriptor
    emitter.instruction("mov x1, #1");                                          // room for the opening bracket
    emitter.instruction("bl __rt_json_buf_ensure");                             // x11 = cursor with headroom reserved
    emitter.instruction("mov w12, #91");                                        // ASCII '['
    emitter.instruction("strb w12, [x11]");                                     // write '['
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction(&format!("str x11, [sp, #{}]", DESC + STRBUF_WRITE));   // save write pos
    emitter.instruction("bl __rt_json_pretty_push");                            // enter one pretty-print indentation level after the array opens

    // -- get array length --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array ptr
    emitter.instruction("ldr x3, [x0]");                                        // load array length from header offset 0
    emitter.instruction("str x3, [sp, #24]");                                   // save length
    emitter.instruction("str xzr, [sp, #32]");                                  // index = 0

    // -- loop through elements --
    emitter.label("__rt_json_arr_int_loop");
    emitter.instruction("ldr x4, [sp, #32]");                                   // load index
    emitter.instruction("ldr x3, [sp, #24]");                                   // load length
    emitter.instruction("cmp x4, x3");                                          // check if done
    emitter.instruction("b.ge __rt_json_arr_int_close");                        // done

    // -- reserve room for the separator, the indentation, and the formatted digits --
    emitter.instruction(&format!("add x0, sp, #{}", DESC));                     // address of the output-buffer descriptor
    emitter.instruction("mov x1, #1");                                          // room for the comma separator
    emitter.instruction("bl __rt_json_buf_ensure");                             // x11 = cursor with headroom reserved

    // -- add comma separator if not first element --
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload index after the reservation
    emitter.instruction("cbz x4, __rt_json_arr_int_elem");                      // skip comma for first element
    emitter.instruction("mov w12, #44");                                        // ASCII ','
    emitter.instruction("strb w12, [x11]");                                     // write ','
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction(&format!("str x11, [sp, #{}]", DESC + STRBUF_WRITE));   // save write pos

    // -- load element value and convert to string --
    emitter.label("__rt_json_arr_int_elem");
    emitter.instruction(&format!("ldr x11, [sp, #{}]", DESC + STRBUF_WRITE));   // reload write pos before optional pretty indentation
    emitter.instruction("bl __rt_json_pretty_line");                            // append newline and indentation for this element when pretty-printing
    emitter.instruction(&format!("str x11, [sp, #{}]", DESC + STRBUF_WRITE));   // save the write pos after any pretty indentation
    emitter.instruction(&format!("add x0, sp, #{}", DESC));                     // address of the output-buffer descriptor
    emitter.instruction("bl __rt_strbuf_sync");                                 // let itoa format just past the cursor (or at the heap buffer's opening offset)
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array ptr
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload index
    emitter.instruction("add x4, x4, #3");                                      // skip 24-byte header (3 * 8 bytes)
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load element value
    emitter.instruction("bl __rt_itoa");                                        // convert to string → x1/x2

    // -- copy itoa result to output --
    emitter.instruction(&format!("add x0, sp, #{}", DESC));                     // address of the output-buffer descriptor
    emitter.instruction("bl __rt_json_buf_append");                             // copy the digits to the cursor and advance it
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload index
    emitter.instruction("add x4, x4, #1");                                      // increment
    emitter.instruction("str x4, [sp, #32]");                                   // save index
    emitter.instruction("b __rt_json_arr_int_loop");                            // continue loop

    // -- write closing bracket --
    emitter.label("__rt_json_arr_int_close");
    emitter.instruction(&format!("add x0, sp, #{}", DESC));                     // address of the output-buffer descriptor
    emitter.instruction("mov x1, #1");                                          // room for the closing bracket
    emitter.instruction("bl __rt_json_buf_ensure");                             // x11 = cursor with headroom reserved
    emitter.instruction("bl __rt_json_pretty_pop");                             // leave the array indentation level before closing it
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload array length to decide whether closing needs its own line
    emitter.instruction("cbz x3, __rt_json_arr_int_close_emit");                // empty arrays stay compact even under JSON_PRETTY_PRINT
    emitter.instruction("bl __rt_json_pretty_line");                            // append the closing-line indentation for non-empty pretty arrays
    emitter.label("__rt_json_arr_int_close_emit");
    emitter.instruction("mov w12, #93");                                        // ASCII ']'
    emitter.instruction("strb w12, [x11]");                                     // write ']'
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction(&format!("str x11, [sp, #{}]", DESC + STRBUF_WRITE));   // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("bl __rt_json_depth_exit");                             // decrement _json_active_depth so a sibling encoder can re-enter cleanly

    // -- compute result and publish the scratch offset --
    emitter.instruction(&format!("add x0, sp, #{}", DESC));                     // address of the output-buffer descriptor
    emitter.instruction("bl __rt_strbuf_finish");                               // x1/x2 = encoded array, _concat_off published

    // -- tear down and return --
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", FRAME - 16));       // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", FRAME));                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_json_encode_array_int` for the x86_64-linux target.
///
/// Mirrors the ARM64 emitter's semantics but uses x86_64 calling conventions and register layout:
/// - Array pointer in `rax` on entry; result returned in `rax` (ptr) and `rdx` (len).
/// - Local slots at `[rbp - 8]` (array), `[rbp - 16]` (length), `[rbp - 24]` (index), and the
///   output-buffer descriptor at `[rbp - 64]`.
/// - Pretty-print, depth tracking, and output-buffer management are identical to the ARM64 path.
fn emit_json_encode_array_int_linux_x86_64(emitter: &mut Emitter) {
    let desc = 64;
    let write = desc - STRBUF_WRITE;
    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_int ---");
    emitter.label_global("__rt_json_encode_array_int");

    // -- hand a run-time promoted array to the object-form encoder --
    // Same runtime-promotion probe as the AArch64 path; `__rt_json_encode_assoc` takes the
    // container in RAX and returns rax/rdx exactly as this entry does. NOTE the register:
    // every json encoder here carries the array in RAX, not rdi.
    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // load the uniform heap-kind header word
    emitter.instruction("and r11, 0xff");                                       // isolate the low-byte heap kind
    emitter.instruction("cmp r11, 3");                                          // kind 3 = associative hash storage
    emitter.instruction("jne __rt_json_arr_int_packed");                        // packed storage encodes inline below
    emitter.instruction("jmp __rt_json_encode_assoc");                          // hash storage is the object-form encoder's job
    emitter.label("__rt_json_arr_int_packed");
    super::json_integer_tree::emit_json_integer_tree_entry(
        emitter,
        "__rt_json_arr_int_integer_tree_x86",
    );

    // -- redirect to the dynamic encoder when JSON_FORCE_OBJECT is set --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_active_flags", 0);      // load the active flag bitmask
    emitter.instruction("test r10, 16");                                        // is JSON_FORCE_OBJECT (bit 16) set?
    emitter.instruction("jne __rt_json_encode_array_dynamic");                  // tail-call the object-aware encoder when the flag is on

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving JSON-array scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source array and the output buffer
    emitter.instruction(&format!("sub rsp, {}", desc));                         // a 16-byte multiple: `push rbp` already aligned rsp, so every call below stays SysV-aligned (pinned by `every_x86_64_runtime_call_site_is_sysv_aligned`)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source integer array pointer across itoa calls and buffer growth

    // Enter the recursion-depth check.
    emitter.instruction("call __rt_json_depth_enter");                          // increment _json_active_depth and throw on overflow when requested

    // -- open the output buffer at the scratch tail and write the opening bracket --
    emitter.instruction(&format!("lea rdi, [rbp - {}]", desc));                 // address of the output-buffer descriptor
    emitter.instruction("call __rt_strbuf_init");                               // open the buffer at the current scratch offset
    emitter.instruction(&format!("lea rdi, [rbp - {}]", desc));                 // address of the output-buffer descriptor
    emitter.instruction("mov esi, 1");                                          // room for the opening bracket
    emitter.instruction("call __rt_json_buf_ensure");                           // r11 = cursor with headroom reserved
    emitter.instruction("mov BYTE PTR [r11], 91");                              // write the opening JSON bracket before any encoded integer payloads
    emitter.instruction("add r11, 1");                                          // advance the write pointer past the opening bracket
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r11", write));      // persist the updated write pointer before entering the element loop
    emitter.instruction("call __rt_json_pretty_push");                          // enter one pretty-print indentation level after the array opens
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source array pointer (depth_enter clobbered rax)
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the integer-array length from the first header field
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the array length across itoa calls and buffer growth
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the integer-array element loop counter to zero

    emitter.label("__rt_json_arr_int_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current integer-array element index at the top of the JSON loop
    emitter.instruction("cmp r10, QWORD PTR [rbp - 16]");                       // have we already encoded every integer-array element?
    emitter.instruction("jae __rt_json_arr_int_close");                         // finish by writing the closing bracket once the loop index reaches the array length
    emitter.instruction(&format!("lea rdi, [rbp - {}]", desc));                 // address of the output-buffer descriptor
    emitter.instruction("mov esi, 1");                                          // room for the comma separator
    emitter.instruction("call __rt_json_buf_ensure");                           // r11 = cursor with headroom for the separator, indentation, and digits
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // is this the first integer-array element in the JSON output?
    emitter.instruction("je __rt_json_arr_int_elem");                           // skip the comma separator before the first encoded integer element
    emitter.instruction("mov BYTE PTR [r11], 44");                              // write the JSON comma separator between encoded integer elements
    emitter.instruction("add r11, 1");                                          // advance the write pointer past the comma separator
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r11", write));      // persist the updated write pointer after appending the comma separator

    emitter.label("__rt_json_arr_int_elem");
    emitter.instruction(&format!("mov r11, QWORD PTR [rbp - {}]", write));      // reload the current write pointer before optional pretty indentation
    emitter.instruction("call __rt_json_pretty_line");                          // append newline and indentation for this element when pretty-printing
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r11", write));      // save the write pointer after any pretty indentation
    emitter.instruction(&format!("lea rdi, [rbp - {}]", desc));                 // address of the output-buffer descriptor
    emitter.instruction("call __rt_strbuf_sync");                               // let itoa format just past the cursor (or at the heap buffer's opening offset)
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source integer-array pointer before loading the current element payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current integer-array element index before computing the payload slot address
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8 + 24]");             // load the integer element payload from the indexed-array storage slot
    emitter.instruction("call __rt_itoa");                                      // encode the integer element as a decimal JSON slice
    emitter.instruction("mov rsi, rax");                                        // pass the decimal slice pointer to the append helper (rdx = length)
    emitter.instruction(&format!("lea rdi, [rbp - {}]", desc));                 // address of the output-buffer descriptor
    emitter.instruction("call __rt_json_buf_append");                           // copy the digits to the cursor and advance it
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // advance the integer-array element loop counter to the next payload slot
    emitter.instruction("jmp __rt_json_arr_int_loop");                          // continue encoding the remaining integer-array elements

    emitter.label("__rt_json_arr_int_close");
    emitter.instruction(&format!("lea rdi, [rbp - {}]", desc));                 // address of the output-buffer descriptor
    emitter.instruction("mov esi, 1");                                          // room for the closing bracket
    emitter.instruction("call __rt_json_buf_ensure");                           // r11 = cursor with headroom reserved
    emitter.instruction("call __rt_json_pretty_pop");                           // leave the array indentation level before closing it
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // did the array contain any elements?
    emitter.instruction("je __rt_json_arr_int_close_emit_x");                   // empty arrays stay compact even under JSON_PRETTY_PRINT
    emitter.instruction("call __rt_json_pretty_line");                          // append the closing-line indentation for non-empty pretty arrays
    emitter.label("__rt_json_arr_int_close_emit_x");
    emitter.instruction("mov BYTE PTR [r11], 93");                              // append the closing JSON bracket to complete the encoded integer array slice
    emitter.instruction("add r11, 1");                                          // advance the write pointer past the closing bracket
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r11", write));      // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("call __rt_json_depth_exit");                           // decrement _json_active_depth so a sibling encoder can re-enter cleanly
    emitter.instruction(&format!("lea rdi, [rbp - {}]", desc));                 // address of the output-buffer descriptor
    emitter.instruction("call __rt_strbuf_finish");                             // rax/rdx = encoded array, _concat_off published
    emitter.instruction(&format!("add rsp, {}", desc));                         // release the local JSON-array frame before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the encoded JSON integer array slice in the x86_64 string result registers
}
