//! Purpose:
//! Emits PHP-compatible `escapeshellarg()` and `escapeshellcmd()` runtime helpers.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through the string runtime module.
//!
//! Key details:
//! - POSIX follows PHP's single-quote and backslash escaping rules; Windows follows cmd.exe's
//!   caret quoting and PHP's space-prefix rule for quotes, percent signs, and exclamation marks.
//! - Counted PHP strings are scanned before shell use so embedded NUL bytes throw `ValueError`
//!   instead of being silently truncated by an operating-system command line.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform};
use crate::codegen_support::runtime::data::{
    ESCAPE_SHELL_ARG_INPUT_LENGTH_MSG, ESCAPE_SHELL_ARG_NUL_MSG,
    ESCAPE_SHELL_ARG_OUTPUT_LENGTH_MSG, ESCAPE_SHELL_CMD_INPUT_LENGTH_MSG,
    ESCAPE_SHELL_CMD_NUL_MSG, ESCAPE_SHELL_CMD_OUTPUT_LENGTH_MSG,
};

/// Emits both platform-aware PHP shell escaping helpers.
pub fn emit_shell_escapes(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_shell_escapes_x86_64(emitter);
    } else {
        emit_shell_escapes_aarch64(emitter);
    }
}

/// Emits POSIX AArch64 shell escaping helpers using the internal string ABI.
fn emit_shell_escapes_aarch64(emitter: &mut Emitter) {
    emit_escapeshellarg_aarch64(emitter);
    emit_escapeshellcmd_aarch64(emitter);
    emit_shell_utf8_sequence_len_aarch64(emitter);
}

/// Emits the POSIX AArch64 `escapeshellarg()` helper.
fn emit_escapeshellarg_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: escapeshellarg (POSIX) ---");
    emitter.label_global("__rt_escapeshellarg");
    emitter.instruction("sub sp, sp, #48");                                     // reserve aligned storage for source metadata and the concat-or-heap result flag
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame while a heap fallback invokes the allocator
    emitter.instruction("add x29, sp, #32");                                    // establish a stable frame base for the shell-argument helper
    emitter.instruction("stp x1, x2, [sp]");                                    // preserve the counted PHP source across a possible heap allocation
    emitter.instruction("mov x4, #4");                                          // a POSIX embedded quote expands one input byte to four output bytes
    emitter.instruction("mul x5, x2, x4");                                      // compute the conservative escaped-payload capacity before writing any bytes
    emitter.instruction("umulh x12, x2, x4");                                   // detect a capacity multiplication that does not fit in the machine word
    emitter.instruction("cbnz x12, __rt_escapeshellarg_capacity_overflow");     // do not pass an already-wrapped conservative capacity to the allocator
    emitter.instruction("adds x5, x5, #2");                                     // reserve the required opening and closing POSIX single quotes
    emitter.instruction("b.cs __rt_escapeshellarg_capacity_overflow");          // do not pass an already-wrapped conservative capacity to the allocator
    abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x7, [x6]");                                        // load the concat-buffer offset before quoting the shell argument
    emitter.instruction("adds x12, x7, x5");                                    // calculate the concat-buffer end offset from the conservative capacity
    emitter.instruction("b.cs __rt_escapeshellarg_heap");                       // reject an offset addition that would wrap around the scratch allocation
    emitter.instruction("mov x4, #65536");                                      // materialize the fixed concat scratch capacity in bytes
    emitter.instruction("cmp x12, x4");                                         // does the worst-case escaped argument fit in scratch storage?
    emitter.instruction("b.hi __rt_escapeshellarg_heap");                       // use owned heap storage before any potentially overflowing write
    abi::emit_symbol_address(emitter, "x8", "_concat_buf");
    emitter.instruction("add x8, x8, x7");                                      // select the next concat-buffer byte as the result start
    emitter.instruction("mov x9, x8");                                          // retain the result start for the returned pointer and length
    emitter.instruction("str xzr, [sp, #16]");                                  // record that this result consumes concat scratch on successful return
    emitter.instruction("b __rt_escapeshellarg_start");                         // skip the heap allocation branch when concat storage is sufficient

    emitter.label("__rt_escapeshellarg_heap");
    emitter.instruction("mov x0, x5");                                          // request the conservative escaped payload capacity from the runtime heap
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate owned storage so large shell arguments cannot overflow concat scratch
    emitter.instruction("mov x4, #1");                                          // heap kind 1 marks an owned elephc string payload
    emitter.instruction("str x4, [x0, #-8]");                                   // stamp the allocation before returning it as a string result
    emitter.instruction("mov x8, x0");                                          // initialize the heap-backed destination cursor
    emitter.instruction("mov x9, x0");                                          // retain the heap payload start for the returned string pair
    emitter.instruction("mov x4, #1");                                          // mark this result as heap-backed for finalization
    emitter.instruction("str x4, [sp, #16]");                                   // leave concat offset untouched for the heap-backed result
    emitter.instruction("ldp x1, x2, [sp]");                                    // restore the counted PHP source after the allocator call

    emitter.label("__rt_escapeshellarg_start");
    emitter.instruction("mov x10, x2");                                         // retain the remaining counted source-byte length
    emitter.instruction("mov w11, #39");                                        // materialize POSIX's enclosing single quote
    emitter.instruction("strb w11, [x8], #1");                                  // write the opening shell-argument quote

    emitter.label("__rt_escapeshellarg_loop");
    emitter.instruction("cbz x10, __rt_escapeshellarg_done");                   // finish after consuming the complete counted PHP string
    emitter.instruction("mov x13, x1");                                         // preserve the source cursor while the bounded UTF-8 classifier uses argument registers
    emitter.instruction("mov x0, x13");                                         // pass the current counted source cursor to the UTF-8 classifier
    emitter.instruction("mov x1, x10");                                         // pass the bounded remaining byte count to the UTF-8 classifier
    emitter.instruction("bl __rt_shell_utf8_sequence_len_aarch64");             // classify valid multibyte units before shell punctuation handling
    emitter.instruction("mov x1, x13");                                         // restore the main source cursor after the helper call
    emitter.instruction("cbnz x0, __rt_escapeshellarg_utf8_valid");             // preserve valid sequences atomically and discard only invalid leading bytes
    emitter.instruction("add x1, x1, #1");                                      // skip one invalid leading byte like php_mblen() without interpreting it as shell syntax
    emitter.instruction("sub x10, x10, #1");                                    // consume the skipped invalid byte from the counted source length
    emitter.instruction("b __rt_escapeshellarg_loop");                          // continue after discarding the invalid byte
    emitter.label("__rt_escapeshellarg_utf8_valid");
    emitter.instruction("cmp x0, #1");                                          // does this valid sequence consist of one ASCII byte?
    emitter.instruction("b.eq __rt_escapeshellarg_ascii");                      // route ASCII through NUL and quote semantics
    emitter.instruction("mov x12, x0");                                         // retain the valid multibyte sequence width for the copy loop
    emitter.label("__rt_escapeshellarg_utf8_copy");
    emitter.instruction("ldrb w11, [x1], #1");                                  // load one already-validated UTF-8 byte from the counted source string
    emitter.instruction("strb w11, [x8], #1");                                  // preserve the multibyte byte unchanged in the escaped result
    emitter.instruction("sub x10, x10, #1");                                    // consume the copied byte from the bounded source length
    emitter.instruction("subs x12, x12, #1");                                   // count down the validated byte sequence width
    emitter.instruction("b.ne __rt_escapeshellarg_utf8_copy");                  // copy every byte of the valid multibyte sequence atomically
    emitter.instruction("b __rt_escapeshellarg_loop");                          // continue at the next independent source sequence
    emitter.label("__rt_escapeshellarg_ascii");
    emitter.instruction("ldrb w11, [x1], #1");                                  // load one ASCII argument byte and advance the counted source cursor
    emitter.instruction("sub x10, x10, #1");                                    // account for the byte just consumed from the source string
    emitter.instruction("cbz w11, __rt_escapeshellarg_nul");                    // reject embedded NUL instead of truncating a future shell command
    emitter.instruction("cmp w11, #255");                                       // does PHP's multibyte scanner reject this invalid byte?
    emitter.instruction("b.eq __rt_escapeshellarg_loop");                       // skip invalid 0xff bytes like php_escape_shell_arg()
    emitter.instruction("cmp w11, #39");                                        // is the byte an embedded POSIX single quote?
    emitter.instruction("b.ne __rt_escapeshellarg_copy");                       // ordinary bytes remain inside the surrounding single quotes
    emitter.instruction("mov w12, #39");                                        // materialize the quote closing the current literal segment
    emitter.instruction("strb w12, [x8], #1");                                  // close the segment before the embedded quote
    emitter.instruction("mov w12, #92");                                        // materialize the POSIX backslash escape separator
    emitter.instruction("strb w12, [x8], #1");                                  // write the separator between quoted segments
    emitter.instruction("mov w12, #39");                                        // materialize the quote character as its own literal segment
    emitter.instruction("strb w12, [x8], #1");                                  // write the quoted single quote byte
    emitter.instruction("mov w12, #39");                                        // reopen the enclosing single-quoted argument segment
    emitter.instruction("strb w12, [x8], #1");                                  // continue the argument after the embedded quote
    emitter.instruction("b __rt_escapeshellarg_loop");                          // process the remaining argument bytes

    emitter.label("__rt_escapeshellarg_copy");
    emitter.instruction("strb w11, [x8], #1");                                  // copy an ordinary valid shell-argument byte unchanged
    emitter.instruction("b __rt_escapeshellarg_loop");                          // continue scanning the counted argument

    emitter.label("__rt_escapeshellarg_done");
    emitter.instruction("mov w11, #39");                                        // materialize the closing POSIX single quote
    emitter.instruction("strb w11, [x8], #1");                                  // terminate the quoted shell argument
    emitter.instruction("mov x1, x9");                                          // return the concat-backed escaped argument pointer
    emitter.instruction("sub x2, x8, x9");                                      // return the escaped argument byte length
    emitter.instruction("ldr x4, [sp, #16]");                                   // determine whether the returned payload is scratch-backed or heap-backed
    emitter.instruction("cbnz x4, __rt_escapeshellarg_return");                 // heap-backed results must not advance the shared concat offset
    abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x7, [x6]");                                        // reload the concat offset before publishing this appended slice
    emitter.instruction("add x7, x7, x2");                                      // advance concat storage by the full quoted result length
    emitter.instruction("str x7, [x6]");                                        // publish the new concat-buffer offset for later string helpers
    emitter.label("__rt_escapeshellarg_return");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame after the shell argument has been materialized
    emitter.instruction("add sp, sp, #48");                                     // release the shell-argument helper frame before returning
    emitter.instruction("ret");                                                 // return the escaped PHP string pair

    emitter.label("__rt_escapeshellarg_nul");
    emitter.instruction("ldr x4, [sp, #16]");                                   // determine whether the rejected argument owns a heap-backed partial result
    emitter.instruction("cbz x4, __rt_escapeshellarg_nul_release");             // concat-backed partial output has no owned allocation to release
    emitter.instruction("mov x0, x9");                                          // pass the owned partial-result payload to the heap releaser
    emitter.instruction("bl __rt_heap_free");                                   // release heap-backed output before the catchable ValueError unwinds
    emitter.label("__rt_escapeshellarg_nul_release");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame before tail-entering the exception unwinder
    emitter.instruction("add sp, sp, #48");                                     // release the shell-argument helper frame before throwing
    emit_throw_value_error_aarch64(emitter, "_escapeshellarg_nul_msg", ESCAPE_SHELL_ARG_NUL_MSG.len());
    emitter.label("__rt_escapeshellarg_capacity_overflow");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // release the helper frame before entering the non-returning allocation failure path
    emitter.instruction("add sp, sp, #48");                                     // restore the caller stack before the fatal allocation tail edge
    emitter.instruction("b __rt_heap_exhausted_entry");                         // preserve the heap-fatal atom for this cross-helper non-returning tail
}

/// Emits the POSIX AArch64 `escapeshellcmd()` helper.
fn emit_escapeshellcmd_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: escapeshellcmd (POSIX) ---");
    emitter.label_global("__rt_escapeshellcmd");
    emitter.instruction("sub sp, sp, #48");                                     // reserve an aligned frame for source restoration and the concat-or-heap result flag
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame while a large result uses the heap allocator
    emitter.instruction("add x29, sp, #32");                                    // establish a stable frame base for the shell-command helper
    emitter.instruction("stp x1, x2, [sp]");                                    // preserve the counted command source across a possible heap allocation
    emitter.instruction("mov x4, #2");                                          // each POSIX command input byte can gain at most one backslash
    emitter.instruction("mul x5, x2, x4");                                      // compute conservative escaped-command capacity before writing any bytes
    emitter.instruction("umulh x12, x2, x4");                                   // detect a capacity multiplication that exceeds the native word size
    emitter.instruction("cbnz x12, __rt_escapeshellcmd_capacity_overflow");     // do not pass an already-wrapped conservative capacity to the allocator
    abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x7, [x6]");                                        // load the concat-buffer offset before escaping the command text
    emitter.instruction("adds x12, x7, x5");                                    // calculate concat-buffer end offset from worst-case escaped-command capacity
    emitter.instruction("b.cs __rt_escapeshellcmd_heap");                       // avoid wrapping the concat offset before selecting scratch storage
    emitter.instruction("mov x4, #65536");                                      // materialize the fixed concat scratch capacity in bytes
    emitter.instruction("cmp x12, x4");                                         // does the worst-case result fit entirely in concat scratch?
    emitter.instruction("b.hi __rt_escapeshellcmd_heap");                       // allocate an owned result before any scratch-buffer overflow can occur
    abi::emit_symbol_address(emitter, "x8", "_concat_buf");
    emitter.instruction("add x8, x8, x7");                                      // select the next concat-buffer byte as the command result start
    emitter.instruction("mov x9, x8");                                          // retain the escaped command start for the return pair
    emitter.instruction("str xzr, [sp, #16]");                                  // record that a successful result consumes concat scratch storage
    emitter.instruction("b __rt_escapeshellcmd_start");                         // skip allocation when concat scratch is provably sufficient

    emitter.label("__rt_escapeshellcmd_heap");
    emitter.instruction("mov x0, x5");                                          // request conservative escaped-command capacity from the runtime heap
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate owned storage so command escaping cannot overrun concat scratch
    emitter.instruction("mov x4, #1");                                          // heap kind 1 marks an owned elephc string payload
    emitter.instruction("str x4, [x0, #-8]");                                   // stamp the allocation before returning it as a string result
    emitter.instruction("mov x8, x0");                                          // initialize the heap-backed command destination cursor
    emitter.instruction("mov x9, x0");                                          // retain the heap payload start for the returned result pair
    emitter.instruction("mov x4, #1");                                          // mark this result as heap-backed so concat offset remains unchanged
    emitter.instruction("str x4, [sp, #16]");                                   // publish the storage kind for common finalization
    emitter.instruction("ldp x1, x2, [sp]");                                    // restore the counted command source after allocator clobbers caller-saved registers

    emitter.label("__rt_escapeshellcmd_start");
    emitter.instruction("mov x10, x2");                                         // retain the remaining counted command byte length
    emitter.instruction("mov x11, #0");                                         // clear the pending paired-quote source pointer

    emitter.label("__rt_escapeshellcmd_loop");
    emitter.instruction("cbz x10, __rt_escapeshellcmd_done");                   // finish after processing every counted command byte
    emitter.instruction("mov x13, x1");                                         // preserve the command cursor while the classifier uses argument registers
    emitter.instruction("mov x0, x1");                                          // pass the current bounded command cursor to the UTF-8 classifier
    emitter.instruction("mov x1, x10");                                         // pass the remaining counted command byte length to the UTF-8 classifier
    emitter.instruction("bl __rt_shell_utf8_sequence_len_aarch64");             // classify a complete scalar before shell punctuation handling
    emitter.instruction("mov x1, x13");                                         // restore the main command cursor after the classifier call
    emitter.instruction("cbnz x0, __rt_escapeshellcmd_utf8_valid");             // preserve valid UTF-8 atomically and discard only invalid leading bytes
    emitter.instruction("add x1, x1, #1");                                      // skip one invalid leading byte like php_mblen() without treating it as shell text
    emitter.instruction("sub x10, x10, #1");                                    // consume the skipped invalid byte from the counted command length
    emitter.instruction("b __rt_escapeshellcmd_loop");                          // continue after discarding the invalid byte
    emitter.label("__rt_escapeshellcmd_utf8_valid");
    emitter.instruction("cmp x0, #1");                                          // does this valid sequence consist of one ASCII byte?
    emitter.instruction("b.eq __rt_escapeshellcmd_ascii");                      // route ASCII through NUL, quote, and metacharacter handling
    emitter.instruction("mov x14, x0");                                         // retain the valid multibyte sequence width for exact copying
    emitter.label("__rt_escapeshellcmd_utf8_copy");
    emitter.instruction("ldrb w15, [x1], #1");                                  // load one already-validated UTF-8 byte from the counted command string
    emitter.instruction("strb w15, [x8], #1");                                  // preserve the multibyte byte unchanged in the escaped command result
    emitter.instruction("sub x10, x10, #1");                                    // consume the copied byte from the bounded command length
    emitter.instruction("subs x14, x14, #1");                                   // count down the validated multibyte sequence width
    emitter.instruction("b.ne __rt_escapeshellcmd_utf8_copy");                  // copy every byte of the valid multibyte sequence atomically
    emitter.instruction("b __rt_escapeshellcmd_loop");                          // continue at the next independent command sequence
    emitter.label("__rt_escapeshellcmd_ascii");
    emitter.instruction("ldrb w12, [x1], #1");                                  // load one ASCII command byte and advance the source cursor
    emitter.instruction("sub x10, x10, #1");                                    // account for the consumed source byte
    emitter.instruction("cbz w12, __rt_escapeshellcmd_nul");                    // reject an embedded NUL before a shell could truncate the command
    emitter.instruction("cmp w12, #255");                                       // does PHP's multibyte scanner reject this invalid byte?
    emitter.instruction("b.eq __rt_escapeshellcmd_loop");                       // skip invalid 0xff bytes like php_escape_shell_cmd()
    emitter.instruction("cmp w12, #34");                                        // is this a double quote needing PHP's pairing rule?
    emitter.instruction("b.eq __rt_escapeshellcmd_quote");                      // pair or escape double quotes according to php-src
    emitter.instruction("cmp w12, #39");                                        // is this a single quote needing PHP's pairing rule?
    emitter.instruction("b.eq __rt_escapeshellcmd_quote");                      // pair or escape single quotes according to php-src
    emit_posix_shellcmd_special_checks_aarch64(emitter);
    emitter.instruction("strb w12, [x8], #1");                                  // copy an ordinary command byte unchanged
    emitter.instruction("b __rt_escapeshellcmd_loop");                          // continue escaping the remaining command text

    emitter.label("__rt_escapeshellcmd_quote");
    emitter.instruction("cbz x11, __rt_escapeshellcmd_quote_scan_init");        // only an opening quote begins a bounded look-ahead scan
    emitter.instruction("sub x14, x1, #1");                                     // recover the current quote's exact source address after consuming it
    emitter.instruction("cmp x14, x11");                                        // is this quote the recorded endpoint rather than another quote type?
    emitter.instruction("b.eq __rt_escapeshellcmd_quote_paired");               // only the recorded endpoint closes PHP's raw quote pair
    emitter.instruction("b __rt_escapeshellcmd_quote_unpaired");                // escape an intervening quote without clearing the pending endpoint
    emitter.label("__rt_escapeshellcmd_quote_scan_init");
    emitter.instruction("mov x13, x1");                                         // begin a bounded forward scan after this quote
    emitter.instruction("mov x14, x10");                                        // preserve the number of following source bytes to inspect
    emitter.label("__rt_escapeshellcmd_quote_scan");
    emitter.instruction("cbz x14, __rt_escapeshellcmd_quote_unpaired");         // no matching quote means this quote must gain a backslash
    emitter.instruction("ldrb w15, [x13]");                                     // inspect the next byte without consuming the main source cursor
    emitter.instruction("cmp w15, w12");                                        // does it match this quote's exact delimiter byte?
    emitter.instruction("b.eq __rt_escapeshellcmd_quote_found");                // remember the matching endpoint and preserve both quotes raw
    emitter.instruction("add x13, x13, #1");                                    // advance the look-ahead cursor past a non-matching byte
    emitter.instruction("sub x14, x14, #1");                                    // consume one bounded look-ahead byte
    emitter.instruction("b __rt_escapeshellcmd_quote_scan");                    // keep searching for the matching quote
    emitter.label("__rt_escapeshellcmd_quote_found");
    emitter.instruction("mov x11, x13");                                        // remember the matching quote source address for its later iteration
    emitter.instruction("strb w12, [x8], #1");                                  // copy the first quote raw because PHP found its matching partner
    emitter.instruction("b __rt_escapeshellcmd_loop");                          // resume normal command-byte processing
    emitter.label("__rt_escapeshellcmd_quote_paired");
    emitter.instruction("mov x11, #0");                                         // clear the pair marker after reaching the matching quote
    emitter.instruction("strb w12, [x8], #1");                                  // copy the matching quote raw like PHP's paired-quote case
    emitter.instruction("b __rt_escapeshellcmd_loop");                          // resume normal command-byte processing
    emitter.label("__rt_escapeshellcmd_quote_unpaired");
    emitter.instruction("mov w15, #92");                                        // materialize a POSIX shell backslash before an unpaired quote
    emitter.instruction("strb w15, [x8], #1");                                  // escape the unpaired quote for the POSIX shell
    emitter.instruction("strb w12, [x8], #1");                                  // write the original unpaired quote after its escape
    emitter.instruction("b __rt_escapeshellcmd_loop");                          // continue with the remaining command bytes

    emitter.label("__rt_escapeshellcmd_escape");
    emitter.instruction("mov w15, #92");                                        // materialize the POSIX metacharacter escape prefix
    emitter.instruction("strb w15, [x8], #1");                                  // escape this command metacharacter for the POSIX shell
    emitter.instruction("strb w12, [x8], #1");                                  // write the original escaped command metacharacter
    emitter.instruction("b __rt_escapeshellcmd_loop");                          // process the next command byte

    emitter.label("__rt_escapeshellcmd_done");
    emitter.instruction("mov x1, x9");                                          // return the concat-backed escaped command pointer
    emitter.instruction("sub x2, x8, x9");                                      // return the escaped command byte length
    emitter.instruction("ldr x4, [sp, #16]");                                   // determine whether this result is scratch-backed or heap-backed
    emitter.instruction("cbnz x4, __rt_escapeshellcmd_return");                 // heap-backed command results must leave concat offset unchanged
    abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x7, [x6]");                                        // reload the concat offset before publishing the command result
    emitter.instruction("add x7, x7, x2");                                      // advance concat storage by the escaped command length
    emitter.instruction("str x7, [x6]");                                        // publish the new concat-buffer offset
    emitter.label("__rt_escapeshellcmd_return");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame after materializing the escaped command
    emitter.instruction("add sp, sp, #48");                                     // release the shell-command helper frame before returning
    emitter.instruction("ret");                                                 // return the escaped command string pair

    emitter.label("__rt_escapeshellcmd_nul");
    emitter.instruction("ldr x4, [sp, #16]");                                   // determine whether the rejected command owns a heap-backed partial result
    emitter.instruction("cbz x4, __rt_escapeshellcmd_nul_release");             // concat-backed partial output has no owned allocation to release
    emitter.instruction("mov x0, x9");                                          // pass the owned partial-result payload to the heap releaser
    emitter.instruction("bl __rt_heap_free");                                   // release heap-backed output before the catchable ValueError unwinds
    emitter.label("__rt_escapeshellcmd_nul_release");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame before tail-entering the exception unwinder
    emitter.instruction("add sp, sp, #48");                                     // release the shell-command helper frame before throwing
    emit_throw_value_error_aarch64(emitter, "_escapeshellcmd_nul_msg", ESCAPE_SHELL_CMD_NUL_MSG.len());
    emitter.label("__rt_escapeshellcmd_capacity_overflow");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // release the helper frame before entering the non-returning allocation failure path
    emitter.instruction("add sp, sp, #48");                                     // restore the caller stack before the fatal allocation tail edge
    emitter.instruction("b __rt_heap_exhausted_entry");                         // preserve the heap-fatal atom for this cross-helper non-returning tail
}

/// Emits POSIX `escapeshellcmd()` metacharacter comparisons for one loaded byte.
fn emit_posix_shellcmd_special_checks_aarch64(emitter: &mut Emitter) {
    for byte in [35, 38, 59, 96, 124, 42, 63, 126, 60, 62, 94, 40, 41, 91, 93, 123, 125, 36, 92, 10] {
        emitter.instruction(&format!("cmp w12, #{byte}"));                      // test one PHP POSIX shell metacharacter requiring a backslash prefix
        emitter.instruction("b.eq __rt_escapeshellcmd_escape");                 // branch when the current byte must be escaped for the POSIX shell
    }
}

/// Emits a bounded, scalar-validating UTF-8 sequence classifier for AArch64 shell helpers.
///
/// Input is `x0` = source cursor and `x1` = remaining byte count; it returns a sequence
/// width in `x0`, or zero when PHP must discard exactly one invalid leading byte.
fn emit_shell_utf8_sequence_len_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: bounded UTF-8 sequence classifier ---");
    emitter.label_global("__rt_shell_utf8_sequence_len_aarch64");
    emitter.instruction("cbz x1, __rt_shell_utf8_invalid_aarch64");             // an empty bounded suffix has no UTF-8 leading sequence
    emitter.instruction("ldrb w2, [x0]");                                       // load the candidate leader without traversing a C string
    emitter.instruction("cmp w2, #128");                                        // is the leading byte ordinary ASCII, including NUL?
    emitter.instruction("b.lo __rt_shell_utf8_one_aarch64");                    // ASCII consumes exactly one byte and receives shell-specific handling
    emitter.instruction("cmp w2, #194");                                        // reject continuation bytes and overlong two-byte leaders
    emitter.instruction("b.lo __rt_shell_utf8_invalid_aarch64");                // PHP discards one malformed leading byte at a time
    emitter.instruction("cmp w2, #223");                                        // is this a valid two-byte UTF-8 leader?
    emitter.instruction("b.ls __rt_shell_utf8_two_aarch64");                    // validate one bounded continuation byte
    emitter.instruction("cmp w2, #239");                                        // is this a valid three-byte UTF-8 leader?
    emitter.instruction("b.ls __rt_shell_utf8_three_aarch64");                  // validate two bounded continuation bytes and scalar boundaries
    emitter.instruction("cmp w2, #244");                                        // Unicode accepts at most the F4 four-byte leader
    emitter.instruction("b.hi __rt_shell_utf8_invalid_aarch64");                // reject impossible leaders before reading continuation bytes
    emitter.instruction("cmp x1, #4");                                          // are all four bytes within the counted PHP string?
    emitter.instruction("b.lo __rt_shell_utf8_invalid_aarch64");                // reject truncated four-byte units without an out-of-bounds read
    emitter.instruction("ldrb w3, [x0, #1]");                                   // inspect the first four-byte continuation candidate
    emitter.instruction("and w4, w3, #192");                                    // isolate its UTF-8 continuation bit pattern
    emitter.instruction("cmp w4, #128");                                        // is the first four-byte continuation structurally valid?
    emitter.instruction("b.ne __rt_shell_utf8_invalid_aarch64");                // reject malformed four-byte continuation bytes
    emitter.instruction("cmp w2, #240");                                        // F0 needs a non-overlong first continuation
    emitter.instruction("b.ne __rt_shell_utf8_four_f4_aarch64");                // only F0 has the lower scalar boundary
    emitter.instruction("cmp w3, #144");                                        // must F0's continuation be at least 0x90?
    emitter.instruction("b.lo __rt_shell_utf8_invalid_aarch64");                // reject an overlong four-byte scalar
    emitter.label("__rt_shell_utf8_four_f4_aarch64");
    emitter.instruction("cmp w2, #244");                                        // F4 needs the Unicode upper scalar boundary
    emitter.instruction("b.ne __rt_shell_utf8_four_rest_aarch64");              // non-F4 leaders have no additional upper bound here
    emitter.instruction("cmp w3, #143");                                        // must F4's continuation be at most 0x8f?
    emitter.instruction("b.hi __rt_shell_utf8_invalid_aarch64");                // reject scalars above U+10FFFF
    emitter.label("__rt_shell_utf8_four_rest_aarch64");
    emit_shell_utf8_continuation_check_aarch64(emitter, 2, "__rt_shell_utf8_invalid_aarch64");
    emit_shell_utf8_continuation_check_aarch64(emitter, 3, "__rt_shell_utf8_invalid_aarch64");
    emitter.instruction("mov x0, #4");                                          // report one valid four-byte UTF-8 scalar sequence
    emitter.instruction("ret");                                                 // return while leaving outer shell-loop registers untouched

    emitter.label("__rt_shell_utf8_three_aarch64");
    emitter.instruction("cmp x1, #3");                                          // are both continuation bytes inside the counted source suffix?
    emitter.instruction("b.lo __rt_shell_utf8_invalid_aarch64");                // reject a truncated three-byte UTF-8 sequence
    emitter.instruction("ldrb w3, [x0, #1]");                                   // inspect the first three-byte continuation candidate
    emitter.instruction("and w4, w3, #192");                                    // isolate its UTF-8 continuation bit pattern
    emitter.instruction("cmp w4, #128");                                        // is the first three-byte continuation structurally valid?
    emitter.instruction("b.ne __rt_shell_utf8_invalid_aarch64");                // reject malformed three-byte continuation bytes
    emitter.instruction("cmp w2, #224");                                        // E0 needs a non-overlong first continuation
    emitter.instruction("b.ne __rt_shell_utf8_three_ed_aarch64");               // only E0 has the lower scalar boundary
    emitter.instruction("cmp w3, #160");                                        // must E0's continuation be at least 0xa0?
    emitter.instruction("b.lo __rt_shell_utf8_invalid_aarch64");                // reject an overlong three-byte scalar
    emitter.label("__rt_shell_utf8_three_ed_aarch64");
    emitter.instruction("cmp w2, #237");                                        // ED starts the UTF-16 surrogate range boundary
    emitter.instruction("b.ne __rt_shell_utf8_three_rest_aarch64");             // non-ED leaders need no additional upper bound here
    emitter.instruction("cmp w3, #159");                                        // must ED's continuation be at most 0x9f?
    emitter.instruction("b.hi __rt_shell_utf8_invalid_aarch64");                // reject UTF-16 surrogate code points
    emitter.label("__rt_shell_utf8_three_rest_aarch64");
    emit_shell_utf8_continuation_check_aarch64(emitter, 2, "__rt_shell_utf8_invalid_aarch64");
    emitter.instruction("mov x0, #3");                                          // report one valid three-byte UTF-8 scalar sequence
    emitter.instruction("ret");                                                 // return while leaving outer shell-loop registers untouched

    emitter.label("__rt_shell_utf8_two_aarch64");
    emitter.instruction("cmp x1, #2");                                          // is the continuation byte inside the counted source suffix?
    emitter.instruction("b.lo __rt_shell_utf8_invalid_aarch64");                // reject a truncated two-byte UTF-8 sequence
    emit_shell_utf8_continuation_check_aarch64(emitter, 1, "__rt_shell_utf8_invalid_aarch64");
    emitter.instruction("mov x0, #2");                                          // report one valid two-byte UTF-8 scalar sequence
    emitter.instruction("ret");                                                 // return while leaving outer shell-loop registers untouched

    emitter.label("__rt_shell_utf8_one_aarch64");
    emitter.instruction("mov x0, #1");                                          // report an ASCII sequence for shell-specific classification
    emitter.instruction("ret");                                                 // return while leaving outer shell-loop registers untouched
    emitter.label("__rt_shell_utf8_invalid_aarch64");
    emitter.instruction("mov x0, #0");                                          // request that callers discard exactly one invalid leading byte
    emitter.instruction("ret");                                                 // return while leaving outer shell-loop registers untouched
}

/// Emits one bounded continuation-byte check for the AArch64 shell UTF-8 classifier.
fn emit_shell_utf8_continuation_check_aarch64(emitter: &mut Emitter, offset: usize, invalid_label: &str) {
    emitter.instruction(&format!("ldrb w3, [x0, #{offset}]"));                  // load one bounded UTF-8 continuation candidate from the source slice
    emitter.instruction("and w4, w3, #192");                                    // isolate its required continuation bit pattern
    emitter.instruction("cmp w4, #128");                                        // does this byte have the 10xxxxxx continuation form?
    emitter.instruction(&format!("b.ne {invalid_label}"));                      // reject the whole sequence when any continuation is malformed
}

/// Emits the x86_64 internal-ABI shell escape helpers, selecting Windows semantics when needed.
fn emit_shell_escapes_x86_64(emitter: &mut Emitter) {
    emit_escapeshellarg_x86_64(emitter, emitter.target.platform == Platform::Windows);
    emit_escapeshellcmd_x86_64(emitter, emitter.target.platform == Platform::Windows);
    emit_shell_utf8_sequence_len_x86_64(emitter);
}

/// Emits x86_64 `escapeshellarg()` using either PHP's POSIX or Windows rule.
fn emit_escapeshellarg_x86_64(emitter: &mut Emitter, windows: bool) {
    emitter.blank();
    emitter.comment(if windows { "--- runtime: escapeshellarg (Windows) ---" } else { "--- runtime: escapeshellarg (POSIX) ---" });
    emitter.label_global("__rt_escapeshellarg");
    emitter.instruction("push rbp");                                            // preserve the caller frame while a large shell argument falls back to heap storage
    emitter.instruction("mov rbp, rsp");                                        // establish stable spill slots for source metadata and storage kind
    emitter.instruction("sub rsp, 32");                                         // reserve aligned storage across the heap allocator call
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the counted source pointer across a possible allocation
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the counted source length across a possible allocation
    if windows {
        emitter.instruction("cmp rdx, 8189");                                   // php-src Windows accepts at most cmd_max_len minus two quotes and one NUL byte
        emitter.instruction("ja __rt_escapeshellarg_input_too_long_x86");       // throw the exact PHP ValueError before reserving or writing output storage
    }
    emitter.instruction("mov r11, rdx");                                        // seed conservative escaped capacity from the input byte length
    emitter.instruction(if windows { "imul r11, 2" } else { "imul r11, 4" });   // account for the platform's largest per-byte escape expansion
    emitter.instruction("jo __rt_escapeshellarg_capacity_overflow_x86");        // never pass a wrapped conservative capacity to the allocator
    emitter.instruction(if windows { "add r11, 3" } else { "add r11, 2" });     // reserve surrounding quotes and Windows's possible trailing slash guard
    emitter.instruction("jc __rt_escapeshellarg_capacity_overflow_x86");        // never pass a wrapped conservative capacity to the allocator
    abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the concat-buffer offset before quoting the shell argument
    emitter.instruction("mov r8, r9");                                          // retain the current concat offset while checking conservative capacity
    emitter.instruction("add r8, r11");                                         // calculate the concat scratch end offset before any output write
    emitter.instruction("jc __rt_escapeshellarg_heap_x86");                     // use owned storage rather than wrapping concat scratch addressing
    emitter.instruction("cmp r8, 65536");                                       // does the worst-case escaped argument fit in the fixed concat scratch buffer?
    emitter.instruction("ja __rt_escapeshellarg_heap_x86");                     // allocate an owned string before a scratch-buffer overflow can occur
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("add r10, r9");                                         // select the concat-buffer destination for the escaped argument
    emitter.instruction("mov r9, r10");                                         // retain the escaped-argument start pointer for the result pair
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // mark the result as concat-backed for final offset publication
    emitter.instruction("jmp __rt_escapeshellarg_start_x86");                   // skip heap allocation when scratch storage is sufficient

    emitter.label("__rt_escapeshellarg_heap_x86");
    emitter.instruction("mov rax, r11");                                        // request conservative escaped payload capacity from the runtime heap
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned output storage for an argument that exceeds concat scratch
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap allocation as a managed string payload
    emitter.instruction("mov r10, rax");                                        // initialize the heap-backed output cursor
    emitter.instruction("mov r9, rax");                                         // retain the heap payload start for the returned result pair
    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // prevent heap-backed results from advancing concat offset
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the counted source cursor after the allocator call
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the counted source length after the allocator call

    emitter.label("__rt_escapeshellarg_start_x86");
    emitter.instruction("mov rcx, rdx");                                        // retain the remaining counted source-byte length
    emitter.instruction(if windows { "mov BYTE PTR [r10], 34" } else { "mov BYTE PTR [r10], 39" }); // write PHP's platform-specific opening argument quote
    emitter.instruction("add r10, 1");                                          // advance past the opening quote in concat storage
    if windows {
        emitter.instruction("xor r11d, r11d");                                  // reset the trailing-backslash run counter for Windows closing-quote protection
    }

    emitter.label("__rt_escapeshellarg_loop_x86");
    emitter.instruction("test rcx, rcx");                                       // finish once every byte in the counted PHP argument has been processed
    emitter.instruction("jz __rt_escapeshellarg_done_x86");                     // finalize the quoted argument after the source cursor reaches its end
    emitter.instruction("mov rdi, rax");                                        // pass the current source cursor to the bounded UTF-8 sequence validator
    emitter.instruction("mov rsi, rcx");                                        // pass the remaining counted source length to the UTF-8 validator
    emitter.instruction("call __rt_shell_utf8_sequence_len");                   // classify one valid UTF-8 sequence without C-string scanning
    emitter.instruction("test eax, eax");                                       // did the current byte begin a valid UTF-8 sequence?
    emitter.instruction("jnz __rt_escapeshellarg_utf8_valid_x86");              // copy valid sequences atomically so continuation bytes never become shell syntax
    emitter.instruction("mov rax, rdi");                                        // restore the source cursor before skipping the invalid leading UTF-8 byte
    emitter.instruction("add rax, 1");                                          // skip exactly one invalid leading UTF-8 byte like php_mblen()
    emitter.instruction("sub rcx, 1");                                          // consume the invalid byte from the bounded PHP string
    emitter.instruction("jmp __rt_escapeshellarg_loop_x86");                    // continue after discarding the invalid byte
    emitter.label("__rt_escapeshellarg_utf8_valid_x86");
    emitter.instruction("mov r8d, eax");                                        // retain the validated UTF-8 sequence width for source and output advancement
    emitter.instruction("mov rax, rdi");                                        // restore the source cursor after using rax for the validator result
    emitter.instruction("cmp r8d, 1");                                          // is this an ASCII byte needing shell-specific classification?
    emitter.instruction("je __rt_escapeshellarg_ascii_x86");                    // route ASCII through quote and NUL handling
    emitter.instruction("mov rsi, rax");                                        // begin copying the validated multibyte UTF-8 sequence unchanged
    emitter.label("__rt_escapeshellarg_utf8_copy_x86");
    emitter.instruction("mov al, BYTE PTR [rsi]");                              // load one already-validated UTF-8 byte for verbatim output
    emitter.instruction("add rsi, 1");                                          // advance the bounded source cursor through the UTF-8 sequence
    emitter.instruction("mov BYTE PTR [r10], al");                              // preserve the UTF-8 byte exactly in the escaped PHP string
    emitter.instruction("add r10, 1");                                          // advance concat storage after the copied UTF-8 byte
    emitter.instruction("sub rcx, 1");                                          // consume the copied UTF-8 byte from the source length
    emitter.instruction("sub r8, 1");                                           // consume one byte from the validated sequence width
    emitter.instruction("jnz __rt_escapeshellarg_utf8_copy_x86");               // copy every byte in the valid multibyte sequence
    emitter.instruction("mov rax, rsi");                                        // publish the source cursor after the complete UTF-8 sequence
    emitter.instruction("jmp __rt_escapeshellarg_loop_x86");                    // continue with the next independent source sequence
    emitter.label("__rt_escapeshellarg_ascii_x86");
    emitter.instruction("mov dl, BYTE PTR [rax]");                              // load the next source byte without using C-string termination
    emitter.instruction("add rax, 1");                                          // advance the counted PHP string cursor after consuming the byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of source bytes still to process
    emitter.instruction("test dl, dl");                                         // would this byte truncate an OS-level command string?
    emitter.instruction("jz __rt_escapeshellarg_nul_x86");                      // raise PHP's ValueError for an embedded NUL byte
    if windows {
        emitter.instruction("cmp dl, 92");                                      // is this byte a backslash that may precede the closing quote?
        emitter.instruction("jne __rt_escapeshellarg_not_backslash_x86");       // reset the trailing-backslash run for every non-backslash byte
        emitter.instruction("add r11, 1");                                      // count this trailing backslash for PHP's Windows quote rule
        emitter.instruction("jmp __rt_escapeshellarg_windows_special_x86");     // preserve a backslash after recording its current run length
        emitter.label("__rt_escapeshellarg_not_backslash_x86");
        emitter.instruction("xor r11d, r11d");                                  // a non-backslash breaks the trailing run before the final quote
        emitter.label("__rt_escapeshellarg_windows_special_x86");
        emitter.instruction("cmp dl, 34");                                      // does PHP replace a double quote with a leading space on Windows?
        emitter.instruction("je __rt_escapeshellarg_windows_space_x86");        // apply the Windows quote replacement rule
        emitter.instruction("cmp dl, 37");                                      // does PHP replace a percent sign with a leading space on Windows?
        emitter.instruction("je __rt_escapeshellarg_windows_space_x86");        // apply the Windows environment-expansion protection rule
        emitter.instruction("cmp dl, 33");                                      // does PHP replace an exclamation mark with a leading space on Windows?
        emitter.instruction("je __rt_escapeshellarg_windows_space_x86");        // apply the delayed-expansion protection rule
        emitter.instruction("jmp __rt_escapeshellarg_copy_x86");                // ordinary Windows argument bytes pass through unchanged
        emitter.label("__rt_escapeshellarg_windows_space_x86");
        emitter.instruction("mov BYTE PTR [r10], 32");                          // replace quote, percent, or exclamation with PHP's Windows replacement space
        emitter.instruction("add r10, 1");                                      // advance concat storage after the replacement space
        emitter.instruction("jmp __rt_escapeshellarg_loop_x86");                // discard the replaced byte and continue with the next source character
    } else {
        emitter.instruction("cmp dl, 39");                                      // is this byte an embedded POSIX single quote?
        emitter.instruction("jne __rt_escapeshellarg_copy_x86");                // ordinary bytes remain inside the enclosing POSIX quotes
        emitter.instruction("mov BYTE PTR [r10], 39");                          // close the current POSIX single-quoted argument segment
        emitter.instruction("mov BYTE PTR [r10 + 1], 92");                      // write the backslash separator between quote segments
        emitter.instruction("mov BYTE PTR [r10 + 2], 39");                      // write the embedded quote as its own single-quoted segment
        emitter.instruction("mov BYTE PTR [r10 + 3], 39");                      // reopen the enclosing POSIX quoted segment
        emitter.instruction("add r10, 4");                                      // advance past PHP's four-byte embedded-quote expansion
        emitter.instruction("jmp __rt_escapeshellarg_loop_x86");                // continue after expanding the embedded POSIX quote
    }

    emitter.label("__rt_escapeshellarg_copy_x86");
    emitter.instruction("mov BYTE PTR [r10], dl");                              // copy an ordinary valid argument byte into the concat-backed result
    emitter.instruction("add r10, 1");                                          // advance concat storage after copying the argument byte
    emitter.instruction("jmp __rt_escapeshellarg_loop_x86");                    // process the remaining counted source bytes

    emitter.label("__rt_escapeshellarg_done_x86");
    if windows {
        emitter.instruction("test r11b, 1");                                    // does an odd run of trailing backslashes precede the closing quote?
        emitter.instruction("jz __rt_escapeshellarg_close_x86");                // an even run already leaves the closing quote unescaped
        emitter.instruction("mov BYTE PTR [r10], 92");                          // double the odd trailing backslash run before closing the Windows argument
        emitter.instruction("add r10, 1");                                      // advance concat storage after the protective extra backslash
        emitter.label("__rt_escapeshellarg_close_x86");
        emitter.instruction("mov BYTE PTR [r10], 34");                          // terminate the Windows shell argument with a double quote
    } else {
        emitter.instruction("mov BYTE PTR [r10], 39");                          // terminate the POSIX shell argument with a single quote
    }
    emitter.instruction("add r10, 1");                                          // advance past the closing platform-specific quote
    emitter.instruction("mov rdx, r10");                                        // snapshot the final destination cursor before calculating the result length
    emitter.instruction("sub rdx, r9");                                         // calculate the escaped argument length from cursor minus result start
    if windows {
        emitter.instruction("cmp rdx, 8193");                                   // php-src rejects escaped Windows arguments longer than cmd_max_len plus its trailing NUL
        emitter.instruction("ja __rt_escapeshellarg_output_too_long_x86");      // throw the exact PHP ValueError instead of returning an over-limit command argument
    }
    emitter.instruction("mov rax, r9");                                         // return either concat-backed or heap-backed result start in the string result register
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // determine whether this result is concat-backed or heap-backed
    emitter.instruction("jne __rt_escapeshellarg_return_x86");                  // heap-backed results must leave the shared concat offset unchanged
    abi::emit_load_symbol_to_reg(emitter, "r8", "_concat_off", 0);            // reload the previous concat offset before publishing the appended argument
    emitter.instruction("add r8, rdx");                                         // advance the concat offset by the full escaped argument length
    abi::emit_store_reg_to_symbol(emitter, "r8", "_concat_off", 0);           // publish the new concat offset for later string results
    emitter.label("__rt_escapeshellarg_return_x86");
    emitter.instruction("mov rsp, rbp");                                        // release local shell-argument spill slots before restoring the caller frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the escaped argument
    emitter.instruction("ret");                                                 // return the escaped argument pointer and length pair

    emitter.label("__rt_escapeshellarg_nul_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // determine whether the rejected argument owns a heap-backed partial result
    emitter.instruction("je __rt_escapeshellarg_nul_release_x86");              // concat-backed partial output has no owned allocation to release
    emitter.instruction("mov rax, r9");                                         // pass the owned partial-result payload to the heap releaser
    emitter.instruction("call __rt_heap_free");                                 // release heap-backed output before the catchable ValueError unwinds
    emitter.label("__rt_escapeshellarg_nul_release_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before tail-entering the catchable exception unwinder
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before the catchable ValueError tail edge
    emit_throw_value_error_x86_64(emitter, "_escapeshellarg_nul_msg", ESCAPE_SHELL_ARG_NUL_MSG.len());
    if windows {
        emitter.label("__rt_escapeshellarg_input_too_long_x86");
        emitter.instruction("mov rsp, rbp");                                    // release the helper frame before tail-entering the catchable exception unwinder
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer before the catchable ValueError tail edge
        emit_throw_value_error_x86_64(
            emitter,
            "_escapeshellarg_input_length_msg",
            ESCAPE_SHELL_ARG_INPUT_LENGTH_MSG.len(),
        );
        emitter.label("__rt_escapeshellarg_output_too_long_x86");
        emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                     // determine whether the rejected argument owns a heap-backed completed result
        emitter.instruction("je __rt_escapeshellarg_output_release_x86");       // concat-backed output has no owned allocation to release
        emitter.instruction("mov rax, r9");                                     // pass the owned completed-result payload to the heap releaser
        emitter.instruction("call __rt_heap_free");                             // release heap-backed output before the catchable ValueError unwinds
        emitter.label("__rt_escapeshellarg_output_release_x86");
        emitter.instruction("mov rsp, rbp");                                    // release the helper frame before tail-entering the catchable exception unwinder
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer before the catchable ValueError tail edge
        emit_throw_value_error_x86_64(
            emitter,
            "_escapeshellarg_output_length_msg",
            ESCAPE_SHELL_ARG_OUTPUT_LENGTH_MSG.len(),
        );
    }
    emitter.label("__rt_escapeshellarg_capacity_overflow_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before entering the non-returning allocation failure path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before the fatal allocation tail edge
    emitter.instruction("jmp __rt_heap_exhausted_entry");                       // preserve the heap-fatal atom for this cross-helper non-returning tail
}

/// Emits x86_64 `escapeshellcmd()` using either PHP's POSIX or Windows metacharacter rule.
fn emit_escapeshellcmd_x86_64(emitter: &mut Emitter, windows: bool) {
    emitter.blank();
    emitter.comment(if windows { "--- runtime: escapeshellcmd (Windows) ---" } else { "--- runtime: escapeshellcmd (POSIX) ---" });
    emitter.label_global("__rt_escapeshellcmd");
    emitter.instruction("push rbp");                                            // preserve the caller frame while a large command result falls back to heap storage
    emitter.instruction("mov rbp, rsp");                                        // establish stable spill slots for source metadata and storage kind
    emitter.instruction("sub rsp, 32");                                         // reserve aligned storage across the heap allocator call
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the counted command source pointer across a possible allocation
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the counted command length across a possible allocation
    if windows {
        emitter.instruction("cmp rdx, 8189");                                   // php-src Windows accepts at most cmd_max_len minus two quotes and one NUL byte
        emitter.instruction("ja __rt_escapeshellcmd_input_too_long_x86");       // throw the exact PHP ValueError before reserving or writing output storage
    }
    emitter.instruction("mov r11, rdx");                                        // seed conservative escaped capacity from the command byte length
    emitter.instruction("imul r11, 2");                                         // every shell command byte can gain at most one escape prefix
    emitter.instruction("jo __rt_escapeshellcmd_capacity_overflow_x86");        // never pass a wrapped conservative capacity to the allocator
    abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the concat-buffer offset before escaping the command string
    emitter.instruction("mov r8, r9");                                          // retain the current concat offset while checking conservative capacity
    emitter.instruction("add r8, r11");                                         // calculate the concat scratch end offset before any output write
    emitter.instruction("jc __rt_escapeshellcmd_heap_x86");                     // use owned storage rather than wrapping concat scratch addressing
    emitter.instruction("cmp r8, 65536");                                       // does the worst-case escaped command fit in the fixed concat scratch buffer?
    emitter.instruction("ja __rt_escapeshellcmd_heap_x86");                     // allocate an owned string before a scratch-buffer overflow can occur
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("add r10, r9");                                         // select the concat-buffer destination for the escaped command
    emitter.instruction("mov r9, r10");                                         // retain the escaped-command start pointer for the result pair
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // mark the result as concat-backed for final offset publication
    emitter.instruction("jmp __rt_escapeshellcmd_start_x86");                   // skip heap allocation when scratch storage is sufficient

    emitter.label("__rt_escapeshellcmd_heap_x86");
    emitter.instruction("mov rax, r11");                                        // request conservative escaped payload capacity from the runtime heap
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned output storage for a command that exceeds concat scratch
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap allocation as a managed string payload
    emitter.instruction("mov r10, rax");                                        // initialize the heap-backed output cursor
    emitter.instruction("mov r9, rax");                                         // retain the heap payload start for the returned result pair
    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // prevent heap-backed results from advancing concat offset
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the counted command source cursor after the allocator call
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the counted command length after the allocator call

    emitter.label("__rt_escapeshellcmd_start_x86");
    emitter.instruction("mov rcx, rdx");                                        // retain the remaining counted command byte length
    if !windows {
        emitter.instruction("xor r11d, r11d");                                  // clear the pending matching-quote pointer for PHP's POSIX pairing rule
    }

    emitter.label("__rt_escapeshellcmd_loop_x86");
    emitter.instruction("test rcx, rcx");                                       // finish once every byte in the counted command string has been processed
    emitter.instruction("jz __rt_escapeshellcmd_done_x86");                     // finalize the escaped command after the source cursor reaches its end
    emitter.instruction("mov rdi, rax");                                        // pass the current command cursor to the bounded UTF-8 sequence validator
    emitter.instruction("mov rsi, rcx");                                        // pass the remaining counted command length to the UTF-8 validator
    emitter.instruction("call __rt_shell_utf8_sequence_len");                   // classify one valid UTF-8 sequence without treating invalid bytes as shell text
    emitter.instruction("test eax, eax");                                       // did the command cursor begin a valid UTF-8 sequence?
    emitter.instruction("jnz __rt_escapeshellcmd_utf8_valid_x86");              // preserve valid sequences and discard only invalid leading bytes
    emitter.instruction("mov rax, rdi");                                        // restore the command cursor before skipping the invalid leading UTF-8 byte
    emitter.instruction("add rax, 1");                                          // skip exactly one invalid leading UTF-8 byte like php_mblen()
    emitter.instruction("sub rcx, 1");                                          // consume the invalid byte from the bounded command string
    emitter.instruction("jmp __rt_escapeshellcmd_loop_x86");                    // continue after discarding the invalid byte
    emitter.label("__rt_escapeshellcmd_utf8_valid_x86");
    emitter.instruction("mov r8d, eax");                                        // retain the validated UTF-8 sequence width for source and output advancement
    emitter.instruction("mov rax, rdi");                                        // restore the command source cursor after validation
    emitter.instruction("cmp r8d, 1");                                          // is this an ASCII byte requiring shell metacharacter classification?
    emitter.instruction("je __rt_escapeshellcmd_ascii_x86");                    // route ASCII through quote, NUL, and metacharacter handling
    emitter.instruction("mov rsi, rax");                                        // begin copying the validated multibyte UTF-8 sequence unchanged
    emitter.label("__rt_escapeshellcmd_utf8_copy_x86");
    emitter.instruction("mov al, BYTE PTR [rsi]");                              // load one already-validated UTF-8 byte for verbatim command output
    emitter.instruction("add rsi, 1");                                          // advance the bounded source cursor through the UTF-8 sequence
    emitter.instruction("mov BYTE PTR [r10], al");                              // preserve the UTF-8 byte exactly in the escaped command
    emitter.instruction("add r10, 1");                                          // advance concat storage after the copied UTF-8 byte
    emitter.instruction("sub rcx, 1");                                          // consume the copied UTF-8 byte from the command length
    emitter.instruction("sub r8, 1");                                           // consume one byte from the validated sequence width
    emitter.instruction("jnz __rt_escapeshellcmd_utf8_copy_x86");               // copy every byte in the valid multibyte sequence
    emitter.instruction("mov rax, rsi");                                        // publish the command cursor after the complete UTF-8 sequence
    emitter.instruction("jmp __rt_escapeshellcmd_loop_x86");                    // continue with the next independent command sequence
    emitter.label("__rt_escapeshellcmd_ascii_x86");
    emitter.instruction("mov dl, BYTE PTR [rax]");                              // load the next command byte without relying on NUL termination
    emitter.instruction("add rax, 1");                                          // advance the counted PHP command cursor after consuming the byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of command bytes still to process
    emitter.instruction("test dl, dl");                                         // would this command byte truncate an OS shell command?
    emitter.instruction("jz __rt_escapeshellcmd_nul_x86");                      // raise PHP's ValueError for an embedded NUL byte
    if windows {
        emit_windows_shellcmd_special_checks_x86_64(emitter);
    } else {
        emitter.instruction("cmp dl, 34");                                      // is this a double quote needing PHP's POSIX pairing rule?
        emitter.instruction("je __rt_escapeshellcmd_quote_x86");                // pair or escape the double quote according to php-src
        emitter.instruction("cmp dl, 39");                                      // is this a single quote needing PHP's POSIX pairing rule?
        emitter.instruction("je __rt_escapeshellcmd_quote_x86");                // pair or escape the single quote according to php-src
        emit_posix_shellcmd_special_checks_x86_64(emitter);
    }
    emitter.instruction("mov BYTE PTR [r10], dl");                              // copy an ordinary command byte unchanged into the result
    emitter.instruction("add r10, 1");                                          // advance concat storage after the ordinary command byte
    emitter.instruction("jmp __rt_escapeshellcmd_loop_x86");                    // continue escaping the remaining command text

    if !windows {
        emitter.label("__rt_escapeshellcmd_quote_x86");
        emitter.instruction("test r11, r11");                                   // did the earlier quote record a particular matching endpoint?
        emitter.instruction("jz __rt_escapeshellcmd_quote_scan_x86");           // only a first quote begins a new bounded look-ahead scan
        emitter.instruction("lea r8, [rax - 1]");                               // recover the current quote's exact source address after consuming it
        emitter.instruction("cmp r8, r11");                                     // is this quote the recorded endpoint rather than another quote type?
        emitter.instruction("je __rt_escapeshellcmd_quote_paired_x86");         // only the recorded endpoint closes PHP's raw quote pair
        emitter.instruction("jmp __rt_escapeshellcmd_quote_unpaired_x86");      // escape an intervening quote without clearing the pending endpoint
        emitter.label("__rt_escapeshellcmd_quote_scan_x86");
        emitter.instruction("mov rdi, rax");                                    // begin a bounded forward scan after the current quote
        emitter.instruction("mov r8, rcx");                                     // preserve the main source-byte count across the quote search
        emitter.instruction("mov al, dl");                                      // make the current quote byte the REPNE SCASB search target
        emitter.instruction("repne scasb");                                     // find the first later matching quote in the counted suffix, if any
        emitter.instruction("mov rcx, r8");                                     // restore the main counted source-byte length after the look-ahead search
        emitter.instruction("jne __rt_escapeshellcmd_quote_unpaired_x86");      // escape this quote when no later matching delimiter exists
        emitter.instruction("lea r11, [rdi - 1]");                              // remember the matching quote's source address for its later iteration
        emitter.instruction("mov BYTE PTR [r10], dl");                          // preserve the first quote raw because PHP found a matching partner
        emitter.instruction("add r10, 1");                                      // advance concat storage after the first paired quote
        emitter.instruction("jmp __rt_escapeshellcmd_loop_x86");                // continue processing command bytes after the paired quote opener
        emitter.label("__rt_escapeshellcmd_quote_paired_x86");
        emitter.instruction("xor r11d, r11d");                                  // clear the pending marker after reaching the matching quote
        emitter.instruction("mov BYTE PTR [r10], dl");                          // preserve the matching quote raw like PHP's paired-quote case
        emitter.instruction("add r10, 1");                                      // advance concat storage after the paired quote closer
        emitter.instruction("jmp __rt_escapeshellcmd_loop_x86");                // continue escaping the remaining command bytes
        emitter.label("__rt_escapeshellcmd_quote_unpaired_x86");
        emitter.instruction("mov BYTE PTR [r10], 92");                          // prefix an unpaired POSIX quote with a shell backslash
        emitter.instruction("add r10, 1");                                      // advance concat storage after the quote escape prefix
        emitter.instruction("mov BYTE PTR [r10], dl");                          // copy the original unpaired quote after its escape prefix
        emitter.instruction("add r10, 1");                                      // advance concat storage after the escaped quote
        emitter.instruction("jmp __rt_escapeshellcmd_loop_x86");                // continue escaping the remaining command bytes
    }

    emitter.label("__rt_escapeshellcmd_escape_x86");
    emitter.instruction(if windows { "mov BYTE PTR [r10], 94" } else { "mov BYTE PTR [r10], 92" }); // choose PHP's Windows caret or POSIX backslash metacharacter prefix
    emitter.instruction("add r10, 1");                                          // advance concat storage after the metacharacter escape prefix
    emitter.instruction("mov BYTE PTR [r10], dl");                              // write the original metacharacter after its shell escape prefix
    emitter.instruction("add r10, 1");                                          // advance concat storage after the escaped metacharacter
    emitter.instruction("jmp __rt_escapeshellcmd_loop_x86");                    // continue processing the remaining command bytes

    emitter.label("__rt_escapeshellcmd_done_x86");
    emitter.instruction("mov rdx, r10");                                        // snapshot the final destination cursor before calculating the command length
    emitter.instruction("sub rdx, r9");                                         // calculate the escaped command length from cursor minus result start
    if windows {
        emitter.instruction("cmp rdx, 8193");                                   // php-src rejects escaped Windows commands longer than cmd_max_len plus its trailing NUL
        emitter.instruction("ja __rt_escapeshellcmd_output_too_long_x86");      // throw the exact PHP ValueError instead of returning an over-limit command string
    }
    emitter.instruction("mov rax, r9");                                         // return either concat-backed or heap-backed command start in the string result register
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // determine whether this command result is concat-backed or heap-backed
    emitter.instruction("jne __rt_escapeshellcmd_return_x86");                  // heap-backed command results must leave concat offset unchanged
    abi::emit_load_symbol_to_reg(emitter, "r8", "_concat_off", 0);            // reload the previous concat offset before publishing the escaped command
    emitter.instruction("add r8, rdx");                                         // advance concat storage by the escaped command length
    abi::emit_store_reg_to_symbol(emitter, "r8", "_concat_off", 0);           // publish the new concat offset for later string results
    emitter.label("__rt_escapeshellcmd_return_x86");
    emitter.instruction("mov rsp, rbp");                                        // release local shell-command spill slots before restoring the caller frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the escaped command
    emitter.instruction("ret");                                                 // return the escaped command pointer and length pair

    emitter.label("__rt_escapeshellcmd_nul_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // determine whether the rejected command owns a heap-backed partial result
    emitter.instruction("je __rt_escapeshellcmd_nul_release_x86");              // concat-backed partial output has no owned allocation to release
    emitter.instruction("mov rax, r9");                                         // pass the owned partial-result payload to the heap releaser
    emitter.instruction("call __rt_heap_free");                                 // release heap-backed output before the catchable ValueError unwinds
    emitter.label("__rt_escapeshellcmd_nul_release_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before tail-entering the catchable exception unwinder
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before the catchable ValueError tail edge
    emit_throw_value_error_x86_64(emitter, "_escapeshellcmd_nul_msg", ESCAPE_SHELL_CMD_NUL_MSG.len());
    if windows {
        emitter.label("__rt_escapeshellcmd_input_too_long_x86");
        emitter.instruction("mov rsp, rbp");                                    // release the helper frame before tail-entering the catchable exception unwinder
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer before the catchable ValueError tail edge
        emit_throw_value_error_x86_64(
            emitter,
            "_escapeshellcmd_input_length_msg",
            ESCAPE_SHELL_CMD_INPUT_LENGTH_MSG.len(),
        );
        emitter.label("__rt_escapeshellcmd_output_too_long_x86");
        emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                     // determine whether the rejected command owns a heap-backed completed result
        emitter.instruction("je __rt_escapeshellcmd_output_release_x86");       // concat-backed output has no owned allocation to release
        emitter.instruction("mov rax, r9");                                     // pass the owned completed-result payload to the heap releaser
        emitter.instruction("call __rt_heap_free");                             // release heap-backed output before the catchable ValueError unwinds
        emitter.label("__rt_escapeshellcmd_output_release_x86");
        emitter.instruction("mov rsp, rbp");                                    // release the helper frame before tail-entering the catchable exception unwinder
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer before the catchable ValueError tail edge
        emit_throw_value_error_x86_64(
            emitter,
            "_escapeshellcmd_output_length_msg",
            ESCAPE_SHELL_CMD_OUTPUT_LENGTH_MSG.len(),
        );
    }
    emitter.label("__rt_escapeshellcmd_capacity_overflow_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before entering the non-returning allocation failure path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before the fatal allocation tail edge
    emitter.instruction("jmp __rt_heap_exhausted_entry");                       // preserve the heap-fatal atom for this cross-helper non-returning tail
}

/// Emits POSIX `escapeshellcmd()` metacharacter comparisons for one x86_64 byte.
fn emit_posix_shellcmd_special_checks_x86_64(emitter: &mut Emitter) {
    for byte in [35, 38, 59, 96, 124, 42, 63, 126, 60, 62, 94, 40, 41, 91, 93, 123, 125, 36, 92, 10] {
        emitter.instruction(&format!("cmp dl, {byte}"));                        // test one PHP POSIX shell metacharacter requiring a backslash prefix
        emitter.instruction("je __rt_escapeshellcmd_escape_x86");               // branch when the current byte must be escaped for the POSIX shell
    }
}

/// Emits Windows `escapeshellcmd()` metacharacter comparisons for one x86_64 byte.
fn emit_windows_shellcmd_special_checks_x86_64(emitter: &mut Emitter) {
    for byte in [37, 33, 34, 39, 35, 38, 59, 96, 124, 42, 63, 126, 60, 62, 94, 40, 41, 91, 93, 123, 125, 36, 92, 10] {
        emitter.instruction(&format!("cmp dl, {byte}"));                        // test one PHP Windows cmd.exe metacharacter requiring a caret prefix
        emitter.instruction("je __rt_escapeshellcmd_escape_x86");               // branch when the current byte must be caret-escaped for cmd.exe
    }
}

/// Emits an AArch64 catchable `ValueError` for an embedded shell-string NUL byte.
fn emit_shell_utf8_sequence_len_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: bounded UTF-8 sequence classifier ---");
    emitter.label_global("__rt_shell_utf8_sequence_len");
    emitter.instruction("test rsi, rsi");                                       // reject a sequence request with no remaining counted source bytes
    emitter.instruction("jz __rt_shell_utf8_invalid_x86");                      // an empty suffix has no leading UTF-8 sequence
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load the candidate leading byte without C-string traversal
    emitter.instruction("cmp al, 128");                                         // is this an ASCII byte including NUL?
    emitter.instruction("jb __rt_shell_utf8_one_x86");                          // ASCII always consumes exactly one byte
    emitter.instruction("cmp al, 194");                                         // reject continuation bytes and overlong leader bytes
    emitter.instruction("jb __rt_shell_utf8_invalid_x86");                      // php_mblen skips one invalid leading byte
    emitter.instruction("cmp al, 223");                                         // is this a two-byte leader?
    emitter.instruction("jbe __rt_shell_utf8_two_x86");                         // validate one continuation byte
    emitter.instruction("cmp al, 239");                                         // is this a three-byte leader?
    emitter.instruction("jbe __rt_shell_utf8_three_x86");                       // validate two continuation bytes
    emitter.instruction("cmp al, 244");                                         // is this within the Unicode four-byte leader range?
    emitter.instruction("ja __rt_shell_utf8_invalid_x86");                      // reject impossible UTF-8 leader bytes
    emitter.instruction("cmp rsi, 4");                                          // are all four bytes inside the counted PHP string?
    emitter.instruction("jb __rt_shell_utf8_invalid_x86");                      // reject a truncated four-byte sequence
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 1]");                       // retain the first four-byte continuation for Unicode scalar-bound checks
    emitter.instruction("mov edx, r8d");                                        // copy the first continuation before masking its structural bit pattern
    emitter.instruction("and edx, 192");                                        // isolate the continuation-bit pattern
    emitter.instruction("cmp edx, 128");                                        // is the first continuation structurally valid?
    emitter.instruction("jne __rt_shell_utf8_invalid_x86");                     // reject malformed four-byte sequences
    emitter.instruction("cmp al, 240");                                         // F0 must not encode an overlong four-byte scalar
    emitter.instruction("jne __rt_shell_utf8_four_f4_x86");                     // only F0 has the lower first-continuation bound
    emitter.instruction("cmp r8d, 144");                                        // must F0's first continuation be at least 0x90?
    emitter.instruction("jb __rt_shell_utf8_invalid_x86");                      // reject an overlong four-byte scalar
    emitter.label("__rt_shell_utf8_four_f4_x86");
    emitter.instruction("cmp al, 244");                                         // F4 must stay within Unicode's U+10FFFF upper bound
    emitter.instruction("jne __rt_shell_utf8_four_rest_x86");                   // leaders below F4 have no additional upper continuation bound
    emitter.instruction("cmp r8d, 143");                                        // must F4's first continuation be at most 0x8f?
    emitter.instruction("ja __rt_shell_utf8_invalid_x86");                      // reject scalar values above U+10FFFF
    emitter.label("__rt_shell_utf8_four_rest_x86");
    emitter.instruction("movzx edx, BYTE PTR [rdi + 2]");                       // load the second four-byte continuation candidate
    emitter.instruction("and edx, 192");                                        // isolate the continuation-bit pattern
    emitter.instruction("cmp edx, 128");                                        // is the second continuation structurally valid?
    emitter.instruction("jne __rt_shell_utf8_invalid_x86");                     // reject malformed four-byte sequences
    emitter.instruction("movzx edx, BYTE PTR [rdi + 3]");                       // load the third four-byte continuation candidate
    emitter.instruction("and edx, 192");                                        // isolate the continuation-bit pattern
    emitter.instruction("cmp edx, 128");                                        // is the third continuation structurally valid?
    emitter.instruction("jne __rt_shell_utf8_invalid_x86");                     // reject malformed four-byte sequences
    emitter.instruction("mov eax, 4");                                          // report a valid four-byte UTF-8 sequence
    emitter.instruction("ret");                                                 // return without clobbering the shell-loop cursor registers
    emitter.label("__rt_shell_utf8_three_x86");
    emitter.instruction("cmp rsi, 3");                                          // are both three-byte continuations inside the source bound?
    emitter.instruction("jb __rt_shell_utf8_invalid_x86");                      // reject a truncated three-byte sequence
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 1]");                       // retain the first three-byte continuation for scalar-bound checks
    emitter.instruction("mov edx, r8d");                                        // copy the first continuation before masking its structural bit pattern
    emitter.instruction("and edx, 192");                                        // isolate the continuation-bit pattern
    emitter.instruction("cmp edx, 128");                                        // is the first continuation structurally valid?
    emitter.instruction("jne __rt_shell_utf8_invalid_x86");                     // reject malformed three-byte sequences
    emitter.instruction("cmp al, 224");                                         // E0 must not encode an overlong three-byte scalar
    emitter.instruction("jne __rt_shell_utf8_three_ed_x86");                    // only E0 has the lower first-continuation bound
    emitter.instruction("cmp r8d, 160");                                        // must E0's first continuation be at least 0xa0?
    emitter.instruction("jb __rt_shell_utf8_invalid_x86");                      // reject an overlong three-byte scalar
    emitter.label("__rt_shell_utf8_three_ed_x86");
    emitter.instruction("cmp al, 237");                                         // ED must not encode a UTF-16 surrogate scalar
    emitter.instruction("jne __rt_shell_utf8_three_rest_x86");                  // non-ED leaders have no additional upper continuation bound
    emitter.instruction("cmp r8d, 159");                                        // must ED's first continuation be at most 0x9f?
    emitter.instruction("ja __rt_shell_utf8_invalid_x86");                      // reject UTF-16 surrogate scalar values
    emitter.label("__rt_shell_utf8_three_rest_x86");
    emitter.instruction("movzx edx, BYTE PTR [rdi + 2]");                       // load the second three-byte continuation candidate
    emitter.instruction("and edx, 192");                                        // isolate the continuation-bit pattern
    emitter.instruction("cmp edx, 128");                                        // is the second continuation structurally valid?
    emitter.instruction("jne __rt_shell_utf8_invalid_x86");                     // reject malformed three-byte sequences
    emitter.instruction("mov eax, 3");                                          // report a valid three-byte UTF-8 sequence
    emitter.instruction("ret");                                                 // return without clobbering the shell-loop cursor registers
    emitter.label("__rt_shell_utf8_two_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the two-byte continuation inside the source bound?
    emitter.instruction("jb __rt_shell_utf8_invalid_x86");                      // reject a truncated two-byte sequence
    emitter.instruction("movzx edx, BYTE PTR [rdi + 1]");                       // load the two-byte continuation candidate
    emitter.instruction("and edx, 192");                                        // isolate the continuation-bit pattern
    emitter.instruction("cmp edx, 128");                                        // is the continuation structurally valid?
    emitter.instruction("jne __rt_shell_utf8_invalid_x86");                     // reject malformed two-byte sequences
    emitter.instruction("mov eax, 2");                                          // report a valid two-byte UTF-8 sequence
    emitter.instruction("ret");                                                 // return without clobbering the shell-loop cursor registers
    emitter.label("__rt_shell_utf8_one_x86");
    emitter.instruction("mov eax, 1");                                          // report an ASCII sequence for shell-specific classification
    emitter.instruction("ret");                                                 // return without clobbering the shell-loop cursor registers
    emitter.label("__rt_shell_utf8_invalid_x86");
    emitter.instruction("xor eax, eax");                                        // request that the caller skip one invalid leading byte
    emitter.instruction("ret");                                                 // return without clobbering the shell-loop cursor registers
}

/// Emits an AArch64 catchable `ValueError` for an embedded shell-string NUL byte.
fn emit_throw_value_error_aarch64(emitter: &mut Emitter, message_symbol: &str, message_len: usize) {
    super::super::arrays::value_error::emit_throw_value_error_aarch64(emitter, message_symbol, message_len);
}

/// Emits an x86_64 catchable `ValueError` for an embedded shell-string NUL byte.
fn emit_throw_value_error_x86_64(emitter: &mut Emitter, message_symbol: &str, message_len: usize) {
    super::super::arrays::value_error::emit_throw_value_error_x86_64(emitter, message_symbol, message_len);
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies x86_64 Windows emission preflights both shell helpers and has an owned-heap fallback.
    #[test]
    fn test_windows_x86_shell_escape_emits_capacity_guards() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shell_escapes(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_escapeshellarg_heap_x86:\n"));
        assert!(asm.contains("__rt_escapeshellcmd_heap_x86:\n"));
        assert!(asm.contains("__rt_escapeshellarg_capacity_overflow_x86:\n"));
        assert!(asm.contains("__rt_escapeshellcmd_capacity_overflow_x86:\n"));
        assert_eq!(asm.matches("jmp __rt_heap_exhausted_entry\n").count(), 2);
        assert_eq!(asm.matches("call __rt_heap_exhausted_entry\n").count(), 0);
        assert!(asm.matches("cmp r8, 65536\n").count() >= 2);
        assert!(asm.matches("call __rt_heap_alloc\n").count() >= 2);
        assert_eq!(asm.matches("call __rt_heap_free\n").count(), 4);
        assert_eq!(asm.matches("cmp QWORD PTR [rbp - 24], 0\n").count(), 6);
        assert!(asm.matches("cmp rdx, 8189\n").count() >= 2);
        assert!(asm.matches("cmp rdx, 8193\n").count() >= 2);
        assert!(asm.contains("__rt_escapeshellarg_input_too_long_x86:\n"));
        assert!(asm.contains("__rt_escapeshellcmd_output_too_long_x86:\n"));
        assert!(
            asm.contains(
                "__rt_escapeshellarg_windows_space_x86:\n    mov BYTE PTR [r10], 32\n    add r10, 1\n    jmp __rt_escapeshellarg_loop_x86\n"
            ),
            "Windows escapeshellarg must replace %, !, and quotes rather than prefixing them"
        );

        let trailing_backslash_guard = asm
            .split("__rt_escapeshellarg_done_x86:\n")
            .nth(1)
            .and_then(|section| section.split("__rt_escapeshellarg_close_x86:\n").next())
            .expect("Windows escapeshellarg trailing-backslash guard");
        assert_eq!(
            trailing_backslash_guard.matches("add r10, 1\n").count(),
            1,
            "the duplicated trailing backslash must advance the output cursor exactly once"
        );
    }

    /// Verifies both emitted shell helpers route an embedded NUL through the canonical ValueError path.
    #[test]
    fn test_shell_escape_emits_nul_value_error_paths() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        emit_shell_escapes(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_escapeshellarg_nul:\n"));
        assert!(asm.contains("__rt_escapeshellcmd_nul:\n"));
        assert!(asm.matches("b __rt_throw_current\n").count() >= 2);
    }
}
