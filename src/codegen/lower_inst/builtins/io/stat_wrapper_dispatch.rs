//! Purpose:
//! Routes the whole stat family through a userspace wrapper's `url_stat()` before the real
//! filesystem — the predicates, the integer fields, `filetype()` and `stat()`/`lstat()`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io::stat_ops`.
//!
//! Key details:
//! - Each builtin passes the `url_stat()` FLAGS php passes it, measured one call at a time on php
//!   8.5.6 with `clearstatcache(true)` between probes — without that php answers most of them out
//!   of its one-entry stat cache and the dispatch is invisible. The four values php actually
//!   uses are the four combinations below; nothing passes 0, which is what elephc used to send.
//! - Every route reads `_url_stat_matched` to tell "this path names no registered wrapper, use the
//!   filesystem" from "the wrapper answered", exactly as `file_exists()` already did.
//! - The predicates need no separate failure test: a failed stat leaves a zero mode, and no
//!   `S_IFMT` php names is zero, so `is_dir()` on an absent path falls out as false on its own.

use super::*;
use crate::codegen_support::runtime::io::{
    STAT_FIELD_ATIME, STAT_FIELD_CTIME, STAT_FIELD_GID, STAT_FIELD_INO, STAT_FIELD_MODE,
    STAT_FIELD_MTIME, STAT_FIELD_UID,
};

/// `PHP_STREAM_URL_STAT_LINK`: do not follow the last symlink.
const URL_STAT_LINK: usize = 1;

/// `PHP_STREAM_URL_STAT_QUIET`: the caller reports nothing of its own on failure.
const URL_STAT_QUIET: usize = 2;

/// `PHP_STREAM_URL_STAT_NOCACHE`: php's own stat cache must not answer this one.
const URL_STAT_NOCACHE: usize = 4;

/// What the value readers pass — `filesize()`, `filemtime()`, `fileperms()`, `stat()`, …
pub(super) const URL_STAT_FLAGS_VALUE: usize = URL_STAT_NOCACHE;

/// What the link-following-free value readers pass — `filetype()` and `lstat()`.
pub(super) const URL_STAT_FLAGS_LINK_VALUE: usize = URL_STAT_LINK | URL_STAT_NOCACHE;

/// What the silent predicates pass — `file_exists()`, `is_file()`, `is_dir()`, `is_readable()`, …
pub(super) const URL_STAT_FLAGS_EXISTS: usize = URL_STAT_QUIET | URL_STAT_NOCACHE;

/// What `is_link()` passes: the only predicate that does not follow the symlink.
pub(super) const URL_STAT_FLAGS_LINK_EXISTS: usize =
    URL_STAT_LINK | URL_STAT_QUIET | URL_STAT_NOCACHE;

/// `S_IFDIR`, the file-type bits `is_dir()` accepts.
const S_IFDIR: usize = 0x4000;

/// `S_IFLNK`, the file-type bits `is_link()` accepts.
const S_IFLNK: usize = 0xA000;

/// Which permission `__rt_user_wrapper_url_stat_access` tests: read.
const ACCESS_READ: usize = 0;

/// Which permission `__rt_user_wrapper_url_stat_access` tests: write.
const ACCESS_WRITE: usize = 1;

/// Which permission `__rt_user_wrapper_url_stat_access` tests: execute.
const ACCESS_EXECUTE: usize = 2;

/// Emits a wrapper `url_stat()` integer-field read with a native filesystem fallback.
///
/// The path arrives in the string pair and both routes leave the `(value, success)` pair
/// `box_stat_int_or_false_result` reads — x0/x1 on AArch64, rax/rdx on x86_64.
pub(super) fn emit_url_stat_int_or_fallback(
    ctx: &mut FunctionContext<'_>,
    fallback_runtime: &str,
    field_selector: usize,
    flags: usize,
) {
    let fallback = ctx.next_label("url_stat_int_fs");
    let done = ctx.next_label("url_stat_int_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve path scratch storage across url_stat
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer for the filesystem fallback
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length for the filesystem fallback
            ctx.emitter.instruction("mov x0, x1");                              // pass the path pointer to the field reader
            ctx.emitter.instruction("mov x1, x2");                              // pass the path length to the field reader
            ctx.emitter.instruction(&format!("mov x2, #{}", field_selector));   // select the stat field php reads here
            ctx.emitter.instruction(&format!("mov x3, #{}", flags));            // the url_stat flags php passes this caller
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat_int");
            abi::emit_symbol_address(ctx.emitter, "x9", "_url_stat_matched");
            ctx.emitter.instruction("ldrb w9, [x9]");                           // read whether a registered wrapper scheme matched
            ctx.emitter.instruction(&format!("cbz w9, {}", fallback));          // fall back to filesystem stat when no wrapper matched
            ctx.emitter.instruction(&format!("b {}", done));                    // keep the wrapper's (value, success) pair
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // restore the path pointer for the filesystem fallback
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // restore the path length for the filesystem fallback
            abi::emit_call_label(ctx.emitter, fallback_runtime);
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add sp, sp, #16");                         // release path scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve path scratch storage across url_stat
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer for the filesystem fallback
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length for the filesystem fallback
            ctx.emitter.instruction("mov rdi, rax");                            // pass the path pointer to the field reader
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the path length to the field reader
            ctx.emitter.instruction(&format!("mov rdx, {}", field_selector));   // select the stat field php reads here
            ctx.emitter.instruction(&format!("mov rcx, {}", flags));            // the url_stat flags php passes this caller
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat_int");
            abi::emit_symbol_address(ctx.emitter, "r9", "_url_stat_matched");
            ctx.emitter.instruction("movzx r9d, BYTE PTR [r9]");                // read whether a registered wrapper scheme matched
            ctx.emitter.instruction("test r9d, r9d");                           // test the url_stat matched flag
            ctx.emitter.instruction(&format!("jz {}", fallback));               // fall back to filesystem stat when no wrapper matched
            ctx.emitter.instruction(&format!("jmp {}", done));                  // keep the wrapper's (value, success) pair
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the path pointer for the filesystem fallback
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // restore the path length for the filesystem fallback
            abi::emit_call_label(ctx.emitter, fallback_runtime);
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add rsp, 16");                             // release path scratch storage
        }
    }
}

/// `S_IFREG`, the file-type bits `is_file()` accepts.
const S_IFREG: usize = 0x8000;

/// Emits `is_file()`'s regular-file test over a wrapper's mode, with the filesystem fallback.
pub(super) fn emit_url_stat_regular_file_predicate(ctx: &mut FunctionContext<'_>) {
    emit_url_stat_type_predicate_or_fallback(ctx, "__rt_is_file", S_IFREG, URL_STAT_FLAGS_EXISTS);
}

/// Emits an `S_IFMT` predicate over a wrapper's mode, with a native filesystem fallback.
///
/// Only the wrapper route needs the mask: the native helpers already answer a boolean. A failed
/// wrapper stat leaves a zero mode here, and zero matches no file type php names, so the absent
/// path falls out as false without a second test.
fn emit_url_stat_type_predicate_or_fallback(
    ctx: &mut FunctionContext<'_>,
    fallback_runtime: &str,
    file_type_bits: usize,
    flags: usize,
) {
    emit_url_stat_int_or_fallback(ctx, fallback_runtime, STAT_FIELD_MODE, flags);
    let no_wrapper = ctx.next_label("url_stat_pred_native");
    let done = ctx.next_label("url_stat_pred_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_url_stat_matched");
            ctx.emitter.instruction("ldrb w9, [x9]");                           // read whether the mode came from a wrapper
            ctx.emitter.instruction(&format!("cbz w9, {}", no_wrapper));        // the native fallback already returned a boolean
            ctx.emitter.instruction("and x0, x0, #0xF000");                     // isolate the mode's file-type bits
            ctx.emitter.instruction(&format!("mov x9, #{:#x}", file_type_bits)); // the type php accepts here
            ctx.emitter.instruction("cmp x0, x9");
            ctx.emitter.instruction("cset x0, eq");                             // true only for that file type
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the native-result path
            ctx.emitter.label(&no_wrapper);
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_url_stat_matched");
            ctx.emitter.instruction("movzx r9d, BYTE PTR [r9]");                // read whether the mode came from a wrapper
            ctx.emitter.instruction("test r9d, r9d");                           // test the url_stat matched flag
            ctx.emitter.instruction(&format!("jz {}", no_wrapper));             // the native fallback already returned a boolean
            ctx.emitter.instruction("and eax, 0xF000");                         // isolate the mode's file-type bits
            ctx.emitter.instruction(&format!("cmp eax, {:#x}", file_type_bits)); // the type php accepts here
            ctx.emitter.instruction("sete al");                                 // true only for that file type
            ctx.emitter.instruction("movzx eax, al");                           // widen the boolean into the result register
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the native-result path
            ctx.emitter.label(&no_wrapper);
            ctx.emitter.label(&done);
        }
    }
}

/// Emits a wrapper permission check with a native `access(2)` fallback.
fn emit_url_stat_access_or_fallback(
    ctx: &mut FunctionContext<'_>,
    fallback_runtime: &str,
    which: usize,
) {
    let fallback = ctx.next_label("url_stat_access_fs");
    let done = ctx.next_label("url_stat_access_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve path scratch storage across url_stat
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer for the native check
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length for the native check
            ctx.emitter.instruction("mov x0, x1");                              // pass the path pointer to the access check
            ctx.emitter.instruction("mov x1, x2");                              // pass the path length to the access check
            ctx.emitter.instruction(&format!("mov x2, #{}", which));            // read / write / execute
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat_access");
            abi::emit_symbol_address(ctx.emitter, "x9", "_url_stat_matched");
            ctx.emitter.instruction("ldrb w9, [x9]");                           // read whether a registered wrapper scheme matched
            ctx.emitter.instruction(&format!("cbz w9, {}", fallback));          // fall back to access(2) when no wrapper matched
            ctx.emitter.instruction(&format!("b {}", done));                    // keep the wrapper's answer
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // restore the path pointer for the native check
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // restore the path length for the native check
            abi::emit_call_label(ctx.emitter, fallback_runtime);
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add sp, sp, #16");                         // release path scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve path scratch storage across url_stat
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer for the native check
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length for the native check
            ctx.emitter.instruction("mov rdi, rax");                            // pass the path pointer to the access check
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the path length to the access check
            ctx.emitter.instruction(&format!("mov rdx, {}", which));            // read / write / execute
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat_access");
            abi::emit_symbol_address(ctx.emitter, "r9", "_url_stat_matched");
            ctx.emitter.instruction("movzx r9d, BYTE PTR [r9]");                // read whether a registered wrapper scheme matched
            ctx.emitter.instruction("test r9d, r9d");                           // test the url_stat matched flag
            ctx.emitter.instruction(&format!("jz {}", fallback));               // fall back to access(2) when no wrapper matched
            ctx.emitter.instruction(&format!("jmp {}", done));                  // keep the wrapper's answer
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the path pointer for the native check
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // restore the path length for the native check
            abi::emit_call_label(ctx.emitter, fallback_runtime);
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add rsp, 16");                             // release path scratch storage
        }
    }
}

/// Emits `filetype()`'s wrapper route, leaving the borrowed name in the string pair.
fn emit_url_stat_name_or_fallback(ctx: &mut FunctionContext<'_>, flags: usize) {
    let fallback = ctx.next_label("url_stat_name_fs");
    let done = ctx.next_label("url_stat_name_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve path scratch storage across url_stat
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer for the native lstat
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length for the native lstat
            ctx.emitter.instruction("mov x0, x1");                              // pass the path pointer to the type reader
            ctx.emitter.instruction("mov x1, x2");                              // pass the path length to the type reader
            ctx.emitter.instruction(&format!("mov x2, #{}", flags));            // the url_stat flags php passes filetype()
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat_type");
            abi::emit_symbol_address(ctx.emitter, "x9", "_url_stat_matched");
            ctx.emitter.instruction("ldrb w9, [x9]");                           // read whether a registered wrapper scheme matched
            ctx.emitter.instruction(&format!("cbz w9, {}", fallback));          // fall back to the native lstat when no wrapper matched
            ctx.emitter.instruction(&format!("b {}", done));                    // keep the wrapper's type name
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // restore the path pointer for the native lstat
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // restore the path length for the native lstat
            abi::emit_call_label(ctx.emitter, "__rt_filetype");
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add sp, sp, #16");                         // release path scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve path scratch storage across url_stat
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer for the native lstat
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length for the native lstat
            ctx.emitter.instruction("mov rdi, rax");                            // pass the path pointer to the type reader
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the path length to the type reader
            ctx.emitter.instruction(&format!("mov rdx, {}", flags));            // the url_stat flags php passes filetype()
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat_type");
            abi::emit_symbol_address(ctx.emitter, "r9", "_url_stat_matched");
            ctx.emitter.instruction("movzx r9d, BYTE PTR [r9]");                // read whether a registered wrapper scheme matched
            ctx.emitter.instruction("test r9d, r9d");                           // test the url_stat matched flag
            ctx.emitter.instruction(&format!("jz {}", fallback));               // fall back to the native lstat when no wrapper matched
            ctx.emitter.instruction(&format!("jmp {}", done));                  // keep the wrapper's type name
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the path pointer for the native lstat
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // restore the path length for the native lstat
            abi::emit_call_label(ctx.emitter, "__rt_filetype");
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add rsp, 16");                             // release path scratch storage
        }
    }
}

/// Emits `stat()`/`lstat()`'s wrapper route, leaving the hash pointer the array boxer reads.
fn emit_url_stat_array_or_fallback(
    ctx: &mut FunctionContext<'_>,
    fallback_runtime: &str,
    flags: usize,
) {
    let fallback = ctx.next_label("url_stat_array_fs");
    let done = ctx.next_label("url_stat_array_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve path scratch storage across url_stat
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer for the native stat
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length for the native stat
            ctx.emitter.instruction("mov x0, x1");                              // pass the path pointer to the array builder
            ctx.emitter.instruction("mov x1, x2");                              // pass the path length to the array builder
            ctx.emitter.instruction(&format!("mov x2, #{}", flags));            // the url_stat flags php passes this caller
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_stat_array");
            abi::emit_symbol_address(ctx.emitter, "x9", "_url_stat_matched");
            ctx.emitter.instruction("ldrb w9, [x9]");                           // read whether a registered wrapper scheme matched
            ctx.emitter.instruction(&format!("cbz w9, {}", fallback));          // fall back to the native stat when no wrapper matched
            ctx.emitter.instruction(&format!("b {}", done));                    // keep the rebuilt array
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // restore the path pointer for the native stat
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // restore the path length for the native stat
            abi::emit_call_label(ctx.emitter, fallback_runtime);
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add sp, sp, #16");                         // release path scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve path scratch storage across url_stat
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer for the native stat
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length for the native stat
            ctx.emitter.instruction("mov rdi, rax");                            // pass the path pointer to the array builder
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the path length to the array builder
            ctx.emitter.instruction(&format!("mov rdx, {}", flags));            // the url_stat flags php passes this caller
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_stat_array");
            abi::emit_symbol_address(ctx.emitter, "r9", "_url_stat_matched");
            ctx.emitter.instruction("movzx r9d, BYTE PTR [r9]");                // read whether a registered wrapper scheme matched
            ctx.emitter.instruction("test r9d, r9d");                           // test the url_stat matched flag
            ctx.emitter.instruction(&format!("jz {}", fallback));               // fall back to the native stat when no wrapper matched
            ctx.emitter.instruction(&format!("jmp {}", done));                  // keep the rebuilt array
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the path pointer for the native stat
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // restore the path length for the native stat
            abi::emit_call_label(ctx.emitter, fallback_runtime);
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add rsp, 16");                             // release path scratch storage
        }
    }
}

/// Where a stat reader's failure signal lives, which differs per result shape.
#[derive(Clone, Copy)]
pub(super) enum StatResultShape {
    /// `(value, success)` — AArch64 x0/x1, x86_64 rax/rdx.
    IntPair,
    /// A borrowed `(ptr, len)` name, null on failure — AArch64 x1/x2, x86_64 rax/rdx.
    Name,
    /// A hash pointer, null on failure — AArch64 x0, x86_64 rax.
    Array,
}

/// Opens the scratch frame the failure diagnostic reads the path back out of.
///
/// The path is still in the string pair when a stat lowering reaches here, and the reader is about
/// to clobber it, so it is staged rather than recomputed.
pub(super) fn emit_stat_scratch_open(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #48");                         // hold the path and all three result registers
            ctx.emitter.instruction("str x1, [sp, #0]");                        // the path pointer
            ctx.emitter.instruction("str x2, [sp, #8]");                        // the path length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 48");                             // hold the path and both result registers
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // the path pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // the path length
        }
    }
}

/// Closes the scratch frame [`emit_stat_scratch_open`] opened.
pub(super) fn emit_stat_scratch_close(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("add sp, sp, #48"),            // release the diagnostic frame
        Arch::X86_64 => ctx.emitter.instruction("add rsp, 48"),                 // release the diagnostic frame
    }
}

/// Prints php's SECOND line — `<caller>(): stat failed for <path>` — when the stat failed.
///
/// php emits it for ANY failure, not just a wrapper's: an absent ordinary file gets it as much as a
/// wrapper without `url_stat()`, which gets it AFTER the missing-hook line. The message is composed
/// from the caller's own `_uwmh_head_*` symbol so the eleven readers that print it share one tail
/// instead of carrying eleven near-identical literals. The PREDICATES print nothing at all —
/// `file_exists()`, `is_file()`, `is_dir()`, `is_link()` and the three access checks all fail
/// silently, measured on php 8.5.6.
///
/// Expects [`emit_stat_scratch_open`]'s frame, with the reader's result still in its registers.
pub(super) fn emit_stat_failed_warning(
    ctx: &mut FunctionContext<'_>,
    head_symbol: &str,
    head_len: usize,
    tail_symbol: &str,
    tail_len: usize,
    shape: StatResultShape,
) {
    let ok = ctx.next_label("stat_failed_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            match shape {
                StatResultShape::IntPair | StatResultShape::Name => {
                    ctx.emitter.instruction(&format!("cbnz x1, {}", ok));       // a successful stat says nothing
                }
                StatResultShape::Array => {
                    ctx.emitter.instruction(&format!("cbnz x0, {}", ok));       // a successful stat says nothing
                }
            }
            ctx.emitter.instruction("stp x0, x1, [sp, #16]");                   // preserve the result across the diagnostic
            ctx.emitter.instruction("str x2, [sp, #32]");                       // the name shape's length half rides in x2
            abi::emit_symbol_address(ctx.emitter, "x1", head_symbol);           // "Warning: <caller>(): "
            ctx.emitter.instruction(&format!("mov x2, #{}", head_len));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");             // honours @ and the filter scope
            abi::emit_symbol_address(ctx.emitter, "x1", tail_symbol);           // "stat failed for " / "Lstat failed for "
            ctx.emitter.instruction(&format!("mov x2, #{}", tail_len));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // the path php names
            ctx.emitter.instruction("ldr x2, [sp, #8]");
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "x1", "_diag_newline");
            ctx.emitter.instruction("mov x2, #1");
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.instruction("ldp x0, x1, [sp, #16]");                   // restore the result
            ctx.emitter.instruction("ldr x2, [sp, #32]");                       // restore the name shape's length half
            ctx.emitter.label(&ok);
        }
        Arch::X86_64 => {
            match shape {
                StatResultShape::IntPair => {
                    ctx.emitter.instruction("test rdx, rdx");                   // a successful stat says nothing
                }
                StatResultShape::Name | StatResultShape::Array => {
                    ctx.emitter.instruction("test rax, rax");                   // a successful stat says nothing
                }
            }
            ctx.emitter.instruction(&format!("jnz {}", ok));
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the result across the diagnostic
            ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rdx");
            abi::emit_symbol_address(ctx.emitter, "rdi", head_symbol);          // "Warning: <caller>(): "
            ctx.emitter.instruction(&format!("mov rsi, {}", head_len));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");             // honours @ and the filter scope
            abi::emit_symbol_address(ctx.emitter, "rdi", tail_symbol);          // "stat failed for " / "Lstat failed for "
            ctx.emitter.instruction(&format!("mov rsi, {}", tail_len));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // the path php names
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "rdi", "_diag_newline");
            ctx.emitter.instruction("mov rsi, 1");
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // restore the result
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");
            ctx.emitter.label(&ok);
        }
    }
}

/// Lowers one of the `int|false` stat readers through a wrapper's `url_stat()` first.
///
/// `head_symbol`/`head_len` name the caller in the "`Class::url_stat` is not implemented!" warning
/// the shared dispatcher prints, which is why every builtin carries its own pair.
pub(super) fn lower_stat_int_field_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    head_symbol: &str,
    head_len: usize,
    fallback_runtime: &str,
    field_selector: usize,
    flags: usize,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    emit_publish_missing_hook_message(
        ctx,
        head_symbol,
        head_len,
        "_uwmh_tail_url_stat",
        WRAPPER_MISSING_HOOK_TAIL_URL_STAT.len(),
    );
    load_string_to_result(ctx, path, name)?;
    emit_stat_scratch_open(ctx);
    emit_url_stat_int_or_fallback(ctx, fallback_runtime, field_selector, flags);
    emit_stat_failed_warning(
        ctx,
        head_symbol,
        head_len,
        "_stat_failed_tail",
        STAT_FAILED_TAIL.len(),
        StatResultShape::IntPair,
    );
    emit_stat_scratch_close(ctx);
    box_stat_int_or_false_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers an `S_IFMT` predicate (`is_dir()`, `is_link()`) through a wrapper's `url_stat()` first.
fn lower_stat_type_predicate_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    head_symbol: &str,
    head_len: usize,
    fallback_runtime: &str,
    file_type_bits: usize,
    flags: usize,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    emit_publish_missing_hook_message(
        ctx,
        head_symbol,
        head_len,
        "_uwmh_tail_url_stat",
        WRAPPER_MISSING_HOOK_TAIL_URL_STAT.len(),
    );
    load_string_to_result(ctx, path, name)?;
    emit_url_stat_type_predicate_or_fallback(ctx, fallback_runtime, file_type_bits, flags);
    store_if_result(ctx, inst)
}

/// Lowers a permission predicate through a wrapper's `url_stat()` first.
fn lower_stat_access_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    head_symbol: &str,
    head_len: usize,
    fallback_runtime: &str,
    which: usize,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    emit_publish_missing_hook_message(
        ctx,
        head_symbol,
        head_len,
        "_uwmh_tail_url_stat",
        WRAPPER_MISSING_HOOK_TAIL_URL_STAT.len(),
    );
    load_string_to_result(ctx, path, name)?;
    emit_url_stat_access_or_fallback(ctx, fallback_runtime, which);
    store_if_result(ctx, inst)
}

/// Lowers `is_dir()` through a userspace `url_stat()`'s mode before the filesystem.
pub(crate) fn lower_is_dir_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_type_predicate_with_wrapper(
        ctx,
        inst,
        "is_dir",
        "_uwmh_head_is_dir",
        WRAPPER_MISSING_HOOK_HEAD_IS_DIR.len(),
        "__rt_is_dir",
        S_IFDIR,
        URL_STAT_FLAGS_EXISTS,
    )
}

/// Lowers `is_link()` through a userspace `url_stat()`'s mode before the filesystem.
pub(crate) fn lower_is_link_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_type_predicate_with_wrapper(
        ctx,
        inst,
        "is_link",
        "_uwmh_head_is_link",
        WRAPPER_MISSING_HOOK_HEAD_IS_LINK.len(),
        "__rt_is_link",
        S_IFLNK,
        URL_STAT_FLAGS_LINK_EXISTS,
    )
}

/// Lowers `is_readable()` through a userspace `url_stat()`'s triad before `access(2)`.
pub(crate) fn lower_is_readable_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_access_with_wrapper(
        ctx,
        inst,
        "is_readable",
        "_uwmh_head_is_readable",
        WRAPPER_MISSING_HOOK_HEAD_IS_READABLE.len(),
        "__rt_is_readable",
        ACCESS_READ,
    )
}

/// Lowers `is_writable()` through a userspace `url_stat()`'s triad before `access(2)`.
pub(crate) fn lower_is_writable_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_access_with_wrapper(
        ctx,
        inst,
        "is_writable",
        "_uwmh_head_is_writable",
        WRAPPER_MISSING_HOOK_HEAD_IS_WRITABLE.len(),
        "__rt_is_writable",
        ACCESS_WRITE,
    )
}

/// Lowers `is_writeable()`, php's alias — which names ITSELF in the missing-hook warning.
pub(crate) fn lower_is_writeable_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_access_with_wrapper(
        ctx,
        inst,
        "is_writeable",
        "_uwmh_head_is_writeable",
        WRAPPER_MISSING_HOOK_HEAD_IS_WRITEABLE.len(),
        "__rt_is_writable",
        ACCESS_WRITE,
    )
}

/// Lowers `is_executable()` through a userspace `url_stat()`'s triad before `access(2)`.
pub(crate) fn lower_is_executable_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_access_with_wrapper(
        ctx,
        inst,
        "is_executable",
        "_uwmh_head_is_executable",
        WRAPPER_MISSING_HOOK_HEAD_IS_EXECUTABLE.len(),
        "__rt_is_executable",
        ACCESS_EXECUTE,
    )
}

/// Lowers `filemtime()` through a userspace `url_stat()['mtime']` before the filesystem.
pub(crate) fn lower_filemtime_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_int_field_with_wrapper(
        ctx,
        inst,
        "filemtime",
        "_uwmh_head_filemtime",
        WRAPPER_MISSING_HOOK_HEAD_FILEMTIME.len(),
        "__rt_filemtime",
        STAT_FIELD_MTIME,
        URL_STAT_FLAGS_VALUE,
    )
}

/// Lowers `fileatime()` through a userspace `url_stat()['atime']` before the filesystem.
pub(crate) fn lower_fileatime_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_int_field_with_wrapper(
        ctx,
        inst,
        "fileatime",
        "_uwmh_head_fileatime",
        WRAPPER_MISSING_HOOK_HEAD_FILEATIME.len(),
        "__rt_fileatime",
        STAT_FIELD_ATIME,
        URL_STAT_FLAGS_VALUE,
    )
}

/// Lowers `filectime()` through a userspace `url_stat()['ctime']` before the filesystem.
pub(crate) fn lower_filectime_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_int_field_with_wrapper(
        ctx,
        inst,
        "filectime",
        "_uwmh_head_filectime",
        WRAPPER_MISSING_HOOK_HEAD_FILECTIME.len(),
        "__rt_filectime",
        STAT_FIELD_CTIME,
        URL_STAT_FLAGS_VALUE,
    )
}

/// Lowers `fileperms()` through a userspace `url_stat()['mode']` before the filesystem.
///
/// php answers the WHOLE mode, file-type bits included — `0100644` measures as `33188`, not
/// `0644` — so nothing masks it here.
pub(crate) fn lower_fileperms_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_int_field_with_wrapper(
        ctx,
        inst,
        "fileperms",
        "_uwmh_head_fileperms",
        WRAPPER_MISSING_HOOK_HEAD_FILEPERMS.len(),
        "__rt_fileperms",
        STAT_FIELD_MODE,
        URL_STAT_FLAGS_VALUE,
    )
}

/// Lowers `fileowner()` through a userspace `url_stat()['uid']` before the filesystem.
pub(crate) fn lower_fileowner_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_int_field_with_wrapper(
        ctx,
        inst,
        "fileowner",
        "_uwmh_head_fileowner",
        WRAPPER_MISSING_HOOK_HEAD_FILEOWNER.len(),
        "__rt_fileowner",
        STAT_FIELD_UID,
        URL_STAT_FLAGS_VALUE,
    )
}

/// Lowers `filegroup()` through a userspace `url_stat()['gid']` before the filesystem.
pub(crate) fn lower_filegroup_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_int_field_with_wrapper(
        ctx,
        inst,
        "filegroup",
        "_uwmh_head_filegroup",
        WRAPPER_MISSING_HOOK_HEAD_FILEGROUP.len(),
        "__rt_filegroup",
        STAT_FIELD_GID,
        URL_STAT_FLAGS_VALUE,
    )
}

/// Lowers `fileinode()` through a userspace `url_stat()['ino']` before the filesystem.
pub(crate) fn lower_fileinode_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_int_field_with_wrapper(
        ctx,
        inst,
        "fileinode",
        "_uwmh_head_fileinode",
        WRAPPER_MISSING_HOOK_HEAD_FILEINODE.len(),
        "__rt_fileinode",
        STAT_FIELD_INO,
        URL_STAT_FLAGS_VALUE,
    )
}

/// Lowers `filetype()` through a userspace `url_stat()['mode']` before the filesystem.
pub(crate) fn lower_filetype_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "filetype", 1)?;
    let path = expect_operand(inst, 0)?;
    emit_publish_missing_hook_message(
        ctx,
        "_uwmh_head_filetype",
        WRAPPER_MISSING_HOOK_HEAD_FILETYPE.len(),
        "_uwmh_tail_url_stat",
        WRAPPER_MISSING_HOOK_TAIL_URL_STAT.len(),
    );
    load_string_to_result(ctx, path, "filetype")?;
    emit_stat_scratch_open(ctx);
    emit_url_stat_name_or_fallback(ctx, URL_STAT_FLAGS_LINK_VALUE);
    emit_stat_failed_warning(
        ctx,
        "_uwmh_head_filetype",
        WRAPPER_MISSING_HOOK_HEAD_FILETYPE.len(),
        "_lstat_failed_tail",
        LSTAT_FAILED_TAIL.len(),
        StatResultShape::Name,
    );
    emit_stat_scratch_close(ctx);
    box_stat_string_or_false_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `stat()` through a userspace `url_stat()` before the filesystem.
pub(crate) fn lower_stat_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_array_with_wrapper(
        ctx,
        inst,
        "stat",
        "_uwmh_head_stat",
        WRAPPER_MISSING_HOOK_HEAD_STAT.len(),
        "__rt_stat_array",
        URL_STAT_FLAGS_VALUE,
        "_stat_failed_tail",
        STAT_FAILED_TAIL.len(),
    )
}

/// Lowers `lstat()` through a userspace `url_stat()` before the filesystem.
pub(crate) fn lower_lstat_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stat_array_with_wrapper(
        ctx,
        inst,
        "lstat",
        "_uwmh_head_lstat",
        WRAPPER_MISSING_HOOK_HEAD_LSTAT.len(),
        "__rt_lstat_array",
        URL_STAT_FLAGS_LINK_VALUE,
        "_lstat_failed_tail",
        LSTAT_FAILED_TAIL.len(),
    )
}

/// Shared body of `stat()`/`lstat()`: the flags and the native fallback are all that differ.
fn lower_stat_array_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    head_symbol: &str,
    head_len: usize,
    fallback_runtime: &str,
    flags: usize,
    tail_symbol: &str,
    tail_len: usize,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    emit_publish_missing_hook_message(
        ctx,
        head_symbol,
        head_len,
        "_uwmh_tail_url_stat",
        WRAPPER_MISSING_HOOK_TAIL_URL_STAT.len(),
    );
    load_string_to_result(ctx, path, name)?;
    emit_stat_scratch_open(ctx);
    emit_url_stat_array_or_fallback(ctx, fallback_runtime, flags);
    emit_stat_failed_warning(ctx, head_symbol, head_len, tail_symbol, tail_len, StatResultShape::Array);
    emit_stat_scratch_close(ctx);
    box_stat_array_or_false_result(ctx);
    store_if_result(ctx, inst)
}
