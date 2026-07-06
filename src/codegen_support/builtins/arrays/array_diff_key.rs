//! Purpose:
//! Emits PHP `array_diff_key` builtin calls over associative or key-aware array data.
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

/// Emits PHP `array_diff_key($arr1, $arr2)` by computing the key-wise difference of two associative arrays.
///
/// # Arguments
/// - `args[0]`: the base associative array whose keys are retained
/// - `args[1]`: the mask associative array whose keys are excluded
///
/// # Behavior
/// Pushes the first array pointer onto the stack, evaluates the second array,
/// then loads both pointers into the runtime helper argument registers and
/// calls `__rt_array_diff_key` to produce a new hash table containing only
/// keys present in `$arr1` but not in `$arr2`.
///
/// # Returns
/// `Some(PhpType)` with the type of the first argument (an associative array type);
/// `None` if no type information is available.
///
/// # ABI constraints
/// - AArch64: first array pointer in `x0`, second array pointer in `x1`; result pointer in `x0`.
/// - X86_64: first array pointer in `rdi`, second array pointer in `rsi`; result pointer in `rax`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_diff_key()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    // -- save first array, evaluate second array --
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the first associative-array pointer while evaluating the mask array
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move the second associative-array pointer into the second runtime helper argument register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the first associative-array pointer into the first runtime helper argument register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // move the second associative-array pointer into the second SysV runtime helper argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the first associative-array pointer into the first SysV runtime helper argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_array_diff_key");                       // compute the associative-array key difference and return the filtered hash table pointer

    Some(arr_ty)
}
