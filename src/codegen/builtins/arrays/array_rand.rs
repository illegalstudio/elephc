//! Purpose:
//! Emits PHP `array_rand` builtin calls for array values.
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

/// Emits a call to the `array_rand` builtin.
///
/// # Arguments
/// - `args[0]`: the input array expression; evaluated and its pointer placed in the
///   appropriate argument register (`rdi` on x86_64, `x0` on ARM64).
/// - `emitter`: used to emit instructions and comments.
/// - `ctx`: carries variable layout and codegen state.
/// - `data`: data section for relocations and constants.
///
/// # Returns
/// `Some(PhpType::Int)` — the selected random array key is returned in `x0`/`rax`
/// depending on target.
///
/// # Codegen behavior
/// - x86_64: moves the array pointer from `rax` to `rdi`, calls `__rt_array_rand`.
/// - ARM64: calls `__rt_array_rand` directly (array pointer already in `x0`).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_rand()");
    emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the source indexed-array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_array_rand");                       // choose a random scalar indexed-array key through the x86_64 runtime helper
        return Some(PhpType::Int);
    }

    // -- call runtime to pick a random index from array --
    emitter.instruction("bl __rt_array_rand");                                  // call runtime: random index → x0=random key

    Some(PhpType::Int)
}
