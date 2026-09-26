//! Purpose:
//! Emits the `__rt_htmlspecialchars`, `__rt_htmlsc_loop` runtime helper assembly for htmlspecialchars.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - HTML escaping helpers are emitted scanners that must keep entity tables and quote handling in sync with PHP semantics.
//! - The worst-case `6 * len` expansion (`&quot;` / `&#039;`) is reserved through
//!   `__rt_concat_reserve` before the first store, so long inputs fall back to heap storage
//!   instead of running off the end of the 64 KiB concat scratch buffer.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_htmlspecialchars` runtime helper for ARM64.
///
/// Replaces HTML-sensitive characters with their entity equivalents:
/// `&` → `&amp;`, `"` → `&quot;`, `'` → `&#039;`, `<` → `&lt;`, `>` → `&gt;`.
///
/// # ABI (ARM64)
/// - **Input**: `x1` = source string pointer, `x2` = source byte length, `x3` = `ENT_*` flags
/// - **Output**: `x1` = result pointer, `x2` = result byte length
/// - Reserves the worst-case `6 * len` expansion through `__rt_concat_reserve` (concat scratch
///   while it fits, owned heap storage otherwise) and finishes through `__rt_concat_publish`.
/// - Clobbers every caller-saved register, because the reservation can reach `__rt_heap_alloc`.
/// - A wrapped `6 * len` product reports PHP's allocation-overflow fatal through
///   `__rt_alloc_overflow` instead of reserving a too-small destination.
///
/// # PHP compatibility
/// `"` is escaped only when `ENT_COMPAT` (2) is set and `'` only when `ENT_HTML_QUOTE_SINGLE`
/// (1) is set, so `ENT_NOQUOTES` leaves both literal. The single quote is `&#039;` under
/// `ENT_HTML401` and `&apos;` under `ENT_XML1`, `ENT_XHTML` and `ENT_HTML5` (#645).
pub fn emit_htmlspecialchars(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_htmlspecialchars_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: htmlspecialchars ---");
    emitter.label_global("__rt_htmlspecialchars");

    // -- reserve the worst-case six-bytes-per-input-byte entity expansion before writing anything --
    emitter.instruction("sub sp, sp, #48");                                     // allocate spill space for the borrowed source string and the flags
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across the reservation call
    emitter.instruction("add x29, sp, #32");                                    // establish the htmlspecialchars helper frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save the source pointer and length across the reservation call
    emitter.instruction("str x3, [sp, #16]");                                   // save the ENT_* flags across the reservation call
    emitter.instruction("mov x9, #6");                                          // worst-case entity expansion factor (`&quot;` / `&#039;`)
    emitter.instruction("umulh x10, x2, x9");                                   // capture the high half of the 6 * length product
    emitter.instruction("cbnz x10, __rt_htmlsc_size_overflow");                 // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("mul x0, x2, x9");                                      // compute the worst-case escaped result size
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the escaped result
    emitter.instruction("mov x9, x0");                                          // destination pointer
    emitter.instruction("mov x10, x0");                                         // save result start
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload the borrowed source pointer and length
    emitter.instruction("ldr x14, [sp, #16]");                                  // reload the ENT_* flags for the quote decisions
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_htmlsc_loop");
    emitter.instruction("cbz x11, __rt_htmlsc_done");                           // no bytes left -> done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining

    // -- check & (38) -> &amp; --
    emitter.instruction("cmp w12, #38");                                        // is it '&'?
    emitter.instruction("b.eq __rt_htmlsc_amp");                                // yes -> write &amp;

    // -- check " (34) -> &quot; --
    emitter.instruction("cmp w12, #34");                                        // is it '"'?
    emitter.instruction("b.eq __rt_htmlsc_quot");                               // yes -> write &quot;

    // -- check ' (39) -> &#039; --
    emitter.instruction("cmp w12, #39");                                        // is it '\''?
    emitter.instruction("b.eq __rt_htmlsc_apos");                               // yes -> write &#039;

    // -- check < (60) -> &lt; --
    emitter.instruction("cmp w12, #60");                                        // is it '<'?
    emitter.instruction("b.eq __rt_htmlsc_lt");                                 // yes -> write &lt;

    // -- check > (62) -> &gt; --
    emitter.instruction("cmp w12, #62");                                        // is it '>'?
    emitter.instruction("b.eq __rt_htmlsc_gt");                                 // yes -> write &gt;

    // -- store unmodified byte --
    emitter.label("__rt_htmlsc_plain");
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &amp; (5 bytes: &, a, m, p, ;) --
    emitter.label("__rt_htmlsc_amp");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #97");                                        // 'a'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'a'
    emitter.instruction("mov w13, #109");                                       // 'm'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'm'
    emitter.instruction("mov w13, #112");                                       // 'p'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'p'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &quot; (6 bytes: &, q, u, o, t, ;) --
    emitter.label("__rt_htmlsc_quot");
    emitter.instruction("tbz x14, #1, __rt_htmlsc_plain");                      // ENT_COMPAT (2) unset: a double quote stays literal
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #113");                                       // 'q'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'q'
    emitter.instruction("mov w13, #117");                                       // 'u'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'u'
    emitter.instruction("mov w13, #111");                                       // 'o'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'o'
    emitter.instruction("mov w13, #116");                                       // 't'
    emitter.instruction("strb w13, [x9], #1");                                  // write 't'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &#039; (6 bytes: &, #, 0, 3, 9, ;) --
    emitter.label("__rt_htmlsc_apos");
    emitter.instruction("tbz x14, #0, __rt_htmlsc_plain");                      // ENT_HTML_QUOTE_SINGLE (1) unset: a single quote stays literal
    emitter.instruction("tst x14, #48");                                        // any of ENT_XML1, ENT_XHTML or ENT_HTML5?
    emitter.instruction("b.ne __rt_htmlsc_apos_named");                         // those doctypes spell the single quote &apos;
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #35");                                        // '#'
    emitter.instruction("strb w13, [x9], #1");                                  // write '#'
    emitter.instruction("mov w13, #48");                                        // '0'
    emitter.instruction("strb w13, [x9], #1");                                  // write '0'
    emitter.instruction("mov w13, #51");                                        // '3'
    emitter.instruction("strb w13, [x9], #1");                                  // write '3'
    emitter.instruction("mov w13, #57");                                        // '9'
    emitter.instruction("strb w13, [x9], #1");                                  // write '9'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &apos; (6 bytes: &, a, p, o, s, ;) for the XML1/XHTML/HTML5 doctypes --
    emitter.label("__rt_htmlsc_apos_named");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #97");                                        // 'a'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'a'
    emitter.instruction("mov w13, #112");                                       // 'p'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'p'
    emitter.instruction("mov w13, #111");                                       // 'o'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'o'
    emitter.instruction("mov w13, #115");                                       // 's'
    emitter.instruction("strb w13, [x9], #1");                                  // write 's'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &lt; (4 bytes: &, l, t, ;) --
    emitter.label("__rt_htmlsc_lt");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #108");                                       // 'l'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'l'
    emitter.instruction("mov w13, #116");                                       // 't'
    emitter.instruction("strb w13, [x9], #1");                                  // write 't'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &gt; (4 bytes: &, g, t, ;) --
    emitter.label("__rt_htmlsc_gt");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #103");                                       // 'g'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'g'
    emitter.instruction("mov w13, #116");                                       // 't'
    emitter.instruction("strb w13, [x9], #1");                                  // write 't'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    emitter.label("__rt_htmlsc_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the htmlspecialchars helper frame
    emitter.instruction("ret");                                                 // return

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_htmlsc_size_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits the `__rt_htmlspecialchars` runtime helper for Linux x86_64.
///
/// Replaces HTML-sensitive characters with their entity equivalents:
/// `&` → `&amp;`, `"` → `&quot;`, `'` → `&#039;`, `<` → `&lt;`, `>` → `&gt;`.
///
/// # ABI (x86_64 System V)
/// - **Input**: `rax` = source string pointer, `rdx` = source byte length, `rdi` = `ENT_*` flags
/// - **Output**: `rax` = result pointer, `rdx` = result byte length
/// - Reserves the worst-case `6 * len` expansion through `__rt_concat_reserve` and publishes the
///   written length through `__rt_concat_publish`, so long inputs use owned heap storage instead
///   of running off the end of the 64 KiB concat scratch buffer.
fn emit_htmlspecialchars_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: htmlspecialchars ---");
    emitter.label_global("__rt_htmlspecialchars");

    // -- reserve the worst-case six-bytes-per-input-byte entity expansion before writing anything --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the reservation and publish calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the borrowed source string
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the source pointer and length
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the borrowed source pointer across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the borrowed source length across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the ENT_* flags across the reservation call
    emitter.instruction("imul rax, rdx, 6");                                    // compute the worst-case escaped result size as 6 * source length
    emitter.instruction("jo __rt_htmlsc_size_overflow_linux_x86_64");           // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the escaped result
    emitter.instruction("mov r11, rax");                                        // compute the destination pointer where the escaped HTML string begins
    emitter.instruction("mov r8, r11");                                         // preserve the result start pointer for the returned string value after the loop mutates the destination cursor
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // seed the remaining source length counter from the borrowed input string length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // preserve the borrowed source string cursor in a dedicated register before the loop mutates caller-saved registers
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the ENT_* flags for the quote decisions

    emitter.label("__rt_htmlsc_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been classified and copied into concat storage
    emitter.instruction("jz __rt_htmlsc_done_linux_x86_64");                    // finish once the full borrowed source string has been escaped
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load one source byte before deciding whether it maps to a named HTML entity
    emitter.instruction("add rsi, 1");                                          // advance the borrowed source string cursor after consuming one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source length after consuming one byte
    emitter.instruction("cmp dl, 38");                                          // is the current byte an ampersand that must expand to `&amp;`?
    emitter.instruction("je __rt_htmlsc_amp_linux_x86_64");                     // write the ampersand entity expansion when the current byte is '&'
    emitter.instruction("cmp dl, 34");                                          // is the current byte a double quote that must expand to `&quot;`?
    emitter.instruction("je __rt_htmlsc_quot_linux_x86_64");                    // write the double-quote entity expansion when the current byte is '\"'
    emitter.instruction("cmp dl, 39");                                          // is the current byte a single quote that must expand to `&#039;`?
    emitter.instruction("je __rt_htmlsc_apos_linux_x86_64");                    // write the single-quote entity expansion when the current byte is '\\''
    emitter.instruction("cmp dl, 60");                                          // is the current byte a less-than sign that must expand to `&lt;`?
    emitter.instruction("je __rt_htmlsc_lt_linux_x86_64");                      // write the less-than entity expansion when the current byte is '<'
    emitter.instruction("cmp dl, 62");                                          // is the current byte a greater-than sign that must expand to `&gt;`?
    emitter.instruction("je __rt_htmlsc_gt_linux_x86_64");                      // write the greater-than entity expansion when the current byte is '>'
    emitter.label("__rt_htmlsc_plain_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store source bytes that do not need HTML escaping directly into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after copying an unescaped source byte
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes until the input string is exhausted

    emitter.label("__rt_htmlsc_amp_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' as the first byte of the `&amp;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the first byte of `&amp;`
    emitter.instruction("mov BYTE PTR [r11], 97");                              // write 'a' as the second byte of the `&amp;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the second byte of `&amp;`
    emitter.instruction("mov BYTE PTR [r11], 109");                             // write 'm' as the third byte of the `&amp;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the third byte of `&amp;`
    emitter.instruction("mov BYTE PTR [r11], 112");                             // write 'p' as the fourth byte of the `&amp;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the fourth byte of `&amp;`
    emitter.instruction("mov BYTE PTR [r11], 59");                              // write ';' as the terminating byte of the `&amp;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the final byte of `&amp;`
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes after expanding one ampersand

    emitter.label("__rt_htmlsc_quot_linux_x86_64");
    emitter.instruction("test r9, 2");                                          // is ENT_COMPAT (2) set?
    emitter.instruction("jz __rt_htmlsc_plain_linux_x86_64");                   // no: a double quote stays literal
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' as the first byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the first byte of `&quot;`
    emitter.instruction("mov BYTE PTR [r11], 113");                             // write 'q' as the second byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the second byte of `&quot;`
    emitter.instruction("mov BYTE PTR [r11], 117");                             // write 'u' as the third byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the third byte of `&quot;`
    emitter.instruction("mov BYTE PTR [r11], 111");                             // write 'o' as the fourth byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the fourth byte of `&quot;`
    emitter.instruction("mov BYTE PTR [r11], 116");                             // write 't' as the fifth byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the fifth byte of `&quot;`
    emitter.instruction("mov BYTE PTR [r11], 59");                              // write ';' as the terminating byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the final byte of `&quot;`
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes after expanding one double quote

    emitter.label("__rt_htmlsc_apos_linux_x86_64");
    emitter.instruction("test r9, 1");                                          // is ENT_HTML_QUOTE_SINGLE (1) set?
    emitter.instruction("jz __rt_htmlsc_plain_linux_x86_64");                   // no: a single quote stays literal
    emitter.instruction("test r9, 48");                                         // any of ENT_XML1, ENT_XHTML or ENT_HTML5?
    emitter.instruction("jnz __rt_htmlsc_apos_named_linux_x86_64");             // those doctypes spell the single quote &apos;
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' as the first byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the first byte of `&#039;`
    emitter.instruction("mov BYTE PTR [r11], 35");                              // write '#' as the second byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the second byte of `&#039;`
    emitter.instruction("mov BYTE PTR [r11], 48");                              // write '0' as the third byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the third byte of `&#039;`
    emitter.instruction("mov BYTE PTR [r11], 51");                              // write '3' as the fourth byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the fourth byte of `&#039;`
    emitter.instruction("mov BYTE PTR [r11], 57");                              // write '9' as the fifth byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the fifth byte of `&#039;`
    emitter.instruction("mov BYTE PTR [r11], 59");                              // write ';' as the terminating byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the final byte of `&#039;`
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes after expanding one single quote

    emitter.label("__rt_htmlsc_apos_named_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' of the `&apos;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the destination cursor
    emitter.instruction("mov BYTE PTR [r11], 97");                              // write 'a' of the `&apos;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the destination cursor
    emitter.instruction("mov BYTE PTR [r11], 112");                             // write 'p' of the `&apos;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the destination cursor
    emitter.instruction("mov BYTE PTR [r11], 111");                             // write 'o' of the `&apos;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the destination cursor
    emitter.instruction("mov BYTE PTR [r11], 115");                             // write 's' of the `&apos;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the destination cursor
    emitter.instruction("mov BYTE PTR [r11], 59");                              // write ';' of the `&apos;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the destination cursor
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes

    emitter.label("__rt_htmlsc_lt_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' as the first byte of the `&lt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the first byte of `&lt;`
    emitter.instruction("mov BYTE PTR [r11], 108");                             // write 'l' as the second byte of the `&lt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the second byte of `&lt;`
    emitter.instruction("mov BYTE PTR [r11], 116");                             // write 't' as the third byte of the `&lt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the third byte of `&lt;`
    emitter.instruction("mov BYTE PTR [r11], 59");                              // write ';' as the terminating byte of the `&lt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the final byte of `&lt;`
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes after expanding one less-than sign

    emitter.label("__rt_htmlsc_gt_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' as the first byte of the `&gt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the first byte of `&gt;`
    emitter.instruction("mov BYTE PTR [r11], 103");                             // write 'g' as the second byte of the `&gt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the second byte of `&gt;`
    emitter.instruction("mov BYTE PTR [r11], 116");                             // write 't' as the third byte of the `&gt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the third byte of `&gt;`
    emitter.instruction("mov BYTE PTR [r11], 59");                              // write ';' as the terminating byte of the `&gt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the final byte of `&gt;`
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes after expanding one greater-than sign

    emitter.label("__rt_htmlsc_done_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // return the reserved result start pointer after escaping the full input string
    emitter.instruction("mov rdx, r11");                                        // copy the final destination cursor before computing the escaped string length
    emitter.instruction("sub rdx, r8");                                         // compute the escaped string length as dest_end - dest_start for the returned x86_64 string value
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("add rsp, 32");                                         // release the htmlspecialchars spill slots before returning the escaped string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the escaped string
    emitter.instruction("ret");                                                 // return the escaped string in the standard x86_64 string result registers

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_htmlsc_size_overflow_linux_x86_64");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller
}
