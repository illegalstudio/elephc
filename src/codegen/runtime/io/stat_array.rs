//! Purpose:
//! Emits the `__rt_stat_array`, `__rt_lstat_array` runtime helper assembly for stat array.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen::{emit::Emitter, platform::Arch};
use crate::codegen::abi;

/// `__rt_stat_array` / `__rt_lstat_array` / `__rt_fstat_array`: build a
/// PHP-compatible associative array describing a path or file descriptor.
///
/// The returned hash carries both numeric (0..=12) and string keys, in the
/// order PHP documents:
///
/// ```text
/// 0/dev, 1/ino, 2/mode, 3/nlink, 4/uid, 5/gid, 6/rdev,
/// 7/size, 8/atime, 9/mtime, 10/ctime, 11/blksize, 12/blocks
/// ```
///
/// All values are inserted as `Int` (tag = 0). On stat failure the runtime
/// returns a null hash pointer so builtin codegen can box PHP `false`.
pub fn emit_stat_array(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stat_array_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let stat_buf = plat.stat_buf_size();
    // Frame layout:
    //   sp + 0 .. stat_buf       : stat buffer (must be at sp+0 for the syscall)
    //   sp + stat_buf            : hash pointer slot (8 bytes)
    //   sp + stat_buf + 8        : (alignment padding)
    //   sp + frame_size - 16     : x29 / x30
    let hash_slot = stat_buf;
    let frame_size = ((stat_buf + 16 + 16) + 15) & !15;
    let save_offset = frame_size - 16;

    // Field descriptor: (numeric_idx, key_symbol, key_len, value_offset_fn,
    //                    load_instr_fn).
    let mode_off = plat.stat_mode_offset();
    let size_off = plat.stat_size_offset();
    let atime_off = plat.stat_atime_offset();
    let mtime_off = plat.stat_mtime_offset();
    let ctime_off = plat.stat_ctime_offset();
    let ino_off = plat.stat_ino_offset();
    let uid_off = plat.stat_uid_offset();
    let gid_off = plat.stat_gid_offset();
    let dev_off = plat.stat_dev_offset();
    let rdev_off = plat.stat_rdev_offset();
    let nlink_off = plat.stat_nlink_offset();
    let blksize_off = plat.stat_blksize_offset();
    let blocks_off = plat.stat_blocks_offset();

    // Each entry: (idx, name_sym, name_len, ARM64 load instruction loading the
    // value into x9 from [sp, #off]).
    let entries: Vec<(i64, &str, i64, String)> = vec![
        (0,  "_stat_key_dev",     3, plat.stat_dev_load_instr("x9", "sp", dev_off)),
        (1,  "_stat_key_ino",     3, format!("ldr x9, [sp, #{}]", ino_off)),
        (2,  "_stat_key_mode",    4, plat.stat_mode_load_instr("w9", "sp", mode_off)),
        (3,  "_stat_key_nlink",   5, plat.stat_nlink_load_instr("w9", "sp", nlink_off)),
        (4,  "_stat_key_uid",     3, format!("ldr w9, [sp, #{}]", uid_off)),
        (5,  "_stat_key_gid",     3, format!("ldr w9, [sp, #{}]", gid_off)),
        (6,  "_stat_key_rdev",    4, plat.stat_rdev_load_instr("x9", "sp", rdev_off)),
        (7,  "_stat_key_size",    4, format!("ldr x9, [sp, #{}]", size_off)),
        (8,  "_stat_key_atime",   5, format!("ldr x9, [sp, #{}]", atime_off)),
        (9,  "_stat_key_mtime",   5, format!("ldr x9, [sp, #{}]", mtime_off)),
        (10, "_stat_key_ctime",   5, format!("ldr x9, [sp, #{}]", ctime_off)),
        (11, "_stat_key_blksize", 7, format!("ldr w9, [sp, #{}]", blksize_off)),
        (12, "_stat_key_blocks",  6, format!("ldr x9, [sp, #{}]", blocks_off)),
    ];

    // Helper that emits the body shared by the three entry points: assumes the
    // stat buffer is already populated, then fills the hash. Saves/restores
    // the hash pointer at [sp, #hash_slot].
    let emit_build_hash = |emitter: &mut Emitter| {
        // -- create hash table --
        emitter.instruction("mov x0, #32");                                     // capacity = 32 (room for 13 fields with low load factor)
        emitter.instruction("mov x1, #0");                                      // value type = Int
        emitter.instruction("bl __rt_hash_new");                                // returns hash pointer in x0
        emitter.instruction(&format!("str x0, [sp, #{}]", hash_slot));          // save hash pointer

        for (idx, key_sym, key_len, load_instr) in &entries {
            // numeric key insertion
            emitter.instruction(&format!("ldr x0, [sp, #{}]", hash_slot));      // reload hash pointer
            emitter.instruction(&format!("mov x1, #{}", idx));                  // key_lo = numeric key value
            emitter.instruction("mov x2, #-1");                                 // key_hi = -1 (integer-key marker)
            emitter.instruction(load_instr);                                    // load value from stat buffer
            emitter.instruction("mov x3, x9");                                  // value_lo = loaded value
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = Int
            emitter.instruction("bl __rt_hash_set");                            // insert entry; x0 = updated hash
            emitter.instruction(&format!("str x0, [sp, #{}]", hash_slot));      // persist updated hash pointer

            // string key insertion
            emitter.instruction(&format!("ldr x0, [sp, #{}]", hash_slot));      // reload hash pointer
            abi::emit_symbol_address(emitter, "x1", key_sym);                   // load page of the key literal
            emitter.instruction(&format!("mov x2, #{}", key_len));              // key length
            emitter.instruction(load_instr);                                    // re-load value (registers were clobbered by the prior call)
            emitter.instruction("mov x3, x9");                                  // value_lo
            emitter.instruction("mov x4, #0");                                  // value_hi
            emitter.instruction("mov x5, #0");                                  // value tag = Int
            emitter.instruction("bl __rt_hash_set");                            // insert entry
            emitter.instruction(&format!("str x0, [sp, #{}]", hash_slot));      // persist updated hash pointer
        }

        emitter.instruction(&format!("ldr x0, [sp, #{}]", hash_slot));          // load final hash pointer into result register
    };

    // -------------- __rt_stat_array (path-based, follows symlinks) ----------
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: stat (associative array) ---");
    emitter.label_global("__rt_stat_array");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_offset));      // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_offset));             // establish new frame pointer
    emitter.instruction("bl __rt_cstr");                                        // null-terminate the path
    emitter.instruction("add x1, sp, #0");                                      // pointer to stat buffer
    emitter.syscall(338);                                                       // stat(path, buf)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("b.ne __rt_stat_array_fail");                           // failure → null hash pointer
    emit_build_hash(emitter);
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame
    emitter.instruction("ret");                                                 // return hash in x0
    emitter.label("__rt_stat_array_fail");
    emitter.instruction("mov x0, #0");                                          // null hash pointer tells codegen to box PHP false
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address (failure path)
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame (failure path)
    emitter.instruction("ret");                                                 // return the failure sentinel

    // -------------- __rt_lstat_array (path-based, does not follow symlinks) -
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: lstat (associative array) ---");
    emitter.label_global("__rt_lstat_array");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_offset));      // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_offset));             // establish new frame pointer
    emitter.instruction("bl __rt_cstr");                                        // null-terminate the path
    emitter.instruction("add x1, sp, #0");                                      // pointer to stat buffer
    emitter.syscall(340);                                                       // lstat(path, buf)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("b.ne __rt_lstat_array_fail");                          // failure → null hash pointer
    emit_build_hash(emitter);
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame
    emitter.instruction("ret");                                                 // return hash in x0
    emitter.label("__rt_lstat_array_fail");
    emitter.instruction("mov x0, #0");                                          // null hash pointer tells codegen to box PHP false
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address (failure path)
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame (failure path)
    emitter.instruction("ret");                                                 // return the failure sentinel

    // -------------- __rt_fstat_array (fd-based, no path conversion) ---------
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: fstat (associative array) ---");
    emitter.label_global("__rt_fstat_array");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_offset));      // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_offset));             // establish new frame pointer
    // Caller passes fd as an integer in x0 (standard PHP int return register).
    // The fstat syscall expects fd in x0 and stat buf in x1.
    emitter.instruction("add x1, sp, #0");                                      // pointer to stat buffer
    emitter.syscall(339);                                                       // fstat(fd, buf)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("b.ne __rt_fstat_array_fail");                          // failure → null hash pointer
    emit_build_hash(emitter);
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame
    emitter.instruction("ret");                                                 // return hash in x0
    emitter.label("__rt_fstat_array_fail");
    emitter.instruction("mov x0, #0");                                          // null hash pointer tells codegen to box PHP false
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address (failure path)
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame (failure path)
    emitter.instruction("ret");                                                 // return the failure sentinel
}

/// Emits `__rt_stat_array`, `__rt_lstat_array`, and `__rt_fstat_array` for the Linux x86_64 ABI.
///
/// Uses the libc `stat`, `lstat`, and `fstat` calls rather than raw syscalls, unlike the ARM64
/// path. Frame layout (rbp-relative): hash pointer slot at `[rbp - 8]`, stat buffer at
/// `[rbp - buf_neg]`. On failure each helper returns a null hash pointer (rax = 0) so builtin
/// codegen can box PHP `false`. Called exclusively from `emit_stat_array` when
/// `emitter.target.arch == Arch::X86_64`.
fn emit_stat_array_linux_x86_64(emitter: &mut Emitter) {
    let stat_buf = 144usize;
    let mode_off = 24usize;
    let size_off = 48usize;
    let atime_off = 72usize;
    let mtime_off = 88usize;
    let ctime_off = 104usize;
    let ino_off = 8usize;
    let uid_off = 28usize;
    let gid_off = 32usize;
    let dev_off = 0usize;
    let rdev_off = 40usize;
    let nlink_off = 16usize;
    let blksize_off = 56usize;
    let blocks_off = 64usize;

    // Frame layout (rbp-relative):
    //   [rbp - 8]              : hash pointer slot
    //   [rbp - stat_buf - 8]   : start of stat buffer (rbp-relative, so the
    //                            buffer is below the hash slot)
    //                            Actually we keep the buffer at [rsp .. rsp+stat_buf]
    //                            and hash slot at [rbp - 8]. Total reserved:
    //                            stat_buf + 8 + 8 (alignment) = stat_buf + 16.
    let frame = ((stat_buf + 16) + 15) & !15;
    let hash_slot_neg = 8usize; // [rbp - 8]
    let buf_neg = (stat_buf + hash_slot_neg) as i64; // [rbp - buf_neg]

    let entries: Vec<(i64, &str, i64, String)> = vec![
        (0,  "_stat_key_dev",     3, format!("mov rax, QWORD PTR [rbp - {}]", buf_neg - dev_off as i64)),
        (1,  "_stat_key_ino",     3, format!("mov rax, QWORD PTR [rbp - {}]", buf_neg - ino_off as i64)),
        (2,  "_stat_key_mode",    4, format!("mov eax, DWORD PTR [rbp - {}]", buf_neg - mode_off as i64)),
        (3,  "_stat_key_nlink",   5, format!("mov rax, QWORD PTR [rbp - {}]", buf_neg - nlink_off as i64)),
        (4,  "_stat_key_uid",     3, format!("mov eax, DWORD PTR [rbp - {}]", buf_neg - uid_off as i64)),
        (5,  "_stat_key_gid",     3, format!("mov eax, DWORD PTR [rbp - {}]", buf_neg - gid_off as i64)),
        (6,  "_stat_key_rdev",    4, format!("mov rax, QWORD PTR [rbp - {}]", buf_neg - rdev_off as i64)),
        (7,  "_stat_key_size",    4, format!("mov rax, QWORD PTR [rbp - {}]", buf_neg - size_off as i64)),
        (8,  "_stat_key_atime",   5, format!("mov rax, QWORD PTR [rbp - {}]", buf_neg - atime_off as i64)),
        (9,  "_stat_key_mtime",   5, format!("mov rax, QWORD PTR [rbp - {}]", buf_neg - mtime_off as i64)),
        (10, "_stat_key_ctime",   5, format!("mov rax, QWORD PTR [rbp - {}]", buf_neg - ctime_off as i64)),
        (11, "_stat_key_blksize", 7, format!("mov rax, QWORD PTR [rbp - {}]", buf_neg - blksize_off as i64)),
        (12, "_stat_key_blocks",  6, format!("mov rax, QWORD PTR [rbp - {}]", buf_neg - blocks_off as i64)),
    ];

    let emit_build_hash = |emitter: &mut Emitter, entries: &[(i64, &str, i64, String)]| {
        emitter.instruction("mov rdi, 32");                                     // first hash_new argument: capacity
        emitter.instruction("mov rsi, 0");                                      // second hash_new argument: value type = Int
        emitter.instruction("call __rt_hash_new");                              // returns hash pointer in rax
        emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", hash_slot_neg)); // save hash pointer
        for (idx, key_sym, key_len, load_instr) in entries {
            // numeric key
            emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", hash_slot_neg)); // hash pointer
            emitter.instruction(&format!("mov rsi, {}", idx));                  // key_lo = numeric key
            emitter.instruction("mov rdx, -1");                                 // key_hi = -1 (integer marker)
            emitter.instruction(load_instr);                                    // load value into rax
            emitter.instruction("mov rcx, rax");                                // value_lo
            emitter.instruction("mov r8, 0");                                   // value_hi
            emitter.instruction("mov r9, 0");                                   // value tag = Int
            emitter.instruction("call __rt_hash_set");                          // insert
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", hash_slot_neg)); // persist hash

            // string key
            emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", hash_slot_neg)); // hash pointer
            abi::emit_symbol_address(emitter, "rsi", key_sym);                  // key pointer
            emitter.instruction(&format!("mov rdx, {}", key_len));              // key length
            emitter.instruction(load_instr);                                    // reload value
            emitter.instruction("mov rcx, rax");                                // value_lo
            emitter.instruction("mov r8, 0");                                   // value_hi
            emitter.instruction("mov r9, 0");                                   // value tag = Int
            emitter.instruction("call __rt_hash_set");                          // insert
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", hash_slot_neg)); // persist hash
        }
        emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", hash_slot_neg)); // result in rax
    };

    // -- stat --
    emitter.blank();
    emitter.comment("--- runtime: stat (associative array) ---");
    emitter.label_global("__rt_stat_array");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction(&format!("sub rsp, {}", frame));                        // reserve stat buffer + hash slot
    emitter.instruction("call __rt_cstr");                                      // path → C string in rax
    emitter.instruction("mov rdi, rax");                                        // first libc stat() argument
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_neg));              // second libc stat() argument: stat buffer
    emitter.instruction("call stat");                                           // libc stat()
    emitter.instruction("cmp eax, 0");                                          // did libc stat() return success as a C int?
    emitter.instruction("jne __rt_stat_array_fail_x86");                        // failure → null hash pointer
    emit_build_hash(emitter, &entries);
    emitter.instruction(&format!("add rsp, {}", frame));                        // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return hash in rax
    emitter.label("__rt_stat_array_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null hash pointer tells codegen to box PHP false
    emitter.instruction(&format!("add rsp, {}", frame));                        // release frame (failure path)
    emitter.instruction("pop rbp");                                             // restore caller frame pointer (failure path)
    emitter.instruction("ret");                                                 // return the failure sentinel

    // -- lstat --
    emitter.blank();
    emitter.comment("--- runtime: lstat (associative array) ---");
    emitter.label_global("__rt_lstat_array");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction(&format!("sub rsp, {}", frame));                        // reserve stat buffer + hash slot
    emitter.instruction("call __rt_cstr");                                      // path → C string in rax
    emitter.instruction("mov rdi, rax");                                        // first libc lstat() argument
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_neg));              // second libc lstat() argument
    emitter.instruction("call lstat");                                          // libc lstat()
    emitter.instruction("cmp eax, 0");                                          // did libc lstat() return success as a C int?
    emitter.instruction("jne __rt_lstat_array_fail_x86");                       // failure → null hash pointer
    emit_build_hash(emitter, &entries);
    emitter.instruction(&format!("add rsp, {}", frame));                        // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return hash in rax
    emitter.label("__rt_lstat_array_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null hash pointer tells codegen to box PHP false
    emitter.instruction(&format!("add rsp, {}", frame));                        // release frame (failure path)
    emitter.instruction("pop rbp");                                             // restore caller frame pointer (failure path)
    emitter.instruction("ret");                                                 // return the failure sentinel

    // -- fstat --
    emitter.blank();
    emitter.comment("--- runtime: fstat (associative array) ---");
    emitter.label_global("__rt_fstat_array");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction(&format!("sub rsp, {}", frame));                        // reserve stat buffer + hash slot
    // Caller passes fd in rax (integer return register).
    emitter.instruction("mov rdi, rax");                                        // fd → first libc fstat() argument
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_neg));              // second libc fstat() argument
    emitter.instruction("call fstat");                                          // libc fstat()
    emitter.instruction("cmp eax, 0");                                          // did libc fstat() return success as a C int?
    emitter.instruction("jne __rt_fstat_array_fail_x86");                       // failure → null hash pointer
    emit_build_hash(emitter, &entries);
    emitter.instruction(&format!("add rsp, {}", frame));                        // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return hash in rax
    emitter.label("__rt_fstat_array_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null hash pointer tells codegen to box PHP false
    emitter.instruction(&format!("add rsp, {}", frame));                        // release frame (failure path)
    emitter.instruction("pop rbp");                                             // restore caller frame pointer (failure path)
    emitter.instruction("ret");                                                 // return the failure sentinel
}
