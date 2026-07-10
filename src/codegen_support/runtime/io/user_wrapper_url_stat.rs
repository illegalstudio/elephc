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

    // -- scan the path for the "://" scheme separator (x0=ptr, x1=len) --
    emitter.instruction("mov x9, #0");                                          // scheme scan index
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
    emitter.instruction("ldr x1, [x12, #16]");                                  // wrapper class name pointer from the registry slot
    emitter.instruction("ldr x2, [x12, #24]");                                  // wrapper class name length from the registry slot
    emitter.instruction("bl __rt_new_by_name");                                 // instantiate the wrapper class → x0 = obj, or 0 when unknown
    emitter.instruction("cbz x0, __rt_uus_false");                              // unknown class → boxed false
    emitter.instruction("str x0, [sp, #48]");                                   // save the throwaway wrapper instance

    // -- look up url_stat in the per-class user-wrapper vtable (slot 9) --
    emitter.instruction("ldr x9, [x0]");                                        // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("ldr x11, [x10, #{}]", VTABLE_URL_STAT_OFFSET)); // load the url_stat method pointer (slot 9)
    emitter.instruction("cbz x11, __rt_uus_false_obj");                         // class did not implement url_stat → boxed false

    // -- call url_stat($this, $path, $flags) → x0 = raw return --
    emitter.instruction("ldr x0, [sp, #48]");                                   // $this = wrapper object
    emitter.instruction("ldr x1, [sp, #16]");                                   // path ptr → string-arg pair
    emitter.instruction("ldr x2, [sp, #24]");                                   // path len → string-arg pair
    emitter.instruction("ldr x3, [sp, #32]");                                   // url_stat flags
    emitter.instruction("blr x11");                                             // invoke url_stat on the throwaway wrapper object
    emitter.instruction("bl __rt_box_wrapper_stat_result");                     // normalize the type-erased return into a boxed Mixed
    emitter.instruction("str x0, [sp, #56]");                                   // save the boxed result across the wrapper-instance release
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the throwaway wrapper object
    emitter.instruction("bl __rt_decref_any");                                  // free the throwaway wrapper instance
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the boxed result for return
    emitter.instruction("b __rt_uus_ret");                                      // share the common return path

    emitter.label("__rt_uus_false_obj");
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the throwaway wrapper object
    emitter.instruction("bl __rt_decref_any");                                  // free it before falling through to boxed false
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
    emitter.instruction("mov rax, rdi");                                        // path pointer → scan base register
    emitter.instruction("mov rdx, rsi");                                        // path length → scan bound register

    // -- scan the path for the "://" scheme separator (rax=ptr, rdx=len) --
    emitter.instruction("xor r9, r9");                                          // scheme scan index
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
    emitter.instruction("mov rax, QWORD PTR [r12 + 16]");                       // wrapper class name pointer from the registry slot
    emitter.instruction("mov rdx, QWORD PTR [r12 + 24]");                       // wrapper class name length (new_by_name reads rax/rdx)
    emitter.instruction("call __rt_new_by_name");                               // instantiate the wrapper class → rax = obj, or 0 when unknown
    emitter.instruction("test rax, rax");                                       // unknown class?
    emitter.instruction("jz __rt_uus_false_x86");                               // unknown class → boxed false
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the throwaway wrapper instance

    // -- look up url_stat in the per-class user-wrapper vtable (slot 9) --
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_vtable_ptrs");      // base of the per-class user-wrapper vtable pointer table
    emitter.instruction("mov r10, QWORD PTR [r10 + r9 * 8]");                   // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {}]", VTABLE_URL_STAT_OFFSET)); // load the url_stat method pointer (slot 9)
    emitter.instruction("test r11, r11");                                       // class did not implement url_stat?
    emitter.instruction("jz __rt_uus_false_obj_x86");                           // no url_stat → boxed false

    // -- call url_stat($this, $path, $flags) → rax = raw return --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // $this = wrapper object
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // path ptr → string-arg pair
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // path len → string-arg pair
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // url_stat flags
    emitter.instruction("call r11");                                            // invoke url_stat on the throwaway wrapper object
    emitter.instruction("call __rt_box_wrapper_stat_result");                   // normalize the type-erased return into a boxed Mixed
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the boxed result across the wrapper-instance release
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the throwaway wrapper object
    emitter.instruction("call __rt_decref_any");                                // free the throwaway wrapper instance
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed result for return
    emitter.instruction("jmp __rt_uus_ret_x86");                                // share the common return path

    emitter.label("__rt_uus_false_obj_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the throwaway wrapper object
    emitter.instruction("call __rt_decref_any");                                // free it before falling through to boxed false
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

/// Emits `__rt_user_wrapper_url_stat_field(path_ptr, path_len, field_sel)`.
///
/// Calls `__rt_user_wrapper_url_stat` (which sets `_url_stat_matched`) and, on a
/// stat-array result, extracts an integer field by its PHP string key:
/// `field_sel` 0 → `'size'`, 1 → `'mode'`. Returns the field as an int, or `-1`
/// when the scheme did not match, the wrapper reported the path absent, or the
/// field is missing/non-integer. Backs `filesize()`/`is_file()` on `scheme://`
/// URLs; the caller reads `_url_stat_matched` to choose between this result and
/// the real-filesystem fallback. Reuses the boxed-Mixed reader
/// (`__rt_mixed_array_get`) with a `__rt_hash_normalize_key`-normalized string
/// key, then releases both the field box and the stat-array box.
pub fn emit_user_wrapper_url_stat_field(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_url_stat_field_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_field ---");
    emitter.label_global("__rt_user_wrapper_url_stat_field");

    // Frame: 48 bytes. [sp,#0..16] x29/x30, [sp,#16] field_sel, [sp,#24] stat
    //   Mixed, [sp,#32] extracted int result.
    emitter.instruction("sub sp, sp, #48");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the field selector (0=size, 1=mode)
    emitter.instruction("mov x2, #0");                                          // url_stat flags = 0
    emitter.instruction("bl __rt_user_wrapper_url_stat");                       // x0 = boxed Mixed stat array (sets _url_stat_matched)
    emitter.instruction("cbz x0, __rt_uusf_fail");                              // scheme not matched / null → -1 (caller ignores when unmatched)
    emitter.instruction("ldr x9, [x0]");                                        // boxed Mixed runtime tag
    emitter.instruction("cmp x9, #3");                                          // wrapper reported the path absent (boxed false)?
    emitter.instruction("b.eq __rt_uusf_fail_box");                             // → release the false box and return -1
    emitter.instruction("str x0, [sp, #24]");                                   // save the stat-array Mixed across the key lookup

    // -- select the stat-array key string by field selector --
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the field selector
    emitter.instruction("cbnz x10, __rt_uusf_mode");                            // non-zero selector → 'mode'
    abi::emit_symbol_address(emitter, "x1", "_stat_key_size");
    emitter.instruction("mov x2, #4");                                          // strlen("size")
    emitter.instruction("b __rt_uusf_havekey");                                 // proceed with the size key
    emitter.label("__rt_uusf_mode");
    abi::emit_symbol_address(emitter, "x1", "_stat_key_mode");
    emitter.instruction("mov x2, #4");                                          // strlen("mode")
    emitter.label("__rt_uusf_havekey");
    emitter.instruction("bl __rt_hash_normalize_key");                          // normalize the string key → key_lo/key_hi in x1/x2
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed → reader receiver
    emitter.instruction("bl __rt_mixed_array_get");                             // x0 = boxed Mixed value at the key (Mixed null on miss)
    emitter.instruction("mov x10, x0");                                         // keep the value box for release
    emitter.instruction("ldr x9, [x0]");                                        // value runtime tag
    emitter.instruction("cmp x9, #0");                                          // is the field an integer?
    emitter.instruction("b.ne __rt_uusf_valfail");                              // missing/non-int field → -1
    emitter.instruction("ldr x11, [x0, #8]");                                   // load the integer field payload
    emitter.instruction("str x11, [sp, #32]");                                  // stash the result across the releases
    emitter.instruction("mov x0, x10");                                         // value box
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed field value
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed stat array
    emitter.instruction("ldr x0, [sp, #32]");                                   // load the integer result
    emitter.instruction("b __rt_uusf_ret");                                     // return it

    emitter.label("__rt_uusf_valfail");
    emitter.instruction("mov x0, x10");                                         // value box
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed field value
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed stat array
    emitter.instruction("b __rt_uusf_fail");                                    // fall through to the -1 result

    emitter.label("__rt_uusf_fail_box");
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed-false stat result (x0)
    emitter.label("__rt_uusf_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 sentinel (caller ignores when _url_stat_matched is 0)

    emitter.label("__rt_uusf_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the integer field (or -1)
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper url stat field.
fn emit_user_wrapper_url_stat_field_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_field ---");
    emitter.label_global("__rt_user_wrapper_url_stat_field");

    // Frame: [rbp-8] field_sel, [rbp-16] stat Mixed, [rbp-24] int result.
    // push rbp then sub rsp,32 keeps rsp 16-aligned for the helper calls.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // spill slots for field_sel/stat-mixed/result
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // save the field selector (0=size, 1=mode)
    emitter.instruction("xor edx, edx");                                        // url_stat flags = 0
    emitter.instruction("call __rt_user_wrapper_url_stat");                     // rax = boxed Mixed stat array (sets _url_stat_matched)
    emitter.instruction("test rax, rax");                                       // scheme not matched / null?
    emitter.instruction("jz __rt_uusf_fail_x86");                               // → -1 (caller ignores when unmatched)
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // boxed Mixed runtime tag
    emitter.instruction("cmp r9, 3");                                           // wrapper reported the path absent (boxed false)?
    emitter.instruction("je __rt_uusf_fail_box_x86");                           // → release the false box and return -1
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the stat-array Mixed across the key lookup

    // -- select the stat-array key string by field selector --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the field selector
    emitter.instruction("test r10, r10");                                       // size (0) or mode (non-zero)?
    emitter.instruction("jnz __rt_uusf_mode_x86");                              // non-zero selector → 'mode'
    abi::emit_symbol_address(emitter, "rax", "_stat_key_size");                 // size key pointer (new_by_name-style rax/rdx string ABI)
    emitter.instruction("mov rdx, 4");                                          // strlen("size")
    emitter.instruction("jmp __rt_uusf_havekey_x86");                           // proceed with the size key
    emitter.label("__rt_uusf_mode_x86");
    abi::emit_symbol_address(emitter, "rax", "_stat_key_mode");                 // mode key pointer
    emitter.instruction("mov rdx, 4");                                          // strlen("mode")
    emitter.label("__rt_uusf_havekey_x86");
    emitter.instruction("call __rt_hash_normalize_key");                        // normalize the string key → key_lo in rax, key_hi in rdx
    emitter.instruction("mov rsi, rax");                                        // key_lo → SysV second arg for the reader
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // stat-array Mixed → reader receiver
    emitter.instruction("call __rt_mixed_array_get");                           // rax = boxed Mixed value at the key (Mixed null on miss)
    emitter.instruction("mov r10, rax");                                        // keep the value box for release
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // value runtime tag
    emitter.instruction("test r9, r9");                                         // is the field an integer (tag 0)?
    emitter.instruction("jnz __rt_uusf_valfail_x86");                           // missing/non-int field → -1
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // load the integer field payload
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // stash the result across the releases
    emitter.instruction("mov rax, r10");                                        // value box
    emitter.instruction("call __rt_decref_any");                                // release the boxed field value
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // stat-array Mixed
    emitter.instruction("call __rt_decref_any");                                // release the boxed stat array
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the integer result
    emitter.instruction("jmp __rt_uusf_ret_x86");                               // return it

    emitter.label("__rt_uusf_valfail_x86");
    emitter.instruction("mov rax, r10");                                        // value box
    emitter.instruction("call __rt_decref_any");                                // release the boxed field value
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // stat-array Mixed
    emitter.instruction("call __rt_decref_any");                                // release the boxed stat array
    emitter.instruction("jmp __rt_uusf_fail_x86");                              // fall through to the -1 result

    emitter.label("__rt_uusf_fail_box_x86");
    emitter.instruction("call __rt_decref_any");                                // release the boxed-false stat result (rax)
    emitter.label("__rt_uusf_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 sentinel (caller ignores when _url_stat_matched is 0)

    emitter.label("__rt_uusf_ret_x86");
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the integer field (or -1)
}
