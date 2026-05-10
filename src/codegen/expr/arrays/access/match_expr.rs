//! Purpose:
//! Lowers array reads used by match expressions with PHP comparison semantics.
//! Produces expression results while preserving container ownership and bounds/null behavior.
//!
//! Called from:
//! - `crate::codegen::expr::arrays::access`
//!
//! Key details:
//! - Element layout and boxed Mixed handling must stay aligned with array runtime helpers.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(crate) fn emit_match_expr(
    subject: &Expr,
    arms: &[(Vec<Expr>, Expr)],
    default: &Option<Box<Expr>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("match expression");
    let subj_ty = emit_expr(subject, emitter, ctx, data);
    match &subj_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                 // save the string subject in one temporary stack slot using the active target ABI
        }
        PhpType::Float => {
            abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));  // save the float subject in one temporary stack slot using the active target ABI
        }
        _ => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // save the scalar subject in one temporary stack slot using the active target ABI
        }
    }

    let end_label = ctx.next_label("match_end");
    let mut result_ty = PhpType::Void;

    for (patterns, result) in arms {
        let arm_label = ctx.next_label("match_arm");
        let next_arm = ctx.next_label("match_next");

        for (i, pattern) in patterns.iter().enumerate() {
            let pat_ty = emit_expr(pattern, emitter, ctx, data);
            match &subj_ty {
                PhpType::Str => match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("mov x3, x1");                      // move the pattern string pointer into the AArch64 right-hand compare register
                        emitter.instruction("mov x4, x2");                      // move the pattern string length into the AArch64 right-hand compare register
                        emitter.instruction("ldp x1, x2, [sp]");                // reload the saved subject string into the AArch64 left-hand compare registers
                        abi::emit_call_label(emitter, "__rt_str_eq");       // compare the subject and pattern strings through the shared runtime helper
                    }
                    Arch::X86_64 => {
                        emitter.instruction("mov rcx, rdx");                    // move the pattern string length into the SysV fourth argument register expected by __rt_str_eq
                        emitter.instruction("mov rdx, rax");                    // move the pattern string pointer into the SysV third argument register expected by __rt_str_eq
                        emitter.instruction("mov rdi, QWORD PTR [rsp]");        // reload the saved subject string pointer into the SysV first argument register
                        emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");    // reload the saved subject string length into the SysV second argument register
                        abi::emit_call_label(emitter, "__rt_str_eq");       // compare the subject and pattern strings through the shared runtime helper
                    }
                },
                PhpType::Float => match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("ldr d1, [sp]");                    // reload the saved subject float into the AArch64 scratch compare register
                        emitter.instruction("fcmp d1, d0");                     // compare the saved subject float against the current pattern float
                        emitter.instruction("cset x0, eq");                     // materialize the float equality result in the canonical AArch64 integer result register
                    }
                    Arch::X86_64 => {
                        emitter.instruction("movsd xmm1, QWORD PTR [rsp]");     // reload the saved subject float into the x86_64 scratch compare register
                        emitter.instruction("ucomisd xmm1, xmm0");              // compare the saved subject float against the current pattern float
                        emitter.instruction("sete al");                         // materialize the float equality result in the low x86_64 result byte
                        emitter.instruction("movzx eax, al");                   // widen the x86_64 boolean byte back into the canonical integer result register
                    }
                },
                _ => match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("ldr x9, [sp]");                    // reload the saved scalar subject into an AArch64 scratch register
                        emitter.instruction("cmp x9, x0");                      // compare the saved scalar subject against the current pattern scalar
                        emitter.instruction("cset x0, eq");                     // materialize the scalar equality result in the canonical AArch64 integer result register
                    }
                    Arch::X86_64 => {
                        emitter.instruction("mov r10, QWORD PTR [rsp]");        // reload the saved scalar subject into an x86_64 scratch register
                        emitter.instruction("cmp r10, rax");                    // compare the saved scalar subject against the current pattern scalar
                        emitter.instruction("sete al");                         // materialize the scalar equality result in the low x86_64 result byte
                        emitter.instruction("movzx eax, al");                   // widen the x86_64 boolean byte back into the canonical integer result register
                    }
                },
            }
            abi::emit_branch_if_int_result_nonzero(emitter, &arm_label);        // jump to the current match arm once the subject equals the current pattern
            if i == patterns.len() - 1 {
                abi::emit_jump(emitter, &next_arm);                             // continue with the next match arm when this arm's patterns all miss
            }
            let _ = pat_ty;
        }

        emitter.label(&arm_label);
        result_ty = emit_expr(result, emitter, ctx, data);
        abi::emit_jump(emitter, &end_label);                                    // skip the remaining match arms after evaluating the selected arm expression
        emitter.label(&next_arm);
    }

    if let Some(def) = default {
        result_ty = emit_expr(def, emitter, ctx, data);
    } else {
        abi::emit_call_label(emitter, "__rt_match_unhandled");                  // abort when no arm matched and the match expression has no default arm
    }

    emitter.label(&end_label);
    abi::emit_release_temporary_stack(emitter, 16);                             // release the saved subject slot without clobbering the match expression result registers
    result_ty
}
