use super::super::abi;
use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::super::expr::{coerce_to_string, emit_expr};
use super::super::platform::Arch;
use super::PhpType;
use crate::parser::ast::Expr;

pub(super) fn emit_echo_stmt(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("echo");
    let ty = emit_expr(expr, emitter, ctx, data);
    match &ty {
        PhpType::Void => {}
        PhpType::Bool => {
            let skip_label = ctx.next_label("echo_skip_false");
            abi::emit_branch_if_int_result_zero(emitter, &skip_label);
            abi::emit_write_stdout(emitter, &ty);
            emitter.label(&skip_label);
        }
        PhpType::Int => {
            let skip_label = ctx.next_label("echo_skip_null");
            let sentinel_reg = abi::symbol_scratch_reg(emitter);
            abi::emit_load_int_immediate(emitter, sentinel_reg, 0x7fff_ffff_ffff_fffe);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(emitter), sentinel_reg)); // compare integer value against the runtime null sentinel
                    emitter.instruction(&format!("b.eq {}", skip_label));           // skip echo if value is the null sentinel
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(emitter), sentinel_reg)); // compare integer value against the runtime null sentinel
                    emitter.instruction(&format!("je {}", skip_label));             // skip echo if value is the null sentinel
                }
            }
            abi::emit_write_stdout(emitter, &ty);
            emitter.label(&skip_label);
        }
        PhpType::Float => {
            abi::emit_write_stdout(emitter, &ty);
        }
        PhpType::Object(_) => {
            coerce_to_string(emitter, ctx, data, &ty);
            abi::emit_write_stdout(emitter, &PhpType::Str);
        }
        _ => {
            abi::emit_write_stdout(emitter, &ty);
        }
    }
}
