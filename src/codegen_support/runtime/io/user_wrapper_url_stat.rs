//! Purpose:
//! Emits `__rt_user_wrapper_url_stat`, the path-based stat dispatcher for
//! userspace stream wrappers. Given a `scheme://...` path it scans the
//! registered-wrapper table, instantiates the matching class, calls its
//! `url_stat($path, $flags)` method (vtable slot 9), and returns the boxed
//! Mixed stat array. Backs `file_exists()`/`is_file()`/`filesize()` on
//! `scheme://` URLs.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The file_exists/is_file/filesize builtin emitters call it before their
//!   normal filesystem path and branch on the `_url_stat_matched` out-flag.
//!
//! Key details:
//! - `_url_stat_matched` is set to 1 only when the path's scheme matches a
//!   registered wrapper, distinguishing "not a wrapper URL → fall back to the
//!   real filesystem" from "the wrapper reported the path absent → false".
//! - The scheme scan / slot match mirrors the inlined logic in
//!   `__rt_fopen`. The throwaway wrapper instance is freed with
//!   `__rt_decref_any` once `url_stat` returns; the boxed array is normalized
//!   by the shared `__rt_box_wrapper_stat_result`.
//! - `__rt_new_by_name` takes the class name in x1/x2 (AArch64) or rax/rdx
//!   (x86_64), NOT the SysV argument registers. The method call uses the
//!   regular elephc method ABI (`$this`, then a string pair, then the int flag).

use super::MIN_WRAPPER_SCHEME_LEN;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Byte offset of the url_stat method pointer in the per-class user-wrapper
/// vtable (slot 9 of `USER_WRAPPER_VTABLE_SLOTS`, 8 bytes per slot).
const VTABLE_URL_STAT_OFFSET: usize = 9 * 8;

/// Emits `__rt_user_wrapper_url_stat(path_ptr, path_len, flags)`.
///
/// On a registered scheme match it sets `_url_stat_matched = 1` and returns the
/// wrapper's `url_stat()` result boxed as a Mixed cell (an associative stat
/// array, or `false` when the class/method is missing or the wrapper reports
/// the path absent). On no match it sets `_url_stat_matched = 0` and returns 0
/// so the caller falls back to the real filesystem. Dispatches by target.
pub fn emit_user_wrapper_url_stat(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_url_stat_linux_x86_64(emitter);
        return;
    }
    emit_user_wrapper_url_stat_aarch64(emitter);
}

/// AArch64 implementation of `__rt_user_wrapper_url_stat`.
///
/// Inputs: x0 = path pointer, x1 = path length, x2 = `url_stat` flags.
/// Output: x0 = boxed Mixed result (valid when `_url_stat_matched` is 1).
fn emit_user_wrapper_url_stat_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat ---");
    emitter.label_global("__rt_user_wrapper_url_stat");

    // Frame: 64 bytes. [sp,#0..16] x29/x30, [sp,#16] path ptr, [sp,#24] path
    //   len, [sp,#32] flags, [sp,#48] obj, [sp,#56] boxed result.
    emitter.instruction("sub sp, sp, #64");                                     // helper frame for the path-stat dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the path pointer across the helper calls
    emitter.instruction("str x1, [sp, #24]");                                   // save the path length across the helper calls
    emitter.instruction("str x2, [sp, #32]");                                   // save the url_stat flags across the helper calls
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_url_stat_failure_kind");
    emitter.instruction("str xzr, [x9]");                                       // clear the prior url_stat failure discriminator
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_url_stat_class_ptr");
    emitter.instruction("str xzr, [x9]");                                       // clear stale wrapper-class bytes
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_url_stat_class_len");
    emitter.instruction("str xzr, [x9]");                                       // clear the stale wrapper-class byte count

    // -- scan the path for the "://" scheme separator (x0=ptr, x1=len) --
    emitter.instruction(&format!("mov x9, #{}", MIN_WRAPPER_SCHEME_LEN));       // scheme scan index: a one-letter scheme is never a wrapper
    emitter.label("__rt_uus_scan");
    emitter.instruction("add x10, x9, #3");                                     // need three bytes for the "://" marker
    emitter.instruction("cmp x10, x1");                                         // do enough bytes remain in the path?
    emitter.instruction("b.gt __rt_uus_nomatch");                               // no scheme separator → not a wrapper URL
    emitter.instruction("ldrb w11, [x0, x9]");                                  // load the candidate ':' byte
    emitter.instruction("cmp w11, #58");                                        // is it ':'?
    emitter.instruction("b.ne __rt_uus_scan_next");                             // not the scheme marker
    emitter.instruction("add x12, x9, #1");                                     // index of the first '/'
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the candidate first '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_uus_scan_next");                             // not the scheme marker
    emitter.instruction("add x12, x9, #2");                                     // index of the second '/'
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the candidate second '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_uus_scan_next");                             // not the scheme marker
    emitter.instruction("b __rt_uus_check");                                    // "://" found at index x9 — x9 is the scheme length
    emitter.label("__rt_uus_scan_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_uus_scan");                                     // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (x9=scheme len) --
    emitter.label("__rt_uus_check");
    abi::emit_symbol_address(emitter, "x10", "_user_wrappers");
    emitter.instruction("mov x11, #0");                                         // wrapper slot index
    emitter.label("__rt_uus_slot");
    emitter.instruction("cmp x11, #64");                                        // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("b.ge __rt_uus_nomatch");                               // no registered wrapper matched the scheme
    emitter.instruction("add x12, x10, x11, lsl #5");                           // slot base = table + index * 32
    emitter.instruction("ldr x13, [x12]");                                      // stored protocol pointer
    emitter.instruction("cbz x13, __rt_uus_slot_next");                         // empty slot — skip it
    emitter.instruction("ldr x14, [x12, #8]");                                  // stored protocol length
    emitter.instruction("cmp x14, x9");                                         // does the stored length match the scheme length?
    emitter.instruction("b.ne __rt_uus_slot_next");                             // length mismatch — try the next slot
    emitter.instruction("mov x15, #0");                                         // byte compare index
    emitter.label("__rt_uus_bytes");
    emitter.instruction("cmp x15, x9");                                         // compared every protocol byte?
    emitter.instruction("b.ge __rt_uus_match");                                 // full match — dispatch into the wrapper class
    emitter.instruction("ldrb w16, [x13, x15]");                                // stored protocol byte
    emitter.instruction("ldrb w17, [x0, x15]");                                 // path scheme byte
    emitter.instruction("cmp w16, w17");                                        // do the bytes match?
    emitter.instruction("b.ne __rt_uus_slot_next");                             // protocol byte differs — try the next slot
    emitter.instruction("add x15, x15, #1");                                    // advance the compare index
    emitter.instruction("b __rt_uus_bytes");                                    // continue comparing bytes
    emitter.label("__rt_uus_slot_next");
    emitter.instruction("add x11, x11, #1");                                    // advance the slot index
    emitter.instruction("b __rt_uus_slot");                                     // continue scanning slots

    // -- matched scheme: x12 = registry slot base --
    emitter.label("__rt_uus_match");
    abi::emit_symbol_address(emitter, "x10", "_url_stat_matched");
    emitter.instruction("mov w9, #1");                                          // record that a registered wrapper scheme matched
    emitter.instruction("strb w9, [x10]");                                      // set _url_stat_matched = 1 (do not fall back to the filesystem)
    emitter.instruction("ldr x13, [x12, #16]");                                 // load the matched wrapper class-name pointer
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_url_stat_class_ptr");
    emitter.instruction("str x13, [x10]");                                      // publish class bytes for a later DOM warning
    emitter.instruction("ldr x13, [x12, #24]");                                 // load the matched wrapper class-name length
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_url_stat_class_len");
    emitter.instruction("str x13, [x10]");                                      // publish the class byte count for a later DOM warning
    emitter.instruction("ldr x1, [x12, #16]");                                  // wrapper class name pointer from the registry slot
    emitter.instruction("ldr x2, [x12, #24]");                                  // wrapper class name length from the registry slot
    emitter.instruction("bl __rt_new_by_name");                                 // instantiate the wrapper class → x0 = obj, or 0 when unknown
    emitter.instruction("cbz x0, __rt_uus_false");                              // unknown class → boxed false
    emitter.instruction("str x0, [sp, #48]");                                   // save the throwaway wrapper instance
    emitter.instruction("bl __rt_user_wrapper_apply_context");                  // expose libxml's selected context before url_stat()
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the wrapper after context-property injection

    // -- look up url_stat in the per-class user-wrapper vtable (slot 9) --
    emitter.instruction("ldr x9, [x0]");                                        // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("ldr x11, [x10, #{}]", VTABLE_URL_STAT_OFFSET)); // load the url_stat method pointer (slot 9)
    emitter.instruction("cbz x11, __rt_uus_missing_obj");                       // class did not implement url_stat → classified boxed false

    // -- call url_stat($this, $path, $flags) → x0 = raw return --
    emitter.instruction("ldr x0, [sp, #48]");                                   // $this = wrapper object
    emitter.instruction("ldr x1, [sp, #16]");                                   // path ptr → string-arg pair
    emitter.instruction("ldr x2, [sp, #24]");                                   // path len → string-arg pair
    emitter.instruction("ldr x3, [sp, #32]");                                   // url_stat flags
    emitter.instruction("blr x11");                                             // invoke url_stat on the throwaway wrapper object
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_url_stat_failure_kind");
    emitter.instruction("str xzr, [x9]");                                       // an implemented url_stat result is never a missing-method failure
    emitter.instruction("bl __rt_box_wrapper_stat_result");                     // normalize the type-erased return into a boxed Mixed
    emitter.instruction("str x0, [sp, #56]");                                   // save the boxed result across the wrapper-instance release
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the throwaway wrapper object
    emitter.instruction("bl __rt_decref_any");                                  // free the throwaway wrapper instance
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the boxed result for return
    emitter.instruction("b __rt_uus_ret");                                      // share the common return path

    emitter.label("__rt_uus_missing_obj");
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the throwaway wrapper object
    emitter.instruction("ldr x9, [x0]");                                        // preserve its class id across destructor re-entry
    emitter.instruction("str x9, [sp, #40]");                                   // keep the class identity in the unused frame slot
    emitter.instruction("bl __rt_decref_any");                                  // release it before publishing stable failure metadata
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the stable wrapper class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // address the class's immutable name metadata
    emitter.instruction("ldr x11, [x10]");                                      // load the wrapper class-name pointer
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_url_stat_class_ptr");
    emitter.instruction("str x11, [x9]");                                       // restore class bytes after destructor re-entry
    emitter.instruction("ldr x11, [x10, #8]");                                  // load the wrapper class-name length
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_url_stat_class_len");
    emitter.instruction("str x11, [x9]");                                       // restore the class byte count after destructor re-entry
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_url_stat_failure_kind");
    emitter.instruction("mov x10, #1");                                         // failure kind one means url_stat is missing
    emitter.instruction("str x10, [x9]");                                       // publish the exact missing-method failure
    emitter.instruction("b __rt_uus_false");                                    // box false without releasing the object twice
    emitter.label("__rt_uus_false");
    emitter.instruction("mov x0, #0");                                          // null sentinel → boxed false (scheme matched, stat unavailable)
    emitter.instruction("bl __rt_box_wrapper_stat_result");                     // produce boxed false; _url_stat_matched stays 1
    emitter.instruction("b __rt_uus_ret");                                      // share the common return path

    emitter.label("__rt_uus_nomatch");
    abi::emit_symbol_address(emitter, "x10", "_url_stat_matched");
    emitter.instruction("strb wzr, [x10]");                                     // _url_stat_matched = 0 — caller falls back to the real filesystem
    emitter.instruction("mov x0, #0");                                          // return 0; the caller ignores it when the flag is 0

    emitter.label("__rt_uus_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed Mixed result (or 0 on no match)
}

/// x86_64 implementation of `__rt_user_wrapper_url_stat`.
///
/// Inputs: rdi = path pointer, rsi = path length, rdx = `url_stat` flags.
/// Output: rax = boxed Mixed result (valid when `_url_stat_matched` is 1).
fn emit_user_wrapper_url_stat_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat ---");
    emitter.label_global("__rt_user_wrapper_url_stat");

    // Frame: [rbp-8] path ptr, [rbp-16] path len, [rbp-24] flags, [rbp-32] obj,
    //   [rbp-40] boxed result. push rbp then sub rsp,64 keeps rsp 16-aligned.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // spill slots for path/flags/obj/result
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the path pointer across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the path length across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the url_stat flags across the helper calls
    abi::emit_store_zero_to_symbol(emitter, "_user_wrapper_url_stat_failure_kind", 0);
    abi::emit_store_zero_to_symbol(emitter, "_user_wrapper_url_stat_class_ptr", 0);
    abi::emit_store_zero_to_symbol(emitter, "_user_wrapper_url_stat_class_len", 0);
    emitter.instruction("mov rax, rdi");                                        // path pointer → scan base register
    emitter.instruction("mov rdx, rsi");                                        // path length → scan bound register

    // -- scan the path for the "://" scheme separator (rax=ptr, rdx=len) --
    emitter.instruction(&format!("mov r9d, {}", MIN_WRAPPER_SCHEME_LEN));       // scheme scan index: a one-letter scheme is never a wrapper
    emitter.label("__rt_uus_scan_x86");
    emitter.instruction("lea r10, [r9 + 3]");                                   // need three bytes for the "://" marker
    emitter.instruction("cmp r10, rdx");                                        // do enough bytes remain in the path?
    emitter.instruction("jg __rt_uus_nomatch_x86");                             // no scheme separator → not a wrapper URL
    emitter.instruction("movzx r11d, BYTE PTR [rax + r9]");                     // load the candidate ':' byte
    emitter.instruction("cmp r11b, 58");                                        // is it ':'?
    emitter.instruction("jne __rt_uus_next_x86");                               // not the scheme marker
    emitter.instruction("lea r12, [r9 + 1]");                                   // index of the first '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate first '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_uus_next_x86");                               // not the scheme marker
    emitter.instruction("lea r12, [r9 + 2]");                                   // index of the second '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate second '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_uus_next_x86");                               // not the scheme marker
    emitter.instruction("jmp __rt_uus_check_x86");                              // "://" found at r9 — r9 is the scheme length
    emitter.label("__rt_uus_next_x86");
    emitter.instruction("inc r9");                                              // advance the scan index
    emitter.instruction("jmp __rt_uus_scan_x86");                               // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (r9=scheme len) --
    emitter.label("__rt_uus_check_x86");
    abi::emit_symbol_address(emitter, "r10", "_user_wrappers");                 // wrapper table base
    emitter.instruction("xor r11, r11");                                        // wrapper slot index
    emitter.label("__rt_uus_slot_x86");
    emitter.instruction("cmp r11, 64");                                         // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("jge __rt_uus_nomatch_x86");                            // no registered wrapper matched the scheme
    emitter.instruction("mov r12, r11");                                        // copy the slot index for scaling
    emitter.instruction("shl r12, 5");                                          // slot offset = index * 32
    emitter.instruction("add r12, r10");                                        // slot base = table + offset
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // stored protocol pointer
    emitter.instruction("test r13, r13");                                       // is this slot empty?
    emitter.instruction("jz __rt_uus_slot_next_x86");                           // skip empty slots
    emitter.instruction("mov r14, QWORD PTR [r12 + 8]");                        // stored protocol length
    emitter.instruction("cmp r14, r9");                                         // does the stored length match the scheme length?
    emitter.instruction("jne __rt_uus_slot_next_x86");                          // length mismatch — try the next slot
    emitter.instruction("xor r15, r15");                                        // byte compare index
    emitter.label("__rt_uus_bytes_x86");
    emitter.instruction("cmp r15, r9");                                         // compared every protocol byte?
    emitter.instruction("jge __rt_uus_match_x86");                              // full match — dispatch into the wrapper class
    emitter.instruction("movzx ecx, BYTE PTR [r13 + r15]");                     // stored protocol byte
    emitter.instruction("movzx r8d, BYTE PTR [rax + r15]");                     // path scheme byte
    emitter.instruction("cmp cl, r8b");                                         // do the bytes match?
    emitter.instruction("jne __rt_uus_slot_next_x86");                          // protocol byte differs — try the next slot
    emitter.instruction("inc r15");                                             // advance the compare index
    emitter.instruction("jmp __rt_uus_bytes_x86");                              // continue comparing bytes
    emitter.label("__rt_uus_slot_next_x86");
    emitter.instruction("inc r11");                                             // advance the slot index
    emitter.instruction("jmp __rt_uus_slot_x86");                               // continue scanning slots

    // -- matched scheme: r12 = registry slot base --
    emitter.label("__rt_uus_match_x86");
    abi::emit_symbol_address(emitter, "r10", "_url_stat_matched");              // out-flag address
    emitter.instruction("mov BYTE PTR [r10], 1");                               // set _url_stat_matched = 1 (do not fall back to the filesystem)
    emitter.instruction("mov r13, QWORD PTR [r12 + 16]");                       // load the matched wrapper class-name pointer
    abi::emit_store_reg_to_symbol(emitter, "r13", "_user_wrapper_url_stat_class_ptr", 0);
    emitter.instruction("mov r13, QWORD PTR [r12 + 24]");                       // load the matched wrapper class-name length
    abi::emit_store_reg_to_symbol(emitter, "r13", "_user_wrapper_url_stat_class_len", 0);
    emitter.instruction("mov rax, QWORD PTR [r12 + 16]");                       // wrapper class name pointer from the registry slot
    emitter.instruction("mov rdx, QWORD PTR [r12 + 24]");                       // wrapper class name length (new_by_name reads rax/rdx)
    emitter.instruction("call __rt_new_by_name");                               // instantiate the wrapper class → rax = obj, or 0 when unknown
    emitter.instruction("test rax, rax");                                       // unknown class?
    emitter.instruction("jz __rt_uus_false_x86");                               // unknown class → boxed false
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the throwaway wrapper instance
    emitter.instruction("mov rdi, rax");                                        // pass the throwaway wrapper to context injection
    emitter.instruction("call __rt_user_wrapper_apply_context");                // expose libxml's selected context before url_stat()
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the wrapper after context-property injection

    // -- look up url_stat in the per-class user-wrapper vtable (slot 9) --
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_vtable_ptrs");      // base of the per-class user-wrapper vtable pointer table
    emitter.instruction("mov r10, QWORD PTR [r10 + r9 * 8]");                   // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {}]", VTABLE_URL_STAT_OFFSET)); // load the url_stat method pointer (slot 9)
    emitter.instruction("test r11, r11");                                       // class did not implement url_stat?
    emitter.instruction("jz __rt_uus_missing_obj_x86");                         // no url_stat → classified boxed false

    // -- call url_stat($this, $path, $flags) → rax = raw return --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // $this = wrapper object
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // path ptr → string-arg pair
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // path len → string-arg pair
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // url_stat flags
    emitter.instruction("call r11");                                            // invoke url_stat on the throwaway wrapper object
    abi::emit_store_zero_to_symbol(emitter, "_user_wrapper_url_stat_failure_kind", 0);
    emitter.instruction("call __rt_box_wrapper_stat_result");                   // normalize the type-erased return into a boxed Mixed
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the boxed result across the wrapper-instance release
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the throwaway wrapper object
    emitter.instruction("call __rt_decref_any");                                // free the throwaway wrapper instance
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed result for return
    emitter.instruction("jmp __rt_uus_ret_x86");                                // share the common return path

    emitter.label("__rt_uus_missing_obj_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the throwaway wrapper object
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // preserve its class id across destructor re-entry
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // keep the class identity in an unused frame slot
    emitter.instruction("call __rt_decref_any");                                // release it before publishing stable failure metadata
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the stable wrapper class id
    abi::emit_symbol_address(emitter, "r11", "_class_name_entries");
    emitter.instruction("shl r10, 4");                                          // scale the class id to a 16-byte metadata row
    emitter.instruction("add r11, r10");                                        // address the class's immutable name metadata
    emitter.instruction("mov r10, QWORD PTR [r11]");                            // load the wrapper class-name pointer
    abi::emit_store_reg_to_symbol(emitter, "r10", "_user_wrapper_url_stat_class_ptr", 0);
    emitter.instruction("mov r10, QWORD PTR [r11 + 8]");                        // load the wrapper class-name length
    abi::emit_store_reg_to_symbol(emitter, "r10", "_user_wrapper_url_stat_class_len", 0);
    emitter.instruction("mov r10, 1");                                          // failure kind one means url_stat is missing
    abi::emit_store_reg_to_symbol(emitter, "r10", "_user_wrapper_url_stat_failure_kind", 0);
    emitter.instruction("jmp __rt_uus_false_x86");                              // box false without releasing the object twice
    emitter.label("__rt_uus_false_x86");
    emitter.instruction("xor eax, eax");                                        // null sentinel → boxed false (scheme matched, stat unavailable)
    emitter.instruction("call __rt_box_wrapper_stat_result");                   // produce boxed false; _url_stat_matched stays 1
    emitter.instruction("jmp __rt_uus_ret_x86");                                // share the common return path

    emitter.label("__rt_uus_nomatch_x86");
    abi::emit_symbol_address(emitter, "r10", "_url_stat_matched");              // out-flag address
    emitter.instruction("mov BYTE PTR [r10], 0");                               // _url_stat_matched = 0 — caller falls back to the real filesystem
    emitter.instruction("xor eax, eax");                                        // return 0; the caller ignores it when the flag is 0

    emitter.label("__rt_uus_ret_x86");
    emitter.instruction("add rsp, 64");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed result (or 0 on no match)
}

/// Emits `__rt_user_wrapper_url_stat_field(path_ptr, path_len, field_sel, flags)`.
///
/// Calls `__rt_user_wrapper_url_stat` (which sets `_url_stat_matched`) and reads
/// the stat array it returns. The selector picks what comes back:
///
/// | sel | key(s) read        | result                                    |
/// |-----|--------------------|-------------------------------------------|
/// | 0   | `size`             | the integer field, or `-1`                |
/// | 1   | `mode`             | the integer field, or `-1`                |
/// | 2   | `mtime`            | the integer field, or `-1`                |
/// | 3   | `mode`+`uid`+`gid` | `is_readable()` as 0/1, or 0              |
/// | 4   | `mode`+`uid`+`gid` | `is_writable()` as 0/1, or 0              |
/// | 5   | `mode`+`uid`+`gid` | `is_executable()` as 0/1, or 0            |
///
/// Backs the whole stat family on `scheme://` URLs; the caller reads
/// `_url_stat_matched` to choose between this result and the real-filesystem
/// fallback. The boolean selectors report a plain `false` rather than the `-1`
/// sentinel, because their callers store the answer straight into a PHP bool
/// where `-1` would read as true.
///
/// The three boolean selectors read three keys from ONE `url_stat` call: PHP
/// calls a wrapper's `url_stat()` once per predicate, and reading the fields
/// through separate calls would make a wrapper with side effects observe a
/// second one.
///
/// Integer selectors return the field in `x0`/`rax` plus a success flag in `x1`/`rdx`; the flag is
/// clear whenever the payload is the `-1` sentinel. `-1` alone could not distinguish "absent" from
/// a real field value for callers that must box PHP `false`, and it is a value a wrapper is free to
/// report. `is_file()` reads the payload register only, so the flag is inert for it. Reuses the
/// boxed-Mixed reader (`__rt_mixed_array_get`) with a `__rt_hash_normalize_key`-normalized string
/// key, then releases both the field box and the stat-array box.
pub fn emit_user_wrapper_url_stat_field(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_url_stat_field_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_field ---");
    emitter.label_global("__rt_user_wrapper_url_stat_field");

    // Frame: 80 bytes. [sp,#0..16] x29/x30, [sp,#16] field_sel, [sp,#24] stat
    //   Mixed, [sp,#32] primary field, [sp,#40] found flag, [sp,#48] uid,
    //   [sp,#56] gid.
    emitter.instruction("sub sp, sp, #80");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the field selector (see the table above)
    emitter.instruction("mov x2, x3");                                          // url_stat flags chosen by the calling builtin, not a fixed 0
    emitter.instruction("bl __rt_user_wrapper_url_stat");                       // x0 = boxed Mixed stat array (sets _url_stat_matched)
    emitter.instruction("cbz x0, __rt_uusf_fail");                              // scheme not matched / null → sentinel (caller ignores when unmatched)
    emitter.instruction("ldr x9, [x0]");                                        // boxed Mixed runtime tag
    emitter.instruction("cmp x9, #3");                                          // wrapper reported the path absent (boxed false)?
    emitter.instruction("b.eq __rt_uusf_fail_box");                             // → release the false box and return the sentinel
    emitter.instruction("str x0, [sp, #24]");                                   // save the stat-array Mixed across the key lookups
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the field selector
    emitter.instruction("cmp x10, #3");                                         // selectors 3..5 are the permission predicates
    emitter.instruction("b.ge __rt_uusf_access");                               // → read mode, uid and gid together

    // -- single integer field: select the stat-array key string --
    emitter.instruction("cmp x10, #1");                                         // selector 1 = 'mode'
    emitter.instruction("b.eq __rt_uusf_mode");
    emitter.instruction("cmp x10, #2");                                         // selector 2 = 'mtime'
    emitter.instruction("b.eq __rt_uusf_mtime");
    abi::emit_symbol_address(emitter, "x1", "_stat_key_size");
    emitter.instruction("mov x2, #4");                                          // strlen("size")
    emitter.instruction("b __rt_uusf_havekey");                                 // proceed with the size key
    emitter.label("__rt_uusf_mode");
    abi::emit_symbol_address(emitter, "x1", "_stat_key_mode");
    emitter.instruction("mov x2, #4");                                          // strlen("mode")
    emitter.instruction("b __rt_uusf_havekey");                                 // proceed with the mode key
    emitter.label("__rt_uusf_mtime");
    abi::emit_symbol_address(emitter, "x1", "_stat_key_mtime");
    emitter.instruction("mov x2, #5");                                          // strlen("mtime")
    emitter.label("__rt_uusf_havekey");
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed → reader receiver
    emitter.instruction("bl __rt_uusf_read");                                   // x0 = integer field, x1 = 1 when present and integral
    emitter.instruction("str x0, [sp, #32]");                                   // stash the field across the array release
    emitter.instruction("str x1, [sp, #40]");                                   // stash whether it was readable at all
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed stat array
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the found flag
    emitter.instruction("cbz x1, __rt_uusf_fail");                              // missing/non-int field → sentinel
    emitter.instruction("ldr x0, [sp, #32]");                                   // load the integer result
    emitter.instruction("mov x1, #1");                                          // success flag for callers that box int|false
    emitter.instruction("b __rt_uusf_ret");                                     // return it

    // -- permission predicate: mode, uid and gid from the same stat array --
    emitter.label("__rt_uusf_access");
    abi::emit_symbol_address(emitter, "x1", "_stat_key_mode");
    emitter.instruction("mov x2, #4");                                          // strlen("mode")
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed → reader receiver
    emitter.instruction("bl __rt_uusf_read");                                   // x0 = mode, x1 = whether it was present
    emitter.instruction("str x0, [sp, #32]");                                   // stash the mode
    emitter.instruction("str x1, [sp, #40]");                                   // stash whether the mode was readable at all
    abi::emit_symbol_address(emitter, "x1", "_stat_key_uid");
    emitter.instruction("mov x2, #3");                                          // strlen("uid")
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed → reader receiver
    emitter.instruction("bl __rt_uusf_read");                                   // x0 = uid (0 when the wrapper omitted it, as PHP zero-fills)
    emitter.instruction("str x0, [sp, #48]");                                   // stash the reported owner uid
    abi::emit_symbol_address(emitter, "x1", "_stat_key_gid");
    emitter.instruction("mov x2, #3");                                          // strlen("gid")
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed → reader receiver
    emitter.instruction("bl __rt_uusf_read");                                   // x0 = gid (0 when the wrapper omitted it)
    emitter.instruction("str x0, [sp, #56]");                                   // stash the reported owning gid
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed stat array
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload whether the mode was present
    emitter.instruction("cbz x9, __rt_uusf_fail");                              // no integer 'mode' → the predicate is false
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the selector to pick the permission bit
    emitter.instruction("mov x10, #4");                                         // selector 3 (is_readable) wants the read bit
    emitter.instruction("mov x11, #2");                                         // selector 4 (is_writable) wants the write bit
    emitter.instruction("cmp x9, #4");
    emitter.instruction("csel x10, x11, x10, eq");
    emitter.instruction("mov x11, #1");                                         // selector 5 (is_executable) wants the execute bit
    emitter.instruction("cmp x9, #5");
    emitter.instruction("csel x10, x11, x10, eq");
    emitter.instruction("ldr x0, [sp, #32]");                                   // mode
    emitter.instruction("ldr x1, [sp, #48]");                                   // reported owner uid
    emitter.instruction("ldr x2, [sp, #56]");                                   // reported owning gid
    emitter.instruction("mov x3, x10");                                         // the permission bit this predicate asks about
    emitter.instruction("bl __rt_stat_mode_access");                            // apply PHP's triad-selection rule
    emitter.instruction("b __rt_uusf_ret");                                     // return the boolean

    emitter.label("__rt_uusf_fail_box");
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed-false stat result (x0)
    emitter.label("__rt_uusf_fail");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the selector to choose the sentinel
    emitter.instruction("mov x0, #-1");                                         // integer selectors report -1
    emitter.instruction("mov x10, #0");                                         // boolean selectors report false
    emitter.instruction("cmp x9, #3");
    emitter.instruction("csel x0, x10, x0, ge");                                // a -1 stored into a PHP bool would read as true
    emitter.instruction("mov x1, #0");                                          // failure flag: `filesize()` boxes PHP false rather than -1

    emitter.label("__rt_uusf_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the integer field, the boolean, or the sentinel

    // -- internal: read one integer field out of a borrowed stat array --
    // Inputs: x0 = stat-array Mixed (borrowed), x1/x2 = key pointer/length.
    // Outputs: x0 = the integer (0 when absent or not an int), x1 = 1 when it
    // was present AND an integer. Factored out because the permission selectors
    // read three keys from one array, and open-coding the read three times is
    // how the release of the value box drifts from the release of the array.
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_field (field reader) ---");
    emitter.label("__rt_uusf_read");
    emitter.instruction("sub sp, sp, #48");                                     // reader frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the reader frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the borrowed stat array across the key normalization
    emitter.instruction("bl __rt_hash_normalize_key");                          // normalize the string key → key_lo/key_hi in x1/x2
    emitter.instruction("ldr x0, [sp, #16]");                                   // stat-array Mixed → reader receiver
    emitter.instruction("mov x3, xzr");                                         // optional stat fields are probed without PHP undefined-key warnings
    emitter.instruction("bl __rt_mixed_array_get");                             // x0 = boxed Mixed value at the key (Mixed null on miss)
    emitter.instruction("mov x10, x0");                                         // keep the value box for release
    emitter.instruction("ldr x9, [x0]");                                        // value runtime tag
    emitter.instruction("ldr x11, [x0, #8]");                                   // integer payload (only meaningful for tag 0)
    emitter.instruction("str x9, [sp, #24]");                                   // stash the tag across the release
    emitter.instruction("str x11, [sp, #32]");                                  // stash the payload across the release
    emitter.instruction("mov x0, x10");                                         // value box
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed field value
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the tag
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the payload
    emitter.instruction("cmp x9, #0");                                          // was the field an integer?
    emitter.instruction("csel x0, x0, xzr, eq");                                // a non-integer field reads as 0
    emitter.instruction("cset x1, eq");                                         // and reports "absent" to the caller
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the reader frame
    emitter.instruction("ret");                                                 // return the field and its presence flag
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper url stat field.
fn emit_user_wrapper_url_stat_field_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_field ---");
    emitter.label_global("__rt_user_wrapper_url_stat_field");

    // Frame: [rbp-8] field_sel, [rbp-16] stat Mixed, [rbp-24] primary field,
    //   [rbp-32] found flag, [rbp-40] uid, [rbp-48] gid.
    // push rbp then sub rsp,64 keeps rsp 16-aligned for the helper calls.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // spill slots for the selector, the array and the read fields
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // save the field selector (see the table on the AArch64 emitter)
    emitter.instruction("mov rdx, rcx");                                        // url_stat flags chosen by the calling builtin, not a fixed 0
    emitter.instruction("call __rt_user_wrapper_url_stat");                     // rax = boxed Mixed stat array (sets _url_stat_matched)
    emitter.instruction("test rax, rax");                                       // scheme not matched / null?
    emitter.instruction("jz __rt_uusf_fail_x86");                               // → sentinel (caller ignores when unmatched)
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // boxed Mixed runtime tag
    emitter.instruction("cmp r9, 3");                                           // wrapper reported the path absent (boxed false)?
    emitter.instruction("je __rt_uusf_fail_box_x86");                           // → release the false box and return the sentinel
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the stat-array Mixed across the key lookups
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the field selector
    emitter.instruction("cmp r10, 3");                                          // selectors 3..5 are the permission predicates
    emitter.instruction("jge __rt_uusf_access_x86");                            // → read mode, uid and gid together

    // -- single integer field: select the stat-array key string --
    emitter.instruction("cmp r10, 1");                                          // selector 1 = 'mode'
    emitter.instruction("je __rt_uusf_mode_x86");
    emitter.instruction("cmp r10, 2");                                          // selector 2 = 'mtime'
    emitter.instruction("je __rt_uusf_mtime_x86");
    abi::emit_symbol_address(emitter, "rax", "_stat_key_size");                 // size key pointer (new_by_name-style rax/rdx string ABI)
    emitter.instruction("mov rdx, 4");                                          // strlen("size")
    emitter.instruction("jmp __rt_uusf_havekey_x86");                           // proceed with the size key
    emitter.label("__rt_uusf_mode_x86");
    abi::emit_symbol_address(emitter, "rax", "_stat_key_mode");                 // mode key pointer
    emitter.instruction("mov rdx, 4");                                          // strlen("mode")
    emitter.instruction("jmp __rt_uusf_havekey_x86");                           // proceed with the mode key
    emitter.label("__rt_uusf_mtime_x86");
    abi::emit_symbol_address(emitter, "rax", "_stat_key_mtime");                // mtime key pointer
    emitter.instruction("mov rdx, 5");                                          // strlen("mtime")
    emitter.label("__rt_uusf_havekey_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // stat-array Mixed → reader receiver
    emitter.instruction("call __rt_uusf_read_x86");                             // rax = integer field, rdx = 1 when present and integral
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // stash the field across the array release
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // stash whether it was readable at all
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // stat-array Mixed
    emitter.instruction("call __rt_decref_any");                                // release the boxed stat array
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the found flag
    emitter.instruction("test rdx, rdx");                                       // was the field present and an integer?
    emitter.instruction("jz __rt_uusf_fail_x86");                               // missing/non-int field → sentinel
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the integer result
    emitter.instruction("mov rdx, 1");                                          // success flag for callers that box int|false
    emitter.instruction("jmp __rt_uusf_ret_x86");                               // return it

    // -- permission predicate: mode, uid and gid from the same stat array --
    emitter.label("__rt_uusf_access_x86");
    abi::emit_symbol_address(emitter, "rax", "_stat_key_mode");                 // mode key pointer
    emitter.instruction("mov rdx, 4");                                          // strlen("mode")
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // stat-array Mixed → reader receiver
    emitter.instruction("call __rt_uusf_read_x86");                             // rax = mode, rdx = whether it was present
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // stash the mode
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // stash whether the mode was readable at all
    abi::emit_symbol_address(emitter, "rax", "_stat_key_uid");                  // uid key pointer
    emitter.instruction("mov rdx, 3");                                          // strlen("uid")
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // stat-array Mixed → reader receiver
    emitter.instruction("call __rt_uusf_read_x86");                             // rax = uid (0 when the wrapper omitted it, as PHP zero-fills)
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // stash the reported owner uid
    abi::emit_symbol_address(emitter, "rax", "_stat_key_gid");                  // gid key pointer
    emitter.instruction("mov rdx, 3");                                          // strlen("gid")
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // stat-array Mixed → reader receiver
    emitter.instruction("call __rt_uusf_read_x86");                             // rax = gid (0 when the wrapper omitted it)
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // stash the reported owning gid
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // stat-array Mixed
    emitter.instruction("call __rt_decref_any");                                // release the boxed stat array
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload whether the mode was present
    emitter.instruction("test r9, r9");                                         // no integer 'mode'?
    emitter.instruction("jz __rt_uusf_fail_x86");                               // → the predicate is false
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the selector to pick the permission bit
    emitter.instruction("mov ecx, 4");                                          // selector 3 (is_readable) wants the read bit
    emitter.instruction("mov r8d, 2");                                          // selector 4 (is_writable) wants the write bit
    emitter.instruction("cmp r10, 4");
    emitter.instruction("cmove ecx, r8d");
    emitter.instruction("mov r8d, 1");                                          // selector 5 (is_executable) wants the execute bit
    emitter.instruction("cmp r10, 5");
    emitter.instruction("cmove ecx, r8d");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // mode
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reported owner uid
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reported owning gid
    emitter.instruction("call __rt_stat_mode_access");                          // apply PHP's triad-selection rule
    emitter.instruction("jmp __rt_uusf_ret_x86");                               // return the boolean

    emitter.label("__rt_uusf_fail_box_x86");
    emitter.instruction("call __rt_decref_any");                                // release the boxed-false stat result (rax)
    emitter.label("__rt_uusf_fail_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the selector to choose the sentinel
    emitter.instruction("mov rax, -1");                                         // integer selectors report -1
    emitter.instruction("xor edx, edx");                                        // boolean selectors report false
    emitter.instruction("cmp r9, 3");
    emitter.instruction("cmovge rax, rdx");                                     // a -1 stored into a PHP bool would read as true
    emitter.instruction("mov rdx, 0");                                          // failure flag: `filesize()` boxes PHP false rather than -1

    emitter.label("__rt_uusf_ret_x86");
    emitter.instruction("add rsp, 64");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the integer field, the boolean, or the sentinel

    // -- internal: read one integer field out of a borrowed stat array --
    // Inputs: rdi = stat-array Mixed (borrowed), rax/rdx = key pointer/length.
    // Outputs: rax = the integer (0 when absent or not an int), rdx = 1 when it
    // was present AND an integer.
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_field (field reader) ---");
    emitter.label("__rt_uusf_read_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the reader frame pointer
    emitter.instruction("sub rsp, 48");                                         // spill slots for the array, the tag and the payload
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the borrowed stat array across the key normalization
    emitter.instruction("call __rt_hash_normalize_key");                        // normalize the string key → key_lo in rax, key_hi in rdx
    emitter.instruction("mov rsi, rax");                                        // key_lo → SysV second arg for the reader
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // stat-array Mixed → reader receiver
    emitter.instruction("xor ecx, ecx");                                        // optional stat fields are probed without PHP undefined-key warnings
    emitter.instruction("call __rt_mixed_array_get");                           // rax = boxed Mixed value at the key (Mixed null on miss)
    emitter.instruction("mov r10, rax");                                        // keep the value box for release
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // value runtime tag
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // integer payload (only meaningful for tag 0)
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // stash the tag across the release
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // stash the payload across the release
    emitter.instruction("mov rax, r10");                                        // value box
    emitter.instruction("call __rt_decref_any");                                // release the boxed field value
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the tag
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the payload
    emitter.instruction("xor edx, edx");                                        // zero source for the non-integer case
    emitter.instruction("test r9, r9");                                         // was the field an integer (tag 0)?
    emitter.instruction("cmovne rax, rdx");                                     // a non-integer field reads as 0
    emitter.instruction("sete dl");                                             // and reports "absent" to the caller
    emitter.instruction("movzx edx, dl");                                       // widen the presence flag
    emitter.instruction("add rsp, 48");                                         // release the reader frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the field and its presence flag
}
