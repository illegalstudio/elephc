//! Purpose:
//! Emits PHP `array_unshift` builtin calls that mutate array arguments in place.
//! Handles COW preparation and writes any replacement array pointer back to caller storage.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::emit()`.
//!
//! Key details:
//! - Mutating/ref-like arguments must avoid value-temp preevaluation so PHP-visible storage is updated.

use super::ensure_unique_arg::emit_ensure_unique_arg;
use super::store_mutating_arg::emit_store_mutating_arg;
use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `array_unshift` builtin call, which prepends a value to an array in place.
///
/// # Arguments
/// - `_name`: Unused; the builtin name is implicit.
/// - `args[0]`: The array to modify (mutating/ref-like).
/// - `args[1]`: The value to prepend.
///
/// # Returns
/// Always `PhpType::Int` (the new array length), matching PHP's return value.
///
/// # Codegen strategy
/// 1. Ensures the array argument is uniquely owned (COW).
/// 2. Stores the array pointer back to caller storage.
/// 3. Evaluates the prepend value while preserving the array pointer.
/// 4. Calls `__rt_array_unshift` with array pointer (x0/di) and value (x1/si) registers.
/// 5. The runtime returns the new array length in x0/di.
///
/// # ABI notes
/// - x86_64: pushes `rax` to preserve the unique array pointer while evaluating the payload,
///   then moves array pointer to `rdi` and payload to `rsi` before the call.
/// - ARM64: pushes array pointer to the stack, evaluates the payload into x0,
///   then swaps to x0=array, x1=payload via stack load.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_unshift()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emit_ensure_unique_arg(emitter, &arr_ty);
        emit_store_mutating_arg(emitter, ctx, &args[0]);
        abi::emit_push_reg(emitter, "rax");                                     // preserve the unique indexed-array pointer while evaluating the prepended scalar payload
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov rsi, rax");                                    // move the prepended scalar payload into the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the unique indexed-array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_array_unshift");                    // prepend the scalar payload through the x86_64 runtime helper and return the new length
        return Some(PhpType::Int);
    }

    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    // -- save array pointer, evaluate value to prepend --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to prepend value to array --
    emitter.instruction("mov x1, x0");                                          // move value to x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop array pointer into x0 (first arg)
    emitter.instruction("bl __rt_array_unshift");                               // call runtime: prepend value → x0=new count

    Some(PhpType::Int)
}
