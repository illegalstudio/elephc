//! Purpose:
//! Records one entry per productive cycle-collector run and emits the
//! `--gc-stats` per-run report helper.
//!
//! Called from:
//! - The AArch64 and x86_64 cycle-collector emitters after a productive run.
//! - `emit_gc_control` once per runtime to define the report printer.
//!
//! Key details:
//! - Records are fixed-size and chronological; runs past the capacity are
//!   counted as dropped rather than overwriting earlier rows.
//! - The report uses the runtime's integer formatter and raw stderr writes so it
//!   works without the Rust debug bridge.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Number of productive collector runs retained in the report.
pub(crate) const GC_STATS_RECORD_CAPACITY: usize = 256;
/// Bytes per record: collected, duration, before, after, trigger, reserved.
pub(crate) const GC_STATS_RECORD_SIZE: usize = 48;
/// Report literal labels and their byte lengths, emitted by the fixed data section.
pub(crate) const GC_STATS_LITERALS: &[(&str, &str)] = &[
    ("_gc_stats_run_prefix", "GC run "),
    ("_gc_stats_collected", ": collected="),
    ("_gc_stats_duration", " duration_ns="),
    ("_gc_stats_before", " before="),
    ("_gc_stats_after", " after="),
    ("_gc_stats_trigger", " trigger="),
    ("_gc_stats_explicit", "explicit\n"),
    ("_gc_stats_automatic", "automatic\n"),
    ("_gc_stats_newline", "\n"),
    ("_gc_stats_count_prefix", "GC runs: "),
    ("_gc_stats_dropped_prefix", "GC runs dropped: "),
];

/// Emits the append of the current productive run's already-published values.
///
/// Input: the reclaimed node count is in `x0` (AArch64) or `rax` (x86_64). The
/// helper only uses caller-saved scratch registers and preserves that result.
pub(crate) fn emit_record_run(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // reserve a scratch frame for the record append
            emitter.instruction("str x9, [sp]");                                // preserve the scratch register used by symbol loads
            emitter.instruction("str x10, [sp, #8]");                           // preserve the slot/count register
            crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_gc_stats_run_records");
            crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_gc_stats_run_count");
            emitter.instruction("ldr x11, [x10]");                              // load the completed record count
            emitter.instruction(&format!("cmp x11, #{GC_STATS_RECORD_CAPACITY}"));// has the fixed table filled?
            emitter.instruction("b.hs __rt_gc_stats_record_dropped");           // keep the first rows and count overflow
            emitter.instruction(&format!("mov x12, #{GC_STATS_RECORD_SIZE}"));  // record stride
            emitter.instruction("mul x12, x11, x12");                           // byte offset of this run
            emitter.instruction("add x12, x9, x12");                            // address of this run's record
            emitter.instruction("str x0, [x12]");                               // publish reclaimed node count
            crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_gc_last_pass_duration_ns");
            emitter.instruction("ldr x14, [x13]");                              // load this run's duration
            emitter.instruction("str x14, [x12, #8]");                          // publish duration in nanoseconds
            crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_gc_last_pass_memory_before");
            emitter.instruction("ldr x14, [x13]");                              // load the pre-collection memory sample
            emitter.instruction("str x14, [x12, #16]");                         // publish live bytes before collection
            crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_gc_live");
            emitter.instruction("ldr x14, [x13]");                              // load the post-collection live bytes
            emitter.instruction("str x14, [x12, #24]");                         // publish live bytes after collection
            crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_gc_run_explicit");
            emitter.instruction("ldr x14, [x13]");                              // load the explicit/automatic trigger
            emitter.instruction("str x14, [x12, #32]");                         // publish the trigger kind
            emitter.instruction("str xzr, [x12, #40]");                         // keep the reserved word zero
            emitter.instruction("add x11, x11, #1");                            // include this run in the report
            emitter.instruction("str x11, [x10]");                              // persist the completed record count
            emitter.instruction("b __rt_gc_stats_record_done");                 // finish the append
            emitter.label("__rt_gc_stats_record_dropped");
            crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_gc_stats_run_dropped");
            emitter.instruction("ldr x11, [x10]");                              // load the overflow count
            emitter.instruction("add x11, x11, #1");                            // count this run as dropped
            emitter.instruction("str x11, [x10]");                              // persist the overflow count
            emitter.label("__rt_gc_stats_record_done");
            emitter.instruction("ldr x9, [sp]");                                // restore the scratch register
            emitter.instruction("ldr x10, [sp, #8]");                           // restore the count register
            emitter.instruction("add sp, sp, #16");                             // release the scratch frame
        }
        Arch::X86_64 => {
            emitter.instruction("push r11");                                    // preserve the symbol scratch register
            emitter.instruction("push r10");                                    // preserve the record address register
            crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_gc_stats_run_records");
            crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_gc_stats_run_count");
            emitter.instruction("mov r10, QWORD PTR [r9]");                     // load the completed record count
            emitter.instruction(&format!("cmp r10, {GC_STATS_RECORD_CAPACITY}"));// has the fixed table filled?
            emitter.instruction("jae __rt_gc_stats_record_dropped");            // keep the first rows and count overflow
            emitter.instruction(&format!("imul r11, r10, {GC_STATS_RECORD_SIZE}"));// byte offset of this run
            emitter.instruction("add r11, r8");                                 // address of this run's record
            emitter.instruction("mov QWORD PTR [r11], rax");                    // publish reclaimed node count
            crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_gc_last_pass_duration_ns");
            emitter.instruction("mov rdx, QWORD PTR [r8]");                     // load this run's duration
            emitter.instruction("mov QWORD PTR [r11 + 8], rdx");                // publish duration in nanoseconds
            crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_gc_last_pass_memory_before");
            emitter.instruction("mov rdx, QWORD PTR [r8]");                     // load the pre-collection memory sample
            emitter.instruction("mov QWORD PTR [r11 + 16], rdx");               // publish live bytes before collection
            crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_gc_live");
            emitter.instruction("mov rdx, QWORD PTR [r8]");                     // load the post-collection live bytes
            emitter.instruction("mov QWORD PTR [r11 + 24], rdx");               // publish live bytes after collection
            crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_gc_run_explicit");
            emitter.instruction("mov rdx, QWORD PTR [r8]");                     // load the explicit/automatic trigger
            emitter.instruction("mov QWORD PTR [r11 + 32], rdx");               // publish the trigger kind
            emitter.instruction("mov QWORD PTR [r11 + 40], 0");                 // keep the reserved word zero
            emitter.instruction("add r10, 1");                                  // include this run in the report
            emitter.instruction("mov QWORD PTR [r9], r10");                     // persist the completed record count
            emitter.instruction("jmp __rt_gc_stats_record_done");               // finish the append
            emitter.label("__rt_gc_stats_record_dropped");
            crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_gc_stats_run_dropped");
            emitter.instruction("add QWORD PTR [r9], 1");                       // count this run as dropped
            emitter.label("__rt_gc_stats_record_done");
            emitter.instruction("pop r10");                                     // restore the record address register
            emitter.instruction("pop r11");                                     // restore the symbol scratch register
        }
    }
}

/// Writes one report literal by label, using the shared stderr literal helper.
fn write_literal(emitter: &mut Emitter, label: &str) {
    let Some((label, text)) = GC_STATS_LITERALS
        .iter()
        .find(|(candidate, _)| *candidate == label)
        .copied()
    else {
        return;
    };
    crate::codegen_support::driver_support::emit_write_literal_stderr(
        emitter,
        label,
        text.len(),
    );
}

/// Emits the per-run table printer called once by the `--gc-stats` epilogue.
pub(crate) fn emit_gc_stats_report(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.label_global("__rt_gc_stats_report");
            emitter.instruction("stp x29, x30, [sp, #-48]!");                   // preserve the caller frame for the report loop
            emitter.instruction("mov x29, sp");                                 // establish the report frame pointer
            emitter.instruction("stp x19, x20, [sp, #16]");                     // preserve the record pointer and remaining count
            emitter.instruction("str x21, [sp, #32]");                          // preserve the one-based run index
            crate::codegen_support::abi::emit_symbol_address(emitter, "x19", "_gc_stats_run_records");
            crate::codegen_support::abi::emit_symbol_address(emitter, "x20", "_gc_stats_run_count");
            emitter.instruction("ldr x20, [x20]");                              // load the number of retained runs
            write_literal(emitter, "_gc_stats_count_prefix");
            emitter.instruction("mov x0, x20");                                 // format the retained-run count
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_newline");
            emitter.instruction("cbz x20, __rt_gc_stats_report_dropped");       // an empty table has only the count line
            emitter.instruction("mov x21, #1");                                 // rows are one-based
            emitter.label("__rt_gc_stats_report_loop");
            write_literal(emitter, "_gc_stats_run_prefix");
            emitter.instruction("mov x0, x21");                                 // format the one-based row number
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_collected");
            emitter.instruction("ldr x0, [x19]");                               // load this run's reclaimed nodes
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_duration");
            emitter.instruction("ldr x0, [x19, #8]");                           // load this run's duration in nanoseconds
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_before");
            emitter.instruction("ldr x0, [x19, #16]");                          // load live bytes before this run
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_after");
            emitter.instruction("ldr x0, [x19, #24]");                          // load live bytes after this run
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_trigger");
            emitter.instruction("ldr x9, [x19, #32]");                          // load the explicit/automatic trigger
            emitter.instruction("cbz x9, __rt_gc_stats_report_automatic");      // zero is an automatic safe point
            write_literal(emitter, "_gc_stats_explicit");
            emitter.instruction("b __rt_gc_stats_report_next");                 // skip the automatic spelling
            emitter.label("__rt_gc_stats_report_automatic");
            write_literal(emitter, "_gc_stats_automatic");
            emitter.label("__rt_gc_stats_report_next");
            emitter.instruction("add x19, x19, #48");                           // advance to the next recorded run
            emitter.instruction("add x21, x21, #1");                            // advance the one-based row number
            emitter.instruction("subs x20, x20, #1");                           // decrement the remaining rows
            emitter.instruction("b.ne __rt_gc_stats_report_loop");              // keep printing retained runs
            emitter.label("__rt_gc_stats_report_dropped");
            crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_gc_stats_run_dropped");
            emitter.instruction("ldr x9, [x9]");                                // load the overflow count
            emitter.instruction("cbz x9, __rt_gc_stats_report_done");           // omit the line when nothing was dropped
            write_literal(emitter, "_gc_stats_dropped_prefix");
            emitter.instruction("mov x0, x9");                                  // format the dropped-run count
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_newline");
            emitter.label("__rt_gc_stats_report_done");
            emitter.instruction("ldr x21, [sp, #32]");                          // restore the row index
            emitter.instruction("ldp x19, x20, [sp, #16]");                     // restore the record pointer and remaining count
            emitter.instruction("ldp x29, x30, [sp], #48");                     // restore the caller frame and return address
            emitter.instruction("ret");                                         // return to the gc-stats epilogue
        }
        Arch::X86_64 => {
            emitter.label_global("__rt_gc_stats_report");
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the report frame pointer
            emitter.instruction("push r12");                                    // preserve the record pointer
            emitter.instruction("push r13");                                    // preserve the remaining-row count
            emitter.instruction("push r14");                                    // preserve the one-based row index
            emitter.instruction("sub rsp, 8");                                  // keep the report frame aligned before formatter calls
            crate::codegen_support::abi::emit_symbol_address(emitter, "r12", "_gc_stats_run_records");
            crate::codegen_support::abi::emit_symbol_address(emitter, "r13", "_gc_stats_run_count");
            emitter.instruction("mov r13, QWORD PTR [r13]");                    // load the number of retained runs
            write_literal(emitter, "_gc_stats_count_prefix");
            emitter.instruction("mov rax, r13");                                // format the retained-run count
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_newline");
            emitter.instruction("test r13, r13");                               // is the table empty?
            emitter.instruction("jz __rt_gc_stats_report_dropped");             // an empty table has only the count line
            emitter.instruction("mov r14, 1");                                  // rows are one-based
            emitter.label("__rt_gc_stats_report_loop");
            write_literal(emitter, "_gc_stats_run_prefix");
            emitter.instruction("mov rax, r14");                                // format the one-based row number
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_collected");
            emitter.instruction("mov rax, QWORD PTR [r12]");                    // load this run's reclaimed nodes
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_duration");
            emitter.instruction("mov rax, QWORD PTR [r12 + 8]");                // load this run's duration in nanoseconds
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_before");
            emitter.instruction("mov rax, QWORD PTR [r12 + 16]");               // load live bytes before this run
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_after");
            emitter.instruction("mov rax, QWORD PTR [r12 + 24]");               // load live bytes after this run
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_trigger");
            emitter.instruction("mov r9, QWORD PTR [r12 + 32]");                // load the explicit/automatic trigger
            emitter.instruction("test r9, r9");                                 // is this an explicit collection?
            emitter.instruction("jz __rt_gc_stats_report_automatic");           // zero is an automatic safe point
            write_literal(emitter, "_gc_stats_explicit");
            emitter.instruction("jmp __rt_gc_stats_report_next");               // skip the automatic spelling
            emitter.label("__rt_gc_stats_report_automatic");
            write_literal(emitter, "_gc_stats_automatic");
            emitter.label("__rt_gc_stats_report_next");
            emitter.instruction("add r12, 48");                                 // advance to the next recorded run
            emitter.instruction("add r14, 1");                                  // advance the one-based row number
            emitter.instruction("sub r13, 1");                                  // decrement the remaining rows
            emitter.instruction("jnz __rt_gc_stats_report_loop");               // keep printing retained runs
            emitter.label("__rt_gc_stats_report_dropped");
            crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_gc_stats_run_dropped");
            emitter.instruction("mov r9, QWORD PTR [r9]");                      // load the overflow count
            emitter.instruction("test r9, r9");                                 // nothing to report when no run was dropped
            emitter.instruction("jz __rt_gc_stats_report_done");                // finish when no run was dropped
            write_literal(emitter, "_gc_stats_dropped_prefix");
            emitter.instruction("mov rax, r9");                                 // format the dropped-run count
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_itoa");
            crate::codegen_support::driver_support::emit_write_current_string_stderr(emitter);
            write_literal(emitter, "_gc_stats_newline");
            emitter.label("__rt_gc_stats_report_done");
            emitter.instruction("add rsp, 8");                                  // release the report alignment padding
            emitter.instruction("pop r14");                                     // restore the row index
            emitter.instruction("pop r13");                                     // restore the remaining-row count
            emitter.instruction("pop r12");                                     // restore the record pointer
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return to the gc-stats epilogue
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Verifies every supported target emits the append and report helpers.
    #[test]
    fn gc_stats_helpers_are_emitted_for_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_record_run(&mut emitter);
            let append = emitter.output();
            assert!(
                append.contains("_gc_stats_run_records"),
                "{name} misses the per-run record table"
            );
            assert!(
                append.contains("_gc_stats_run_count"),
                "{name} misses the per-run count"
            );

            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_gc_stats_report(&mut emitter);
            let report = emitter.output();
            assert!(
                report.contains("__rt_gc_stats_report:"),
                "{name} misses the report helper"
            );
            assert!(
                report.contains("_gc_stats_run_prefix"),
                "{name} misses the report row prefix"
            );
            assert!(
                report.contains("__rt_itoa"),
                "{name} misses the shared integer formatter"
            );
        }
    }
}
