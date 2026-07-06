//! Purpose:
//! Emits PHP `array_diff` builtin calls over associative or key-aware array data.
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

/// Emits code for the PHP `array_diff($arr1, $arr2)` builtin call.
///
/// Compares `$arr1` against `$arr2` and returns all values from `$arr1` that are
/// not present in `$arr2`. Only values are compared; keys are preserved.
///
/// # Arguments
/// * `args` - Must contain exactly two array expressions (the two input arrays).
///
/// # Behavior
/// - Pushes the first array pointer, evaluates the second array expression, then
///   calls the appropriate runtime helper (`__rt_array_diff` or `__rt_array_diff_refcounted`)
///   based on whether the first array uses refcounted heap storage.
/// - On x86_64: uses register-based ABI (rdi/rsi for first/second array pointers).
/// - On ARM64: uses stack-based push/pop and x0/x1 for array pointers.
/// - Returns the array type of the first argument if it is already an Array,
///   otherwise returns `Array<Int>` as the default value type.
///
/// # Return type
/// `Some(PhpType::Array(...))` reflecting the first input array's inner type.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_diff()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        let uses_refcounted_runtime =
            matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
        abi::emit_push_reg(emitter, "rax");                                     // preserve the first input array while evaluating the second input array expression
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov rsi, rax");                                    // place the second input array pointer in the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the first input array pointer into the first x86_64 runtime argument register
        if uses_refcounted_runtime {
            abi::emit_call_label(emitter, "__rt_array_diff_refcounted");        // compute the borrowed-heap-aware array difference through the dedicated x86_64 runtime helper
        } else {
            abi::emit_call_label(emitter, "__rt_array_diff");                   // compute the integer array difference through the x86_64 runtime helper
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
    // -- call runtime to compute value difference --
    emitter.instruction("mov x1, x0");                                          // move second array pointer to x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop first array pointer into x0
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_diff_refcounted"
    } else {
        "bl __rt_array_diff"
    };
    emitter.instruction(runtime_call);                                          // call runtime: diff arrays → x0=new array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
