//! Purpose:
//! Emits the `stream_wrapper_register` runtime helper
//! `__rt_stream_wrapper_register`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_stream_wrapper_register` is the entry point invoked by the
//!   `stream_wrapper_register` builtin.
//!
//! Key details:
//! - Stores up to 64 `(protocol_ptr, protocol_len, class_ptr, class_len)`
//!   tuples in `_user_wrappers` (each slot is 32 bytes; an empty slot has a
//!   null `protocol_ptr`). Returns 1 on a successful registration, 0 when the
//!   table is full.
//! - Both strings are persisted with `__rt_str_persist` before being stored:
//!   the registration outlives the call, so keeping the caller's pointer means
//!   the registered scheme silently follows whatever that buffer holds later.
//!   A literal argument hid this — it lives in rodata and never moves — while
//!   `$s = "aa"; stream_wrapper_register($s, "W"); $s = "zz";` left `aa://`
//!   unroutable and `zz://`, never registered, dispatching into the wrapper.
//! - The free slot is located BEFORE persisting, so a full table allocates
//!   nothing.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Warning PHP emits when the protocol contains a byte outside `[A-Za-z0-9+.-]`.
///
/// Reference PHP appends the class name, the protocol and the script location; the
/// runtime's other warnings (`fopen()`, `file_get_contents()`) are fixed strings
/// without a location, and this follows that convention rather than inventing a
/// half-interpolated one.
pub(crate) const BAD_PROTOCOL_WARNING: &str =
    "Warning: stream_wrapper_register(): Invalid protocol scheme specified.\n";
/// Warning PHP emits when the protocol is already registered — including the builtin
/// `file://`, which a program could otherwise shadow silently.
pub(crate) const DUPLICATE_PROTOCOL_WARNING: &str =
    "Warning: stream_wrapper_register(): Protocol is already defined.\n";

/// Emits the `__rt_stream_wrapper_register` runtime helper.
/// Input:  AArch64 x0 = proto ptr, x1 = proto len, x2 = class ptr, x3 = class len.
///         x86_64  rdi = proto ptr, rsi = proto len, rdx = class ptr, rcx = class len.
/// Output: 1 when the registration was stored, 0 when the table is full.
pub fn emit_stream_wrapper_register(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_wrapper_register_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_register ---");
    emitter.label_global("__rt_stream_wrapper_register");

    // Frame: [sp,#0..16] x29/x30, [sp,#16] free slot base, [sp,#24] class-name
    //   pointer, [sp,#32] class-name length.
    emitter.instruction("sub sp, sp, #48");                                     // frame for the two string-persist calls
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- reject a protocol PHP would not accept, before touching the table --
    emitter.instruction("mov x5, #0");                                          // protocol byte index
    emitter.label("__rt_swr_chk");
    emitter.instruction("cmp x5, x1");                                          // checked every protocol byte?
    emitter.instruction("b.ge __rt_swr_scan_start");                            // the whole protocol is in PHP's accepted set
    emitter.instruction("ldrb w6, [x0, x5]");                                   // next protocol byte
    emitter.instruction("cmp w6, #43");                                         // '+'
    emitter.instruction("b.eq __rt_swr_chk_next");
    emitter.instruction("cmp w6, #45");                                         // below '-' is outside the set (',' and friends)
    emitter.instruction("b.lt __rt_swr_badproto");
    emitter.instruction("cmp w6, #46");                                         // '-' or '.'
    emitter.instruction("b.le __rt_swr_chk_next");
    emitter.instruction("cmp w6, #48");                                         // '/' sits between '.' and '0'
    emitter.instruction("b.lt __rt_swr_badproto");
    emitter.instruction("cmp w6, #57");                                         // '0'..'9'
    emitter.instruction("b.le __rt_swr_chk_next");
    emitter.instruction("cmp w6, #65");                                         // punctuation between '9' and 'A'
    emitter.instruction("b.lt __rt_swr_badproto");
    emitter.instruction("cmp w6, #90");                                         // 'A'..'Z'
    emitter.instruction("b.le __rt_swr_chk_next");
    emitter.instruction("cmp w6, #97");                                         // punctuation between 'Z' and 'a'
    emitter.instruction("b.lt __rt_swr_badproto");
    emitter.instruction("cmp w6, #122");                                        // 'a'..'z'
    emitter.instruction("b.gt __rt_swr_badproto");
    emitter.label("__rt_swr_chk_next");
    emitter.instruction("add x5, x5, #1");                                      // advance the byte index
    emitter.instruction("b __rt_swr_chk");                                      // keep validating

    // -- one pass over _user_wrappers: reject a duplicate, remember the first free slot --
    emitter.label("__rt_swr_scan_start");
    abi::emit_symbol_address(emitter, "x4", "_user_wrappers");
    emitter.instruction("mov x5, #0");                                          // wrapper slot index
    emitter.instruction("mov x7, #-1");                                         // first free slot, -1 until one is seen
    emitter.label("__rt_swr_scan");
    emitter.instruction("cmp x5, #64");                                         // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("b.ge __rt_swr_scanned");                               // the table has been walked end to end
    emitter.instruction("add x6, x4, x5, lsl #5");                              // slot base = table + index * 32
    emitter.instruction("ldr x12, [x6]");                                       // stored protocol pointer
    emitter.instruction("cbz x12, __rt_swr_free");                              // a null pointer marks an empty slot
    emitter.instruction("ldr x13, [x6, #8]");                                   // stored protocol length
    emitter.instruction("cmp x13, x1");                                         // same length as the protocol being registered?
    emitter.instruction("b.ne __rt_swr_scan_next");                             // different length — cannot be the same protocol
    emitter.instruction("mov x14, #0");                                         // byte compare index
    emitter.label("__rt_swr_cmp");
    emitter.instruction("cmp x14, x1");                                         // compared every byte?
    emitter.instruction("b.ge __rt_swr_dupproto");                              // identical protocol — PHP refuses to redefine it
    emitter.instruction("ldrb w15, [x12, x14]");                                // stored protocol byte
    emitter.instruction("ldrb w16, [x0, x14]");                                 // candidate protocol byte
    emitter.instruction("cmp w15, w16");                                        // do the bytes match?
    emitter.instruction("b.ne __rt_swr_scan_next");                             // differs — not the same protocol
    emitter.instruction("add x14, x14, #1");                                    // advance the compare index
    emitter.instruction("b __rt_swr_cmp");                                      // continue comparing
    emitter.label("__rt_swr_free");
    emitter.instruction("cmn x7, #1");                                          // is the first free slot still unset (-1)?
    emitter.instruction("b.ne __rt_swr_scan_next");                             // already remembered an earlier free slot
    emitter.instruction("mov x7, x5");                                          // remember this one
    emitter.label("__rt_swr_scan_next");
    emitter.instruction("add x5, x5, #1");                                      // advance the slot index
    emitter.instruction("b __rt_swr_scan");                                     // continue scanning
    emitter.label("__rt_swr_scanned");
    emitter.instruction("cmn x7, #1");                                          // did the walk find a free slot?
    emitter.instruction("b.eq __rt_swr_full");                                  // table full
    emitter.instruction("add x6, x4, x7, lsl #5");                              // slot base of the first free slot

    // -- take ownership of both strings, then store the registration --
    emitter.label("__rt_swr_store");
    emitter.instruction("str x6, [sp, #16]");                                   // save the free slot base across the persist calls
    emitter.instruction("str x2, [sp, #24]");                                   // save the class-name pointer across the protocol persist
    emitter.instruction("str x3, [sp, #32]");                                   // save the class-name length across the protocol persist
    emitter.instruction("mov x2, x1");                                          // protocol length → persist length input
    emitter.instruction("mov x1, x0");                                          // protocol pointer → persist pointer input
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned protocol bytes, x2 = length
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the free slot base
    emitter.instruction("str x1, [x6]");                                        // owned protocol pointer
    emitter.instruction("str x2, [x6, #8]");                                    // protocol length
    emitter.instruction("ldr x1, [sp, #24]");                                   // class-name pointer → persist pointer input
    emitter.instruction("ldr x2, [sp, #32]");                                   // class-name length → persist length input
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned class name, x2 = length
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the free slot base
    emitter.instruction("str x1, [x6, #16]");                                   // owned class-name pointer
    emitter.instruction("str x2, [x6, #24]");                                   // class-name length
    emitter.instruction("mov x0, #1");                                          // return true for a successful registration
    emitter.instruction("b __rt_swr_ret");                                      // share the common epilogue

    emitter.label("__rt_swr_badproto");
    abi::emit_symbol_address(emitter, "x1", "_swr_bad_proto_msg");
    emitter.instruction(&format!("mov x2, #{}", BAD_PROTOCOL_WARNING.len()));   // length of the invalid-scheme warning
    emitter.instruction("bl __rt_diag_warning");                                // honours an enclosing @ suppression
    emitter.instruction("mov x0, #0");                                          // PHP returns false for an invalid protocol
    emitter.instruction("b __rt_swr_ret");

    emitter.label("__rt_swr_dupproto");
    abi::emit_symbol_address(emitter, "x1", "_swr_dup_proto_msg");
    emitter.instruction(&format!("mov x2, #{}", DUPLICATE_PROTOCOL_WARNING.len())); // length of the already-defined warning
    emitter.instruction("bl __rt_diag_warning");                                // honours an enclosing @ suppression
    emitter.instruction("mov x0, #0");                                          // PHP returns false rather than redefining
    emitter.instruction("b __rt_swr_ret");

    emitter.label("__rt_swr_full");
    emitter.instruction("mov x0, #0");                                          // return false when the table is full

    emitter.label("__rt_swr_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 stream runtime helper for stream wrapper register.
fn emit_stream_wrapper_register_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_register ---");
    emitter.label_global("__rt_stream_wrapper_register");

    // Frame: [rbp-8] free slot base, [rbp-16] class-name pointer, [rbp-24]
    //   class-name length. push rbp then sub rsp,48 keeps rsp 16-aligned.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // frame for the two string-persist calls
    // The class name is spilled BEFORE the scan, not at the store below. x86_64 has far fewer
    // scratch registers than AArch64, and the byte-compare loop below takes `rcx` as its index
    // and `edx` for the loaded byte — but `rdx` is the class-name POINTER and `rcx` its LENGTH.
    // That loop runs only when a slot is occupied AND its length matches, which is the second
    // registration of a same-length scheme onward: `stream_wrapper_register("aa", …)` then
    // `("bb", …)` stored a byte value as `bb`'s class pointer, so `bb://` never dispatched and a
    // later read through that pointer killed the process. AArch64 keeps the scan in x12-x16, all
    // clear of its argument registers, so the same source is correct there and wrong here.
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the class-name pointer before the scan can clobber rdx
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the class-name length before the scan can clobber rcx

    // -- reject a protocol PHP would not accept, before touching the table --
    emitter.instruction("xor r9, r9");                                          // protocol byte index
    emitter.label("__rt_swr_chk_x86");
    emitter.instruction("cmp r9, rsi");                                         // checked every protocol byte?
    emitter.instruction("jge __rt_swr_scan_start_x86");                         // the whole protocol is in PHP's accepted set
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r9]");                     // next protocol byte
    emitter.instruction("cmp r10b, 43");                                        // '+'
    emitter.instruction("je __rt_swr_chk_next_x86");
    emitter.instruction("cmp r10b, 45");                                        // below '-' is outside the set
    emitter.instruction("jb __rt_swr_badproto_x86");
    emitter.instruction("cmp r10b, 46");                                        // '-' or '.'
    emitter.instruction("jbe __rt_swr_chk_next_x86");
    emitter.instruction("cmp r10b, 48");                                        // '/' sits between '.' and '0'
    emitter.instruction("jb __rt_swr_badproto_x86");
    emitter.instruction("cmp r10b, 57");                                        // '0'..'9'
    emitter.instruction("jbe __rt_swr_chk_next_x86");
    emitter.instruction("cmp r10b, 65");                                        // punctuation between '9' and 'A'
    emitter.instruction("jb __rt_swr_badproto_x86");
    emitter.instruction("cmp r10b, 90");                                        // 'A'..'Z'
    emitter.instruction("jbe __rt_swr_chk_next_x86");
    emitter.instruction("cmp r10b, 97");                                        // punctuation between 'Z' and 'a'
    emitter.instruction("jb __rt_swr_badproto_x86");
    emitter.instruction("cmp r10b, 122");                                       // 'a'..'z'
    emitter.instruction("ja __rt_swr_badproto_x86");
    emitter.label("__rt_swr_chk_next_x86");
    emitter.instruction("inc r9");                                              // advance the byte index
    emitter.instruction("jmp __rt_swr_chk_x86");                                // keep validating

    // -- one pass over _user_wrappers: reject a duplicate, remember the first free slot --
    emitter.label("__rt_swr_scan_start_x86");
    abi::emit_symbol_address(emitter, "r8", "_user_wrappers");                  // wrapper table base
    emitter.instruction("xor r9, r9");                                          // wrapper slot index
    emitter.instruction("mov rax, -1");                                         // first free slot, -1 until one is seen
    emitter.label("__rt_swr_scan_x86");
    emitter.instruction("cmp r9, 64");                                          // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("jge __rt_swr_scanned_x86");                            // the table has been walked end to end
    emitter.instruction("mov r10, r9");                                         // copy the slot index for scaling
    emitter.instruction("shl r10, 5");                                          // slot offset = index * 32
    emitter.instruction("add r10, r8");                                         // slot base = table + offset
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // stored protocol pointer
    emitter.instruction("test r11, r11");                                       // a null pointer marks an empty slot
    emitter.instruction("jz __rt_swr_free_x86");                                // remember it and move on
    emitter.instruction("cmp QWORD PTR [r10 + 8], rsi");                        // same length as the protocol being registered?
    emitter.instruction("jne __rt_swr_scan_next_x86");                          // different length — cannot be the same protocol
    emitter.instruction("xor rcx, rcx");                                        // byte compare index
    emitter.label("__rt_swr_cmp_x86");
    emitter.instruction("cmp rcx, rsi");                                        // compared every byte?
    emitter.instruction("jge __rt_swr_dupproto_x86");                           // identical protocol — PHP refuses to redefine it
    emitter.instruction("movzx edx, BYTE PTR [r11 + rcx]");                     // stored protocol byte
    emitter.instruction("cmp dl, BYTE PTR [rdi + rcx]");                        // against the candidate protocol byte
    emitter.instruction("jne __rt_swr_scan_next_x86");                          // differs — not the same protocol
    emitter.instruction("inc rcx");                                             // advance the compare index
    emitter.instruction("jmp __rt_swr_cmp_x86");                                // continue comparing
    emitter.label("__rt_swr_free_x86");
    emitter.instruction("cmp rax, -1");                                         // is the first free slot still unset?
    emitter.instruction("jne __rt_swr_scan_next_x86");                          // already remembered an earlier free slot
    emitter.instruction("mov rax, r9");                                         // remember this one
    emitter.label("__rt_swr_scan_next_x86");
    emitter.instruction("inc r9");                                              // advance the slot index
    emitter.instruction("jmp __rt_swr_scan_x86");                               // continue scanning
    emitter.label("__rt_swr_scanned_x86");
    emitter.instruction("cmp rax, -1");                                         // did the walk find a free slot?
    emitter.instruction("je __rt_swr_full_x86");                                // table full
    emitter.instruction("mov r10, rax");                                        // first free slot index
    emitter.instruction("shl r10, 5");                                          // slot offset = index * 32
    emitter.instruction("add r10, r8");                                         // slot base = table + offset

    // -- take ownership of both strings, then store the registration --
    emitter.label("__rt_swr_store_x86");
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save the free slot base across the persist calls
    emitter.instruction("mov rax, rdi");                                        // protocol pointer → persist pointer input
    emitter.instruction("mov rdx, rsi");                                        // protocol length → persist length input
    emitter.instruction("call __rt_str_persist");                               // rax = owned protocol bytes, rdx = length
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the free slot base
    emitter.instruction("mov QWORD PTR [r10], rax");                            // owned protocol pointer
    emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                        // protocol length
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // class-name pointer → persist pointer input
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // class-name length → persist length input
    emitter.instruction("call __rt_str_persist");                               // rax = owned class name, rdx = length
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the free slot base
    emitter.instruction("mov QWORD PTR [r10 + 16], rax");                       // owned class-name pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], rdx");                       // class-name length
    emitter.instruction("mov eax, 1");                                          // return true for a successful registration
    emitter.instruction("jmp __rt_swr_ret_x86");                                // share the common epilogue

    emitter.label("__rt_swr_badproto_x86");
    abi::emit_symbol_address(emitter, "rdi", "_swr_bad_proto_msg");
    emitter.instruction(&format!("mov esi, {}", BAD_PROTOCOL_WARNING.len()));   // length of the invalid-scheme warning
    emitter.instruction("call __rt_diag_warning");                              // honours an enclosing @ suppression
    emitter.instruction("xor eax, eax");                                        // PHP returns false for an invalid protocol
    emitter.instruction("jmp __rt_swr_ret_x86");

    emitter.label("__rt_swr_dupproto_x86");
    abi::emit_symbol_address(emitter, "rdi", "_swr_dup_proto_msg");
    emitter.instruction(&format!("mov esi, {}", DUPLICATE_PROTOCOL_WARNING.len())); // length of the already-defined warning
    emitter.instruction("call __rt_diag_warning");                              // honours an enclosing @ suppression
    emitter.instruction("xor eax, eax");                                        // PHP returns false rather than redefining
    emitter.instruction("jmp __rt_swr_ret_x86");

    emitter.label("__rt_swr_full_x86");
    emitter.instruction("xor eax, eax");                                        // return false when the table is full

    emitter.label("__rt_swr_ret_x86");
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
