//! Purpose:
//! Emits the path-based mutation dispatchers for userspace stream wrappers:
//! `__rt_user_wrapper_path_op` (a generic single-path helper backing
//! `unlink()`/`rmdir()`/`mkdir()`) and `__rt_user_wrapper_rename` (the two-path
//! helper backing `rename()`). Each scans the registered-wrapper table for the
//! path's `scheme://` prefix, instantiates the matching class, calls the
//! requested `StreamWrapper` method through the regular elephc method ABI, and
//! returns the method's bool result.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The `unlink`/`rmdir`/`mkdir`/`rename` builtin emitters, after a
//!   `__rt_path_is_wrapper` probe selects the wrapper branch (mirroring the
//!   `readfile()` builtin's wrapper/filesystem split).
//!
//! Key details:
//! - The scheme scan / slot match mirrors `__rt_path_is_wrapper` and
//!   `__rt_user_wrapper_url_stat`. The throwaway wrapper instance is freed with
//!   `__rt_decref_any` after the method returns.
//! - `__rt_new_by_name` takes the class name in x1/x2 (AArch64) or rax/rdx
//!   (x86_64), NOT the SysV argument registers.
//! - A missing class or missing method returns 0 (`false`), matching PHP's
//!   result when a wrapper does not implement the operation.
//! - The wrapper method receives the FULL `scheme://...` path (PHP passes the
//!   whole URL to `StreamWrapper` path methods), plus the caller's extra int
//!   arguments (`$mode`/`$options` for `mkdir`, `$options` for `rmdir`).

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_user_wrapper_path_op(path_ptr, path_len, slot, a3, a4) -> 1/0`.
///
/// Inputs (AArch64): x0 = path ptr, x1 = path len, x2 = vtable slot index,
/// x3/x4 = the wrapper method's extra int arguments. (x86_64): rdi/rsi = path
/// ptr/len, rdx = slot, rcx/r8 = extra args. Output: x0 / rax = the wrapper
/// method's bool result, or 0 when the scheme/class/method is absent.
///
/// The selected method is invoked as `method($this, path_ptr, path_len, a3, a4)`
/// through the regular elephc method ABI; wrappers that declare fewer parameters
/// simply ignore the trailing arguments.
pub fn emit_user_wrapper_path_op(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_path_op_linux_x86_64(emitter);
        return;
    }
    emit_user_wrapper_path_op_aarch64(emitter);
}

/// AArch64 implementation of `__rt_user_wrapper_path_op`.
fn emit_user_wrapper_path_op_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_path_op ---");
    emitter.label_global("__rt_user_wrapper_path_op");

    // Frame: 64 bytes. [sp,#0..16] x29/x30, [sp,#16] path ptr, [sp,#24] path
    //   len, [sp,#32] slot, [sp,#40] a3, [sp,#48] a4, [sp,#56] obj.
    emitter.instruction("sub sp, sp, #64");                                     // helper frame for the path-op dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the path pointer across the helper calls
    emitter.instruction("str x1, [sp, #24]");                                   // save the path length across the helper calls
    emitter.instruction("str x2, [sp, #32]");                                   // save the vtable slot index
    emitter.instruction("str x3, [sp, #40]");                                   // save the wrapper method's extra arg 3
    emitter.instruction("str x4, [sp, #48]");                                   // save the wrapper method's extra arg 4

    // -- scan the path for the "://" scheme separator (x0=ptr, x1=len) --
    emitter.instruction("mov x9, #0");                                          // scheme scan index
    emitter.label("__rt_uwpo_scan");
    emitter.instruction("add x10, x9, #3");                                     // need three bytes for the "://" marker
    emitter.instruction("cmp x10, x1");                                         // do enough bytes remain in the path?
    emitter.instruction("b.gt __rt_uwpo_false");                                // no scheme separator → not a wrapper URL → false
    emitter.instruction("ldrb w11, [x0, x9]");                                  // load the candidate ':' byte
    emitter.instruction("cmp w11, #58");                                        // is it ':'?
    emitter.instruction("b.ne __rt_uwpo_scan_next");                            // not the scheme marker
    emitter.instruction("add x12, x9, #1");                                     // index of the first '/'
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the candidate first '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_uwpo_scan_next");                            // not the scheme marker
    emitter.instruction("add x12, x9, #2");                                     // index of the second '/'
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the candidate second '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_uwpo_scan_next");                            // not the scheme marker
    emitter.instruction("b __rt_uwpo_check");                                   // "://" found at index x9 — x9 is the scheme length
    emitter.label("__rt_uwpo_scan_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_uwpo_scan");                                    // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (x9=scheme len) --
    emitter.label("__rt_uwpo_check");
    abi::emit_symbol_address(emitter, "x10", "_user_wrappers");
    emitter.instruction("mov x11, #0");                                         // wrapper slot index
    emitter.label("__rt_uwpo_slot");
    emitter.instruction("cmp x11, #64");                                        // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("b.ge __rt_uwpo_false");                                // no registered wrapper matched the scheme → false
    emitter.instruction("add x12, x10, x11, lsl #5");                           // slot base = table + index * 32
    emitter.instruction("ldr x13, [x12]");                                      // stored protocol pointer
    emitter.instruction("cbz x13, __rt_uwpo_slot_next");                        // empty slot — skip it
    emitter.instruction("ldr x14, [x12, #8]");                                  // stored protocol length
    emitter.instruction("cmp x14, x9");                                         // does the stored length match the scheme length?
    emitter.instruction("b.ne __rt_uwpo_slot_next");                            // length mismatch — try the next slot
    emitter.instruction("mov x15, #0");                                         // byte compare index
    emitter.label("__rt_uwpo_bytes");
    emitter.instruction("cmp x15, x9");                                         // compared every protocol byte?
    emitter.instruction("b.ge __rt_uwpo_match");                                // full match — dispatch into the wrapper class
    emitter.instruction("ldrb w16, [x13, x15]");                                // stored protocol byte
    emitter.instruction("ldrb w17, [x0, x15]");                                 // path scheme byte
    emitter.instruction("cmp w16, w17");                                        // do the bytes match?
    emitter.instruction("b.ne __rt_uwpo_slot_next");                            // protocol byte differs — try the next slot
    emitter.instruction("add x15, x15, #1");                                    // advance the compare index
    emitter.instruction("b __rt_uwpo_bytes");                                   // continue comparing bytes
    emitter.label("__rt_uwpo_slot_next");
    emitter.instruction("add x11, x11, #1");                                    // advance the slot index
    emitter.instruction("b __rt_uwpo_slot");                                    // continue scanning slots

    // -- matched scheme: x12 = registry slot base --
    emitter.label("__rt_uwpo_match");
    emitter.instruction("ldr x1, [x12, #16]");                                  // wrapper class name pointer from the registry slot
    emitter.instruction("ldr x2, [x12, #24]");                                  // wrapper class name length from the registry slot
    emitter.instruction("bl __rt_new_by_name");                                 // instantiate the wrapper class → x0 = obj, or 0 when unknown
    emitter.instruction("cbz x0, __rt_uwpo_false");                             // unknown class → false
    emitter.instruction("str x0, [sp, #56]");                                   // save the throwaway wrapper instance

    // -- look up the method in the per-class vtable at the requested slot --
    emitter.instruction("ldr x9, [x0]");                                        // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // per-class user-wrapper vtable for the resolved class
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the requested vtable slot index
    emitter.instruction("ldr x11, [x10, x9, lsl #3]");                          // load the requested method pointer from the vtable
    emitter.instruction("cbz x11, __rt_uwpo_false_obj");                        // class did not implement the method → false

    // -- call method($this, path_ptr, path_len, a3, a4) → x0 = bool --
    emitter.instruction("ldr x0, [sp, #56]");                                   // $this = wrapper object
    emitter.instruction("ldr x1, [sp, #16]");                                   // path ptr → string-arg pair
    emitter.instruction("ldr x2, [sp, #24]");                                   // path len → string-arg pair
    emitter.instruction("ldr x3, [sp, #40]");                                   // extra arg 3 (mode/options)
    emitter.instruction("ldr x4, [sp, #48]");                                   // extra arg 4 (options)
    emitter.instruction("blr x11");                                             // invoke the wrapper path method
    emitter.instruction("str x0, [sp, #40]");                                   // stash the bool result across the instance release
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the throwaway wrapper object
    emitter.instruction("bl __rt_decref_any");                                  // free the throwaway wrapper instance
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the wrapper method's bool result
    emitter.instruction("b __rt_uwpo_ret");                                     // share the common return path

    emitter.label("__rt_uwpo_false_obj");
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the throwaway wrapper object
    emitter.instruction("bl __rt_decref_any");                                  // free it before returning false
    emitter.label("__rt_uwpo_false");
    emitter.instruction("mov x0, #0");                                          // false: no scheme / unknown class / missing method

    emitter.label("__rt_uwpo_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the bool result
}

/// x86_64 implementation of `__rt_user_wrapper_path_op`.
fn emit_user_wrapper_path_op_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_path_op ---");
    emitter.label_global("__rt_user_wrapper_path_op");

    // Frame: [rbp-8] path ptr, [rbp-16] path len, [rbp-24] slot, [rbp-32] a3,
    //   [rbp-40] a4, [rbp-48] obj. push rbp then sub rsp,48 keeps rsp aligned.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // spill slots for path/slot/args/obj
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the path pointer across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the path length across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the vtable slot index
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the wrapper method's extra arg 3
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the wrapper method's extra arg 4
    emitter.instruction("mov rax, rdi");                                        // path pointer → scan base register
    emitter.instruction("mov rdx, rsi");                                        // path length → scan bound register

    // -- scan the path for the "://" scheme separator (rax=ptr, rdx=len) --
    emitter.instruction("xor r9, r9");                                          // scheme scan index
    emitter.label("__rt_uwpo_scan_x86");
    emitter.instruction("lea r10, [r9 + 3]");                                   // need three bytes for the "://" marker
    emitter.instruction("cmp r10, rdx");                                        // do enough bytes remain in the path?
    emitter.instruction("jg __rt_uwpo_false_x86");                              // no scheme separator → false
    emitter.instruction("movzx r11d, BYTE PTR [rax + r9]");                     // load the candidate ':' byte
    emitter.instruction("cmp r11b, 58");                                        // is it ':'?
    emitter.instruction("jne __rt_uwpo_next_x86");                              // not the scheme marker
    emitter.instruction("lea r12, [r9 + 1]");                                   // index of the first '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate first '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_uwpo_next_x86");                              // not the scheme marker
    emitter.instruction("lea r12, [r9 + 2]");                                   // index of the second '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate second '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_uwpo_next_x86");                              // not the scheme marker
    emitter.instruction("jmp __rt_uwpo_check_x86");                             // "://" found at r9 — r9 is the scheme length
    emitter.label("__rt_uwpo_next_x86");
    emitter.instruction("inc r9");                                              // advance the scan index
    emitter.instruction("jmp __rt_uwpo_scan_x86");                              // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (r9=scheme len) --
    emitter.label("__rt_uwpo_check_x86");
    abi::emit_symbol_address(emitter, "r10", "_user_wrappers");                 // wrapper table base
    emitter.instruction("xor r11, r11");                                        // wrapper slot index
    emitter.label("__rt_uwpo_slot_x86");
    emitter.instruction("cmp r11, 64");                                         // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("jge __rt_uwpo_false_x86");                             // no registered wrapper matched the scheme → false
    emitter.instruction("mov r12, r11");                                        // copy the slot index for scaling
    emitter.instruction("shl r12, 5");                                          // slot offset = index * 32
    emitter.instruction("add r12, r10");                                        // slot base = table + offset
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // stored protocol pointer
    emitter.instruction("test r13, r13");                                       // is this slot empty?
    emitter.instruction("jz __rt_uwpo_slot_next_x86");                          // skip empty slots
    emitter.instruction("mov r14, QWORD PTR [r12 + 8]");                        // stored protocol length
    emitter.instruction("cmp r14, r9");                                         // does the stored length match the scheme length?
    emitter.instruction("jne __rt_uwpo_slot_next_x86");                         // length mismatch — try the next slot
    emitter.instruction("xor r15, r15");                                        // byte compare index
    emitter.label("__rt_uwpo_bytes_x86");
    emitter.instruction("cmp r15, r9");                                         // compared every protocol byte?
    emitter.instruction("jge __rt_uwpo_match_x86");                             // full match — dispatch into the wrapper class
    emitter.instruction("movzx ecx, BYTE PTR [r13 + r15]");                     // stored protocol byte
    emitter.instruction("movzx r8d, BYTE PTR [rax + r15]");                     // path scheme byte
    emitter.instruction("cmp cl, r8b");                                         // do the bytes match?
    emitter.instruction("jne __rt_uwpo_slot_next_x86");                         // protocol byte differs — try the next slot
    emitter.instruction("inc r15");                                             // advance the compare index
    emitter.instruction("jmp __rt_uwpo_bytes_x86");                             // continue comparing bytes
    emitter.label("__rt_uwpo_slot_next_x86");
    emitter.instruction("inc r11");                                             // advance the slot index
    emitter.instruction("jmp __rt_uwpo_slot_x86");                              // continue scanning slots

    // -- matched scheme: r12 = registry slot base --
    emitter.label("__rt_uwpo_match_x86");
    emitter.instruction("mov rax, QWORD PTR [r12 + 16]");                       // wrapper class name pointer from the registry slot
    emitter.instruction("mov rdx, QWORD PTR [r12 + 24]");                       // wrapper class name length (new_by_name reads rax/rdx)
    emitter.instruction("call __rt_new_by_name");                               // instantiate the wrapper class → rax = obj, or 0 when unknown
    emitter.instruction("test rax, rax");                                       // unknown class?
    emitter.instruction("jz __rt_uwpo_false_x86");                              // unknown class → false
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the throwaway wrapper instance

    // -- look up the method in the per-class vtable at the requested slot --
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_vtable_ptrs");      // base of the per-class user-wrapper vtable pointer table
    emitter.instruction("mov r10, QWORD PTR [r10 + r9 * 8]");                   // per-class user-wrapper vtable for the resolved class
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the requested vtable slot index
    emitter.instruction("mov r11, QWORD PTR [r10 + r9 * 8]");                   // load the requested method pointer from the vtable
    emitter.instruction("test r11, r11");                                       // class did not implement the method?
    emitter.instruction("jz __rt_uwpo_false_obj_x86");                          // missing method → false

    // -- call method($this, path_ptr, path_len, a3, a4) → rax = bool --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // $this = wrapper object
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // path ptr → string-arg pair
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // path len → string-arg pair
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // extra arg 3 (mode/options)
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // extra arg 4 (options)
    emitter.instruction("call r11");                                            // invoke the wrapper path method
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // stash the bool result across the instance release
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the throwaway wrapper object
    emitter.instruction("call __rt_decref_any");                                // free the throwaway wrapper instance
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the wrapper method's bool result
    emitter.instruction("jmp __rt_uwpo_ret_x86");                               // share the common return path

    emitter.label("__rt_uwpo_false_obj_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the throwaway wrapper object
    emitter.instruction("call __rt_decref_any");                                // free it before returning false
    emitter.label("__rt_uwpo_false_x86");
    emitter.instruction("xor eax, eax");                                        // false: no scheme / unknown class / missing method

    emitter.label("__rt_uwpo_ret_x86");
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the bool result
}

/// Byte offset of the `rename` method pointer in the per-class user-wrapper
/// vtable (slot 16 of `USER_WRAPPER_VTABLE_SLOTS`, 8 bytes per slot).
const VTABLE_RENAME_OFFSET: usize = 16 * 8;

/// Emits `__rt_user_wrapper_rename(from_ptr, from_len, to_ptr, to_len) -> 1/0`.
///
/// Inputs (AArch64): x0/x1 = from ptr/len, x2/x3 = to ptr/len. (x86_64):
/// rdi/rsi = from ptr/len, rdx/rcx = to ptr/len. Output: x0 / rax = the
/// wrapper's `rename()` bool result, or 0 when the `from` scheme does not match
/// a registered wrapper / the class or method is absent.
///
/// The `from` path's scheme selects the wrapper; the method is invoked as
/// `rename($this, from_ptr, from_len, to_ptr, to_len)` through the regular
/// elephc method ABI. PHP passes both full `scheme://...` URLs to the wrapper's
/// `rename()`.
pub fn emit_user_wrapper_rename(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_rename_linux_x86_64(emitter);
        return;
    }
    emit_user_wrapper_rename_aarch64(emitter);
}

/// AArch64 implementation of `__rt_user_wrapper_rename`.
fn emit_user_wrapper_rename_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_rename ---");
    emitter.label_global("__rt_user_wrapper_rename");

    // Frame: 64 bytes. [sp,#0..16] x29/x30, [sp,#16] from ptr, [sp,#24] from
    //   len, [sp,#32] to ptr, [sp,#40] to len, [sp,#48] obj.
    emitter.instruction("sub sp, sp, #64");                                     // helper frame for the rename dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the from-path pointer across the helper calls
    emitter.instruction("str x1, [sp, #24]");                                   // save the from-path length across the helper calls
    emitter.instruction("str x2, [sp, #32]");                                   // save the to-path pointer across the helper calls
    emitter.instruction("str x3, [sp, #40]");                                   // save the to-path length across the helper calls

    // -- scan the from-path for the "://" scheme separator (x0=ptr, x1=len) --
    emitter.instruction("mov x9, #0");                                          // scheme scan index
    emitter.label("__rt_uwrn_scan");
    emitter.instruction("add x10, x9, #3");                                     // need three bytes for the "://" marker
    emitter.instruction("cmp x10, x1");                                         // do enough bytes remain in the from-path?
    emitter.instruction("b.gt __rt_uwrn_false");                                // no scheme separator → not a wrapper URL → false
    emitter.instruction("ldrb w11, [x0, x9]");                                  // load the candidate ':' byte
    emitter.instruction("cmp w11, #58");                                        // is it ':'?
    emitter.instruction("b.ne __rt_uwrn_scan_next");                            // not the scheme marker
    emitter.instruction("add x12, x9, #1");                                     // index of the first '/'
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the candidate first '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_uwrn_scan_next");                            // not the scheme marker
    emitter.instruction("add x12, x9, #2");                                     // index of the second '/'
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the candidate second '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_uwrn_scan_next");                            // not the scheme marker
    emitter.instruction("b __rt_uwrn_check");                                   // "://" found at index x9 — x9 is the scheme length
    emitter.label("__rt_uwrn_scan_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_uwrn_scan");                                    // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (x9=scheme len) --
    emitter.label("__rt_uwrn_check");
    abi::emit_symbol_address(emitter, "x10", "_user_wrappers");
    emitter.instruction("mov x11, #0");                                         // wrapper slot index
    emitter.label("__rt_uwrn_slot");
    emitter.instruction("cmp x11, #64");                                        // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("b.ge __rt_uwrn_false");                                // no registered wrapper matched the scheme → false
    emitter.instruction("add x12, x10, x11, lsl #5");                           // slot base = table + index * 32
    emitter.instruction("ldr x13, [x12]");                                      // stored protocol pointer
    emitter.instruction("cbz x13, __rt_uwrn_slot_next");                        // empty slot — skip it
    emitter.instruction("ldr x14, [x12, #8]");                                  // stored protocol length
    emitter.instruction("cmp x14, x9");                                         // does the stored length match the scheme length?
    emitter.instruction("b.ne __rt_uwrn_slot_next");                            // length mismatch — try the next slot
    emitter.instruction("mov x15, #0");                                         // byte compare index
    emitter.label("__rt_uwrn_bytes");
    emitter.instruction("cmp x15, x9");                                         // compared every protocol byte?
    emitter.instruction("b.ge __rt_uwrn_match");                                // full match — dispatch into the wrapper class
    emitter.instruction("ldrb w16, [x13, x15]");                                // stored protocol byte
    emitter.instruction("ldrb w17, [x0, x15]");                                 // from-path scheme byte
    emitter.instruction("cmp w16, w17");                                        // do the bytes match?
    emitter.instruction("b.ne __rt_uwrn_slot_next");                            // protocol byte differs — try the next slot
    emitter.instruction("add x15, x15, #1");                                    // advance the compare index
    emitter.instruction("b __rt_uwrn_bytes");                                   // continue comparing bytes
    emitter.label("__rt_uwrn_slot_next");
    emitter.instruction("add x11, x11, #1");                                    // advance the slot index
    emitter.instruction("b __rt_uwrn_slot");                                    // continue scanning slots

    // -- matched scheme: x12 = registry slot base --
    emitter.label("__rt_uwrn_match");
    emitter.instruction("ldr x1, [x12, #16]");                                  // wrapper class name pointer from the registry slot
    emitter.instruction("ldr x2, [x12, #24]");                                  // wrapper class name length from the registry slot
    emitter.instruction("bl __rt_new_by_name");                                 // instantiate the wrapper class → x0 = obj, or 0 when unknown
    emitter.instruction("cbz x0, __rt_uwrn_false");                             // unknown class → false
    emitter.instruction("str x0, [sp, #48]");                                   // save the throwaway wrapper instance

    // -- look up rename in the per-class user-wrapper vtable (slot 16) --
    emitter.instruction("ldr x9, [x0]");                                        // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("ldr x11, [x10, #{}]", VTABLE_RENAME_OFFSET)); // load the rename method pointer (slot 16)
    emitter.instruction("cbz x11, __rt_uwrn_false_obj");                        // class did not implement rename → false

    // -- call rename($this, from_ptr, from_len, to_ptr, to_len) → x0 = bool --
    emitter.instruction("ldr x0, [sp, #48]");                                   // $this = wrapper object
    emitter.instruction("ldr x1, [sp, #16]");                                   // from ptr → first string-arg pair
    emitter.instruction("ldr x2, [sp, #24]");                                   // from len → first string-arg pair
    emitter.instruction("ldr x3, [sp, #32]");                                   // to ptr → second string-arg pair
    emitter.instruction("ldr x4, [sp, #40]");                                   // to len → second string-arg pair
    emitter.instruction("blr x11");                                             // invoke the wrapper's rename method
    emitter.instruction("str x0, [sp, #16]");                                   // stash the bool result across the instance release
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the throwaway wrapper object
    emitter.instruction("bl __rt_decref_any");                                  // free the throwaway wrapper instance
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the wrapper's bool result
    emitter.instruction("b __rt_uwrn_ret");                                     // share the common return path

    emitter.label("__rt_uwrn_false_obj");
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the throwaway wrapper object
    emitter.instruction("bl __rt_decref_any");                                  // free it before returning false
    emitter.label("__rt_uwrn_false");
    emitter.instruction("mov x0, #0");                                          // false: no scheme / unknown class / missing method

    emitter.label("__rt_uwrn_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the bool result
}

/// x86_64 implementation of `__rt_user_wrapper_rename`.
fn emit_user_wrapper_rename_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_rename ---");
    emitter.label_global("__rt_user_wrapper_rename");

    // Frame: [rbp-8] from ptr, [rbp-16] from len, [rbp-24] to ptr, [rbp-32] to
    //   len, [rbp-40] obj. push rbp then sub rsp,48 keeps rsp aligned.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // spill slots for from/to paths and obj
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the from-path pointer across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the from-path length across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the to-path pointer across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the to-path length across the helper calls
    emitter.instruction("mov rax, rdi");                                        // from-path pointer → scan base register
    emitter.instruction("mov rdx, rsi");                                        // from-path length → scan bound register

    // -- scan the from-path for the "://" scheme separator (rax=ptr, rdx=len) --
    emitter.instruction("xor r9, r9");                                          // scheme scan index
    emitter.label("__rt_uwrn_scan_x86");
    emitter.instruction("lea r10, [r9 + 3]");                                   // need three bytes for the "://" marker
    emitter.instruction("cmp r10, rdx");                                        // do enough bytes remain in the from-path?
    emitter.instruction("jg __rt_uwrn_false_x86");                              // no scheme separator → false
    emitter.instruction("movzx r11d, BYTE PTR [rax + r9]");                     // load the candidate ':' byte
    emitter.instruction("cmp r11b, 58");                                        // is it ':'?
    emitter.instruction("jne __rt_uwrn_next_x86");                              // not the scheme marker
    emitter.instruction("lea r12, [r9 + 1]");                                   // index of the first '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate first '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_uwrn_next_x86");                              // not the scheme marker
    emitter.instruction("lea r12, [r9 + 2]");                                   // index of the second '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate second '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_uwrn_next_x86");                              // not the scheme marker
    emitter.instruction("jmp __rt_uwrn_check_x86");                             // "://" found at r9 — r9 is the scheme length
    emitter.label("__rt_uwrn_next_x86");
    emitter.instruction("inc r9");                                              // advance the scan index
    emitter.instruction("jmp __rt_uwrn_scan_x86");                              // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (r9=scheme len) --
    emitter.label("__rt_uwrn_check_x86");
    abi::emit_symbol_address(emitter, "r10", "_user_wrappers");                 // wrapper table base
    emitter.instruction("xor r11, r11");                                        // wrapper slot index
    emitter.label("__rt_uwrn_slot_x86");
    emitter.instruction("cmp r11, 64");                                         // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("jge __rt_uwrn_false_x86");                             // no registered wrapper matched the scheme → false
    emitter.instruction("mov r12, r11");                                        // copy the slot index for scaling
    emitter.instruction("shl r12, 5");                                          // slot offset = index * 32
    emitter.instruction("add r12, r10");                                        // slot base = table + offset
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // stored protocol pointer
    emitter.instruction("test r13, r13");                                       // is this slot empty?
    emitter.instruction("jz __rt_uwrn_slot_next_x86");                          // skip empty slots
    emitter.instruction("mov r14, QWORD PTR [r12 + 8]");                        // stored protocol length
    emitter.instruction("cmp r14, r9");                                         // does the stored length match the scheme length?
    emitter.instruction("jne __rt_uwrn_slot_next_x86");                         // length mismatch — try the next slot
    emitter.instruction("xor r15, r15");                                        // byte compare index
    emitter.label("__rt_uwrn_bytes_x86");
    emitter.instruction("cmp r15, r9");                                         // compared every protocol byte?
    emitter.instruction("jge __rt_uwrn_match_x86");                             // full match — dispatch into the wrapper class
    emitter.instruction("movzx ecx, BYTE PTR [r13 + r15]");                     // stored protocol byte
    emitter.instruction("movzx r8d, BYTE PTR [rax + r15]");                     // from-path scheme byte
    emitter.instruction("cmp cl, r8b");                                         // do the bytes match?
    emitter.instruction("jne __rt_uwrn_slot_next_x86");                         // protocol byte differs — try the next slot
    emitter.instruction("inc r15");                                             // advance the compare index
    emitter.instruction("jmp __rt_uwrn_bytes_x86");                             // continue comparing bytes
    emitter.label("__rt_uwrn_slot_next_x86");
    emitter.instruction("inc r11");                                             // advance the slot index
    emitter.instruction("jmp __rt_uwrn_slot_x86");                              // continue scanning slots

    // -- matched scheme: r12 = registry slot base --
    emitter.label("__rt_uwrn_match_x86");
    emitter.instruction("mov rax, QWORD PTR [r12 + 16]");                       // wrapper class name pointer from the registry slot
    emitter.instruction("mov rdx, QWORD PTR [r12 + 24]");                       // wrapper class name length (new_by_name reads rax/rdx)
    emitter.instruction("call __rt_new_by_name");                               // instantiate the wrapper class → rax = obj, or 0 when unknown
    emitter.instruction("test rax, rax");                                       // unknown class?
    emitter.instruction("jz __rt_uwrn_false_x86");                              // unknown class → false
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the throwaway wrapper instance

    // -- look up rename in the per-class user-wrapper vtable (slot 16) --
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_vtable_ptrs");      // base of the per-class user-wrapper vtable pointer table
    emitter.instruction("mov r10, QWORD PTR [r10 + r9 * 8]");                   // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {}]", VTABLE_RENAME_OFFSET)); // load the rename method pointer (slot 16)
    emitter.instruction("test r11, r11");                                       // class did not implement rename?
    emitter.instruction("jz __rt_uwrn_false_obj_x86");                          // missing method → false

    // -- call rename($this, from_ptr, from_len, to_ptr, to_len) → rax = bool --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // $this = wrapper object
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // from ptr → first string-arg pair
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // from len → first string-arg pair
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // to ptr → second string-arg pair
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // to len → second string-arg pair
    emitter.instruction("call r11");                                            // invoke the wrapper's rename method
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // stash the bool result across the instance release
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the throwaway wrapper object
    emitter.instruction("call __rt_decref_any");                                // free the throwaway wrapper instance
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the wrapper's bool result
    emitter.instruction("jmp __rt_uwrn_ret_x86");                               // share the common return path

    emitter.label("__rt_uwrn_false_obj_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the throwaway wrapper object
    emitter.instruction("call __rt_decref_any");                                // free it before returning false
    emitter.label("__rt_uwrn_false_x86");
    emitter.instruction("xor eax, eax");                                        // false: no scheme / unknown class / missing method

    emitter.label("__rt_uwrn_ret_x86");
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the bool result
}
