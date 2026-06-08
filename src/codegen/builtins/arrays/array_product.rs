//! Purpose:
//! Emits PHP `array_product` builtin calls for array values.
//! Materializes arguments and delegates payload work to the matching runtime helper or inline lowering.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Array element type and ownership assumptions must match the type checker and runtime layout.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
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
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    // -- a float[] multiplies as IEEE doubles (result in d0/xmm0); everything else as int --
    let (runtime, ret_ty) = match &arr_ty {
        PhpType::Array(elem) if matches!(**elem, PhpType::Float) => {
            ("__rt_array_product_float", PhpType::Float)
        }
        _ => ("__rt_array_product", PhpType::Int),
    };
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the source indexed-array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, runtime);                                 // multiply the indexed-array payloads through the x86_64 runtime helper
        return Some(ret_ty);
    }

    // -- call runtime to compute product of all array elements --
    emitter.instruction(&format!("bl {}", runtime));                            // call runtime: multiply array elements → x0 (int) or d0 (float)

    Some(ret_ty)
}
