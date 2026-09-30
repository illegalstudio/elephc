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
//! - Stores `(protocol_ptr, protocol_len, class_ptr, class_len)` tuples in the
//!   heap-backed registration table and the matching registration flags in the
//!   parallel flag table. An empty slot has a null `protocol_ptr`.
//! - The slot comes from `__rt_user_wrappers_reserve`, which grows the table on
//!   demand, so registration is bounded only by the heap. PHP imposes no limit,
//!   and the previous fixed 64-slot array silently refused the 65th call.
//! - Both names are copied into owned heap storage via `__rt_str_persist`
//!   before they are stored: a registration outlives the caller's buffer, and a
//!   PHP-level `$scheme = "dyn" . $i;` reuses one local slot per iteration, so
//!   keeping the borrowed pointer made every entry alias the final value.

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
/// Input:  AArch64 x0 = proto ptr, x1 = proto len, x2 = class ptr, x3 = class len,
///         x4 = flags. x86_64 uses rdi/rsi/rdx/rcx/r8 for the same values.
/// Output: 1 when the registration was stored, 0 when the table is full.
pub fn emit_stream_wrapper_register(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_wrapper_register_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_register ---");
    emitter.label_global("__rt_stream_wrapper_register");
    // The registration outlives every caller-owned buffer, so the helper needs a
    // real frame: `__rt_str_persist` is a call and would otherwise clobber LR.
    emitter.instruction("stp x29, x30, [sp, #-80]!");                           // establish the frame and preserve the return address
    emitter.instruction("mov x29, sp");                                         // frame pointer for the persisted-argument spill area
    emitter.instruction("str x0, [sp, #16]");                                   // spill the borrowed protocol pointer
    emitter.instruction("str x1, [sp, #24]");                                   // spill the protocol length
    emitter.instruction("str x2, [sp, #32]");                                   // spill the borrowed class-name pointer
    emitter.instruction("str x3, [sp, #40]");                                   // spill the class-name length
    emitter.instruction("str x4, [sp, #48]");                                   // spill the registration flags

    // -- php burns a resource id here, and elephc must burn it too --
    //
    // php allocates a registration resource userland never sees. Nothing reads it, but the id
    // CURSOR is observable: every `var_dump()` of a later stream, and the `$context` handed to a
    // wrapper, reported one less than php. MEASURED — a REFUSED registration burns one as well,
    // an unregistration burns none, and a call that throws for an undefined class burns none
    // because the throw comes first, which is why this sits at the top of the helper the throw
    // path never reaches.
    abi::emit_symbol_address(emitter, "x9", "_resource_id_next");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("add x10, x10, #1");                                    // the id php spends on the registration
    emitter.instruction("str x10, [x9]");

    // -- php refuses a protocol holding a byte outside [A-Za-z0-9+.-] --
    // An EMPTY protocol registers successfully (measured), so the scan must be a per-byte filter
    // rather than a "looks like a scheme" test.
    emitter.instruction("ldr x9, [sp, #16]");                                   // the borrowed protocol pointer
    emitter.instruction("ldr x10, [sp, #24]");                                  // the protocol length
    emitter.instruction("mov x11, #0");                                         // byte cursor
    emitter.label("__rt_swr_char_scan");
    emitter.instruction("cmp x11, x10");                                        // examined every byte?
    emitter.instruction("b.hs __rt_swr_chars_ok");
    emitter.instruction("ldrb w12, [x9, x11]");
    emitter.instruction("cmp w12, #0x30");                                      // below '0' — the three punctuation bytes live there
    emitter.instruction("b.lo __rt_swr_char_punct");
    emitter.instruction("cmp w12, #0x39");                                      // '9'
    emitter.instruction("b.ls __rt_swr_char_next");
    emitter.instruction("cmp w12, #0x41");                                      // 'A'
    emitter.instruction("b.lo __rt_swr_bad_proto");
    emitter.instruction("cmp w12, #0x5a");                                      // 'Z'
    emitter.instruction("b.ls __rt_swr_char_next");
    emitter.instruction("cmp w12, #0x61");                                      // 'a'
    emitter.instruction("b.lo __rt_swr_bad_proto");
    emitter.instruction("cmp w12, #0x7a");                                      // 'z'
    emitter.instruction("b.ls __rt_swr_char_next");
    emitter.instruction("b __rt_swr_bad_proto");                                // above 'z', including every non-ASCII byte
    emitter.label("__rt_swr_char_punct");
    emitter.instruction("cmp w12, #0x2b");                                      // '+'
    emitter.instruction("b.eq __rt_swr_char_next");
    emitter.instruction("cmp w12, #0x2d");                                      // '-'
    emitter.instruction("b.eq __rt_swr_char_next");
    emitter.instruction("cmp w12, #0x2e");                                      // '.'
    emitter.instruction("b.eq __rt_swr_char_next");
    emitter.instruction("b __rt_swr_bad_proto");
    emitter.label("__rt_swr_char_next");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_swr_char_scan");
    emitter.label("__rt_swr_chars_ok");

    // -- php refuses a protocol that is already registered --
    // The comparison is byte-exact: registration is case-SENSITIVE, so `Cd`, `cd` and `CD` are
    // three separate registrations rather than one.
    super::emit_load_table_base(emitter, "x4");
    emitter.instruction("mov x5, #0");                                          // wrapper slot index
    emitter.label("__rt_swr_dup_scan");
    super::emit_load_table_cap(emitter, "x6");
    emitter.instruction("cmp x5, x6");                                          // scanned every allocated slot?
    emitter.instruction("b.ge __rt_swr_dup_builtin");
    emitter.instruction("add x6, x4, x5, lsl #5");                              // slot base = table + index * 32
    emitter.instruction("ldr x7, [x6]");                                        // stored protocol pointer
    emitter.instruction("cbz x7, __rt_swr_dup_next");                           // skip empty slots
    emitter.instruction("ldr x8, [x6, #8]");                                    // stored protocol length
    emitter.instruction("ldr x10, [sp, #24]");                                  // requested protocol length
    emitter.instruction("cmp x8, x10");
    emitter.instruction("b.ne __rt_swr_dup_next");
    emitter.instruction("ldr x9, [sp, #16]");                                   // requested protocol pointer
    emitter.instruction("mov x11, #0");                                         // byte cursor
    emitter.label("__rt_swr_dup_cmp");
    emitter.instruction("cmp x11, x10");                                        // compared every byte?
    emitter.instruction("b.hs __rt_swr_dup_proto");                             // the names agree
    emitter.instruction("ldrb w12, [x7, x11]");
    emitter.instruction("ldrb w13, [x9, x11]");
    emitter.instruction("cmp w12, w13");
    emitter.instruction("b.ne __rt_swr_dup_next");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_swr_dup_cmp");
    emitter.label("__rt_swr_dup_next");
    emitter.instruction("add x5, x5, #1");
    emitter.instruction("b __rt_swr_dup_scan");

    emitter.label("__rt_swr_dup_builtin");
    // A built-in occupies its name too, so `stream_wrapper_register("file", …)` is false — unless
    // it was UNREGISTERED, which frees the name. That is the same bitmask
    // `stream_wrapper_unregister` maintains, read rather than duplicated.
    emitter.instruction("ldr x0, [sp, #16]");
    emitter.instruction("ldr x1, [sp, #24]");
    abi::emit_call_label(emitter, "__rt_builtin_wrapper_index");                // x0 = built-in index or -1
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.lt __rt_swr_proto_ok");                              // not a built-in name either
    abi::emit_symbol_address(emitter, "x9", "_disabled_builtin_wrappers");
    emitter.instruction("ldr x10, [x9]");                                       // current disabled mask
    emitter.instruction("mov x11, #1");
    emitter.instruction("lsl x11, x11, x0");                                    // the bit for this wrapper
    emitter.instruction("tst x10, x11");
    emitter.instruction("b.ne __rt_swr_proto_ok");                              // unregistered: the name is free again
    emitter.instruction("b __rt_swr_dup_proto");
    emitter.label("__rt_swr_proto_ok");

    // -- reserve a free slot, growing the table when every slot is taken --
    // Reserving happens BEFORE persisting so a failed reservation cannot leak
    // owned copies. Past the two refusals above, the only failure mode left is
    // heap exhaustion, which the helper reports as -1.
    emitter.instruction("bl __rt_user_wrappers_reserve");                       // x0 = free slot index (-1 on heap exhaustion)
    emitter.instruction("cmp x0, #0");                                          // did the reservation fail?
    emitter.instruction("b.lt __rt_swr_full");                                  // report false when no slot could be reserved
    emitter.instruction("mov x5, x0");                                          // wrapper slot index

    // -- copy both names onto the heap so the table never aliases caller storage --
    // A PHP-level `$scheme = "dyn" . $i;` reuses one local slot per iteration, so
    // storing the borrowed pointer made every registration alias the final value.
    emitter.label("__rt_swr_found");
    emitter.instruction("str x5, [sp, #56]");                                   // remember the target slot index across the calls
    emitter.instruction("ldr x1, [sp, #16]");                                   // borrowed protocol pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // protocol length
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned protocol copy
    emitter.instruction("str x1, [sp, #16]");                                   // replace the spill with the owned pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // borrowed class-name pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // class-name length
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned class-name copy
    emitter.instruction("str x1, [sp, #32]");                                   // replace the spill with the owned pointer

    // -- store the registration into the reserved slot --
    // The slot base is re-derived: the persist calls clobber the caller-saved scratch.
    super::emit_load_table_base(emitter, "x6");
    emitter.instruction("ldr x5, [sp, #56]");                                   // reload the target slot index
    emitter.instruction("add x7, x6, x5, lsl #5");                              // slot base = table + index * 32
    emitter.instruction("ldr x9, [sp, #16]");
    emitter.instruction("str x9, [x7]");                                        // owned protocol pointer
    emitter.instruction("ldr x9, [sp, #24]");
    emitter.instruction("str x9, [x7, #8]");                                    // protocol length
    emitter.instruction("ldr x9, [sp, #32]");
    emitter.instruction("str x9, [x7, #16]");                                   // owned class-name pointer
    emitter.instruction("ldr x9, [sp, #40]");
    emitter.instruction("str x9, [x7, #24]");                                   // class-name length
    super::emit_load_flags_base(emitter, "x10");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the registration flags
    emitter.instruction("str x9, [x10, x5, lsl #3]");                           // store definition flags beside the registration slot
    emitter.instruction("mov x0, #1");                                          // return true for a successful registration
    emitter.instruction("ldp x29, x30, [sp], #80");                             // tear down the frame
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swr_full");
    emitter.instruction("mov x0, #0");                                          // return false when the table is full
    emitter.instruction("ldp x29, x30, [sp], #80");                             // tear down the frame
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swr_bad_proto");
    abi::emit_symbol_address(emitter, "x1", "_swr_bad_proto_msg");
    emitter.instruction(&format!("mov x2, #{}", BAD_PROTOCOL_WARNING.len()));
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // `@` suppresses it, as it does in php
    emitter.instruction("mov x0, #0");                                          // php answers false for a malformed protocol
    emitter.instruction("ldp x29, x30, [sp], #80");                             // tear down the frame
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swr_dup_proto");
    abi::emit_symbol_address(emitter, "x1", "_swr_dup_proto_msg");
    emitter.instruction(&format!("mov x2, #{}", DUPLICATE_PROTOCOL_WARNING.len()));
    abi::emit_call_label(emitter, "__rt_diag_warning");
    emitter.instruction("mov x0, #0");                                          // php answers false for a name already taken
    emitter.instruction("ldp x29, x30, [sp], #80");                             // tear down the frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 stream runtime helper for stream wrapper register.
fn emit_stream_wrapper_register_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_register ---");
    emitter.label_global("__rt_stream_wrapper_register");
    // The registration outlives every caller-owned buffer, so the helper needs a
    // real frame: `__rt_str_persist` is a call and would otherwise clobber scratch.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the spill-area frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve the persisted-argument spill slots (keeps rsp 16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // spill the borrowed protocol pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // spill the protocol length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // spill the borrowed class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // spill the class-name length

    // -- php burns a resource id here; see the AArch64 arm --
    abi::emit_symbol_address(emitter, "r9", "_resource_id_next");
    emitter.instruction("mov r10, QWORD PTR [r9]");
    emitter.instruction("inc r10");                                             // the id php spends on the registration
    emitter.instruction("mov QWORD PTR [r9], r10");
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // spill the registration flags

    // -- php refuses a protocol holding a byte outside [A-Za-z0-9+.-] --
    // See the AArch64 arm: an EMPTY protocol registers successfully, so this is a per-byte filter.
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // the borrowed protocol pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // the protocol length
    emitter.instruction("xor r11, r11");                                        // byte cursor
    emitter.label("__rt_swr_char_scan_x");
    emitter.instruction("cmp r11, r10");                                        // examined every byte?
    emitter.instruction("jae __rt_swr_chars_ok_x");
    emitter.instruction("movzx eax, BYTE PTR [r9 + r11]");
    emitter.instruction("cmp eax, 0x30");                                       // below '0' — the three punctuation bytes live there
    emitter.instruction("jb __rt_swr_char_punct_x");
    emitter.instruction("cmp eax, 0x39");                                       // '9'
    emitter.instruction("jbe __rt_swr_char_next_x");
    emitter.instruction("cmp eax, 0x41");                                       // 'A'
    emitter.instruction("jb __rt_swr_bad_proto_x");
    emitter.instruction("cmp eax, 0x5a");                                       // 'Z'
    emitter.instruction("jbe __rt_swr_char_next_x");
    emitter.instruction("cmp eax, 0x61");                                       // 'a'
    emitter.instruction("jb __rt_swr_bad_proto_x");
    emitter.instruction("cmp eax, 0x7a");                                       // 'z'
    emitter.instruction("jbe __rt_swr_char_next_x");
    emitter.instruction("jmp __rt_swr_bad_proto_x");                            // above 'z', including every non-ASCII byte
    emitter.label("__rt_swr_char_punct_x");
    emitter.instruction("cmp eax, 0x2b");                                       // '+'
    emitter.instruction("je __rt_swr_char_next_x");
    emitter.instruction("cmp eax, 0x2d");                                       // '-'
    emitter.instruction("je __rt_swr_char_next_x");
    emitter.instruction("cmp eax, 0x2e");                                       // '.'
    emitter.instruction("je __rt_swr_char_next_x");
    emitter.instruction("jmp __rt_swr_bad_proto_x");
    emitter.label("__rt_swr_char_next_x");
    emitter.instruction("inc r11");
    emitter.instruction("jmp __rt_swr_char_scan_x");
    emitter.label("__rt_swr_chars_ok_x");

    // -- php refuses a protocol that is already registered --
    // See the AArch64 arm: byte-exact, because registration is case-SENSITIVE.
    super::emit_load_table_base(emitter, "rax");
    emitter.instruction("xor r9, r9");                                          // wrapper slot index
    emitter.label("__rt_swr_dup_scan_x");
    super::emit_load_table_cap(emitter, "r10");
    emitter.instruction("cmp r9, r10");                                         // scanned every allocated slot?
    emitter.instruction("jge __rt_swr_dup_builtin_x");
    emitter.instruction("mov r11, r9");
    emitter.instruction("shl r11, 5");                                          // slot offset = index * 32
    emitter.instruction("add r11, rax");                                        // slot base
    emitter.instruction("mov rcx, QWORD PTR [r11]");                            // stored protocol pointer
    emitter.instruction("test rcx, rcx");
    emitter.instruction("jz __rt_swr_dup_next_x");                              // skip empty slots
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // requested protocol length
    emitter.instruction("cmp QWORD PTR [r11 + 8], rsi");                        // stored length
    emitter.instruction("jne __rt_swr_dup_next_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // requested protocol pointer
    emitter.instruction("xor rdx, rdx");                                        // byte cursor
    emitter.label("__rt_swr_dup_cmp_x");
    emitter.instruction("cmp rdx, rsi");                                        // compared every byte?
    emitter.instruction("jae __rt_swr_dup_proto_x");                            // the names agree
    emitter.instruction("movzx r8d, BYTE PTR [rcx + rdx]");
    emitter.instruction("cmp r8b, BYTE PTR [rdi + rdx]");
    emitter.instruction("jne __rt_swr_dup_next_x");
    emitter.instruction("inc rdx");
    emitter.instruction("jmp __rt_swr_dup_cmp_x");
    emitter.label("__rt_swr_dup_next_x");
    emitter.instruction("inc r9");
    emitter.instruction("jmp __rt_swr_dup_scan_x");

    emitter.label("__rt_swr_dup_builtin_x");
    // See the AArch64 arm: a built-in occupies its name unless it has been unregistered.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    abi::emit_call_label(emitter, "__rt_builtin_wrapper_index");                // rax = built-in index or -1
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jl __rt_swr_proto_ok_x");                              // not a built-in name either
    abi::emit_symbol_address(emitter, "r9", "_disabled_builtin_wrappers");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // current disabled mask
    emitter.instruction("mov rcx, rax");                                        // the shift count must live in cl
    emitter.instruction("mov r11, 1");
    emitter.instruction("shl r11, cl");                                         // the bit for this wrapper
    emitter.instruction("test r10, r11");
    emitter.instruction("jnz __rt_swr_proto_ok_x");                             // unregistered: the name is free again
    emitter.instruction("jmp __rt_swr_dup_proto_x");
    emitter.label("__rt_swr_proto_ok_x");

    // -- reserve a free slot, growing the table when every slot is taken --
    // Reserving happens BEFORE persisting so a failed reservation cannot leak
    // owned copies. PHP never refuses a registration, so the only failure mode
    // left is heap exhaustion, which the helper reports as -1.
    emitter.instruction("call __rt_user_wrappers_reserve");                     // rax = free slot index (-1 on heap exhaustion)
    emitter.instruction("test rax, rax");                                       // did the reservation fail?
    emitter.instruction("js __rt_swr_full_x86");                                // report false when no slot could be reserved
    emitter.instruction("mov r9, rax");                                         // wrapper slot index

    // -- copy both names onto the heap so the table never aliases caller storage --
    // A PHP-level `$scheme = "dyn" . $i;` reuses one local slot per iteration, so
    // storing the borrowed pointer made every registration alias the final value.
    // NOTE: __rt_str_persist consumes the source pointer in rax (not rdi) on x86_64.
    emitter.label("__rt_swr_found_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // remember the target slot index across the calls
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // borrowed protocol pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // protocol length
    emitter.instruction("call __rt_str_persist");                               // rax = owned protocol copy
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // replace the spill with the owned pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // borrowed class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // class-name length
    emitter.instruction("call __rt_str_persist");                               // rax = owned class-name copy
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // replace the spill with the owned pointer

    // -- store the registration into the reserved slot --
    // The slot base is re-derived: the persist calls clobber the caller-saved scratch.
    super::emit_load_table_base(emitter, "r8");                                 // wrapper table base
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the target slot index
    emitter.instruction("mov r10, r9");                                         // copy the slot index for scaling
    emitter.instruction("shl r10, 5");                                          // slot offset = index * 32
    emitter.instruction("add r10, r8");                                         // slot base = table + offset
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    emitter.instruction("mov QWORD PTR [r10], rax");                            // owned protocol pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // protocol length
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");
    emitter.instruction("mov QWORD PTR [r10 + 16], rax");                       // owned class-name pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");
    emitter.instruction("mov QWORD PTR [r10 + 24], rax");                       // class-name length
    super::emit_load_flags_base(emitter, "r10");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the registration flags
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], rax");                   // store definition flags beside the registration slot
    emitter.instruction("mov eax, 1");                                          // return true for a successful registration
    emitter.instruction("mov rsp, rbp");                                        // discard the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swr_full_x86");
    emitter.instruction("xor eax, eax");                                        // return false when the table is full
    emitter.instruction("mov rsp, rbp");                                        // discard the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller

    // `__rt_diag_warning` takes its pointer in RDI and its length in RSI here, where the AArch64
    // arm passes x1 and x2 — the registers are not the same and the two arms cannot be mirrored.
    emitter.label("__rt_swr_bad_proto_x");
    abi::emit_symbol_address(emitter, "rdi", "_swr_bad_proto_msg");
    emitter.instruction(&format!("mov rsi, {}", BAD_PROTOCOL_WARNING.len()));
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // `@` suppresses it, as it does in php
    emitter.instruction("xor eax, eax");                                        // php answers false for a malformed protocol
    emitter.instruction("mov rsp, rbp");                                        // discard the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swr_dup_proto_x");
    abi::emit_symbol_address(emitter, "rdi", "_swr_dup_proto_msg");
    emitter.instruction(&format!("mov rsi, {}", DUPLICATE_PROTOCOL_WARNING.len()));
    abi::emit_call_label(emitter, "__rt_diag_warning");
    emitter.instruction("xor eax, eax");                                        // php answers false for a name already taken
    emitter.instruction("mov rsp, rbp");                                        // discard the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
