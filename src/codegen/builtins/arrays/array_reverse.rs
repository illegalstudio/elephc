//! Purpose:
//! Emits PHP `array_reverse` builtin calls that allocate or reshape array values.
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

/// Emits code for the PHP `array_reverse` builtin.
///
/// # Arguments
/// - `_name`: Unused name for dispatcher compatibility (builtin is identified by signature).
/// - `args[0]`: The array expression to reverse. Must be evaluated first; result is in `x0`/`rax`.
/// - `emitter`: Target-aware instruction emitter.
/// - `ctx`: Codegen context carrying variable layout and class metadata.
/// - `data`: Read-only data section for relocations and static data.
///
/// # Returns
/// `Some(PhpType::Array(...))` matching the input array's element type, or
/// `Some(PhpType::Array(Box::new(PhpType::Int)))` if the input type is not an `Array`
/// (defaulting to `int`-indexed array on return).
///
/// # Behavior
/// - Evaluates `args[0]` to produce the source array in `x0`/`rax`.
/// - On x86_64: moves the array pointer to `rdi` (first calling-convention arg) and calls
///   `__rt_array_reverse` or `__rt_array_reverse_refcounted` based on `arr_ty`.
/// - On ARM64: uses `bl` to call `__rt_array_reverse` or `__rt_array_reverse_refcounted`.
/// - Result array is returned in `x0`/`rax`.
/// - Preserves `ctx` state; emits a `"array_reverse()"` comment for debug traceability.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_reverse()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the source scalar indexed-array pointer into the first x86_64 runtime argument register
        if uses_refcounted_runtime {
            abi::emit_call_label(emitter, "__rt_array_reverse_refcounted");     // reverse the refcounted indexed-array payloads through the x86_64 runtime helper
        } else {
            abi::emit_call_label(emitter, "__rt_array_reverse");                // reverse the scalar indexed-array payloads through the x86_64 runtime helper
        }

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(inner)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    // -- call runtime to create reversed copy of array --
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_reverse_refcounted"
    } else {
        "bl __rt_array_reverse"
    };
    emitter.instruction(runtime_call);                                          // call runtime: reverse array → x0=new array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
