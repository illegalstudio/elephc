//! Purpose:
//! Emits the `__rt_unlink`, `__rt_cstr` runtime helper assembly for fs.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

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

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- null-terminate path and call unlink --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr
    emitter.syscall(10);

    // -- return success/failure --
    emitter.instruction("cmp x0, #0");                                          // check syscall result
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if unlink succeeded

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
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

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate frame for mode, flag, buffer and scan index
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer
    emitter.instruction("str x3, [sp, #16]");                                   // save the requested mode across __rt_cstr
    emitter.instruction("str x4, [sp, #24]");                                   // save the recursive flag across __rt_cstr

    // -- null-terminate the path into our own scratch buffer --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr
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

    // -- return success/failure --
    emitter.instruction("cmp x0, #0");                                          // check syscall result
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if mkdir succeeded

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

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- null-terminate path and call rmdir --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr
    emitter.syscall(137);

    // -- return success/failure --
    emitter.instruction("cmp x0, #0");                                          // check syscall result
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if rmdir succeeded

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
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

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save destination path before clobbering registers --
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save 'to' path ptr and len on stack

    // -- null-terminate source path using primary buffer --
    emitter.instruction("bl __rt_cstr");                                        // convert 'from' to C string in _cstr_buf
    emitter.instruction("str x0, [sp, #0]");                                    // save source cstr pointer

    // -- null-terminate destination path using secondary buffer --
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload 'to' path ptr and len
    emitter.instruction("bl __rt_cstr2");                                       // convert 'to' to C string in _cstr_buf2
    emitter.instruction("str x0, [sp, #8]");                                    // save destination cstr pointer

    // -- call rename syscall --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source cstr path
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload destination cstr path
    emitter.syscall(128);

    // -- return success/failure --
    emitter.instruction("cmp x0, #0");                                          // check syscall result
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if rename succeeded

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

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save destination path for after reading source --
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save 'to' path ptr and len on stack

    // -- read source file contents --
    emitter.instruction("bl __rt_file_get_contents");                           // read source, x1=data ptr, x2=data len

    // -- write contents to destination file --
    emitter.instruction("mov x3, x1");                                          // move data ptr to x3 (data arg)
    emitter.instruction("mov x4, x2");                                          // move data len to x4 (data arg)
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload destination path ptr and len
    emitter.instruction("bl __rt_file_put_contents");                           // write data to dest file, x0=bytes written

    // -- return 1 if bytes were written --
    emitter.instruction("cmp x0, #0");                                          // check if any bytes were written
    emitter.instruction("cset x0, gt");                                         // x0 = 1 if bytes written > 0

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits x86_64 Linux variants of all filesystem helpers using libc calls.
/// Uses a stack-based frame (rbp/rsp convention) instead of the ARM64 link-register frame.
/// Return value convention matches `emit_fs`: x0=1 on success, 0 on failure.
fn emit_fs_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unlink ---");
    emitter.label_global("__rt_unlink");
    emit_single_path_libc_bool_helper(emitter, "unlink", None);

    emitter.blank();
    emitter.comment("--- runtime: mkdir ---");
    emitter.label_global("__rt_mkdir");
    emit_mkdir_libc_helper(emitter);

    emitter.blank();
    emitter.comment("--- runtime: rmdir ---");
    emitter.label_global("__rt_rmdir");
    emit_single_path_libc_bool_helper(emitter, "rmdir", None);

    emitter.blank();
    emitter.comment("--- runtime: chdir ---");
    emitter.label_global("__rt_chdir");
    emit_single_path_libc_bool_helper(emitter, "chdir", None);

    emitter.blank();
    emitter.comment("--- runtime: rename ---");
    emitter.label_global("__rt_rename");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while rename uses temporary path slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source and destination path temporaries
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack space for the saved destination and source C-string pointers
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the destination elephc path pointer while converting the source path
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the destination elephc path length while converting the source path
    emitter.instruction("call __rt_cstr");                                      // convert the source elephc path in rax/rdx into a null-terminated C string
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the source C-string pointer for the later libc rename() call
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination elephc path pointer before converting it to a C string
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the destination elephc path length before converting it to a C string
    emitter.instruction("call __rt_cstr2");                                     // convert the destination elephc path into the secondary null-terminated C string buffer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the destination C-string pointer for the later libc rename() call
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the source C-string pointer as the first libc rename() argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the destination C-string pointer as the second libc rename() argument
    emitter.instruction("call rename");                                         // rename or move the file-system path through libc rename()
    emitter.instruction("cmp eax, 0");                                          // a successful libc rename() call returns zero as a C int
    emitter.instruction("sete al");                                             // convert the rename() success flag into a boolean byte
    emitter.instruction("movzx rax, al");                                       // widen the boolean byte into the canonical integer result register
    emitter.instruction("add rsp, 32");                                         // release the aligned stack locals used by rename()
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the rename() success predicate to the caller

    emitter.blank();
    emitter.comment("--- runtime: copy ---");
    emitter.label_global("__rt_copy");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while copy() uses path and payload spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved destination path and copied file payload
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack space for the destination path pair and copied payload pair
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the destination elephc path pointer while the source file is read into owned storage
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the destination elephc path length while the source file is read into owned storage
    emitter.instruction("call __rt_file_get_contents");                         // read the source file into an owned elephc string before writing it to the destination path
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the copied file payload pointer across the destination-path reload and write helper call
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // preserve the copied file payload length across the destination-path reload and write helper call
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination elephc path pointer into the primary x86_64 string argument register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the destination elephc path length into the primary x86_64 string length register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the copied file payload pointer as the data pointer argument to file_put_contents()
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the copied file payload length as the data length argument to file_put_contents()
    emitter.instruction("call __rt_file_put_contents");                         // write the copied file payload into the destination path through the shared file_put_contents() helper
    emitter.instruction("cmp rax, 0");                                          // treat zero-byte writes as success so empty files can still be copied correctly
    emitter.instruction("setge al");                                            // convert the signed write result into a boolean success byte where any non-negative byte count is success
    emitter.instruction("movzx rax, al");                                       // widen the boolean success byte into the canonical integer result register
    emitter.instruction("add rsp, 32");                                         // release the aligned stack locals used by copy()
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the copy() success predicate
    emitter.instruction("ret");                                                 // return the copy() success predicate to the caller

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
    emitter.instruction("call __rt_cstr");                                      // convert the elephc path in rax/rdx into a null-terminated C string in rax
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
    emitter.instruction("cmp eax, 0");                                          // libc mkdir() returns zero as a C int on success
    emitter.instruction("sete al");                                             // convert the success code into a boolean byte
    emitter.instruction("movzx rax, al");                                       // widen the boolean byte into the canonical integer result register
    emitter.instruction("add rsp, 32");                                         // release the aligned stack locals used by mkdir()
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the libc helper returns
    emitter.instruction("ret");                                                 // return the file-system success predicate to the caller
}

fn emit_single_path_libc_bool_helper(emitter: &mut Emitter, symbol: &str, extra_setup: Option<&str>) {
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while the helper makes libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the call-aligned helper body
    emitter.instruction("call __rt_cstr");                                      // convert the elephc path in rax/rdx into a null-terminated C string in rax
    emitter.instruction("mov rdi, rax");                                        // pass the C path pointer as the first libc argument
    if let Some(setup) = extra_setup {
        emitter.instruction(setup);                                             // populate any additional libc arguments required by this helper
    }
    emitter.instruction(&format!("call {}", symbol));                           // invoke the matching libc file-system helper on Linux x86_64
    emitter.instruction("cmp eax, 0");                                          // libc path helpers return zero as a C int on success
    emitter.instruction("sete al");                                             // convert the success code into a boolean byte
    emitter.instruction("movzx rax, al");                                       // widen the boolean byte into the canonical integer result register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the libc helper returns
    emitter.instruction("ret");                                                 // return the file-system success predicate to the caller
}
