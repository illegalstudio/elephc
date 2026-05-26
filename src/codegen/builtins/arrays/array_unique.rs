//! Purpose:
//! Emits PHP `array_unique` builtin calls that allocate or reshape array values.
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

/// Emits the `array_unique` builtin call, removing duplicate values from an indexed array.
///
/// Arguments:
///   - `args[0]`: the source array expression
///
/// Runtime helpers:
///   - `__rt_array_unique` for scalar indexed arrays
///   - `__rt_array_unique_refcounted` for refcounted indexed arrays
///
/// On x86_64: moves the source array pointer from `rax` to `rdi` before the call.
/// On ARM64: uses `bl` with the appropriate helper label.
///
/// Returns an `Array(Int)` type (keys are renumbered sequentially as integers).
/// The `_name` parameter is unused; builtin resolution is handled by the caller.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_unique()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the source scalar indexed-array pointer into the first x86_64 runtime argument register
        if uses_refcounted_runtime {
            abi::emit_call_label(emitter, "__rt_array_unique_refcounted");      // deduplicate the refcounted indexed-array payloads through the x86_64 runtime helper
        } else {
            abi::emit_call_label(emitter, "__rt_array_unique");                 // deduplicate the scalar indexed-array payloads through the x86_64 runtime helper
        }

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(inner)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    // -- call runtime to create array with duplicate values removed --
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_unique_refcounted"
    } else {
        "bl __rt_array_unique"
    };
    emitter.instruction(runtime_call);                                          // call runtime: deduplicate array → x0=new array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
