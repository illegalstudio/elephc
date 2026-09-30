//! Purpose:
//! Emits `__rt_php_wrapper_open`, which resolves a `php://` URL whose bytes are only known at
//! run time.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The dynamic `fopen()` lowering, when the path carries the `php://` prefix.
//!
//! Key details:
//! - `fopen()` picks a wrapper from a LITERAL path at compile time. A path built at run time had
//!   no such dispatch and fell through to the file opener, so `fopen($path, 'r')` with
//!   `php://memory` looked for a FILE of that name and answered `false` — as did every other
//!   scheme. This helper makes the same choices from the bytes.
//! - The sub-scheme names live in `_php_wrapper_schemes`, a fixed-stride table built in the data
//!   section, so the names and their actions are declared once instead of being spelled out as a
//!   comparison chain in each of the two assembly emitters here.
//! - Descriptor-backed schemes hand out a `dup()`, never the process's own descriptor, matching
//!   what the literal path already does: `fclose()` on the result must not close the real stdout.

use crate::codegen_support::abi;
use crate::codegen_support::runtime::io::PHP_FD_FORM;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Byte stride of one `_php_wrapper_schemes` record.
const RECORD_STRIDE: i64 = 16;

/// Offset of the name-length byte inside a record.
const RECORD_LEN_OFFSET: i64 = 8;

/// Offset of the action byte inside a record.
const RECORD_ACTION_OFFSET: i64 = 9;

/// Offset of the prefix-match flag inside a record.
const RECORD_PREFIX_OFFSET: i64 = 10;

/// Emits `__rt_php_wrapper_open`.
pub fn emit_php_wrapper_open(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// `__rt_php_wrapper_open(x0 = path, x1 = length) -> x0 = descriptor, or -1`.
///
/// The caller has already established the `php://` prefix; the six bytes are skipped here so the
/// helper reads the same way as the table it walks.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: open a run-time php:// URL ---");
    emitter.label_global("__rt_php_wrapper_open");
    // Frame: [0]=sub-scheme pointer [8]=sub-scheme length [16]=matched record
    //        [24]=whole URI pointer [32]=whole URI length [48]/[56]=linkage.
    //
    // The URI is kept whole because php NAMES it in the failed-open line, and by the time a
    // refusal is reached the cursor has been advanced past `php://`.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the dispatch frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #24]");                                   // the URI, whole, for the refusal line
    emitter.instruction("str x1, [sp, #32]");
    emitter.instruction("cmp x1, #6");                                          // is there anything after "php://"?
    emitter.instruction("b.le __rt_pwo_unknown");                               // a bare "php://" names no stream
    emitter.instruction("add x0, x0, #6");                                      // skip the scheme prefix
    emitter.instruction("sub x1, x1, #6");                                      // and shorten the length to match
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the sub-scheme pointer
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the sub-scheme length

    abi::emit_symbol_address(emitter, "x9", "_php_wrapper_schemes");            // walk cursor
    emitter.label("__rt_pwo_next_record");
    emitter.instruction(&format!("ldrb w10, [x9, #{RECORD_LEN_OFFSET}]"));      // the record's name length
    emitter.instruction("cbz w10, __rt_pwo_unknown");                           // a zero length terminates the table
    emitter.instruction(&format!("ldrb w11, [x9, #{RECORD_PREFIX_OFFSET}]"));   // does this record match as a prefix?
    emitter.instruction("ldr x12, [sp, #8]");                                   // the sub-scheme length
    emitter.instruction("cmp x12, x10");                                        // enough bytes for the name?
    emitter.instruction("b.lt __rt_pwo_advance");                               // too short to match
    emitter.instruction("cbnz w11, __rt_pwo_compare");                          // a prefix record accepts a longer input
    emitter.instruction("cmp x12, x10");                                        // an exact record needs the same length
    emitter.instruction("b.ne __rt_pwo_advance");                               // a longer input is a different scheme

    emitter.label("__rt_pwo_compare");
    emitter.instruction("ldr x13, [sp, #0]");                                   // the sub-scheme bytes
    emitter.instruction("mov x14, #0");                                         // comparison index
    emitter.label("__rt_pwo_compare_byte");
    emitter.instruction("cmp x14, x10");                                        // compared the whole name?
    emitter.instruction("b.hs __rt_pwo_matched");                               // every byte agreed
    emitter.instruction("ldrb w15, [x13, x14]");                                // one input byte
    emitter.instruction("ldrb w16, [x9, x14]");                                 // the corresponding name byte
    emitter.instruction("cmp w15, w16");                                        // do they agree?
    emitter.instruction("b.ne __rt_pwo_advance");                               // this record does not match
    emitter.instruction("add x14, x14, #1");                                    // advance the comparison index
    emitter.instruction("b __rt_pwo_compare_byte");                             // keep comparing

    emitter.label("__rt_pwo_advance");
    emitter.instruction(&format!("add x9, x9, #{RECORD_STRIDE}"));              // step to the next record
    emitter.instruction("b __rt_pwo_next_record");                              // continue the walk

    emitter.label("__rt_pwo_matched");
    emitter.instruction(&format!("ldrb w11, [x9, #{RECORD_ACTION_OFFSET}]"));   // what this scheme opens
    emitter.instruction("cmp w11, #3");                                         // an anonymous temporary buffer?
    emitter.instruction("b.eq __rt_pwo_tmpfile");                               // php://memory and php://temp
    emitter.instruction("cmp w11, #4");                                         // a descriptor named in the URL?
    emitter.instruction("b.eq __rt_pwo_fd_number");                             // php://fd/N
    emitter.instruction("cmp w11, #5");                                         // php://temp, which also consumes an id
    emitter.instruction("b.eq __rt_pwo_tmpfile_temp");
    emitter.instruction("mov x0, x11");                                         // otherwise the action IS the descriptor
    emitter.instruction("b __rt_pwo_dup");                                      // hand out a copy of it

    // `php://temp` opens the same anonymous buffer as `php://memory` and, in php, registers a
    // SECOND resource for the memory stream it wraps. The flag is published here and burnt by
    // the caller once the stream has taken its own id, so the temp stream keeps the FIRST one.
    emitter.label("__rt_pwo_tmpfile_temp");
    abi::emit_load_int_immediate(emitter, "x9", 1);
    abi::emit_store_reg_to_symbol(emitter, "x9", "_php_temp_id_pending", 0);
    emitter.label("__rt_pwo_tmpfile");
    emitter.instruction("bl __rt_tmpfile");                                     // x0 = the temporary descriptor
    emitter.instruction("b __rt_pwo_done");                                     // it is already ours to hand out

    // php://fd/N: the digits sit right after the matched name.
    //
    // php-src reads them with `ZEND_STRTOL(start, &end, 10)` and refuses the URL with the FORM
    // sentence when nothing parsed or a byte is left over — NOT with the unknown-target pair,
    // which is what a non-digit used to reach here. A sign parses, so `php://fd/-1` is a NUMBER
    // and gets the range sentence `__rt_php_fd_open` words instead.
    emitter.label("__rt_pwo_fd_number");
    emitter.instruction("ldr x13, [sp, #0]");                                   // the sub-scheme bytes
    emitter.instruction("ldr x12, [sp, #8]");                                   // its length
    emitter.instruction("add x13, x13, x10");                                   // step past "fd/"
    emitter.instruction("sub x12, x12, x10");                                   // bytes of digits remaining
    emitter.instruction("cmp x12, #1");                                         // php://fd/ with no number names nothing
    emitter.instruction("b.lt __rt_pwo_fd_form");                               // php has its own sentence for this one
    emitter.instruction("mov x14, #0");                                         // digit index
    emitter.instruction("mov x0, #0");                                          // accumulated descriptor
    emitter.instruction("mov x11, #0");                                         // negative flag (x11 held the action byte, which is spent)
    emitter.instruction("ldrb w15, [x13]");                                     // the byte a sign would occupy
    emitter.instruction("cmp w15, #0x2B");                                      // '+'
    emitter.instruction("b.eq __rt_pwo_fd_sign");
    emitter.instruction("cmp w15, #0x2D");                                      // '-'
    emitter.instruction("b.ne __rt_pwo_fd_digit");                              // no sign: the first byte is a digit
    emitter.instruction("mov x11, #1");                                         // remember to negate the result
    emitter.label("__rt_pwo_fd_sign");
    emitter.instruction("add x14, x14, #1");                                    // the sign is consumed, not accumulated
    emitter.instruction("cmp x14, x12");                                        // a lone sign parses no number at all
    emitter.instruction("b.hs __rt_pwo_fd_form");
    emitter.label("__rt_pwo_fd_digit");
    emitter.instruction("cmp x14, x12");                                        // consumed every byte?
    emitter.instruction("b.hs __rt_pwo_fd_ready");                              // the number is complete
    emitter.instruction("ldrb w15, [x13, x14]");                                // one candidate digit
    emitter.instruction("sub w15, w15, #48");                                   // fold ASCII '0' to zero
    emitter.instruction("cmp w15, #9");                                         // is it actually a digit?
    emitter.instruction("b.hi __rt_pwo_fd_form");                               // a leftover byte reads as no number
    emitter.instruction("mov x16, #10");
    emitter.instruction("mul x0, x0, x16");                                     // shift the accumulator one decimal place
    emitter.instruction("add x0, x0, x15");                                     // and add the new digit
    emitter.instruction("add x14, x14, #1");                                    // advance the digit index
    emitter.instruction("b __rt_pwo_fd_digit");                                 // keep parsing

    // The descriptor goes to the opener that words php's two refusals, not to the bare `dup`
    // the fixed descriptors use: those always exist, and this one may not.
    emitter.label("__rt_pwo_fd_ready");
    emitter.instruction("cbz x11, __rt_pwo_fd_open");                           // no sign to apply
    emitter.instruction("neg x0, x0");                                          // php's strtol reads the sign as part of the number
    emitter.label("__rt_pwo_fd_open");
    emitter.instruction("ldr x1, [sp, #24]");                                   // the URI, whole, for the refusal line
    emitter.instruction("ldr x2, [sp, #32]");
    emitter.instruction("bl __rt_php_fd_open");                                 // x0 = the copy, or -1 after saying why
    emitter.instruction("b __rt_pwo_done");

    emitter.label("__rt_pwo_dup");
    emitter.bl_c("dup");                                                        // hand out a copy, never the process's own descriptor
    emitter.label("__rt_pwo_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the dispatch frame
    emitter.instruction("ret");                                                 // return the descriptor

    // -- `php://fd/` with no number: php words this one itself, in the single line --
    emitter.label("__rt_pwo_fd_form");
    abi::emit_symbol_address(emitter, "x9", "_diag_php_fd_form");
    emitter.instruction("str x9, [sp, #0]");                                    // park the reason; the sub-scheme is finished with
    emitter.instruction(&format!("mov x9, #{}", PHP_FD_FORM.len()));
    emitter.instruction("str x9, [sp, #8]");
    emitter.instruction("b __rt_pwo_refuse");

    emitter.label("__rt_pwo_unknown");
    // php-src reports this one with a DIRECT `php_error_docref`, so it prints at once, on its own
    // line, and leaves the wrapper error stack empty — which is why the failed-open line below has
    // nothing left to say but `operation failed`. Measured: `fopen("php://bogus","r")` prints both.
    // php names the CALLING builtin here, and this helper sees only a path. The lowering knows
    // the name at every call site and publishes the whole line; zero means it did not, and the
    // `fopen` wording this always had is the fallback.
    abi::emit_symbol_address(emitter, "x1", "_diag_php_invalid_url");
    emitter.instruction(&format!(
        "mov x2, #{}",
        crate::codegen_support::runtime::io::PHP_INVALID_URL_LINE.len()
    ));
    abi::emit_symbol_address(emitter, "x9", "_pwo_callee_invalid_ptr");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("cbz x10, __rt_pwo_invalid_line_ready");
    emitter.instruction("mov x1, x10");
    abi::emit_symbol_address(emitter, "x9", "_pwo_callee_invalid_len");
    emitter.instruction("ldr x2, [x9]");
    emitter.label("__rt_pwo_invalid_line_ready");
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x9", "_diag_open_operation_failed");
    emitter.instruction("str x9, [sp, #0]");
    emitter.instruction(&format!(
        "mov x9, #{}",
        crate::codegen_support::runtime::io::OPEN_OPERATION_FAILED.len()
    ));
    emitter.instruction("str x9, [sp, #8]");

    emitter.label("__rt_pwo_refuse");
    // The composer wants a NUL-terminated path and the URI arrived as a pointer/length pair, so it
    // goes through the same `__rt_cstr` scratch `__rt_fopen` uses; the composer copies out of it
    // before anything else can claim it.
    emitter.instruction("ldr x1, [sp, #24]");                                   // the URI, still whole
    emitter.instruction("ldr x2, [sp, #32]");
    emitter.instruction("bl __rt_cstr");                                        // x0 = a NUL-terminated copy
    emitter.instruction("mov x2, x0");                                          // the path php names in the parentheses
    emitter.instruction("ldr x3, [sp, #0]");                                    // the reason this refusal chose
    emitter.instruction("ldr x4, [sp, #8]");
    abi::emit_symbol_address(emitter, "x0", "_diag_open_failed_fopen_prefix");
    emitter.instruction(&format!("mov x1, #{}", "Warning: fopen(".len()));
    // The same published prefix, for the same reason.
    abi::emit_symbol_address(emitter, "x9", "_pwo_callee_prefix_ptr");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("cbz x10, __rt_pwo_prefix_ready");
    emitter.instruction("mov x0, x10");
    abi::emit_symbol_address(emitter, "x9", "_pwo_callee_prefix_len");
    emitter.instruction("ldr x1, [x9]");
    emitter.label("__rt_pwo_prefix_ready");
    emitter.instruction("bl __rt_open_failed_reason_warning");
    emitter.instruction("mov x0, #-1");                                         // an unrecognised php:// URL opens nothing
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the dispatch frame
    emitter.instruction("ret");                                                 // report the failure
}

/// x86_64 form of [`emit_aarch64`].
///
/// `__rt_php_wrapper_open(rdi = path, rsi = length) -> rax = descriptor, or -1`.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: open a run-time php:// URL ---");
    emitter.label_global("__rt_php_wrapper_open");
    // Frame: [rbp-8]=sub-scheme pointer [rbp-16]=sub-scheme length [rbp-24]=whole URI pointer
    //        [rbp-32]=whole URI length. See the AArch64 counterpart on why the URI is kept whole.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 48");                                         // reserve the dispatch spill slots
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // the URI, whole, for the refusal line
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");
    emitter.instruction("cmp rsi, 6");                                          // is there anything after "php://"?
    emitter.instruction("jle __rt_pwo_unknown_x");                              // a bare "php://" names no stream
    emitter.instruction("add rdi, 6");                                          // skip the scheme prefix
    emitter.instruction("sub rsi, 6");                                          // and shorten the length to match
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the sub-scheme pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the sub-scheme length

    abi::emit_symbol_address(emitter, "r9", "_php_wrapper_schemes");            // walk cursor
    emitter.label("__rt_pwo_next_record_x");
    emitter.instruction(&format!("movzx r10d, BYTE PTR [r9 + {RECORD_LEN_OFFSET}]")); // the record's name length
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_pwo_unknown_x");                               // a zero length terminates the table
    emitter.instruction(&format!("movzx r11d, BYTE PTR [r9 + {RECORD_PREFIX_OFFSET}]")); // prefix record?
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // the sub-scheme length
    emitter.instruction("cmp rcx, r10");                                        // enough bytes for the name?
    emitter.instruction("jl __rt_pwo_advance_x");                               // too short to match
    emitter.instruction("test r11, r11");
    emitter.instruction("jnz __rt_pwo_compare_x");                              // a prefix record accepts a longer input
    emitter.instruction("cmp rcx, r10");                                        // an exact record needs the same length
    emitter.instruction("jne __rt_pwo_advance_x");                              // a longer input is a different scheme

    emitter.label("__rt_pwo_compare_x");
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // the sub-scheme bytes
    emitter.instruction("xor rdx, rdx");                                        // comparison index
    emitter.label("__rt_pwo_compare_byte_x");
    emitter.instruction("cmp rdx, r10");                                        // compared the whole name?
    emitter.instruction("jae __rt_pwo_matched_x");                              // every byte agreed
    emitter.instruction("movzx eax, BYTE PTR [r8 + rdx]");                      // one input byte
    emitter.instruction("movzx edi, BYTE PTR [r9 + rdx]");                      // the corresponding name byte
    emitter.instruction("cmp al, dil");                                         // do they agree?
    emitter.instruction("jne __rt_pwo_advance_x");                              // this record does not match
    emitter.instruction("add rdx, 1");                                          // advance the comparison index
    emitter.instruction("jmp __rt_pwo_compare_byte_x");                         // keep comparing

    emitter.label("__rt_pwo_advance_x");
    emitter.instruction(&format!("add r9, {RECORD_STRIDE}"));                   // step to the next record
    emitter.instruction("jmp __rt_pwo_next_record_x");                          // continue the walk

    emitter.label("__rt_pwo_matched_x");
    emitter.instruction(&format!("movzx r11d, BYTE PTR [r9 + {RECORD_ACTION_OFFSET}]")); // what this scheme opens
    emitter.instruction("cmp r11, 3");                                          // an anonymous temporary buffer?
    emitter.instruction("je __rt_pwo_tmpfile_x");                               // php://memory
    emitter.instruction("cmp r11, 4");                                          // a descriptor named in the URL?
    emitter.instruction("je __rt_pwo_fd_number_x");                             // php://fd/N
    emitter.instruction("cmp r11, 5");                                          // php://temp, which also consumes an id
    emitter.instruction("je __rt_pwo_tmpfile_temp_x");
    emitter.instruction("mov rdi, r11");                                        // otherwise the action IS the descriptor
    emitter.instruction("jmp __rt_pwo_dup_x");                                  // hand out a copy of it

    // See the AArch64 counterpart: php registers a second resource for the memory stream a
    // `php://temp` wraps, and the caller burns the id once this stream has taken its own.
    emitter.label("__rt_pwo_tmpfile_temp_x");
    abi::emit_load_int_immediate(emitter, "r9", 1);
    abi::emit_store_reg_to_symbol(emitter, "r9", "_php_temp_id_pending", 0);
    emitter.label("__rt_pwo_tmpfile_x");
    emitter.instruction("call __rt_tmpfile");                                   // rax = the temporary descriptor
    emitter.instruction("jmp __rt_pwo_done_x");                                 // it is already ours to hand out

    emitter.label("__rt_pwo_fd_number_x");
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // the sub-scheme bytes
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // its length
    emitter.instruction("add r8, r10");                                         // step past "fd/"
    emitter.instruction("sub rcx, r10");                                        // bytes of digits remaining
    emitter.instruction("cmp rcx, 1");                                          // php://fd/ with no number names nothing
    emitter.instruction("jl __rt_pwo_fd_form_x");                               // php has its own sentence for this one
    emitter.instruction("xor rdx, rdx");                                        // digit index
    emitter.instruction("xor rdi, rdi");                                        // accumulated descriptor
    emitter.instruction("xor r11, r11");                                        // negative flag (the action byte is spent)
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // the byte a sign would occupy
    emitter.instruction("cmp eax, 0x2B");                                       // '+'
    emitter.instruction("je __rt_pwo_fd_sign_x");
    emitter.instruction("cmp eax, 0x2D");                                       // '-'
    emitter.instruction("jne __rt_pwo_fd_digit_x");                             // no sign: the first byte is a digit
    emitter.instruction("mov r11, 1");                                          // remember to negate the result
    emitter.label("__rt_pwo_fd_sign_x");
    emitter.instruction("add rdx, 1");                                          // the sign is consumed, not accumulated
    emitter.instruction("cmp rdx, rcx");                                        // a lone sign parses no number at all
    emitter.instruction("jae __rt_pwo_fd_form_x");
    emitter.label("__rt_pwo_fd_digit_x");
    emitter.instruction("cmp rdx, rcx");                                        // consumed every byte?
    emitter.instruction("jae __rt_pwo_fd_ready_x");                             // the number is complete
    emitter.instruction("movzx eax, BYTE PTR [r8 + rdx]");                      // one candidate digit
    emitter.instruction("sub eax, 48");                                         // fold ASCII '0' to zero
    emitter.instruction("cmp eax, 9");                                          // is it actually a digit?
    emitter.instruction("ja __rt_pwo_fd_form_x");                               // a leftover byte reads as no number
    emitter.instruction("imul rdi, rdi, 10");                                   // shift the accumulator one decimal place
    emitter.instruction("add rdi, rax");                                        // and add the new digit
    emitter.instruction("add rdx, 1");                                          // advance the digit index
    emitter.instruction("jmp __rt_pwo_fd_digit_x");                             // keep parsing

    // The descriptor goes to the opener that words php's two refusals, not to the bare `dup`
    // the fixed descriptors use: those always exist, and this one may not.
    emitter.label("__rt_pwo_fd_ready_x");
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_pwo_fd_open_x");                               // no sign to apply
    emitter.instruction("neg rdi");                                             // php's strtol reads the sign as part of the number
    emitter.label("__rt_pwo_fd_open_x");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // the URI, whole, for the refusal line
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");
    emitter.instruction("call __rt_php_fd_open");                               // rax = the copy, or -1 after saying why
    emitter.instruction("jmp __rt_pwo_done_x");

    emitter.label("__rt_pwo_dup_x");
    emitter.bl_c("dup");                                                        // hand out a copy, never the process's own descriptor
    emitter.label("__rt_pwo_done_x");
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the descriptor

    // -- `php://fd/` with no number: php words this one itself, in the single line --
    emitter.label("__rt_pwo_fd_form_x");
    abi::emit_symbol_address(emitter, "r9", "_diag_php_fd_form");
    emitter.instruction("mov QWORD PTR [rbp - 8], r9");                         // park the reason; the sub-scheme is finished with
    emitter.instruction(&format!("mov QWORD PTR [rbp - 16], {}", PHP_FD_FORM.len()));
    emitter.instruction("jmp __rt_pwo_refuse_x");

    emitter.label("__rt_pwo_unknown_x");
    // See the AArch64 counterpart: php reports this one with a direct `php_error_docref`, which is
    // why a second line follows saying only `operation failed`.
    abi::emit_symbol_address(emitter, "rdi", "_diag_php_invalid_url");
    emitter.instruction(&format!(
        "mov esi, {}",
        crate::codegen_support::runtime::io::PHP_INVALID_URL_LINE.len()
    ));
    // The line the LOWERING composed in the calling builtin's name, when it published one.
    abi::emit_symbol_address(emitter, "r9", "_pwo_callee_invalid_ptr");
    emitter.instruction("mov r10, QWORD PTR [r9]");
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_pwo_invalid_line_ready_x");
    emitter.instruction("mov rdi, r10");
    abi::emit_symbol_address(emitter, "r9", "_pwo_callee_invalid_len");
    emitter.instruction("mov rsi, QWORD PTR [r9]");
    emitter.label("__rt_pwo_invalid_line_ready_x");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "r9", "_diag_open_operation_failed");
    emitter.instruction("mov QWORD PTR [rbp - 8], r9");
    emitter.instruction(&format!(
        "mov QWORD PTR [rbp - 16], {}",
        crate::codegen_support::runtime::io::OPEN_OPERATION_FAILED.len()
    ));

    emitter.label("__rt_pwo_refuse_x");
    // See the AArch64 counterpart on the `__rt_cstr` round trip.
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the URI, still whole
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");
    emitter.instruction("call __rt_cstr");                                      // rax = a NUL-terminated copy
    emitter.instruction("mov rdx, rax");                                        // the path php names in the parentheses
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // the reason this refusal chose
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");
    abi::emit_symbol_address(emitter, "rdi", "_diag_open_failed_fopen_prefix");
    emitter.instruction(&format!("mov esi, {}", "Warning: fopen(".len()));
    // The same published prefix, for the same reason.
    abi::emit_symbol_address(emitter, "r9", "_pwo_callee_prefix_ptr");
    emitter.instruction("mov r10, QWORD PTR [r9]");
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_pwo_prefix_ready_x");
    emitter.instruction("mov rdi, r10");
    abi::emit_symbol_address(emitter, "r9", "_pwo_callee_prefix_len");
    emitter.instruction("mov rsi, QWORD PTR [r9]");
    emitter.label("__rt_pwo_prefix_ready_x");
    emitter.instruction("call __rt_open_failed_reason_warning");
    emitter.instruction("mov rax, -1");                                         // an unrecognised php:// URL opens nothing
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // report the failure
}
