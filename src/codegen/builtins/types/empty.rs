use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

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
        PhpType::Int => {
            // -- int is empty if it equals zero --
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
                    emitter.instruction("sete al");                             // materialize the x86_64 floating-point comparison result in the low byte when the value is 0.0
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
        PhpType::Void => {
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
