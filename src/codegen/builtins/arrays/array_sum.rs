//! Purpose:
//! Emits PHP `array_sum` builtin calls for array values.
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

/// Emits code to compute the sum of all numeric values in a PHP `array` argument.
///
/// ## Arguments
/// - `args[0]` — the array expression to sum; evaluated and loaded into `rax` before the call.
/// - `_name` — unused; matches the builtin dispatch signature.
///
/// ## Codegen
/// - Evaluates `args[0]` into `rax`.
/// - **x86_64**: copies `rax` → `rdi` (first integer arg register), then calls `__rt_array_sum`.
/// - **ARM64**: directly calls `__rt_array_sum` with the value already in `x0`.
/// - Both architectures return the integer sum in `x0`/`rax` via the runtime helper.
///
/// ## Returns
/// `Some(PhpType::Int)` — the summed integer result. Runtime helper handles empty arrays and non-integer elements per PHP semantics.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_sum()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    // -- a float[] sums as IEEE doubles (result in d0/xmm0); everything else sums as int --
    let (runtime, ret_ty) = match &arr_ty {
        PhpType::Array(elem) if matches!(**elem, PhpType::Float) => {
            ("__rt_array_sum_float", PhpType::Float)
        }
        _ => ("__rt_array_sum", PhpType::Int),
    };
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the source indexed-array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, runtime);                                 // sum the indexed-array payloads through the x86_64 runtime helper
        return Some(ret_ty);
    }

    // -- call runtime to compute sum of all array elements --
    emitter.instruction(&format!("bl {}", runtime));                            // call runtime: sum array elements → x0 (int) or d0 (float)

    Some(ret_ty)
}
