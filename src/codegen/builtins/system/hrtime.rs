//! Purpose:
//! Lowers the PHP `hrtime()` builtin: evaluates the optional as-number flag and calls the
//! `__rt_hrtime` runtime helper, which returns the already-boxed Mixed result.
//!
//! Called from:
//! - `crate::codegen::builtins::system` dispatch for the `hrtime` builtin.
//!
//! Key details:
//! - The as-number flag defaults to `0` (return a `[sec, nsec]` array). `__rt_hrtime` boxes its own
//!   result (a Mixed int or a Mixed assoc array), so the emitter just forwards it.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits `hrtime([$as_number])`: materializes the flag (defaulting to `0`) in the integer argument
/// register and calls `__rt_hrtime`, returning the `PhpType::Mixed` result it boxes.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hrtime()");
    if args.is_empty() {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #0");                              // as_number defaults to false (return the array)
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, 0");                              // as_number defaults to false (return the array)
            }
        }
    } else {
        let arg_ty = emit_expr(&args[0], emitter, ctx, data);
        coerce_to_int(emitter, &arg_ty);                                        // coerce the bool/int as-number flag to an integer
    }
    abi::emit_call_label(emitter, "__rt_hrtime");                               // read the monotonic clock and return the boxed Mixed result
    Some(PhpType::Mixed)
}
