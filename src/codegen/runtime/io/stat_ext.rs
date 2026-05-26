//! Purpose:
//! Emits the `__rt_filesize`, `__rt_filemtime` runtime helper assembly for stat ext.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the `__rt_filesize`, `__rt_filemtime` runtime helper assembly for stat ext.
/// Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
///
/// Called from:
/// - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
///
/// Key details:
/// - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.
pub fn emit_stat_ext(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stat_ext_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let stat_buf = plat.stat_buf_size();
    let frame_size = (stat_buf + 32 + 15) & !15;
    let save_offset = frame_size - 16;
    let mode_off = plat.stat_mode_offset();
    let atime_off = plat.stat_atime_offset();
    let ctime_off = plat.stat_ctime_offset();
    let ino_off = plat.stat_ino_offset();
    let uid_off = plat.stat_uid_offset();
    let gid_off = plat.stat_gid_offset();

    // Helper closure that emits the standard prologue for a stat-based scalar
    // helper: setup frame, cstr the path, syscall stat64, then leave the
    // caller to interpret the buffer. The buffer lives at sp+0..stat_buf.
    let emit_prologue = |emitter: &mut Emitter| {
        emitter.instruction(&format!("sub sp, sp, #{}", frame_size));           // allocate stack for stat buf + frame
        emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_offset));  // save frame pointer and return address
        emitter.instruction(&format!("add x29, sp, #{}", save_offset));         // establish new frame pointer
        emitter.instruction("bl __rt_cstr");                                    // null-terminate the path; x0 = cstr pointer
        emitter.instruction("add x1, sp, #0");                                  // pointer to stat buffer on stack
        emitter.syscall(338);
    };
    let emit_epilogue = |emitter: &mut Emitter| {
        emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));  // restore frame pointer and return address
        emitter.instruction(&format!("add sp, sp, #{}", frame_size));           // deallocate stack frame
        emitter.instruction("ret");                                             // return to caller
    };

    // ================================================================
    // __rt_fileatime / __rt_filectime: load a timespec.tv_sec field
    // ================================================================
    for (label, off) in [
        ("__rt_fileatime", atime_off),
        ("__rt_filectime", ctime_off),
    ] {
        emitter.blank();
        emitter.raw("    .p2align 2");                                          // ensure 4-byte alignment for the next runtime helper
        emitter.comment(&format!("--- runtime: {} ---", &label[5..]));
        emitter.label_global(label);
        emit_prologue(emitter);
        emitter.instruction("cmp x0, #0");                                      // did stat() succeed?
        emitter.instruction(&format!("b.ne {}_fail", label));                   // failure path: return false flag
        emitter.instruction(&format!("ldr x0, [sp, #{}]", off));                // load tv_sec at the requested timespec offset
        emitter.instruction("mov x1, #1");                                      // success flag for codegen-side int|false boxing
        emit_epilogue(emitter);
        emitter.label(&format!("{}_fail", label));
        emitter.instruction("mov x0, #0");                                      // stat failed: integer payload defaults to 0
        emitter.instruction("mov x1, #0");                                      // failure flag tells codegen to box PHP false
        emit_epilogue(emitter);
    }

    // ================================================================
    // __rt_fileperms: full st_mode (file-type bits + permissions)
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: fileperms ---");
    emitter.label_global("__rt_fileperms");
    emit_prologue(emitter);
    emitter.instruction("cmp x0, #0");                                          // stat success?
    emitter.instruction("b.ne __rt_fileperms_fail");                            // failure → false flag
    emitter.instruction(&plat.stat_mode_load_instr("w0", "sp", mode_off));      // load full st_mode (zero-extended into x0)
    emitter.instruction("mov x1, #1");                                          // success flag for codegen-side int|false boxing
    emit_epilogue(emitter);
    emitter.label("__rt_fileperms_fail");
    emitter.instruction("mov x0, #0");                                          // stat failed: integer payload defaults to 0
    emitter.instruction("mov x1, #0");                                          // failure flag tells codegen to box PHP false
    emit_epilogue(emitter);

    // ================================================================
    // __rt_fileowner / __rt_filegroup: 32-bit uid / gid load
    // ================================================================
    for (label, off) in [
        ("__rt_fileowner", uid_off),
        ("__rt_filegroup", gid_off),
    ] {
        emitter.blank();
        emitter.raw("    .p2align 2");                                          // ensure 4-byte alignment for the next runtime helper
        emitter.comment(&format!("--- runtime: {} ---", &label[5..]));
        emitter.label_global(label);
        emit_prologue(emitter);
        emitter.instruction("cmp x0, #0");                                      // stat success?
        emitter.instruction(&format!("b.ne {}_fail", label));                   // failure → false flag
        emitter.instruction(&format!("ldr w0, [sp, #{}]", off));                // load 32-bit uid/gid (zero-extended)
        emitter.instruction("mov x1, #1");                                      // success flag for codegen-side int|false boxing
        emit_epilogue(emitter);
        emitter.label(&format!("{}_fail", label));
        emitter.instruction("mov x0, #0");                                      // stat failed: integer payload defaults to 0
        emitter.instruction("mov x1, #0");                                      // failure flag tells codegen to box PHP false
        emit_epilogue(emitter);
    }

    // ================================================================
    // __rt_fileinode: 64-bit st_ino
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: fileinode ---");
    emitter.label_global("__rt_fileinode");
    emit_prologue(emitter);
    emitter.instruction("cmp x0, #0");                                          // stat success?
    emitter.instruction("b.ne __rt_fileinode_fail");                            // failure → false flag
    emitter.instruction(&format!("ldr x0, [sp, #{}]", ino_off));                // load 64-bit st_ino
    emitter.instruction("mov x1, #1");                                          // success flag for codegen-side int|false boxing
    emit_epilogue(emitter);
    emitter.label("__rt_fileinode_fail");
    emitter.instruction("mov x0, #0");                                          // stat failed: integer payload defaults to 0
    emitter.instruction("mov x1, #0");                                          // failure flag tells codegen to box PHP false
    emit_epilogue(emitter);

    // ================================================================
    // __rt_filetype: returns one of "file"/"dir"/"link"/"char"/"block"/
    //   "fifo"/"socket"/"unknown" as a borrowed pointer into runtime data.
    // Output: x1=ptr, x2=len. Uses lstat() semantics so symlinks report "link".
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: filetype ---");
    emitter.label_global("__rt_filetype");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack for stat buf + frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_offset));      // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_offset));             // establish new frame pointer
    emitter.instruction("bl __rt_cstr");                                        // null-terminate the path; x0 = cstr pointer
    emitter.instruction("add x1, sp, #0");                                      // pointer to stat buffer on stack
    emitter.syscall(340);                                                       // lstat64 (Darwin 340 / Linux remap to fstatat)
    emitter.instruction("cmp x0, #0");                                          // lstat success?
    emitter.instruction("b.ne __rt_filetype_fail");                             // lstat failed → PHP false

    emitter.instruction(&plat.stat_mode_load_instr("w9", "sp", mode_off));      // load st_mode
    emitter.instruction("and w9, w9, #0xF000");                                 // mask with S_IFMT
    emitter.instruction("mov w10, #0x8000");                                    // S_IFREG
    emitter.instruction("cmp w9, w10");                                         // file?
    emitter.instruction("b.eq __rt_filetype_file");                             // → "file"
    emitter.instruction("mov w10, #0x4000");                                    // S_IFDIR
    emitter.instruction("cmp w9, w10");                                         // dir?
    emitter.instruction("b.eq __rt_filetype_dir");                              // → "dir"
    emitter.instruction("mov w10, #0xA000");                                    // S_IFLNK
    emitter.instruction("cmp w9, w10");                                         // symlink?
    emitter.instruction("b.eq __rt_filetype_link");                             // → "link"
    emitter.instruction("mov w10, #0x2000");                                    // S_IFCHR
    emitter.instruction("cmp w9, w10");                                         // character device?
    emitter.instruction("b.eq __rt_filetype_char");                             // → "char"
    emitter.instruction("mov w10, #0x6000");                                    // S_IFBLK
    emitter.instruction("cmp w9, w10");                                         // block device?
    emitter.instruction("b.eq __rt_filetype_block");                            // → "block"
    emitter.instruction("mov w10, #0x1000");                                    // S_IFIFO
    emitter.instruction("cmp w9, w10");                                         // fifo?
    emitter.instruction("b.eq __rt_filetype_fifo");                             // → "fifo"
    emitter.instruction("mov w10, #0xC000");                                    // S_IFSOCK
    emitter.instruction("cmp w9, w10");                                         // socket?
    emitter.instruction("b.eq __rt_filetype_socket");                           // → "socket"
    emitter.instruction("b __rt_filetype_unknown");                             // fall-through → "unknown"

    let ft_emit = |emitter: &mut Emitter, sym: &str, len: i64| {
        emitter.adrp("x1", sym);                                                // load page of the type-name literal
        emitter.add_lo12("x1", "x1", sym);                                      // resolve full address of the type-name literal
        emitter.instruction(&format!("mov x2, #{}", len));                      // length of the type-name literal
        emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));  // restore frame pointer and return address
        emitter.instruction(&format!("add sp, sp, #{}", frame_size));           // deallocate stack frame
        emitter.instruction("ret");                                             // return type-name slice
    };
    emitter.label("__rt_filetype_file");
    ft_emit(emitter, "_filetype_file", 4);
    emitter.label("__rt_filetype_dir");
    ft_emit(emitter, "_filetype_dir", 3);
    emitter.label("__rt_filetype_link");
    ft_emit(emitter, "_filetype_link", 4);
    emitter.label("__rt_filetype_char");
    ft_emit(emitter, "_filetype_char", 4);
    emitter.label("__rt_filetype_block");
    ft_emit(emitter, "_filetype_block", 5);
    emitter.label("__rt_filetype_fifo");
    ft_emit(emitter, "_filetype_fifo", 4);
    emitter.label("__rt_filetype_socket");
    ft_emit(emitter, "_filetype_socket", 6);
    emitter.label("__rt_filetype_unknown");
    ft_emit(emitter, "_filetype_unknown", 7);
    emitter.label("__rt_filetype_fail");
    emitter.instruction("mov x1, #0");                                          // null string pointer tells codegen to box PHP false
    emitter.instruction("mov x2, #0");                                          // failure string length is zero
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return the failure sentinel

    // ================================================================
    // __rt_is_executable: access(path, X_OK) — same skeleton as is_readable
    // Input:  x1/x2 = path
    // Output: x0 = 1 if executable, 0 otherwise
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: is_executable ---");
    emitter.label_global("__rt_is_executable");
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer
    emitter.instruction("bl __rt_cstr");                                        // null-terminate path
    emitter.instruction("mov x1, #1");                                          // X_OK = 1 (execute permission check)
    emitter.syscall(33);                                                        // access(path, X_OK)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if access succeeded
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return executable predicate

    // ================================================================
    // __rt_is_link: lstat() + check S_ISLNK on st_mode
    // Input:  x1/x2 = path
    // Output: x0 = 1 if symlink, 0 otherwise
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: is_link ---");
    emitter.label_global("__rt_is_link");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack for stat buf + frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_offset));      // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_offset));             // establish new frame pointer
    emitter.instruction("bl __rt_cstr");                                        // null-terminate the path; x0 = cstr pointer
    emitter.instruction("add x1, sp, #0");                                      // pointer to stat buffer on stack
    emitter.syscall(340);                                                       // lstat
    emitter.instruction("cmp x0, #0");                                          // lstat success?
    emitter.instruction("b.ne __rt_is_link_no");                                // failure → 0
    emitter.instruction(&plat.stat_mode_load_instr("w9", "sp", mode_off));      // load st_mode
    emitter.instruction("and w9, w9, #0xF000");                                 // mask with S_IFMT
    emitter.instruction("mov w10, #0xA000");                                    // S_IFLNK
    emitter.instruction("cmp w9, w10");                                         // is it a symlink?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if S_ISLNK
    emit_epilogue(emitter);
    emitter.label("__rt_is_link_no");
    emitter.instruction("mov x0, #0");                                          // not a symlink
    emit_epilogue(emitter);
}

/// x86_64 Linux variant of `emit_stat_ext`. Uses the GNU libc stat()/lstat()/
/// access() calling convention instead of raw syscalls. Frame layout and field
/// offsets differ from the generic ARM64 path because it allocates the stat
/// buffer in C ABI space rather than relying on a fixed syscall buffer layout.
fn emit_stat_ext_linux_x86_64(emitter: &mut Emitter) {
    let frame_size = 144usize;
    let mode_off = 24usize;
    let atime_off = 72usize;
    let ctime_off = 104usize;
    let ino_off = 8usize;
    let uid_off = 28usize;
    let gid_off = 32usize;

    // Reusable prologue/epilogue helpers for the libc-stat-based scalar getters.
    let stat_call = |emitter: &mut Emitter| {
        emitter.instruction("push rbp");                                        // preserve caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish a stable frame base for the stat buffer
        emitter.instruction(&format!("sub rsp, {}", frame_size));               // reserve 16-byte aligned stat buffer
        emitter.instruction("call __rt_cstr");                                  // convert path to null-terminated C string
        emitter.instruction("mov rdi, rax");                                    // first libc stat() argument
        emitter.instruction("lea rsi, [rsp]");                                  // second libc stat() argument: stat buffer
        emitter.instruction("call stat");                                       // libc stat() into the buffer
    };
    let lstat_call = |emitter: &mut Emitter| {
        emitter.instruction("push rbp");                                        // preserve caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish a stable frame base for the stat buffer
        emitter.instruction(&format!("sub rsp, {}", frame_size));               // reserve 16-byte aligned stat buffer
        emitter.instruction("call __rt_cstr");                                  // convert path to null-terminated C string
        emitter.instruction("mov rdi, rax");                                    // first libc lstat() argument
        emitter.instruction("lea rsi, [rsp]");                                  // second libc lstat() argument: stat buffer
        emitter.instruction("call lstat");                                      // libc lstat() into the buffer
    };
    let unwind_zero = |emitter: &mut Emitter| {
        emitter.instruction("xor eax, eax");                                    // failure path returns a zero payload
        emitter.instruction("xor edx, edx");                                    // clear the success flag for int|false callers
        emitter.instruction(&format!("add rsp, {}", frame_size));               // release the stat buffer
        emitter.instruction("pop rbp");                                         // restore caller frame pointer
        emitter.instruction("ret");                                             // return zero result
    };
    let unwind_with_rax = |emitter: &mut Emitter| {
        emitter.instruction(&format!("add rsp, {}", frame_size));               // release the stat buffer
        emitter.instruction("pop rbp");                                         // restore caller frame pointer
        emitter.instruction("ret");                                             // return result already in rax
    };

    // -- fileatime / filectime --
    for (label, off) in [
        ("__rt_fileatime", atime_off),
        ("__rt_filectime", ctime_off),
    ] {
        emitter.blank();
        emitter.comment(&format!("--- runtime: {} ---", &label[5..]));
        emitter.label_global(label);
        stat_call(emitter);
        emitter.instruction("cmp rax, 0");                                      // stat() success?
        emitter.instruction(&format!("jne {}_fail", label));                    // failure → false flag
        emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", off));    // load tv_sec at offset
        emitter.instruction("mov rdx, 1");                                      // success flag for codegen-side int|false boxing
        unwind_with_rax(emitter);
        emitter.label(&format!("{}_fail", label));
        unwind_zero(emitter);
    }

    // -- fileperms --
    emitter.blank();
    emitter.comment("--- runtime: fileperms ---");
    emitter.label_global("__rt_fileperms");
    stat_call(emitter);
    emitter.instruction("cmp rax, 0");                                          // stat() success?
    emitter.instruction("jne __rt_fileperms_fail");                             // failure → false flag
    emitter.instruction(&format!("mov eax, DWORD PTR [rsp + {}]", mode_off));   // load 32-bit st_mode (zero-extends into rax)
    emitter.instruction("mov rdx, 1");                                          // success flag for codegen-side int|false boxing
    unwind_with_rax(emitter);
    emitter.label("__rt_fileperms_fail");
    unwind_zero(emitter);

    // -- fileowner / filegroup --
    for (label, off) in [
        ("__rt_fileowner", uid_off),
        ("__rt_filegroup", gid_off),
    ] {
        emitter.blank();
        emitter.comment(&format!("--- runtime: {} ---", &label[5..]));
        emitter.label_global(label);
        stat_call(emitter);
        emitter.instruction("cmp rax, 0");                                      // stat() success?
        emitter.instruction(&format!("jne {}_fail", label));                    // failure → false flag
        emitter.instruction(&format!("mov eax, DWORD PTR [rsp + {}]", off));    // load 32-bit uid/gid
        emitter.instruction("mov rdx, 1");                                      // success flag for codegen-side int|false boxing
        unwind_with_rax(emitter);
        emitter.label(&format!("{}_fail", label));
        unwind_zero(emitter);
    }

    // -- fileinode --
    emitter.blank();
    emitter.comment("--- runtime: fileinode ---");
    emitter.label_global("__rt_fileinode");
    stat_call(emitter);
    emitter.instruction("cmp rax, 0");                                          // stat() success?
    emitter.instruction("jne __rt_fileinode_fail");                             // failure → false flag
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", ino_off));    // load 64-bit st_ino
    emitter.instruction("mov rdx, 1");                                          // success flag for codegen-side int|false boxing
    unwind_with_rax(emitter);
    emitter.label("__rt_fileinode_fail");
    unwind_zero(emitter);

    // -- filetype --
    emitter.blank();
    emitter.comment("--- runtime: filetype ---");
    emitter.label_global("__rt_filetype");
    lstat_call(emitter);
    emitter.instruction("cmp rax, 0");                                          // lstat() success?
    emitter.instruction("jne __rt_filetype_fail");                              // failure → PHP false
    emitter.instruction(&format!("mov r9d, DWORD PTR [rsp + {}]", mode_off));   // load st_mode
    emitter.instruction("and r9d, 0xF000");                                     // mask with S_IFMT
    emitter.instruction("cmp r9d, 0x8000");                                     // S_IFREG?
    emitter.instruction("je __rt_filetype_file");                               // return "file" for regular files
    emitter.instruction("cmp r9d, 0x4000");                                     // S_IFDIR?
    emitter.instruction("je __rt_filetype_dir");                                // return "dir" for directories
    emitter.instruction("cmp r9d, 0xA000");                                     // S_IFLNK?
    emitter.instruction("je __rt_filetype_link");                               // return "link" for symlinks
    emitter.instruction("cmp r9d, 0x2000");                                     // S_IFCHR?
    emitter.instruction("je __rt_filetype_char");                               // return "char" for character devices
    emitter.instruction("cmp r9d, 0x6000");                                     // S_IFBLK?
    emitter.instruction("je __rt_filetype_block");                              // return "block" for block devices
    emitter.instruction("cmp r9d, 0x1000");                                     // S_IFIFO?
    emitter.instruction("je __rt_filetype_fifo");                               // return "fifo" for named pipes
    emitter.instruction("cmp r9d, 0xC000");                                     // S_IFSOCK?
    emitter.instruction("je __rt_filetype_socket");                             // return "socket" for sockets
    emitter.instruction("jmp __rt_filetype_unknown");                           // fall-through → "unknown"

    let ft_emit = |emitter: &mut Emitter, sym: &str, len: i64| {
        emitter.instruction(&format!("lea rax, [rip + {}]", sym));              // result pointer
        emitter.instruction(&format!("mov rdx, {}", len));                      // result length
        emitter.instruction(&format!("add rsp, {}", frame_size));               // release the stat buffer
        emitter.instruction("pop rbp");                                         // restore caller frame pointer
        emitter.instruction("ret");                                             // return type-name slice
    };
    emitter.label("__rt_filetype_file");
    ft_emit(emitter, "_filetype_file", 4);
    emitter.label("__rt_filetype_dir");
    ft_emit(emitter, "_filetype_dir", 3);
    emitter.label("__rt_filetype_link");
    ft_emit(emitter, "_filetype_link", 4);
    emitter.label("__rt_filetype_char");
    ft_emit(emitter, "_filetype_char", 4);
    emitter.label("__rt_filetype_block");
    ft_emit(emitter, "_filetype_block", 5);
    emitter.label("__rt_filetype_fifo");
    ft_emit(emitter, "_filetype_fifo", 4);
    emitter.label("__rt_filetype_socket");
    ft_emit(emitter, "_filetype_socket", 6);
    emitter.label("__rt_filetype_unknown");
    ft_emit(emitter, "_filetype_unknown", 7);
    emitter.label("__rt_filetype_fail");
    emitter.instruction("xor eax, eax");                                        // null string pointer tells codegen to box PHP false
    emitter.instruction("xor edx, edx");                                        // failure string length is zero
    emitter.instruction(&format!("add rsp, {}", frame_size));                   // release the stat buffer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the failure sentinel

    // -- is_executable --
    emitter.blank();
    emitter.comment("--- runtime: is_executable ---");
    emitter.label_global("__rt_is_executable");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("call __rt_cstr");                                      // null-terminate the path
    emitter.instruction("mov rdi, rax");                                        // first libc access() argument
    emitter.instruction("mov rsi, 1");                                          // X_OK = 1 (execute permission check)
    emitter.instruction("call access");                                         // libc access(path, X_OK)
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen to canonical integer result
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- is_link --
    emitter.blank();
    emitter.comment("--- runtime: is_link ---");
    emitter.label_global("__rt_is_link");
    lstat_call(emitter);
    emitter.instruction("cmp rax, 0");                                          // lstat() success?
    emitter.instruction("jne __rt_is_link_no");                                 // failure → 0
    emitter.instruction(&format!("mov r9d, DWORD PTR [rsp + {}]", mode_off));   // load st_mode
    emitter.instruction("and r9d, 0xF000");                                     // mask with S_IFMT
    emitter.instruction("cmp r9d, 0xA000");                                     // S_IFLNK?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen to canonical integer result
    unwind_with_rax(emitter);
    emitter.label("__rt_is_link_no");
    unwind_zero(emitter);
}
