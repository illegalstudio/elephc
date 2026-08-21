//! Purpose:
//! Emits `__rt_open_failed_warning`, the "Failed to open stream" diagnostic with the path
//! and the reason PHP puts in it.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `__rt_fopen` and `__rt_file_get_contents` failure paths.
//!
//! Key details:
//! - php-src writes `fopen(/no/such/file): Failed to open stream: No such file or directory`.
//!   elephc wrote `fopen(): Failed to open stream` — neither WHICH path failed nor WHY, which
//!   is most of what the message is for when several opens share one line.
//! - The caller supplies the prefix, so `fopen()` and `file_get_contents()` name themselves
//!   without a branch in here, and passes a POSITIVE errno: the two platforms report a failed
//!   syscall differently and normalising at the call site keeps that knowledge where the
//!   syscall is.
//! - Both the path and the reason are clamped: a diagnostic is never worth writing past the
//!   buffer into the neighbouring globals.
//! - Not every failed open HAS an errno. php-src's wrappers report most of their own refusals
//!   with a sentence — `` `z' is not a valid mode for fopen ``, `rfc2397: unable to decode`,
//!   `wrapper does not support stream open` — through `php_stream_wrapper_log_error`, which the
//!   generic caller then prints after `Failed to open stream: `. So the composition is split:
//!   `__rt_open_failed_reason_warning` takes the sentence, and `__rt_open_failed_warning` is the
//!   thin `strerror` shim in front of it. Passing a path pointer where an errno was expected is
//!   what produced `Failed to open stream: Unknown error: 80792944`.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Bytes reserved for the composed message.
///
/// A prefix, a clamped path, the fixed middle, a clamped reason and a newline all fit
/// comfortably; nothing here is sized around anything else.
pub(crate) const OPEN_FAILED_MSG_CAPACITY: usize = 512;

/// The most path bytes copied into the message.
const PATH_CLAMP: usize = 300;

/// The most reason bytes copied into the message.
const REASON_CLAMP: usize = 120;

/// The fixed text between the path and the reason.
pub(crate) const OPEN_FAILED_MIDDLE: &str = "): Failed to open stream: ";

/// What php-src puts after the rejected mode, closing quote included.
///
/// The whole reason reads `` `z' is not a valid mode for fopen `` — an opening BACKTICK and a
/// closing APOSTROPHE, which is php-src's house style for quoting a value in a wrapper error and
/// not a typo. Measured on `php -n` 8.5.6, and the empty mode spells it as `` `' ``.
pub(crate) const BAD_MODE_TAIL: &str = "' is not a valid mode for fopen";

/// Bytes reserved for the composed invalid-mode reason.
///
/// The opening quote, a clamped mode and [`BAD_MODE_TAIL`] all fit with room to spare.
pub(crate) const BAD_MODE_REASON_CAPACITY: usize = 160;

/// The most mode bytes echoed back. php prints the mode whole; nothing sane is near this.
const MODE_CLAMP: usize = 100;

/// Every sentence a built-in wrapper refuses an open with, and the symbol that holds it.
///
/// php-src's wrappers do not report these through `errno` — there is no failed syscall behind
/// them — but through `php_stream_wrapper_log_error`, which the generic caller prints after
/// `Failed to open stream: `. All measured on `php -n` 8.5.6; `data://text/plain;,hi` and
/// `data://text/plain;BASE64,SGk=` are `illegal parameter` rather than `illegal media type`,
/// which is not the split the names suggest: the TYPE is the first `;`-segment and everything
/// after it is a parameter, so a rejected `;base64` spelling lands in the second bucket.
///
/// The reasons carry no newline: [`emit_open_failed_reason_warning`]'s composer adds it, the same
/// way it does for a `strerror` string.
pub(crate) const WRAPPER_REFUSAL_REASONS: &[(&str, &str)] = &[
    ("_diag_rfc2397_no_comma", "rfc2397: no comma in URL"),
    ("_diag_rfc2397_undecodable", "rfc2397: unable to decode"),
    ("_diag_rfc2397_media_type", "rfc2397: illegal media type"),
    ("_diag_rfc2397_parameter", "rfc2397: illegal parameter"),
    ("_diag_wrapper_no_stream_open", "wrapper does not support stream open"),
    ("_diag_open_operation_failed", "operation failed"),
    ("_diag_php_fd_form", PHP_FD_FORM),
];

/// What `php://fd/` with no number answers. It is a REASON, so it prints in the one line.
pub(crate) const PHP_FD_FORM: &str =
    "php://fd/ stream must be specified in the form php://fd/<orig fd>";

/// The line php prints BEFORE the failed-open line for a php:// target it does not recognise.
///
/// Whole and newline-terminated, because php-src emits it with a direct `php_error_docref` rather
/// than through the wrapper error stack — which is also why the failed-open line that follows says
/// only `operation failed`.
pub(crate) const PHP_INVALID_URL_LINE: &str = "Warning: fopen(): Invalid php:// URL specified\n";

/// The line php prints when an `ssl.peer_fingerprint` pin does not match the peer.
///
/// Whole and newline-terminated: php-src emits it with a direct `php_error_docref`
/// from the crypto setup, before the `Failed to enable crypto` and
/// `Failed to open stream: operation failed` lines the open then composes. elephc's
/// runtime does not know which builtin is on the stack, so the `<callee>(): ` prefix
/// php puts in front of the sentence is missing here; the sentence is verbatim.
pub(crate) const PEER_FINGERPRINT_MISMATCH_LINE: &str =
    "Warning: peer_fingerprint match failure\n";

/// The reason a `glob://` open answers, whose wrapper has no `stream_opener` at all.
pub(crate) const GLOB_NO_STREAM_OPEN: &str = "wrapper does not support stream open";

/// What the generic caller prints when the wrapper stashed no reason of its own.
///
/// `php://bogus` reaches it: `php_stream_url_wrap_php` reports `Invalid php:// URL specified`
/// with a DIRECT `php_error_docref`, which prints immediately as `fopen(): …` and leaves the
/// error stack empty — so the failed-open line that follows has nothing to say but this.
pub(crate) const OPEN_OPERATION_FAILED: &str = "operation failed";

/// Emits `__rt_open_failed_warning(prefix_ptr, prefix_len, path_cstr, errno)`.
///
/// AArch64 takes `x0`/`x1`/`x2`/`x3`; x86_64 takes `rdi`/`rsi`/`rdx`/`rcx`.
pub fn emit_open_failed_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_aarch64(emitter);
            emit_bad_mode_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_x86_64(emitter);
            emit_bad_mode_x86_64(emitter);
        }
    }
}

/// Emits the AArch64 composer.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: describe an errno, then compose the failed-open warning ---");
    emitter.label_global("__rt_open_failed_warning");
    // Frame: [0] prefix pointer, [8] prefix length, [16] path pointer, [32] linkage.
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");
    emitter.instruction("str x0, [sp, #0]");                                    // hold the caller's prefix across strerror
    emitter.instruction("str x1, [sp, #8]");
    emitter.instruction("str x2, [sp, #16]");                                   // and the path
    emitter.instruction("mov x0, x3");                                          // the errno to describe
    emitter.bl_c("strerror");                                                   // x0 = static NUL-terminated reason, or 0
    emitter.instruction("mov x3, x0");                                          // the reason text
    emitter.instruction("mov x4, #0");                                          // its length, measured below
    emitter.instruction("cbz x3, __rt_ofw_errno_ready");                        // an unknown code has no text
    emitter.label("__rt_ofw_errno_len");
    emitter.instruction(&format!("cmp x4, #{REASON_CLAMP}"));
    emitter.instruction("b.hs __rt_ofw_errno_ready");
    emitter.instruction("ldrb w9, [x3, x4]");
    emitter.instruction("cbz w9, __rt_ofw_errno_ready");
    emitter.instruction("add x4, x4, #1");
    emitter.instruction("b __rt_ofw_errno_len");
    emitter.label("__rt_ofw_errno_ready");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the prefix, the path and go
    emitter.instruction("ldr x1, [sp, #8]");
    emitter.instruction("ldr x2, [sp, #16]");
    emitter.instruction("bl __rt_open_failed_reason_warning");
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");

    emitter.blank();
    emitter.comment("--- runtime: compose the failed-open warning ---");
    emitter.label_global("__rt_open_failed_reason_warning");
    // Frame: [8] saved reason pointer, [16] saved path pointer, [48] saved reason length,
    //        [32]/[40] linkage.
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");
    emitter.instruction("str x3, [sp, #8]");                                    // hold the reason across the copies
    emitter.instruction("str x2, [sp, #16]");                                   // hold the path across the copies
    emitter.instruction("str x4, [sp, #48]");                                   // and how many of its bytes to take

    abi::emit_symbol_address(emitter, "x9", "_open_failed_msg");                // the destination buffer
    emitter.instruction("mov x10, #0");                                         // bytes written so far

    // -- the caller's prefix, ending just before the path --
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_ofw_prefix");
    emitter.instruction("cmp x11, x1");
    emitter.instruction("b.hs __rt_ofw_path");
    emitter.instruction("ldrb w12, [x0, x11]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_ofw_prefix");

    // -- the path, up to its NUL --
    emitter.label("__rt_ofw_path");
    emitter.instruction("ldr x13, [sp, #16]");                                  // the path pointer
    emitter.instruction("cbz x13, __rt_ofw_middle");                            // no path to name
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_ofw_path_loop");
    emitter.instruction(&format!("cmp x11, #{PATH_CLAMP}"));
    emitter.instruction("b.hs __rt_ofw_middle");
    emitter.instruction("ldrb w12, [x13, x11]");
    emitter.instruction("cbz w12, __rt_ofw_middle");                            // the terminator ends the path
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_ofw_path_loop");

    // -- "): Failed to open stream: " --
    emitter.label("__rt_ofw_middle");
    abi::emit_symbol_address(emitter, "x14", "_diag_open_failed_middle");
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_ofw_middle_loop");
    emitter.instruction(&format!("cmp x11, #{}", OPEN_FAILED_MIDDLE.len()));
    emitter.instruction("b.hs __rt_ofw_reason");
    emitter.instruction("ldrb w12, [x14, x11]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_ofw_middle_loop");

    // -- the caller's wording for the failure, whatever produced it --
    emitter.label("__rt_ofw_reason");
    emitter.instruction("ldr x13, [sp, #8]");                                   // the reason text
    emitter.instruction("ldr x14, [sp, #48]");                                  // and how many bytes of it the caller offered
    emitter.instruction("cbz x13, __rt_ofw_tail");                              // no text to give
    emitter.instruction(&format!("mov x15, #{REASON_CLAMP}"));
    emitter.instruction("cmp x14, x15");
    emitter.instruction("csel x14, x14, x15, lo");                              // never copy past the clamp
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_ofw_reason_loop");
    emitter.instruction("cmp x11, x14");
    emitter.instruction("b.hs __rt_ofw_tail");
    emitter.instruction("ldrb w12, [x13, x11]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_ofw_reason_loop");

    emitter.label("__rt_ofw_tail");
    emitter.instruction("mov w12, #0x0a");                                      // '\n'
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");

    emitter.instruction("mov x1, x9");                                          // message pointer
    emitter.instruction("mov x2, x10");                                         // message length
    emitter.instruction("bl __rt_diag_warning");                                // stderr, and `@` suppresses it

    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// Emits the AArch64 `__rt_fopen_bad_mode_warning(x0 = path cstr, x1 = mode ptr, x2 = mode len)`.
///
/// Builds php's reason for a mode whose FIRST byte is none of `r`, `w`, `a`, `x`, `c` — the only
/// thing php-src's `php_stream_parse_fopen_modes` validates — and hands it to the shared composer.
fn emit_bad_mode_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the invalid-fopen-mode warning ---");
    emitter.label_global("__rt_fopen_bad_mode_warning");
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");
    emitter.instruction("str x0, [sp, #0]");                                    // hold the path across the composition

    abi::emit_symbol_address(emitter, "x9", "_fopen_bad_mode_reason");          // the reason scratch
    emitter.instruction("mov w10, #0x60");                                      // '`' opens php's quoting
    emitter.instruction("strb w10, [x9]");
    emitter.instruction("mov x11, #1");                                         // reason cursor
    emitter.instruction("mov x12, #0");                                         // mode index
    emitter.label("__rt_fbm_mode");
    emitter.instruction("cmp x12, x2");                                         // copied the whole mode?
    emitter.instruction("b.hs __rt_fbm_tail");
    emitter.instruction(&format!("cmp x12, #{MODE_CLAMP}"));
    emitter.instruction("b.hs __rt_fbm_tail");                                  // never write past the scratch
    emitter.instruction("ldrb w13, [x1, x12]");
    emitter.instruction("strb w13, [x9, x11]");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_fbm_mode");

    emitter.label("__rt_fbm_tail");
    abi::emit_symbol_address(emitter, "x13", "_diag_mode_not_valid_tail");
    emitter.instruction("mov x12, #0");
    emitter.label("__rt_fbm_tail_loop");
    emitter.instruction(&format!("cmp x12, #{}", BAD_MODE_TAIL.len()));
    emitter.instruction("b.hs __rt_fbm_done");
    emitter.instruction("ldrb w14, [x13, x12]");
    emitter.instruction("strb w14, [x9, x11]");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_fbm_tail_loop");

    emitter.label("__rt_fbm_done");
    emitter.instruction("mov x3, x9");                                          // the reason, before the argument registers are rebuilt
    emitter.instruction("mov x4, x11");                                         // and its length
    emitter.instruction("ldr x2, [sp, #0]");                                    // the path php names in the parentheses
    abi::emit_symbol_address(emitter, "x0", "_diag_open_failed_fopen_prefix");
    emitter.instruction(&format!("mov x1, #{}", "Warning: fopen(".len()));
    emitter.instruction("bl __rt_open_failed_reason_warning");
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}

/// The x86_64 counterpart of [`emit_bad_mode_aarch64`]: `rdi` path, `rsi` mode ptr, `rdx` len.
fn emit_bad_mode_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the invalid-fopen-mode warning ---");
    emitter.label_global("__rt_fopen_bad_mode_warning");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 32");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // hold the path across the composition

    abi::emit_symbol_address(emitter, "r9", "_fopen_bad_mode_reason");          // the reason scratch
    emitter.instruction("mov BYTE PTR [r9], 0x60");                             // '`' opens php's quoting
    emitter.instruction("mov r10, 1");                                          // reason cursor
    emitter.instruction("xor r11d, r11d");                                      // mode index
    emitter.label("__rt_fbm_mode_x86");
    emitter.instruction("cmp r11, rdx");                                        // copied the whole mode?
    emitter.instruction("jae __rt_fbm_tail_x86");
    emitter.instruction(&format!("cmp r11, {MODE_CLAMP}"));
    emitter.instruction("jae __rt_fbm_tail_x86");                               // never write past the scratch
    emitter.instruction("movzx eax, BYTE PTR [rsi + r11]");
    emitter.instruction("mov BYTE PTR [r9 + r10], al");
    emitter.instruction("inc r10");
    emitter.instruction("inc r11");
    emitter.instruction("jmp __rt_fbm_mode_x86");

    emitter.label("__rt_fbm_tail_x86");
    abi::emit_symbol_address(emitter, "r8", "_diag_mode_not_valid_tail");
    emitter.instruction("xor r11d, r11d");
    emitter.label("__rt_fbm_tail_loop_x86");
    emitter.instruction(&format!("cmp r11, {}", BAD_MODE_TAIL.len()));
    emitter.instruction("jae __rt_fbm_done_x86");
    emitter.instruction("movzx eax, BYTE PTR [r8 + r11]");
    emitter.instruction("mov BYTE PTR [r9 + r10], al");
    emitter.instruction("inc r10");
    emitter.instruction("inc r11");
    emitter.instruction("jmp __rt_fbm_tail_loop_x86");

    emitter.label("__rt_fbm_done_x86");
    emitter.instruction("mov rcx, r9");                                         // the reason, before the argument registers are rebuilt
    emitter.instruction("mov r8, r10");                                         // and its length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // the path php names in the parentheses
    abi::emit_symbol_address(emitter, "rdi", "_diag_open_failed_fopen_prefix");
    emitter.instruction(&format!("mov esi, {}", "Warning: fopen(".len()));
    emitter.instruction("call __rt_open_failed_reason_warning");
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// Emits the Linux x86_64 composer.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: describe an errno, then compose the failed-open warning ---");
    emitter.label_global("__rt_open_failed_warning");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 48");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // hold the caller's prefix across strerror
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // and the path
    emitter.instruction("mov rdi, rcx");                                        // the errno to describe
    emitter.instruction("call strerror");                                       // rax = static NUL-terminated reason, or 0
    emitter.instruction("mov rcx, rax");                                        // the reason text
    emitter.instruction("xor r8d, r8d");                                        // its length, measured below
    emitter.instruction("test rcx, rcx");
    emitter.instruction("jz __rt_ofw_errno_ready_x86");                         // an unknown code has no text
    emitter.label("__rt_ofw_errno_len_x86");
    emitter.instruction(&format!("cmp r8, {REASON_CLAMP}"));
    emitter.instruction("jae __rt_ofw_errno_ready_x86");
    emitter.instruction("movzx eax, BYTE PTR [rcx + r8]");
    emitter.instruction("test al, al");
    emitter.instruction("jz __rt_ofw_errno_ready_x86");
    emitter.instruction("inc r8");
    emitter.instruction("jmp __rt_ofw_errno_len_x86");
    emitter.label("__rt_ofw_errno_ready_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the prefix, the path and go
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");
    emitter.instruction("call __rt_open_failed_reason_warning");
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");

    emitter.blank();
    emitter.comment("--- runtime: compose the failed-open warning ---");
    emitter.label_global("__rt_open_failed_reason_warning");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 48");
    emitter.instruction("mov QWORD PTR [rbp - 8], rcx");                        // hold the reason across the copies
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // hold the path across the copies
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // and how many of its bytes to take

    abi::emit_symbol_address(emitter, "r9", "_open_failed_msg");                // the destination buffer
    emitter.instruction("xor r10d, r10d");                                      // bytes written so far

    // -- the caller's prefix, ending just before the path --
    emitter.instruction("xor r11d, r11d");
    emitter.label("__rt_ofw_prefix_x86");
    emitter.instruction("cmp r11, rsi");
    emitter.instruction("jae __rt_ofw_path_x86");
    emitter.instruction("movzx eax, BYTE PTR [rdi + r11]");
    emitter.instruction("mov BYTE PTR [r9 + r10], al");
    emitter.instruction("inc r10");
    emitter.instruction("inc r11");
    emitter.instruction("jmp __rt_ofw_prefix_x86");

    // -- the path, up to its NUL --
    emitter.label("__rt_ofw_path_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");
    emitter.instruction("test r8, r8");
    emitter.instruction("jz __rt_ofw_middle_x86");
    emitter.instruction("xor r11d, r11d");
    emitter.label("__rt_ofw_path_loop_x86");
    emitter.instruction(&format!("cmp r11, {PATH_CLAMP}"));
    emitter.instruction("jae __rt_ofw_middle_x86");
    emitter.instruction("movzx eax, BYTE PTR [r8 + r11]");
    emitter.instruction("test al, al");
    emitter.instruction("jz __rt_ofw_middle_x86");
    emitter.instruction("mov BYTE PTR [r9 + r10], al");
    emitter.instruction("inc r10");
    emitter.instruction("inc r11");
    emitter.instruction("jmp __rt_ofw_path_loop_x86");

    // -- "): Failed to open stream: " --
    emitter.label("__rt_ofw_middle_x86");
    abi::emit_symbol_address(emitter, "r8", "_diag_open_failed_middle");
    emitter.instruction("xor r11d, r11d");
    emitter.label("__rt_ofw_middle_loop_x86");
    emitter.instruction(&format!("cmp r11, {}", OPEN_FAILED_MIDDLE.len()));
    emitter.instruction("jae __rt_ofw_reason_x86");
    emitter.instruction("movzx eax, BYTE PTR [r8 + r11]");
    emitter.instruction("mov BYTE PTR [r9 + r10], al");
    emitter.instruction("inc r10");
    emitter.instruction("inc r11");
    emitter.instruction("jmp __rt_ofw_middle_loop_x86");

    // -- the caller's wording for the failure, whatever produced it --
    emitter.label("__rt_ofw_reason_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // the reason text
    emitter.instruction("test r8, r8");
    emitter.instruction("jz __rt_ofw_tail_x86");                                // no text to give
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // how many bytes of it the caller offered
    emitter.instruction(&format!("mov rax, {REASON_CLAMP}"));
    emitter.instruction("cmp rcx, rax");
    emitter.instruction("cmovae rcx, rax");                                     // never copy past the clamp
    emitter.instruction("xor r11d, r11d");
    emitter.label("__rt_ofw_reason_loop_x86");
    emitter.instruction("cmp r11, rcx");
    emitter.instruction("jae __rt_ofw_tail_x86");
    emitter.instruction("movzx eax, BYTE PTR [r8 + r11]");
    emitter.instruction("mov BYTE PTR [r9 + r10], al");
    emitter.instruction("inc r10");
    emitter.instruction("inc r11");
    emitter.instruction("jmp __rt_ofw_reason_loop_x86");

    emitter.label("__rt_ofw_tail_x86");
    emitter.instruction("mov BYTE PTR [r9 + r10], 0x0a");                       // '\n'
    emitter.instruction("inc r10");

    emitter.instruction("mov rdi, r9");                                         // message pointer
    emitter.instruction("mov rsi, r10");                                        // message length
    emitter.instruction("call __rt_diag_warning");                              // stderr, and `@` suppresses it

    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}
