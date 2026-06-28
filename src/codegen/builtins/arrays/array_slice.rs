//! Purpose:
//! Emits PHP `array_slice` builtin calls that allocate or reshape array values.
//! Coordinates element type selection with runtime helpers that build indexed or associative arrays.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Returned arrays must use the payload layout expected by later codegen and GC/refcount paths.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `array_slice($array, $offset, $length)` builtin call.
///
/// Evaluates arguments in source order, materializes them into ABI register order,
/// and calls `__rt_array_slice` (scalar) or `__rt_array_slice_refcounted` (refcounted
/// elements) depending on the source array's element type. On x86_64 uses register-
/// based argument passing; on ARM64 uses stack-based argument passing with x0–x2.
/// A missing `$length` is signaled by passing -1 to request "until end of array".
///
/// # Arguments
/// * `args[0]` — source array expression
/// * `args[1]` — byte offset into the array
/// * `args[2]` — optional slice length; absent means rest of array
///
/// # Returns
/// `PhpType::Array` preserving the inner element type from the source array, or
/// `PhpType::Array(Int)` when the source type is non-array (treated as integer-indexed).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_slice()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the source indexed-array pointer while evaluating the slice offset
        let offset_ty = emit_expr(&args[1], emitter, ctx, data);
        coerce_to_int(emitter, &offset_ty);                                     // unbox a Mixed/Union slice offset into a raw integer
        if args.len() > 2 {
            abi::emit_push_reg(emitter, "rax");                                 // preserve the requested slice offset while evaluating the slice length
            let length_ty = emit_expr(&args[2], emitter, ctx, data);
            coerce_to_int(emitter, &length_ty);                                 // unbox a Mixed/Union slice length into a raw integer
            emitter.instruction("mov rdx, rax");                                // move the requested slice length into the third x86_64 runtime argument register
            abi::emit_pop_reg(emitter, "rsi");                                  // restore the requested slice offset into the second x86_64 runtime argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the source indexed-array pointer into the first x86_64 runtime argument register
        } else {
            emitter.instruction("mov rsi, rax");                                // move the requested slice offset into the second x86_64 runtime argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the source indexed-array pointer into the first x86_64 runtime argument register
            emitter.instruction("mov rdx, -1");                                 // use -1 as the x86_64 runtime sentinel for slicing until the end of the source array
        }
        if uses_refcounted_runtime {
            abi::emit_call_label(emitter, "__rt_array_slice_refcounted");       // extract the refcounted indexed-array slice through the x86_64 runtime helper
        } else {
            abi::emit_call_label(emitter, "__rt_array_slice");                  // extract the scalar indexed-array slice through the x86_64 runtime helper
        }

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(inner)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    // -- save array pointer, evaluate offset --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    let offset_ty = emit_expr(&args[1], emitter, ctx, data);
    coerce_to_int(emitter, &offset_ty);                                         // unbox a Mixed/Union slice offset into a raw integer
    if args.len() > 2 {
        // -- save offset, evaluate length --
        emitter.instruction("str x0, [sp, #-16]!");                             // push offset onto stack
        let length_ty = emit_expr(&args[2], emitter, ctx, data);
        coerce_to_int(emitter, &length_ty);                                     // unbox a Mixed/Union slice length into a raw integer
        // -- set up three-arg call: array, offset, length --
        emitter.instruction("mov x2, x0");                                      // move length to x2 (third arg)
        emitter.instruction("ldr x1, [sp], #16");                               // pop offset into x1 (second arg)
        emitter.instruction("ldr x0, [sp], #16");                               // pop array pointer into x0 (first arg)
    } else {
        // -- set up two-arg call: array, offset (length = rest of array) --
        emitter.instruction("mov x1, x0");                                      // move offset to x1 (second arg)
        emitter.instruction("ldr x0, [sp], #16");                               // pop array pointer into x0 (first arg)
        emitter.instruction("mov x2, #-1");                                     // length = -1 signals "until end of array"
    }
    // -- call runtime to extract slice --
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_slice_refcounted"
    } else {
        "bl __rt_array_slice"
    };
    emitter.instruction(runtime_call);                                          // call runtime: slice array → x0=new array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
