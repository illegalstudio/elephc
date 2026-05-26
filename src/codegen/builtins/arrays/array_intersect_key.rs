//! Purpose:
//! Emits PHP `array_intersect_key` builtin calls over associative or key-aware array data.
//! Owns key/value payload setup and runtime hash-helper invocation for array results or lookups.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Array key typing and Mixed payload tags must match the runtime hash-table representation.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `array_intersect_key($arr1, $arr2)` builtin call.
///
/// Reduces `$arr1` to only keys present in `$arr2` using runtime helper
/// `__rt_array_intersect_key`. Preserves the first array's type as return type.
///
/// ## Arguments
/// - `args[0]`: the base associative array to filter
/// - `args[1]`: the mask associative array whose keys define the intersection
///
/// ## Register/ABI usage
/// - On AArch64: first array pointer in `x0`, second in `x1`, result pointer in `x0`
/// - On x86_64: first array pointer in `rdi`, second in `rsi`, result pointer in `rax`
///
/// ## Side effects
/// - Re-evaluates both argument expressions (caller must ensure side-effect order)
/// - Clobbers caller-saved registers used for array pointer transport
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_intersect_key()");
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
    abi::emit_call_label(emitter, "__rt_array_intersect_key");                  // compute the associative-array key intersection and return the filtered hash table pointer

    Some(arr_ty)
}
