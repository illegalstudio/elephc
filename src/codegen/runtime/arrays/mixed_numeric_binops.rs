//! Purpose:
//! Emits runtime helpers for arithmetic on boxed Mixed numeric values.
//! Centralizes PHP integer-overflow promotion for dynamic int|float results.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Helpers return a boxed Mixed cell so callers can observe either integer or double at runtime.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};

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

    emitter.label("__rt_mixed_numeric_common");
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

    // -- integer path with PHP overflow promotion --
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

    // -- integer path with PHP overflow promotion --
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
