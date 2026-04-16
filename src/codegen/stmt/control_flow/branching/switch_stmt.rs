use crate::codegen::context::{Context, LoopLabels};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::stmt::emit_stmt;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, Stmt};
use crate::types::PhpType;

pub(super) fn emit_switch_stmt(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let switch_end = ctx.next_label("switch_end");
    emitter.blank();
    emitter.comment("switch");

    let subj_ty = emit_expr(subject, emitter, ctx, data);
    match &subj_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                 // save the switch subject string in one temporary stack slot using the active target ABI
        }
        _ => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // save the switch subject scalar in one temporary stack slot using the active target ABI
        }
    }

    let mut body_labels = Vec::new();
    for (i, (values, _)) in cases.iter().enumerate() {
        let body_label = ctx.next_label(&format!("case_{}", i));
        for val in values {
            let val_ty = emit_expr(val, emitter, ctx, data);
            match &subj_ty {
                PhpType::Str => {
                    match emitter.target.arch {
                        Arch::AArch64 => {
                            emitter.instruction("mov x3, x1");                  // move the case-pattern string pointer into the AArch64 right-hand compare register
                            emitter.instruction("mov x4, x2");                  // move the case-pattern string length into the AArch64 right-hand compare register
                            emitter.instruction("ldp x1, x2, [sp]");            // reload the saved switch subject string into the AArch64 left-hand compare registers
                            abi::emit_call_label(emitter, "__rt_str_eq");       // compare the switch subject and case pattern strings through the shared runtime helper
                        }
                        Arch::X86_64 => {
                            emitter.instruction("mov rcx, rdx");                // move the case-pattern string length into the SysV fourth argument register expected by __rt_str_eq
                            emitter.instruction("mov rdx, rax");                // move the case-pattern string pointer into the SysV third argument register expected by __rt_str_eq
                            emitter.instruction("mov rdi, QWORD PTR [rsp]");    // reload the saved switch subject string pointer into the SysV first argument register
                            emitter.instruction("mov rsi, QWORD PTR [rsp + 8]"); // reload the saved switch subject string length into the SysV second argument register
                            abi::emit_call_label(emitter, "__rt_str_eq");       // compare the switch subject and case pattern strings through the shared runtime helper
                        }
                    }
                }
                _ => {
                    match emitter.target.arch {
                        Arch::AArch64 => {
                            emitter.instruction("ldr x9, [sp]");                // reload the saved switch subject scalar into an AArch64 scratch register
                            emitter.instruction("cmp x9, x0");                  // compare the saved switch subject scalar against the current case-pattern scalar
                            emitter.instruction("cset x0, eq");                 // materialize the scalar equality result in the canonical AArch64 integer result register
                        }
                        Arch::X86_64 => {
                            emitter.instruction("mov r10, QWORD PTR [rsp]");    // reload the saved switch subject scalar into an x86_64 scratch register
                            emitter.instruction("cmp r10, rax");                // compare the saved switch subject scalar against the current case-pattern scalar
                            emitter.instruction("sete al");                     // materialize the scalar equality result in the low x86_64 result byte
                            emitter.instruction("movzx eax, al");               // widen the x86_64 boolean byte back into the canonical integer result register
                        }
                    }
                }
            }
            abi::emit_branch_if_int_result_nonzero(emitter, &body_label);       // jump to the case body once the switch subject equals the current case pattern
            let _ = val_ty;
        }
        body_labels.push(body_label);
    }

    let default_label = ctx.next_label("switch_default");
    if default.is_some() {
        abi::emit_jump(emitter, &default_label);                                // jump to the default case after all explicit case patterns miss
    } else {
        abi::emit_jump(emitter, &switch_end);                                   // jump straight to the end after all explicit case patterns miss and no default exists
    }

    ctx.loop_stack.push(LoopLabels {
        continue_label: switch_end.clone(),
        break_label: switch_end.clone(),
        sp_adjust: 16,
    });
    for (i, (_, body)) in cases.iter().enumerate() {
        emitter.label(&body_labels[i]);
        for s in body {
            emit_stmt(s, emitter, ctx, data);
        }
    }

    if let Some(def_body) = default {
        emitter.label(&default_label);
        for s in def_body {
            emit_stmt(s, emitter, ctx, data);
        }
    }

    ctx.loop_stack.pop();
    emitter.label(&switch_end);
    abi::emit_release_temporary_stack(emitter, 16);                             // release the saved switch subject slot without disturbing the surrounding result registers
}
