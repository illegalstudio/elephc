//! Purpose:
//! Emits PHP `substr` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `substr(string, offset, length?)` builtin call.
///
/// Evaluates arguments in source order, materializes them into ABI registers,
/// then emits platform-specific instructions that compute the resulting substring
/// pointer and length. Handles negative offsets (converted relative to end) and
/// offset clamping. The optional length is matched to PHP: a non-negative length
/// keeps `min(length, remaining)`, and a negative length drops that many characters
/// from the end (`max(0, remaining + length)`). Whether a length was passed is a
/// compile-time fact (`args.len()`), so no runtime sentinel is used — that avoids
/// confusing an omitted length with an explicit length of `-1`.
///
/// # Arguments
/// - `_name`: unused, always `null` (name resolved by catalog lookup)
/// - `args`: exactly 2 or 3 expressions: `(string, offset, length?)`
/// - `emitter`: drives instruction emission and label allocation
/// - `ctx`: carries target arch, vtable, and local variable layout
/// - `data`: scratch area for relocatable immediates and string data
///
/// # Returns
/// `Some(PhpType::Str)` — the result is always a PHP string.
///
/// # Side effects
/// Clobbers temporary registers used for integer materialization. On x86_64,
/// also clobbers `r8` as a zero materialized for negative clamping. String result
/// is returned as borrowed pointer/length in `x1`/`x2` (AArch64) or `rax`/`rdx` (x86_64).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("substr()");
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    let neg_done = ctx.next_label("substr_neg_done");
    let has_length = args.len() >= 3;
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- save string and evaluate offset --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push string ptr and length onto stack
            super::args::push_int_arg(&args[1], emitter, ctx, data);
            if has_length {
                super::args::emit_int_arg(&args[2], emitter, ctx, data);
                emitter.instruction("mov x3, x0");                              // move length argument to x3
            }
            // -- restore offset and string from stack --
            emitter.instruction("ldr x0, [sp], #16");                           // pop offset into x0
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop string ptr into x1, length into x2
            // -- handle negative offset --
            emitter.instruction("cmp x0, #0");                                  // check if offset is negative
            emitter.instruction(&format!("b.ge {}", neg_done));                 // skip adjustment if offset >= 0
            emitter.instruction("add x0, x2, x0");                              // convert negative offset: offset = length + offset
            emitter.instruction("cmp x0, #0");                                  // check if adjusted offset is still negative
            emitter.instruction("csel x0, xzr, x0, lt");                        // clamp to 0 if offset went below zero
            emitter.label(&neg_done);
            // -- clamp offset to string length --
            emitter.instruction("cmp x0, x2");                                  // compare offset to string length
            emitter.instruction("csel x0, x2, x0, gt");                         // clamp offset to length if it exceeds it
            // -- adjust pointer and compute remaining length --
            emitter.instruction("add x1, x1, x0");                              // advance string pointer by offset bytes
            emitter.instruction("sub x2, x2, x0");                              // remaining = length - offset
            // -- apply the optional length argument when one was passed --
            if has_length {
                let len_neg = ctx.next_label("substr_len_neg");
                let len_done = ctx.next_label("substr_len_done");
                emitter.instruction("cmp x3, #0");                              // check if the requested length is negative
                emitter.instruction(&format!("b.lt {}", len_neg));              // negative length drops chars from the end
                // -- non-negative length: keep min(length, remaining) --
                emitter.instruction("cmp x3, x2");                              // compare requested length to remaining chars
                emitter.instruction("csel x2, x3, x2, lt");                     // result length = min(length, remaining)
                emitter.instruction(&format!("b {}", len_done));                // skip the negative-length handling
                emitter.label(&len_neg);
                // -- negative length: keep max(0, remaining + length) --
                emitter.instruction("add x3, x2, x3");                          // remaining + length (negative) = chars before the cut
                emitter.instruction("cmp x3, #0");                              // did the cut remove more than is available?
                emitter.instruction("csel x2, xzr, x3, lt");                    // result length = max(0, remaining + length)
                emitter.label(&len_done);
            }
        }
        Arch::X86_64 => {
            // -- save string and evaluate offset --
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // push string ptr and length onto the temporary stack
            super::args::push_int_arg(&args[1], emitter, ctx, data);
            if has_length {
                super::args::emit_int_arg(&args[2], emitter, ctx, data);
                emitter.instruction("mov rcx, rax");                            // move the optional length argument into the scratch register
            }
            // -- restore offset and string from stack --
            abi::emit_pop_reg(emitter, "rax");                                  // pop the substring offset into the primary integer result register
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // pop the source string pointer and length into scratch registers
            // -- handle negative offset --
            emitter.instruction("cmp rax, 0");                                  // check whether the requested offset is negative
            emitter.instruction(&format!("jge {}", neg_done));                  // skip the negative-offset fixup when the offset is non-negative
            emitter.instruction("add rax, rsi");                                // convert the negative offset into a tail-relative byte index
            emitter.instruction("cmp rax, 0");                                  // check whether the adjusted offset still underflowed past the start
            emitter.instruction("mov r8, 0");                                   // materialize zero for the negative-offset clamp
            emitter.instruction("cmovl rax, r8");                               // clamp the adjusted offset back to zero when it points before the start
            emitter.label(&neg_done);
            // -- clamp offset to string length --
            emitter.instruction("cmp rax, rsi");                                // compare the requested offset against the full source-string length
            emitter.instruction("cmovg rax, rsi");                              // clamp the offset to the full string length when it points past the end
            // -- adjust pointer and compute remaining length --
            emitter.instruction("add rdi, rax");                                // advance the source-string pointer by the final byte offset
            emitter.instruction("sub rsi, rax");                                // compute the remaining substring length after the final byte offset
            // -- apply the optional length argument when one was passed --
            if has_length {
                let len_neg = ctx.next_label("substr_len_neg");
                let len_done = ctx.next_label("substr_len_done");
                emitter.instruction("cmp rcx, 0");                              // check whether the requested length is negative
                emitter.instruction(&format!("jl {}", len_neg));                // a negative length drops characters from the end
                // -- non-negative length: keep min(length, remaining) --
                emitter.instruction("cmp rcx, rsi");                            // compare the requested length against the remaining tail length
                emitter.instruction("cmovl rsi, rcx");                          // shrink the tail when the explicit length is shorter
                emitter.instruction(&format!("jmp {}", len_done));              // skip the negative-length handling
                emitter.label(&len_neg);
                // -- negative length: keep max(0, remaining + length) --
                emitter.instruction("add rcx, rsi");                            // remaining + length (negative) = chars before the cut
                emitter.instruction("mov r8, 0");                               // materialize zero for the underflow clamp
                emitter.instruction("cmp rcx, 0");                              // did the cut remove more than is available?
                emitter.instruction("cmovl rcx, r8");                           // clamp to zero when remaining + length underflowed
                emitter.instruction("mov rsi, rcx");                            // result length = max(0, remaining + length)
                emitter.label(&len_done);
            }
            emitter.instruction("mov rax, rdi");                                // return the borrowed substring pointer in the primary result register
            emitter.instruction("mov rdx, rsi");                                // return the borrowed substring length in the secondary result register
        }
    }

    Some(PhpType::Str)
}
