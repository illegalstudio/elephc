//! Purpose:
//! Emits `__rt_http_fire_notification`, the runtime shim that invokes a stream
//! context's registered `notification` callback at an HTTP transfer milestone
//! (`STREAM_NOTIFY_*`). The callback descriptor is captured at codegen time into
//! the `_stream_notification_callback` global by `stream_context_create` /
//! `stream_context_set_params`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - `__rt_http_open` (src/codegen/runtime/io/http.rs) at the CONNECT,
//!   COMPLETED, and FAILURE milestones.
//!
//! Key details:
//! - PHP's callback signature is
//!   `(int $code, int $severity, ?string $message, int $message_code,
//!     int $bytes_transferred, int $bytes_max): void`. This shim builds a
//!   6-element indexed array of boxed-Mixed arguments and invokes the callback
//!   through its descriptor's own invoker (offset 56), which self-applies the
//!   declared signature — the same mechanism `call_user_func_array` uses.
//! - The argument array is filled with direct slot stores (`[array + 24 + i*8]`
//!   for each boxed element, `[array + 0]` for the running length) after a single
//!   `value_type = 7` (Mixed) stamp. It deliberately does NOT use
//!   `__rt_array_push_int`, which re-stamps the element type to scalar on the
//!   first write — exactly the invoker-array contract in `call_user_func_array`.
//! - When `_stream_notification_callback` is null (no callback registered) the
//!   shim is a no-op. `$message_code` is always 0 in v1 (HTTP does not surface a
//!   protocol message code); a null `$message` is passed when `msg_ptr` is 0.
//! - The boxed argument array and the callback's boxed return value are released
//!   before returning; the notification result is `void` in PHP.

use crate::codegen::callable_descriptor::CALLABLE_DESC_INVOKER_OFFSET;
use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_http_fire_notification(code, severity, msg_ptr, msg_len,
/// bytes_now, bytes_max)`.
///
/// Inputs (AArch64): x0 = code, x1 = severity, x2 = message pointer (0 = null
/// message), x3 = message length, x4 = bytes_transferred, x5 = bytes_max.
/// (x86_64): rdi, rsi, rdx, rcx, r8, r9 in the same order. No result (the
/// PHP callback returns void); all integer/pointer args are caller-saved.
pub fn emit_fire_notification(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fire_notification_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: http_fire_notification ---");
    emitter.label_global("__rt_http_fire_notification");

    // Stack frame (96 bytes):
    //   [sp, #0]  = code           [sp, #8]  = severity
    //   [sp, #16] = msg_ptr        [sp, #24] = msg_len
    //   [sp, #32] = bytes_now      [sp, #40] = bytes_max
    //   [sp, #48] = args array ptr [sp, #56] = descriptor ptr
    //   [sp, #80] = saved x29      [sp, #88] = saved x30
    emitter.instruction("sub sp, sp, #96");                                     // allocate the notification-shim frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer

    // -- no-op fast path when no callback is registered --
    abi::emit_symbol_address(emitter, "x9", "_stream_notification_callback");
    emitter.instruction("ldr x9, [x9]");                                        // load the captured callback descriptor pointer
    emitter.instruction("cbz x9, __rt_hfn_done");                               // no callback registered → nothing to fire
    emitter.instruction("str x9, [sp, #56]");                                   // save the descriptor across the arg-building calls

    emitter.instruction("str x0, [sp, #0]");                                    // save code
    emitter.instruction("str x1, [sp, #8]");                                    // save severity
    emitter.instruction("str x2, [sp, #16]");                                   // save message pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save message length
    emitter.instruction("str x4, [sp, #32]");                                   // save bytes_transferred
    emitter.instruction("str x5, [sp, #40]");                                   // save bytes_max

    // -- allocate a 6-slot argument array and stamp value_type = Mixed once --
    emitter.instruction("mov x0, #6");                                          // capacity: six PHP arguments (no growth needed)
    emitter.instruction("mov x1, #8");                                          // boxed Mixed slots store one pointer each
    abi::emit_call_label(emitter, "__rt_array_new");                            // x0 = indexed array backing storage
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the packed array kind word from the header
    emitter.instruction("mov x12, #0x80ff");                                    // preserve the indexed-array kind and persistent COW flag
    emitter.instruction("and x10, x10, x12");                                   // keep only the persistent metadata bits
    emitter.instruction("mov x11, #7");                                         // value_type tag 7 = boxed Mixed
    emitter.instruction("lsl x11, x11, #8");                                    // move the tag into the packed kind-word byte lane
    emitter.instruction("orr x10, x10, x11");                                   // combine the heap kind with the value_type tag
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the stamped kind word (never re-stamped: no push_int)
    emitter.instruction("str x0, [sp, #48]");                                   // save the args array pointer (fixed for all slots)

    // -- arg 0: code (int) --
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #0]");                                    // value_lo = code
    emitter.instruction("mov x2, #0");                                          // value_hi = 0 for an integer scalar
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = boxed Mixed(int) code
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the args array pointer
    emitter.instruction("str x0, [x9, #24]");                                   // store boxed code into slot 0 (data region + 0)
    emitter.instruction("mov x10, #1");                                         // running element count = 1
    emitter.instruction("str x10, [x9]");                                       // update the array length field

    // -- arg 1: severity (int) --
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #8]");                                    // value_lo = severity
    emitter.instruction("mov x2, #0");                                          // value_hi = 0
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = boxed Mixed(int) severity
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the args array pointer
    emitter.instruction("str x0, [x9, #32]");                                   // store boxed severity into slot 1
    emitter.instruction("mov x10, #2");                                         // running element count = 2
    emitter.instruction("str x10, [x9]");                                       // update the array length field

    // -- arg 2: message (string when msg_ptr != 0, else null) --
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the message pointer
    emitter.instruction("cbz x9, __rt_hfn_msg_null");                           // a null pointer means a PHP null message
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("ldr x1, [sp, #16]");                                   // value_lo = message pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // value_hi = message length
    emitter.instruction("b __rt_hfn_msg_box");                                  // box the string message
    emitter.label("__rt_hfn_msg_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null payload low word
    emitter.instruction("mov x2, #0");                                          // null payload high word
    emitter.label("__rt_hfn_msg_box");
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = boxed Mixed(string|null) message
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the args array pointer
    emitter.instruction("str x0, [x9, #40]");                                   // store boxed message into slot 2
    emitter.instruction("mov x10, #3");                                         // running element count = 3
    emitter.instruction("str x10, [x9]");                                       // update the array length field

    // -- arg 3: message_code (always 0 in v1) --
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("mov x1, #0");                                          // value_lo = 0 (HTTP surfaces no message code)
    emitter.instruction("mov x2, #0");                                          // value_hi = 0
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = boxed Mixed(int) message_code
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the args array pointer
    emitter.instruction("str x0, [x9, #48]");                                   // store boxed message_code into slot 3
    emitter.instruction("mov x10, #4");                                         // running element count = 4
    emitter.instruction("str x10, [x9]");                                       // update the array length field

    // -- arg 4: bytes_transferred (int) --
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #32]");                                   // value_lo = bytes_transferred
    emitter.instruction("mov x2, #0");                                          // value_hi = 0
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = boxed Mixed(int) bytes_transferred
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the args array pointer
    emitter.instruction("str x0, [x9, #56]");                                   // store boxed bytes_transferred into slot 4
    emitter.instruction("mov x10, #5");                                         // running element count = 5
    emitter.instruction("str x10, [x9]");                                       // update the array length field

    // -- arg 5: bytes_max (int) --
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #40]");                                   // value_lo = bytes_max
    emitter.instruction("mov x2, #0");                                          // value_hi = 0
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = boxed Mixed(int) bytes_max
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the args array pointer
    emitter.instruction("str x0, [x9, #64]");                                   // store boxed bytes_max into slot 5
    emitter.instruction("mov x10, #6");                                         // running element count = 6 (complete)
    emitter.instruction("str x10, [x9]");                                       // update the array length field

    // -- box the indexed argument array as a Mixed(indexed-array) cell --
    // The descriptor invoker expects arg1 to be a boxed Mixed cell (tag 4, array
    // pointer at offset 8), not a raw array — it un-boxes, clones, and normalizes
    // the elements (exactly the call_user_func_array invoker contract).
    emitter.instruction("ldr x1, [sp, #48]");                                   // raw args array pointer → payload lo
    emitter.instruction("mov x2, #0");                                          // payload hi unused for an array
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 = indexed array (this increfs the array)
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = boxed Mixed cell wrapping the args array
    emitter.instruction("str x0, [sp, #64]");                                   // save the boxed Mixed argument cell

    // -- invoke the callback through its descriptor's invoker (offset 56) --
    emitter.instruction("ldr x0, [sp, #56]");                                   // arg0 = callback descriptor pointer
    emitter.instruction("ldr x1, [sp, #64]");                                   // arg1 = the boxed Mixed argument cell
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); //load the per-callable invoker function pointer
    emitter.instruction("cbz x9, __rt_hfn_free");                               // no invoker (shouldn't happen) → just release the args
    emitter.instruction("blr x9");                                              // invoke notification($code, …) → boxed Mixed result in x0
    emitter.instruction("bl __rt_decref_mixed");                                // release the ignored boxed void/return value (pointer in x0)

    emitter.label("__rt_hfn_free");
    emitter.instruction("ldr x0, [sp, #64]");                                   // boxed Mixed argument cell
    emitter.instruction("bl __rt_decref_mixed");                                // release the cell (drops the array ref taken by boxing)
    emitter.instruction("ldr x0, [sp, #48]");                                   // raw args array pointer
    abi::emit_call_label(emitter, "__rt_decref_any");                           // release the args array and its boxed elements

    emitter.label("__rt_hfn_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the shim frame
    emitter.instruction("ret");                                                 // return to the http_open milestone site
}

/// x86_64 implementation of `__rt_http_fire_notification`.
fn emit_fire_notification_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: http_fire_notification ---");
    emitter.label_global("__rt_http_fire_notification");

    // Frame: [rbp-8] code, [rbp-16] severity, [rbp-24] msg_ptr, [rbp-32] msg_len,
    //   [rbp-40] bytes_now, [rbp-48] bytes_max, [rbp-56] args array, [rbp-64] desc,
    //   [rbp-72] boxed Mixed args cell.
    //   push rbp + sub rsp,80 keeps rsp 16-aligned for the nested calls.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 80");                                         // spill slots for the six args, the array, the descriptor, and the cell

    // -- save the six incoming arguments before clobbering any register --
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save code
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save severity
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save message pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save message length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save bytes_transferred
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save bytes_max (the 6th incoming argument)

    // -- no-op fast path when no callback is registered --
    abi::emit_symbol_address(emitter, "r10", "_stream_notification_callback");  // address of the captured-callback global
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the callback descriptor pointer
    emitter.instruction("test r10, r10");                                       // is a callback registered?
    emitter.instruction("jz __rt_hfn_done_x86");                                // no callback → nothing to fire
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // save the descriptor across the arg-building calls

    // -- allocate a 6-slot argument array and stamp value_type = Mixed once --
    emitter.instruction("mov rdi, 6");                                          // capacity: six PHP arguments (no growth needed)
    emitter.instruction("mov rsi, 8");                                          // boxed Mixed slots store one pointer each
    abi::emit_call_label(emitter, "__rt_array_new");                            // rax = indexed array backing storage
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed array kind word from the header
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve heap marker + indexed-array kind + COW bit
    emitter.instruction("and r10, r11");                                        // keep only the persistent metadata bits
    emitter.instruction("mov r11, 7");                                          // value_type tag 7 = boxed Mixed
    emitter.instruction("shl r11, 8");                                          // move the tag into the packed kind-word byte lane
    emitter.instruction("or r10, r11");                                         // combine the heap kind with the value_type tag
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // persist the stamped kind word (never re-stamped: no push_int)
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the args array pointer (fixed for all slots)

    // -- arg 0: code (int) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // value_lo = code
    emitter.instruction("xor esi, esi");                                        // value_hi = 0 for an integer scalar
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // rax = boxed Mixed(int) code
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], rax");                       // store boxed code into slot 0 (data region + 0)
    emitter.instruction("mov QWORD PTR [r10], 1");                              // update the array length field to 1

    // -- arg 1: severity (int) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // value_lo = severity
    emitter.instruction("xor esi, esi");                                        // value_hi = 0
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // rax = boxed Mixed(int) severity
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 32], rax");                       // store boxed severity into slot 1
    emitter.instruction("mov QWORD PTR [r10], 2");                              // update the array length field to 2

    // -- arg 2: message (string when msg_ptr != 0, else null) --
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the message pointer
    emitter.instruction("test r10, r10");                                       // a null pointer means a PHP null message
    emitter.instruction("jz __rt_hfn_msg_null_x86");                            // box null when there is no message
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // value_lo = message pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // value_hi = message length
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("jmp __rt_hfn_msg_box_x86");                            // box the string message
    emitter.label("__rt_hfn_msg_null_x86");
    emitter.instruction("xor edi, edi");                                        // null payload low word
    emitter.instruction("xor esi, esi");                                        // null payload high word
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null
    emitter.label("__rt_hfn_msg_box_x86");
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // rax = boxed Mixed(string|null) message
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 40], rax");                       // store boxed message into slot 2
    emitter.instruction("mov QWORD PTR [r10], 3");                              // update the array length field to 3

    // -- arg 3: message_code (always 0 in v1) --
    emitter.instruction("xor edi, edi");                                        // value_lo = 0 (HTTP surfaces no message code)
    emitter.instruction("xor esi, esi");                                        // value_hi = 0
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // rax = boxed Mixed(int) message_code
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 48], rax");                       // store boxed message_code into slot 3
    emitter.instruction("mov QWORD PTR [r10], 4");                              // update the array length field to 4

    // -- arg 4: bytes_transferred (int) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // value_lo = bytes_transferred
    emitter.instruction("xor esi, esi");                                        // value_hi = 0
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // rax = boxed Mixed(int) bytes_transferred
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 56], rax");                       // store boxed bytes_transferred into slot 4
    emitter.instruction("mov QWORD PTR [r10], 5");                              // update the array length field to 5

    // -- arg 5: bytes_max (int) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // value_lo = bytes_max
    emitter.instruction("xor esi, esi");                                        // value_hi = 0
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // rax = boxed Mixed(int) bytes_max
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the args array pointer
    emitter.instruction("mov QWORD PTR [r10 + 64], rax");                       // store boxed bytes_max into slot 5
    emitter.instruction("mov QWORD PTR [r10], 6");                              // update the array length field to 6 (complete)

    // -- box the indexed argument array as a Mixed(indexed-array) cell --
    // The descriptor invoker expects arg1 to be a boxed Mixed cell (tag 4, array
    // pointer at offset 8), not a raw array — it un-boxes, clones, and normalizes
    // the elements (exactly the call_user_func_array invoker contract).
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // raw args array pointer → payload lo
    emitter.instruction("xor esi, esi");                                        // payload hi unused for an array
    emitter.instruction("mov eax, 4");                                          // runtime tag 4 = indexed array (this increfs the array)
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // rax = boxed Mixed cell wrapping the args array
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the boxed Mixed argument cell

    // -- invoke the callback through its descriptor's invoker (offset 56) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // arg0 = callback descriptor pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // arg1 = the boxed Mixed argument cell
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", CALLABLE_DESC_INVOKER_OFFSET)); //load the per-callable invoker function pointer
    emitter.instruction("test r10, r10");                                       // no invoker (shouldn't happen)?
    emitter.instruction("jz __rt_hfn_free_x86");                                // just release the args when absent
    emitter.instruction("call r10");                                            // invoke notification($code, …) → boxed Mixed result in rax
    abi::emit_call_label(emitter, "__rt_decref_mixed");                        // release the ignored boxed return value (pointer already in rax)

    emitter.label("__rt_hfn_free_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // boxed Mixed argument cell
    abi::emit_call_label(emitter, "__rt_decref_mixed");                        // release the cell (drops the array ref taken by boxing)
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // raw args array pointer
    abi::emit_call_label(emitter, "__rt_decref_any");                          // release the args array and its boxed elements

    emitter.label("__rt_hfn_done_x86");
    emitter.instruction("add rsp, 80");                                         // release the shim frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the http_open milestone site
}
