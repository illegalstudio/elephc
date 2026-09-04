//! Purpose:
//! Emits the `__rt_unlink`, `__rt_cstr` runtime helper assembly for fs.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::abi;
use crate::codegen_support::runtime::data::{
    COPY_SOURCE_IS_DIR_MSG, MKDIR_WARNING_HEAD, PATH_WARNING_MIDDLE, RMDIR_WARNING_HEAD,
    UNLINK_WARNING_HEAD,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits all filesystem runtime helpers: `__rt_unlink`, `__rt_mkdir`, `__rt_rmdir`,
/// `__rt_chdir`, `__rt_rename`, and `__rt_copy`.
///
/// Dispatches to `emit_fs_linux_x86_64` on x86_64 Linux; emits ARM64 syscall-based
/// helpers on all other targets. Each helper takes PHP string path arguments (x1=ptr,
/// x2=len) and returns x0=1 on success, 0 on failure.
pub fn emit_fs(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fs_linux_x86_64(emitter);
        return;
    }

    // ================================================================
    // __rt_unlink: delete a file
    // Input:  x1/x2=path
    // Output: x0=1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: unlink ---");
    emitter.label_global("__rt_unlink");
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(0),
        super::fopen::DisabledWrapperNotice::Fixed {
            symbol: "_diag_no_wrapper_unlink",
            len: super::fopen::unable_to_locate_wrapper_message("unlink").len(),
        },
    );

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // linkage plus the path the warning names
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- null-terminate path and call unlink --
    emitter.instruction("bl __rt_path_cstr");                                   // convert path to C string, x0=cstr
    emitter.instruction("str x0, [sp, #16]");                                   // the path the warning below names
    emitter.syscall(10);

    // -- php says WHY; elephc said nothing at all --
    // `unlink("nope.txt")` prints `Warning: unlink(nope.txt): No such file or directory` in php
    // and printed not a byte here, so the only thing a caller could learn was `false`.
    emitter.instruction("cmp x0, #0");                                          // zero is success on both platforms
    emitter.instruction("b.eq __rt_unlink_ok");
    super::path_op_warning::emit_call_aarch64(
        emitter,
        "_warn_unlink_head",
        UNLINK_WARNING_HEAD.len(),
        Some("[sp, #16]"),
        "_warn_path_mid",
        PATH_WARNING_MIDDLE.len(),
    );
    emitter.instruction("mov x0, #0");                                          // php answers false for the failure it just named
    emitter.instruction("b __rt_unlink_done");
    emitter.label("__rt_unlink_ok");
    emitter.instruction("mov x0, #1");
    emitter.label("__rt_unlink_done");

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // ================================================================
    // __rt_mkdir: create a directory
    // Input:  x1/x2=path, x3=mode, x4=recursive
    // Output: x0=1 on success, 0 on failure
    //
    // The mode was hard-coded to 0755 and `$recursive` had nowhere to arrive,
    // because the contract stopped at one parameter. With `$recursive` set the
    // helper walks the C string and creates each parent in turn: `_cstr_buf` is
    // our own scratch, so a separator can be overwritten with a terminator to
    // name the prefix and put back afterwards, with no second buffer. Parent
    // failures are ignored on purpose — the usual one is EEXIST, and only the
    // LEAF decides the return value, which is what php reports.
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: mkdir ---");
    emitter.label_global("__rt_mkdir");
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled(emitter, super::fopen::DisabledWrapperAnswer::Predicate(0));

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate frame for mode, flag, buffer and scan index
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer
    emitter.instruction("str x3, [sp, #16]");                                   // save the requested mode across __rt_cstr
    emitter.instruction("str x4, [sp, #24]");                                   // save the recursive flag across __rt_cstr

    // -- null-terminate the path into our own scratch buffer --
    emitter.instruction("bl __rt_path_cstr");                                   // convert path to C string, x0=cstr
    emitter.instruction("str x0, [sp, #32]");                                   // save the buffer base for both routes
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the recursive flag
    emitter.instruction("cbz x4, __rt_mkdir_leaf");                             // not recursive: create only the named directory

    // -- recursive: create every parent component in turn --
    emitter.instruction("mov x10, #0");                                         // scan index into the C string
    emitter.label("__rt_mkdir_walk");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the buffer base
    emitter.instruction("ldrb w11, [x9, x10]");                                 // load the byte at the scan index
    emitter.instruction("cbz w11, __rt_mkdir_leaf");                            // terminator reached: only the leaf is left
    emitter.instruction("cmp w11, #47");                                        // is this byte a '/' separator?
    emitter.instruction("b.ne __rt_mkdir_walk_next");                           // ordinary byte — keep scanning
    emitter.instruction("cbz x10, __rt_mkdir_walk_next");                       // a leading '/' names the root, which always exists
    emitter.instruction("strb wzr, [x9, x10]");                                 // terminate here so the buffer names the parent
    emitter.instruction("mov x0, x9");                                          // pass the parent path to mkdir
    emitter.instruction("ldr x1, [sp, #16]");                                   // parents are created with the requested mode
    emitter.instruction("str x10, [sp, #40]");                                  // preserve the scan index across the syscall
    emitter.syscall(136);
    emitter.instruction("ldr x10, [sp, #40]");                                  // restore the scan index
    emitter.instruction("ldr x9, [sp, #32]");                                   // restore the buffer base
    emitter.instruction("mov w11, #47");                                        // the separator byte we overwrote
    emitter.instruction("strb w11, [x9, x10]");                                 // put the separator back before scanning on
    emitter.label("__rt_mkdir_walk_next");
    emitter.instruction("add x10, x10, #1");                                    // advance the scan index
    emitter.instruction("b __rt_mkdir_walk");                                   // continue walking the path

    // -- create the named directory; its result is the answer --
    emitter.label("__rt_mkdir_leaf");
    emitter.instruction("ldr x0, [sp, #32]");                                   // the full path
    emitter.instruction("ldr x1, [sp, #16]");                                   // the requested mode
    emitter.syscall(136);

    // -- php names no path in this one, and elephc named nothing at all --
    // MEASURED: `Warning: mkdir(): File exists`, with the parentheses EMPTY even though a path
    // was passed. The composer takes a null path for exactly this shape.
    emitter.instruction("cmp x0, #0");                                          // zero is success on both platforms
    emitter.instruction("b.eq __rt_mkdir_ok");
    super::path_op_warning::emit_call_aarch64(
        emitter,
        "_warn_mkdir_head",
        MKDIR_WARNING_HEAD.len(),
        None,
        "_warn_mkdir_head",
        0,
    );
    emitter.instruction("mov x0, #0");                                          // php answers false for the failure it just named
    emitter.instruction("b __rt_mkdir_done");
    emitter.label("__rt_mkdir_ok");
    emitter.instruction("mov x0, #1");
    emitter.label("__rt_mkdir_done");

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // ================================================================
    // __rt_rmdir: remove a directory
    // Input:  x1/x2=path
    // Output: x0=1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: rmdir ---");
    emitter.label_global("__rt_rmdir");
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled(emitter, super::fopen::DisabledWrapperAnswer::Predicate(0));

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // linkage plus the path the warning names
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- null-terminate path and call rmdir --
    emitter.instruction("bl __rt_path_cstr");                                   // convert path to C string, x0=cstr
    emitter.instruction("str x0, [sp, #16]");                                   // the path the warning below names
    emitter.syscall(137);

    // -- see `unlink()`: php says WHY, and this said nothing --
    emitter.instruction("cmp x0, #0");                                          // zero is success on both platforms
    emitter.instruction("b.eq __rt_rmdir_ok");
    super::path_op_warning::emit_call_aarch64(
        emitter,
        "_warn_rmdir_head",
        RMDIR_WARNING_HEAD.len(),
        Some("[sp, #16]"),
        "_warn_path_mid",
        PATH_WARNING_MIDDLE.len(),
    );
    emitter.instruction("mov x0, #0");                                          // php answers false for the failure it just named
    emitter.instruction("b __rt_rmdir_done");
    emitter.label("__rt_rmdir_ok");
    emitter.instruction("mov x0, #1");
    emitter.label("__rt_rmdir_done");

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // ================================================================
    // __rt_chdir: change working directory
    // Input:  x1/x2=path
    // Output: x0=1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: chdir ---");
    emitter.label_global("__rt_chdir");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- null-terminate path and call chdir --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr
    emitter.syscall(12);

    // -- return success/failure --
    emitter.instruction("cmp x0, #0");                                          // check syscall result
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if chdir succeeded

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // ================================================================
    // __rt_rename: rename a file or directory
    // Input:  x1/x2=from path, x3/x4=to path
    // Output: x0=1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: rename ---");
    emitter.label_global("__rt_rename");
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(0),
        super::fopen::DisabledWrapperNotice::Fixed {
            symbol: "_diag_no_wrapper_rename",
            len: super::fopen::unable_to_locate_wrapper_message("rename").len(),
        },
    );

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save destination path before clobbering registers --
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save 'to' path ptr and len on stack

    // -- null-terminate source path using primary buffer --
    emitter.instruction("bl __rt_path_cstr");                                   // convert 'from' to C string in _cstr_buf
    emitter.instruction("str x0, [sp, #0]");                                    // save source cstr pointer

    // -- null-terminate destination path using secondary buffer --
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload 'to' path ptr and len
    emitter.instruction("bl __rt_path_cstr2");                                  // convert 'to' to C string in _cstr_buf2
    emitter.instruction("str x0, [sp, #8]");                                    // save destination cstr pointer

    // -- call rename syscall --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source cstr path
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload destination cstr path
    emitter.syscall(128);

    // -- the only shape that names TWO paths --
    // MEASURED: `Warning: rename(nope.txt,other.txt): No such file or directory` — comma, no
    // space, and elephc printed nothing at all.
    emitter.instruction("cmp x0, #0");                                          // zero is success on both platforms
    emitter.instruction("b.eq __rt_rename_ok");
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("neg x2, x0");                                      // Linux answers -errno
    } else {
        emitter.instruction("mov x2, x0");                                      // macOS answers the errno itself
    }
    emitter.instruction("ldr x0, [sp, #0]");                                    // the source path
    emitter.instruction("ldr x1, [sp, #8]");                                    // and the destination
    emitter.instruction("bl __rt_rename_warning");
    emitter.instruction("mov x0, #0");                                          // php answers false for the failure it just named
    emitter.instruction("b __rt_rename_done");
    emitter.label("__rt_rename_ok");
    emitter.instruction("mov x0, #1");
    emitter.label("__rt_rename_done");

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // ================================================================
    // __rt_copy: copy a file
    // Input:  x1/x2=from path, x3/x4=to path
    // Output: x0=1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: copy ---");
    emitter.label_global("__rt_copy");
    // php locates a wrapper for every path, and the one it names is the SOURCE. Guarded HERE
    // rather than in the helper copy() reads through, which would name that helper and its
    // DESTINATION path — both wrong.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(0),
        super::fopen::DisabledWrapperNotice::FailedToOpen {
            name_symbol: "_uww_name_copy",
            name_len: 4,
            directory: false,
        },
    );

    // -- set up stack frame --
    // The two stat buffers sit ABOVE the original 48 bytes, so every offset below still reads
    // what it always did.
    let stat_buf = emitter.platform.stat_buf_size(emitter.target.arch);
    let copy_frame = 48 + ((2 * stat_buf + 15) & !15);
    let src_stat = 48;
    let dst_stat = 48 + stat_buf;
    let ino_off = emitter.platform.stat_ino_offset();
    let dev_off = emitter.platform.stat_dev_offset();
    let mode_off = emitter.platform.stat_mode_offset(emitter.target.arch);
    emitter.instruction(&format!("sub sp, sp, #{}", copy_frame));               // allocate the frame plus two stat buffers
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save both paths: the same-file test below needs each of them in turn --
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save 'from' path ptr and len on stack
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save 'to' path ptr and len on stack

    // -- EITHER end may be served by a registered userspace wrapper --
    //
    // Everything below this point is paths and syscalls, which cannot reach one: MEASURED,
    // `copy("cc://a", "out.txt")` warned `Failed to open stream` and answered false where php
    // copies and answers true. The probe is done here rather than in the lowering because a
    // wrapper is registered at RUN time, so a literal path is not what decides it.
    emitter.instruction("ldp x0, x1, [sp, #0]");                                // the source path
    emitter.instruction("bl __rt_path_is_wrapper");
    emitter.instruction("cbnz x0, __rt_copy_via_wrapper");
    emitter.instruction("ldp x0, x1, [sp, #16]");                               // the destination path
    emitter.instruction("bl __rt_path_is_wrapper");
    emitter.instruction("cbnz x0, __rt_copy_via_wrapper");
    // The probe CLOBBERS x0/x1, and `__rt_path_cstr` below reads the source path from x1/x2.
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore the source path for the stat that follows

    // php refuses to copy a file onto ITSELF, and decides that by (st_dev, st_ino) rather than by
    // comparing the paths: a hard link and a symlink to the source are refused too, and `./x` is
    // refused for `x`. It answers false, says nothing, and leaves the file alone. A destination
    // that does not exist yet cannot be stat'ed, which is what lets an ordinary copy through.
    emitter.instruction("bl __rt_path_cstr");                                   // the source path as a C string
    emitter.instruction(&format!("add x1, sp, #{}", src_stat));                 // fill the source stat buffer
    emitter.syscall(338);
    emitter.instruction("cbnz x0, __rt_copy_not_same_file");                    // no source to stat: the read below reports it
    // php refuses a DIRECTORY source before it opens anything, with a sentence of its own, and
    // never touches the destination. Without the check the read failed instead — and on macOS a
    // failed `read(2)` answers the errno in the result register, so `EISDIR` became a 21-byte
    // string of uninitialised heap that `copy()` wrote out and called a success.
    emitter.instruction(&format!("ldr w9, [sp, #{}]", src_stat + mode_off));    // st_mode
    emitter.instruction("and w9, w9, #0xF000");                                 // isolate the S_IFMT bits
    emitter.instruction("mov w10, #0x4000");                                    // S_IFDIR
    emitter.instruction("cmp w9, w10");
    emitter.instruction("b.ne __rt_copy_source_not_dir");
    abi::emit_symbol_address(emitter, "x1", "_copy_source_is_dir_msg");
    emitter.instruction(&format!("mov x2, #{}", COPY_SOURCE_IS_DIR_MSG.len()));
    emitter.instruction("bl __rt_diag_warning");                                // honours @ like every other php warning
    emitter.instruction("mov x0, #0");                                          // php answers false and copies nothing
    emitter.instruction("b __rt_copy_return");
    emitter.label("__rt_copy_source_not_dir");
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // the destination path
    emitter.instruction("bl __rt_path_cstr");
    emitter.instruction(&format!("add x1, sp, #{}", dst_stat));                 // fill the destination stat buffer
    emitter.syscall(338);
    emitter.instruction("cbnz x0, __rt_copy_not_same_file");                    // nothing there yet: an ordinary copy
    emitter.instruction(&format!("ldr x9, [sp, #{}]", src_stat + ino_off));
    emitter.instruction(&format!("ldr x10, [sp, #{}]", dst_stat + ino_off));
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.ne __rt_copy_not_same_file");                        // different inode: different file
    // Darwin stores st_dev as a signed 32-bit int, so only a word of it is the device.
    let dev_reg = if emitter.platform.stat_dev_is_narrow() { ("w9", "w10") } else { ("x9", "x10") };
    emitter.instruction(&format!("ldr {}, [sp, #{}]", dev_reg.0, src_stat + dev_off));
    emitter.instruction(&format!("ldr {}, [sp, #{}]", dev_reg.1, dst_stat + dev_off));
    emitter.instruction(&format!("cmp {}, {}", dev_reg.0, dev_reg.1));
    emitter.instruction("b.ne __rt_copy_not_same_file");                        // same inode on another device
    emitter.instruction("mov x0, #0");                                          // php answers false and copies nothing
    emitter.instruction("b __rt_copy_return");
    emitter.label("__rt_copy_not_same_file");

    // -- read source file contents --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // the source path the stats consumed
    emitter.instruction("bl __rt_file_get_contents");                           // read source, x1=data ptr, x2=data len

    // php opens the SOURCE first and never touches the destination when that open fails, so a
    // failed copy leaves an existing file alone. Writing the nothing that came back truncated it.
    // A NULL pointer is the failure; an EMPTY file comes back as a real pointer of length zero,
    // which is the same distinction `file_get_contents()` uses to tell `false` from `""`.
    emitter.instruction("cbz x1, __rt_copy_source_failed");

    // -- write contents to destination file --
    emitter.instruction("mov x3, x1");                                          // move data ptr to x3 (data arg)
    emitter.instruction("mov x4, x2");                                          // move data len to x4 (data arg)
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload destination path ptr and len
    emitter.instruction("mov x5, xzr");                                         // the writer reads $flags for FILE_APPEND: this caller has none
    emitter.instruction("bl __rt_file_put_contents");                           // write data to dest file, x0=bytes written

    // A zero-byte write is a SUCCESS: php copies an empty file and answers true. This asked for
    // more than zero, so an empty source answered false here and true on x86_64 — the two arms
    // disagreed about the same program.
    emitter.instruction("cmp x0, #0");                                          // check the byte count the write reported
    emitter.instruction("cset x0, ge");                                         // any non-negative count is success
    emitter.instruction("b __rt_copy_return");
    emitter.label("__rt_copy_source_failed");
    emitter.instruction("mov x0, #0");                                          // php answers false and leaves the destination alone
    emitter.label("__rt_copy_return");

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", copy_frame));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- hand the whole copy to the stream route, with this frame already gone --
    emitter.label("__rt_copy_via_wrapper");
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // the source path, in the shape the route takes
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // and the destination
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", copy_frame));               // deallocate stack frame
    emitter.instruction("b __rt_copy_wrapper");                                 // tail call: its answer is this one's
}

/// Emits x86_64 Linux variants of all filesystem helpers using libc calls.
/// Uses a stack-based frame (rbp/rsp convention) instead of the ARM64 link-register frame.
/// Return value convention matches `emit_fs`: x0=1 on success, 0 on failure.
fn emit_fs_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unlink ---");
    emitter.label_global("__rt_unlink");
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(0),
        super::fopen::DisabledWrapperNotice::Fixed {
            symbol: "_diag_no_wrapper_unlink",
            len: super::fopen::unable_to_locate_wrapper_message("unlink").len(),
        },
    );
    emit_single_path_libc_bool_helper_with(
        emitter,
        "unlink",
        None,
        PathUrls::Honoured,
        Some(("_warn_unlink_head", UNLINK_WARNING_HEAD.len())),
    );

    emitter.blank();
    emitter.comment("--- runtime: mkdir ---");
    emitter.label_global("__rt_mkdir");
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled(emitter, super::fopen::DisabledWrapperAnswer::Predicate(0));
    emit_mkdir_libc_helper(emitter);

    emitter.blank();
    emitter.comment("--- runtime: rmdir ---");
    emitter.label_global("__rt_rmdir");
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled(emitter, super::fopen::DisabledWrapperAnswer::Predicate(0));
    emit_single_path_libc_bool_helper_with(
        emitter,
        "rmdir",
        None,
        PathUrls::Honoured,
        Some(("_warn_rmdir_head", RMDIR_WARNING_HEAD.len())),
    );

    emitter.blank();
    emitter.comment("--- runtime: chdir ---");
    emitter.label_global("__rt_chdir");
    emit_single_path_libc_bool_helper_with(emitter, "chdir", None, PathUrls::Verbatim, None);

    emitter.blank();
    emitter.comment("--- runtime: rename ---");
    emitter.label_global("__rt_rename");
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(0),
        super::fopen::DisabledWrapperNotice::Fixed {
            symbol: "_diag_no_wrapper_rename",
            len: super::fopen::unable_to_locate_wrapper_message("rename").len(),
        },
    );
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while rename uses temporary path slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source and destination path temporaries
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack space for the saved destination and source C-string pointers
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the destination elephc path pointer while converting the source path
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the destination elephc path length while converting the source path
    emitter.instruction("call __rt_path_cstr");                                 // convert the source elephc path in rax/rdx into a null-terminated C string
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the source C-string pointer for the later libc rename() call
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination elephc path pointer before converting it to a C string
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the destination elephc path length before converting it to a C string
    emitter.instruction("call __rt_path_cstr2");                                // convert the destination elephc path into the secondary null-terminated C string buffer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the destination C-string pointer for the later libc rename() call
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the source C-string pointer as the first libc rename() argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the destination C-string pointer as the second libc rename() argument
    emitter.instruction("call rename");                                         // rename or move the file-system path through libc rename()
    // See the AArch64 counterpart: the only shape that names TWO paths, comma-separated.
    emitter.instruction("cmp eax, 0");                                          // a successful libc rename() call returns zero as a C int
    emitter.instruction("je __rt_rename_ok_x86");
    emitter.bl_c(match emitter.platform {
        crate::codegen_support::platform::Platform::MacOS => "__error",
        crate::codegen_support::platform::Platform::Linux => "__errno_location",
        crate::codegen_support::platform::Platform::Windows => {
            panic!("Windows target is not yet supported (see issue #379)")
        }
    });                                                                         // rax = &errno for this thread
    emitter.instruction("movsxd rdx, DWORD PTR [rax]");                         // the reason libc recorded
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // the source path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // and the destination
    emitter.instruction("call __rt_rename_warning");
    emitter.instruction("xor eax, eax");                                        // php answers false for the failure it just named
    emitter.instruction("jmp __rt_rename_done_x86");
    emitter.label("__rt_rename_ok_x86");
    emitter.instruction("mov eax, 1");
    emitter.label("__rt_rename_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the aligned stack locals used by rename()
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the rename() success predicate to the caller

    emitter.blank();
    emitter.comment("--- runtime: copy ---");
    emitter.label_global("__rt_copy");
    // php locates a wrapper for every path, and the one it names is the SOURCE. Guarded HERE
    // rather than in the helper copy() reads through, which would name that helper and its
    // DESTINATION path — both wrong.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(0),
        super::fopen::DisabledWrapperNotice::FailedToOpen {
            name_symbol: "_uww_name_copy",
            name_len: 4,
            directory: false,
        },
    );
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while copy() uses path and payload spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved destination path and copied file payload
    // The two stat buffers sit BELOW the original four slots, so every offset below still reads
    // what it always did.
    let stat_buf = emitter.platform.stat_buf_size(emitter.target.arch);
    let copy_frame = 48 + ((2 * stat_buf + 15) & !15);
    let src_stat = 48 + stat_buf;
    let dst_stat = 48 + 2 * stat_buf;
    let ino_off = emitter.platform.stat_ino_offset();
    let dev_off = emitter.platform.stat_dev_offset();
    let mode_off = emitter.platform.stat_mode_offset(emitter.target.arch);
    emitter.instruction(&format!("sub rsp, {}", copy_frame));                   // reserve the path/payload slots plus two stat buffers
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the destination elephc path pointer while the source file is read into owned storage
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the destination elephc path length while the source file is read into owned storage
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the source path pointer: the same-file test below consumes it
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the source path length alongside it

    // -- EITHER end may be served by a registered userspace wrapper --
    //
    // Everything below this point is paths and syscalls, which cannot reach one: see the AArch64 arm; MEASURED,
    // `copy("cc://a", "out.txt")` warned `Failed to open stream` and answered false where php
    // copies and answers true. The probe is done here rather than in the lowering because a
    // wrapper is registered at RUN time, so a literal path is not what decides it.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // the source path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");
    emitter.instruction("call __rt_path_is_wrapper");
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_copy_via_wrapper_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the destination path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("call __rt_path_is_wrapper");
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_copy_via_wrapper_x86");
    // The probe CLOBBERS rax/rdi/rsi, and `__rt_path_cstr` below reads the source path pair.
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // restore the source path pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // and its length

    // See the AArch64 arm: php refuses to copy a file onto itself, judged by (st_dev, st_ino).
    emitter.instruction("call __rt_path_cstr");                                 // the source path as a C string
    emitter.instruction("mov rdi, rax");
    emitter.instruction(&format!("lea rsi, [rbp - {}]", src_stat));             // fill the source stat buffer
    emitter.instruction("call stat");
    emitter.instruction("test eax, eax");
    emitter.instruction("jnz __rt_copy_not_same_file_x86");                     // no source to stat: the read below reports it
    // See the AArch64 arm: php refuses a DIRECTORY source before it opens anything.
    emitter.instruction(&format!(
        "mov r10d, DWORD PTR [rbp - {}]", src_stat - mode_off
    ));                                                                         // st_mode
    emitter.instruction("and r10d, 0xF000");                                    // isolate the S_IFMT bits
    emitter.instruction("cmp r10d, 0x4000");                                    // S_IFDIR
    emitter.instruction("jne __rt_copy_source_not_dir_x86");
    abi::emit_symbol_address(emitter, "rdi", "_copy_source_is_dir_msg");
    emitter.instruction(&format!("mov esi, {}", COPY_SOURCE_IS_DIR_MSG.len()));
    emitter.instruction("call __rt_diag_warning");                              // honours @ like every other php warning
    emitter.instruction("xor eax, eax");                                        // php answers false and copies nothing
    emitter.instruction("jmp __rt_copy_return_x86");
    emitter.label("__rt_copy_source_not_dir_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the destination path
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");
    emitter.instruction("call __rt_path_cstr");
    emitter.instruction("mov rdi, rax");
    emitter.instruction(&format!("lea rsi, [rbp - {}]", dst_stat));             // fill the destination stat buffer
    emitter.instruction("call stat");
    emitter.instruction("test eax, eax");
    emitter.instruction("jnz __rt_copy_not_same_file_x86");                     // nothing there yet: an ordinary copy
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", src_stat - ino_off));
    emitter.instruction(&format!("cmp r10, QWORD PTR [rbp - {}]", dst_stat - ino_off));
    emitter.instruction("jne __rt_copy_not_same_file_x86");                     // different inode: different file
    // Darwin stores st_dev as a signed 32-bit int, so only a word of it is the device.
    let (dev_reg, dev_ptr) = if emitter.platform.stat_dev_is_narrow() {
        ("r10d", "DWORD")
    } else {
        ("r10", "QWORD")
    };
    emitter.instruction(&format!("mov {}, {} PTR [rbp - {}]", dev_reg, dev_ptr, src_stat - dev_off));
    emitter.instruction(&format!("cmp {}, {} PTR [rbp - {}]", dev_reg, dev_ptr, dst_stat - dev_off));
    emitter.instruction("jne __rt_copy_not_same_file_x86");                     // same inode on another device
    emitter.instruction("xor eax, eax");                                        // php answers false and copies nothing
    emitter.instruction("jmp __rt_copy_return_x86");
    emitter.label("__rt_copy_not_same_file_x86");

    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the source path the stats consumed
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");
    emitter.instruction("call __rt_file_get_contents");                         // read the source file into an owned elephc string before writing it to the destination path
    // See the AArch64 arm: a failed source open must leave the destination untouched.
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_copy_source_failed_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the copied file payload pointer across the destination-path reload and write helper call
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // preserve the copied file payload length across the destination-path reload and write helper call
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination elephc path pointer into the primary x86_64 string argument register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the destination elephc path length into the primary x86_64 string length register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the copied file payload pointer as the data pointer argument to file_put_contents()
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the copied file payload length as the data length argument to file_put_contents()
    emitter.instruction("xor ecx, ecx");                                        // the writer reads $flags for FILE_APPEND: this caller has none
    emitter.instruction("call __rt_file_put_contents");                         // write the copied file payload into the destination path through the shared file_put_contents() helper
    emitter.instruction("cmp rax, 0");                                          // treat zero-byte writes as success so empty files can still be copied correctly
    emitter.instruction("setge al");                                            // convert the signed write result into a boolean success byte where any non-negative byte count is success
    emitter.instruction("movzx rax, al");                                       // widen the boolean success byte into the canonical integer result register
    emitter.instruction("jmp __rt_copy_return_x86");
    emitter.label("__rt_copy_source_failed_x86");
    emitter.instruction("xor eax, eax");                                        // php answers false and leaves the destination alone
    emitter.label("__rt_copy_return_x86");
    emitter.instruction(&format!("add rsp, {}", copy_frame));                   // release the aligned stack locals used by copy()
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the copy() success predicate
    emitter.instruction("ret");                                                 // return the copy() success predicate to the caller

    // See the AArch64 arm: hand the whole copy to the stream route, frame already gone.
    emitter.label("__rt_copy_via_wrapper_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the source path, in the shape the route takes
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // and the destination
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction(&format!("add rsp, {}", copy_frame));                   // release the aligned stack locals used by copy()
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_copy_wrapper");                               // tail call: its answer is this one's

}

/// Emits a leaf helper for single-path libc filesystem functions on x86_64.
///
/// Takes an optional setup instruction string inserted before the libc call to populate
/// extra arguments (e.g., mode for `mkdir`). The C path is passed via `__rt_cstr`
/// output in `rax`; the libc result is compared against 0 and returned as 1 (success) or
/// 0 (failure) in `rax`.
/// Emits the x86_64 `__rt_mkdir` body: honours `$permissions` and creates parents when asked.
///
/// Input: rax/rdx = path pair, rcx = mode, r8 = recursive. Mirrors the AArch64 helper above,
/// including the in-place separator trick over `_cstr_buf` — see its comment for why that is safe.
/// The mode and flag are spilled first because `__rt_cstr` uses r8-r11 as scratch, so they would
/// not survive the conversion in registers.
fn emit_mkdir_libc_helper(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while the helper makes libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the call-aligned helper body
    emitter.instruction("sub rsp, 32");                                         // reserve slots for the mode, the flag, the buffer base and the scan index
    emitter.instruction("mov QWORD PTR [rbp - 8], rcx");                        // save the requested mode across __rt_cstr
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // save the recursive flag across __rt_cstr
    emitter.instruction("call __rt_path_cstr");                                 // convert the elephc path in rax/rdx into a null-terminated C string in rax
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the buffer base for both routes
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the recursive flag
    emitter.instruction("test r9, r9");                                         // was a recursive create requested?
    emitter.instruction("jz __rt_mkdir_leaf");                                  // not recursive: create only the named directory

    emitter.instruction("xor r10, r10");                                        // scan index into the C string
    emitter.label("__rt_mkdir_walk");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the buffer base
    emitter.instruction("movzx eax, BYTE PTR [r11 + r10]");                     // load the byte at the scan index
    emitter.instruction("test al, al");                                         // is it the terminator?
    emitter.instruction("jz __rt_mkdir_leaf");                                  // terminator reached: only the leaf is left
    emitter.instruction("cmp al, 47");                                          // is this byte a '/' separator?
    emitter.instruction("jne __rt_mkdir_walk_next");                            // ordinary byte — keep scanning
    emitter.instruction("test r10, r10");                                       // is the separator the leading one?
    emitter.instruction("jz __rt_mkdir_walk_next");                             // a leading '/' names the root, which always exists
    emitter.instruction("mov BYTE PTR [r11 + r10], 0");                         // terminate here so the buffer names the parent
    emitter.instruction("mov rdi, r11");                                        // pass the parent path to libc mkdir()
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // parents are created with the requested mode
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the scan index across the libc call
    emitter.instruction("call mkdir");                                          // create the parent, ignoring EEXIST and every other failure
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // restore the scan index
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // restore the buffer base
    emitter.instruction("mov BYTE PTR [r11 + r10], 47");                        // put the separator back before scanning on
    emitter.label("__rt_mkdir_walk_next");
    emitter.instruction("inc r10");                                             // advance the scan index
    emitter.instruction("jmp __rt_mkdir_walk");                                 // continue walking the path

    emitter.label("__rt_mkdir_leaf");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // the full path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // the requested mode
    emitter.instruction("call mkdir");                                          // create the named directory; its result is the answer
    // See the AArch64 counterpart: php names no path here, and elephc named nothing at all.
    emitter.instruction("cmp eax, 0");                                          // libc mkdir() returns zero as a C int on success
    emitter.instruction("je __rt_mkdir_ok_x86");
    super::path_op_warning::emit_libc_call_x86_64(
        emitter,
        "_warn_mkdir_head",
        MKDIR_WARNING_HEAD.len(),
        None,
        "_warn_mkdir_head",
        0,
    );
    emitter.instruction("xor eax, eax");                                        // php answers false for the failure it just named
    emitter.instruction("jmp __rt_mkdir_done_x86");
    emitter.label("__rt_mkdir_ok_x86");
    emitter.instruction("mov eax, 1");
    emitter.label("__rt_mkdir_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the aligned stack locals used by mkdir()
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the libc helper returns
    emitter.instruction("ret");                                                 // return the file-system success predicate to the caller
}

/// Whether this helper's path may be a `file://` URL.
///
/// php routes most path builtins through its plain-files WRAPPER, which reads the URL; the ones
/// that call libc directly never see it. MEASURED: `chdir("file:///tmp")` answers false in php.
#[derive(Clone, Copy, PartialEq)]
enum PathUrls {
    /// The plain-files wrapper handles the path, so a `file://` URL names the file it points at.
    Honoured,
    /// The call goes straight to libc, so the URL is just an unusual filename.
    Verbatim,
}

/// Same, with the URL rule spelled out.
fn emit_single_path_libc_bool_helper_with(
    emitter: &mut Emitter,
    symbol: &str,
    extra_setup: Option<&str>,
    urls: PathUrls,
    warn: Option<(&str, usize)>,
) {
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while the helper makes libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the call-aligned helper body
    emitter.instruction("sub rsp, 16");                                         // a slot for the path the warning names
    emitter.instruction(match urls {
        PathUrls::Honoured => "call __rt_path_cstr",                            // a `file://` URL names the file it points at
        PathUrls::Verbatim => "call __rt_cstr",                                 // this one php hands straight to libc
    });                                                                         // convert the elephc path in rax/rdx into a null-terminated C string in rax
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // the path the warning below names
    emitter.instruction("mov rdi, rax");                                        // pass the C path pointer as the first libc argument
    if let Some(setup) = extra_setup {
        emitter.instruction(setup);                                             // populate any additional libc arguments required by this helper
    }
    emitter.instruction(&format!("call {}", symbol));                           // invoke the matching libc file-system helper on Linux x86_64
    match warn {
        // See the AArch64 counterpart: php says WHY, and this said nothing at all.
        Some((head, head_len)) => {
            let ok = format!("__rt_{symbol}_ok_x86");
            let done = format!("__rt_{symbol}_done_x86");
            emitter.instruction("cmp eax, 0");                                  // libc answers zero on success
            emitter.instruction(&format!("je {ok}"));
            super::path_op_warning::emit_libc_call_x86_64(
                emitter,
                head,
                head_len,
                Some("[rbp - 8]"),
                "_warn_path_mid",
                PATH_WARNING_MIDDLE.len(),
            );
            emitter.instruction("xor eax, eax");                                // php answers false for the failure it just named
            emitter.instruction(&format!("jmp {done}"));
            emitter.label(&ok);
            emitter.instruction("mov eax, 1");
            emitter.label(&done);
        }
        None => {
            emitter.instruction("cmp eax, 0");                                  // libc path helpers return zero as a C int on success
            emitter.instruction("sete al");                                     // convert the success code into a boolean byte
            emitter.instruction("movzx rax, al");                               // widen the boolean byte into the canonical integer result register
        }
    }
    emitter.instruction("mov rsp, rbp");                                        // release the path slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the libc helper returns
    emitter.instruction("ret");                                                 // return the file-system success predicate to the caller
}
