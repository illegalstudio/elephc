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
//! - The ` in <file>:<line>` suffix uses the same borrowed lookup as the public getters.
//!   Ordinary and compact creation storage supply defaults; ErrorException overrides either
//!   component independently. Explicit line zero and an empty filename remain visible.
//!   Synthesized errors with no known location omit the suffix.
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
//! - The published exception owns borrowed filename bytes throughout reporting. A small
//!   stack area preserves the pointer, length, and line across syscalls. The x86_64 entry
//!   realigns the stack before any nested helper call; this fatal path never returns.

use crate::codegen_support::platform::Arch;
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

    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_prefix");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_PREFIX_LEN));          // "Fatal error: Uncaught "
    emitter.instruction("mov x0, #1");                                          // fd = stdout: PHP writes this report to stdout, not stderr (measured)
    emitter.syscall(4);

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
    emitter.instruction("sub sp, sp, #32");                                     // preserve borrowed filename and line across write syscalls
    abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_throwable_location_borrowed");
    emitter.instruction("stp x1, x2, [sp]");                                    // save the resolved borrowed filename pointer and length
    emitter.instruction("str x0, [sp, #16]");                                   // save the effective line before output overwrites result registers
    emitter.instruction("cbz x3, __rt_uncaught_newline");                       // omit only unknown origins, preserving explicit zero and empty overrides
    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_in");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_IN_LEN));              // introduce the effective location
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("ldp x1, x2, [sp]");                                    // print the resolved filename, including an explicit empty string
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_colon");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_COLON_LEN));           // separate the filename and line
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("ldr x0, [sp, #16]");                                   // format the effective line saved before output began
    emitter.instruction("bl __rt_itoa");                                        // return digit bytes in the write argument registers
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);

    emitter.label("__rt_uncaught_newline");
    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_nl");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_NEWLINE_LEN));         // terminating newline
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
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

    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_prefix");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_PREFIX_LEN));          // "Fatal error: Uncaught "
    emitter.instruction("mov edi, 1");                                          // fd = stdout: PHP writes this report to stdout, not stderr (measured)
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the fatal-error prefix to stdout

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
    emitter.instruction("syscall");                                             // write the throwable class name to stdout

    emitter.label("__rt_uncaught_no_name");
    emitter.instruction("mov rdx, QWORD PTR [r8 + 16]");                        // Throwable message length lives at payload offset 16
    emitter.instruction("test rdx, rdx");                                       // an EMPTY message drops the ": " separator, matching reference PHP
    emitter.instruction("jz __rt_uncaught_location");                           // an EMPTY message keeps the location suffix, only the ": " separator goes
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_sep");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_SEPARATOR_LEN));       // ": "
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the message separator to stdout
    emitter.instruction("mov rsi, QWORD PTR [r8 + 8]");                         // Throwable message pointer lives at payload offset 8
    emitter.instruction("mov rdx, QWORD PTR [r8 + 16]");                        // and its length at offset 16
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the throwable message to stdout

    emitter.label("__rt_uncaught_location");
    emitter.instruction("sub rsp, 32");                                         // preserve borrowed filename and line across write syscalls
    abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_throwable_location_borrowed");
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // save the resolved borrowed filename pointer
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // save the resolved filename length, including an explicit zero
    emitter.instruction("mov QWORD PTR [rsp + 16], rcx");                       // preserve the effective line before syscall clobbers rcx
    emitter.instruction("test r8, r8");                                         // distinguish explicit zero from an unknown origin
    emitter.instruction("jz __rt_uncaught_newline");                            // omit only unknown origins, preserving explicit zero and empty overrides
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_in");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_IN_LEN));              // introduce the effective location
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the location introducer to stdout
    emitter.instruction("mov rsi, QWORD PTR [rsp]");                            // print the filename bytes owned by the published exception
    emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                        // preserve an explicitly empty filename
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the effective filename to stdout
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_colon");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_COLON_LEN));           // separate the filename and line
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the filename/line separator to stdout
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // format the line saved before the first syscall
    emitter.instruction("call __rt_itoa");                                      // return digit bytes and length in the ordinary result registers
    emitter.instruction("mov rsi, rax");                                        // preserve digit bytes before loading the write syscall number
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the effective line to stdout

    emitter.label("__rt_uncaught_newline");
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_nl");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_NEWLINE_LEN));         // terminating newline
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // terminate the uncaught-exception diagnostic with a newline
    emitter.instruction(&format!("mov edi, {}", UNCAUGHT_EXIT_STATUS));         // PHP exits 255 for an uncaught exception
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // exit the process after reporting the throwable
    emitter.instruction("ud2");                                                 // keep the terminal syscall from falling through into a differently sized frame

    emitter.label("__rt_uncaught_fallback");
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_msg");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_FALLBACK_LEN));        // the pre-existing constant message
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the constant fallback diagnostic to stdout
    emitter.instruction(&format!("mov edi, {}", UNCAUGHT_EXIT_STATUS));         // same status as the reporting path
    emitter.instruction("mov eax, 60");                                         // syscall 60 = exit
    emitter.instruction("syscall");                                             // exit the process after the fallback diagnostic
    emitter.instruction("ud2");                                                 // terminate control flow if the exit syscall unexpectedly returns
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
        assert_eq!(asm.matches("mov x0, #2\n").count(), 0, "the report must not write to stdout");
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
        assert!(asm.contains("bl __rt_throwable_location_borrowed"));
        assert!(asm.contains("cbz x3, __rt_uncaught_newline"));
        assert!(asm.contains("ldp x1, x2, [sp]"));
        assert!(asm.contains("_uncaught_exc_in"));
        assert!(asm.contains("__rt_throwable_location_borrowed"));
        assert!(asm.contains("_uncaught_exc_colon"));
        assert!(asm.contains("ldr x0, [sp, #16]"));
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
        assert_eq!(asm.matches("mov edi, 2\n").count(), 0, "the report must not write to stdout");
        assert!(
            asm.matches("mov edi, 1\n").count() >= 2,
            "every write in the report goes to stdout, where PHP puts it"
        );
    }

    /// Verifies the x86_64 location suffix makes the same decisions, in SysV registers.
    ///
    /// `mov rsi, rax` is the one instruction with no AArch64 counterpart and no visible effect
    /// until it is missing: `__rt_itoa` returns the digits in `rax`, which the very next
    /// instruction overwrites with the `write` syscall number. Without the move, stdout gets a
    /// pointer built from `1` and the process dies on a bad address instead of printing a line.
    #[test]
    fn test_uncaught_report_x86_64_appends_the_source_location() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_report_uncaught_exception(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("jz __rt_uncaught_location"));
        assert!(asm.contains("__rt_uncaught_location:\n"));
        assert!(asm.contains("call __rt_throwable_location_borrowed"));
        assert_eq!(asm.matches("jz __rt_uncaught_newline").count(), 1);
        assert!(asm.contains("mov rsi, QWORD PTR [rsp]"));
        assert!(asm.contains("_uncaught_exc_in"));
        assert!(asm.contains("__rt_throwable_location_borrowed"));
        assert!(asm.contains("_uncaught_exc_colon"));
        assert!(asm.contains("mov rax, QWORD PTR [rsp + 16]"));
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
        assert!(x86_asm.contains("jz __rt_report_uncaught_exception"));
        assert!(!x86_asm.contains("_uncaught_exc_msg"));
    }
}
