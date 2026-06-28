//! Purpose:
//! Lowers the PHP `getdate()` builtin: evaluates the optional timestamp argument (defaulting to the
//! current time) and calls the `__rt_getdate` runtime helper, which returns the associative array.
//!
//! Called from:
//! - `crate::codegen::builtins::system` dispatch for the `getdate` builtin.
//!
//! Key details:
//! - With no argument, passes the `-1` sentinel so the runtime substitutes the current time (matching
//!   the `date()`/`__rt_date` convention). The result is the Mixed assoc-array hash pointer.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits `getdate([$timestamp])`: materializes the timestamp (or `-1` for the current time) in the
/// integer result register, then calls `__rt_getdate`, yielding the `PhpType::Mixed` assoc array.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("getdate()");
    if args.is_empty() {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #-1");                             // -1 sentinel → runtime uses the current time
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, -1");                             // -1 sentinel → runtime uses the current time
            }
        }
    } else {
        let arg_ty = emit_expr(&args[0], emitter, ctx, data);
        coerce_to_int(emitter, &arg_ty);                                        // unbox a Mixed/Union timestamp into a raw integer
    }
    abi::emit_call_label(emitter, "__rt_getdate");                              // build the getdate associative array → hash pointer
    // Box the raw hash pointer into a Mixed cell (runtime tag 5 = assoc array), like stat().
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // x1 = hash pointer (low payload word)
            emitter.instruction("mov x2, #0");                                  // x2 = high payload word (unused)
            emitter.instruction("mov x0, #5");                                  // x0 = runtime tag 5 (assoc array)
            emitter.instruction("bl __rt_mixed_from_value");                    // → x0 = boxed mixed cell
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // rdi = hash pointer (low payload word)
            emitter.instruction("mov rsi, 0");                                  // rsi = high payload word (unused)
            emitter.instruction("mov rax, 5");                                  // rax = runtime tag 5 (assoc array)
            emitter.instruction("call __rt_mixed_from_value");                  // → rax = boxed mixed cell
        }
    }
    Some(PhpType::Mixed)
}
