//! Purpose:
//! Lowers echo and output-oriented statements into target-specific write calls.
//! Coerces expression results to strings and emits stdout writes.
//!
//! Called from:
//! - `crate::codegen::stmt`
//!
//! Key details:
//! - String temporaries must stay alive through the write and be released only when this path owns them.

use super::super::abi;
use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::super::context::HeapOwnership;
use super::super::expr::{
    coerce_to_string_releasing_owned, emit_expr, expr_result_heap_ownership,
    string_result_is_owned_call_temp,
};
use super::super::platform::Arch;
use super::PhpType;
use crate::parser::ast::Expr;

/// Lowers a top-level `echo` statement into stdout writes.
/// Outputs a blank line and comment, then delegates to `emit_expr_to_stdout`.
pub(super) fn emit_echo_stmt(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("echo");
    emit_expr_to_stdout(expr, emitter, ctx, data);
}

/// Emits `expr` to stdout, coercing the expression result to a string first.
/// For `PhpType::Str` and `PhpType::Object`, the string is written and released
/// (the caller's ownership determines whether release is needed).
/// For integer results on x86_64, stabilizes the result through a spill/reload
/// pair before sentinel checks to avoid register corruption in caller-saved registers.
/// Other types are passed directly to `emit_write_stdout`. Exits early for zero/null
/// sentinels where PHP semantics suppress output.
pub(crate) fn emit_expr_to_stdout(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let ty = emit_expr(expr, emitter, ctx, data);
    stabilize_x86_64_echo_result(emitter, &ty);
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
                    emitter.instruction(&format!("b.eq {}", skip_label));       // skip echo if value is the null sentinel
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(emitter), sentinel_reg)); // compare integer value against the runtime null sentinel
                    emitter.instruction(&format!("je {}", skip_label));         // skip echo if value is the null sentinel
                }
            }
            abi::emit_write_stdout(emitter, &ty);
            emitter.label(&skip_label);
        }
        PhpType::Float => {
            abi::emit_write_stdout(emitter, &ty);
        }
        PhpType::Str => {
            emit_string_to_stdout_and_release_if_needed(
                emitter,
                string_result_is_owned_call_temp(expr, ctx),
            );
        }
        PhpType::Object(_) => {
            let release_object = expr_result_heap_ownership(expr) == HeapOwnership::Owned;
            coerce_to_string_releasing_owned(emitter, ctx, data, &ty, release_object);
            emit_string_to_stdout_and_release_if_needed(emitter, true);
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            // Arrays stringify to the literal "Array" (matching PHP). The array pointer is
            // preserved across the write so an owned temporary can be released afterward; a
            // borrowed array (e.g. a plain variable) is left for its owner to release.
            let release_array = expr_result_heap_ownership(expr) == HeapOwnership::Owned;
            if release_array {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
            }
            coerce_to_string_releasing_owned(emitter, ctx, data, &ty, false);
            abi::emit_write_stdout(emitter, &PhpType::Str);
            if release_array {
                abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 0);
                abi::emit_decref_if_refcounted(emitter, &ty);
                abi::emit_release_temporary_stack(emitter, 16);
            }
        }
        _ => {
            abi::emit_write_stdout(emitter, &ty);
        }
    }
}

/// Emits a string to stdout, releasing the temporary if `release_owned_temp` is true.
fn emit_string_to_stdout_and_release_if_needed(emitter: &mut Emitter, release_owned_temp: bool) {
    if !release_owned_temp {
        abi::emit_write_stdout(emitter, &PhpType::Str);
        return;
    }

    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve owned temporary string until the write syscall has consumed it
    abi::emit_write_stdout(emitter, &PhpType::Str);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_call_label(emitter, "__rt_heap_free_safe");                       // release heap-backed call result after echo copied it to stdout
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the saved string pointer/length pair
}

/// On x86_64 only, stabilizes the echo result through a temporary stack slot before
/// sentinel checks or helper calls that may consume caller-saved registers.
/// Handles both integer (x0) and float (d0) result registers; other types are left unchanged.
fn stabilize_x86_64_echo_result(emitter: &mut Emitter, ty: &PhpType) {
    if emitter.target.arch != Arch::X86_64 {
        return;
    }

    match ty.codegen_repr() {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // spill register-only x86_64 echo results through a temporary slot before sentinel checks or helper calls consume them
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));           // reload the stabilized x86_64 echo result back into the canonical integer result register
        }
        PhpType::Float => {
            abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));  // spill floating-point x86_64 echo results through a temporary slot before helper calls consume them
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));   // reload the stabilized x86_64 echo result back into the canonical floating-point result register
        }
        PhpType::Str | PhpType::Void | PhpType::Never => {}
    }
}
