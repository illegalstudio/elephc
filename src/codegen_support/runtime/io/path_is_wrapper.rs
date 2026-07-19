//! Purpose:
//! Emits `__rt_path_is_wrapper`, a reusable predicate that reports whether a
//! path's `scheme://` prefix matches a registered userspace stream wrapper.
//! Lets path-based builtins (e.g. `readfile()`) decide between the wrapper
//! dispatch and the real-filesystem path without instantiating the class.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The `readfile()` builtin (and future path-based builtins) before choosing
//!   their wrapper vs filesystem code path.
//!
//! Key details:
//! - The scheme scan / slot match mirrors the inlined logic in `__rt_fopen` and
//!   `__rt_user_wrapper_url_stat` (scan for "://", then byte-compare the scheme
//!   against each `_user_wrappers` slot). It performs no instantiation and has
//!   no side effects, so it is safe to call speculatively.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_path_is_wrapper(path_ptr, path_len) -> 1/0`.
///
/// Inputs: x0 = path pointer, x1 = path length (AArch64); rdi = path pointer,
/// rsi = path length (x86_64). Output: x0 / rax = 1 when the path begins with a
/// `scheme://` whose scheme matches a registered wrapper protocol, else 0.
pub fn emit_path_is_wrapper(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_path_is_wrapper_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: path_is_wrapper ---");
    emitter.label_global("__rt_path_is_wrapper");

    // -- scan the path for the "://" scheme separator (x0=ptr, x1=len) --
    emitter.instruction("mov x9, #0");                                          // scheme scan index
    emitter.label("__rt_piw_scan");
    emitter.instruction("add x10, x9, #3");                                     // need three bytes for the "://" marker
    emitter.instruction("cmp x10, x1");                                         // do enough bytes remain in the path?
    emitter.instruction("b.gt __rt_piw_no");                                    // no scheme separator → not a wrapper URL
    emitter.instruction("ldrb w11, [x0, x9]");                                  // load the candidate ':' byte
    emitter.instruction("cmp w11, #58");                                        // is it ':'?
    emitter.instruction("b.ne __rt_piw_next");                                  // not the scheme marker
    emitter.instruction("add x12, x9, #1");                                     // index of the first '/'
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the candidate first '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_piw_next");                                  // not the scheme marker
    emitter.instruction("add x12, x9, #2");                                     // index of the second '/'
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the candidate second '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_piw_next");                                  // not the scheme marker
    emitter.instruction("b __rt_piw_check");                                    // "://" found at index x9 — x9 is the scheme length
    emitter.label("__rt_piw_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_piw_scan");                                     // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (x9=scheme len) --
    emitter.label("__rt_piw_check");
    abi::emit_symbol_address(emitter, "x10", "_user_wrappers");
    emitter.instruction("mov x11, #0");                                         // wrapper slot index
    emitter.label("__rt_piw_slot");
    emitter.instruction("cmp x11, #64");                                        // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("b.ge __rt_piw_no");                                    // no registered wrapper matched the scheme
    emitter.instruction("add x12, x10, x11, lsl #5");                           // slot base = table + index * 32
    emitter.instruction("ldr x13, [x12]");                                      // stored protocol pointer
    emitter.instruction("cbz x13, __rt_piw_slot_next");                         // empty slot — skip it
    emitter.instruction("ldr x14, [x12, #8]");                                  // stored protocol length
    emitter.instruction("cmp x14, x9");                                         // does the stored length match the scheme length?
    emitter.instruction("b.ne __rt_piw_slot_next");                             // length mismatch — try the next slot
    emitter.instruction("mov x15, #0");                                         // byte compare index
    emitter.label("__rt_piw_bytes");
    emitter.instruction("cmp x15, x9");                                         // compared every protocol byte?
    emitter.instruction("b.ge __rt_piw_yes");                                   // full match — the scheme is a registered wrapper
    emitter.instruction("ldrb w16, [x13, x15]");                                // stored protocol byte
    emitter.instruction("ldrb w17, [x0, x15]");                                 // path scheme byte
    emitter.instruction("cmp w16, w17");                                        // do the bytes match?
    emitter.instruction("b.ne __rt_piw_slot_next");                             // protocol byte differs — try the next slot
    emitter.instruction("add x15, x15, #1");                                    // advance the compare index
    emitter.instruction("b __rt_piw_bytes");                                    // continue comparing bytes
    emitter.label("__rt_piw_slot_next");
    emitter.instruction("add x11, x11, #1");                                    // advance the slot index
    emitter.instruction("b __rt_piw_slot");                                     // continue scanning slots

    emitter.label("__rt_piw_yes");
    emitter.instruction("mov x0, #1");                                          // matched a registered wrapper scheme
    emitter.instruction("ret");                                                 // return true
    emitter.label("__rt_piw_no");
    emitter.instruction("mov x0, #0");                                          // not a registered wrapper scheme
    emitter.instruction("ret");                                                 // return false
}

/// Emits the Linux x86_64 stream runtime helper for path is wrapper.
fn emit_path_is_wrapper_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: path_is_wrapper ---");
    emitter.label_global("__rt_path_is_wrapper");

    emitter.instruction("mov rax, rdi");                                        // path pointer → scan base register
    emitter.instruction("mov rdx, rsi");                                        // path length → scan bound register

    // -- scan the path for the "://" scheme separator (rax=ptr, rdx=len) --
    emitter.instruction("xor r9, r9");                                          // scheme scan index
    emitter.label("__rt_piw_scan_x86");
    emitter.instruction("lea r10, [r9 + 3]");                                   // need three bytes for the "://" marker
    emitter.instruction("cmp r10, rdx");                                        // do enough bytes remain in the path?
    emitter.instruction("jg __rt_piw_no_x86");                                  // no scheme separator → not a wrapper URL
    emitter.instruction("movzx r11d, BYTE PTR [rax + r9]");                     // load the candidate ':' byte
    emitter.instruction("cmp r11b, 58");                                        // is it ':'?
    emitter.instruction("jne __rt_piw_next_x86");                               // not the scheme marker
    emitter.instruction("lea r12, [r9 + 1]");                                   // index of the first '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate first '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_piw_next_x86");                               // not the scheme marker
    emitter.instruction("lea r12, [r9 + 2]");                                   // index of the second '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate second '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_piw_next_x86");                               // not the scheme marker
    emitter.instruction("jmp __rt_piw_check_x86");                              // "://" found at r9 — r9 is the scheme length
    emitter.label("__rt_piw_next_x86");
    emitter.instruction("inc r9");                                              // advance the scan index
    emitter.instruction("jmp __rt_piw_scan_x86");                               // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (r9=scheme len) --
    emitter.label("__rt_piw_check_x86");
    abi::emit_symbol_address(emitter, "r10", "_user_wrappers");                 // wrapper table base
    emitter.instruction("xor r11, r11");                                        // wrapper slot index
    emitter.label("__rt_piw_slot_x86");
    emitter.instruction("cmp r11, 64");                                         // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("jge __rt_piw_no_x86");                                 // no registered wrapper matched the scheme
    emitter.instruction("mov r12, r11");                                        // copy the slot index for scaling
    emitter.instruction("shl r12, 5");                                          // slot offset = index * 32
    emitter.instruction("add r12, r10");                                        // slot base = table + offset
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // stored protocol pointer
    emitter.instruction("test r13, r13");                                       // is this slot empty?
    emitter.instruction("jz __rt_piw_slot_next_x86");                           // skip empty slots
    emitter.instruction("mov r14, QWORD PTR [r12 + 8]");                        // stored protocol length
    emitter.instruction("cmp r14, r9");                                         // does the stored length match the scheme length?
    emitter.instruction("jne __rt_piw_slot_next_x86");                          // length mismatch — try the next slot
    emitter.instruction("xor r15, r15");                                        // byte compare index
    emitter.label("__rt_piw_bytes_x86");
    emitter.instruction("cmp r15, r9");                                         // compared every protocol byte?
    emitter.instruction("jge __rt_piw_yes_x86");                                // full match — the scheme is a registered wrapper
    emitter.instruction("movzx ecx, BYTE PTR [r13 + r15]");                     // stored protocol byte
    emitter.instruction("movzx r8d, BYTE PTR [rax + r15]");                     // path scheme byte
    emitter.instruction("cmp cl, r8b");                                         // do the bytes match?
    emitter.instruction("jne __rt_piw_slot_next_x86");                          // protocol byte differs — try the next slot
    emitter.instruction("inc r15");                                             // advance the compare index
    emitter.instruction("jmp __rt_piw_bytes_x86");                              // continue comparing bytes
    emitter.label("__rt_piw_slot_next_x86");
    emitter.instruction("inc r11");                                             // advance the slot index
    emitter.instruction("jmp __rt_piw_slot_x86");                               // continue scanning slots

    emitter.label("__rt_piw_yes_x86");
    emitter.instruction("mov eax, 1");                                          // matched a registered wrapper scheme
    emitter.instruction("ret");                                                 // return true
    emitter.label("__rt_piw_no_x86");
    emitter.instruction("xor eax, eax");                                        // not a registered wrapper scheme
    emitter.instruction("ret");                                                 // return false
}
