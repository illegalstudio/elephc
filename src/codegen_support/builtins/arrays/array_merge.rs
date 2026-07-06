//! Purpose:
//! Emits PHP `array_merge` builtin calls that allocate or reshape array values.
//! Coordinates element type selection with runtime helpers that build indexed or associative arrays.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::emit()`.
//!
//! Key details:
//! - Returned arrays must use the payload layout expected by later codegen and GC/refcount paths.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::functions;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `array_merge` builtin.
///
/// Combines two PHP array operands into a single merged array at runtime.
/// The returned `PhpType` reflects the first operand's inner type, or `Array(Int)`
/// when the first operand is a scalar.
///
/// # Arguments
/// * `_name` — unused, present to match the builtin emitter signature
/// * `args` — two PHP expressions: `[0]` is the first array, `[1]` is the second
/// * `emitter` — target-specific assembly emitter
/// * `ctx` — codegen context (frame layout, variables, class metadata)
/// * `data` — data section for relocations and static data
///
/// # ABI behavior
/// * **x86_64**: pushes first array pointer in `rax` to the stack, evaluates second
///   array into `rax`, then moves pointers into `rdi`/`rsi` for the runtime call.
/// * **ARM64**: pushes first array pointer to the stack, evaluates second into `x0`,
///   then loads both pointers into `x0`/`x1` for the runtime call.
///
/// # Runtime helpers
/// * `__rt_array_merge` — merges two scalar/indexed arrays (no refcount management)
/// * `__rt_array_merge_refcounted` — merges arrays with refcounted elements, retaining
///   borrowed heap references
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_merge()");
    let second_arr_ty = args
        .get(1)
        .map(|arg| functions::infer_contextual_type(arg, ctx));
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let result_ty = array_merge_result_type(arr_ty.clone(), second_arr_ty.as_ref());
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the first scalar indexed-array pointer while evaluating the second merge operand
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov rsi, rax");                                    // move the second scalar indexed-array pointer into the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the first scalar indexed-array pointer into the first x86_64 runtime argument register
        if uses_refcounted_runtime {
            abi::emit_call_label(emitter, "__rt_array_merge_refcounted");       // merge the two refcounted indexed arrays through the x86_64 runtime helper
        } else {
            abi::emit_call_label(emitter, "__rt_array_merge");                  // merge the two scalar indexed arrays through the x86_64 runtime helper
        }

        return Some(result_ty);
    }

    // -- save first array, evaluate second array --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push first array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to merge two arrays --
    emitter.instruction("mov x1, x0");                                          // move second array pointer to x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop first array pointer into x0
    if uses_refcounted_runtime {
        emitter.instruction("bl __rt_array_merge_refcounted");                  // merge arrays while retaining borrowed heap elements
    } else {
        emitter.instruction("bl __rt_array_merge");                             // call runtime: merge arrays → x0=new array
    }

    Some(result_ty)
}

/// Infers the legacy emitter result type for `array_merge()`.
///
/// The runtime helper can copy scalar 8-byte payloads from the right operand even when
/// the first operand is statically empty, so the result may adopt the right element type
/// for that supported subset.
fn array_merge_result_type(first: PhpType, second: Option<&PhpType>) -> PhpType {
    match first {
        PhpType::Array(elem) if is_empty_array_element_type(elem.as_ref()) => match second {
            Some(PhpType::Array(right)) if is_scalar_merge_element_type(right.as_ref()) => {
                PhpType::Array(right.clone())
            }
            _ => PhpType::Array(elem),
        },
        PhpType::Array(elem) => PhpType::Array(elem),
        _ => PhpType::Array(Box::new(PhpType::Int)),
    }
}

/// Returns true for the element sentinel used by statically empty indexed arrays.
fn is_empty_array_element_type(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Void)
}

/// Returns true for element types copied safely by the scalar merge runtime helper.
fn is_scalar_merge_element_type(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Callable | PhpType::Void
    )
}
