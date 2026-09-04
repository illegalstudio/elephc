//! Purpose:
//! Emits `__rt_report_uncaught_exception`, the fatal path taken when a `throw` finds no active
//! handler. It reports the Throwable's CLASS, MESSAGE and SOURCE LOCATION instead of a fixed
//! string, then exits with PHP's status.
//!
//! Called from:
//! - `super::throw_current::emit_throw_current` on both architectures, replacing an inline
//!   `write()` of the constant `_uncaught_exc_msg`.
//!
//! Key details:
//! - Reference PHP 8.5.6, measured with `php -d xdebug.mode=off`:
//!
//!   ```text
//!   Fatal error: Uncaught RuntimeException: boom detail in /path/e.php:2
//!   Fatal error: Uncaught MyErr: custom text in /path/m2.php:3
//!   Fatal error: Uncaught Exception in /path/m1.php:2        <- EMPTY message: no colon
//!   ```
//!
//!   An empty message drops the `: ` separator entirely; that is why the length is tested before
//!   the separator is written, not after.
//!
//! - THE ` in <file>:<line>` SUFFIX. The line is the one stamped into the compact Throwable
//!   payload at `THROWABLE_CREATION_LINE_OFFSET` by whichever emitter allocated it, and the file
//!   is the module's canonical source path in `_script_source_file`. Both are the CONSTRUCTION
//!   site, not the throw site, because that is what PHP reports: `$e = new RuntimeException(...)`
//!   on line 2 followed by `throw $e;` on line 5 prints line 2.
//!
//!   A zero line means no user `new` stood behind the Throwable — an `ArithmeticError` raised by a
//!   division, a `TypeError` from an argument check — and the suffix is then OMITTED rather than
//!   printed as `:0`. Same for an empty `_script_source_file_len`. Printing a fabricated origin
//!   would be worse than printing none.
//!
//! - WHAT THIS STILL DOES NOT EMIT. PHP continues with `Stack trace:`, `#0 {main}` and
//!   `  thrown in <file> on line <n>`. `Throwable::getTrace()` and `getTraceAsString()` remain
//!   synthetic in `lower_inst.rs` — an empty array and an empty string — because elephc keeps no
//!   call stack to render, so there is nothing truthful to print.
//!
//! - The exit status moves from `1` to `255`, which is what reference PHP returns for an uncaught
//!   exception. A script that branched on `$?` saw the wrong value before.
//!
//! - A null `_exc_value` keeps the original constant message. That slot is written by
//!   `lower_throw_value` immediately before the call, so null means the runtime reached this path
//!   without a throwable (an internal invariant break); reporting a class read from a null pointer
//!   would turn a diagnostic into a segfault.
//!
//! - Register discipline: only caller-saved scratch (`x9`-`x11`, `r8`-`r10`) holds state across
//!   the `write` syscalls, and the helper never returns, so no callee-saved register is disturbed.
//!   The throwable pointer is RE-READ from `_exc_value` at each step of the location block rather
//!   than parked in a register, because `abi::emit_load_symbol_to_reg` resolves an AArch64 symbol
//!   through `x9` — the very register holding it. The helper is entered by a TAIL JUMP from
//!   `__rt_throw_current`, so there is no frame to spill into either; `_exc_value` is still
//!   published throughout, which makes the global the cheapest place to keep it. The x86_64 arm
//!   re-reads in the same places purely to keep the two listings comparable: SysV `syscall` clobbers
//!   only `rcx`/`r11`, so `r8` would in fact have survived.
//!
//! - Stack alignment across `__rt_itoa` was checked rather than assumed. Because this helper is
//!   reached by a tail jump, `rsp` arrives 16-aligned or 8-off depending on whether the throwing
//!   site `call`ed or `jmp`ed into `__rt_throw_current`, so the call cannot guarantee SysV's
//!   entry invariant. It does not need to: the x86_64 `__rt_itoa` body is `push rbp` … `pop rbp`
//!   with no SSE operand, no `sub rsp`, and no nested call, so nothing there faults on a
//!   misaligned stack. The AArch64 side is unconditionally safe — `sp` is 16-aligned at every
//!   memory access by AAPCS64, and `sub sp, sp, #16` preserves that.

use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::{
    THROWABLE_CREATION_LINE_OFFSET, THROWABLE_TRACE_EXACT_OFFSET,
};
use crate::codegen_support::{abi, emit::Emitter};

/// Byte length of `"Fatal error: Uncaught "`.
const UNCAUGHT_PREFIX_LEN: i64 = 23;

/// Byte length of the `": "` separator between class name and message.
const UNCAUGHT_SEPARATOR_LEN: i64 = 2;

/// Byte length of the `" in "` lead-in before the source location.
const UNCAUGHT_IN_LEN: i64 = 4;

/// Byte length of the `":"` between the filename and the line number.
const UNCAUGHT_COLON_LEN: i64 = 1;

/// Byte length of the trailing newline.
const UNCAUGHT_NEWLINE_LEN: i64 = 1;

/// Byte length of the constant fallback message, kept in sync with `data::fixed`.
const UNCAUGHT_FALLBACK_LEN: i64 = 32;

/// Byte length of `"\nNext "`, the introducer php puts before every link after the first.
const UNCAUGHT_NEXT_LEN: i64 = 6;

/// Offset of a compact Throwable's `previous` slot — see `sentinels`, which lays the payload out.
const THROWABLE_PREVIOUS_OFFSET: i64 = 40;

/// PHP's process exit status for an uncaught exception.
///
/// Shared with `codegen::lower_inst::exceptions`, which has its OWN uncaught path for the errors
/// it synthesizes (a `DivisionByZeroError` from `intdiv($n, 0)`, say): that path short-circuits
/// before the throwable is allocated and never reaches this helper. Both must agree, or the exit
/// status a script sees would depend on which kind of exception went unhandled.
pub(crate) const UNCAUGHT_EXIT_STATUS: u32 = 255;

/// Emits `__rt_report_uncaught_exception`, which never returns.
pub fn emit_report_uncaught_exception(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_report_uncaught_exception_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: report_uncaught_exception ---");
    emitter.label_global("__rt_report_uncaught_exception");

    // Drain still-active output buffers FIRST. PHP writes this report through the same output
    // path as `echo`, so buffered text precedes it; this helper used to exit without flushing,
    // which discarded every byte a program had buffered — `ob_start(); echo "x"; throw …` printed
    // nothing at all on stdout. Nothing is live yet at the entry, so the call costs no spill.
    emitter.instruction("bl __rt_ob_flush_all");                                // drain every open output buffer before the report is written

    abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_value", 0);
    emitter.instruction("cbz x9, __rt_uncaught_fallback");                      // no throwable published: keep the constant message rather than dereferencing null

    // -- php reports the whole CHAIN, oldest first --
    //
    // `throw new X($m, 0, $previous)` is how a library says what it was doing when something
    // underneath it failed, and php prints every link: the deepest one under `Fatal error:
    // Uncaught`, each later one under `Next`, and the tail line only once at the end. elephc
    // printed the OUTERMOST link and dropped the rest — the wrapper, without the failure it
    // wrapped, which is the half that says what actually went wrong.
    //
    // The walk starts at the deepest `previous` and climbs back. A link's parent is found by
    // walking down from the head again rather than by keeping a back-pointer: the chain is a
    // handful of links, and the report runs once, at exit.
    emitter.instruction("sub sp, sp, #32");                                     // frame: [0] = the link being printed, [8] = 0 while it is the first
    emitter.instruction("stp x29, x30, [sp, #16]");
    emitter.instruction("add x29, sp, #16");
    emitter.instruction("mov x10, x9");
    emitter.label("__rt_uncaught_deepest");
    emitter.instruction(&format!("ldr x11, [x10, #{THROWABLE_PREVIOUS_OFFSET}]"));
    emitter.instruction("cbz x11, __rt_uncaught_deepest_found");                // no previous: this link is the oldest
    emitter.instruction("mov x10, x11");
    emitter.instruction("b __rt_uncaught_deepest");
    emitter.label("__rt_uncaught_deepest_found");
    emitter.instruction("str x10, [sp, #0]");                                   // the oldest link is reported first
    emitter.instruction("str xzr, [sp, #8]");                                   // and carries the `Fatal error: Uncaught` introducer

    emitter.label("__rt_uncaught_block");
    emitter.instruction("ldr x9, [sp, #8]");
    emitter.instruction("cbnz x9, __rt_uncaught_next_word");                    // a later link is introduced by `Next`
    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_prefix");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_PREFIX_LEN));          // "Fatal error: Uncaught "
    emitter.instruction("mov x0, #1");                                          // fd = stdout: PHP writes this report to stdout, not stderr (measured)
    emitter.syscall(4);
    emitter.instruction("b __rt_uncaught_introduced");
    emitter.label("__rt_uncaught_next_word");
    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_next");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_NEXT_LEN));            // "\nNext "
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.label("__rt_uncaught_introduced");

    emitter.instruction("ldr x9, [sp, #0]");                                    // the link being reported
    emitter.instruction("ldr x10, [x9]");                                       // runtime class id from the object header
    abi::emit_symbol_address(emitter, "x11", "_class_name_count");
    emitter.instruction("ldr x11, [x11]");                                      // number of named class ids
    emitter.instruction("cmp x10, x11");                                        // is the class id inside the name table?
    emitter.instruction("b.hs __rt_uncaught_no_name");                          // an unknown id writes no class name, exactly as var_dump does
    abi::emit_symbol_address(emitter, "x11", "_class_name_entries");
    emitter.instruction("add x11, x11, x10, lsl #4");                           // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("ldr x1, [x11]");                                       // class-name pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // class-name length
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);

    emitter.label("__rt_uncaught_no_name");
    emitter.instruction("ldr x2, [x9, #16]");                                   // Throwable message length lives at payload offset 16
    emitter.instruction("cbz x2, __rt_uncaught_location");                      // an EMPTY message drops the ": " separator, matching reference PHP — but keeps the location
    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_sep");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_SEPARATOR_LEN));       // ": "
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("ldr x1, [x9, #8]");                                    // Throwable message pointer lives at payload offset 8
    emitter.instruction("ldr x2, [x9, #16]");                                   // and its length at offset 16
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);

    emitter.label("__rt_uncaught_location");
    emitter.instruction("ldr x9, [sp, #0]");                                    // re-read: x9 is AArch64's symbol scratch, so every symbol load below overwrites it
    emitter.instruction(&format!(
        "ldr x10, [x9, #{}]",
        THROWABLE_CREATION_LINE_OFFSET
    ));                                                                         // creation line stamped by the allocating `new`
    emitter.instruction("cbz x10, __rt_uncaught_newline");                      // line 0 means no user `new` behind this throwable: omit rather than invent
    abi::emit_load_symbol_to_reg(emitter, "x11", "_script_source_file_len", 0);
    emitter.instruction("cbz x11, __rt_uncaught_newline");                      // a module with no source path has no filename to print

    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_in");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_IN_LEN));              // " in "
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    abi::emit_symbol_address(emitter, "x1", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "x2", "_script_source_file_len", 0);
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_colon");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_COLON_LEN));           // ":"
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload: the file-length load above resolved its symbol through x9
    emitter.instruction(&format!(
        "ldr x0, [x9, #{}]",
        THROWABLE_CREATION_LINE_OFFSET
    ));                                                                         // itoa takes the line in x0
    emitter.instruction("bl __rt_itoa");                                        // returns pointer in x1 and length in x2 — already the write arguments
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);

    emitter.label("__rt_uncaught_newline");
    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_nl");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_NEWLINE_LEN));         // terminating newline
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);

    // -- the tail line belongs to the LAST link only --
    // Every link gets its own `Stack trace:` block; a zero line is what tells the writer to stop
    // before `  thrown in ...`, which php prints once, for the exception that was actually thrown.
    emitter.instruction("ldr x9, [sp, #0]");                                    // the link just reported
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_value", 0);
    emitter.instruction("ldr x9, [sp, #0]");                                    // the symbol load above resolved through x9
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.eq __rt_uncaught_last");                             // the thrown exception itself: print the tail and leave
    emitter.instruction("mov x0, #0");                                          // an inner link has no tail of its own
    emitter.instruction("ldr x9, [sp, #0]");                                    // the link just reported
    emitter.instruction(&format!("ldr x1, [x9, #{THROWABLE_TRACE_EXACT_OFFSET}]")); // the proof its own construction site stamped
    emitter.instruction("bl __rt_trace_write_block");                           // php's Stack trace: block, when the frame list is complete

    // -- climb one link: the parent is whoever names this one as its previous --
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_value", 0);
    emitter.instruction("ldr x11, [sp, #0]");                                   // the link just reported
    emitter.label("__rt_uncaught_parent");
    emitter.instruction(&format!("ldr x12, [x10, #{THROWABLE_PREVIOUS_OFFSET}]"));
    emitter.instruction("cmp x12, x11");
    emitter.instruction("b.eq __rt_uncaught_parent_found");
    emitter.instruction("mov x10, x12");
    emitter.instruction("b __rt_uncaught_parent");
    emitter.label("__rt_uncaught_parent_found");
    emitter.instruction("str x10, [sp, #0]");                                   // report the wrapper next
    emitter.instruction("mov x9, #1");
    emitter.instruction("str x9, [sp, #8]");                                    // under `Next`
    emitter.instruction("b __rt_uncaught_block");

    emitter.label("__rt_uncaught_last");
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction(&format!("ldr x0, [x9, #{}]", THROWABLE_CREATION_LINE_OFFSET));
    emitter.instruction(&format!("ldr x1, [x9, #{THROWABLE_TRACE_EXACT_OFFSET}]")); // the proof its own construction site stamped
    emitter.instruction("bl __rt_trace_write_block");                           // php's Stack trace: block, when the frame list is complete
    emitter.instruction(&format!("mov x0, #{}", UNCAUGHT_EXIT_STATUS));         // PHP exits 255 for an uncaught exception
    emitter.syscall(1);

    emitter.label("__rt_uncaught_fallback");
    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_msg");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_FALLBACK_LEN));        // the pre-existing constant message
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction(&format!("mov x0, #{}", UNCAUGHT_EXIT_STATUS));         // same status as the reporting path
    emitter.syscall(1);
}

/// Emits `__rt_report_uncaught_exception` for Linux x86_64 (System V AMD64).
fn emit_report_uncaught_exception_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: report_uncaught_exception ---");
    emitter.label_global("__rt_report_uncaught_exception");

    // See the ARM64 path. `and rsp, -16` is the realignment the shared exit path performs for the
    // same call; this helper never returns, so clobbering the alignment afterwards is harmless.
    emitter.instruction("and rsp, -16");                                        // realign the stack for the C-ABI flush; this helper never returns
    emitter.instruction("call __rt_ob_flush_all");                              // drain every open output buffer before the report is written

    abi::emit_load_symbol_to_reg(emitter, "r8", "_exc_value", 0);
    emitter.instruction("test r8, r8");                                         // no throwable published: keep the constant message rather than dereferencing null
    emitter.instruction("jz __rt_uncaught_fallback");                           // use the constant fallback when no throwable is published

    // See the AArch64 counterpart: php reports the whole CHAIN, oldest first, and elephc printed
    // only the outermost link. `rsp` is already 16-aligned here, so the two slots keep it aligned.
    emitter.instruction("sub rsp, 32");                                         // [rsp] = the link being printed, [rsp+8] = 0 while it is the first
    emitter.instruction("mov r9, r8");
    emitter.label("__rt_uncaught_deepest");
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [r9 + {THROWABLE_PREVIOUS_OFFSET}]"
    ));
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_uncaught_deepest_found");                      // no previous: this link is the oldest
    emitter.instruction("mov r9, r10");
    emitter.instruction("jmp __rt_uncaught_deepest");
    emitter.label("__rt_uncaught_deepest_found");
    emitter.instruction("mov QWORD PTR [rsp], r9");                             // the oldest link is reported first
    emitter.instruction("mov QWORD PTR [rsp + 8], 0");                          // and carries the `Fatal error: Uncaught` introducer

    emitter.label("__rt_uncaught_block");
    emitter.instruction("cmp QWORD PTR [rsp + 8], 0");
    emitter.instruction("jne __rt_uncaught_next_word");                         // a later link is introduced by `Next`
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_prefix");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_PREFIX_LEN));          // "Fatal error: Uncaught "
    emitter.instruction("mov edi, 1");                                          // fd = stdout: PHP writes this report to stdout, not stderr (measured)
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the fatal-error prefix to stderr
    emitter.instruction("jmp __rt_uncaught_introduced");
    emitter.label("__rt_uncaught_next_word");
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_next");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_NEXT_LEN));            // "\nNext "
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // introduce a later link in the chain
    emitter.label("__rt_uncaught_introduced");

    emitter.instruction("mov r8, QWORD PTR [rsp]");                             // the link being reported
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // runtime class id from the object header
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // number of named class ids
    emitter.instruction("cmp r9, r10");                                         // is the class id inside the name table?
    emitter.instruction("jae __rt_uncaught_no_name");                           // an unknown id writes no class name, exactly as var_dump does
    abi::emit_symbol_address(emitter, "r10", "_class_name_entries");
    emitter.instruction("shl r9, 4");                                           // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("add r10, r9");                                         // address of this class's entry
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // class-name length
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the throwable class name to stderr

    emitter.label("__rt_uncaught_no_name");
    emitter.instruction("mov rdx, QWORD PTR [r8 + 16]");                        // Throwable message length lives at payload offset 16
    emitter.instruction("test rdx, rdx");                                       // an EMPTY message drops the ": " separator, matching reference PHP
    emitter.instruction("jz __rt_uncaught_location");                           // an EMPTY message keeps the location suffix, only the ": " separator goes
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_sep");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_SEPARATOR_LEN));       // ": "
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the message separator to stderr
    emitter.instruction("mov rsi, QWORD PTR [r8 + 8]");                         // Throwable message pointer lives at payload offset 8
    emitter.instruction("mov rdx, QWORD PTR [r8 + 16]");                        // and its length at offset 16
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the throwable message to stderr

    emitter.label("__rt_uncaught_location");
    emitter.instruction("mov r8, QWORD PTR [rsp]");                             // re-read for symmetry with the AArch64 arm, whose x9 scratch makes this mandatory
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [r8 + {}]",
        THROWABLE_CREATION_LINE_OFFSET
    ));                                                                         // creation line stamped by the allocating `new`
    emitter.instruction("test r9, r9");                                         // line 0 means no user `new` behind this throwable: omit rather than invent
    emitter.instruction("jz __rt_uncaught_newline");                            // omit a location when the throwable has no creation line
    abi::emit_load_symbol_to_reg(emitter, "r10", "_script_source_file_len", 0);
    emitter.instruction("test r10, r10");                                       // a module with no source path has no filename to print
    emitter.instruction("jz __rt_uncaught_newline");                            // omit a location when the module has no source filename

    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_in");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_IN_LEN));              // " in "
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the location introducer to stderr
    abi::emit_symbol_address(emitter, "rsi", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_script_source_file_len", 0);
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the source filename to stderr
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_colon");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_COLON_LEN));           // ":"
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the filename/line separator to stderr
    emitter.instruction("mov r8, QWORD PTR [rsp]");                             // reload for symmetry: SysV `syscall` spares r8/r9, but the AArch64 arm cannot
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r8 + {}]",
        THROWABLE_CREATION_LINE_OFFSET
    ));                                                                         // itoa takes the line in rax
    emitter.instruction("call __rt_itoa");                                      // returns pointer in rax and length in rdx
    emitter.instruction("mov rsi, rax");                                        // move the digits out of rax before it becomes the syscall number
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the decimal creation line to stderr

    emitter.label("__rt_uncaught_newline");
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_nl");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_NEWLINE_LEN));         // terminating newline
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // terminate the uncaught-exception diagnostic with a newline

    // See the AArch64 counterpart: every link gets its own trace block, and a zero line is what
    // stops the writer before `  thrown in ...`, which php prints once for the thrown exception.
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_value", 0);
    emitter.instruction("cmp QWORD PTR [rsp], r10");
    emitter.instruction("je __rt_uncaught_last");                               // the thrown exception itself: print the tail and leave
    emitter.instruction("xor edi, edi");                                        // an inner link has no tail of its own
    emitter.instruction("mov r8, QWORD PTR [rsp]");                             // the link just reported
    emitter.instruction(&format!(
        "mov rsi, QWORD PTR [r8 + {THROWABLE_TRACE_EXACT_OFFSET}]"
    ));                                                                         // the proof its own construction site stamped
    emitter.instruction("call __rt_trace_write_block");                         // php's Stack trace: block, when the frame list is complete

    // -- climb one link: the parent is whoever names this one as its previous --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_value", 0);
    emitter.instruction("mov r8, QWORD PTR [rsp]");                             // the link just reported
    emitter.label("__rt_uncaught_parent");
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [r10 + {THROWABLE_PREVIOUS_OFFSET}]"
    ));
    emitter.instruction("cmp r9, r8");
    emitter.instruction("je __rt_uncaught_parent_found");
    emitter.instruction("mov r10, r9");
    emitter.instruction("jmp __rt_uncaught_parent");
    emitter.label("__rt_uncaught_parent_found");
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // report the wrapper next
    emitter.instruction("mov QWORD PTR [rsp + 8], 1");                          // under `Next`
    emitter.instruction("jmp __rt_uncaught_block");

    emitter.label("__rt_uncaught_last");
    emitter.instruction("mov r10, QWORD PTR [rsp]");
    emitter.instruction(&format!("mov rdi, QWORD PTR [r10 + {}]", THROWABLE_CREATION_LINE_OFFSET));
    emitter.instruction(&format!(
        "mov rsi, QWORD PTR [r10 + {THROWABLE_TRACE_EXACT_OFFSET}]"
    ));                                                                         // the proof its own construction site stamped
    emitter.instruction("call __rt_trace_write_block");                         // php's Stack trace: block, when the frame list is complete
    emitter.instruction(&format!("mov edi, {}", UNCAUGHT_EXIT_STATUS));         // PHP exits 255 for an uncaught exception
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // exit the process after reporting the throwable

    emitter.label("__rt_uncaught_fallback");
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_msg");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_FALLBACK_LEN));        // the pre-existing constant message
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the constant fallback diagnostic to stderr
    emitter.instruction(&format!("mov edi, {}", UNCAUGHT_EXIT_STATUS));         // same status as the reporting path
    emitter.instruction("mov eax, 60");                                         // syscall 60 = exit
    emitter.instruction("syscall");                                             // exit the process after the fallback diagnostic
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies the AArch64 report reads the Throwable rather than writing a fixed string.
    ///
    /// Each assertion pins a distinct decision, so a sentinel that breaks one cannot hide behind
    /// another: the null guard, the class-id bounds check against the shared name table, the
    /// 16-byte entry stride, the message loads, and the exit status. `mov x0, #255` is asserted
    /// TWICE — the reporting path and the fallback must agree, and an earlier revision left the
    /// fallback on the old `1`. The empty-message branch moved to
    /// [`test_uncaught_report_arm64_appends_the_source_location`], which is where its target now
    /// carries meaning.
    #[test]
    fn test_uncaught_report_arm64_reads_class_and_message() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_report_uncaught_exception(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_report_uncaught_exception:\n"));
        assert!(asm.contains("cbz x9, __rt_uncaught_fallback"));
        assert!(asm.contains("b.hs __rt_uncaught_no_name"));
        assert!(asm.contains("add x11, x11, x10, lsl #4"));
        assert!(asm.contains("ldr x2, [x9, #16]"));
        assert!(asm.contains("ldr x1, [x9, #8]"));
        assert_eq!(asm.matches("mov x0, #255").count(), 2);
        // The stream and the exit status both live in x0, so they are pinned separately now that
        // `mov x0, #1` is a legitimate fd. The count above is what still guards the exit status —
        // it caught a revision that had left the fallback on the old `1`.
        assert_eq!(asm.matches("mov x0, #2\n").count(), 0, "the report must not write to stderr");
        assert!(
            asm.matches("mov x0, #1\n").count() >= 2,
            "every write in the report goes to stdout, where PHP puts it"
        );
    }

    /// Verifies the AArch64 location suffix reads the payload line and formats it through itoa.
    ///
    /// The empty-message branch must land on `__rt_uncaught_location`, NOT on
    /// `__rt_uncaught_newline`: skipping one label too far drops the file and line along with the
    /// `": "` separator, and every test with a non-empty message would still pass.
    #[test]
    fn test_uncaught_report_arm64_appends_the_source_location() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_report_uncaught_exception(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("cbz x2, __rt_uncaught_location"));
        assert!(asm.contains("__rt_uncaught_location:\n"));
        assert!(asm.contains(&format!("ldr x10, [x9, #{}]", THROWABLE_CREATION_LINE_OFFSET)));
        assert!(asm.contains("cbz x10, __rt_uncaught_newline"));
        assert!(asm.contains("cbz x11, __rt_uncaught_newline"));
        assert!(asm.contains("_uncaught_exc_in"));
        assert!(asm.contains("_script_source_file"));
        assert!(asm.contains("_uncaught_exc_colon"));
        assert!(asm.contains(&format!("ldr x0, [x9, #{}]", THROWABLE_CREATION_LINE_OFFSET)));
        assert!(asm.contains("bl __rt_itoa"));
    }

    /// Verifies the x86_64 report makes the same decisions, in SysV registers.
    ///
    /// The x86 arm exists as its own emitter, so it can drift independently; an upstream leak fix
    /// was silently deleted from `implode.rs` on this exact architecture because nothing pinned
    /// it. `shl r9, 4` is the stride the AArch64 arm expresses as a shifted add.
    #[test]
    fn test_uncaught_report_x86_64_reads_class_and_message() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_report_uncaught_exception(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_report_uncaught_exception:\n"));
        assert!(asm.contains("jz __rt_uncaught_fallback"));
        assert!(asm.contains("jae __rt_uncaught_no_name"));
        assert!(asm.contains("shl r9, 4"));
        assert!(asm.contains("mov rdx, QWORD PTR [r8 + 16]"));
        assert!(asm.contains("jz __rt_uncaught_newline"));
        assert!(asm.contains("mov rsi, QWORD PTR [r8 + 8]"));
        assert_eq!(asm.matches("mov edi, 255").count(), 2);
        // See the AArch64 test: stream and exit status are pinned apart.
        assert_eq!(asm.matches("mov edi, 2\n").count(), 0, "the report must not write to stderr");
        assert!(
            asm.matches("mov edi, 1\n").count() >= 2,
            "every write in the report goes to stdout, where PHP puts it"
        );
    }

    /// Verifies the x86_64 location suffix makes the same decisions, in SysV registers.
    ///
    /// `mov rsi, rax` is the one instruction with no AArch64 counterpart and no visible effect
    /// until it is missing: `__rt_itoa` returns the digits in `rax`, which the very next
    /// instruction overwrites with the `write` syscall number. Without the move, stderr gets a
    /// pointer built from `1` and the process dies on a bad address instead of printing a line.
    #[test]
    fn test_uncaught_report_x86_64_appends_the_source_location() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_report_uncaught_exception(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("jz __rt_uncaught_location"));
        assert!(asm.contains("__rt_uncaught_location:\n"));
        assert!(asm.contains(&format!(
            "mov r9, QWORD PTR [r8 + {}]",
            THROWABLE_CREATION_LINE_OFFSET
        )));
        assert_eq!(asm.matches("jz __rt_uncaught_newline").count(), 2);
        assert!(asm.contains("_uncaught_exc_in"));
        assert!(asm.contains("_script_source_file"));
        assert!(asm.contains("_uncaught_exc_colon"));
        assert!(asm.contains(&format!(
            "mov rax, QWORD PTR [r8 + {}]",
            THROWABLE_CREATION_LINE_OFFSET
        )));
        assert!(asm.contains("call __rt_itoa"));
        assert!(asm.contains("mov rsi, rax"));
    }

    /// Verifies both throw paths JUMP to the report instead of inlining the old constant write.
    ///
    /// The tail call is what makes the two architectures share one implementation; an inlined copy
    /// is exactly how the two arms drifted apart before.
    #[test]
    fn test_throw_current_delegates_to_the_report_on_both_arches() {
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        super::super::emit_throw_current(&mut arm);
        let arm_asm = arm.output();
        assert!(arm_asm.contains("b __rt_report_uncaught_exception"));
        assert!(!arm_asm.contains("_uncaught_exc_msg"));

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        super::super::emit_throw_current(&mut x86);
        let x86_asm = x86.output();
        assert!(x86_asm.contains("jmp __rt_report_uncaught_exception"));
        assert!(!x86_asm.contains("_uncaught_exc_msg"));
    }
}
