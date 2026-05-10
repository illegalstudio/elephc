//! Purpose:
//! Lowers null coalescing expressions with short-circuit reads.
//! Keeps comparison-specific branching and register normalization out of generic expression code.
//!
//! Called from:
//! - `crate::codegen::expr::compare`
//!
//! Key details:
//! - Null, type-tag, and string comparisons must follow PHP semantics before emitting boolean results.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::{coerce_result_to_type, emit_expr, widen_codegen_type};

pub(in crate::codegen::expr) fn emit_null_coalesce(
    value: &Expr,
    default: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("null coalesce ??");
    let val_ty = emit_expr(value, emitter, ctx, data);

    if val_ty == PhpType::Void {
        return emit_expr(default, emitter, ctx, data);
    }

    let default_ty = crate::codegen::functions::infer_contextual_type(default, ctx);
    let result_ty = widen_codegen_type(&val_ty, &default_ty);

    let use_value_label = ctx.next_label("nc_keep");
    let end_label = ctx.next_label("nc_end");
    if matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // save the boxed mixed/union value across the null check and fallback evaluation
        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // inspect the boxed payload tag before deciding whether ?? should fall back
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction("cmp x0, #8");                              // runtime tag 8 = null
                emitter.instruction(&format!("b.ne {}", use_value_label));      // non-null mixed payload keeps the original boxed value
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction("cmp rax, 8");                              // runtime tag 8 = null
                emitter.instruction(&format!("jne {}", use_value_label));       // non-null mixed payload keeps the original boxed value
            }
        }
    } else {
        let null_reg = abi::symbol_scratch_reg(emitter);
        abi::emit_load_int_immediate(emitter, null_reg, 0x7fff_ffff_ffff_fffe_u64 as i64); // materialize the shared null sentinel for the direct null test
        if val_ty == PhpType::Float {
            match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    emitter.instruction("fmov x0, d0");                         // copy float bits into x0 for the null-sentinel check on AArch64
                }
                crate::codegen::platform::Arch::X86_64 => {
                    emitter.instruction("movq rax, xmm0");                      // copy float bits into rax for the null-sentinel check on x86_64
                }
            }
        }
        let cmp_reg = if val_ty == PhpType::Str { abi::string_result_regs(emitter).0 } else { abi::int_result_reg(emitter) };
        emitter.instruction(&format!("cmp {}, {}", cmp_reg, null_reg));         // compare value against the null sentinel
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("b.ne {}", use_value_label));      // if not null, skip default branch and keep value
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("jne {}", use_value_label));       // if not null, skip default branch and keep value
            }
        }
    }

    let default_runtime_ty = emit_expr(default, emitter, ctx, data);
    coerce_result_to_type(emitter, ctx, data, &default_runtime_ty, &result_ty);
    if matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the saved original boxed mixed/union value on the null fallback path
    }
    abi::emit_jump(emitter, &end_label);                                        // skip the non-null branch after evaluating the default expression
    emitter.label(&use_value_label);
    if matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));               // restore the original boxed mixed/union payload for the keep-left branch
    }
    coerce_result_to_type(emitter, ctx, data, &val_ty, &result_ty);
    emitter.label(&end_label);

    result_ty
}
