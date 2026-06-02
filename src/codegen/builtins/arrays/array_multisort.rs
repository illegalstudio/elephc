//! Purpose:
//! Emits PHP `array_multisort` builtin calls over two parallel arrays, mutating both in place.
//! Prepares copy-on-write storage for each by-reference array, then sorts them in tandem.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Both arrays are by-reference; their PHP-visible storage is updated before the in-place tandem sort.

use crate::codegen::abi;
use super::ensure_unique_arg::emit_ensure_unique_arg;
use super::store_mutating_arg::emit_store_mutating_arg;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the PHP `array_multisort($arr1, $arr2)` builtin call.
///
/// Sorts `$arr1` ascending (stable) and reorders `$arr2` in tandem so the two parallel
/// arrays stay aligned. Both arguments are by-reference: each is made copy-on-write unique
/// and its caller storage is updated before the runtime sorts both arrays in place.
///
/// Supports the common two-array form with scalar (integer) elements and ascending order.
/// Sort flags, descending order, multi-key tie-breaking, and more than two arrays are not
/// yet supported.
///
/// # Returns
/// `Some(PhpType::Bool)` — `array_multisort()` returns `true` on success.
///
/// # ABI
/// - AArch64: arr1 in `x0`, arr2 in `x1`. x86_64: arr1 in `rdi`, arr2 in `rsi`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_multisort()");
    let result_reg = abi::int_result_reg(emitter);

    // -- first array: evaluate, split shared storage, write the pointer back to its variable --
    let ty1 = emit_expr(&args[0], emitter, ctx, data);
    emit_ensure_unique_arg(emitter, &ty1);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    abi::emit_push_reg(emitter, result_reg);                                     // save arr1 pointer while evaluating arr2

    // -- second array: evaluate, split shared storage, write the pointer back to its variable --
    let ty2 = emit_expr(&args[1], emitter, ctx, data);
    emit_ensure_unique_arg(emitter, &ty2);
    emit_store_mutating_arg(emitter, ctx, &args[1]);

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move arr2 pointer into the second runtime argument register
            abi::emit_pop_reg(emitter, "x0");
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // move arr2 pointer into the second SysV runtime argument register
            abi::emit_pop_reg(emitter, "rdi");
        }
    }
    abi::emit_call_label(emitter, "__rt_array_multisort");

    // -- array_multisort returns true on success --
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, #1"),                     // result: true
        Arch::X86_64 => emitter.instruction("mov eax, 1"),                      // result: true
    }

    Some(PhpType::Bool)
}
