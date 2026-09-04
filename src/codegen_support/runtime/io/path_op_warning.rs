//! Purpose:
//! Emits `__rt_path_op_warning`, the shared composer for the line php prints when a filesystem
//! PATH OPERATION fails — `unlink()`, `rmdir()`, `mkdir()`, `chmod()`, `touch()`, `opendir()` —
//! plus `__rt_rename_warning`, the one shape that names two paths.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The failure arm of each of those runtime helpers.
//!
//! Key details:
//! - MEASURED: php prints eleven warnings for a program of eleven failing path calls, and elephc
//!   printed none of them. The calls answered `false` in complete silence, so a script that
//!   checked the return value behaved the same and a script that read the log learned nothing.
//! - THE SHAPES ARE NOT ONE SHAPE, which is why the composer takes its text in two pieces:
//!
//!   ```text
//!   Warning: unlink(nope.txt): No such file or directory
//!   Warning: rmdir(f.txt): Not a directory
//!   Warning: opendir(nope): Failed to open directory: No such file or directory
//!   Warning: rename(nope.txt,other.txt): No such file or directory
//!   Warning: mkdir(): File exists
//!   Warning: chmod(): No such file or directory
//!   Warning: touch(): Unable to create file /no/such/x.txt because No such file or directory
//!   ```
//!
//!   `mkdir()` and `chmod()` name no path AT ALL, and `touch()` puts it in the middle of a
//!   sentence rather than in the parentheses. A composer built around `name(path): reason` would
//!   have had to lie about three of the seven.
//! - The path is the argument as WRITTEN, measured from the C string the call is about to use —
//!   so a `file://` URL prints as the path it names, which is what php prints for this family.
//! - `__rt_errno_warning` is the tail: it appends `strerror` and the newline, and every fragment
//!   goes out through `__rt_diag_warning`, which is what honours `@`.
//! - `__rt_path_op_fragment` is the same body without that tail, so `rename()` can write its
//!   first path and comma and then let the ordinary entry close the line.

use crate::codegen_support::runtime::data::{
    PATH_WARNING_MIDDLE, RENAME_WARNING_HEAD, RENAME_WARNING_SEPARATOR,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_path_op_warning(head, head_len, path_or_null, mid, mid_len, errno)` and its
/// tail-less twin `__rt_path_op_fragment(head, head_len, path_or_null, mid, mid_len)`.
///
/// AArch64 takes `x0`-`x5`; x86_64 takes the SysV argument registers. A null path is skipped
/// entirely, which is how the `mkdir()` and `chmod()` shapes are spelled, and a zero-length head
/// is skipped too, which is what lets `rename()` continue a line it has already begun.
pub fn emit_path_op_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// The AArch64 arm.
///
/// The two entry points carry their own copy of the body rather than sharing one through a flag.
/// A `label_global` OPENS AN ATOM on macOS, so a local label defined under one global and branched
/// to from another is a cross-atom reference `-dead_strip` removes — the emitter has a gate that
/// says so by name, and this is what it caught.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: path_op_fragment ---");
    emitter.label_global("__rt_path_op_fragment");
    emit_body_aarch64(emitter, "frag", false);

    emitter.blank();
    emitter.comment("--- runtime: path_op_warning ---");
    emitter.label_global("__rt_path_op_warning");
    emit_body_aarch64(emitter, "warn", true);
}

/// One copy of the AArch64 body. `close` appends `strerror` and the newline.
fn emit_body_aarch64(emitter: &mut Emitter, tag: &str, close: bool) {
    let path_l = format!("__rt_pow_{tag}_path");
    let mid_l = format!("__rt_pow_{tag}_mid");
    let scan_l = format!("__rt_pow_{tag}_scan");
    let scanned_l = format!("__rt_pow_{tag}_scanned");
    let done_l = format!("__rt_pow_{tag}_done");

    // Frame: [0]=path [8]=mid ptr [16]=mid len [24]=errno
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");
    emitter.instruction("add x29, sp, #48");
    emitter.instruction("str x2, [sp, #0]");                                    // the path, or zero for the shapes that name none
    emitter.instruction("str x3, [sp, #8]");                                    // the text between the path and the reason
    emitter.instruction("str x4, [sp, #16]");
    emitter.instruction("str x5, [sp, #24]");                                   // the error number the failing call gave

    emitter.instruction(&format!("cbz x1, {path_l}"));                          // a zero-length head continues a line already begun
    emitter.instruction("mov x2, x1");                                          // the diagnostic sink takes the length in x2
    emitter.instruction("mov x1, x0");                                          // and the pointer in x1
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth

    emitter.label(&path_l);
    emitter.instruction("ldr x1, [sp, #0]");
    emitter.instruction(&format!("cbz x1, {mid_l}"));                           // this shape names no path
    emitter.instruction("mov x9, #0");                                          // measured length
    emitter.label(&scan_l);
    emitter.instruction("ldrb w10, [x1, x9]");
    emitter.instruction(&format!("cbz w10, {scanned_l}"));                      // reached the terminator
    emitter.instruction("add x9, x9, #1");
    emitter.instruction(&format!("b {scan_l}"));
    emitter.label(&scanned_l);
    emitter.instruction("mov x2, x9");
    emitter.instruction("bl __rt_diag_warning");

    emitter.label(&mid_l);
    if close {
        emitter.instruction("ldr x0, [sp, #8]");
        emitter.instruction("ldr x1, [sp, #16]");
        emitter.instruction("ldr x2, [sp, #24]");
        emitter.instruction("bl __rt_errno_warning");                           // strerror and the newline close the line
    } else {
        emitter.instruction("ldr x1, [sp, #8]");
        emitter.instruction("ldr x2, [sp, #16]");
        emitter.instruction("bl __rt_diag_warning");                            // the caller writes the rest
    }

    emitter.label(&done_l);
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// The x86_64 arm. Mirrors the AArch64 one, body copy included.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: path_op_fragment ---");
    emitter.label_global("__rt_path_op_fragment");
    emit_body_x86_64(emitter, "frag", false);

    emitter.blank();
    emitter.comment("--- runtime: path_op_warning ---");
    emitter.label_global("__rt_path_op_warning");
    emit_body_x86_64(emitter, "warn", true);
}

/// One copy of the x86_64 body. `close` appends `strerror` and the newline.
fn emit_body_x86_64(emitter: &mut Emitter, tag: &str, close: bool) {
    let path_l = format!("__rt_pow_{tag}_path_x86");
    let scan_l = format!("__rt_pow_{tag}_scan_x86");
    let scanned_l = format!("__rt_pow_{tag}_scanned_x86");
    let mid_l = format!("__rt_pow_{tag}_mid_x86");

    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 48");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // the path, or zero for the shapes that name none
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // the text between the path and the reason
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // the error number the failing call gave

    emitter.instruction("test rsi, rsi");
    emitter.instruction(&format!("jz {path_l}"));                               // a zero-length head continues a line already begun
    emitter.instruction("call __rt_diag_warning");                              // head already sits in rdi/rsi

    emitter.label(&path_l);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("test rdi, rdi");
    emitter.instruction(&format!("jz {mid_l}"));                                // this shape names no path
    emitter.instruction("xor r9, r9");                                          // measured length
    emitter.label(&scan_l);
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r9]");
    emitter.instruction("test r10b, r10b");
    emitter.instruction(&format!("jz {scanned_l}"));                            // reached the terminator
    emitter.instruction("add r9, 1");
    emitter.instruction(&format!("jmp {scan_l}"));
    emitter.label(&scanned_l);
    emitter.instruction("mov rsi, r9");
    emitter.instruction("call __rt_diag_warning");

    emitter.label(&mid_l);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");
    if close {
        emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");
        emitter.instruction("call __rt_errno_warning");                         // strerror and the newline close the line
    } else {
        emitter.instruction("call __rt_diag_warning");                          // the caller writes the rest
    }

    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// Emits `__rt_rename_warning(from, to, errno)`, the one shape that names TWO paths.
///
/// MEASURED: `Warning: rename(nope.txt,other.txt): No such file or directory` — comma, no space.
/// It gets a helper of its own rather than a seventh argument on the shared composer, which
/// x86_64 would have to pass on the stack for this single caller.
pub fn emit_rename_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: rename_warning ---");
            emitter.label_global("__rt_rename_warning");
            emitter.instruction("sub sp, sp, #48");
            emitter.instruction("stp x29, x30, [sp, #32]");
            emitter.instruction("add x29, sp, #32");
            emitter.instruction("str x0, [sp, #0]");                            // the source path
            emitter.instruction("str x1, [sp, #8]");                            // the destination path
            emitter.instruction("str x2, [sp, #16]");                           // the error number

            abi::emit_symbol_address(emitter, "x0", "_warn_rename_head");
            emitter.instruction(&format!("mov x1, #{}", RENAME_WARNING_HEAD.len()));
            emitter.instruction("ldr x2, [sp, #0]");
            abi::emit_symbol_address(emitter, "x3", "_warn_rename_sep");
            emitter.instruction(&format!("mov x4, #{}", RENAME_WARNING_SEPARATOR.len()));
            emitter.instruction("bl __rt_path_op_fragment");                    // head, source, comma

            emitter.instruction("mov x0, #0");                                  // the head is already written
            emitter.instruction("mov x1, #0");
            emitter.instruction("ldr x2, [sp, #8]");
            abi::emit_symbol_address(emitter, "x3", "_warn_path_mid");
            emitter.instruction(&format!("mov x4, #{}", PATH_WARNING_MIDDLE.len()));
            emitter.instruction("ldr x5, [sp, #16]");
            emitter.instruction("bl __rt_path_op_warning");                     // destination, "): ", reason

            emitter.instruction("ldp x29, x30, [sp, #32]");
            emitter.instruction("add sp, sp, #48");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: rename_warning ---");
            emitter.label_global("__rt_rename_warning");
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 32");
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // the source path
            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // the destination path
            emitter.instruction("mov QWORD PTR [rbp - 24], rdx");               // the error number

            emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");
            abi::emit_symbol_address(emitter, "rdi", "_warn_rename_head");
            emitter.instruction(&format!("mov rsi, {}", RENAME_WARNING_HEAD.len()));
            abi::emit_symbol_address(emitter, "rcx", "_warn_rename_sep");
            emitter.instruction(&format!("mov r8, {}", RENAME_WARNING_SEPARATOR.len()));
            emitter.instruction("call __rt_path_op_fragment");                  // head, source, comma

            emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");
            emitter.instruction("xor edi, edi");                                // the head is already written
            emitter.instruction("xor esi, esi");
            abi::emit_symbol_address(emitter, "rcx", "_warn_path_mid");
            emitter.instruction(&format!("mov r8, {}", PATH_WARNING_MIDDLE.len()));
            emitter.instruction("mov r9, QWORD PTR [rbp - 24]");
            emitter.instruction("call __rt_path_op_warning");                   // destination, "): ", reason

            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits the AArch64 call for a site whose failure came from LIBC rather than a syscall.
///
/// libc reports through a thread-local `errno` and answers a bare `-1`, so the reason has to be
/// fetched before anything else clobbers it. The accessor is spelled differently per platform —
/// the same split `scandir()` and the flock helpers already carry.
pub(super) fn emit_libc_call_aarch64(
    emitter: &mut Emitter,
    head: &str,
    head_len: usize,
    path_slot: Option<&str>,
    mid: &str,
    mid_len: usize,
) {
    let errno_function = match emitter.platform {
        crate::codegen_support::platform::Platform::MacOS => "__error",
        crate::codegen_support::platform::Platform::Linux => "__errno_location",
        crate::codegen_support::platform::Platform::Windows => {
            panic!("Windows target is not yet supported (see issue #379)")
        }
    };
    emitter.bl_c(errno_function);                                               // x0 = &errno for this thread
    emitter.instruction("ldrsw x5, [x0]");                                      // the reason libc recorded
    match path_slot {
        Some(slot) => emitter.instruction(&format!("ldr x2, {slot}")),          // the path php names
        None => emitter.instruction("mov x2, #0"),                              // this shape names none
    }
    abi::emit_symbol_address(emitter, "x0", head);
    emitter.instruction(&format!("mov x1, #{head_len}"));
    abi::emit_symbol_address(emitter, "x3", mid);
    emitter.instruction(&format!("mov x4, #{mid_len}"));
    emitter.instruction("bl __rt_path_op_warning");
}

/// The x86_64 counterpart.
pub(super) fn emit_libc_call_x86_64(
    emitter: &mut Emitter,
    head: &str,
    head_len: usize,
    path_slot: Option<&str>,
    mid: &str,
    mid_len: usize,
) {
    let errno_function = match emitter.platform {
        crate::codegen_support::platform::Platform::MacOS => "__error",
        crate::codegen_support::platform::Platform::Linux => "__errno_location",
        crate::codegen_support::platform::Platform::Windows => {
            panic!("Windows target is not yet supported (see issue #379)")
        }
    };
    emitter.bl_c(errno_function);                                               // rax = &errno for this thread
    emitter.instruction("movsxd r9, DWORD PTR [rax]");                          // the reason libc recorded
    match path_slot {
        Some(slot) => emitter.instruction(&format!("mov rdx, QWORD PTR {slot}")), // the path php names
        None => emitter.instruction("xor edx, edx"),                            // this shape names none
    }
    abi::emit_symbol_address(emitter, "rdi", head);
    emitter.instruction(&format!("mov rsi, {head_len}"));
    abi::emit_symbol_address(emitter, "rcx", mid);
    emitter.instruction(&format!("mov r8, {mid_len}"));
    emitter.instruction("call __rt_path_op_warning");
}

/// Emits the AArch64 call that raises one of these warnings.
///
/// Entered with the FAILING syscall result still in `x0`: Linux answers `-errno` there and macOS
/// the errno itself, which is the one difference between the two and the reason this is a helper
/// rather than six copies of the same six instructions.
///
/// `path_slot` is a frame reference like `"[sp, #8]"` holding the NUL-terminated C path, or
/// `None` for the shapes php prints with empty parentheses.
pub(super) fn emit_call_aarch64(
    emitter: &mut Emitter,
    head: &str,
    head_len: usize,
    path_slot: Option<&str>,
    mid: &str,
    mid_len: usize,
) {
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("neg x5, x0");                                      // Linux answers -errno
    } else {
        emitter.instruction("mov x5, x0");                                      // macOS answers the errno itself
    }
    match path_slot {
        Some(slot) => emitter.instruction(&format!("ldr x2, {slot}")),          // the path php names
        None => emitter.instruction("mov x2, #0"),                              // this shape names none
    }
    abi::emit_symbol_address(emitter, "x0", head);
    emitter.instruction(&format!("mov x1, #{head_len}"));
    abi::emit_symbol_address(emitter, "x3", mid);
    emitter.instruction(&format!("mov x4, #{mid_len}"));
    emitter.instruction("bl __rt_path_op_warning");
}

