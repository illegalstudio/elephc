//! Purpose:
//! Emits runtime diagnostic setup used by expression paths that can fail or throw.
//! Keeps line, file, and fatal-message preparation close to the lowering sites that need it.
//!
//! Called from:
//! - `crate::codegen::expr` and runtime error call sites
//!
//! Key details:
//! - Diagnostic state must be set before helper calls that can unwind or terminate execution.

use super::super::abi;
use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `@` error-control operator around an expression.
///
/// Pushes a suppression scope, evaluates the inner expression while preserving its
/// result across the pop call, then restores the result after leaving the scope.
/// Returns the type of the inner expression.
pub(super) fn emit_error_suppress(
    inner: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("@ error-control scope");
    abi::emit_call_label(emitter, "__rt_diag_push_suppression");                // enter a runtime diagnostic-suppression scope before evaluating the operand
    let ty = emit_expr(inner, emitter, ctx, data);
    preserve_result(emitter, &ty);
    abi::emit_call_label(emitter, "__rt_diag_pop_suppression");                 // leave the diagnostic-suppression scope after the operand result is saved
    restore_result(emitter, &ty);
    ty
}

/// Pushes the current expression result onto the stack to preserve it across a call.
fn preserve_result(emitter: &mut Emitter, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);
        }
        _ => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
        }
    }
}

/// Pops the preserved expression result from the stack back into the appropriate result register.
fn restore_result(emitter: &mut Emitter, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
        }
        _ => {
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        }
    }
}
