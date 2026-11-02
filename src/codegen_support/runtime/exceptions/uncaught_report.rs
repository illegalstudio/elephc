//! Purpose:
//! Emits `__rt_report_uncaught_exception`, the fatal path taken when a `throw` finds no active
//! handler. It prints PHP's complete uncaught-exception report -- class, message, source
//! location, the `Stack trace:` block, the `Next ...` chain of previous exceptions and the
//! closing `thrown in` line -- then exits with PHP's status.
//!
//! Called from:
//! - `super::throw_current::emit_throw_current` on both architectures, replacing an inline
//!   `write()` of the constant `_uncaught_exc_msg`.
//! - `__rt_report_debuginfo_exception`, the entry for an exception escaping a run-time
//!   `__debugInfo()` call, which selects the `Warning: Uncaught ...` flavour and appends PHP's
//!   `__debuginfo() must return an array` fatal.
//!
//! Key details:
//! - Reference PHP 8.5, measured with `php -n -d display_errors=1`:
//!
//!   ```text
//!   (blank line)
//!   Fatal error: Uncaught RuntimeException: boom in /path/t.php:3
//!   Stack trace:
//!   #0 /path/t.php(7): f(1)
//!   #1 {main}
//!     thrown in /path/t.php on line 3
//!   ```
//!
//!   With a previous exception PHP prints the INNERMOST previous first and each outer one after
//!   a `\n\nNext ` separator, each with its own `Stack trace:` block; the single `thrown in`
//!   line closes the whole report and names the OUTERMOST (actually thrown) exception:
//!
//!   ```text
//!   Fatal error: Uncaught LogicException: root in /path/c.php:4
//!   Stack trace:
//!   #0 {main}
//!
//!   Next RuntimeException: boom in /path/c.php:6
//!   Stack trace:
//!   #0 {main}
//!     thrown in /path/c.php on line 6
//!   ```
//!
//!   An empty message drops the `: ` separator entirely (`Uncaught Exception in /path/m1.php:2`).
//!
//! - THE ` in <file>:<line>` SUFFIX and the closing `thrown in` line use
//!   `__rt_throwable_location_borrowed`, the lookup behind `getFile()`/`getLine()`: the
//!   construction site by default, with ErrorException overrides applied per component. A
//!   synthesized error with no known origin omits both rather than printing a fabricated `:0`.
//!
//! - THE STACK TRACE is rendered by walking the activation chain (`_exc_call_frame_top`) and
//!   calling each frame's reader (`[activation + 24]`, published by the function prologue, see
//!   `codegen::frame`) in PRINT mode: the very routine behind `debug_print_backtrace()`, whose
//!   `#<n> <file>(<line>): <function>(<args>)` line is byte-for-byte the shape of an exception
//!   trace frame. `#<count> {main}` closes the block. Programs without a `throw` carry no readers
//!   (the prologue publishes `0`), so only `#0 {main}` is printed for them.
//!
//!   The walk happens at REPORT time, not at construction time as in PHP, so every exception of a
//!   chain prints the frames live at the uncaught throw. That is exact for the usual case
//!   (constructed and thrown in the same call chain) and diverges for a previous exception built
//!   in a callee that has since returned, because the activation chain no longer holds those
//!   frames. Internal-function frames (`#0 [internal function]: ...`, `intdiv(1, 0)`) have no
//!   activation either and are not printed.
//!
//! - All output goes through `__rt_stdout_write`, the funnel behind `echo`, after the output
//!   buffers are flushed, so the report reaches stdout (PHP CLI writes it there) and, in a
//!   `--web` worker, the response body instead of a stray process descriptor.
//!
//! - The exit status moves from `1` to `255`, which is what reference PHP returns for an uncaught
//!   exception.
//!
//! - A null `_exc_value` keeps the original constant message. That slot is written by
//!   `lower_throw_value` immediately before the call, so null means the runtime reached this path
//!   without a throwable (an internal invariant break); reporting a class read from a null pointer
//!   would turn a diagnostic into a segfault.
//!
//! - Register discipline: the helpers below run on a fresh frame of their own (the entry is a TAIL
//!   JUMP from `__rt_throw_current`, so there is none to reuse) and keep the throwable in a
//!   callee-saved register. The frame readers do not preserve callee-saved registers, so the
//!   trace walker keeps its cursor and index in stack slots, and every routine that calls it
//!   restores its own saved registers on exit. x86_64 realigns `rsp` once at the entry because the
//!   tail jump may arrive 8-off.

use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::{DEBUGINFO_FATAL_MSG, DEBUGINFO_WARNING_PREFIX};
use crate::codegen_support::{abi, emit::Emitter};

/// Byte length of `"\nFatal error: Uncaught "`.
const UNCAUGHT_PREFIX_LEN: usize = 23;

/// Byte length of `"\nWarning: Uncaught "`, the prefix used for an exception escaping `__debugInfo()`.
const DEBUGINFO_WARNING_PREFIX_LEN: usize = DEBUGINFO_WARNING_PREFIX.len();

/// Byte length of the `": "` separator between class name and message.
const UNCAUGHT_SEPARATOR_LEN: usize = 2;

/// Byte length of the `" in "` lead-in before the source location.
const UNCAUGHT_IN_LEN: usize = 4;

/// Byte length of the `":"` between the filename and the line number.
const UNCAUGHT_COLON_LEN: usize = 1;

/// Byte length of a single `"\n"`.
const UNCAUGHT_NEWLINE_LEN: usize = 1;

/// Byte length of `"\nStack trace:\n"`.
const UNCAUGHT_STACK_TRACE_LEN: usize = 14;

/// Byte length of `"\n\nNext "`, the separator between a previous exception and the next one.
const UNCAUGHT_NEXT_LEN: usize = 7;

/// Byte length of `"#"`, the lead-in of every trace line.
const UNCAUGHT_HASH_LEN: usize = 1;

/// Byte length of `" {main}"`, the closing trace line after its `#<n>` index.
const UNCAUGHT_MAIN_LEN: usize = 7;

/// Byte length of `"\n  thrown in "`.
const UNCAUGHT_THROWN_IN_LEN: usize = 13;

/// Byte length of `" on line "`.
const UNCAUGHT_ON_LINE_LEN: usize = 9;

/// Byte length of the constant fallback message, kept in sync with `data::fixed`.
const UNCAUGHT_FALLBACK_LEN: usize = 32;

/// PHP's process exit status for an uncaught exception.
///
/// Shared with `codegen::lower_inst::exceptions`, which has its OWN uncaught path for the errors
/// it synthesizes (a `DivisionByZeroError` from `intdiv($n, 0)`, say): that path short-circuits
/// before the throwable is allocated and never reaches this helper. Both must agree, or the exit
/// status a script sees would depend on which kind of exception went unhandled.
pub(crate) const UNCAUGHT_EXIT_STATUS: u32 = 255;

/// Emits `__rt_report_uncaught_exception` and its helpers; the report never returns.
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
    // which discarded every byte a program had buffered -- `ob_start(); echo "x"; throw ...` printed
    // nothing at all on stdout. Nothing is live yet at the entry, so the call costs no spill.
    emitter.instruction("mov x10, #0");                                         // zero clears the capture flags below
    abi::emit_store_reg_to_symbol(emitter, "x10", "_print_r_mode", 0);          // an escaping exception abandons a print_r(..., true) capture: the report must reach stdout
    abi::emit_store_reg_to_symbol(emitter, "x10", "_ob_in_handler", 0);         // and output produced inside an ob_start() handler is no longer discarded
    emitter.instruction("bl __rt_ob_flush_all");                                // drain every open output buffer before the report is written

    abi::emit_load_symbol_to_reg(emitter, "x10", "_uncaught_report_mode", 0);
    abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_value", 0);
    emitter.instruction("cbz x9, __rt_uncaught_fallback");                      // no throwable published: keep the constant message rather than dereferencing null

    emitter.instruction("sub sp, sp, #16");                                     // reserve the report's own linkage frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address for the nested helper calls
    emitter.instruction("mov x29, sp");                                         // establish the frame pointer
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_prefix");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_PREFIX_LEN));          // "\nFatal error: Uncaught "
    emitter.instruction("cbz x10, __rt_uncaught_prefix_ready");                 // ordinary uncaught exception: keep the fatal prefix
    abi::emit_symbol_address(emitter, "x0", "_debuginfo_warning_prefix");
    emitter.instruction(&format!("mov x1, #{}", DEBUGINFO_WARNING_PREFIX_LEN)); // "\nWarning: Uncaught " for an exception escaping __debugInfo()
    emitter.label("__rt_uncaught_prefix_ready");
    emitter.instruction("bl __rt_stdout_write");                                // PHP writes this report to stdout, not stderr (measured)

    abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
    emitter.instruction("bl __rt_uncaught_chain_length");                       // count the distinct links: a re-run constructor can close a cycle
    emitter.instruction("mov x1, x0");                                          // render exactly that many links
    abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
    emitter.instruction("bl __rt_uncaught_render_chain");                       // previous chain first, then the thrown exception, each with its trace

    // -- closing `  thrown in <file> on line <n>` for the OUTERMOST exception --
    abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
    emitter.instruction("bl __rt_throwable_location_borrowed");                 // the same effective location the getters report
    emitter.instruction("cbz x3, __rt_uncaught_newline");                       // no known origin: omit rather than invent
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_thrown_in");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_THROWN_IN_LEN));       // "\n  thrown in "
    emitter.instruction("bl __rt_stdout_write");                                // introduce the file of the thrown exception
    abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
    emitter.instruction("bl __rt_uncaught_write_location_file");                // its effective filename
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_on_line");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_ON_LINE_LEN));         // " on line "
    emitter.instruction("bl __rt_stdout_write");                                // introduce the line number
    abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
    emitter.instruction("bl __rt_throwable_location_borrowed");                 // the effective line, in x0
    emitter.instruction("bl __rt_itoa");                                        // returns pointer in x1 and length in x2
    emitter.instruction("mov x0, x1");                                          // the write funnel takes the digits pointer in x0
    emitter.instruction("mov x1, x2");                                          // and their length in x1
    emitter.instruction("bl __rt_stdout_write");                                // write the decimal creation line

    emitter.label("__rt_uncaught_newline");
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_nl");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_NEWLINE_LEN));         // terminating newline
    emitter.instruction("bl __rt_stdout_write");                                // terminate the report
    abi::emit_load_symbol_to_reg(emitter, "x10", "_uncaught_report_mode", 0);
    emitter.instruction("cbz x10, __rt_uncaught_exit");                         // ordinary uncaught exception: the report is complete
    emitter.instruction("bl __rt_debuginfo_fatal_text");                        // PHP follows the warning with the `__debuginfo() must return an array` fatal
    emitter.label("__rt_uncaught_exit");
    emitter.instruction("bl __rt_run_exit_hook");                               // shut hosted PHP extensions down (RSHUTDOWN, MSHUTDOWN)
    emitter.instruction(&format!("mov x0, #{}", UNCAUGHT_EXIT_STATUS));         // PHP exits 255 for an uncaught exception
    emitter.syscall(1);

    emitter.label("__rt_uncaught_fallback");
    abi::emit_symbol_address(emitter, "x1", "_uncaught_exc_msg");
    emitter.instruction(&format!("mov x2, #{}", UNCAUGHT_FALLBACK_LEN));        // the pre-existing constant message
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);
    emitter.instruction("bl __rt_run_exit_hook");                               // shut hosted PHP extensions down (RSHUTDOWN, MSHUTDOWN)
    emitter.instruction(&format!("mov x0, #{}", UNCAUGHT_EXIT_STATUS));         // same status as the reporting path
    emitter.syscall(1);

    // -- chain length: x0 = Throwable; returns the number of links before one repeats --
    // `Exception::__construct()` can run again on a live object, so `$previous` links may close
    // a cycle. PHP's report stops at the first link it has already printed; the renderer below
    // is bounded by this count, found by scanning the accepted prefix for each new link.
    emitter.blank();
    emitter.comment("--- runtime: uncaught_chain_length ---");
    emitter.label_global("__rt_uncaught_chain_length");
    emitter.instruction("sub sp, sp, #64");                                     // reserve the callee-saved walk registers and linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the frame pointer
    emitter.instruction("stp x19, x20, [sp]");                                  // preserve the head and tail registers
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve the count and scan-cursor registers
    emitter.instruction("str x23, [sp, #32]");                                  // preserve the scan-budget register
    emitter.instruction("mov x19, x0");                                         // head of the chain: the thrown exception
    emitter.instruction("mov x20, x0");                                         // tail of the accepted prefix
    emitter.instruction("mov x21, #1");                                         // the thrown exception is always rendered
    emitter.label("__rt_uncaught_chain_length_next");
    emitter.instruction("mov x0, x20");                                         // follow the tail's previous link
    emitter.instruction("bl __rt_throwable_previous");                          // borrow it, or zero at the end of the chain
    emitter.instruction("cbz x0, __rt_uncaught_chain_length_done");             // the chain ended without repeating
    emitter.instruction("mov x20, x0");                                         // candidate link to accept
    emitter.instruction("mov x22, x19");                                        // scan the accepted prefix from the head
    emitter.instruction("mov x23, x21");                                        // which holds this many links
    emitter.label("__rt_uncaught_chain_length_scan");
    emitter.instruction("cmp x22, x20");                                        // was the candidate already accepted?
    emitter.instruction("b.eq __rt_uncaught_chain_length_done");                // a cycle: stop before printing it again
    emitter.instruction("subs x23, x23, #1");                                   // one fewer accepted link left to compare
    emitter.instruction("b.eq __rt_uncaught_chain_length_accept");              // the whole prefix differs: accept the candidate
    emitter.instruction("mov x0, x22");                                         // step the scan cursor
    emitter.instruction("bl __rt_throwable_previous");                          // to the next accepted link
    emitter.instruction("mov x22, x0");                                         // keep the advanced scan cursor
    emitter.instruction("b __rt_uncaught_chain_length_scan");                   // compare the candidate with it
    emitter.label("__rt_uncaught_chain_length_accept");
    emitter.instruction("add x21, x21, #1");                                    // count the accepted link
    emitter.instruction("b __rt_uncaught_chain_length_next");                   // and extend the walk from it
    emitter.label("__rt_uncaught_chain_length_done");
    emitter.instruction("mov x0, x21");                                         // return the number of links to render
    emitter.instruction("ldr x23, [sp, #32]");                                  // restore the scan-budget register
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore the count and scan-cursor registers
    emitter.instruction("ldp x19, x20, [sp]");                                  // restore the head and tail registers
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // back to the report entry

    // -- recursive chain renderer: x0 = Throwable, x1 = links left; previous exceptions print first --
    emitter.blank();
    emitter.comment("--- runtime: uncaught_render_chain ---");
    emitter.label_global("__rt_uncaught_render_chain");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the saved throwable and linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("stp x19, x20, [sp]");                                  // preserve the callee-saved throwable and budget registers
    emitter.instruction("add x29, sp, #16");                                    // establish the frame pointer
    emitter.instruction("mov x19, x0");                                         // keep the throwable across the helper calls
    emitter.instruction("mov x20, x1");                                         // keep the number of links still to render
    emitter.instruction("cmp x20, #1");                                         // is this the last distinct link?
    emitter.instruction("b.ls __rt_uncaught_render_chain_one");                 // yes: its previous link was already printed, or is absent
    emitter.instruction("bl __rt_throwable_previous");                          // borrow its previous exception, or zero
    emitter.instruction("cbz x0, __rt_uncaught_render_chain_one");              // no previous link: render only this exception
    emitter.instruction("sub x1, x20, #1");                                     // the older links get one fewer to render
    emitter.instruction("bl __rt_uncaught_render_chain");                       // render the older exceptions first, as PHP does
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_next");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_NEXT_LEN));            // "\n\nNext "
    emitter.instruction("bl __rt_stdout_write");                                // separate the older exception from this one
    emitter.label("__rt_uncaught_render_chain_one");
    emitter.instruction("mov x0, x19");                                         // render this exception's own block
    emitter.instruction("bl __rt_uncaught_render_one");                         // class, message, location and trace
    emitter.instruction("ldp x19, x20, [sp]");                                  // restore the caller's callee-saved registers
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // back to the previous level or the report entry

    // -- one exception block: x0 = Throwable --
    emitter.blank();
    emitter.comment("--- runtime: uncaught_render_one ---");
    emitter.label_global("__rt_uncaught_render_one");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the saved throwable and linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("str x19, [sp]");                                       // preserve the callee-saved throwable register
    emitter.instruction("add x29, sp, #16");                                    // establish the frame pointer
    emitter.instruction("mov x19, x0");                                         // keep the throwable across the writes

    emitter.instruction("ldr x10, [x19]");                                      // runtime class id from the object header
    abi::emit_symbol_address(emitter, "x11", "_class_name_count");
    emitter.instruction("ldr x11, [x11]");                                      // number of named class ids
    emitter.instruction("cmp x10, x11");                                        // is the class id inside the name table?
    emitter.instruction("b.hs __rt_uncaught_no_name");                          // an unknown id writes no class name, exactly as var_dump does
    abi::emit_symbol_address(emitter, "x11", "_class_name_entries");
    emitter.instruction("add x11, x11, x10, lsl #4");                           // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("ldr x0, [x11]");                                       // class-name pointer
    emitter.instruction("ldr x1, [x11, #8]");                                   // class-name length
    emitter.instruction("bl __rt_stdout_write");                                // write the throwable class name

    emitter.label("__rt_uncaught_no_name");
    emitter.instruction("ldr x1, [x19, #16]");                                  // Throwable message length lives at payload offset 16
    emitter.instruction("cbz x1, __rt_uncaught_location");                      // an EMPTY message drops the ": " separator, matching reference PHP -- but keeps the location
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_sep");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_SEPARATOR_LEN));       // ": "
    emitter.instruction("bl __rt_stdout_write");                                // write the message separator
    emitter.instruction("ldr x0, [x19, #8]");                                   // Throwable message pointer lives at payload offset 8
    emitter.instruction("ldr x1, [x19, #16]");                                  // and its length at offset 16
    emitter.instruction("bl __rt_stdout_write");                                // write the throwable message

    emitter.label("__rt_uncaught_location");
    emitter.instruction("mov x0, x19");                                         // resolve this exception's effective location
    emitter.instruction("bl __rt_throwable_location_borrowed");                 // line in x0, filename in x1/x2, known flag in x3
    emitter.instruction("cbz x3, __rt_uncaught_trace");                         // no known origin: omit rather than invent
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_in");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_IN_LEN));              // " in "
    emitter.instruction("bl __rt_stdout_write");                                // introduce the source location
    emitter.instruction("mov x0, x19");                                         // this exception's filename
    emitter.instruction("bl __rt_uncaught_write_location_file");                // write it
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_colon");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_COLON_LEN));           // ":"
    emitter.instruction("bl __rt_stdout_write");                                // separate the filename from the line
    emitter.instruction("mov x0, x19");                                         // this exception's line
    emitter.instruction("bl __rt_throwable_location_borrowed");                 // the effective line, in x0
    emitter.instruction("bl __rt_itoa");                                        // returns pointer in x1 and length in x2
    emitter.instruction("mov x0, x1");                                          // the write funnel takes the digits pointer in x0
    emitter.instruction("mov x1, x2");                                          // and their length in x1
    emitter.instruction("bl __rt_stdout_write");                                // write the decimal creation line

    emitter.label("__rt_uncaught_trace");
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_stack_trace");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_STACK_TRACE_LEN));     // "\nStack trace:\n"
    emitter.instruction("bl __rt_stdout_write");                                // open the trace block
    emitter.instruction("bl __rt_uncaught_write_trace");                        // one `#n file(line): call(args)` line per live frame; x0 = frame count
    emitter.instruction("mov x19, x0");                                         // the count is the index of the closing {main} line
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_hash");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_HASH_LEN));            // "#"
    emitter.instruction("bl __rt_stdout_write");                                // start the closing trace line
    emitter.instruction("mov x0, x19");                                         // itoa takes the frame count in x0
    emitter.instruction("bl __rt_itoa");                                        // returns pointer in x1 and length in x2
    emitter.instruction("mov x0, x1");                                          // the write funnel takes the digits pointer in x0
    emitter.instruction("mov x1, x2");                                          // and their length in x1
    emitter.instruction("bl __rt_stdout_write");                                // write the frame index
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_main");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_MAIN_LEN));            // " {main}"
    emitter.instruction("bl __rt_stdout_write");                                // the pseudo-frame every PHP trace ends with
    emitter.instruction("ldr x19, [sp]");                                       // restore the caller's callee-saved register
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // back to the chain renderer

    // -- writes the canonical script path --
    emitter.blank();
    emitter.comment("--- runtime: uncaught_write_file ---");
    emitter.label_global("__rt_uncaught_write_file");
    emitter.instruction("sub sp, sp, #16");                                     // reserve the linkage frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the frame pointer
    abi::emit_symbol_address(emitter, "x0", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "x1", "_script_source_file_len", 0);
    emitter.instruction("bl __rt_stdout_write");                                // write the source filename
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the linkage frame
    emitter.instruction("ret");                                                 // back to the caller

    // -- writes a Throwable's effective filename: x0 = Throwable --
    emitter.blank();
    emitter.comment("--- runtime: uncaught_write_location_file ---");
    emitter.label_global("__rt_uncaught_write_location_file");
    emitter.instruction("sub sp, sp, #16");                                     // reserve the linkage frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the frame pointer
    emitter.instruction("bl __rt_throwable_location_borrowed");                 // borrowed filename in x1/x2, owned by the Throwable
    emitter.instruction("mov x0, x1");                                          // the write funnel takes the filename pointer in x0
    emitter.instruction("mov x1, x2");                                          // and its length in x1
    emitter.instruction("bl __rt_stdout_write");                                // write the filename
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the linkage frame
    emitter.instruction("ret");                                                 // back to the caller

    // -- activation-chain walker: returns the number of frames printed in x0 --
    emitter.blank();
    emitter.comment("--- runtime: uncaught_write_trace ---");
    emitter.label_global("__rt_uncaught_write_trace");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the cursor, the index and linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the frame pointer
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
    emitter.instruction("str x10, [sp]");                                       // cursor starts at the innermost live activation
    emitter.instruction("str xzr, [sp, #8]");                                   // no frame printed yet
    emitter.label("__rt_uncaught_write_trace_loop");
    emitter.instruction("ldr x0, [sp]");                                        // current activation record
    emitter.instruction("cbz x0, __rt_uncaught_write_trace_done");              // the chain ends at a null link
    emitter.instruction("ldr x11, [x0, #16]");                                  // activation self-pointer, or an owned-value payload
    emitter.instruction("cmp x11, x0");                                         // an ownership guard stores its own address here
    emitter.instruction("b.eq __rt_uncaught_write_trace_next");                 // guards are not PHP frames
    emitter.instruction("ldr x10, [x0, #24]");                                  // the frame reader published by the function prologue
    emitter.instruction("cbz x10, __rt_uncaught_write_trace_next");             // a hidden or synthetic frame exposes no reader
    emitter.instruction("mov x1, #0");                                          // options 0: print the arguments
    emitter.instruction("ldr x2, [sp, #8]");                                    // the zero-based output index of this frame
    emitter.instruction("blr x10");                                             // print `#<n> file(line): call(args)` for the suspended frame
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the index: the reader owns every register
    emitter.instruction("add x2, x2, #1");                                      // one more frame printed
    emitter.instruction("str x2, [sp, #8]");                                    // keep the running count
    emitter.label("__rt_uncaught_write_trace_next");
    emitter.instruction("ldr x0, [sp]");                                        // current activation record
    emitter.instruction("ldr x0, [x0]");                                        // link to the caller's activation
    emitter.instruction("str x0, [sp]");                                        // advance the cursor
    emitter.instruction("b __rt_uncaught_write_trace_loop");                    // visit the next outer frame
    emitter.label("__rt_uncaught_write_trace_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the number of printed frames
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // back to the exception renderer

    // -- hosted-extension shutdown before the process exits: calls `_php_exit_hook` when set --
    emitter.blank();
    emitter.comment("--- runtime: run_exit_hook ---");
    emitter.label_global("__rt_run_exit_hook");
    abi::emit_load_symbol_to_reg(emitter, "x9", "_php_exit_hook", 0);
    emitter.instruction("cbz x9, __rt_run_exit_hook_done");                     // no hosted extension: nothing to shut down
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address across the C call
    emitter.instruction("mov x29, sp");                                         // establish the frame pointer
    emitter.instruction("blr x9");                                              // run request and module shutdown of the hosted extensions
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.label("__rt_run_exit_hook_done");
    emitter.instruction("ret");                                                 // back to the exit sequence

    // -- the `__debuginfo() must return an array` fatal: x0-free entry, never returns --
    emitter.blank();
    emitter.comment("--- runtime: debuginfo_fatal ---");
    emitter.label_global("__rt_debuginfo_fatal");
    emitter.instruction("mov x10, #0");                                         // zero clears the capture flags below
    abi::emit_store_reg_to_symbol(emitter, "x10", "_print_r_mode", 0);          // the fatal abandons a print_r(..., true) capture: it must reach stdout
    abi::emit_store_reg_to_symbol(emitter, "x10", "_ob_in_handler", 0);         // and output produced inside an ob_start() handler is no longer discarded
    emitter.instruction("sub sp, sp, #16");                                     // reserve the linkage frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address for the helper calls
    emitter.instruction("mov x29, sp");                                         // establish the frame pointer
    emitter.instruction("bl __rt_ob_flush_all");                                // drain every open output buffer before the fatal is written
    emitter.instruction("bl __rt_debuginfo_fatal_text");                        // the fatal line with its location
    emitter.instruction("bl __rt_run_exit_hook");                               // shut hosted PHP extensions down (RSHUTDOWN, MSHUTDOWN)
    emitter.instruction(&format!("mov x0, #{}", UNCAUGHT_EXIT_STATUS));         // PHP exits 255 on a fatal error
    emitter.syscall(1);

    emitter.blank();
    emitter.comment("--- runtime: debuginfo_fatal_text ---");
    emitter.label_global("__rt_debuginfo_fatal_text");
    emitter.instruction("sub sp, sp, #16");                                     // reserve the linkage frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the frame pointer
    abi::emit_symbol_address(emitter, "x0", "_debuginfo_fatal_msg");
    emitter.instruction(&format!("mov x1, #{}", DEBUGINFO_FATAL_MSG.len()));    // "\nFatal error: __debuginfo() must return an array"
    emitter.instruction("bl __rt_stdout_write");                                // write the fatal text
    abi::emit_load_symbol_to_reg(emitter, "x10", "_debuginfo_call_line", 0);
    emitter.instruction("cbz x10, __rt_debuginfo_fatal_nl");                    // no recorded dump call: omit the location rather than invent one
    abi::emit_load_symbol_to_reg(emitter, "x11", "_script_source_file_len", 0);
    emitter.instruction("cbz x11, __rt_debuginfo_fatal_nl");                    // a module with no source path has no filename to print
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_in");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_IN_LEN));              // " in "
    emitter.instruction("bl __rt_stdout_write");                                // introduce the file
    emitter.instruction("bl __rt_uncaught_write_file");                         // the canonical script path
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_on_line");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_ON_LINE_LEN));         // " on line "
    emitter.instruction("bl __rt_stdout_write");                                // introduce the line number
    abi::emit_load_symbol_to_reg(emitter, "x0", "_debuginfo_call_line", 0);
    emitter.instruction("bl __rt_itoa");                                        // returns pointer in x1 and length in x2
    emitter.instruction("mov x0, x1");                                          // the write funnel takes the digits pointer in x0
    emitter.instruction("mov x1, x2");                                          // and their length in x1
    emitter.instruction("bl __rt_stdout_write");                                // write the decimal dump line
    emitter.label("__rt_debuginfo_fatal_nl");
    abi::emit_symbol_address(emitter, "x0", "_uncaught_exc_nl");
    emitter.instruction(&format!("mov x1, #{}", UNCAUGHT_NEWLINE_LEN));         // terminating newline
    emitter.instruction("bl __rt_stdout_write");                                // terminate the fatal line
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the linkage frame
    emitter.instruction("ret");                                                 // back to the caller

    // -- entry for an exception escaping a run-time `__debugInfo()` (see `objects::debug_info`) --
    emitter.blank();
    emitter.comment("--- runtime: report_debuginfo_exception ---");
    emitter.label_global("__rt_report_debuginfo_exception");
    emitter.instruction("bl __rt_debuginfo_pre_report");                        // var_dump: PHP has already printed the nested value's indent
    abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_value", 0);
    emitter.instruction("cbz x9, __rt_debuginfo_report_value_ready");           // keep the existing null fallback for an absent thrown value
    emitter.instruction("ldr x9, [x9]");                                        // inspect only the class word of the private eight-byte marker
    emitter.instruction("cmn x9, #3");                                          // coroutine cancellation is not a PHP Throwable
    abi::emit_branch_if_equal_wide(emitter, "__rt_debuginfo_fatal");
    emitter.label("__rt_debuginfo_report_value_ready");
    emitter.instruction("mov x10, #1");                                         // select the `Warning: Uncaught ...` + fatal report flavour
    abi::emit_store_reg_to_symbol(emitter, "x10", "_uncaught_report_mode", 0);
    emitter.instruction("b __rt_report_uncaught_exception");                    // share the class/message/location/trace writer

    emitter.blank();
    emitter.comment("--- runtime: debuginfo_pre_report ---");
    emitter.label_global("__rt_debuginfo_pre_report");
    emitter.instruction("sub sp, sp, #16");                                     // reserve the linkage frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    abi::emit_load_symbol_to_reg(emitter, "x10", "_debuginfo_pad", 0);
    emitter.instruction("cbz x10, __rt_debuginfo_pre_report_done");             // print_r (or no dump): nothing was written ahead of the value
    emitter.instruction("bl __rt_vd_pad");                                      // write the indent PHP's var_dump prints before the nested value
    emitter.label("__rt_debuginfo_pre_report_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the linkage frame
    emitter.instruction("ret");                                                 // back to the report entry
}

/// Emits `__rt_report_uncaught_exception` and its helpers for Linux x86_64 (System V AMD64).
fn emit_report_uncaught_exception_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: report_uncaught_exception ---");
    emitter.label_global("__rt_report_uncaught_exception");

    // See the ARM64 path. `and rsp, -16` is the realignment the shared exit path performs for the
    // same call; this helper never returns, so clobbering the alignment afterwards is harmless.
    emitter.instruction("and rsp, -16");                                        // realign the stack for the C-ABI flush; this helper never returns
    emitter.instruction("xor r10d, r10d");                                      // zero clears the capture flags below
    abi::emit_store_reg_to_symbol(emitter, "r10", "_print_r_mode", 0);          // an escaping exception abandons a print_r(..., true) capture: the report must reach stdout
    abi::emit_store_reg_to_symbol(emitter, "r10", "_ob_in_handler", 0);         // and output produced inside an ob_start() handler is no longer discarded
    emitter.instruction("call __rt_ob_flush_all");                              // drain every open output buffer before the report is written

    abi::emit_load_symbol_to_reg(emitter, "r10", "_uncaught_report_mode", 0);
    abi::emit_load_symbol_to_reg(emitter, "r8", "_exc_value", 0);
    emitter.instruction("test r8, r8");                                         // no throwable published: keep the constant message rather than dereferencing null
    emitter.instruction("jz __rt_uncaught_fallback");                           // use the constant fallback when no throwable is published

    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_prefix");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_PREFIX_LEN));          // "\nFatal error: Uncaught "
    emitter.instruction("test r10, r10");                                       // is this the exception-escaping-__debugInfo() flavour?
    emitter.instruction("jz __rt_uncaught_prefix_ready");                       // ordinary uncaught exception: keep the fatal prefix
    abi::emit_symbol_address(emitter, "rdi", "_debuginfo_warning_prefix");
    emitter.instruction(&format!("mov esi, {}", DEBUGINFO_WARNING_PREFIX_LEN)); // "\nWarning: Uncaught " for an exception escaping __debugInfo()
    emitter.label("__rt_uncaught_prefix_ready");
    emitter.instruction("call __rt_stdout_write");                              // PHP writes this report to stdout, not stderr (measured)

    abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
    emitter.instruction("call __rt_uncaught_chain_length");                     // count the distinct links: a re-run constructor can close a cycle
    emitter.instruction("mov rdi, rax");                                        // render exactly that many links
    abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
    emitter.instruction("call __rt_uncaught_render_chain");                     // previous chain first, then the thrown exception, each with its trace

    // -- closing `  thrown in <file> on line <n>` for the OUTERMOST exception --
    abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
    emitter.instruction("call __rt_throwable_location_borrowed");               // the same effective location the getters report
    emitter.instruction("test r8, r8");                                         // is the origin known?
    emitter.instruction("jz __rt_uncaught_newline");                            // no known origin: omit rather than invent
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_thrown_in");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_THROWN_IN_LEN));       // "\n  thrown in "
    emitter.instruction("call __rt_stdout_write");                              // introduce the file of the thrown exception
    abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
    emitter.instruction("call __rt_uncaught_write_location_file");              // its effective filename
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_on_line");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_ON_LINE_LEN));         // " on line "
    emitter.instruction("call __rt_stdout_write");                              // introduce the line number
    abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
    emitter.instruction("call __rt_throwable_location_borrowed");               // the effective line, in rcx
    emitter.instruction("mov rax, rcx");                                        // itoa takes the line in rax
    emitter.instruction("call __rt_itoa");                                      // returns pointer in rax and length in rdx
    emitter.instruction("mov rdi, rax");                                        // the write funnel takes the digits pointer in rdi
    emitter.instruction("mov rsi, rdx");                                        // and their length in rsi
    emitter.instruction("call __rt_stdout_write");                              // write the decimal creation line

    emitter.label("__rt_uncaught_newline");
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_nl");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_NEWLINE_LEN));         // terminating newline
    emitter.instruction("call __rt_stdout_write");                              // terminate the report
    abi::emit_load_symbol_to_reg(emitter, "r10", "_uncaught_report_mode", 0);
    emitter.instruction("test r10, r10");                                       // is this the exception-escaping-__debugInfo() flavour?
    emitter.instruction("jz __rt_uncaught_exit");                               // ordinary uncaught exception: the report is complete
    emitter.instruction("call __rt_debuginfo_fatal_text");                      // PHP follows the warning with the `__debuginfo() must return an array` fatal
    emitter.label("__rt_uncaught_exit");
    emitter.instruction("call __rt_run_exit_hook");                             // shut hosted PHP extensions down (RSHUTDOWN, MSHUTDOWN)
    emitter.instruction(&format!("mov edi, {}", UNCAUGHT_EXIT_STATUS));         // PHP exits 255 for an uncaught exception
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // exit the process after reporting the throwable
    emitter.instruction("ud2");                                                 // keep the terminal syscall from falling through into the next helper

    emitter.label("__rt_uncaught_fallback");
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_msg");
    emitter.instruction(&format!("mov edx, {}", UNCAUGHT_FALLBACK_LEN));        // the pre-existing constant message
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write
    emitter.instruction("syscall");                                             // write the constant fallback diagnostic to stdout
    emitter.instruction("and rsp, -16");                                        // align the stack for the exit hook (this path never returns)
    emitter.instruction("call __rt_run_exit_hook");                             // shut hosted PHP extensions down (RSHUTDOWN, MSHUTDOWN)
    emitter.instruction(&format!("mov edi, {}", UNCAUGHT_EXIT_STATUS));         // same status as the reporting path
    emitter.instruction("mov eax, 60");                                         // syscall 60 = exit
    emitter.instruction("syscall");                                             // exit the process after the fallback diagnostic
    emitter.instruction("ud2");                                                 // terminate control flow if the exit syscall unexpectedly returns

    // -- chain length: rax = Throwable; returns the number of links before one repeats --
    // See the AArch64 path: a re-run constructor can close a `$previous` cycle, and PHP's report
    // stops at the first link it has already printed.
    emitter.blank();
    emitter.comment("--- runtime: uncaught_chain_length ---");
    emitter.label_global("__rt_uncaught_chain_length");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    emitter.instruction("push rbx");                                            // preserve the head register
    emitter.instruction("push r12");                                            // preserve the tail register
    emitter.instruction("push r13");                                            // preserve the count register
    emitter.instruction("push r14");                                            // preserve the scan-cursor register
    emitter.instruction("push r15");                                            // preserve the scan-budget register
    emitter.instruction("sub rsp, 8");                                          // keep the stack 16-byte aligned for calls
    emitter.instruction("mov rbx, rax");                                        // head of the chain: the thrown exception
    emitter.instruction("mov r12, rax");                                        // tail of the accepted prefix
    emitter.instruction("mov r13d, 1");                                         // the thrown exception is always rendered
    emitter.label("__rt_uncaught_chain_length_next");
    emitter.instruction("mov rax, r12");                                        // follow the tail's previous link
    emitter.instruction("call __rt_throwable_previous");                        // borrow it, or zero at the end of the chain
    emitter.instruction("test rax, rax");                                       // did the chain end?
    emitter.instruction("jz __rt_uncaught_chain_length_done");                  // the chain ended without repeating
    emitter.instruction("mov r12, rax");                                        // candidate link to accept
    emitter.instruction("mov r14, rbx");                                        // scan the accepted prefix from the head
    emitter.instruction("mov r15, r13");                                        // which holds this many links
    emitter.label("__rt_uncaught_chain_length_scan");
    emitter.instruction("cmp r14, r12");                                        // was the candidate already accepted?
    emitter.instruction("je __rt_uncaught_chain_length_done");                  // a cycle: stop before printing it again
    emitter.instruction("sub r15, 1");                                          // one fewer accepted link left to compare
    emitter.instruction("jz __rt_uncaught_chain_length_accept");                // the whole prefix differs: accept the candidate
    emitter.instruction("mov rax, r14");                                        // step the scan cursor
    emitter.instruction("call __rt_throwable_previous");                        // to the next accepted link
    emitter.instruction("mov r14, rax");                                        // keep the advanced scan cursor
    emitter.instruction("jmp __rt_uncaught_chain_length_scan");                 // compare the candidate with it
    emitter.label("__rt_uncaught_chain_length_accept");
    emitter.instruction("add r13, 1");                                          // count the accepted link
    emitter.instruction("jmp __rt_uncaught_chain_length_next");                 // and extend the walk from it
    emitter.label("__rt_uncaught_chain_length_done");
    emitter.instruction("mov rax, r13");                                        // return the number of links to render
    emitter.instruction("add rsp, 8");                                          // drop the alignment pad
    emitter.instruction("pop r15");                                             // restore the scan-budget register
    emitter.instruction("pop r14");                                             // restore the scan-cursor register
    emitter.instruction("pop r13");                                             // restore the count register
    emitter.instruction("pop r12");                                             // restore the tail register
    emitter.instruction("pop rbx");                                             // restore the head register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // back to the report entry

    // -- recursive chain renderer: rax = Throwable, rdi = links left; previous exceptions print first --
    emitter.blank();
    emitter.comment("--- runtime: uncaught_render_chain ---");
    emitter.label_global("__rt_uncaught_render_chain");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and align the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    emitter.instruction("push r12");                                            // preserve the callee-saved throwable register
    emitter.instruction("push r13");                                            // preserve the callee-saved budget register; keeps 16-byte alignment
    emitter.instruction("mov r12, rax");                                        // keep the throwable across the helper calls
    emitter.instruction("mov r13, rdi");                                        // keep the number of links still to render
    emitter.instruction("cmp r13, 1");                                          // is this the last distinct link?
    emitter.instruction("jbe __rt_uncaught_render_chain_one");                  // yes: its previous link was already printed, or is absent
    emitter.instruction("call __rt_throwable_previous");                        // borrow its previous exception, or zero
    emitter.instruction("test rax, rax");                                       // is there an older exception in the chain?
    emitter.instruction("jz __rt_uncaught_render_chain_one");                   // no previous link: render only this exception
    emitter.instruction("lea rdi, [r13 - 1]");                                  // the older links get one fewer to render
    emitter.instruction("call __rt_uncaught_render_chain");                     // render the older exceptions first, as PHP does
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_next");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_NEXT_LEN));            // "\n\nNext "
    emitter.instruction("call __rt_stdout_write");                              // separate the older exception from this one
    emitter.label("__rt_uncaught_render_chain_one");
    emitter.instruction("mov rax, r12");                                        // render this exception's own block
    emitter.instruction("call __rt_uncaught_render_one");                       // class, message, location and trace
    emitter.instruction("pop r13");                                             // restore the caller's callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller's callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // back to the previous level or the report entry

    // -- one exception block: rax = Throwable --
    emitter.blank();
    emitter.comment("--- runtime: uncaught_render_one ---");
    emitter.label_global("__rt_uncaught_render_one");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and align the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    emitter.instruction("push r12");                                            // preserve the callee-saved throwable register
    emitter.instruction("push r13");                                            // second push keeps the stack 16-byte aligned for calls
    emitter.instruction("mov r12, rax");                                        // keep the throwable across the writes

    emitter.instruction("mov r9, QWORD PTR [r12]");                             // runtime class id from the object header
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // number of named class ids
    emitter.instruction("cmp r9, r10");                                         // is the class id inside the name table?
    emitter.instruction("jae __rt_uncaught_no_name");                           // an unknown id writes no class name, exactly as var_dump does
    abi::emit_symbol_address(emitter, "r10", "_class_name_entries");
    emitter.instruction("shl r9, 4");                                           // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("add r10, r9");                                         // address of this class's entry
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // class-name length
    emitter.instruction("call __rt_stdout_write");                              // write the throwable class name

    emitter.label("__rt_uncaught_no_name");
    emitter.instruction("mov rdx, QWORD PTR [r12 + 16]");                       // Throwable message length lives at payload offset 16
    emitter.instruction("test rdx, rdx");                                       // an EMPTY message drops the ": " separator, matching reference PHP
    emitter.instruction("jz __rt_uncaught_location");                           // an EMPTY message keeps the location suffix, only the ": " separator goes
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_sep");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_SEPARATOR_LEN));       // ": "
    emitter.instruction("call __rt_stdout_write");                              // write the message separator
    emitter.instruction("mov rdi, QWORD PTR [r12 + 8]");                        // Throwable message pointer lives at payload offset 8
    emitter.instruction("mov rsi, QWORD PTR [r12 + 16]");                       // and its length at offset 16
    emitter.instruction("call __rt_stdout_write");                              // write the throwable message

    emitter.label("__rt_uncaught_location");
    emitter.instruction("mov rax, r12");                                        // resolve this exception's effective location
    emitter.instruction("call __rt_throwable_location_borrowed");               // filename in rax/rdx, line in rcx, known flag in r8
    emitter.instruction("test r8, r8");                                         // is the origin known?
    emitter.instruction("jz __rt_uncaught_trace");                              // no known origin: omit rather than invent
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_in");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_IN_LEN));              // " in "
    emitter.instruction("call __rt_stdout_write");                              // introduce the source location
    emitter.instruction("mov rax, r12");                                        // this exception's filename
    emitter.instruction("call __rt_uncaught_write_location_file");              // write it
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_colon");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_COLON_LEN));           // ":"
    emitter.instruction("call __rt_stdout_write");                              // separate the filename from the line
    emitter.instruction("mov rax, r12");                                        // this exception's line
    emitter.instruction("call __rt_throwable_location_borrowed");               // the effective line, in rcx
    emitter.instruction("mov rax, rcx");                                        // itoa takes the line in rax
    emitter.instruction("call __rt_itoa");                                      // returns pointer in rax and length in rdx
    emitter.instruction("mov rdi, rax");                                        // the write funnel takes the digits pointer in rdi
    emitter.instruction("mov rsi, rdx");                                        // and their length in rsi
    emitter.instruction("call __rt_stdout_write");                              // write the decimal creation line

    emitter.label("__rt_uncaught_trace");
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_stack_trace");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_STACK_TRACE_LEN));     // "\nStack trace:\n"
    emitter.instruction("call __rt_stdout_write");                              // open the trace block
    emitter.instruction("call __rt_uncaught_write_trace");                      // one `#n file(line): call(args)` line per live frame; rax = frame count
    emitter.instruction("mov r12, rax");                                        // the count is the index of the closing {main} line
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_hash");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_HASH_LEN));            // "#"
    emitter.instruction("call __rt_stdout_write");                              // start the closing trace line
    emitter.instruction("mov rax, r12");                                        // itoa takes the frame count in rax
    emitter.instruction("call __rt_itoa");                                      // returns pointer in rax and length in rdx
    emitter.instruction("mov rdi, rax");                                        // the write funnel takes the digits pointer in rdi
    emitter.instruction("mov rsi, rdx");                                        // and their length in rsi
    emitter.instruction("call __rt_stdout_write");                              // write the frame index
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_main");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_MAIN_LEN));            // " {main}"
    emitter.instruction("call __rt_stdout_write");                              // the pseudo-frame every PHP trace ends with
    emitter.instruction("pop r13");                                             // restore the caller's callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller's callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // back to the chain renderer

    // -- writes the canonical script path --
    emitter.blank();
    emitter.comment("--- runtime: uncaught_write_file ---");
    emitter.label_global("__rt_uncaught_write_file");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and align the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    abi::emit_symbol_address(emitter, "rdi", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_script_source_file_len", 0);
    emitter.instruction("call __rt_stdout_write");                              // write the source filename
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // back to the caller

    // -- writes a Throwable's effective filename: rax = Throwable --
    emitter.blank();
    emitter.comment("--- runtime: uncaught_write_location_file ---");
    emitter.label_global("__rt_uncaught_write_location_file");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and align the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    emitter.instruction("call __rt_throwable_location_borrowed");               // borrowed filename in rax/rdx, owned by the Throwable
    emitter.instruction("mov rdi, rax");                                        // the write funnel takes the filename pointer in rdi
    emitter.instruction("mov rsi, rdx");                                        // and its length in rsi
    emitter.instruction("call __rt_stdout_write");                              // write the filename
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // back to the caller

    // -- activation-chain walker: returns the number of frames printed in rax --
    emitter.blank();
    emitter.comment("--- runtime: uncaught_write_trace ---");
    emitter.label_global("__rt_uncaught_write_trace");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and align the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve the cursor and the index, keeping 16-byte alignment
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // cursor starts at the innermost live activation
    emitter.instruction("mov QWORD PTR [rsp + 8], 0");                          // no frame printed yet
    emitter.label("__rt_uncaught_write_trace_loop");
    emitter.instruction("mov rdi, QWORD PTR [rsp]");                            // current activation record
    emitter.instruction("test rdi, rdi");                                       // the chain ends at a null link
    emitter.instruction("jz __rt_uncaught_write_trace_done");                   // nothing left to print
    emitter.instruction("cmp QWORD PTR [rdi + 16], rdi");                       // an ownership guard stores its own address here
    emitter.instruction("je __rt_uncaught_write_trace_next");                   // guards are not PHP frames
    emitter.instruction("mov r10, QWORD PTR [rdi + 24]");                       // the frame reader published by the function prologue
    emitter.instruction("test r10, r10");                                       // a hidden or synthetic frame exposes no reader
    emitter.instruction("jz __rt_uncaught_write_trace_next");                   // skip frames without a reader
    emitter.instruction("xor esi, esi");                                        // options 0: print the arguments
    emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                        // the zero-based output index of this frame
    emitter.instruction("call r10");                                            // print `#<n> file(line): call(args)` for the suspended frame
    emitter.instruction("add QWORD PTR [rsp + 8], 1");                          // one more frame printed
    emitter.label("__rt_uncaught_write_trace_next");
    emitter.instruction("mov rdi, QWORD PTR [rsp]");                            // current activation record
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // link to the caller's activation
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // advance the cursor
    emitter.instruction("jmp __rt_uncaught_write_trace_loop");                  // visit the next outer frame
    emitter.label("__rt_uncaught_write_trace_done");
    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // return the number of printed frames
    emitter.instruction("mov rsp, rbp");                                        // release the cursor slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // back to the exception renderer

    // -- hosted-extension shutdown before the process exits: calls `_php_exit_hook` when set --
    emitter.blank();
    emitter.comment("--- runtime: run_exit_hook ---");
    emitter.label_global("__rt_run_exit_hook");
    abi::emit_load_symbol_to_reg(emitter, "r11", "_php_exit_hook", 0);
    emitter.instruction("test r11, r11");                                       // is a hosted extension registered?
    emitter.instruction("jz __rt_run_exit_hook_done");                          // no hosted extension: nothing to shut down
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    emitter.instruction("and rsp, -16");                                        // the C routine needs an aligned stack
    emitter.instruction("call r11");                                            // run request and module shutdown of the hosted extensions
    emitter.instruction("mov rsp, rbp");                                        // drop the alignment
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.label("__rt_run_exit_hook_done");
    emitter.instruction("ret");                                                 // back to the exit sequence

    // -- the `__debuginfo() must return an array` fatal: never returns --
    emitter.blank();
    emitter.comment("--- runtime: debuginfo_fatal ---");
    emitter.label_global("__rt_debuginfo_fatal");
    emitter.instruction("and rsp, -16");                                        // realign the stack for the C-ABI flush; this helper never returns
    emitter.instruction("xor r10d, r10d");                                      // zero clears the capture flags below
    abi::emit_store_reg_to_symbol(emitter, "r10", "_print_r_mode", 0);          // the fatal abandons a print_r(..., true) capture: it must reach stdout
    abi::emit_store_reg_to_symbol(emitter, "r10", "_ob_in_handler", 0);         // and output produced inside an ob_start() handler is no longer discarded
    emitter.instruction("call __rt_ob_flush_all");                              // drain every open output buffer before the fatal is written
    emitter.instruction("call __rt_debuginfo_fatal_text");                      // the fatal line with its location
    emitter.instruction("call __rt_run_exit_hook");                             // shut hosted PHP extensions down (RSHUTDOWN, MSHUTDOWN)
    emitter.instruction(&format!("mov edi, {}", UNCAUGHT_EXIT_STATUS));         // PHP exits 255 on a fatal error
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process
    emitter.instruction("ud2");                                                 // terminate control flow if the exit syscall unexpectedly returns

    emitter.blank();
    emitter.comment("--- runtime: debuginfo_fatal_text ---");
    emitter.label_global("__rt_debuginfo_fatal_text");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and align the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    abi::emit_symbol_address(emitter, "rdi", "_debuginfo_fatal_msg");
    emitter.instruction(&format!("mov esi, {}", DEBUGINFO_FATAL_MSG.len()));    // "\nFatal error: __debuginfo() must return an array"
    emitter.instruction("call __rt_stdout_write");                              // write the fatal text
    abi::emit_load_symbol_to_reg(emitter, "r10", "_debuginfo_call_line", 0);
    emitter.instruction("test r10, r10");                                       // was the dump call line recorded?
    emitter.instruction("jz __rt_debuginfo_fatal_nl");                          // no recorded dump call: omit the location rather than invent one
    abi::emit_load_symbol_to_reg(emitter, "r11", "_script_source_file_len", 0);
    emitter.instruction("test r11, r11");                                       // a module with no source path has no filename to print
    emitter.instruction("jz __rt_debuginfo_fatal_nl");                          // omit the location when the module has no source filename
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_in");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_IN_LEN));              // " in "
    emitter.instruction("call __rt_stdout_write");                              // introduce the file
    emitter.instruction("call __rt_uncaught_write_file");                       // the canonical script path
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_on_line");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_ON_LINE_LEN));         // " on line "
    emitter.instruction("call __rt_stdout_write");                              // introduce the line number
    abi::emit_load_symbol_to_reg(emitter, "rax", "_debuginfo_call_line", 0);
    emitter.instruction("call __rt_itoa");                                      // returns pointer in rax and length in rdx
    emitter.instruction("mov rdi, rax");                                        // the write funnel takes the digits pointer in rdi
    emitter.instruction("mov rsi, rdx");                                        // and their length in rsi
    emitter.instruction("call __rt_stdout_write");                              // write the decimal dump line
    emitter.label("__rt_debuginfo_fatal_nl");
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_nl");
    emitter.instruction(&format!("mov esi, {}", UNCAUGHT_NEWLINE_LEN));         // terminating newline
    emitter.instruction("call __rt_stdout_write");                              // terminate the fatal line
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // back to the caller

    // -- entry for an exception escaping a run-time `__debugInfo()` (see `objects::debug_info`) --
    emitter.blank();
    emitter.comment("--- runtime: report_debuginfo_exception ---");
    emitter.label_global("__rt_report_debuginfo_exception");
    emitter.instruction("and rsp, -16");                                        // the tail jump from __rt_throw_current may arrive 8-off: realign before the call
    emitter.instruction("call __rt_debuginfo_pre_report");                      // var_dump: PHP has already printed the nested value's indent
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_value", 0);
    emitter.instruction("test r10, r10");                                       // keep the existing null fallback for an absent thrown value
    emitter.instruction("jz __rt_debuginfo_report_value_ready");                // no object header is available to inspect
    emitter.instruction("cmp QWORD PTR [r10], -3");                             // cancellation owns only its private class word
    emitter.instruction("je __rt_debuginfo_fatal");                             // preserve PHP's fatal without rendering the private marker
    emitter.label("__rt_debuginfo_report_value_ready");
    emitter.instruction("mov r10, 1");                                          // select the `Warning: Uncaught ...` + fatal report flavour
    abi::emit_store_reg_to_symbol(emitter, "r10", "_uncaught_report_mode", 0);
    emitter.instruction("jmp __rt_report_uncaught_exception");                  // share the class/message/location/trace writer

    emitter.blank();
    emitter.comment("--- runtime: debuginfo_pre_report ---");
    emitter.label_global("__rt_debuginfo_pre_report");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("and rsp, -16");                                        // realign for the var_dump writers
    abi::emit_load_symbol_to_reg(emitter, "r10", "_debuginfo_pad", 0);
    emitter.instruction("test r10, r10");                                       // print_r (or no dump): nothing was written ahead of the value
    emitter.instruction("jz __rt_debuginfo_pre_report_done");                   // skip the indent
    emitter.instruction("call __rt_vd_pad");                                    // write the indent PHP's var_dump prints before the nested value
    emitter.label("__rt_debuginfo_pre_report_done");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // back to the report entry
}

#[cfg(test)]
mod assembly_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Private cancellation reaches only the debug-info fatal, before any Throwable field read.
    #[test]
    fn debuginfo_cancellation_bypasses_throwable_report_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for pic in [false, true] {
                let mut emitter = Emitter::new(target);
                emitter.pic_data_refs = pic;
                emit_report_uncaught_exception(&mut emitter);
                let assembly = emitter.output();
                let start = assembly.find("__rt_report_debuginfo_exception:\n").unwrap();
                let entry = &assembly[start..];
                let pre_report = entry.find("__rt_debuginfo_pre_report").unwrap();
                let (marker, fatal, report) = if target.arch == Arch::AArch64 {
                    ("cmn x9, #3", "b __rt_debuginfo_fatal", "b __rt_report_uncaught_exception")
                } else {
                    ("cmp QWORD PTR [r10], -3", "je __rt_debuginfo_fatal", "jmp __rt_report_uncaught_exception")
                };
                let marker = entry.find(marker).unwrap();
                let fatal = entry.find(fatal).unwrap();
                let report = entry.find(report).unwrap();
                assert!(pre_report < marker && marker < fatal && fatal < report, "{name}, PIC {pic}: {entry}");
            }
        }
    }

    /// Verifies the AArch64 report reads the Throwable rather than writing a fixed string.
    ///
    /// Each assertion pins a distinct decision, so a sentinel that breaks one cannot hide behind
    /// another: the null guard, the class-id bounds check against the shared name table, the
    /// 16-byte entry stride, the message loads, and the exit status. `mov x0, #255` is asserted
    /// THRICE -- the reporting path, the fallback and the `__debugInfo()` fatal must agree.
    #[test]
    fn test_uncaught_report_arm64_reads_class_and_message() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_report_uncaught_exception(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_report_uncaught_exception:\n"));
        assert!(asm.contains("cbz x9, __rt_uncaught_fallback"));
        assert!(asm.contains("b.hs __rt_uncaught_no_name"));
        assert!(asm.contains("add x11, x11, x10, lsl #4"));
        assert!(asm.contains("ldr x1, [x19, #16]"));
        assert!(asm.contains("ldr x0, [x19, #8]"));
        assert_eq!(asm.matches("mov x0, #255").count(), 3);
        assert_eq!(asm.matches("mov x0, #2\n").count(), 0, "the report must not write to stderr");
    }

    /// Verifies the AArch64 location suffix uses the getters' lookup and formats it through itoa.
    ///
    /// The empty-message branch must land on `__rt_uncaught_location`, NOT on the trace block:
    /// skipping one label too far drops the file and line along with the `": "` separator, and
    /// every test with a non-empty message would still pass.
    #[test]
    fn test_uncaught_report_arm64_appends_the_source_location() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_report_uncaught_exception(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("cbz x1, __rt_uncaught_location"));
        assert!(asm.contains("__rt_uncaught_location:\n"));
        assert!(asm.contains("bl __rt_throwable_location_borrowed"));
        assert!(asm.contains("cbz x3, __rt_uncaught_trace"));
        assert!(asm.contains("cbz x3, __rt_uncaught_newline"));
        assert!(asm.contains("bl __rt_uncaught_write_location_file"));
        assert!(asm.contains("_uncaught_exc_in"));
        assert!(asm.contains("_uncaught_exc_colon"));
        assert!(asm.contains("bl __rt_itoa"));
    }

    /// Verifies the AArch64 report walks the activation chain and chains previous exceptions.
    ///
    /// The ownership-guard test (`[activation + 16] == activation`) is what keeps an owned-value
    /// guard record from being "called" as a frame reader; the recursion into the previous link
    /// before `Next` is what puts the innermost exception first.
    #[test]
    fn test_uncaught_report_arm64_prints_trace_and_chain() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_report_uncaught_exception(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_uncaught_write_trace:\n"));
        assert!(asm.contains("_exc_call_frame_top"));
        assert!(asm.contains("ldr x10, [x0, #24]"));
        assert!(asm.contains("blr x10"));
        assert!(asm.contains("b.eq __rt_uncaught_write_trace_next"));
        assert!(asm.contains("bl __rt_throwable_previous"));
        assert!(asm.contains("_uncaught_exc_next"));
        assert!(asm.contains("_uncaught_exc_stack_trace"));
        assert!(asm.contains("_uncaught_exc_main"));
        assert!(asm.contains("_uncaught_exc_thrown_in"));
        assert!(asm.contains("_uncaught_exc_on_line"));
    }

    /// Verifies the x86_64 report makes the same decisions, in SysV registers.
    ///
    /// The x86 arm exists as its own emitter, so it can drift independently; `shl r9, 4` is the
    /// stride the AArch64 arm expresses as a shifted add.
    #[test]
    fn test_uncaught_report_x86_64_reads_class_and_message() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_report_uncaught_exception(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_report_uncaught_exception:\n"));
        assert!(asm.contains("jz __rt_uncaught_fallback"));
        assert!(asm.contains("jae __rt_uncaught_no_name"));
        assert!(asm.contains("shl r9, 4"));
        assert!(asm.contains("mov rdx, QWORD PTR [r12 + 16]"));
        assert!(asm.contains("mov rdi, QWORD PTR [r12 + 8]"));
        assert_eq!(asm.matches("mov edi, 255").count(), 3);
        assert_eq!(asm.matches("mov edi, 2\n").count(), 0, "the report must not write to stderr");
    }

    /// Verifies the x86_64 location suffix makes the same decisions, in SysV registers.
    ///
    /// `mov rdi, rax` is the instruction with no AArch64 counterpart: `__rt_itoa` returns the
    /// digits in `rax`, which the write funnel expects in `rdi`.
    #[test]
    fn test_uncaught_report_x86_64_appends_the_source_location() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_report_uncaught_exception(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("jz __rt_uncaught_location"));
        assert!(asm.contains("__rt_uncaught_location:\n"));
        assert!(asm.contains("call __rt_throwable_location_borrowed"));
        assert_eq!(asm.matches("jz __rt_uncaught_trace").count(), 1);
        assert!(asm.contains("mov rax, rcx"));
        assert!(asm.contains("call __rt_uncaught_write_location_file"));
        assert!(asm.contains("_uncaught_exc_in"));
        assert!(asm.contains("_uncaught_exc_colon"));
        assert!(asm.contains("call __rt_itoa"));
        assert!(asm.contains("mov rdi, rax"));
    }

    /// Verifies the x86_64 report walks the activation chain and chains previous exceptions.
    #[test]
    fn test_uncaught_report_x86_64_prints_trace_and_chain() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_report_uncaught_exception(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_uncaught_write_trace:\n"));
        assert!(asm.contains("_exc_call_frame_top"));
        assert!(asm.contains("mov r10, QWORD PTR [rdi + 24]"));
        assert!(asm.contains("call r10"));
        assert!(asm.contains("cmp QWORD PTR [rdi + 16], rdi"));
        assert!(asm.contains("call __rt_throwable_previous"));
        assert!(asm.contains("_uncaught_exc_next"));
        assert!(asm.contains("_uncaught_exc_stack_trace"));
        assert!(asm.contains("_uncaught_exc_main"));
        assert!(asm.contains("_uncaught_exc_thrown_in"));
        assert!(asm.contains("_uncaught_exc_on_line"));
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
