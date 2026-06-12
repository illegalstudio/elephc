//! Purpose:
//! Lowers the PHP `checkdate()` builtin: evaluates the month/day/year arguments and calls the
//! `__rt_checkdate` runtime helper, returning a PHP boolean.
//!
//! Called from:
//! - `crate::codegen::builtins::system` dispatch for the `checkdate` builtin.
//!
//! Key details:
//! - Mirrors `mktime`'s argument marshalling (evaluate, coerce to int, materialize in ABI order),
//!   then delegates the range/leap-year validation to `__rt_checkdate`.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits `checkdate($month, $day, $year)`: evaluates the three integer arguments into the ABI
/// argument registers and calls `__rt_checkdate`, yielding a `PhpType::Bool` (1 valid / 0 invalid).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("checkdate()");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- evaluate month, day, year; push reversed so they pop back in order --
            for i in (0..3).rev() {
                let arg_ty = emit_expr(&args[i], emitter, ctx, data);
                coerce_to_int(emitter, &arg_ty);                                // unbox a Mixed/Union argument into a raw integer before pushing it
                emitter.instruction("str x0, [sp, #-16]!");                     // push the evaluated integer argument onto the temporary stack
            }
            emitter.instruction("ldr x0, [sp], #16");                           // restore the month argument into the first integer argument register
            emitter.instruction("ldr x1, [sp], #16");                           // restore the day argument into the second integer argument register
            emitter.instruction("ldr x2, [sp], #16");                           // restore the year argument into the third integer argument register
        }
        Arch::X86_64 => {
            // -- evaluate month, day, year; push reversed so they pop back in order --
            for i in (0..3).rev() {
                let arg_ty = emit_expr(&args[i], emitter, ctx, data);
                coerce_to_int(emitter, &arg_ty);                                // unbox a Mixed/Union argument into a raw integer before pushing it
                abi::emit_push_reg(emitter, "rax");                             // push the evaluated integer argument onto the temporary x86_64 stack slot
            }
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the month argument into the first SysV integer argument register
            abi::emit_pop_reg(emitter, "rsi");                                  // restore the day argument into the second SysV integer argument register
            abi::emit_pop_reg(emitter, "rdx");                                  // restore the year argument into the third SysV integer argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_checkdate");                            // validate the Gregorian date and return the PHP boolean through the active target ABI
    Some(PhpType::Bool)
}
