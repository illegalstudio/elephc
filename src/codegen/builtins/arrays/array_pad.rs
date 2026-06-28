//! Purpose:
//! Emits PHP `array_pad` builtin calls that allocate or reshape array values.
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

/// Emits the `array_pad($array, $target_size, $pad_value)` builtin call.
///
/// Saves the input array pointer, evaluates `$target_size` and `$pad_value` from left to right,
/// then calls the appropriate runtime helper (`__rt_array_pad` or `__rt_array_pad_refcounted`).
/// The chosen runtime path depends on whether the input array is refcounted.
/// Returns the type of the resulting padded array (preserves the inner type for Array inputs).
///
/// # Arguments
/// * `_name` - Unused; present to match the builtin emitter signature.
/// * `args` - Three expressions: the input array, the target size, and the pad value.
/// * `emitter` - The assembly emitter for the target architecture.
/// * `ctx` - Compilation context (types, locals, etc.).
/// * `data` - Data section for relocations and constants.
///
/// # Architecture
/// - **x86_64**: Uses `push`/`pop` to preserve registers across argument evaluation.
/// - **ARM64**: Uses pre-decrement store/load to push arguments onto the stack.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_pad()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the source scalar indexed-array pointer while evaluating the target size expression
        let size_ty = emit_expr(&args[1], emitter, ctx, data);
        coerce_to_int(emitter, &size_ty);                                       // unbox a Mixed/Union target size into a raw integer
        abi::emit_push_reg(emitter, "rax");                                     // preserve the requested target size while evaluating the scalar pad value
        emit_expr(&args[2], emitter, ctx, data);
        emitter.instruction("mov rdx, rax");                                    // move the scalar pad value into the third x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rsi");                                      // restore the requested target size into the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the source scalar indexed-array pointer into the first x86_64 runtime argument register
        if uses_refcounted_runtime {
            abi::emit_call_label(emitter, "__rt_array_pad_refcounted");         // pad the refcounted indexed array through the x86_64 runtime helper
        } else {
            abi::emit_call_label(emitter, "__rt_array_pad");                    // pad the scalar indexed array through the x86_64 runtime helper
        }

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(inner)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    // -- save array pointer, evaluate target size --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    let size_ty = emit_expr(&args[1], emitter, ctx, data);
    coerce_to_int(emitter, &size_ty);                                           // unbox a Mixed/Union target size into a raw integer
    // -- save target size, evaluate pad value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push target size onto stack
    emit_expr(&args[2], emitter, ctx, data);
    // -- set up three-arg call: array, size, value --
    emitter.instruction("mov x2, x0");                                          // move pad value to x2 (third arg)
    emitter.instruction("ldr x1, [sp], #16");                                   // pop target size into x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop array pointer into x0 (first arg)
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_pad_refcounted"
    } else {
        "bl __rt_array_pad"
    };
    emitter.instruction(runtime_call);                                          // call runtime: pad array → x0=new array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
