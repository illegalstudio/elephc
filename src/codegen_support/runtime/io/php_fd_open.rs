//! Purpose:
//! Emits `__rt_php_fd_open`, the `php://fd/N` opener — the one php:// sub-wrapper whose refusal
//! carries numbers that are only known while the program runs.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The literal `fopen("php://fd/N", …)` lowering, and `__rt_php_wrapper_open` for a URL whose
//!   bytes arrive at run time.
//!
//! Key details:
//! - php-src's `php_stream_url_wrap_php` refuses a descriptor in TWO different ways and elephc
//!   used to answer a silent `false` for both. Measured on `php -n` 8.5.6:
//!
//!   ```text
//!   fopen("php://fd/99")     Warning: fopen(php://fd/99): Failed to open stream: Error duping
//!                            file descriptor 99; possibly it doesn't exist: [9]: Bad file descriptor
//!   fopen("php://fd/-1")     Warning: fopen(php://fd/-1): Failed to open stream: The file
//!                            descriptors must be non-negative numbers smaller than 61440
//!   ```
//!
//!   The first is the only diagnostic in php that prints an errno NUMBER in brackets AND its
//!   `strerror` text; every other failed open prints the text alone.
//! - The bound is `getdtablesize()`, a run-time property of the process, so the second sentence
//!   cannot be composed at compile time either — `61440` above is this host's answer, not a
//!   constant.
//! - Neither line is composed into a buffer: they go out in fragments through `__rt_diag_warning`,
//!   the way `__rt_scandir` reports an unopenable directory, and `__rt_errno_warning` serves as
//!   the shared tail that appends `strerror` and the newline.
//! - `__rt_itoa` formats into the shared concat arena and ADVANCES its cursor, so the entry value
//!   is restored around every conversion: a loop over bad descriptors would otherwise eat the
//!   64 KiB buffer a few bytes at a time.

use crate::codegen_support::runtime::data::DIAG_NEWLINE;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch, platform::Platform};

/// What php-src puts between the URL and the descriptor number when `dup()` fails.
pub(crate) const PHP_FD_DUP_HEAD: &str =
    "): Failed to open stream: Error duping file descriptor ";

/// What follows the descriptor number, up to and including the bracket that opens the errno.
pub(crate) const PHP_FD_DUP_MIDDLE: &str = "; possibly it doesn't exist: [";

/// What closes the bracketed errno, before `strerror` supplies the rest.
pub(crate) const PHP_FD_DUP_TAIL: &str = "]: ";

/// What php-src says for a descriptor outside `[0, getdtablesize())`, up to the bound itself.
pub(crate) const PHP_FD_RANGE_HEAD: &str =
    "): Failed to open stream: The file descriptors must be non-negative numbers smaller than ";

/// The `Warning: fopen(` every line here opens with, whose symbol is shared with the composer.
const FOPEN_PREFIX_LEN: usize = "Warning: fopen(".len();

/// The thread-local `errno` accessor, spelled differently per platform.
fn errno_symbol(platform: Platform) -> &'static str {
    match platform {
        Platform::MacOS => "__error",
        Platform::Linux => "__errno_location",
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    }
}

/// Emits `__rt_php_fd_open`.
pub fn emit_php_fd_open(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// `__rt_php_fd_open(x0 = requested descriptor, x1 = URL pointer, x2 = URL length) -> x0 = fd|-1`.
///
/// The URL arrives as a pointer/length pair rather than a C string because that is what both
/// callers hold: the literal lowering has a `.ascii` symbol and its length, and the run-time
/// dispatch has the caller's bytes and the length it measured.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: open php://fd/N, or say why it cannot be opened ---");
    emitter.label_global("__rt_php_fd_open");
    // Frame: [0]=requested fd [8]=URL pointer [16]=URL length [24]=the number being rendered
    //        [32]=saved concat cursor [48]/[56]=linkage.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the diagnostic frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // the descriptor the URL named
    emitter.instruction("str x1, [sp, #8]");                                    // the URL php names in the parentheses
    emitter.instruction("str x2, [sp, #16]");

    // -- the bound php checks first, which is a property of THIS process --
    emitter.bl_c("getdtablesize");                                              // x0 = the descriptor table size
    emitter.instruction("str x0, [sp, #24]");                                   // it is also the number the refusal prints
    emitter.instruction("ldr x9, [sp, #0]");                                    // the requested descriptor
    emitter.instruction("cmp x9, #0");
    emitter.instruction("b.lt __rt_pfo_range");                                 // php refuses a negative descriptor by NAME
    emitter.instruction("cmp x9, x0");
    emitter.instruction("b.ge __rt_pfo_range");                                 // and one at or past the table size

    // -- dup(): php hands out a copy so fclose() cannot close the process's own descriptor --
    emitter.instruction("mov x0, x9");                                          // the descriptor to duplicate
    emitter.bl_c("dup");                                                        // x0 = the copy, or -1
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.ge __rt_pfo_ret");                                   // it opened; nothing to say

    // -- dup() failed: php names the descriptor, the errno NUMBER and its text --
    emitter.bl_c(errno_symbol(emitter.platform));                               // x0 = &errno for this thread
    emitter.instruction("ldrsw x9, [x0]");                                      // the code dup() set
    emitter.instruction("str x9, [sp, #24]");                                   // hold it across every fragment
    emit_line_head_aarch64(emitter);
    abi::emit_symbol_address(emitter, "x1", "_diag_php_fd_dup_head");
    emitter.instruction(&format!("mov x2, #{}", PHP_FD_DUP_HEAD.len()));
    emitter.instruction("bl __rt_diag_warning");
    emit_decimal_aarch64(emitter, 0);                                           // the descriptor the URL named
    abi::emit_symbol_address(emitter, "x1", "_diag_php_fd_dup_middle");
    emitter.instruction(&format!("mov x2, #{}", PHP_FD_DUP_MIDDLE.len()));
    emitter.instruction("bl __rt_diag_warning");
    emit_decimal_aarch64(emitter, 24);                                          // the errno, as a bare number
    abi::emit_symbol_address(emitter, "x0", "_diag_php_fd_dup_tail");
    emitter.instruction(&format!("mov x1, #{}", PHP_FD_DUP_TAIL.len()));
    emitter.instruction("ldr x2, [sp, #24]");                                   // the same errno, this time as text
    emitter.instruction("bl __rt_errno_warning");                               // appends strerror and closes the line
    emitter.instruction("mov x0, #-1");                                         // a descriptor that cannot be duped opens nothing
    emitter.instruction("b __rt_pfo_ret");

    emitter.label("__rt_pfo_range");
    emit_line_head_aarch64(emitter);
    abi::emit_symbol_address(emitter, "x1", "_diag_php_fd_range_head");
    emitter.instruction(&format!("mov x2, #{}", PHP_FD_RANGE_HEAD.len()));
    emitter.instruction("bl __rt_diag_warning");
    emit_decimal_aarch64(emitter, 24);                                          // the table size php compared against
    abi::emit_symbol_address(emitter, "x1", "_diag_newline");
    emitter.instruction(&format!("mov x2, #{}", DIAG_NEWLINE.len()));
    emitter.instruction("bl __rt_diag_warning");                                // close the line
    emitter.instruction("mov x0, #-1");                                         // an out-of-range descriptor opens nothing

    emitter.label("__rt_pfo_ret");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the diagnostic frame
    emitter.instruction("ret");                                                 // x0 = the descriptor, or -1
}

/// Emits `Warning: fopen(` followed by the URL, which both refusals open with.
fn emit_line_head_aarch64(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, "x1", "_diag_open_failed_fopen_prefix");
    emitter.instruction(&format!("mov x2, #{FOPEN_PREFIX_LEN}"));
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth
    emitter.instruction("ldr x1, [sp, #8]");                                    // the URL, as the caller measured it
    emitter.instruction("ldr x2, [sp, #16]");
    emitter.instruction("bl __rt_diag_warning");
}

/// Emits the decimal text of the frame slot at `offset`, giving the concat arena its scratch back.
fn emit_decimal_aarch64(emitter: &mut Emitter, offset: i64) {
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // the caller's concat write offset
    emitter.instruction("str x10, [sp, #32]");                                  // hold it across the conversion
    emitter.instruction(&format!("ldr x0, [sp, #{offset}]"));                   // the number to render
    emitter.instruction("bl __rt_itoa");                                        // x1/x2 = its decimal text
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [sp, #32]");
    emitter.instruction("str x10, [x9]");                                       // reclaim the diagnostic's scratch
}

/// x86_64 form of [`emit_aarch64`].
///
/// `__rt_php_fd_open(rdi = requested descriptor, rsi = URL pointer, rdx = URL length) -> rax`.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: open php://fd/N, or say why it cannot be opened ---");
    emitter.label_global("__rt_php_fd_open");
    // Frame: [rbp-8]=requested fd [rbp-16]=URL pointer [rbp-24]=URL length
    //        [rbp-32]=the number being rendered [rbp-40]=saved concat cursor.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 48");                                         // reserve the diagnostic slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the descriptor the URL named
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // the URL php names in the parentheses
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");

    // -- the bound php checks first, which is a property of THIS process --
    emitter.bl_c("getdtablesize");                                              // rax = the descriptor table size
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // it is also the number the refusal prints
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // the requested descriptor
    emitter.instruction("cmp r10, 0");
    emitter.instruction("jl __rt_pfo_range_x");                                 // php refuses a negative descriptor by NAME
    emitter.instruction("cmp r10, rax");
    emitter.instruction("jge __rt_pfo_range_x");                                // and one at or past the table size

    // -- dup(): php hands out a copy so fclose() cannot close the process's own descriptor --
    emitter.instruction("mov rdi, r10");                                        // the descriptor to duplicate
    emitter.bl_c("dup");                                                        // rax = the copy, or -1
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jge __rt_pfo_ret_x");                                  // it opened; nothing to say

    // -- dup() failed: php names the descriptor, the errno NUMBER and its text --
    emitter.bl_c(errno_symbol(emitter.platform));                               // rax = &errno for this thread
    emitter.instruction("movsxd r10, DWORD PTR [rax]");                         // the code dup() set
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // hold it across every fragment
    emit_line_head_x86_64(emitter);
    abi::emit_symbol_address(emitter, "rdi", "_diag_php_fd_dup_head");
    emitter.instruction(&format!("mov esi, {}", PHP_FD_DUP_HEAD.len()));
    emitter.instruction("call __rt_diag_warning");
    emit_decimal_x86_64(emitter, 8);                                            // the descriptor the URL named
    abi::emit_symbol_address(emitter, "rdi", "_diag_php_fd_dup_middle");
    emitter.instruction(&format!("mov esi, {}", PHP_FD_DUP_MIDDLE.len()));
    emitter.instruction("call __rt_diag_warning");
    emit_decimal_x86_64(emitter, 32);                                           // the errno, as a bare number
    abi::emit_symbol_address(emitter, "rdi", "_diag_php_fd_dup_tail");
    emitter.instruction(&format!("mov esi, {}", PHP_FD_DUP_TAIL.len()));
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // the same errno, this time as text
    emitter.instruction("call __rt_errno_warning");                             // appends strerror and closes the line
    emitter.instruction("mov rax, -1");                                         // a descriptor that cannot be duped opens nothing
    emitter.instruction("jmp __rt_pfo_ret_x");

    emitter.label("__rt_pfo_range_x");
    emit_line_head_x86_64(emitter);
    abi::emit_symbol_address(emitter, "rdi", "_diag_php_fd_range_head");
    emitter.instruction(&format!("mov esi, {}", PHP_FD_RANGE_HEAD.len()));
    emitter.instruction("call __rt_diag_warning");
    emit_decimal_x86_64(emitter, 32);                                           // the table size php compared against
    abi::emit_symbol_address(emitter, "rdi", "_diag_newline");
    emitter.instruction(&format!("mov esi, {}", DIAG_NEWLINE.len()));
    emitter.instruction("call __rt_diag_warning");                              // close the line
    emitter.instruction("mov rax, -1");                                         // an out-of-range descriptor opens nothing

    emitter.label("__rt_pfo_ret_x");
    emitter.instruction("mov rsp, rbp");                                        // release the diagnostic frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // rax = the descriptor, or -1
}

/// The x86_64 counterpart of [`emit_line_head_aarch64`].
fn emit_line_head_x86_64(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, "rdi", "_diag_open_failed_fopen_prefix");
    emitter.instruction(&format!("mov esi, {FOPEN_PREFIX_LEN}"));
    emitter.instruction("call __rt_diag_warning");                              // warnings honour the @ suppression depth
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the URL, as the caller measured it
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");
    emitter.instruction("call __rt_diag_warning");
}

/// The x86_64 counterpart of [`emit_decimal_aarch64`]; `offset` is the slot's `rbp` displacement.
fn emit_decimal_x86_64(emitter: &mut Emitter, offset: i64) {
    abi::emit_symbol_address(emitter, "r9", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // the caller's concat write offset
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // hold it across the conversion
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {offset}]"));       // the number to render
    emitter.instruction("call __rt_itoa");                                      // rax/rdx = its decimal text
    emitter.instruction("mov rdi, rax");                                        // the diagnostic helper takes the pointer in rdi
    emitter.instruction("mov rsi, rdx");                                        // and the length in rsi
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "r9", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");
    emitter.instruction("mov QWORD PTR [r9], r10");                             // reclaim the diagnostic's scratch
}
