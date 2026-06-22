//! Purpose:
//! Emits the directory-iteration dispatch helpers for synthetic userspace
//! stream-wrapper descriptors: `__rt_user_wrapper_opendir` (path → `dir_opendir`,
//! vtable slot 19, allocating a handle and returning a synthetic fd) plus the
//! fd-based `__rt_user_wrapper_dir_readdir`/`dir_closedir`/`dir_rewinddir`
//! (vtable slots 20/21/22).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - `__rt_opendir`'s userspace-wrapper probe (for `__rt_user_wrapper_opendir`)
//!   and the `readdir`/`closedir`/`rewinddir` builtin emitters after a
//!   synthetic-fd check (`fd >= USER_WRAPPER_FD_BASE`).
//!
//! Key details:
//! - A directory handle is just a wrapper object held in the shared
//!   `_user_wrapper_handles` table (256-slot cap) under the same
//!   `0x40000000 | slot` synthetic fd as a stream handle; `dir_closedir` frees
//!   the slot exactly like `fclose`.
//! - `__rt_user_wrapper_opendir` returns the synthetic fd on success, `-1` when
//!   the matched wrapper failed (so `opendir()` boxes `false`), and `-2` when no
//!   registered scheme matched (so `__rt_opendir` falls through to `glob://`/libc).
//! - `dir_readdir` returns the wrapper method's declared string in the
//!   string-result pair (x1/x2 on ARM64, rax/rdx on x86_64); a zero-length
//!   result is normalized to a null pointer so `readdir()` boxes end-of-directory
//!   as `false` (a real entry name is never empty).

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Byte offset of `dir_opendir` in the per-class vtable (slot 19, 8 bytes each).
const VTABLE_DIR_OPENDIR_OFFSET: usize = 19 * 8;
/// Byte offset of `dir_readdir` in the per-class vtable (slot 20).
const VTABLE_DIR_READDIR_OFFSET: usize = 20 * 8;
/// Byte offset of `dir_closedir` in the per-class vtable (slot 21).
const VTABLE_DIR_CLOSEDIR_OFFSET: usize = 21 * 8;
/// Byte offset of `dir_rewinddir` in the per-class vtable (slot 22).
const VTABLE_DIR_REWINDDIR_OFFSET: usize = 22 * 8;

/// Emits `__rt_user_wrapper_opendir(path_ptr, path_len) -> fd | -1 | -2`.
///
/// Inputs (AArch64): x1 = path pointer, x2 = path length. (x86_64): rax = path
/// pointer, rdx = path length — matching `__rt_opendir`'s own entry convention so
/// the probe needs no register shuffle. Output: a synthetic wrapper fd
/// (`0x40000000 | slot`) when a registered scheme matched and its `dir_opendir`
/// returned true; `-1` when a scheme matched but instantiation/`dir_opendir`
/// failed; `-2` when no registered scheme matched the path.
pub fn emit_user_wrapper_opendir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_opendir_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_opendir ---");
    emitter.label_global("__rt_user_wrapper_opendir");

    // Frame: 48 bytes. [sp,#0..16] x29/x30, [sp,#16] path ptr, [sp,#24] path
    //   len, [sp,#32] obj.
    emitter.instruction("sub sp, sp, #48");                                     // helper frame for the opendir dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #16]");                                   // save the path pointer across the helper calls
    emitter.instruction("str x2, [sp, #24]");                                   // save the path length across the helper calls

    // -- scan the path for the "://" scheme separator (x1=ptr, x2=len) --
    emitter.instruction("mov x9, #0");                                          // scheme scan index
    emitter.label("__rt_uwod_scan");
    emitter.instruction("add x10, x9, #3");                                     // need three bytes for the "://" marker
    emitter.instruction("cmp x10, x2");                                         // do enough bytes remain in the path?
    emitter.instruction("b.gt __rt_uwod_none");                                 // no scheme separator → not a wrapper URL
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load the candidate ':' byte
    emitter.instruction("cmp w11, #58");                                        // is it ':'?
    emitter.instruction("b.ne __rt_uwod_scan_next");                            // not the scheme marker
    emitter.instruction("add x12, x9, #1");                                     // index of the first '/'
    emitter.instruction("ldrb w11, [x1, x12]");                                 // load the candidate first '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_uwod_scan_next");                            // not the scheme marker
    emitter.instruction("add x12, x9, #2");                                     // index of the second '/'
    emitter.instruction("ldrb w11, [x1, x12]");                                 // load the candidate second '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_uwod_scan_next");                            // not the scheme marker
    emitter.instruction("b __rt_uwod_check");                                   // "://" found at index x9 — x9 is the scheme length
    emitter.label("__rt_uwod_scan_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_uwod_scan");                                    // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (x9=scheme len) --
    emitter.label("__rt_uwod_check");
    abi::emit_symbol_address(emitter, "x10", "_user_wrappers");
    emitter.instruction("mov x11, #0");                                         // wrapper slot index
    emitter.label("__rt_uwod_slot");
    emitter.instruction("cmp x11, #64");                                        // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("b.ge __rt_uwod_none");                                 // no registered wrapper matched the scheme
    emitter.instruction("add x12, x10, x11, lsl #5");                           // slot base = table + index * 32
    emitter.instruction("ldr x13, [x12]");                                      // stored protocol pointer
    emitter.instruction("cbz x13, __rt_uwod_slot_next");                        // empty slot — skip it
    emitter.instruction("ldr x14, [x12, #8]");                                  // stored protocol length
    emitter.instruction("cmp x14, x9");                                         // does the stored length match the scheme length?
    emitter.instruction("b.ne __rt_uwod_slot_next");                            // length mismatch — try the next slot
    emitter.instruction("mov x15, #0");                                         // byte compare index
    emitter.label("__rt_uwod_bytes");
    emitter.instruction("cmp x15, x9");                                         // compared every protocol byte?
    emitter.instruction("b.ge __rt_uwod_match");                                // full match — instantiate the wrapper class
    emitter.instruction("ldrb w16, [x13, x15]");                                // stored protocol byte
    emitter.instruction("ldrb w17, [x1, x15]");                                 // path scheme byte (x1 still = path ptr)
    emitter.instruction("cmp w16, w17");                                        // do the bytes match?
    emitter.instruction("b.ne __rt_uwod_slot_next");                            // protocol byte differs — try the next slot
    emitter.instruction("add x15, x15, #1");                                    // advance the compare index
    emitter.instruction("b __rt_uwod_bytes");                                   // continue comparing bytes
    emitter.label("__rt_uwod_slot_next");
    emitter.instruction("add x11, x11, #1");                                    // advance the slot index
    emitter.instruction("b __rt_uwod_slot");                                    // continue scanning slots

    // -- matched scheme: x12 = registry slot base --
    emitter.label("__rt_uwod_match");
    emitter.instruction("ldr x1, [x12, #16]");                                  // wrapper class name pointer from the registry slot
    emitter.instruction("ldr x2, [x12, #24]");                                  // wrapper class name length from the registry slot
    emitter.instruction("bl __rt_new_by_name");                                 // instantiate the wrapper class → x0 = obj, or 0 when unknown
    emitter.instruction("cbz x0, __rt_uwod_fail_noobj");                        // unknown class → false (no object to free)
    emitter.instruction("str x0, [sp, #32]");                                   // save the wrapper instance

    // -- look up dir_opendir (vtable slot 19) for the object's class --
    emitter.instruction("ldr x9, [x0]");                                        // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // per-class user-wrapper vtable
    emitter.instruction(&format!("ldr x11, [x10, #{}]", VTABLE_DIR_OPENDIR_OFFSET)); //load the dir_opendir method pointer (slot 19)
    emitter.instruction("cbz x11, __rt_uwod_fail");                             // class did not implement dir_opendir → false

    // -- call dir_opendir($this, path_ptr, path_len, options=0) → x0 = bool --
    emitter.instruction("ldr x0, [sp, #32]");                                   // $this = wrapper object
    emitter.instruction("ldr x1, [sp, #16]");                                   // path ptr → string-arg pair
    emitter.instruction("ldr x2, [sp, #24]");                                   // path len → string-arg pair
    emitter.instruction("mov x3, #0");                                          // options = 0
    emitter.instruction("blr x11");                                             // invoke dir_opendir on the wrapper object
    emitter.instruction("cbz x0, __rt_uwod_fail");                              // dir_opendir returned false → free obj, false

    // -- success: allocate the first free slot in _user_wrapper_handles --
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_handles");
    emitter.instruction("mov x12, #0");                                         // start scanning from handle slot 0
    emitter.label("__rt_uwod_hslot");
    emitter.instruction("cmp x12, #256");                                       // does any free handle slot remain (USER_WRAPPER_HANDLES_CAP)?
    emitter.instruction("b.ge __rt_uwod_fail");                                 // no free handle slot → free obj, false
    emitter.instruction("ldr x13, [x10, x12, lsl #3]");                         // load slot — null means free
    emitter.instruction("cbz x13, __rt_uwod_hfound");                           // free slot found
    emitter.instruction("add x12, x12, #1");                                    // advance to the next handle slot
    emitter.instruction("b __rt_uwod_hslot");                                   // keep scanning for a free handle slot
    emitter.label("__rt_uwod_hfound");
    emitter.instruction("ldr x13, [sp, #32]");                                  // reload the wrapper object
    emitter.instruction("str x13, [x10, x12, lsl #3]");                         // _user_wrapper_handles[slot] = obj
    emitter.instruction("mov x0, #0x4000");                                     // low 16 bits of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl x0, x0, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("orr x0, x0, x12");                                     // synthetic fd = USER_WRAPPER_FD_BASE | slot index
    emitter.instruction("b __rt_uwod_ret");                                     // return the synthetic directory descriptor

    emitter.label("__rt_uwod_fail");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the wrapper object
    emitter.instruction("bl __rt_decref_any");                                  // free the instance before returning false
    emitter.label("__rt_uwod_fail_noobj");
    emitter.instruction("mov x0, #-1");                                         // -1: matched wrapper failed → opendir() boxes false
    emitter.instruction("b __rt_uwod_ret");                                     // share the common return path

    emitter.label("__rt_uwod_none");
    emitter.instruction("mov x0, #-2");                                         // -2: no registered scheme matched → fall through to libc

    emitter.label("__rt_uwod_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return fd / -1 / -2
}

/// x86_64 implementation of `__rt_user_wrapper_opendir`.
fn emit_user_wrapper_opendir_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_opendir ---");
    emitter.label_global("__rt_user_wrapper_opendir");

    // Frame: [rbp-8] path ptr, [rbp-16] path len, [rbp-24] obj.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // spill slots for path and the wrapper object
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the path pointer across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the path length across the helper calls

    // -- scan the path for the "://" scheme separator (rax=ptr, rdx=len) --
    emitter.instruction("xor r9, r9");                                          // scheme scan index
    emitter.label("__rt_uwod_scan_x86");
    emitter.instruction("lea r10, [r9 + 3]");                                   // need three bytes for the "://" marker
    emitter.instruction("cmp r10, rdx");                                        // do enough bytes remain in the path?
    emitter.instruction("jg __rt_uwod_none_x86");                               // no scheme separator → not a wrapper URL
    emitter.instruction("movzx r11d, BYTE PTR [rax + r9]");                     // load the candidate ':' byte
    emitter.instruction("cmp r11b, 58");                                        // is it ':'?
    emitter.instruction("jne __rt_uwod_next_x86");                              // not the scheme marker
    emitter.instruction("lea r12, [r9 + 1]");                                   // index of the first '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate first '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_uwod_next_x86");                              // not the scheme marker
    emitter.instruction("lea r12, [r9 + 2]");                                   // index of the second '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate second '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_uwod_next_x86");                              // not the scheme marker
    emitter.instruction("jmp __rt_uwod_check_x86");                             // "://" found at r9 — r9 is the scheme length
    emitter.label("__rt_uwod_next_x86");
    emitter.instruction("inc r9");                                              // advance the scan index
    emitter.instruction("jmp __rt_uwod_scan_x86");                              // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (r9=scheme len) --
    emitter.label("__rt_uwod_check_x86");
    abi::emit_symbol_address(emitter, "r10", "_user_wrappers");                 // base of the registered-wrapper table
    emitter.instruction("xor r11, r11");                                        // wrapper slot index
    emitter.label("__rt_uwod_slot_x86");
    emitter.instruction("cmp r11, 64");                                         // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("jge __rt_uwod_none_x86");                              // no registered wrapper matched the scheme
    emitter.instruction("mov r12, r11");                                        // copy the slot index for scaling
    emitter.instruction("shl r12, 5");                                          // slot offset = index * 32
    emitter.instruction("add r12, r10");                                        // slot base = table + offset
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // stored protocol pointer
    emitter.instruction("test r13, r13");                                       // is this slot empty?
    emitter.instruction("jz __rt_uwod_slotnext_x86");                           // empty slot — skip it
    emitter.instruction("mov r14, QWORD PTR [r12 + 8]");                        // stored protocol length
    emitter.instruction("cmp r14, r9");                                         // does the stored length match the scheme length?
    emitter.instruction("jne __rt_uwod_slotnext_x86");                          // length mismatch — try the next slot
    emitter.instruction("xor r15, r15");                                        // byte compare index
    emitter.label("__rt_uwod_bytes_x86");
    emitter.instruction("cmp r15, r9");                                         // compared every protocol byte?
    emitter.instruction("jge __rt_uwod_match_x86");                             // full match — instantiate the wrapper class
    emitter.instruction("movzx ecx, BYTE PTR [r13 + r15]");                     // stored protocol byte
    emitter.instruction("movzx esi, BYTE PTR [rax + r15]");                     // path scheme byte (rax still = path ptr)
    emitter.instruction("cmp cl, sil");                                         // do the bytes match?
    emitter.instruction("jne __rt_uwod_slotnext_x86");                          // protocol byte differs — try the next slot
    emitter.instruction("inc r15");                                             // advance the compare index
    emitter.instruction("jmp __rt_uwod_bytes_x86");                             // continue comparing bytes
    emitter.label("__rt_uwod_slotnext_x86");
    emitter.instruction("inc r11");                                             // advance the slot index
    emitter.instruction("jmp __rt_uwod_slot_x86");                              // continue scanning slots

    // -- matched scheme: r12 = registry slot base --
    emitter.label("__rt_uwod_match_x86");
    emitter.instruction("mov rax, QWORD PTR [r12 + 16]");                       // wrapper class name pointer from the registry slot
    emitter.instruction("mov rdx, QWORD PTR [r12 + 24]");                       // wrapper class name length from the registry slot
    emitter.instruction("call __rt_new_by_name");                               // instantiate the wrapper class → rax = obj, or 0 when unknown
    emitter.instruction("test rax, rax");                                       // did instantiation fail?
    emitter.instruction("jz __rt_uwod_failnoobj_x86");                          // unknown class → false (no object to free)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the wrapper instance

    // -- look up dir_opendir (vtable slot 19) for the object's class --
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");      // base of the per-class vtable pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-wrapper vtable
    emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", VTABLE_DIR_OPENDIR_OFFSET)); //load the dir_opendir method pointer (slot 19)
    emitter.instruction("test r11, r11");                                       // class did not implement dir_opendir?
    emitter.instruction("jz __rt_uwod_fail_x86");                               // missing dir_opendir → free obj, false

    // -- call dir_opendir($this, path_ptr, path_len, options=0) → rax = bool --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // $this = wrapper object
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // path ptr → string-arg pair
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // path len → string-arg pair
    emitter.instruction("xor rcx, rcx");                                        // options = 0
    emitter.instruction("call r11");                                            // invoke dir_opendir on the wrapper object
    emitter.instruction("test rax, rax");                                       // did dir_opendir return false?
    emitter.instruction("jz __rt_uwod_fail_x86");                               // dir_opendir returned false → free obj, false

    // -- success: allocate the first free slot in _user_wrapper_handles --
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("xor r12, r12");                                        // start scanning from handle slot 0
    emitter.label("__rt_uwod_hslot_x86");
    emitter.instruction("cmp r12, 256");                                        // does any free handle slot remain (USER_WRAPPER_HANDLES_CAP)?
    emitter.instruction("jge __rt_uwod_fail_x86");                              // no free handle slot → free obj, false
    emitter.instruction("mov r13, QWORD PTR [r10 + r12 * 8]");                  // load slot — null means free
    emitter.instruction("test r13, r13");                                       // is this slot free?
    emitter.instruction("jz __rt_uwod_hfound_x86");                             // free slot found
    emitter.instruction("inc r12");                                             // advance to the next handle slot
    emitter.instruction("jmp __rt_uwod_hslot_x86");                             // keep scanning for a free handle slot
    emitter.label("__rt_uwod_hfound_x86");
    emitter.instruction("mov r13, QWORD PTR [rbp - 24]");                       // reload the wrapper object
    emitter.instruction("mov QWORD PTR [r10 + r12 * 8], r13");                  // _user_wrapper_handles[slot] = obj
    emitter.instruction("mov rax, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("or rax, r12");                                         // synthetic fd = USER_WRAPPER_FD_BASE | slot index
    emitter.instruction("jmp __rt_uwod_ret_x86");                               // return the synthetic directory descriptor

    emitter.label("__rt_uwod_fail_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the wrapper object
    emitter.instruction("call __rt_decref_any");                                // free the instance before returning false
    emitter.label("__rt_uwod_failnoobj_x86");
    emitter.instruction("mov rax, -1");                                         // -1: matched wrapper failed → opendir() boxes false
    emitter.instruction("jmp __rt_uwod_ret_x86");                               // share the common return path

    emitter.label("__rt_uwod_none_x86");
    emitter.instruction("mov rax, -2");                                         // -2: no registered scheme matched → fall through to libc

    emitter.label("__rt_uwod_ret_x86");
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return fd / -1 / -2
}

/// Emits `__rt_user_wrapper_dir_readdir(fd) -> string`.
///
/// Inputs (AArch64): x0 = synthetic wrapper fd. (x86_64): rdi = fd. Output: the
/// wrapper `dir_readdir()` string in the string-result pair (x1/x2 on ARM64,
/// rax/rdx on x86_64). A missing handle/method or a zero-length result yields a
/// null pointer so `readdir()` boxes end-of-directory as `false`.
pub fn emit_user_wrapper_dir_readdir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_dir_readdir_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_dir_readdir ---");
    emitter.label_global("__rt_user_wrapper_dir_readdir");

    emitter.instruction("sub sp, sp, #16");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- resolve the open wrapper instance from the synthetic fd --
    emitter.instruction("mov x9, #0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("sub x9, x0, x9");                                      // x9 = handle slot index = fd - BASE
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_handles");
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // obj = _user_wrapper_handles[slot]
    emitter.instruction("cbz x0, __rt_uwrd_empty");                             // empty slot → end of directory

    // -- resolve dir_readdir (vtable slot 20) for the object's class --
    emitter.instruction("ldr x10, [x0]");                                       // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x11", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // per-class user-wrapper vtable
    emitter.instruction(&format!("ldr x11, [x11, #{}]", VTABLE_DIR_READDIR_OFFSET)); //load the dir_readdir method pointer (slot 20)
    emitter.instruction("cbz x11, __rt_uwrd_empty");                            // class did not implement dir_readdir → end of directory

    // -- call dir_readdir($this) → returns string in x1/x2 --
    emitter.instruction("blr x11");                                             // invoke dir_readdir on the wrapper object
    emitter.instruction("cbz x2, __rt_uwrd_empty");                             // empty name (len 0) is the end-of-directory sentinel
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the entry name in x1/x2

    emitter.label("__rt_uwrd_empty");
    emitter.instruction("mov x1, #0");                                          // null pointer → readdir() boxes false
    emitter.instruction("mov x2, #0");                                          // zero length for the end-of-directory result
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the end-of-directory result
}

/// x86_64 implementation of `__rt_user_wrapper_dir_readdir`.
fn emit_user_wrapper_dir_readdir_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_dir_readdir ---");
    emitter.label_global("__rt_user_wrapper_dir_readdir");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    // -- resolve the open wrapper instance from the synthetic fd --
    emitter.instruction("mov r9, rdi");                                         // copy the synthetic fd
    emitter.instruction("sub r9, 0x40000000");                                  // r9 = handle slot index = fd - USER_WRAPPER_FD_BASE
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9 * 8]");                   // obj = _user_wrapper_handles[slot]
    emitter.instruction("test rdi, rdi");                                       // empty slot?
    emitter.instruction("jz __rt_uwrd_empty_x86");                              // empty slot → end of directory

    // -- resolve dir_readdir (vtable slot 20) for the object's class --
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");      // base of the per-class vtable pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-wrapper vtable
    emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", VTABLE_DIR_READDIR_OFFSET)); //load the dir_readdir method pointer (slot 20)
    emitter.instruction("test r11, r11");                                       // class did not implement dir_readdir?
    emitter.instruction("jz __rt_uwrd_empty_x86");                              // missing dir_readdir → end of directory

    // -- call dir_readdir($this) → returns string in rax/rdx --
    emitter.instruction("call r11");                                            // invoke dir_readdir on the wrapper object
    emitter.instruction("test rdx, rdx");                                       // empty name (len 0) is the end-of-directory sentinel
    emitter.instruction("jz __rt_uwrd_empty_x86");                              // box end-of-directory as false
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the entry name in rax/rdx

    emitter.label("__rt_uwrd_empty_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer → readdir() boxes false
    emitter.instruction("xor edx, edx");                                        // zero length for the end-of-directory result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the end-of-directory result
}

/// Emits `__rt_user_wrapper_dir_closedir(fd) -> 1`.
///
/// Inputs (AArch64): x0 = synthetic wrapper fd. (x86_64): rdi = fd. Invokes the
/// wrapper's `dir_closedir()` (if present), then frees the `_user_wrapper_handles`
/// slot so the synthetic fd cannot be reused stale. Always returns 1, mirroring
/// `fclose`'s wrapper behavior.
pub fn emit_user_wrapper_dir_closedir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_dir_closedir_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_dir_closedir ---");
    emitter.label_global("__rt_user_wrapper_dir_closedir");

    // Frame: 32 bytes. [sp,#0..16] x29/x30, [sp,#16] fd.
    emitter.instruction("sub sp, sp, #32");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the synthetic file descriptor

    // -- resolve the open wrapper instance from the synthetic fd --
    emitter.instruction("mov x9, #0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("sub x9, x0, x9");                                      // x9 = handle slot index = fd - BASE
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_handles");
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // obj = _user_wrapper_handles[slot]
    emitter.instruction("cbz x0, __rt_uwcd_clear");                             // empty slot → just clear and report success

    // -- resolve dir_closedir (vtable slot 21) for the object's class --
    emitter.instruction("ldr x10, [x0]");                                       // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x11", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // per-class user-wrapper vtable
    emitter.instruction(&format!("ldr x11, [x11, #{}]", VTABLE_DIR_CLOSEDIR_OFFSET)); //load the dir_closedir method pointer (slot 21)
    emitter.instruction("cbz x11, __rt_uwcd_clear");                            // class did not implement dir_closedir → just clear
    emitter.instruction("blr x11");                                             // invoke dir_closedir on the wrapper object

    emitter.label("__rt_uwcd_clear");
    // -- free the handle slot so the synthetic fd cannot be reused stale --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the synthetic file descriptor
    emitter.instruction("mov x9, #0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("sub x9, x0, x9");                                      // x9 = handle slot index = fd - BASE
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_handles");
    emitter.instruction("str xzr, [x10, x9, lsl #3]");                          // clear the freed handle slot
    emitter.instruction("mov x0, #1");                                          // closedir() on a wrapper always reports success
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return success
}

/// x86_64 implementation of `__rt_user_wrapper_dir_closedir`.
fn emit_user_wrapper_dir_closedir_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_dir_closedir ---");
    emitter.label_global("__rt_user_wrapper_dir_closedir");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // helper frame for the wrapper dispatch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the synthetic file descriptor

    // -- resolve the open wrapper instance from the synthetic fd --
    emitter.instruction("mov r9, rdi");                                         // copy the synthetic fd
    emitter.instruction("sub r9, 0x40000000");                                  // r9 = handle slot index = fd - USER_WRAPPER_FD_BASE
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9 * 8]");                   // obj = _user_wrapper_handles[slot]
    emitter.instruction("test rdi, rdi");                                       // empty slot?
    emitter.instruction("jz __rt_uwcd_clear_x86");                              // empty slot → just clear and report success

    // -- resolve dir_closedir (vtable slot 21) for the object's class --
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");      // base of the per-class vtable pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-wrapper vtable
    emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", VTABLE_DIR_CLOSEDIR_OFFSET)); //load the dir_closedir method pointer (slot 21)
    emitter.instruction("test r11, r11");                                       // class did not implement dir_closedir?
    emitter.instruction("jz __rt_uwcd_clear_x86");                              // missing dir_closedir → just clear
    emitter.instruction("call r11");                                            // invoke dir_closedir on the wrapper object

    emitter.label("__rt_uwcd_clear_x86");
    // -- free the handle slot so the synthetic fd cannot be reused stale --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the synthetic file descriptor
    emitter.instruction("mov r9, rdi");                                         // copy the synthetic fd
    emitter.instruction("sub r9, 0x40000000");                                  // r9 = handle slot index = fd - USER_WRAPPER_FD_BASE
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], 0");                     // clear the freed handle slot
    emitter.instruction("mov eax, 1");                                          // closedir() on a wrapper always reports success
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return success
}

/// Emits `__rt_user_wrapper_dir_rewinddir(fd) -> bool`.
///
/// Inputs (AArch64): x0 = synthetic wrapper fd. (x86_64): rdi = fd. Invokes the
/// wrapper's `dir_rewinddir()` and returns its bool result, or 0 when the handle
/// slot is empty or the class does not implement the method.
pub fn emit_user_wrapper_dir_rewinddir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_dir_rewinddir_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_dir_rewinddir ---");
    emitter.label_global("__rt_user_wrapper_dir_rewinddir");

    emitter.instruction("sub sp, sp, #16");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- resolve the open wrapper instance from the synthetic fd --
    emitter.instruction("mov x9, #0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("sub x9, x0, x9");                                      // x9 = handle slot index = fd - BASE
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_handles");
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // obj = _user_wrapper_handles[slot]
    emitter.instruction("cbz x0, __rt_uwrw_false");                             // empty slot → false

    // -- resolve dir_rewinddir (vtable slot 22) for the object's class --
    emitter.instruction("ldr x10, [x0]");                                       // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x11", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // per-class user-wrapper vtable
    emitter.instruction(&format!("ldr x11, [x11, #{}]", VTABLE_DIR_REWINDDIR_OFFSET)); //load the dir_rewinddir method pointer (slot 22)
    emitter.instruction("cbz x11, __rt_uwrw_false");                            // class did not implement dir_rewinddir → false

    // -- call dir_rewinddir($this) → bool in x0 --
    emitter.instruction("blr x11");                                             // invoke dir_rewinddir on the wrapper object
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the wrapper's bool result

    emitter.label("__rt_uwrw_false");
    emitter.instruction("mov x0, #0");                                          // false when the handle or method is absent
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return false
}

/// x86_64 implementation of `__rt_user_wrapper_dir_rewinddir`.
fn emit_user_wrapper_dir_rewinddir_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_dir_rewinddir ---");
    emitter.label_global("__rt_user_wrapper_dir_rewinddir");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    // -- resolve the open wrapper instance from the synthetic fd --
    emitter.instruction("mov r9, rdi");                                         // copy the synthetic fd
    emitter.instruction("sub r9, 0x40000000");                                  // r9 = handle slot index = fd - USER_WRAPPER_FD_BASE
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9 * 8]");                   // obj = _user_wrapper_handles[slot]
    emitter.instruction("test rdi, rdi");                                       // empty slot?
    emitter.instruction("jz __rt_uwrw_false_x86");                              // empty slot → false

    // -- resolve dir_rewinddir (vtable slot 22) for the object's class --
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");      // base of the per-class vtable pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-wrapper vtable
    emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", VTABLE_DIR_REWINDDIR_OFFSET)); //load the dir_rewinddir method pointer (slot 22)
    emitter.instruction("test r11, r11");                                       // class did not implement dir_rewinddir?
    emitter.instruction("jz __rt_uwrw_false_x86");                              // missing dir_rewinddir → false

    // -- call dir_rewinddir($this) → bool in rax --
    emitter.instruction("call r11");                                            // invoke dir_rewinddir on the wrapper object
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper's bool result

    emitter.label("__rt_uwrw_false_x86");
    emitter.instruction("xor eax, eax");                                        // false when the handle or method is absent
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return false
}
