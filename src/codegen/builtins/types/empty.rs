//! Purpose:
//! Emits PHP `empty` checks without reducing them to ordinary boolean casts.
//! Handles unset/null/zero/empty string and array cases according to PHP truthiness rules.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Must distinguish undefined storage probes from evaluated expressions where PHP suppresses notices.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for PHP's `empty(expr)` builtin.
///
/// Evaluates whether `args[0]` is "empty" according to PHP truthiness rules,
/// then writes a boolean result to the canonical integer result register.
/// Handles all PHP types: int/float/bool compare against zero, null returns true,
/// strings compare length, arrays inspect element count, objects/resources/callables
/// return false, pointers check for null, and Mixed delegates to `__rt_mixed_is_empty`.
///
/// # Arguments
/// * `name` - Unused; present to match the builtin emitter signature
/// * `args` - The expression to evaluate (exactly one)
/// * `emitter` - Target-aware instruction emitter
/// * `ctx` - Codegen context (labels, frame layout, types)
/// * `data` - Data section for relocations
///
/// # Returns
/// `Some(PhpType::Bool)` as `empty()` always produces a boolean.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("empty()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match &ty {
        PhpType::Int | PhpType::TaggedScalar => {
            // -- int is empty if it equals zero; a null tagged scalar narrows to zero --
            crate::codegen::expr::coerce_null_to_zero(emitter, &ty);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #0");                          // compare the integer value against zero using the native AArch64 integer result register
                    emitter.instruction("cset x0, eq");                         // normalize the AArch64 comparison result to 1 when the integer is zero and 0 otherwise
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 0");                          // compare the integer value against zero using the native x86_64 integer result register
                    emitter.instruction("sete al");                             // materialize the x86_64 comparison result in the low byte when the integer is zero
                    emitter.instruction("movzx eax, al");                       // widen the x86_64 boolean byte back into the canonical integer result register
                }
            }
        }
        PhpType::Float => {
            // -- float is empty if it equals 0.0 --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("fcmp d0, #0.0");                       // compare the float value against 0.0 using the native AArch64 floating-point compare instruction
                    emitter.instruction("cset x0, eq");                         // normalize the AArch64 floating-point comparison to 1 when the value is 0.0 and 0 otherwise
                }
                Arch::X86_64 => {
                    emitter.instruction("xorpd xmm1, xmm1");                    // materialize a canonical 0.0 comparison operand in a scratch SIMD register for the x86_64 compare
                    emitter.instruction("ucomisd xmm0, xmm1");                  // compare the float result against 0.0 using the native x86_64 scalar-double compare
                    emitter.instruction("sete al");                             // float is empty when the value equals 0.0
                    emitter.instruction("setnp cl");                            // NaN is truthy, so require an ordered compare
                    emitter.instruction("and al, cl");                          // empty(NAN) is false: equal-to-zero AND ordered
                    emitter.instruction("movzx eax, al");                       // widen the x86_64 boolean byte back into the canonical integer result register
                }
            }
        }
        PhpType::Bool => {
            // -- bool is empty if false (0) --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #0");                          // compare the boolean payload against false using the native AArch64 integer result register
                    emitter.instruction("cset x0, eq");                         // normalize the AArch64 comparison result to 1 when the boolean is false and 0 otherwise
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 0");                          // compare the boolean payload against false using the native x86_64 integer result register
                    emitter.instruction("sete al");                             // materialize the x86_64 comparison result in the low byte when the boolean is false
                    emitter.instruction("movzx eax, al");                       // widen the x86_64 boolean byte back into the canonical integer result register
                }
            }
        }
        PhpType::Void | PhpType::Never => {
            // -- null is always empty --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x0, #1");                          // null is always empty, so return true in the native AArch64 integer result register
                }
                Arch::X86_64 => {
                    emitter.instruction("mov eax, 1");                          // null is always empty, so return true in the native x86_64 integer result register
                }
            }
        }
        PhpType::Iterable => {
            // -- iterable values are raw heap pointers, so inspect the heap kind before applying empty() --
            let array_case = ctx.next_label("empty_iterable_array");
            let false_case = ctx.next_label("empty_iterable_false");
            let true_case = ctx.next_label("empty_iterable_true");
            let done = ctx.next_label("empty_iterable_done");
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // preserve the iterable pointer while checking its heap kind
                    emitter.instruction("bl __rt_heap_kind");                   // classify the raw iterable pointer by heap kind
                    emitter.instruction("cmp x0, #2");                          // is this iterable backed by an indexed array?
                    emitter.instruction(&format!("b.eq {}", array_case));       // indexed arrays are empty only when their length is zero
                    emitter.instruction("cmp x0, #3");                          // is this iterable backed by an associative array?
                    emitter.instruction(&format!("b.eq {}", array_case));       // associative arrays are empty only when their length is zero
                    emitter.instruction("cmp x0, #4");                          // is this iterable backed by an object?
                    emitter.instruction(&format!("b.eq {}", false_case));       // objects are never empty in PHP
                    emitter.instruction(&format!("b {}", true_case));           // null/unknown iterable payloads are treated as empty

                    emitter.label(&array_case);
                    emitter.instruction("ldr x9, [sp], #16");                   // restore the array/hash pointer from the temporary stack slot
                    emitter.instruction("ldr x0, [x9]");                        // load the container element count from the shared header layout
                    emitter.instruction("cmp x0, #0");                          // compare the iterable container length against zero
                    emitter.instruction("cset x0, eq");                         // return true only when the iterable container has no elements
                    emitter.instruction(&format!("b {}", done));                // finish after the array/hash empty result is materialized

                    emitter.label(&false_case);
                    emitter.instruction("add sp, sp, #16");                     // discard the preserved iterable pointer before returning false
                    emitter.instruction("mov x0, #0");                          // object-backed iterables are not empty
                    emitter.instruction(&format!("b {}", done));                // finish after the false result is materialized

                    emitter.label(&true_case);
                    emitter.instruction("add sp, sp, #16");                     // discard the preserved iterable pointer before returning true
                    emitter.instruction("mov x0, #1");                          // null or unknown iterable payloads are empty
                    emitter.label(&done);
                }
                Arch::X86_64 => {
                    abi::emit_push_reg(emitter, "rax");                         // preserve the iterable pointer while checking its heap kind
                    emitter.instruction("call __rt_heap_kind");                 // classify the raw iterable pointer by heap kind
                    emitter.instruction("cmp rax, 2");                          // is this iterable backed by an indexed array?
                    emitter.instruction(&format!("je {}", array_case));         // indexed arrays are empty only when their length is zero
                    emitter.instruction("cmp rax, 3");                          // is this iterable backed by an associative array?
                    emitter.instruction(&format!("je {}", array_case));         // associative arrays are empty only when their length is zero
                    emitter.instruction("cmp rax, 4");                          // is this iterable backed by an object?
                    emitter.instruction(&format!("je {}", false_case));         // objects are never empty in PHP
                    emitter.instruction(&format!("jmp {}", true_case));         // null/unknown iterable payloads are treated as empty

                    emitter.label(&array_case);
                    abi::emit_pop_reg(emitter, "r10");                          // restore the array/hash pointer from the temporary stack slot
                    emitter.instruction("mov rax, QWORD PTR [r10]");            // load the container element count from the shared header layout
                    emitter.instruction("cmp rax, 0");                          // compare the iterable container length against zero
                    emitter.instruction("sete al");                             // return true only when the iterable container has no elements
                    emitter.instruction("movzx rax, al");                       // widen the boolean byte into the canonical integer result
                    emitter.instruction(&format!("jmp {}", done));              // finish after the array/hash empty result is materialized

                    emitter.label(&false_case);
                    abi::emit_pop_reg(emitter, "r10");                          // discard the preserved iterable pointer before returning false
                    emitter.instruction("xor eax, eax");                        // object-backed iterables are not empty
                    emitter.instruction(&format!("jmp {}", done));              // finish after the false result is materialized

                    emitter.label(&true_case);
                    abi::emit_pop_reg(emitter, "r10");                          // discard the preserved iterable pointer before returning true
                    emitter.instruction("mov eax, 1");                          // null or unknown iterable payloads are empty
                    emitter.label(&done);
                }
            }
        }
        PhpType::Mixed | PhpType::Union(_) => {
            // -- mixed values use PHP empty() semantics for the boxed payload --
            abi::emit_call_label(emitter, "__rt_mixed_is_empty");               // inspect the boxed payload instead of the mixed box pointer through the target-aware runtime helper
        }
        PhpType::Str => {
            // -- string is empty if length is zero --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x2, #0");                          // compare the string length against zero using the native AArch64 string-length result register
                    emitter.instruction("cset x0, eq");                         // normalize the AArch64 comparison result to 1 when the string length is zero and 0 otherwise
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rdx, 0");                          // compare the string length against zero using the native x86_64 string-length result register
                    emitter.instruction("sete al");                             // materialize the x86_64 comparison result in the low byte when the string length is zero
                    emitter.instruction("movzx eax, al");                       // widen the x86_64 boolean byte back into the canonical integer result register
                }
            }
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // -- array is empty if element count is zero --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldr x0, [x0]");                        // load the container element count from the header into the AArch64 integer result register
                    emitter.instruction("cmp x0, #0");                          // compare the container element count against zero on AArch64
                    emitter.instruction("cset x0, eq");                         // normalize the AArch64 comparison result to 1 when the container is empty and 0 otherwise
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rax, QWORD PTR [rax]");            // load the container element count from the header into the x86_64 integer result register
                    emitter.instruction("cmp rax, 0");                          // compare the container element count against zero on x86_64
                    emitter.instruction("sete al");                             // materialize the x86_64 comparison result in the low byte when the container is empty
                    emitter.instruction("movzx eax, al");                       // widen the x86_64 boolean byte back into the canonical integer result register
                }
            }
        }
        PhpType::Callable | PhpType::Object(_) => {
            // -- callable/object is never empty --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x0, #0");                          // callable/object values are never empty, so return false in the native AArch64 integer result register
                }
                Arch::X86_64 => {
                    emitter.instruction("xor eax, eax");                        // callable/object values are never empty, so return false in the native x86_64 integer result register
                }
            }
        }
        PhpType::Resource(_) => {
            // -- resources are never empty in PHP --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x0, #0");                          // resource values are never empty, so return false in the native AArch64 integer result register
                }
                Arch::X86_64 => {
                    emitter.instruction("xor eax, eax");                        // resource values are never empty, so return false in the native x86_64 integer result register
                }
            }
        }
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            // -- pointer is empty only when it is the null pointer --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #0");                          // compare the pointer-like value against null using the native AArch64 integer result register
                    emitter.instruction("cset x0, eq");                         // normalize the AArch64 comparison result to 1 when the pointer-like value is null and 0 otherwise
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 0");                          // compare the pointer-like value against null using the native x86_64 integer result register
                    emitter.instruction("sete al");                             // materialize the x86_64 comparison result in the low byte when the pointer-like value is null
                    emitter.instruction("movzx eax, al");                       // widen the x86_64 boolean byte back into the canonical integer result register
                }
            }
        }
    }
    Some(PhpType::Bool)
}
