//! Purpose:
//! Lowers the PHP `localtime()` builtin: evaluates the optional timestamp and associative-keys flag,
//! calls the `__rt_localtime` runtime helper, and boxes the resulting array into a Mixed cell.
//!
//! Called from:
//! - `crate::codegen::builtins::system` dispatch for the `localtime` builtin.
//!
//! Key details:
//! - The timestamp defaults to the current time (passed as the `-1` sentinel) and the associative
//!   flag defaults to `0`. The arguments are materialized in the runtime's input registers
//!   (timestamp first, flag second), then the raw hash pointer is boxed into a Mixed assoc array.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits `localtime([$timestamp [, $associative]])`: materializes the timestamp (or `-1`) and the
/// associative-keys flag (or `0`), calls `__rt_localtime`, and boxes the hash pointer into a Mixed
/// assoc array (runtime tag 5), like `getdate`/`stat`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("localtime()");
    match emitter.target.arch {
        Arch::AArch64 => {
            if args.len() >= 2 {
                let flag_ty = emit_expr(&args[1], emitter, ctx, data);
                coerce_to_int(emitter, &flag_ty);                               // associative flag → integer
                emitter.instruction("str x0, [sp, #-16]!");                     // push the flag while the timestamp is evaluated
                let ts_ty = emit_expr(&args[0], emitter, ctx, data);
                coerce_to_int(emitter, &ts_ty);                                 // timestamp → integer in x0
                emitter.instruction("ldr x1, [sp], #16");                       // restore the flag into the second argument register
            } else if args.len() == 1 {
                let ts_ty = emit_expr(&args[0], emitter, ctx, data);
                coerce_to_int(emitter, &ts_ty);                                 // timestamp → integer in x0
                emitter.instruction("mov x1, #0");                              // associative flag defaults to 0 (numeric keys)
            } else {
                emitter.instruction("mov x0, #-1");                             // -1 sentinel → runtime uses the current time
                emitter.instruction("mov x1, #0");                              // associative flag defaults to 0 (numeric keys)
            }
        }
        Arch::X86_64 => {
            if args.len() >= 2 {
                let flag_ty = emit_expr(&args[1], emitter, ctx, data);
                coerce_to_int(emitter, &flag_ty);                               // associative flag → integer
                abi::emit_push_reg(emitter, "rax");                             // push the flag while the timestamp is evaluated
                let ts_ty = emit_expr(&args[0], emitter, ctx, data);
                coerce_to_int(emitter, &ts_ty);                                 // timestamp → integer in rax
                abi::emit_pop_reg(emitter, "rsi");                              // restore the flag into the second argument register
            } else if args.len() == 1 {
                let ts_ty = emit_expr(&args[0], emitter, ctx, data);
                coerce_to_int(emitter, &ts_ty);                                 // timestamp → integer in rax
                emitter.instruction("mov rsi, 0");                              // associative flag defaults to 0 (numeric keys)
            } else {
                emitter.instruction("mov rax, -1");                             // -1 sentinel → runtime uses the current time
                emitter.instruction("mov rsi, 0");                              // associative flag defaults to 0 (numeric keys)
            }
        }
    }
    abi::emit_call_label(emitter, "__rt_localtime");                            // build the localtime array → hash pointer
    // Box the raw hash pointer into a Mixed cell (runtime tag 5 = assoc array), like getdate().
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
