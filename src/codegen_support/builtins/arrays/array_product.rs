//! Purpose:
//! Emits PHP `array_product` builtin calls for array values.
//! Materializes arguments and delegates payload work to the matching runtime helper or inline lowering.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::emit()`.
//!
//! Key details:
//! - Array element type and ownership assumptions must match the type checker and runtime layout.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code to compute the product of all numeric values in a PHP array.
///
/// Arguments:
/// - `args[0]` must be the array expression (already emitted by caller).
///
/// ABI:
/// - x86_64: passes array pointer via `rdi`, returns product in `rax` as `PhpType::Int`.
/// - ARM64: calls `__rt_array_product` runtime helper, returns product in `x0` as `PhpType::Int`.
///
/// Side effects: calls `__rt_array_product` runtime routine which iterates the array.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_product()");
    emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the source scalar indexed-array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_array_product");                    // multiply the scalar indexed-array payloads through the x86_64 runtime helper
        return Some(PhpType::Int);
    }

    // -- call runtime to compute product of all array elements --
    emitter.instruction("bl __rt_array_product");                               // call runtime: multiply array elements → x0=product

    Some(PhpType::Int)
}
