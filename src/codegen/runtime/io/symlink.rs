//! Purpose:
//! Emits symbolic / hard link runtime helpers (`__rt_symlink`, `__rt_link`,
//! `__rt_readlink`, `__rt_linkinfo`).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` and
//!   `crate::codegen::runtime::x86_minimal::emit_runtime_linux_x86_64_minimal()`.
//!
//! Key details:
//! - All helpers route through libc to avoid per-syscall remapping work.
//! - `__rt_readlink` returns an owned-heap string (pointer/length in
//!   `x1/x2` or `rax/rdx`); the codegen wrapper boxes it as `Mixed`
//!   (`string|false`).
//! - `__rt_linkinfo` returns the `st_dev` field or PHP's `-1` failure value.

use crate::codegen::{emit::Emitter, platform::Arch};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits the `__rt_symlink` runtime helper on ARM64, or dispatches to the
/// x86_64-specific emitter. On ARM64: takes `target` (x1/x2) and `link` (x3/x4)
/// as PHP string pointer/length pairs, calls `libc::symlink`, and returns
/// x0 = 1 on success, x0 = 0 on failure.
pub fn emit_symlink(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_symlink_linux_x86_64(emitter);
        return;
    }

    // ================================================================
    // __rt_symlink: libc symlink(target, link).
    // Input:  x1/x2 = target, x3/x4 = link
    // Output: x0 = 1 on success, 0 on failure
    //
    // Frame layout (48 bytes):
    //   sp+ 0 : link source ptr (saved from x3 across cstr calls)
    //   sp+ 8 : link source len (saved from x4)
    //   sp+16 : target C-string pointer (returned by __rt_cstr)
    //   sp+32 : x29 / x30
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: symlink ---");
    emitter.label_global("__rt_symlink");
    emitter.instruction("sub sp, sp, #48");                                     // allocate frame + spill slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer
    emitter.instruction("stp x3, x4, [sp, #0]");                                // spill link source (ptr+len) so it survives the cstr calls
    emitter.instruction("bl __rt_cstr");                                        // target → cstring in _cstr_buf, returned in x0
    emitter.instruction("str x0, [sp, #16]");                                   // save target C-string pointer
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore link source into the cstr2 input slot
    emitter.instruction("bl __rt_cstr2");                                       // link → cstring in _cstr_buf2, returned in x0
    emitter.instruction("mov x1, x0");                                          // libc symlink second arg = link C-string
    emitter.instruction("ldr x0, [sp, #16]");                                   // libc symlink first arg = target C-string
    emitter.bl_c("symlink");                                                    // libc symlink(target, link)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if symlink succeeded
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate

    // ================================================================
    // __rt_link: libc link(oldpath, newpath). Same ABI/skeleton as symlink.
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: link ---");
    emitter.label_global("__rt_link");
    emitter.instruction("sub sp, sp, #48");                                     // allocate frame + spill slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer
    emitter.instruction("stp x3, x4, [sp, #0]");                                // spill new-path source (ptr+len) so it survives the cstr calls
    emitter.instruction("bl __rt_cstr");                                        // old path → cstring in _cstr_buf
    emitter.instruction("str x0, [sp, #16]");                                   // save old C path
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore new-path source into the cstr2 input slot
    emitter.instruction("bl __rt_cstr2");                                       // new path → cstring in _cstr_buf2
    emitter.instruction("mov x1, x0");                                          // libc link second arg = new C path
    emitter.instruction("ldr x0, [sp, #16]");                                   // libc link first arg = old C path
    emitter.bl_c("link");                                                       // libc link(oldpath, newpath)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if link succeeded
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate

    // ================================================================
    // __rt_readlink: libc readlink(path, buf, 4096).
    // Input:  x1/x2 = path
    // Output: x1/x2 = canonical link target (owned-heap string), or 0/0 on failure
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: readlink ---");
    emitter.label_global("__rt_readlink");
    emitter.instruction("sub sp, sp, #32");                                     // allocate frame + spill slots for cstr & buffer pointers
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer

    emitter.instruction("bl __rt_cstr");                                        // path → null-terminated C path in x0
    emitter.instruction("str x0, [sp, #0]");                                    // save C path pointer

    emitter.instruction("mov x0, #4096");                                       // PATH_MAX-style buffer for the link target
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate owned heap storage; result in x0
    emitter.instruction("mov x9, #1");                                          // heap-kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // tag the buffer in the uniform heap header
    emitter.instruction("str x0, [sp, #8]");                                    // save buffer pointer

    emitter.instruction("ldr x0, [sp, #0]");                                    // first arg: path
    emitter.instruction("ldr x1, [sp, #8]");                                    // second arg: buffer
    emitter.instruction("mov x2, #4096");                                       // third arg: capacity
    emitter.bl_c("readlink");                                                   // libc readlink → bytes filled or -1
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("b.lt __rt_readlink_fail");                             // -1 → failure path

    emitter.instruction("ldr x1, [sp, #8]");                                    // result pointer = owned buffer
    emitter.instruction("mov x2, x0");                                          // result length = readlink return
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return canonical link target

    emitter.label("__rt_readlink_fail");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload owned buffer for cleanup
    emitter.instruction("bl __rt_heap_free");                                   // release the unused buffer
    emitter.instruction("mov x1, #0");                                          // empty pointer (string|false sentinel)
    emitter.instruction("mov x2, #0");                                          // empty length (string|false sentinel)
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return empty string

    // ================================================================
    // __rt_linkinfo: libc lstat(path), return st_dev.
    // Input:  x1/x2 = path
    // Output: x0 = device id, or -1 on failure
    //
    // st_dev sits at offset 0 in both Darwin (int32_t) and Linux (__dev_t),
    // but the field width differs by platform.
    // ================================================================
    let plat = emitter.platform;
    let stat_buf = plat.stat_buf_size();
    let frame_size = (stat_buf + 32 + 15) & !15;
    let save_offset = frame_size - 16;

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: linkinfo ---");
    emitter.label_global("__rt_linkinfo");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate frame + stat buffer
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_offset));      // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_offset));             // establish new frame pointer

    emitter.instruction("bl __rt_cstr");                                        // path → C string in x0
    emitter.instruction("add x1, sp, #0");                                      // stat buffer
    emitter.bl_c("lstat");                                                      // libc lstat(path, buf)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("b.ne __rt_linkinfo_fail");                             // failure → PHP -1
    emitter.instruction(&plat.stat_dev_load_instr("x0", "sp", 0));              // load st_dev using the platform field width
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame
    emitter.instruction("ret");                                                 // return device id

    emitter.label("__rt_linkinfo_fail");
    emitter.instruction("mov x0, #-1");                                         // failure: return PHP's linkinfo() sentinel
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame
    emitter.instruction("ret");                                                 // return failure sentinel
}

/// Emits `__rt_symlink`, `__rt_link`, `__rt_readlink`, and `__rt_linkinfo`
/// for the Linux x86_64 target. Takes arguments via the System V AMD64 ABI:
/// rdi = link ptr, rsi = link len, rdx = target ptr, rcx = target len.
/// `__rt_readlink` returns an owned-heap string in rax/rdx; failure returns
/// 0/0 (mapped by the codegen wrapper to `string|false`).
/// `__rt_linkinfo` returns the `st_dev` field or -1 on failure.
fn emit_symlink_linux_x86_64(emitter: &mut Emitter) {
    // -- symlink --
    emitter.blank();
    emitter.comment("--- runtime: symlink ---");
    emitter.label_global("__rt_symlink");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 32");                                         // reserve spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save target ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save target len
    emitter.instruction("mov rax, rdi");                                        // move link ptr into cstr input
    emitter.instruction("mov rdx, rsi");                                        // move link len into cstr input
    emitter.instruction("call __rt_cstr2");                                     // link → C string in rax
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save link C path
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload target ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload target len
    emitter.instruction("call __rt_cstr");                                      // target → C string in rax
    emitter.instruction("mov rdi, rax");                                        // first libc symlink arg = target
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // second libc symlink arg = link
    emitter.instruction("call symlink");                                        // libc symlink(target, link)
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen
    emitter.instruction("add rsp, 32");                                         // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- link --
    emitter.blank();
    emitter.comment("--- runtime: link ---");
    emitter.label_global("__rt_link");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 32");                                         // reserve spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save old path ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save old path len
    emitter.instruction("mov rax, rdi");                                        // move new path into cstr input
    emitter.instruction("mov rdx, rsi");                                        // move new path len into cstr input
    emitter.instruction("call __rt_cstr2");                                     // new path → C string
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save new C path
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload old path
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload old path len
    emitter.instruction("call __rt_cstr");                                      // old path → C string
    emitter.instruction("mov rdi, rax");                                        // first libc link arg = old path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // second arg = new path
    emitter.instruction("call link");                                           // libc link(oldpath, newpath)
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen
    emitter.instruction("add rsp, 32");                                         // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- readlink --
    emitter.blank();
    emitter.comment("--- runtime: readlink ---");
    emitter.label_global("__rt_readlink");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 32");                                         // reserve spill slots: cstr / buffer
    emitter.instruction("call __rt_cstr");                                      // path → C string in rax
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save C path

    emitter.instruction("mov rax, 4096");                                       // request 4 KiB owned buffer
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned heap storage in rax
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // owned-string heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // tag the buffer in the uniform heap header
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save buffer pointer

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // first libc readlink arg = path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // second arg = buffer
    emitter.instruction("mov rdx, 4096");                                       // third arg = capacity
    emitter.instruction("call readlink");                                       // libc readlink → rax = bytes or -1
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("jl __rt_readlink_fail_x86");                           // failure path

    emitter.instruction("mov rdx, rax");                                        // result length
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // result pointer = owned buffer
    emitter.instruction("add rsp, 32");                                         // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return owned string

    emitter.label("__rt_readlink_fail_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload buffer for cleanup
    emitter.instruction("call __rt_heap_free");                                 // release unused buffer
    emitter.instruction("xor eax, eax");                                        // empty string pointer
    emitter.instruction("xor edx, edx");                                        // empty string length
    emitter.instruction("add rsp, 32");                                         // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return empty string

    // -- linkinfo --
    let stat_buf = 144usize;
    let frame = ((stat_buf + 16) + 15) & !15;
    emitter.blank();
    emitter.comment("--- runtime: linkinfo ---");
    emitter.label_global("__rt_linkinfo");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction(&format!("sub rsp, {}", frame));                        // reserve stat buffer
    emitter.instruction("call __rt_cstr");                                      // path → C string in rax
    emitter.instruction("mov rdi, rax");                                        // first libc lstat arg = path
    emitter.instruction("lea rsi, [rsp]");                                      // stat buffer
    emitter.instruction("call lstat");                                          // libc lstat()
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("jne __rt_linkinfo_fail_x86");                          // failure → PHP -1
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // load 64-bit Linux st_dev from offset 0
    emitter.instruction(&format!("add rsp, {}", frame));                        // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return device id
    emitter.label("__rt_linkinfo_fail_x86");
    emitter.instruction("mov rax, -1");                                         // failure → PHP's linkinfo() sentinel
    emitter.instruction(&format!("add rsp, {}", frame));                        // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return failure sentinel
}
