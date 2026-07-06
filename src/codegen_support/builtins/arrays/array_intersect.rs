//! Purpose:
//! Emits PHP `array_intersect` builtin calls over associative or key-aware array data.
//! Owns key/value payload setup and runtime hash-helper invocation for array results or lookups.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::emit()`.
//!
//! Key details:
//! - Array key typing and Mixed payload tags must match the runtime hash-table representation.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `array_intersect` builtin call.
///
/// Computes the value-based intersection of two arrays, returning a new array
/// containing all entries from the first array whose values appear in the second.
///
/// # Arguments
/// * `_name` - Unused; present to match the builtin emitter signature convention.
/// * `args` - Two expressions: the base array and the array to intersect against.
/// * `emitter` - Target-specific assembly emitter.
/// * `ctx` - Codegen context (types, locals, etc.).
/// * `data` - Data section for literals and runtime metadata.
///
/// # Returns
/// `Some(PhpType::Array(...))` matching the input array's inner type, or `Array(Int)` for non-array inputs.
///
/// # ABI / Runtime Behavior
/// - **x86_64**: preserves first array in `rax` while evaluating second argument (push/pop via `rdi`/`rsi` registers); calls `__rt_array_intersect` or `__rt_array_intersect_refcounted`.
/// - **ARM64**: pushes first array to stack, evaluates second argument into `x0`, pops first array into `x0`; calls `__rt_array_intersect` or `__rt_array_intersect_refcounted`.
/// - Picks the refcounted runtime variant when the input array holds refcounted values (objects or arrays); otherwise uses the non-refcounted variant.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_intersect()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        let uses_refcounted_runtime =
            matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
        abi::emit_push_reg(emitter, "rax");                                     // preserve the first input array while evaluating the second input array expression
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov rsi, rax");                                    // place the second input array pointer in the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the first input array pointer into the first x86_64 runtime argument register
        if uses_refcounted_runtime {
            abi::emit_call_label(emitter, "__rt_array_intersect_refcounted");   // compute the borrowed-heap-aware array intersection through the dedicated x86_64 runtime helper
        } else {
            abi::emit_call_label(emitter, "__rt_array_intersect");              // compute the integer array intersection through the x86_64 runtime helper
        }

        return match arr_ty {
            PhpType::Array(inner) => Some(PhpType::Array(inner)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    // -- save first array, evaluate second array --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push first array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to compute value intersection --
    emitter.instruction("mov x1, x0");                                          // move second array pointer to x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop first array pointer into x0
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_intersect_refcounted"
    } else {
        "bl __rt_array_intersect"
    };
    emitter.instruction(runtime_call);                                          // call runtime: intersect arrays → x0=new array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
