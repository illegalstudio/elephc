//! Purpose:
//! Emits runtime helpers for arithmetic on boxed Mixed numeric values.
//! Centralizes PHP integer-overflow promotion for dynamic int|float results.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Helpers return a boxed Mixed cell so callers can observe either integer or double at runtime.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{
    abi,
    platform::{Arch, Platform},
};

/// Dispatches to architecture-specific helpers for add/sub/mul on boxed Mixed numeric values.
///
/// For ARM64 emits `__rt_mixed_numeric_add/sub/mul` that unbox operands, classify each
/// payload as integer or double, and compute in integer or floating-point arithmetic with
/// PHP integer-overflow promotion (overflowing integers are promoted to double).
///
/// For x86_64 emits the equivalent Linux x86_64 ABI helpers under the same symbol names.
///
/// Input:  AArch64 x0=left Mixed*, x1=right Mixed*
///         x86_64 rax=left Mixed*, rdi=right Mixed*
/// Output: boxed Mixed pointer in the integer result register (x0 / rax)
pub fn emit_mixed_numeric_binops(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_numeric_binops_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_numeric_binops ---");

    emit_aarch64_entry(emitter, "__rt_mixed_numeric_add", 0);
    emit_aarch64_entry(emitter, "__rt_mixed_numeric_sub", 1);
    emit_aarch64_entry(emitter, "__rt_mixed_numeric_mul", 2);

    // Shared body reached via `b` from `add`/`sub`/`mul`: `.alt_entry` under macOS
    // dead stripping keeps it a real symbol (so a cross-helper `b` from a live
    // entry keeps it alive) without splitting the atom or being `L`-localized.
    emitter.label_shared("__rt_mixed_numeric_common");
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed left operand pointer for unboxing and casts
    emitter.instruction("str x1, [sp, #8]");                                    // save the boxed right operand pointer for unboxing and casts
    emitter.instruction("str x9, [sp, #16]");                                   // save the selected arithmetic opcode across helper calls

    // -- classify operands so float payloads force floating-point arithmetic --
    emitter.instruction("bl __rt_mixed_unbox");                                 // inspect the left boxed payload tag and value words
    emitter.instruction("str x0, [sp, #24]");                                   // save the left runtime value tag for numeric dispatch
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the boxed right operand pointer for unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // inspect the right boxed payload tag and value words
    emitter.instruction("str x0, [sp, #32]");                                   // save the right runtime value tag for numeric dispatch
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left runtime value tag
    emitter.instruction("cmp x9, #2");                                          // does the left operand hold a double payload?
    emitter.instruction("b.eq __rt_mixed_numeric_float_path");                  // any double payload makes the whole operation double-valued
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the right runtime value tag
    emitter.instruction("cmp x9, #2");                                          // does the right operand hold a double payload?
    emitter.instruction("b.eq __rt_mixed_numeric_float_path");                  // any double payload makes the whole operation double-valued

    // -- a float-form numeric-string operand ("1.5") forces the float path --
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left runtime value tag
    emitter.instruction("cmp x9, #1");                                          // does the left operand hold a string payload?
    emitter.instruction("b.ne __rt_mixed_numeric_str_check_right");             // left is not a string: test the right operand instead
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed left string operand for classification
    emitter.instruction("bl __rt_mixed_str_operand_is_float");                  // classify the left string as int-form or float-form via PHP grammar
    emitter.instruction("cbnz x0, __rt_mixed_numeric_float_path");              // a float-form left string makes the whole operation double-valued
    emitter.label("__rt_mixed_numeric_str_check_right");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the right runtime value tag
    emitter.instruction("cmp x9, #1");                                          // does the right operand hold a string payload?
    emitter.instruction("b.ne __rt_mixed_numeric_int_path");                    // right is not a string: fall through to the integer path
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed right string operand for classification
    emitter.instruction("bl __rt_mixed_str_operand_is_float");                  // classify the right string as int-form or float-form via PHP grammar
    emitter.instruction("cbnz x0, __rt_mixed_numeric_float_path");              // a float-form right string makes the whole operation double-valued

    // -- integer path with PHP overflow promotion --
    emitter.label("__rt_mixed_numeric_int_path");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed left operand before casting to integer
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce the left operand using the current integer numeric rules
    emitter.instruction("str x0, [sp, #40]");                                   // save the left integer payload across the right cast
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed right operand before casting to integer
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce the right operand using the current integer numeric rules
    emitter.instruction("mov x2, x0");                                          // keep the right integer operand in x2 for arithmetic and overflow fallback
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the left integer operand into x1
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the selected arithmetic opcode
    emitter.instruction("cmp x9, #1");                                          // is this helper handling subtraction?
    emitter.instruction("b.eq __rt_mixed_numeric_int_sub");                     // branch to the subtraction overflow sequence
    emitter.instruction("cmp x9, #2");                                          // is this helper handling multiplication?
    emitter.instruction("b.eq __rt_mixed_numeric_int_mul");                     // branch to the multiplication overflow sequence

    emitter.label("__rt_mixed_numeric_int_add");
    emitter.instruction("adds x0, x1, x2");                                     // compute integer addition and set overflow flags
    emitter.instruction("b.vs __rt_mixed_numeric_int_overflow");                // promote to double when signed addition overflowed
    emitter.instruction("b __rt_mixed_numeric_box_int");                        // box the in-range integer result

    emitter.label("__rt_mixed_numeric_int_sub");
    emitter.instruction("subs x0, x1, x2");                                     // compute integer subtraction and set overflow flags
    emitter.instruction("b.vs __rt_mixed_numeric_int_overflow");                // promote to double when signed subtraction overflowed
    emitter.instruction("b __rt_mixed_numeric_box_int");                        // box the in-range integer result

    emitter.label("__rt_mixed_numeric_int_mul");
    emitter.instruction("mul x0, x1, x2");                                      // compute the low half of the signed integer product
    emitter.instruction("smulh x3, x1, x2");                                    // compute the high half needed for overflow detection
    emitter.instruction("cmp x3, x0, asr #63");                                 // high half must equal the sign extension of the low half
    emitter.instruction("b.ne __rt_mixed_numeric_int_overflow");                // promote to double when signed multiplication overflowed

    emitter.label("__rt_mixed_numeric_box_int");
    emitter.instruction("mov x1, x0");                                          // move the integer result into the Mixed helper payload register
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the integer result into a Mixed cell
    emitter.instruction("b __rt_mixed_numeric_done");                           // restore the helper frame and return the boxed result

    emitter.label("__rt_mixed_numeric_int_overflow");
    emitter.instruction("scvtf d0, x1");                                        // convert the original left integer to double for PHP overflow promotion
    emitter.instruction("scvtf d1, x2");                                        // convert the original right integer to double for PHP overflow promotion
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the selected arithmetic opcode for the double fallback
    emitter.instruction("cmp x9, #1");                                          // is this overflow fallback for subtraction?
    emitter.instruction("b.eq __rt_mixed_numeric_float_sub_loaded");            // use floating-point subtraction for an overflowing integer subtraction
    emitter.instruction("cmp x9, #2");                                          // is this overflow fallback for multiplication?
    emitter.instruction("b.eq __rt_mixed_numeric_float_mul_loaded");            // use floating-point multiplication for an overflowing integer multiplication
    emitter.instruction("b __rt_mixed_numeric_float_add_loaded");               // use floating-point addition for an overflowing integer addition

    // -- float path: cast both operands to double, then box the double result --
    emitter.label("__rt_mixed_numeric_float_path");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed left operand before casting to double
    emitter.instruction("bl __rt_mixed_cast_float");                            // coerce the left operand to double
    emitter.instruction("str d0, [sp, #48]");                                   // save the left double across the right cast
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed right operand before casting to double
    emitter.instruction("bl __rt_mixed_cast_float");                            // coerce the right operand to double
    emitter.instruction("fmov d1, d0");                                         // keep the right double operand in d1
    emitter.instruction("ldr d0, [sp, #48]");                                   // reload the left double operand into d0
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the selected arithmetic opcode for double arithmetic
    emitter.instruction("cmp x9, #1");                                          // is this helper handling subtraction?
    emitter.instruction("b.eq __rt_mixed_numeric_float_sub_loaded");            // branch to the floating-point subtraction sequence
    emitter.instruction("cmp x9, #2");                                          // is this helper handling multiplication?
    emitter.instruction("b.eq __rt_mixed_numeric_float_mul_loaded");            // branch to the floating-point multiplication sequence

    emitter.label("__rt_mixed_numeric_float_add_loaded");
    emitter.instruction("fadd d0, d0, d1");                                     // compute the double addition result
    emitter.instruction("b __rt_mixed_numeric_box_float");                      // box the double result

    emitter.label("__rt_mixed_numeric_float_sub_loaded");
    emitter.instruction("fsub d0, d0, d1");                                     // compute the double subtraction result
    emitter.instruction("b __rt_mixed_numeric_box_float");                      // box the double result

    emitter.label("__rt_mixed_numeric_float_mul_loaded");
    emitter.instruction("fmul d0, d0, d1");                                     // compute the double multiplication result

    emitter.label("__rt_mixed_numeric_box_float");
    emitter.instruction("fmov x1, d0");                                         // move the double bits into the Mixed helper payload register
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the double result into a Mixed cell

    emitter.label("__rt_mixed_numeric_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return to generated code with boxed Mixed result in x0

    emit_str_operand_is_float(emitter);
}

/// Emits the ARM64 entry point for one mixed numeric binary operation.
///
/// Allocates a 80-byte helper frame on the stack, saves the frame pointer and link register,
/// then loads `opcode` into x9 and branches to the shared `__rt_mixed_numeric_common` implementation.
///
/// - `label`: global symbol name for the entry point (e.g. `__rt_mixed_numeric_add`)
/// - `opcode`: 0 = add, 1 = sub, 2 = mul — passed via x9 to the common handler
fn emit_aarch64_entry(emitter: &mut Emitter, label: &str, opcode: i64) {
    emitter.label_global(label);
    emitter.instruction("sub sp, sp, #80");                                     // allocate a helper frame for operands, tags, and saved FP state
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a stable helper frame pointer
    abi::emit_load_int_immediate(emitter, "x9", opcode);
    emitter.instruction("b __rt_mixed_numeric_common");                         // enter the shared mixed numeric implementation
}

/// Emits the Linux x86_64 ABI helpers for mixed numeric add/sub/mul.
///
/// Each entry point (`__rt_mixed_numeric_add/sub/mul`) establishes a frame via `push rbp`,
/// allocates 80 bytes of stack space, loads the opcode into r10, and jumps to the shared
/// `__rt_mixed_numeric_common_linux_x86_64` implementation.
///
/// The common handler unboxes both operands, classifies each as integer or double, and
/// dispatches to the appropriate arithmetic path with PHP integer-overflow promotion
/// (overflowing integers are converted to double before the operation).
fn emit_mixed_numeric_binops_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_numeric_binops ---");

    emit_x86_64_entry(emitter, "__rt_mixed_numeric_add", 0);
    emit_x86_64_entry(emitter, "__rt_mixed_numeric_sub", 1);
    emit_x86_64_entry(emitter, "__rt_mixed_numeric_mul", 2);

    emitter.label("__rt_mixed_numeric_common_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the boxed left operand pointer for unboxing and casts
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the boxed right operand pointer for unboxing and casts
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the selected arithmetic opcode across helper calls

    // -- classify operands so float payloads force floating-point arithmetic --
    emitter.instruction("call __rt_mixed_unbox");                               // inspect the left boxed payload tag and value words
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the left runtime value tag for numeric dispatch
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the boxed right operand pointer for unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // inspect the right boxed payload tag and value words
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the right runtime value tag for numeric dispatch
    emitter.instruction("cmp QWORD PTR [rbp - 32], 2");                         // does the left operand hold a double payload?
    emitter.instruction("je __rt_mixed_numeric_float_path_linux_x86_64");       // any double payload makes the whole operation double-valued
    emitter.instruction("cmp QWORD PTR [rbp - 40], 2");                         // does the right operand hold a double payload?
    emitter.instruction("je __rt_mixed_numeric_float_path_linux_x86_64");       // any double payload makes the whole operation double-valued

    // -- a float-form numeric-string operand ("1.5") forces the float path --
    emitter.instruction("cmp QWORD PTR [rbp - 32], 1");                         // does the left operand hold a string payload?
    emitter.instruction("jne __rt_mixed_numeric_str_check_right_x86_64");       // left is not a string: test the right operand instead
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed left string operand for classification
    emitter.instruction("call __rt_mixed_str_operand_is_float");                // classify the left string as int-form or float-form via PHP grammar
    emitter.instruction("test rax, rax");                                       // is the left string float-form?
    emitter.instruction("jne __rt_mixed_numeric_float_path_linux_x86_64");      // a float-form left string makes the whole operation double-valued
    emitter.label("__rt_mixed_numeric_str_check_right_x86_64");
    emitter.instruction("cmp QWORD PTR [rbp - 40], 1");                         // does the right operand hold a string payload?
    emitter.instruction("jne __rt_mixed_numeric_int_path_linux_x86_64");        // right is not a string: fall through to the integer path
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed right string operand for classification
    emitter.instruction("call __rt_mixed_str_operand_is_float");                // classify the right string as int-form or float-form via PHP grammar
    emitter.instruction("test rax, rax");                                       // is the right string float-form?
    emitter.instruction("jne __rt_mixed_numeric_float_path_linux_x86_64");      // a float-form right string makes the whole operation double-valued

    // -- integer path with PHP overflow promotion --
    emitter.label("__rt_mixed_numeric_int_path_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed left operand before casting to integer
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce the left operand using the current integer numeric rules
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the left integer payload across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed right operand before casting to integer
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce the right operand using the current integer numeric rules
    emitter.instruction("mov r11, rax");                                        // keep the right integer operand in r11
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the left integer operand into r10
    emitter.instruction("mov r8, r10");                                         // preserve the original left integer for overflow promotion
    emitter.instruction("mov r9, r11");                                         // preserve the original right integer for overflow promotion
    emitter.instruction("cmp QWORD PTR [rbp - 24], 1");                         // is this helper handling subtraction?
    emitter.instruction("je __rt_mixed_numeric_int_sub_linux_x86_64");          // branch to the subtraction overflow sequence
    emitter.instruction("cmp QWORD PTR [rbp - 24], 2");                         // is this helper handling multiplication?
    emitter.instruction("je __rt_mixed_numeric_int_mul_linux_x86_64");          // branch to the multiplication overflow sequence

    emitter.label("__rt_mixed_numeric_int_add_linux_x86_64");
    emitter.instruction("add r10, r11");                                        // compute integer addition and set overflow flags
    emitter.instruction("jo __rt_mixed_numeric_int_overflow_linux_x86_64");     // promote to double when signed addition overflowed
    emitter.instruction("jmp __rt_mixed_numeric_box_int_linux_x86_64");         // box the in-range integer result

    emitter.label("__rt_mixed_numeric_int_sub_linux_x86_64");
    emitter.instruction("sub r10, r11");                                        // compute integer subtraction and set overflow flags
    emitter.instruction("jo __rt_mixed_numeric_int_overflow_linux_x86_64");     // promote to double when signed subtraction overflowed
    emitter.instruction("jmp __rt_mixed_numeric_box_int_linux_x86_64");         // box the in-range integer result

    emitter.label("__rt_mixed_numeric_int_mul_linux_x86_64");
    emitter.instruction("mov rax, r10");                                        // move the left operand into rax for one-operand signed multiply
    emitter.instruction("imul r11");                                            // compute signed multiplication and set overflow flags
    emitter.instruction("jo __rt_mixed_numeric_int_overflow_linux_x86_64");     // promote to double when signed multiplication overflowed
    emitter.instruction("mov r10, rax");                                        // keep the in-range product in the integer result scratch

    emitter.label("__rt_mixed_numeric_box_int_linux_x86_64");
    emitter.instruction("mov rdi, r10");                                        // move the integer result into the Mixed helper payload register
    emitter.instruction("xor rsi, rsi");                                        // integer payloads do not use a high word
    emitter.instruction("mov rax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the integer result into a Mixed cell
    emitter.instruction("jmp __rt_mixed_numeric_done_linux_x86_64");            // restore the helper frame and return the boxed result

    emitter.label("__rt_mixed_numeric_int_overflow_linux_x86_64");
    emitter.instruction("cvtsi2sd xmm0, r8");                                   // convert the original left integer to double for PHP overflow promotion
    emitter.instruction("cvtsi2sd xmm1, r9");                                   // convert the original right integer to double for PHP overflow promotion
    emitter.instruction("cmp QWORD PTR [rbp - 24], 1");                         // is this overflow fallback for subtraction?
    emitter.instruction("je __rt_mixed_numeric_float_sub_loaded_linux_x86_64"); // use floating-point subtraction for an overflowing integer subtraction
    emitter.instruction("cmp QWORD PTR [rbp - 24], 2");                         // is this overflow fallback for multiplication?
    emitter.instruction("je __rt_mixed_numeric_float_mul_loaded_linux_x86_64"); // use floating-point multiplication for an overflowing integer multiplication
    emitter.instruction("jmp __rt_mixed_numeric_float_add_loaded_linux_x86_64"); // use floating-point addition for an overflowing integer addition

    // -- float path: cast both operands to double, then box the double result --
    emitter.label("__rt_mixed_numeric_float_path_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed left operand before casting to double
    emitter.instruction("call __rt_mixed_cast_float");                          // coerce the left operand to double
    emitter.instruction("movsd QWORD PTR [rbp - 56], xmm0");                    // save the left double across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed right operand before casting to double
    emitter.instruction("call __rt_mixed_cast_float");                          // coerce the right operand to double
    emitter.instruction("movapd xmm1, xmm0");                                   // keep the right double operand in xmm1
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 56]");                    // reload the left double operand into xmm0
    emitter.instruction("cmp QWORD PTR [rbp - 24], 1");                         // is this helper handling subtraction?
    emitter.instruction("je __rt_mixed_numeric_float_sub_loaded_linux_x86_64"); // branch to the floating-point subtraction sequence
    emitter.instruction("cmp QWORD PTR [rbp - 24], 2");                         // is this helper handling multiplication?
    emitter.instruction("je __rt_mixed_numeric_float_mul_loaded_linux_x86_64"); // branch to the floating-point multiplication sequence

    emitter.label("__rt_mixed_numeric_float_add_loaded_linux_x86_64");
    emitter.instruction("addsd xmm0, xmm1");                                    // compute the double addition result
    emitter.instruction("jmp __rt_mixed_numeric_box_float_linux_x86_64");       // box the double result

    emitter.label("__rt_mixed_numeric_float_sub_loaded_linux_x86_64");
    emitter.instruction("subsd xmm0, xmm1");                                    // compute the double subtraction result
    emitter.instruction("jmp __rt_mixed_numeric_box_float_linux_x86_64");       // box the double result

    emitter.label("__rt_mixed_numeric_float_mul_loaded_linux_x86_64");
    emitter.instruction("mulsd xmm0, xmm1");                                    // compute the double multiplication result

    emitter.label("__rt_mixed_numeric_box_float_linux_x86_64");
    emitter.instruction("movq rdi, xmm0");                                      // move the double bits into the Mixed helper payload register
    emitter.instruction("xor rsi, rsi");                                        // double payloads do not use a high word
    emitter.instruction("mov rax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the double result into a Mixed cell

    emitter.label("__rt_mixed_numeric_done_linux_x86_64");
    emitter.instruction("add rsp, 80");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code with boxed Mixed result in rax

    emit_str_operand_is_float(emitter);
}

/// Emits the Linux x86_64 entry point for one mixed numeric binary operation.
///
/// Saves and establishes rbp as the frame pointer, allocates an aligned 80-byte stack region
/// for operand slots and saved FP state, loads `opcode` into r10, then jumps to the shared
/// `__rt_mixed_numeric_common_linux_x86_64` implementation.
///
/// - `label`: global symbol name for the entry point (e.g. `__rt_mixed_numeric_add`)
/// - `opcode`: 0 = add, 1 = sub, 2 = mul — saved to the stack and read by the common handler
fn emit_x86_64_entry(emitter: &mut Emitter, label: &str, opcode: i64) {
    emitter.label_global(label);
    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 80");                                         // allocate aligned helper slots for operands, tags, and FP state
    abi::emit_load_int_immediate(emitter, "r10", opcode);
    emitter.instruction("jmp __rt_mixed_numeric_common_linux_x86_64");          // enter the shared mixed numeric implementation
}

/// Emits `__rt_mixed_str_operand_is_float`, which reports whether a boxed Mixed string
/// operand spells a PHP float (`IS_DOUBLE`) rather than an integer.
///
/// Input:  boxed Mixed pointer in the integer result register (x0 / rax).
/// Output: `1` in the integer result register when the numeric string is float-form, `0`
///         otherwise (integer-form, leading-numeric integer, or no numeric prefix).
///
/// The classification clips the string to PHP's leading numeric run through
/// `__rt_php_num_scan` — the same grammar the compile-time classifier and every other string
/// numeric helper use — and then reports `IS_DOUBLE` for either of the two reasons PHP does:
///
/// 1. the clipped run contains a `.` or an exponent marker (`"1.5"`, `"1e3"`);
/// 2. the clipped run is a plain integer spelling that does not fit `i64`
///    (`"99999999999999999999"`), which php-src detects the same way — `ZEND_STRTOL` plus
///    `errno == ERANGE` — and which must be a double *before* the arithmetic runs, or
///    `strtoll`'s `PHP_INT_MAX` saturation silently answers `9.2e18` for `1e20`.
///
/// Clipping first is what keeps `"0x1A"` (run `0`), `"INF"`/`"NAN"` (empty run), and
/// `"1_000"` (run `1`) on the integer path instead of misreading libc `strtod` spellings
/// PHP's grammar rejects.
fn emit_str_operand_is_float(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_operand_is_float_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_str_operand_is_float ---");
    emitter.label_global("__rt_mixed_str_operand_is_float");
    emitter.instruction("sub sp, sp, #32");                                     // allocate a frame for the nested calls plus the clipped-run slot
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across the helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame pointer
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the boxed string: x0=tag, x1=string ptr, x2=string length
    emitter.instruction("bl __rt_cstr");                                        // copy the bounded string into the shared C-string scratch buffer
    emitter.instruction("bl __rt_php_num_scan");                                // clip the scratch to PHP's leading numeric run (x0=run, x1=flag)
    emitter.instruction("str x0, [sp, #0]");                                    // save the clipped run for the integer-range probe below
    emitter.instruction("mov x9, x0");                                          // x9 = scan cursor over the clipped numeric run

    // -- a '.' or exponent marker in the run is PHP's syntactic IS_DOUBLE --
    emitter.label("__rt_msoif_loop");
    emitter.instruction("ldrb w10, [x9]");                                      // load the next byte of the clipped numeric run
    emitter.instruction("cbz w10, __rt_msoif_int_probe");                       // end of run: the run is a plain integer spelling
    emitter.instruction("cmp w10, #46");                                        // ASCII '.' marks a fractional part
    emitter.instruction("b.eq __rt_msoif_found");                               // a decimal point makes the numeric string float-form
    emitter.instruction("orr w11, w10, #0x20");                                 // fold the byte to lowercase ASCII
    emitter.instruction("cmp w11, #101");                                       // lowercase 'e' marks an exponent
    emitter.instruction("b.eq __rt_msoif_found");                               // an exponent makes the numeric string float-form
    emitter.instruction("add x9, x9, #1");                                      // advance to the next byte of the run
    emitter.instruction("b __rt_msoif_loop");                                   // keep scanning the clipped run
    emitter.label("__rt_msoif_found");
    emitter.instruction("mov x0, #1");                                          // report a float-form numeric string
    emitter.instruction("b __rt_msoif_done");                                   // skip the integer-range probe

    // -- an integer spelling wider than i64 is PHP's numeric IS_DOUBLE --
    emitter.label("__rt_msoif_int_probe");
    emitter.bl_c(errno_location_symbol(emitter));                               // fetch the thread-local errno slot before parsing
    emitter.instruction("str wzr, [x0]");                                       // clear errno so only strtoll can report ERANGE below
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the clipped integer run
    emitter.instruction("mov x1, #0");                                          // strtoll endptr = NULL: the run is already clipped
    emitter.instruction("mov x2, #10");                                         // parse in base 10 like PHP string-to-int
    emitter.emit_call_c("strtoll");                                                    // saturates to PHP_INT_MAX/MIN and sets ERANGE past the range
    emitter.bl_c(errno_location_symbol(emitter));                               // fetch the thread-local errno slot strtoll just wrote
    emitter.instruction("ldr w9, [x0]");                                        // load the errno value left by strtoll
    emitter.instruction(&format!("cmp w9, #{}", ERANGE));                       // ERANGE means the integer run overflowed i64
    emitter.instruction("cset x0, eq");                                         // PHP classifies an out-of-range integer string as IS_DOUBLE

    emitter.label("__rt_msoif_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the float-form flag in x0
}

/// Emits the Linux x86_64 variant of `__rt_mixed_str_operand_is_float` (see the AArch64 twin).
fn emit_str_operand_is_float_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_str_operand_is_float ---");
    emitter.label_global("__rt_mixed_str_operand_is_float");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before nested runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer (keeps calls 16-byte aligned)
    emitter.instruction("sub rsp, 16");                                         // reserve an aligned slot for the clipped-run pointer
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the boxed string: rax=tag, rdi=string ptr, rdx=string length
    emitter.instruction("mov rax, rdi");                                        // move the string pointer into the x86_64 string-result pointer register
    emitter.instruction("call __rt_cstr");                                      // copy the bounded string into the shared C-string scratch buffer
    emitter.instruction("mov rdi, rax");                                        // pass the C-string pointer to the numeric-grammar scanner
    emitter.instruction("call __rt_php_num_scan");                              // clip the scratch to PHP's leading numeric run (rax=run, rdx=flag)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the clipped run for the integer-range probe below
    emitter.instruction("mov r8, rax");                                         // r8 = scan cursor over the clipped numeric run

    // -- a '.' or exponent marker in the run is PHP's syntactic IS_DOUBLE --
    emitter.label("__rt_msoif_loop_x86_64");
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                            // load the next byte of the clipped numeric run
    emitter.instruction("test cl, cl");                                         // check for the C-string terminator
    emitter.instruction("jz __rt_msoif_int_probe_x86_64");                      // end of run: the run is a plain integer spelling
    emitter.instruction("cmp cl, 46");                                          // ASCII '.' marks a fractional part
    emitter.instruction("je __rt_msoif_found_x86_64");                          // a decimal point makes the numeric string float-form
    emitter.instruction("mov r9d, ecx");                                        // copy the byte before case folding
    emitter.instruction("or r9d, 32");                                          // fold the byte to lowercase ASCII
    emitter.instruction("cmp r9d, 101");                                        // lowercase 'e' marks an exponent
    emitter.instruction("je __rt_msoif_found_x86_64");                          // an exponent makes the numeric string float-form
    emitter.instruction("inc r8");                                              // advance to the next byte of the run
    emitter.instruction("jmp __rt_msoif_loop_x86_64");                          // keep scanning the clipped run
    emitter.label("__rt_msoif_found_x86_64");
    emitter.instruction("mov eax, 1");                                          // report a float-form numeric string
    emitter.instruction("jmp __rt_msoif_done_x86_64");                          // skip the integer-range probe

    // -- an integer spelling wider than i64 is PHP's numeric IS_DOUBLE --
    emitter.label("__rt_msoif_int_probe_x86_64");
    emitter.bl_c(errno_location_symbol(emitter));                               // fetch the thread-local errno slot before parsing
    emitter.instruction("mov DWORD PTR [rax], 0");                              // clear errno so only strtoll can report ERANGE below
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the clipped integer run as strtoll's first argument
    emitter.instruction("xor esi, esi");                                        // strtoll endptr = NULL: the run is already clipped
    emitter.instruction("mov edx, 10");                                         // parse in base 10 like PHP string-to-int
    emitter.emit_call_c("strtoll");                                                    // saturates to PHP_INT_MAX/MIN and sets ERANGE past the range
    emitter.bl_c(errno_location_symbol(emitter));                               // fetch the thread-local errno slot strtoll just wrote
    emitter.instruction(&format!("cmp DWORD PTR [rax], {}", ERANGE));           // ERANGE means the integer run overflowed i64
    emitter.instruction("sete al");                                             // PHP classifies an out-of-range integer string as IS_DOUBLE
    emitter.instruction("movzx eax, al");                                       // widen the flag to the full integer result register

    emitter.label("__rt_msoif_done_x86_64");
    emitter.instruction("add rsp, 16");                                         // release the clipped-run slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the float-form flag in rax
}

/// POSIX `ERANGE`, the errno `strtoll` sets when the parsed value leaves the `long` range.
/// The value is 34 on every platform in the supported target matrix.
const ERANGE: i32 = 34;

/// Returns the libc symbol that yields the address of the thread-local `errno` slot.
///
/// Same platform split as the other runtime helpers that read errno after a libc call
/// (`crate::codegen_support::runtime::io::streams_ext`,
/// `crate::codegen_support::runtime::strings::mb_strlen`).
fn errno_location_symbol(emitter: &Emitter) -> &'static str {
    match emitter.platform {
        Platform::MacOS => "__error",
        Platform::Linux => "__errno_location",
        Platform::Windows => "__errno_location",
    }
}
