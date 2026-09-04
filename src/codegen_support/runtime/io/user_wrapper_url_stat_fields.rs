//! Purpose:
//! Emits the userspace-wrapper `url_stat()` READERS the whole stat family shares:
//! `__rt_user_wrapper_url_stat_int` (one integer field), `__rt_user_wrapper_url_stat_type`
//! (`filetype()`'s name for the mode) and `__rt_user_wrapper_stat_array` (`stat()`/`lstat()`'s
//! 26-entry array). All three sit on top of `__rt_user_wrapper_url_stat`, so a wrapper's method
//! runs exactly once per builtin call and the missing-hook warning keeps naming the caller.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The stat-family builtin lowerings in
//!   `crate::codegen::lower_inst::builtins::io::wrapper_dispatch`, which branch on
//!   `_url_stat_matched` to choose between these and the real filesystem.
//!
//! Key details:
//! - php reads the wrapper's array by STRING key only and leaves every ABSENT field at zero
//!   (`statbuf_from_array` in `main/streams/userspace.c` zeroes the buffer first), so a wrapper
//!   answering `['mode' => 0100644]` gives `filesize()` int(0), not false. Measured on php 8.5.6.
//!   Only `url_stat()` answering `false` is a FAILED stat, and that alone is what the found-flag
//!   reports — conflating the two would turn php's `int(0)` into `bool(false)`.
//! - Nothing here hands the wrapper's own array back: `stat()` rebuilds php's canonical 13 numeric
//!   plus 13 string entries from the fields it could read, which is why a one-key wrapper array
//!   still measures as `array(26)` on php 8.5.6.
//! - The key selector is the numeric position php documents (0 = dev … 12 = blocks), so one index
//!   names the numeric key and picks the string key.

use crate::codegen_support::runtime::data::{FILETYPE_UNKNOWN_HEAD, FILETYPE_UNKNOWN_TAIL};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// php's stat fields in documented order: `(string key symbol, key length)`.
///
/// The index into this table IS the numeric key php stores the field under as well. php fills every
/// NUMERIC key first and only then the named ones — MEASURED — so `__rt_user_wrapper_stat_array`
/// walks the table TWICE rather than inserting both halves of an entry together.
const STAT_FIELDS: &[(&str, usize)] = &[
    ("_stat_key_dev", 3),
    ("_stat_key_ino", 3),
    ("_stat_key_mode", 4),
    ("_stat_key_nlink", 5),
    ("_stat_key_uid", 3),
    ("_stat_key_gid", 3),
    ("_stat_key_rdev", 4),
    ("_stat_key_size", 4),
    ("_stat_key_atime", 5),
    ("_stat_key_mtime", 5),
    ("_stat_key_ctime", 5),
    ("_stat_key_blksize", 7),
    ("_stat_key_blocks", 6),
];

/// Selector for `ino`, which `fileinode()` reads.
pub(crate) const STAT_FIELD_INO: usize = 1;

/// Selector for `mode`, which every type predicate and `fileperms()` read.
pub(crate) const STAT_FIELD_MODE: usize = 2;

/// Selector for `uid`, which `fileowner()` reads.
pub(crate) const STAT_FIELD_UID: usize = 4;

/// Selector for `gid`, which `filegroup()` reads.
pub(crate) const STAT_FIELD_GID: usize = 5;

/// Selector for `size`, which `filesize()` reads.
pub(crate) const STAT_FIELD_SIZE: usize = 7;

/// Selector for `atime`, which `fileatime()` reads.
pub(crate) const STAT_FIELD_ATIME: usize = 8;

/// Selector for `mtime`, which `filemtime()` reads.
pub(crate) const STAT_FIELD_MTIME: usize = 9;

/// Selector for `ctime`, which `filectime()` reads.
pub(crate) const STAT_FIELD_CTIME: usize = 10;

/// `filetype()`'s answers, keyed by the `S_IFMT` bits of the mode.
const FILETYPE_NAMES: &[(u32, &str, usize)] = &[
    (0x8000, "_filetype_file", 4),
    (0x4000, "_filetype_dir", 3),
    (0xA000, "_filetype_link", 4),
    (0x2000, "_filetype_char", 4),
    (0x6000, "_filetype_block", 5),
    (0x1000, "_filetype_fifo", 4),
    (0xC000, "_filetype_socket", 6),
];

/// The name php falls back to for a mode it does not recognize — including the zero mode a
/// wrapper leaves behind when its array names no `mode` at all. Measured on php 8.5.6.
const FILETYPE_UNKNOWN: (&str, usize) = ("_filetype_unknown", 7);

/// `PHP_STREAM_URL_STAT_QUIET | PHP_STREAM_URL_STAT_NOCACHE`, the flags php hands `url_stat()`
/// for `is_readable()`/`is_writable()`/`is_executable()`. Measured on php 8.5.6.
const ACCESS_STAT_FLAGS: usize = 6;

/// How many supplementary groups the access check will look at.
///
/// `NGROUPS_MAX` is 16 on Darwin and 65536 on Linux, but a process with more groups than this
/// simply falls through to php's "other" triad rather than reading past the buffer.
const ACCESS_GROUP_SLOTS: usize = 64;

/// Owner triad base bit `S_IRUSR`; `>> which` walks it to write and execute.
const ACCESS_OWNER_BASE: usize = 0o400;

/// Group triad base bit `S_IRGRP`.
const ACCESS_GROUP_BASE: usize = 0o40;

/// World triad base bit `S_IROTH`.
const ACCESS_OTHER_BASE: usize = 0o4;

/// Emits every `url_stat()` reader the stat family shares. Dispatches by target.
pub fn emit_user_wrapper_url_stat_readers(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_field_reader_x86_64(emitter);
        emit_int_reader_x86_64(emitter);
        emit_type_reader_x86_64(emitter);
        emit_stat_array_x86_64(emitter);
        emit_access_x86_64(emitter);
        return;
    }
    emit_field_reader_aarch64(emitter);
    emit_int_reader_aarch64(emitter);
    emit_type_reader_aarch64(emitter);
    emit_stat_array_aarch64(emitter);
    emit_access_aarch64(emitter);
}

// ---------------------------------------------------------------------------
// __rt_uws_field: read one integer field out of an ALREADY boxed stat array
// ---------------------------------------------------------------------------

/// AArch64 `__rt_uws_field(box, selector)`: x0 = boxed Mixed stat array, x1 = field selector.
///
/// Returns the field in x0, or 0 when the wrapper's array does not name it — php's own default,
/// because its statbuf starts zeroed. The caller keeps ownership of the box.
fn emit_field_reader_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // 4-byte alignment for the helper entry
    emitter.comment("--- runtime: user_wrapper url_stat field reader ---");
    emitter.label_global("__rt_uws_field");
    // Frame: [sp,#0..16] x29/x30, [sp,#16] the stat box, [sp,#24] the extracted value.
    emitter.instruction("sub sp, sp, #48");                                     // reader frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the reader frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // hold the caller's stat box across the lookup
    for (index, (symbol, length)) in STAT_FIELDS.iter().enumerate() {
        let next = format!("__rt_uwsf_k{}", index + 1);
        emitter.instruction(&format!("cmp x1, #{}", index));                    // is this the selected field?
        emitter.instruction(&format!("b.ne {}", next));                         // no — try the next key
        abi::emit_symbol_address(emitter, "x1", symbol);                        // the key php stores the field under
        emitter.instruction(&format!("mov x2, #{}", length));
        emitter.instruction("b __rt_uwsf_havekey");                             // key chosen
        emitter.label(&next);
    }
    // No lowering passes an out-of-range selector, but a wrong answer would be silent; naming no
    // key reads as "absent", which php answers with zero.
    emitter.instruction("mov x0, #0");                                          // unknown selector → php's zero default
    emitter.instruction("b __rt_uwsf_ret");
    emitter.label("__rt_uwsf_havekey");
    emitter.instruction("bl __rt_hash_normalize_key");                          // normalize → key_lo/key_hi in x1/x2
    emitter.instruction("ldr x0, [sp, #16]");                                   // the stat box is the reader's receiver
    emitter.instruction("mov x3, xzr");                                         // an absent stat field is php's zero, not a warning
    emitter.instruction("bl __rt_mixed_array_get");                             // x0 = boxed value (Mixed null on miss)
    emitter.instruction("mov x10, x0");                                         // keep the value box for release
    emitter.instruction("ldr x9, [x0]");                                        // value runtime tag
    emitter.instruction("cmp x9, #0");                                          // integer?
    emitter.instruction("b.ne __rt_uwsf_zero");                                 // absent or non-integer → zero
    emitter.instruction("ldr x9, [x0, #8]");                                    // the integer payload
    emitter.instruction("b __rt_uwsf_have");
    emitter.label("__rt_uwsf_zero");
    emitter.instruction("mov x9, #0");                                          // php leaves unnamed fields at zero
    emitter.label("__rt_uwsf_have");
    emitter.instruction("str x9, [sp, #24]");                                   // stash the value across the release
    emitter.instruction("mov x0, x10");                                         // the value box
    emitter.instruction("bl __rt_decref_any");                                  // release it; the stat box stays the caller's
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the value
    emitter.label("__rt_uwsf_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the reader frame
    emitter.instruction("ret");                                                 // return the integer field
}

/// x86_64 counterpart of [`emit_field_reader_aarch64`]: rdi = box, rsi = selector, rax = field.
fn emit_field_reader_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper url_stat field reader ---");
    emitter.label_global("__rt_uws_field");
    // Frame: [rbp-8] the stat box, [rbp-16] the extracted value.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the reader frame pointer
    emitter.instruction("sub rsp, 32");                                         // spill slots for the box and the value
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // hold the caller's stat box across the lookup
    for (index, (symbol, length)) in STAT_FIELDS.iter().enumerate() {
        let next = format!("__rt_uwsf_k{}_x86", index + 1);
        emitter.instruction(&format!("cmp rsi, {}", index));                    // is this the selected field?
        emitter.instruction(&format!("jne {}", next));                          // no — try the next key
        abi::emit_symbol_address(emitter, "rax", symbol);                       // normalize reads the rax/rdx string pair
        emitter.instruction(&format!("mov rdx, {}", length));
        emitter.instruction("jmp __rt_uwsf_havekey_x86");                       // key chosen
        emitter.label(&next);
    }
    emitter.instruction("xor eax, eax");                                        // unknown selector → php's zero default
    emitter.instruction("jmp __rt_uwsf_ret_x86");
    emitter.label("__rt_uwsf_havekey_x86");
    emitter.instruction("call __rt_hash_normalize_key");                        // key_lo in rax, key_hi in rdx
    emitter.instruction("mov rsi, rax");                                        // key_lo → SysV second argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the stat box is the reader's receiver
    emitter.instruction("xor ecx, ecx");                                        // an absent stat field is php's zero, not a warning
    emitter.instruction("call __rt_mixed_array_get");                           // rax = boxed value (Mixed null on miss)
    emitter.instruction("mov r10, rax");                                        // keep the value box for release
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // value runtime tag
    emitter.instruction("test r9, r9");                                         // integer (tag 0)?
    emitter.instruction("jnz __rt_uwsf_zero_x86");                              // absent or non-integer → zero
    emitter.instruction("mov r9, QWORD PTR [rax + 8]");                         // the integer payload
    emitter.instruction("jmp __rt_uwsf_have_x86");
    emitter.label("__rt_uwsf_zero_x86");
    emitter.instruction("xor r9d, r9d");                                        // php leaves unnamed fields at zero
    emitter.label("__rt_uwsf_have_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // stash the value across the release
    emitter.instruction("mov rax, r10");                                        // the value box
    emitter.instruction("call __rt_decref_any");                                // release it; the stat box stays the caller's
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the value
    emitter.label("__rt_uwsf_ret_x86");
    emitter.instruction("add rsp, 32");                                         // release the reader frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the integer field
}

// ---------------------------------------------------------------------------
// __rt_user_wrapper_url_stat_int: one integer field, with a found flag
// ---------------------------------------------------------------------------

/// AArch64 `__rt_user_wrapper_url_stat_int(ptr, len, selector, flags)`.
///
/// Inputs x0/x1 = path, x2 = field selector, x3 = the `url_stat()` flags php passes.
/// Outputs x0 = the field, x1 = 1 when the stat SUCCEEDED — the pair
/// `box_stat_int_or_false_result` already reads. A wrapper answering `false`, or one with no
/// `url_stat()` at all, reports 0 so the caller boxes php's false; an array that simply does not
/// name the field reports 1 with a zero value, which is what php answers.
fn emit_int_reader_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // 4-byte alignment for the helper entry
    emitter.comment("--- runtime: user_wrapper_url_stat_int ---");
    emitter.label_global("__rt_user_wrapper_url_stat_int");
    // Frame: [sp,#0..16] x29/x30, [sp,#16] selector, [sp,#24] the stat box, [sp,#32] the value.
    emitter.instruction("sub sp, sp, #48");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x2, [sp, #16]");                                   // hold the field selector across the dispatch
    emitter.instruction("mov x2, x3");                                          // the flags php documents for this caller
    emitter.instruction("bl __rt_user_wrapper_url_stat");                       // x0 = boxed stat array (sets _url_stat_matched)
    emitter.instruction("cbz x0, __rt_uwsi_fail");                              // no wrapper matched — the caller reads the flag
    emitter.instruction("ldr x9, [x0]");                                        // boxed runtime tag
    emitter.instruction("cmp x9, #3");                                          // tag 3 means url_stat() answered false
    emitter.instruction("b.eq __rt_uwsi_fail_box");                             // a failed stat, not a missing field
    emitter.instruction("str x0, [sp, #24]");                                   // hold the stat box across the field read
    emitter.instruction("ldr x1, [sp, #16]");                                   // the field selector
    emitter.instruction("bl __rt_uws_field");                                   // x0 = the integer field (0 when unnamed)
    emitter.instruction("str x0, [sp, #32]");                                   // stash it across the release
    emitter.instruction("ldr x0, [sp, #24]");                                   // the stat box
    emitter.instruction("bl __rt_decref_any");                                  // release it
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the field
    emitter.instruction("mov x1, #1");                                          // the stat succeeded
    emitter.instruction("b __rt_uwsi_ret");
    emitter.label("__rt_uwsi_fail_box");
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed false
    emitter.label("__rt_uwsi_fail");
    emitter.instruction("mov x0, #0");                                          // no value to report
    emitter.instruction("mov x1, #0");                                          // the stat failed → the caller boxes php's false
    emitter.label("__rt_uwsi_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return (value, success)
}

/// x86_64 counterpart of [`emit_int_reader_aarch64`].
///
/// Inputs rdi/rsi = path, rdx = selector, rcx = flags. Outputs rax = the field, rdx = success —
/// the register pair `box_stat_int_or_false_result` reads on this arch.
fn emit_int_reader_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_int ---");
    emitter.label_global("__rt_user_wrapper_url_stat_int");
    // Frame: [rbp-8] selector, [rbp-16] the stat box, [rbp-24] the value.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // spill slots for selector/box/value
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // hold the field selector across the dispatch
    emitter.instruction("mov rdx, rcx");                                        // the flags php documents for this caller
    emitter.instruction("call __rt_user_wrapper_url_stat");                     // rax = boxed stat array (sets _url_stat_matched)
    emitter.instruction("test rax, rax");                                       // no wrapper matched?
    emitter.instruction("jz __rt_uwsi_fail_x86");                               // the caller reads the flag
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // boxed runtime tag
    emitter.instruction("cmp r9, 3");                                           // tag 3 means url_stat() answered false
    emitter.instruction("je __rt_uwsi_fail_box_x86");                           // a failed stat, not a missing field
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // hold the stat box across the field read
    emitter.instruction("mov rdi, rax");                                        // the reader's receiver
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // the field selector
    emitter.instruction("call __rt_uws_field");                                 // rax = the integer field (0 when unnamed)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // stash it across the release
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the stat box
    emitter.instruction("call __rt_decref_any");                                // release it
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the field
    emitter.instruction("mov rdx, 1");                                          // the stat succeeded
    emitter.instruction("jmp __rt_uwsi_ret_x86");
    emitter.label("__rt_uwsi_fail_box_x86");
    emitter.instruction("call __rt_decref_any");                                // release the boxed false (rax)
    emitter.label("__rt_uwsi_fail_x86");
    emitter.instruction("xor eax, eax");                                        // no value to report
    emitter.instruction("xor edx, edx");                                        // the stat failed → the caller boxes php's false
    emitter.label("__rt_uwsi_ret_x86");
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return (value, success)
}

// ---------------------------------------------------------------------------
// __rt_user_wrapper_url_stat_type: filetype()'s name for the wrapper's mode
// ---------------------------------------------------------------------------

/// AArch64 `__rt_user_wrapper_url_stat_type(ptr, len, flags)`.
///
/// Inputs x0/x1 = path, x2 = flags. Outputs the borrowed name in x1/x2, the string pair
/// `box_stat_string_or_false_result` reads, with a null pointer for a failed stat.
fn emit_type_reader_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // 4-byte alignment for the helper entry
    emitter.comment("--- runtime: user_wrapper_url_stat_type ---");
    emitter.label_global("__rt_user_wrapper_url_stat_type");
    emitter.instruction("sub sp, sp, #32");                                     // helper frame plus the notice's mode slot
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("mov x3, x2");                                          // the flags php documents for filetype()
    emitter.instruction(&format!("mov x2, #{}", STAT_FIELD_MODE));              // filetype() reads the mode
    emitter.instruction("bl __rt_user_wrapper_url_stat_int");                   // x0 = mode, x1 = success
    emitter.instruction("cbz x1, __rt_uwst_fail");                              // a failed stat is php's false
    emitter.instruction("and w0, w0, #0xF000");                                 // isolate the S_IFMT bits
    for (index, (bits, symbol, length)) in FILETYPE_NAMES.iter().enumerate() {
        let next = format!("__rt_uwst_t{}", index + 1);
        emitter.instruction(&format!("mov w9, #{:#x}", bits));                  // the file-type bits php names
        emitter.instruction("cmp w0, w9");
        emitter.instruction(&format!("b.ne {}", next));                         // not this one — try the next
        abi::emit_symbol_address(emitter, "x1", symbol);
        emitter.instruction(&format!("mov x2, #{}", length));
        emitter.instruction("b __rt_uwst_ret");                                 // php stops at the first match
        emitter.label(&next);
    }
    // php says so out loud before answering "unknown"; the number is the MASKED mode.
    emitter.instruction("str x0, [sp, #16]");                                   // hold the masked mode across the notice
    abi::emit_symbol_address(emitter, "x1", "_filetype_unknown_head");
    emitter.instruction(&format!("mov x2, #{}", FILETYPE_UNKNOWN_HEAD.len()));
    emitter.instruction("bl __rt_diag_warning");                                // honours @ and the filter scope
    emitter.instruction("ldr x0, [sp, #16]");                                   // the masked mode php prints
    emitter.instruction("bl __rt_itoa");                                        // decimal digits into x1/x2
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_filetype_unknown_tail");
    emitter.instruction(&format!("mov x2, #{}", FILETYPE_UNKNOWN_TAIL.len()));
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", FILETYPE_UNKNOWN.0);                // no S_IFMT php names
    emitter.instruction(&format!("mov x2, #{}", FILETYPE_UNKNOWN.1));
    emitter.instruction("b __rt_uwst_ret");
    emitter.label("__rt_uwst_fail");
    emitter.instruction("mov x1, #0");                                          // a null pointer boxes php's false
    emitter.instruction("mov x2, #0");
    emitter.label("__rt_uwst_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the borrowed name
}

/// x86_64 counterpart of [`emit_type_reader_aarch64`]: rdi/rsi = path, rdx = flags; the name comes
/// back in rax/rdx, the pair this arch's string boxer reads.
fn emit_type_reader_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_type ---");
    emitter.label_global("__rt_user_wrapper_url_stat_type");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // keep rsp 16-aligned across the call
    emitter.instruction("mov rcx, rdx");                                        // the flags php documents for filetype()
    emitter.instruction(&format!("mov rdx, {}", STAT_FIELD_MODE));              // filetype() reads the mode
    emitter.instruction("call __rt_user_wrapper_url_stat_int");                 // rax = mode, rdx = success
    emitter.instruction("test rdx, rdx");                                       // a failed stat is php's false
    emitter.instruction("jz __rt_uwst_fail_x86");
    emitter.instruction("and eax, 0xF000");                                     // isolate the S_IFMT bits
    for (index, (bits, symbol, length)) in FILETYPE_NAMES.iter().enumerate() {
        let next = format!("__rt_uwst_t{}_x86", index + 1);
        emitter.instruction(&format!("cmp eax, {:#x}", bits));                  // the file-type bits php names
        emitter.instruction(&format!("jne {}", next));                          // not this one — try the next
        abi::emit_symbol_address(emitter, "rax", symbol);
        emitter.instruction(&format!("mov rdx, {}", length));
        emitter.instruction("jmp __rt_uwst_ret_x86");                           // php stops at the first match
        emitter.label(&next);
    }
    // See the AArch64 counterpart: php names the masked mode before answering "unknown".
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // hold the masked mode across the notice
    abi::emit_symbol_address(emitter, "rdi", "_filetype_unknown_head");
    emitter.instruction(&format!("mov rsi, {}", FILETYPE_UNKNOWN_HEAD.len()));
    emitter.instruction("call __rt_diag_warning");                              // honours @ and the filter scope
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the masked mode php prints
    emitter.instruction("call __rt_itoa");                                      // decimal digits into rax/rdx
    emitter.instruction("mov rdi, rax");                                        // the diagnostic reads the SysV pair
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", "_filetype_unknown_tail");
    emitter.instruction(&format!("mov rsi, {}", FILETYPE_UNKNOWN_TAIL.len()));
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rax", FILETYPE_UNKNOWN.0);               // no S_IFMT php names
    emitter.instruction(&format!("mov rdx, {}", FILETYPE_UNKNOWN.1));
    emitter.instruction("jmp __rt_uwst_ret_x86");
    emitter.label("__rt_uwst_fail_x86");
    emitter.instruction("xor eax, eax");                                        // a null pointer boxes php's false
    emitter.instruction("xor edx, edx");
    emitter.label("__rt_uwst_ret_x86");
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the borrowed name
}

// ---------------------------------------------------------------------------
// __rt_user_wrapper_stat_array: stat()/lstat()'s 26-entry array
// ---------------------------------------------------------------------------

/// AArch64 `__rt_user_wrapper_stat_array(ptr, len, flags)`: x0/x1 = path, x2 = flags.
///
/// Returns the hash pointer `box_stat_array_or_false_result` expects, or 0 for a failed stat.
fn emit_stat_array_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // 4-byte alignment for the helper entry
    emitter.comment("--- runtime: user_wrapper_stat_array ---");
    emitter.label_global("__rt_user_wrapper_stat_array");
    // Frame: [sp,#0..16] x29/x30, [sp,#16] the wrapper's array, [sp,#24] the hash,
    //   [sp,#32] the field value across the two insertions.
    emitter.instruction("sub sp, sp, #48");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("bl __rt_user_wrapper_url_stat");                       // x0 = boxed stat array (sets _url_stat_matched)
    emitter.instruction("cbz x0, __rt_uwsa_fail");                              // no wrapper matched — the caller reads the flag
    emitter.instruction("ldr x9, [x0]");                                        // boxed runtime tag
    emitter.instruction("cmp x9, #3");                                          // tag 3 means url_stat() answered false
    emitter.instruction("b.eq __rt_uwsa_fail_box");                             // a failed stat
    emitter.instruction("str x0, [sp, #16]");                                   // hold the wrapper's array across the rebuild
    emitter.instruction("mov x0, #32");                                         // capacity 32 leaves the 13 fields a low load factor
    emitter.instruction("mov x1, #0");                                          // value type = Int
    emitter.instruction("bl __rt_hash_new");
    emitter.instruction("str x0, [sp, #24]");                                   // the hash under construction
    // Every NUMERIC key first, then every named one: php's order, and not the pairwise one the
    // field list reads like. Each pass re-reads the field, because reading it is a CALL and
    // holding thirteen values across the first pass would cost a frame to save nothing.
    for index in 0..STAT_FIELDS.len() {
        emitter.instruction("ldr x0, [sp, #16]");                               // the wrapper's array
        emitter.instruction(&format!("mov x1, #{}", index));                    // the field to read
        emitter.instruction("bl __rt_uws_field");                               // x0 = the integer (0 when unnamed)
        emitter.instruction("str x0, [sp, #32]");                               // hold it across the insertion
        emitter.instruction("ldr x0, [sp, #24]");                               // the hash
        emitter.instruction(&format!("mov x1, #{}", index));                    // key_lo = the numeric key
        emitter.instruction("mov x2, #-1");                                     // key_hi = -1 marks an integer key
        emitter.instruction("ldr x3, [sp, #32]");                               // value_lo
        emitter.instruction("mov x4, #0");                                      // value_hi
        emitter.instruction("mov x5, #0");                                      // value tag = Int
        emitter.instruction("bl __rt_hash_set");
        emitter.instruction("str x0, [sp, #24]");                               // persist the possibly-grown hash
    }
    for (index, (key_symbol, key_length)) in STAT_FIELDS.iter().enumerate() {
        emitter.instruction("ldr x0, [sp, #16]");                               // the wrapper's array
        emitter.instruction(&format!("mov x1, #{}", index));                    // the field to read
        emitter.instruction("bl __rt_uws_field");                               // x0 = the integer (0 when unnamed)
        emitter.instruction("str x0, [sp, #32]");                               // hold it across the insertion
        emitter.instruction("ldr x0, [sp, #24]");
        abi::emit_symbol_address(emitter, "x1", key_symbol);
        emitter.instruction(&format!("mov x2, #{}", key_length));
        emitter.instruction("ldr x3, [sp, #32]");                               // value_lo
        emitter.instruction("mov x4, #0");                                      // value_hi
        emitter.instruction("mov x5, #0");                                      // value tag = Int
        emitter.instruction("bl __rt_hash_set");
        emitter.instruction("str x0, [sp, #24]");                               // persist the possibly-grown hash
    }
    emitter.instruction("ldr x0, [sp, #16]");                                   // the wrapper's array
    emitter.instruction("bl __rt_decref_any");                                  // release it; the rebuilt hash is ours
    emitter.instruction("ldr x0, [sp, #24]");                                   // the rebuilt hash
    emitter.instruction("b __rt_uwsa_ret");
    emitter.label("__rt_uwsa_fail_box");
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed false
    emitter.label("__rt_uwsa_fail");
    emitter.instruction("mov x0, #0");                                          // a null hash pointer boxes php's false
    emitter.label("__rt_uwsa_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the hash (or 0)
}

/// x86_64 counterpart of [`emit_stat_array_aarch64`]: rdi/rsi = path, rdx = flags, rax = the hash.
fn emit_stat_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_stat_array ---");
    emitter.label_global("__rt_user_wrapper_stat_array");
    // Frame: [rbp-8] the wrapper's array, [rbp-16] the hash, [rbp-24] the field value.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // spill slots for array/hash/value
    emitter.instruction("call __rt_user_wrapper_url_stat");                     // rax = boxed stat array (sets _url_stat_matched)
    emitter.instruction("test rax, rax");                                       // no wrapper matched?
    emitter.instruction("jz __rt_uwsa_fail_x86");                               // the caller reads the flag
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // boxed runtime tag
    emitter.instruction("cmp r9, 3");                                           // tag 3 means url_stat() answered false
    emitter.instruction("je __rt_uwsa_fail_box_x86");                           // a failed stat
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // hold the wrapper's array across the rebuild
    emitter.instruction("mov rdi, 32");                                         // capacity 32 leaves the 13 fields a low load factor
    emitter.instruction("mov rsi, 0");                                          // value type = Int
    emitter.instruction("call __rt_hash_new");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the hash under construction
    // See the AArch64 arm: every NUMERIC key first, then every named one.
    for index in 0..STAT_FIELDS.len() {
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // the wrapper's array
        emitter.instruction(&format!("mov rsi, {}", index));                    // the field to read
        emitter.instruction("call __rt_uws_field");                             // rax = the integer (0 when unnamed)
        emitter.instruction("mov QWORD PTR [rbp - 24], rax");                   // hold it across the insertion
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                   // the hash
        emitter.instruction(&format!("mov rsi, {}", index));                    // key_lo = the numeric key
        emitter.instruction("mov rdx, -1");                                     // key_hi = -1 marks an integer key
        emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                   // value_lo
        emitter.instruction("mov r8, 0");                                       // value_hi
        emitter.instruction("mov r9, 0");                                       // value tag = Int
        emitter.instruction("call __rt_hash_set");
        emitter.instruction("mov QWORD PTR [rbp - 16], rax");                   // persist the possibly-grown hash
    }
    for (index, (key_symbol, key_length)) in STAT_FIELDS.iter().enumerate() {
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // the wrapper's array
        emitter.instruction(&format!("mov rsi, {}", index));                    // the field to read
        emitter.instruction("call __rt_uws_field");                             // rax = the integer (0 when unnamed)
        emitter.instruction("mov QWORD PTR [rbp - 24], rax");                   // hold it across the insertion
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
        abi::emit_symbol_address(emitter, "rsi", key_symbol);
        emitter.instruction(&format!("mov rdx, {}", key_length));
        emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                   // value_lo
        emitter.instruction("mov r8, 0");                                       // value_hi
        emitter.instruction("mov r9, 0");                                       // value tag = Int
        emitter.instruction("call __rt_hash_set");
        emitter.instruction("mov QWORD PTR [rbp - 16], rax");                   // persist the possibly-grown hash
    }
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the wrapper's array
    emitter.instruction("call __rt_decref_any");                                // release it; the rebuilt hash is ours
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the rebuilt hash
    emitter.instruction("jmp __rt_uwsa_ret_x86");
    emitter.label("__rt_uwsa_fail_box_x86");
    emitter.instruction("call __rt_decref_any");                                // release the boxed false (rax)
    emitter.label("__rt_uwsa_fail_x86");
    emitter.instruction("xor eax, eax");                                        // a null hash pointer boxes php's false
    emitter.label("__rt_uwsa_ret_x86");
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the hash (or 0)
}

// ---------------------------------------------------------------------------
// __rt_user_wrapper_url_stat_access: is_readable/is_writable/is_executable
// ---------------------------------------------------------------------------

/// AArch64 `__rt_user_wrapper_url_stat_access(ptr, len, which)`: x0/x1 = path,
/// x2 = 0 read / 1 write / 2 execute. Returns the predicate in x0.
///
/// php does NOT run `access(2)` for a wrapper URL — it SELECTS one permission triad out of the
/// mode the wrapper reported and tests a single bit, so the answer follows the array's `uid` and
/// `gid` rather than anything the kernel knows about the path. Measured on php 8.5.6:
///
/// - `st_uid == getuid()` picks the owner triad, whatever the group bits say;
/// - otherwise `st_gid == getgid()` OR `st_gid` among the supplementary groups picks the group
///   triad — a file owned by another user with mode `0040` reads as readable;
/// - otherwise the world triad, so `0004` reads as readable and `0400` does not.
///
/// Because `which` walks each triad's base bit down by one position, the three predicates share
/// one shift instead of nine constants. The wrapper's `url_stat()` runs ONCE, as it does in php:
/// three field reads over one boxed array, not three dispatches.
fn emit_access_aarch64(emitter: &mut Emitter) {
    let frame = 320;
    let groups_offset = 64;
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // 4-byte alignment for the helper entry
    emitter.comment("--- runtime: user_wrapper_url_stat_access ---");
    emitter.label_global("__rt_user_wrapper_url_stat_access");
    // Frame: [sp,#0..16] x29/x30, [sp,#16] which, [sp,#24] the stat box, [sp,#32] mode,
    //   [sp,#40] st_uid, [sp,#48] st_gid, [sp,#56] the group count, [sp,#64..] the group buffer.
    emitter.instruction(&format!("sub sp, sp, #{}", frame));                    // helper frame plus the group buffer
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x2, [sp, #16]");                                   // which permission the caller asked about
    emitter.instruction(&format!("mov x2, #{}", ACCESS_STAT_FLAGS));            // the flags php passes for an access check
    emitter.instruction("bl __rt_user_wrapper_url_stat");                       // x0 = boxed stat array (sets _url_stat_matched)
    emitter.instruction("cbz x0, __rt_uwsq_fail");                              // no wrapper matched — the caller reads the flag
    emitter.instruction("ldr x9, [x0]");                                        // boxed runtime tag
    emitter.instruction("cmp x9, #3");                                          // tag 3 means url_stat() answered false
    emitter.instruction("b.eq __rt_uwsq_fail_box");                             // an absent path is never readable
    emitter.instruction("str x0, [sp, #24]");                                   // hold the array across the three field reads
    emitter.instruction(&format!("mov x1, #{}", STAT_FIELD_MODE));
    emitter.instruction("bl __rt_uws_field");                                   // the permission bits
    emitter.instruction("str x0, [sp, #32]");
    emitter.instruction("ldr x0, [sp, #24]");
    emitter.instruction(&format!("mov x1, #{}", STAT_FIELD_UID));
    emitter.instruction("bl __rt_uws_field");                                   // the owning user
    emitter.instruction("str x0, [sp, #40]");
    emitter.instruction("ldr x0, [sp, #24]");
    emitter.instruction(&format!("mov x1, #{}", STAT_FIELD_GID));
    emitter.instruction("bl __rt_uws_field");                                   // the owning group
    emitter.instruction("str x0, [sp, #48]");
    emitter.instruction("ldr x0, [sp, #24]");
    emitter.instruction("bl __rt_decref_any");                                  // the array has given up everything php reads
    // -- triad selection --
    emitter.bl_c("getuid");                                                     // the real uid php compares against
    emitter.instruction("ldr x9, [sp, #40]");
    emitter.instruction("cmp w0, w9");                                          // uid_t is 32-bit; compare the halves php compares
    emitter.instruction("b.eq __rt_uwsq_owner");                                // the owner triad wins outright
    emitter.bl_c("getgid");                                                     // the real gid
    emitter.instruction("ldr x9, [sp, #48]");
    emitter.instruction("cmp w0, w9");
    emitter.instruction("b.eq __rt_uwsq_group");                                // the primary group matches
    emitter.instruction(&format!("mov x0, #{}", ACCESS_GROUP_SLOTS));           // how many supplementary groups to read
    emitter.instruction(&format!("add x1, sp, #{}", groups_offset));            // the group buffer
    emitter.bl_c("getgroups");                                                  // x0 = how many were written, or -1
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.le __rt_uwsq_other");                                // no supplementary groups to check
    emitter.instruction("str x0, [sp, #56]");                                   // the group count
    emitter.instruction("mov x10, #0");                                         // scan index
    emitter.label("__rt_uwsq_scan");
    emitter.instruction("ldr x11, [sp, #56]");
    emitter.instruction("cmp x10, x11");
    emitter.instruction("b.ge __rt_uwsq_other");                                // no supplementary group matched
    emitter.instruction(&format!("add x12, sp, #{}", groups_offset));           // the group buffer
    emitter.instruction("ldr w13, [x12, x10, lsl #2]");                         // gid_t entries are 32 bits wide
    emitter.instruction("ldr x9, [sp, #48]");
    emitter.instruction("cmp w13, w9");
    emitter.instruction("b.eq __rt_uwsq_group");                                // a supplementary group matched
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("b __rt_uwsq_scan");
    emitter.label("__rt_uwsq_owner");
    emitter.instruction(&format!("mov x13, #{}", ACCESS_OWNER_BASE));
    emitter.instruction("b __rt_uwsq_apply");
    emitter.label("__rt_uwsq_group");
    emitter.instruction(&format!("mov x13, #{}", ACCESS_GROUP_BASE));
    emitter.instruction("b __rt_uwsq_apply");
    emitter.label("__rt_uwsq_other");
    emitter.instruction(&format!("mov x13, #{}", ACCESS_OTHER_BASE));
    emitter.label("__rt_uwsq_apply");
    emitter.instruction("ldr x14, [sp, #16]");                                  // read / write / execute
    emitter.instruction("lsr x13, x13, x14");                                   // walk the triad's base bit to the asked-for one
    emitter.instruction("ldr x9, [sp, #32]");                                   // the mode
    emitter.instruction("and x9, x9, x13");
    emitter.instruction("cmp x9, #0");
    emitter.instruction("cset x0, ne");                                         // the bit is the whole answer
    emitter.instruction("b __rt_uwsq_ret");
    emitter.label("__rt_uwsq_fail_box");
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed false
    emitter.label("__rt_uwsq_fail");
    emitter.instruction("mov x0, #0");                                          // a failed stat is never readable
    emitter.label("__rt_uwsq_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame));                    // release the helper frame
    emitter.instruction("ret");                                                 // return the predicate
}

/// x86_64 counterpart of [`emit_access_aarch64`]: rdi/rsi = path, rdx = which, rax = predicate.
fn emit_access_x86_64(emitter: &mut Emitter) {
    let frame = 320;
    // The buffer occupies the far end of the frame — rbp-320 .. rbp-65 — so its 256 bytes clear
    // the spill slots at rbp-8 .. rbp-48. Anchoring it any nearer to rbp would run over them and
    // over the saved rbp and return address.
    let groups_neg = frame;
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_access ---");
    emitter.label_global("__rt_user_wrapper_url_stat_access");
    // Frame: [rbp-8] which, [rbp-16] the stat box, [rbp-24] mode, [rbp-32] st_uid,
    //   [rbp-40] st_gid, [rbp-48] the group count, [rbp-320 .. rbp-65] the group buffer.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction(&format!("sub rsp, {}", frame));                        // helper frame plus the group buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // which permission the caller asked about
    emitter.instruction(&format!("mov rdx, {}", ACCESS_STAT_FLAGS));            // the flags php passes for an access check
    emitter.instruction("call __rt_user_wrapper_url_stat");                     // rax = boxed stat array (sets _url_stat_matched)
    emitter.instruction("test rax, rax");                                       // no wrapper matched?
    emitter.instruction("jz __rt_uwsq_fail_x86");                               // the caller reads the flag
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // boxed runtime tag
    emitter.instruction("cmp r9, 3");                                           // tag 3 means url_stat() answered false
    emitter.instruction("je __rt_uwsq_fail_box_x86");                           // an absent path is never readable
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // hold the array across the three field reads
    emitter.instruction("mov rdi, rax");
    emitter.instruction(&format!("mov rsi, {}", STAT_FIELD_MODE));
    emitter.instruction("call __rt_uws_field");                                 // the permission bits
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    emitter.instruction(&format!("mov rsi, {}", STAT_FIELD_UID));
    emitter.instruction("call __rt_uws_field");                                 // the owning user
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    emitter.instruction(&format!("mov rsi, {}", STAT_FIELD_GID));
    emitter.instruction("call __rt_uws_field");                                 // the owning group
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");
    emitter.instruction("call __rt_decref_any");                                // the array has given up everything php reads
    // -- triad selection --
    emitter.bl_c("getuid");                                                     // the real uid php compares against
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");
    emitter.instruction("cmp eax, r9d");                                        // uid_t is 32-bit; compare the halves php compares
    emitter.instruction("je __rt_uwsq_owner_x86");                              // the owner triad wins outright
    emitter.bl_c("getgid");                                                     // the real gid
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");
    emitter.instruction("cmp eax, r9d");
    emitter.instruction("je __rt_uwsq_group_x86");                              // the primary group matches
    emitter.instruction(&format!("mov rdi, {}", ACCESS_GROUP_SLOTS));           // how many supplementary groups to read
    emitter.instruction(&format!("lea rsi, [rbp - {}]", groups_neg));           // the group buffer
    emitter.bl_c("getgroups");                                                  // rax = how many were written, or -1
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jle __rt_uwsq_other_x86");                             // no supplementary groups to check
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // the group count
    emitter.instruction("xor r10, r10");                                        // scan index
    emitter.label("__rt_uwsq_scan_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");
    emitter.instruction("cmp r10, r11");
    emitter.instruction("jge __rt_uwsq_other_x86");                             // no supplementary group matched
    emitter.instruction(&format!("lea rax, [rbp - {}]", groups_neg));           // the group buffer
    emitter.instruction("mov r9d, DWORD PTR [rax + r10 * 4]");                  // gid_t entries are 32 bits wide
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");
    emitter.instruction("cmp r9d, r11d");
    emitter.instruction("je __rt_uwsq_group_x86");                              // a supplementary group matched
    emitter.instruction("inc r10");
    emitter.instruction("jmp __rt_uwsq_scan_x86");
    emitter.label("__rt_uwsq_owner_x86");
    emitter.instruction(&format!("mov r9, {}", ACCESS_OWNER_BASE));
    emitter.instruction("jmp __rt_uwsq_apply_x86");
    emitter.label("__rt_uwsq_group_x86");
    emitter.instruction(&format!("mov r9, {}", ACCESS_GROUP_BASE));
    emitter.instruction("jmp __rt_uwsq_apply_x86");
    emitter.label("__rt_uwsq_other_x86");
    emitter.instruction(&format!("mov r9, {}", ACCESS_OTHER_BASE));
    emitter.label("__rt_uwsq_apply_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // read / write / execute
    emitter.instruction("shr r9, cl");                                          // walk the triad's base bit to the asked-for one
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // the mode
    emitter.instruction("and r10, r9");
    emitter.instruction("test r10, r10");
    emitter.instruction("setne al");                                            // the bit is the whole answer
    emitter.instruction("movzx eax, al");
    emitter.instruction("jmp __rt_uwsq_ret_x86");
    emitter.label("__rt_uwsq_fail_box_x86");
    emitter.instruction("call __rt_decref_any");                                // release the boxed false (rax)
    emitter.label("__rt_uwsq_fail_x86");
    emitter.instruction("xor eax, eax");                                        // a failed stat is never readable
    emitter.label("__rt_uwsq_ret_x86");
    emitter.instruction(&format!("add rsp, {}", frame));                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the predicate
}
