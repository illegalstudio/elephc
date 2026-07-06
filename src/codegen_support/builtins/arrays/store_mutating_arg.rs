//! Purpose:
//! Stores a possibly replaced array pointer back into the original mutating argument storage.
//! Handles variable and addressable array arguments after COW or growth routines run.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::*::emit() for mutating array builtins`.
//!
//! Key details:
//! - Must match call-argument by-ref semantics so PHP-visible mutations update the caller slot.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};

/// Stores a possibly replaced array pointer back into the original mutating argument slot.
///
/// After COW or growth routines produce a new container pointer in `x0`/`rax`, this function
/// writes that pointer back to the caller's slot based on argument kind:
///
/// - **Global variable**: stores via the global symbol address computed from `_gvar_<name>`
/// - **By-ref parameter**: loads the reference pointer from the stack slot, then stores through it
/// - **Local variable**: stores directly into the stack frame at the variable's offset
///
/// The caller is responsible for placing the updated pointer in the appropriate register
/// before calling this function (ARM64: `x0`, x86_64: `rax`).
pub(crate) fn emit_store_mutating_arg(emitter: &mut Emitter, ctx: &Context, arg: &Expr) {
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };

    if ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name)) {
        let label = format!("_gvar_{}", name);
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_symbol_address(emitter, "x9", &label);                // resolve the global variable storage address for the mutated array/hash
                emitter.instruction("str x0, [x9]");                            // overwrite the global slot with the updated container pointer
            }
            Arch::X86_64 => {
                abi::emit_store_reg_to_symbol(emitter, "rax", &label, 0);        // overwrite the global slot with the updated container pointer through the x86_64 symbol helper
            }
        }
    } else if ctx.ref_params.contains(name) {
        let offset = ctx
            .variables
            .get(name)
            .expect("codegen bug: missing ref-param slot for mutating array builtin")
            .stack_offset;
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x9", offset);                      // load the by-reference slot that points at the mutating argument storage
                emitter.instruction("str x0, [x9]");                            // overwrite the referenced slot with the updated container pointer
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r11", offset);                     // load the by-reference slot that points at the mutating argument storage
                abi::emit_store_to_address(emitter, "rax", "r11", 0);            // overwrite the referenced slot with the updated container pointer
            }
        }
    } else if let Some(var) = ctx.variables.get(name) {
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::store_at_offset(emitter, "x0", var.stack_offset);            // store the updated container pointer in the local variable slot
            }
            Arch::X86_64 => {
                abi::store_at_offset(emitter, "rax", var.stack_offset);           // store the updated container pointer in the local variable slot
            }
        }
    }
}
