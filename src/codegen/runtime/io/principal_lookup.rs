//! Purpose:
//! Emits passwd/group file lookup helpers used by chown/chgrp name resolution.
//! Keeps static binaries independent from host NSS modules for local principal lookups.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` before filesystem modify helpers.
//!
//! Key details:
//! - Helpers scan `/etc/passwd` or `/etc/group` with libc file I/O and return `-1` when absent.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits runtime helpers that resolve local user/group names to numeric ids.
///
/// Both helpers take a null-terminated principal name plus its byte length and
/// return the numeric uid/gid, or `-1` when the local files cannot be read or no
/// matching entry exists.
pub(crate) fn emit_principal_lookup(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_lookup_x86_64(emitter, "__rt_lookup_passwd_uid", "_etc_passwd_path");
        emit_lookup_x86_64(emitter, "__rt_lookup_group_gid", "_etc_group_path");
        return;
    }

    emit_lookup_aarch64(emitter, "__rt_lookup_passwd_uid", "_etc_passwd_path");
    emit_lookup_aarch64(emitter, "__rt_lookup_group_gid", "_etc_group_path");
}

/// Emits one x86_64 helper that scans a colon-separated principal database.
fn emit_lookup_x86_64(emitter: &mut Emitter, label: &str, path_symbol: &str) {
    let loop_label = format!("{label}_loop");
    let close_fail_label = format!("{label}_close_fail");
    let fail_label = format!("{label}_fail");
    let done_label = format!("{label}_done");

    emitter.blank();
    emitter.comment(&format!("--- runtime: {label} ---"));
    emitter.label_global(label);
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 48");                                         // align stack and reserve lookup spills
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // preserve principal C-string pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve principal byte length
    abi::emit_symbol_address(emitter, "rdi", path_symbol);
    abi::emit_symbol_address(emitter, "rsi", "_principal_lookup_read_mode");
    emitter.instruction("call fopen");                                          // open the local principal database for reading
    emitter.instruction("test rax, rax");                                       // did fopen return a FILE pointer?
    emitter.instruction(&format!("je {fail_label}"));                           // missing database behaves like an unknown principal
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve FILE pointer across loop calls

    emitter.label(&loop_label);
    abi::emit_symbol_address(emitter, "rdi", "_principal_lookup_buf");
    emitter.instruction("mov esi, 4096");                                       // maximum line bytes to read into the shared scratch buffer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // pass FILE pointer to fgets
    emitter.instruction("call fgets");                                          // read one principal database line
    emitter.instruction("test rax, rax");                                       // did fgets return a line?
    emitter.instruction(&format!("je {close_fail_label}"));                     // EOF without a match returns not found
    abi::emit_symbol_address(emitter, "rdi", "_principal_lookup_buf");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // compare against the requested principal name
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // compare exactly the requested name length
    emitter.instruction("call strncmp");                                        // compare database entry prefix with requested name
    emitter.instruction("test eax, eax");                                       // did the prefix match?
    emitter.instruction(&format!("jne {loop_label}"));                          // keep scanning until the name matches
    abi::emit_symbol_address(emitter, "r8", "_principal_lookup_buf");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // load matched-name length for delimiter validation
    emitter.instruction("cmp BYTE PTR [r8 + rcx], 58");                         // entry name must be followed by ':' to avoid prefix matches
    emitter.instruction(&format!("jne {loop_label}"));                          // reject partial-prefix matches
    emitter.instruction("lea rdi, [r8 + rcx + 1]");                             // search after the entry-name delimiter
    emitter.instruction("mov esi, 58");                                         // delimiter byte ':'
    emitter.instruction("call strchr");                                         // locate the delimiter before uid/gid
    emitter.instruction("test rax, rax");                                       // was the numeric field delimiter present?
    emitter.instruction(&format!("je {loop_label}"));                           // malformed line: keep scanning
    emitter.instruction("lea rdi, [rax + 1]");                                  // numeric id starts after the second delimiter
    emitter.instruction("xor esi, esi");                                        // endptr = NULL
    emitter.instruction("mov edx, 10");                                         // parse decimal uid/gid
    emitter.instruction("call strtoul");                                        // parse the numeric principal id
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve parsed id across fclose
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // close the database before returning a match
    emitter.instruction("call fclose");                                         // release FILE handle
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return parsed uid/gid
    emitter.instruction(&format!("jmp {done_label}"));                          // skip not-found sentinel

    emitter.label(&close_fail_label);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // close the database after EOF
    emitter.instruction("call fclose");                                         // release FILE handle
    emitter.label(&fail_label);
    emitter.instruction("mov rax, -1");                                         // not found sentinel

    emitter.label(&done_label);
    emitter.instruction("add rsp, 48");                                         // release lookup frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return uid/gid or -1
}

/// Emits one AArch64 helper that scans a colon-separated principal database.
fn emit_lookup_aarch64(emitter: &mut Emitter, label: &str, path_symbol: &str) {
    let loop_label = format!("{label}_loop");
    let close_fail_label = format!("{label}_close_fail");
    let fail_label = format!("{label}_fail");
    let done_label = format!("{label}_done");

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for ARM64 instructions
    emitter.comment(&format!("--- runtime: {label} ---"));
    emitter.label_global(label);
    emitter.instruction("sub sp, sp, #64");                                     // allocate lookup frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer
    emitter.instruction("str x0, [sp, #8]");                                    // preserve principal C-string pointer
    emitter.instruction("str x1, [sp, #16]");                                   // preserve principal byte length
    abi::emit_symbol_address(emitter, "x0", path_symbol);
    abi::emit_symbol_address(emitter, "x1", "_principal_lookup_read_mode");
    emitter.bl_c("fopen");                                                      // open the local principal database for reading
    emitter.instruction(&format!("cbz x0, {fail_label}"));                      // missing database behaves like an unknown principal
    emitter.instruction("str x0, [sp, #0]");                                    // preserve FILE pointer across loop calls

    emitter.label(&loop_label);
    abi::emit_symbol_address(emitter, "x0", "_principal_lookup_buf");
    emitter.instruction("mov x1, #4096");                                       // maximum line bytes to read into the shared scratch buffer
    emitter.instruction("ldr x2, [sp, #0]");                                    // pass FILE pointer to fgets
    emitter.bl_c("fgets");                                                      // read one principal database line
    emitter.instruction(&format!("cbz x0, {close_fail_label}"));                // EOF without a match returns not found
    abi::emit_symbol_address(emitter, "x0", "_principal_lookup_buf");
    emitter.instruction("ldr x1, [sp, #8]");                                    // compare against the requested principal name
    emitter.instruction("ldr x2, [sp, #16]");                                   // compare exactly the requested name length
    emitter.bl_c("strncmp");                                                    // compare database entry prefix with requested name
    emitter.instruction(&format!("cbnz x0, {loop_label}"));                     // keep scanning until the name matches
    abi::emit_symbol_address(emitter, "x9", "_principal_lookup_buf");
    emitter.instruction("ldr x10, [sp, #16]");                                  // load matched-name length for delimiter validation
    emitter.instruction("ldrb w11, [x9, x10]");                                 // read the byte after the matched prefix
    emitter.instruction("cmp w11, #58");                                        // entry name must be followed by ':' to avoid prefix matches
    emitter.instruction(&format!("b.ne {loop_label}"));                         // reject partial-prefix matches
    emitter.instruction("add x0, x9, x10");                                     // move to the entry-name delimiter
    emitter.instruction("add x0, x0, #1");                                      // search after the entry-name delimiter
    emitter.instruction("mov x1, #58");                                         // delimiter byte ':'
    emitter.bl_c("strchr");                                                     // locate the delimiter before uid/gid
    emitter.instruction(&format!("cbz x0, {loop_label}"));                      // malformed line: keep scanning
    emitter.instruction("add x0, x0, #1");                                      // numeric id starts after the second delimiter
    emitter.instruction("mov x1, #0");                                          // endptr = NULL
    emitter.instruction("mov x2, #10");                                         // parse decimal uid/gid
    emitter.bl_c("strtoul");                                                    // parse the numeric principal id
    emitter.instruction("str x0, [sp, #24]");                                   // preserve parsed id across fclose
    emitter.instruction("ldr x0, [sp, #0]");                                    // close the database before returning a match
    emitter.bl_c("fclose");                                                     // release FILE handle
    emitter.instruction("ldr x0, [sp, #24]");                                   // return parsed uid/gid
    emitter.instruction(&format!("b {done_label}"));                            // skip not-found sentinel

    emitter.label(&close_fail_label);
    emitter.instruction("ldr x0, [sp, #0]");                                    // close the database after EOF
    emitter.bl_c("fclose");                                                     // release FILE handle
    emitter.label(&fail_label);
    emitter.instruction("mov x0, #-1");                                         // not found sentinel

    emitter.label(&done_label);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release lookup frame
    emitter.instruction("ret");                                                 // return uid/gid or -1
}
