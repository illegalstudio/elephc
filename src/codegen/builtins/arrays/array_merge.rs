//! Purpose:
//! Emits PHP `array_merge` builtin calls that allocate or reshape array values.
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
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
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
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    // Pick the merge helper from the element kind of EITHER argument: an empty `[]` first
    // argument carries no element-type hint, so a common `$r = []; array_merge($r, $strs)`
    // must still use the string path. String elements use 16-byte (ptr+len) slots; other
    // refcounted elements (array/object/Mixed) are 8-byte heap pointers; the rest are scalar.
    // Routing strings through the 8-byte merge would corrupt every element past the first.
    let second_ty = crate::codegen::functions::infer_contextual_type(&args[1], ctx);
    let is_str_array =
        |t: &PhpType| matches!(t, PhpType::Array(inner) if matches!(**inner, PhpType::Str));
    let is_refcounted_array =
        |t: &PhpType| matches!(t, PhpType::Array(inner) if inner.is_refcounted());
    let any_str = is_str_array(&arr_ty) || is_str_array(&second_ty);
    let any_refcounted = is_refcounted_array(&arr_ty) || is_refcounted_array(&second_ty);
    let merge_label = if any_str {
        "__rt_array_merge_str"
    } else if any_refcounted {
        "__rt_array_merge_refcounted"
    } else {
        "__rt_array_merge"
    };
    let result_ty = if any_str {
        PhpType::Array(Box::new(PhpType::Str))
    } else {
        [&arr_ty, &second_ty]
            .into_iter()
            .find_map(|t| match t {
                PhpType::Array(inner) if inner.is_refcounted() => {
                    Some(PhpType::Array(inner.clone()))
                }
                _ => None,
            })
            .or_else(|| match &arr_ty {
                PhpType::Array(inner) => Some(PhpType::Array(inner.clone())),
                _ => None,
            })
            .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Int)))
    };
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the first indexed-array pointer while evaluating the second merge operand
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov rsi, rax");                                    // move the second indexed-array pointer into the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the first indexed-array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, merge_label);                            // merge the two indexed arrays through the element-kind-appropriate x86_64 runtime helper

        return Some(result_ty.clone());
    }

    // -- save first array, evaluate second array --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push first array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to merge two arrays --
    emitter.instruction("mov x1, x0");                                          // move second array pointer to x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop first array pointer into x0
    emitter.instruction(&format!("bl {}", merge_label));                        // merge the two indexed arrays through the element-kind-appropriate runtime helper

    Some(result_ty)
}
