//! Purpose:
//! Emits PHP `array_chunk` builtin calls that allocate or reshape array values.
//! Coordinates element type selection with runtime helpers that build indexed or associative arrays.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Returned arrays must use the payload layout expected by later codegen and GC/refcount paths.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `array_chunk($array, $size, $preserve_keys)` builtin call.
///
/// Splits `$array` into chunks of `$size` elements, returning an array of arrays.
/// Calls `__rt_array_chunk` (scalar arrays) or `__rt_array_chunk_refcounted` (refcounted
/// arrays such as those containing strings or nested arrays) via the platform ABI.
///
/// ## Arguments
/// - `args[0]`: source array to chunk
/// - `args[1]`: chunk size (positive integer)
///
/// ## Return type
/// `PhpType::Array(Array(inner))` — an array of arrays preserving the inner element type.
/// If the input type cannot be determined, defaults to `Array(Array(Int))`.
///
/// ## Runtime helpers
/// - `__rt_array_chunk`: for scalar indexed arrays (int/float-only elements)
/// - `__rt_array_chunk_refcounted`: for arrays with refcounted elements (strings, objects, nested arrays)
///
/// ## ABI notes
/// - x86_64: preserves source array in `rax` during size evaluation, passes array in `rdi`, size in `rsi`
/// - ARM64: pushes array pointer on stack, passes size in `x1`, restores array in `x0`
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_chunk()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime = matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the source indexed array while evaluating the requested chunk size expression
        let size_ty = emit_expr(&args[1], emitter, ctx, data);
        coerce_to_int(emitter, &size_ty);                                       // unbox a Mixed/Union chunk size into a raw integer
        emitter.instruction("mov rsi, rax");                                    // place the requested chunk size in the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the source indexed array into the first x86_64 runtime argument register
        if uses_refcounted_runtime {
            abi::emit_call_label(emitter, "__rt_array_chunk_refcounted");       // split the refcounted indexed array into chunk arrays through the x86_64 runtime helper
        } else {
            abi::emit_call_label(emitter, "__rt_array_chunk");                  // split the scalar indexed array into chunk arrays through the x86_64 runtime helper
        }

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(Box::new(PhpType::Array(inner)))),
            _ => Some(PhpType::Array(Box::new(PhpType::Array(Box::new(PhpType::Int))))),
        };
    }

    // -- save array pointer, evaluate chunk size --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    let size_ty = emit_expr(&args[1], emitter, ctx, data);
    coerce_to_int(emitter, &size_ty);                                           // unbox a Mixed/Union chunk size into a raw integer
    // -- call runtime to split array into chunks --
    emitter.instruction("mov x1, x0");                                          // move chunk size to x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop array pointer into x0 (first arg)
    if uses_refcounted_runtime {
        emitter.instruction("bl __rt_array_chunk_refcounted");                  // chunk array while retaining borrowed heap elements
    } else {
        emitter.instruction("bl __rt_array_chunk");                             // call runtime: chunk array → x0=array of arrays
    }

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(Box::new(PhpType::Array(inner)))),
        _ => Some(PhpType::Array(Box::new(PhpType::Array(Box::new(PhpType::Int))))),
    }
}
