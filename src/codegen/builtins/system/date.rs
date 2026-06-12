//! Purpose:
//! Emits PHP `date` / `gmdate` time/date builtin calls.
//! Marshals timestamp and format arguments into runtime helpers that consult wall-clock state.
//! `gmdate` reuses the same marshalling and only targets the UTC runtime entry `__rt_gmdate`.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Time calls are effectful/non-deterministic and must preserve PHP scalar return conventions.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `date(format[, timestamp])` and `gmdate(format[, timestamp])` builtins.
///
/// Compiles to a call into `__rt_date` (local time) or `__rt_gmdate` (UTC) with the format
/// string pointer/length in the first string-argument registers and the timestamp in the
/// first integer register. When the timestamp argument is omitted, `-1` is passed to signal
/// the runtime to use the current wall-clock time. The runtime entry is chosen from `name`,
/// so both builtins share identical argument marshalling.
///
/// # Arguments
/// - `name`: builtin name (`"date"` or `"gmdate"`), selects the runtime entry
/// - `args`: first arg is the format string, optional second arg is the Unix timestamp
/// - `emitter`: target-aware instruction emitter
/// - `ctx`: current codegen context (used by `emit_expr`)
/// - `data`: data section for relocatable strings/labels
///
/// # Returns
/// `Some(PhpType::Str)` since `date()`/`gmdate()` always return a string.
///
/// # Architecture behavior
/// - **AArch64**: format ptr/length in x1/x2, timestamp in x0, result in x0
/// - **x86_64**: format ptr/length in rdi/rsi, timestamp in rax, result in rax
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let is_gmt = name == "gmdate";
    let runtime_label = if is_gmt { "__rt_gmdate" } else { "__rt_date" };
    emitter.comment(if is_gmt { "gmdate()" } else { "date()" });

    match emitter.target.arch {
        Arch::AArch64 => {
            if args.len() == 2 {
                // -- evaluate timestamp argument first --
                let ts_ty = emit_expr(&args[1], emitter, ctx, data);
                coerce_to_int(emitter, &ts_ty);                                 // unbox a Mixed/Union timestamp into a raw integer
                emitter.instruction("str x0, [sp, #-16]!");                     // push timestamp onto stack

                // -- evaluate format string --
                emit_expr(&args[0], emitter, ctx, data);
                // x1=format ptr, x2=format len

                // -- pop timestamp into x0 --
                emitter.instruction("ldr x0, [sp], #16");                       // pop timestamp from stack
            } else {
                // -- evaluate format string --
                emit_expr(&args[0], emitter, ctx, data);
                // x1=format ptr, x2=format len

                // -- use -1 to signal "use current time" --
                emitter.instruction("mov x0, #-1");                             // timestamp -1 = use current time
            }
        }
        Arch::X86_64 => {
            if args.len() == 2 {
                // -- evaluate timestamp argument first --
                let ts_ty = emit_expr(&args[1], emitter, ctx, data);
                coerce_to_int(emitter, &ts_ty);                                 // unbox a Mixed/Union timestamp into a raw integer
                abi::emit_push_reg(emitter, "rax");                             // save the timestamp while the format-string expression is evaluated

                // -- evaluate format string --
                emit_expr(&args[0], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the format-string pointer into the first x86_64 string-argument register
                emitter.instruction("mov rsi, rdx");                            // move the format-string length into the paired x86_64 string-argument register
                abi::emit_pop_reg(emitter, "rax");                              // restore the timestamp into the x86_64 integer result register
            } else {
                // -- evaluate format string --
                emit_expr(&args[0], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the format-string pointer into the first x86_64 string-argument register
                emitter.instruction("mov rsi, rdx");                            // move the format-string length into the paired x86_64 string-argument register
                emitter.instruction("mov rax, -1");                             // timestamp -1 = use current time
            }
        }
    }

    // -- call runtime: aarch64 x0/x1/x2, x86_64 rax/rdi/rsi --
    abi::emit_call_label(emitter, runtime_label);                               // format the timestamp through the local (date) or UTC (gmdate) runtime helper

    Some(PhpType::Str)
}
