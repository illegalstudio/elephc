//! Purpose:
//! Emits string numeric-detection helpers used by PHP loose comparison and int-parameter coercion.
//! Converts pointer/length PHP strings through libc `strtod` while rejecting trailing junk.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - The helper returns both a numeric flag and the parsed double without losing PHP byte-string bounds.
//! - The enum/int coercion probe rejects libc-only spellings such as hex floats, INF, and NAN.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_str_to_number`: converts a PHP string to a double and reports whether it is numeric.
/// Copies the PHP string into the C-string scratch buffer via `__rt_cstr`, then parses it with libc `strtod`.
/// The parsed double is returned in d0/xmm0; the integer result register is 1 when the string is fully
/// numeric (strtod consumed at least one byte and trailing bytes are all ASCII space or tab/newline/form-feed/carriage-return),
/// 0 otherwise.
/// Dispatches to the x86_64-specific implementation when targeting that architecture.
pub fn emit_str_to_number(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_to_number_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_to_number ---");
    emitter.label_global("__rt_str_to_number");

    emitter.instruction("sub sp, sp, #32");                                     // allocate helper slots for the C string start and strtod end pointer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame pointer
    emitter.instruction("bl __rt_cstr");                                        // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("str x0, [sp, #0]");                                    // save the C-string start pointer for the no-consumption check
    emitter.instruction("add x1, sp, #8");                                      // pass the address of the local end-pointer slot to strtod
    emitter.bl_c("strtod");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the end pointer returned by strtod
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the C-string start pointer
    emitter.instruction("cmp x9, x10");                                         // reject strings where strtod consumed no numeric bytes
    emitter.instruction("b.eq __rt_str_to_number_false");                       // no consumed bytes means this is not a numeric string

    emitter.label("__rt_str_to_number_trailing_loop");
    emitter.instruction("ldrb w11, [x9], #1");                                  // load the next trailing byte and advance the scan cursor
    emitter.instruction("cbz w11, __rt_str_to_number_true");                    // end of C string means all trailing bytes were acceptable
    emitter.instruction("cmp w11, #32");                                        // ASCII space is allowed after the numeric payload
    emitter.instruction("b.eq __rt_str_to_number_trailing_loop");               // keep scanning after an allowed space
    emitter.instruction("sub w12, w11, #9");                                    // normalize ASCII tab/newline/form-feed/carriage-return range
    emitter.instruction("cmp w12, #4");                                         // values 9 through 13 are accepted trailing whitespace
    emitter.instruction("b.ls __rt_str_to_number_trailing_loop");               // keep scanning after accepted control whitespace

    emitter.label("__rt_str_to_number_false");
    emitter.instruction("mov x0, #0");                                          // report that the string is not numeric
    emitter.instruction("b __rt_str_to_number_done");                           // restore the helper frame and return

    emitter.label("__rt_str_to_number_true");
    emitter.instruction("mov x0, #1");                                          // report that the string parsed as a complete numeric string

    emitter.label("__rt_str_to_number_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the numeric flag while preserving the parsed double in d0
}

/// Emits `__rt_str_to_number` for the Linux x86_64 target. Identical logic to the ARM64 path but using
/// x86_64 calling conventions: copies the PHP string via `__rt_cstr`, parses with libc `strtod`, checks
/// that strtod consumed at least one byte, then validates that all trailing bytes are ASCII space or
/// tab/newline/form-feed/carriage-return. Returns 1 in rax when fully numeric, 0 otherwise; parsed
/// double is preserved in xmm0.
fn emit_str_to_number_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_to_number ---");
    emitter.label_global("__rt_str_to_number");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // allocate aligned helper slots for start and end pointers
    emitter.instruction("call __rt_cstr");                                      // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the C-string start pointer for the no-consumption check
    emitter.instruction("lea rsi, [rbp - 16]");                                 // pass the address of the local end-pointer slot to strtod
    emitter.instruction("mov rdi, rax");                                        // pass the C-string start pointer as strtod's first argument
    emitter.instruction("call strtod");                                         // parse the C string as a double through libc
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // load the end pointer returned by strtod
    emitter.instruction("cmp r8, QWORD PTR [rbp - 8]");                         // reject strings where strtod consumed no numeric bytes
    emitter.instruction("je __rt_str_to_number_false_linux_x86_64");            // no consumed bytes means this is not a numeric string

    emitter.label("__rt_str_to_number_trailing_loop_linux_x86_64");
    emitter.instruction("movzx r9d, BYTE PTR [r8]");                            // load the next trailing byte without sign extension
    emitter.instruction("add r8, 1");                                           // advance the trailing-byte scan cursor
    emitter.instruction("test r9d, r9d");                                       // check whether the scan reached the C-string terminator
    emitter.instruction("je __rt_str_to_number_true_linux_x86_64");             // end of C string means all trailing bytes were acceptable
    emitter.instruction("cmp r9d, 32");                                         // ASCII space is allowed after the numeric payload
    emitter.instruction("je __rt_str_to_number_trailing_loop_linux_x86_64");    // keep scanning after an allowed space
    emitter.instruction("sub r9d, 9");                                          // normalize ASCII tab/newline/form-feed/carriage-return range
    emitter.instruction("cmp r9d, 4");                                          // values 9 through 13 are accepted trailing whitespace
    emitter.instruction("jbe __rt_str_to_number_trailing_loop_linux_x86_64");   // keep scanning after accepted control whitespace

    emitter.label("__rt_str_to_number_false_linux_x86_64");
    emitter.instruction("xor rax, rax");                                        // report that the string is not numeric
    emitter.instruction("jmp __rt_str_to_number_done_linux_x86_64");            // restore the helper frame and return

    emitter.label("__rt_str_to_number_true_linux_x86_64");
    emitter.instruction("mov rax, 1");                                          // report that the string parsed as a complete numeric string

    emitter.label("__rt_str_to_number_done_linux_x86_64");
    emitter.instruction("add rsp, 32");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the numeric flag while preserving the parsed double in xmm0
}

/// Emits `__rt_str_looks_like_int_for_coercion` for PHP string-to-int parameter coercion.
///
/// The helper accepts the same bounded string inputs as `__rt_str_to_number`, but rejects libc
/// `strtod` extensions that PHP does not accept for coercive `int` parameters: hexadecimal
/// float prefixes (`0x`/`0X`) and case-insensitive `INF`/`INFINITY`/`NAN` spellings after
/// optional leading whitespace and sign.
pub fn emit_str_looks_like_int_for_coercion(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_looks_like_int_for_coercion_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_looks_like_int_for_coercion ---");
    emitter.label_global("__rt_str_looks_like_int_for_coercion");

    emitter.instruction("sub sp, sp, #32");                                     // allocate helper slots for the C string start and strtod end pointer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame pointer
    emitter.instruction("bl __rt_cstr");                                        // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("str x0, [sp, #0]");                                    // save the C-string start pointer for validation and parsing
    emitter.instruction("mov x9, x0");                                          // scan from the C-string start for PHP-forbidden prefixes

    emitter.label("__rt_sliic_ws");
    emitter.instruction("ldrb w10, [x9]");                                      // load the next byte while skipping leading whitespace
    emitter.instruction("cmp w10, #32");                                        // ASCII space is allowed before the numeric payload
    emitter.instruction("b.eq __rt_sliic_ws_next");                             // skip an allowed leading space
    emitter.instruction("sub w11, w10, #9");                                    // normalize ASCII tab/newline/form-feed/carriage-return range
    emitter.instruction("cmp w11, #4");                                         // values 9 through 13 are accepted leading whitespace
    emitter.instruction("b.ls __rt_sliic_ws_next");                             // skip accepted control whitespace
    emitter.instruction("b __rt_sliic_sign");                                   // inspect an optional sign before checking prefixes

    emitter.label("__rt_sliic_ws_next");
    emitter.instruction("add x9, x9, #1");                                      // advance past one leading whitespace byte
    emitter.instruction("b __rt_sliic_ws");                                     // keep scanning leading whitespace

    emitter.label("__rt_sliic_sign");
    emitter.instruction("cmp w10, #43");                                        // plus sign may precede the numeric payload
    emitter.instruction("b.eq __rt_sliic_after_sign");                          // skip a leading plus sign
    emitter.instruction("cmp w10, #45");                                        // minus sign may precede the numeric payload
    emitter.instruction("b.ne __rt_sliic_special");                             // no sign: validate the current payload byte

    emitter.label("__rt_sliic_after_sign");
    emitter.instruction("add x9, x9, #1");                                      // advance past the optional sign byte
    emitter.instruction("ldrb w10, [x9]");                                      // reload the first payload byte after the sign

    emitter.label("__rt_sliic_special");
    emitter.instruction("cmp w10, #48");                                        // hexadecimal floats begin with 0x or 0X after sign/whitespace
    emitter.instruction("b.ne __rt_sliic_check_inf");                           // non-zero prefixes cannot be libc hexadecimal floats
    emitter.instruction("ldrb w11, [x9, #1]");                                  // inspect the byte after the leading zero
    emitter.instruction("cmp w11, #120");                                       // lowercase x marks a libc hexadecimal float
    emitter.instruction("b.eq __rt_sliic_false");                               // reject 0x-prefixed strings for int parameter coercion
    emitter.instruction("cmp w11, #88");                                        // uppercase X marks a libc hexadecimal float
    emitter.instruction("b.eq __rt_sliic_false");                               // reject 0X-prefixed strings for int parameter coercion
    emitter.instruction("b __rt_sliic_parse");                                  // ordinary zero-prefixed decimal strings remain valid candidates

    emitter.label("__rt_sliic_check_inf");
    emitter.instruction("orr w11, w10, #0x20");                                 // fold the first payload byte to lowercase ASCII
    emitter.instruction("cmp w11, #105");                                       // lowercase i starts libc INF/INFINITY spellings
    emitter.instruction("b.eq __rt_sliic_inf");                                 // verify and reject an INF prefix
    emitter.instruction("cmp w11, #110");                                       // lowercase n starts libc NAN spellings
    emitter.instruction("b.eq __rt_sliic_nan");                                 // verify and reject a NAN prefix
    emitter.instruction("b __rt_sliic_parse");                                  // other prefixes are handled by the numeric parser

    emitter.label("__rt_sliic_inf");
    emitter.instruction("ldrb w12, [x9, #1]");                                  // load the second INF byte
    emitter.instruction("orr w12, w12, #0x20");                                 // fold the second INF byte to lowercase ASCII
    emitter.instruction("cmp w12, #110");                                       // require n after i for INF
    emitter.instruction("b.ne __rt_sliic_parse");                               // not INF: let strtod/trailing checks decide
    emitter.instruction("ldrb w12, [x9, #2]");                                  // load the third INF byte
    emitter.instruction("orr w12, w12, #0x20");                                 // fold the third INF byte to lowercase ASCII
    emitter.instruction("cmp w12, #102");                                       // require f after in for INF
    emitter.instruction("b.eq __rt_sliic_false");                               // reject INF and INFINITY spellings
    emitter.instruction("b __rt_sliic_parse");                                  // not INF: let strtod/trailing checks decide

    emitter.label("__rt_sliic_nan");
    emitter.instruction("ldrb w12, [x9, #1]");                                  // load the second NAN byte
    emitter.instruction("orr w12, w12, #0x20");                                 // fold the second NAN byte to lowercase ASCII
    emitter.instruction("cmp w12, #97");                                        // require a after n for NAN
    emitter.instruction("b.ne __rt_sliic_parse");                               // not NAN: let strtod/trailing checks decide
    emitter.instruction("ldrb w12, [x9, #2]");                                  // load the third NAN byte
    emitter.instruction("orr w12, w12, #0x20");                                 // fold the third NAN byte to lowercase ASCII
    emitter.instruction("cmp w12, #110");                                       // require n after na for NAN
    emitter.instruction("b.eq __rt_sliic_false");                               // reject NAN spellings

    emitter.label("__rt_sliic_parse");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the C-string start pointer for strtod
    emitter.instruction("add x1, sp, #8");                                      // pass the address of the local end-pointer slot to strtod
    emitter.bl_c("strtod");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the end pointer returned by strtod
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the C-string start pointer
    emitter.instruction("cmp x9, x10");                                         // reject strings where strtod consumed no numeric bytes
    emitter.instruction("b.eq __rt_sliic_false");                               // no consumed bytes means this is not coercible to int

    emitter.label("__rt_sliic_trailing");
    emitter.instruction("ldrb w11, [x9], #1");                                  // load the next trailing byte and advance the scan cursor
    emitter.instruction("cbz w11, __rt_sliic_true");                            // end of C string means all trailing bytes were acceptable
    emitter.instruction("cmp w11, #32");                                        // ASCII space is allowed after the numeric payload
    emitter.instruction("b.eq __rt_sliic_trailing");                            // keep scanning after an allowed space
    emitter.instruction("sub w12, w11, #9");                                    // normalize ASCII tab/newline/form-feed/carriage-return range
    emitter.instruction("cmp w12, #4");                                         // values 9 through 13 are accepted trailing whitespace
    emitter.instruction("b.ls __rt_sliic_trailing");                            // keep scanning after accepted control whitespace

    emitter.label("__rt_sliic_false");
    emitter.instruction("mov x0, #0");                                          // report that the string cannot coerce to an int parameter
    emitter.instruction("b __rt_sliic_done");                                   // restore the helper frame and return

    emitter.label("__rt_sliic_true");
    emitter.instruction("mov x0, #1");                                          // report that the string can coerce to an int parameter

    emitter.label("__rt_sliic_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the coercion flag
}

/// Emits the Linux x86_64 variant of `__rt_str_looks_like_int_for_coercion`.
///
/// The x86_64 implementation mirrors the AArch64 helper while using SysV argument registers
/// for `strtod`; it returns 1 in `rax` for strings PHP may coerce into an `int` parameter.
fn emit_str_looks_like_int_for_coercion_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_looks_like_int_for_coercion ---");
    emitter.label_global("__rt_str_looks_like_int_for_coercion");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // allocate aligned helper slots for start and end pointers
    emitter.instruction("call __rt_cstr");                                      // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the C-string start pointer for validation and parsing
    emitter.instruction("mov r8, rax");                                         // scan from the C-string start for PHP-forbidden prefixes

    emitter.label("__rt_sliic_ws_x");
    emitter.instruction("movzx r9d, BYTE PTR [r8]");                            // load the next byte while skipping leading whitespace
    emitter.instruction("cmp r9d, 32");                                         // ASCII space is allowed before the numeric payload
    emitter.instruction("je __rt_sliic_ws_next_x");                             // skip an allowed leading space
    emitter.instruction("mov r10d, r9d");                                       // copy the byte before normalizing control whitespace
    emitter.instruction("sub r10d, 9");                                         // normalize ASCII tab/newline/form-feed/carriage-return range
    emitter.instruction("cmp r10d, 4");                                         // values 9 through 13 are accepted leading whitespace
    emitter.instruction("jbe __rt_sliic_ws_next_x");                            // skip accepted control whitespace
    emitter.instruction("jmp __rt_sliic_sign_x");                               // inspect an optional sign before checking prefixes

    emitter.label("__rt_sliic_ws_next_x");
    emitter.instruction("add r8, 1");                                           // advance past one leading whitespace byte
    emitter.instruction("jmp __rt_sliic_ws_x");                                 // keep scanning leading whitespace

    emitter.label("__rt_sliic_sign_x");
    emitter.instruction("cmp r9d, 43");                                         // plus sign may precede the numeric payload
    emitter.instruction("je __rt_sliic_after_sign_x");                          // skip a leading plus sign
    emitter.instruction("cmp r9d, 45");                                         // minus sign may precede the numeric payload
    emitter.instruction("jne __rt_sliic_special_x");                            // no sign: validate the current payload byte

    emitter.label("__rt_sliic_after_sign_x");
    emitter.instruction("add r8, 1");                                           // advance past the optional sign byte
    emitter.instruction("movzx r9d, BYTE PTR [r8]");                            // reload the first payload byte after the sign

    emitter.label("__rt_sliic_special_x");
    emitter.instruction("cmp r9d, 48");                                         // hexadecimal floats begin with 0x or 0X after sign/whitespace
    emitter.instruction("jne __rt_sliic_check_inf_x");                          // non-zero prefixes cannot be libc hexadecimal floats
    emitter.instruction("movzx r10d, BYTE PTR [r8 + 1]");                       // inspect the byte after the leading zero
    emitter.instruction("cmp r10d, 120");                                       // lowercase x marks a libc hexadecimal float
    emitter.instruction("je __rt_sliic_false_x");                               // reject 0x-prefixed strings for int parameter coercion
    emitter.instruction("cmp r10d, 88");                                        // uppercase X marks a libc hexadecimal float
    emitter.instruction("je __rt_sliic_false_x");                               // reject 0X-prefixed strings for int parameter coercion
    emitter.instruction("jmp __rt_sliic_parse_x");                              // ordinary zero-prefixed decimal strings remain valid candidates

    emitter.label("__rt_sliic_check_inf_x");
    emitter.instruction("mov r10d, r9d");                                       // copy the first payload byte before case folding
    emitter.instruction("or r10d, 32");                                         // fold the first payload byte to lowercase ASCII
    emitter.instruction("cmp r10d, 105");                                       // lowercase i starts libc INF/INFINITY spellings
    emitter.instruction("je __rt_sliic_inf_x");                                 // verify and reject an INF prefix
    emitter.instruction("cmp r10d, 110");                                       // lowercase n starts libc NAN spellings
    emitter.instruction("je __rt_sliic_nan_x");                                 // verify and reject a NAN prefix
    emitter.instruction("jmp __rt_sliic_parse_x");                              // other prefixes are handled by the numeric parser

    emitter.label("__rt_sliic_inf_x");
    emitter.instruction("movzx r11d, BYTE PTR [r8 + 1]");                       // load the second INF byte
    emitter.instruction("or r11d, 32");                                         // fold the second INF byte to lowercase ASCII
    emitter.instruction("cmp r11d, 110");                                       // require n after i for INF
    emitter.instruction("jne __rt_sliic_parse_x");                              // not INF: let strtod/trailing checks decide
    emitter.instruction("movzx r11d, BYTE PTR [r8 + 2]");                       // load the third INF byte
    emitter.instruction("or r11d, 32");                                         // fold the third INF byte to lowercase ASCII
    emitter.instruction("cmp r11d, 102");                                       // require f after in for INF
    emitter.instruction("je __rt_sliic_false_x");                               // reject INF and INFINITY spellings
    emitter.instruction("jmp __rt_sliic_parse_x");                              // not INF: let strtod/trailing checks decide

    emitter.label("__rt_sliic_nan_x");
    emitter.instruction("movzx r11d, BYTE PTR [r8 + 1]");                       // load the second NAN byte
    emitter.instruction("or r11d, 32");                                         // fold the second NAN byte to lowercase ASCII
    emitter.instruction("cmp r11d, 97");                                        // require a after n for NAN
    emitter.instruction("jne __rt_sliic_parse_x");                              // not NAN: let strtod/trailing checks decide
    emitter.instruction("movzx r11d, BYTE PTR [r8 + 2]");                       // load the third NAN byte
    emitter.instruction("or r11d, 32");                                         // fold the third NAN byte to lowercase ASCII
    emitter.instruction("cmp r11d, 110");                                       // require n after na for NAN
    emitter.instruction("je __rt_sliic_false_x");                               // reject NAN spellings

    emitter.label("__rt_sliic_parse_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the C-string start pointer for strtod
    emitter.instruction("lea rsi, [rbp - 16]");                                 // pass the address of the local end-pointer slot to strtod
    emitter.instruction("call strtod");                                         // parse the C string as a double through libc
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // load the end pointer returned by strtod
    emitter.instruction("cmp r8, QWORD PTR [rbp - 8]");                         // reject strings where strtod consumed no numeric bytes
    emitter.instruction("je __rt_sliic_false_x");                               // no consumed bytes means this is not coercible to int

    emitter.label("__rt_sliic_trailing_x");
    emitter.instruction("movzx r9d, BYTE PTR [r8]");                            // load the next trailing byte without sign extension
    emitter.instruction("add r8, 1");                                           // advance the trailing-byte scan cursor
    emitter.instruction("test r9d, r9d");                                       // check whether the scan reached the C-string terminator
    emitter.instruction("je __rt_sliic_true_x");                                // end of C string means all trailing bytes were acceptable
    emitter.instruction("cmp r9d, 32");                                         // ASCII space is allowed after the numeric payload
    emitter.instruction("je __rt_sliic_trailing_x");                            // keep scanning after an allowed space
    emitter.instruction("sub r9d, 9");                                          // normalize ASCII tab/newline/form-feed/carriage-return range
    emitter.instruction("cmp r9d, 4");                                          // values 9 through 13 are accepted trailing whitespace
    emitter.instruction("jbe __rt_sliic_trailing_x");                           // keep scanning after accepted control whitespace

    emitter.label("__rt_sliic_false_x");
    emitter.instruction("xor rax, rax");                                        // report that the string cannot coerce to an int parameter
    emitter.instruction("jmp __rt_sliic_done_x");                               // restore the helper frame and return

    emitter.label("__rt_sliic_true_x");
    emitter.instruction("mov rax, 1");                                          // report that the string can coerce to an int parameter

    emitter.label("__rt_sliic_done_x");
    emitter.instruction("add rsp, 32");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the coercion flag
}
