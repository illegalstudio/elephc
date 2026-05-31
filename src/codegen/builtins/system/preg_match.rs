//! Purpose:
//! Emits PHP `preg_match` PCRE-style regex builtin calls.
//! Connects pattern/subject arguments and optional match arrays to runtime regex helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Match arrays and false/error results must use PHP-compatible Mixed array payloads.

use crate::codegen::abi;
use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits a `preg_match` call against a PCRE pattern.
///
/// `args[0]` is the pattern (string), `args[1]` is the subject (string).
/// Calls `__rt_preg_match` which returns 1 in the result register on match, 0 otherwise.
///
/// AArch64: pattern ptr/len in x0/x1, subject ptr/len pushed then popped into x3/x4, result in x0.
/// X86_64: pattern ptr/len in rdi/rsi (SysV), subject ptr/len pushed then popped into rdx/rcx, result in rax.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("preg_match()");

    match emitter.target.arch {
        Arch::AArch64 => {
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push subject ptr and len
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("ldp x3, x4, [sp], #16");                       // pop subject ptr/len into x3/x4
            if let Some(matches_arg) = args.get(2) {
                emitter.instruction("bl __rt_preg_match_capture");              // regex match → x0=match flag, x1=matches array
                emit_store_matches_arg(emitter, ctx, matches_arg);
            } else {
                emitter.instruction("bl __rt_preg_match");                      // regex match → x0=1 if matched, 0 if not
            }
        }
        Arch::X86_64 => {
            emit_expr(&args[1], emitter, ctx, data);
            crate::codegen::abi::emit_push_reg_pair(emitter, "rax", "rdx");     // push subject ptr and len
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // pass the pattern pointer in the first SysV integer argument register
            emitter.instruction("mov rsi, rdx");                                // pass the pattern length in the second SysV integer argument register
            crate::codegen::abi::emit_pop_reg_pair(emitter, "rdx", "rcx");      // pop subject ptr/len into the remaining SysV argument registers
            if let Some(matches_arg) = args.get(2) {
                crate::codegen::abi::emit_call_label(emitter, "__rt_preg_match_capture"); // regex match → rax=match flag, rdx=matches array
                emit_store_matches_arg(emitter, ctx, matches_arg);
            } else {
                crate::codegen::abi::emit_call_label(emitter, "__rt_preg_match"); // regex match → rax=1 if matched, 0 if not
            }
        }
    }

    Some(PhpType::Int)
}

/// Stores the runtime-built `$matches` array back into the by-reference argument.
fn emit_store_matches_arg(emitter: &mut Emitter, ctx: &mut Context, arg: &Expr) {
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };
    match emitter.target.arch {
        Arch::AArch64 => {
            if ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name)) {
                let label = format!("_gvar_{}", name);
                emitter.adrp("x9", &label);                                    // load page of the global preg_match matches slot
                emitter.add_lo12("x9", "x9", &label);                          // resolve the global preg_match matches slot address
                emitter.instruction("str x1, [x9]");                            // store the matches array into the global variable
            } else if ctx.ref_params.contains(name) {
                let offset = ctx
                    .variables
                    .get(name)
                    .expect("codegen bug: missing preg_match matches ref slot")
                    .stack_offset;
                abi::load_at_offset(emitter, "x9", offset);                     // load the by-reference matches variable storage pointer
                emitter.instruction("str x1, [x9]");                            // store the matches array through the referenced storage slot
            } else if let Some(var) = ctx.variables.get(name) {
                abi::store_at_offset(emitter, "x1", var.stack_offset);          // store the matches array into the local variable slot
            }
        }
        Arch::X86_64 => {
            if ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name)) {
                let label = format!("_gvar_{}", name);
                abi::emit_store_reg_to_symbol(emitter, "rdx", &label, 0);       // store the matches array into the global variable
            } else if ctx.ref_params.contains(name) {
                let offset = ctx
                    .variables
                    .get(name)
                    .expect("codegen bug: missing preg_match matches ref slot")
                    .stack_offset;
                abi::load_at_offset(emitter, "r11", offset);                    // load the by-reference matches variable storage pointer
                abi::emit_store_to_address(emitter, "rdx", "r11", 0);           // store the matches array through the referenced storage slot
            } else if let Some(var) = ctx.variables.get(name) {
                abi::store_at_offset(emitter, "rdx", var.stack_offset);         // store the matches array into the local variable slot
            }
        }
    }
    let matches_ty = PhpType::Array(Box::new(PhpType::Str));
    ctx.update_var_type_static_and_ownership(
        name,
        matches_ty.clone(),
        matches_ty,
        HeapOwnership::Owned,
    );
}
