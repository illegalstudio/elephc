//! Purpose:
//! Emits PHP `array_sum` builtin calls for array values.
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
    emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the source scalar indexed-array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_array_sum");                        // add the scalar indexed-array payloads through the x86_64 runtime helper
        return Some(PhpType::Int);
    }

    // -- call runtime to compute sum of all array elements --
    emitter.instruction("bl __rt_array_sum");                                   // call runtime: sum array elements → x0=sum

    Some(PhpType::Int)
}
