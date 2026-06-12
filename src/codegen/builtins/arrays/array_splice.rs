//! Purpose:
//! Emits PHP `array_splice` builtin calls that mutate array arguments in place.
//! Handles COW preparation and writes any replacement array pointer back to caller storage.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Mutating/ref-like arguments must avoid value-temp preevaluation so PHP-visible storage is updated.

use super::ensure_unique_arg::emit_ensure_unique_arg;
use super::store_mutating_arg::emit_store_mutating_arg;
use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `array_splice($array, $offset, $length, $replacement)` builtin call.
///
/// Removes the portion of `$array` starting at `$offset` (negative = from end) and
/// optionally replaces it with `$replacement`. The array is mutated in place (ref-like
/// COW semantics). Returns the removed elements as a new indexed array.
///
/// ## COW semantics
/// - `emit_ensure_unique_arg` guarantees `$array` is uniquely held before mutation.
/// - `emit_store_mutating_arg` writes the mutated array pointer back to the caller's
///   storage slot so the caller sees the change.
///
/// ## Argument order
/// - Args are evaluated in source order; temporaries are saved on the stack so the
///   array pointer is preserved across offset/length/replacement evaluation.
/// - On x86_64: arguments arrive in `rdi`, `rsi`, `rdx`; on ARM64: `x0`, `x1`, `x2`.
/// - `-1` for `$length` signals "remove until end" (handled by the runtime helper).
///
/// ## Runtime helpers
/// - `__rt_array_splice` for non-refcounted (scalar) arrays.
/// - `__rt_array_splice_refcounted` for refcounted arrays (inner element type is refcounted).
///
/// ## Return type
/// Returns `PhpType::Array` wrapping the inner type of `$array` (preserves element type
/// for non-refcounted case; `PhpType::Int` fallback for the removed-elements return
/// when the original type is unknown, matching PHP's `array_splice` returning `array`).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_splice()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the unique indexed-array pointer while evaluating the splice offset
        let offset_ty = emit_expr(&args[1], emitter, ctx, data);
        coerce_to_int(emitter, &offset_ty);                                     // unbox a Mixed/Union splice offset into a raw integer
        if args.len() > 2 {
            abi::emit_push_reg(emitter, "rax");                                 // preserve the requested splice offset while evaluating the removal length
            let length_ty = emit_expr(&args[2], emitter, ctx, data);
            coerce_to_int(emitter, &length_ty);                                 // unbox a Mixed/Union removal length into a raw integer
            emitter.instruction("mov rdx, rax");                                // move the removal length into the third x86_64 runtime argument register
            abi::emit_pop_reg(emitter, "rsi");                                  // restore the splice offset into the second x86_64 runtime argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the unique indexed-array pointer into the first x86_64 runtime argument register
        } else {
            emitter.instruction("mov rsi, rax");                                // move the splice offset into the second x86_64 runtime argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the unique indexed-array pointer into the first x86_64 runtime argument register
            emitter.instruction("mov rdx, -1");                                 // use -1 as the x86_64 runtime sentinel for removing until the end of the source array
        }
        if uses_refcounted_runtime {
            abi::emit_call_label(emitter, "__rt_array_splice_refcounted");      // remove the requested refcounted indexed-array slice through the x86_64 runtime helper
        } else {
            abi::emit_call_label(emitter, "__rt_array_splice");                 // remove the requested scalar indexed-array slice through the x86_64 runtime helper
        }

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(inner)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    // -- save array pointer, evaluate offset --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    let offset_ty = emit_expr(&args[1], emitter, ctx, data);
    coerce_to_int(emitter, &offset_ty);                                         // unbox a Mixed/Union splice offset into a raw integer
    if args.len() > 2 {
        // -- save offset, evaluate length --
        emitter.instruction("str x0, [sp, #-16]!");                             // push offset onto stack
        let length_ty = emit_expr(&args[2], emitter, ctx, data);
        coerce_to_int(emitter, &length_ty);                                     // unbox a Mixed/Union removal length into a raw integer
        // -- set up three-arg call: array, offset, length --
        emitter.instruction("mov x2, x0");                                      // move length to x2 (third arg)
        emitter.instruction("ldr x1, [sp], #16");                               // pop offset into x1 (second arg)
        emitter.instruction("ldr x0, [sp], #16");                               // pop array pointer into x0 (first arg)
    } else {
        // -- set up two-arg call: array, offset (remove rest) --
        emitter.instruction("mov x1, x0");                                      // move offset to x1 (second arg)
        emitter.instruction("ldr x0, [sp], #16");                               // pop array pointer into x0 (first arg)
        emitter.instruction("mov x2, #-1");                                     // length = -1 signals "remove until end"
    }
    // -- call runtime to splice array --
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_splice_refcounted"
    } else {
        "bl __rt_array_splice"
    };
    emitter.instruction(runtime_call);                                          // call runtime: splice array → x0=removed elements array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
