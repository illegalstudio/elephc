//! Purpose:
//! Emits PHP `array_key_first` and `array_key_last` builtin calls.
//! Returns the first or last key of an array boxed as a Mixed value.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - The selected key is boxed by `__rt_array_edge_key`; empty containers yield a boxed null.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `array_key_first` / `array_key_last` builtins.
///
/// `array_key_first($array)` returns the first key, `array_key_last($array)` the
/// last key, both in insertion order, or `null` when the array is empty. The key
/// is returned as a boxed `Mixed` value so int and string keys share one path.
///
/// # Arguments
/// - `name`: selects the variant; `"array_key_last"` reads the tail, otherwise the head.
/// - `args[0]`: the array expression, evaluated into the container register.
///
/// # Codegen
/// - Evaluates `args[0]` into the container register.
/// - Loads the first/last selector and calls `__rt_array_edge_key`, which reads the
///   heap kind, picks the head or tail entry, and boxes the key (or null) as a Mixed cell.
///
/// # Returns
/// `Some(PhpType::Mixed)` — the boxed key (or boxed null) in the integer result register.
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let last = name == "array_key_last";
    emitter.comment(if last { "array_key_last()" } else { "array_key_first()" });
    emit_expr(&args[0], emitter, ctx, data);

    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the container pointer into the first x86_64 argument register
        if last {
            emitter.instruction("mov esi, 1");                                  // select the last key
        } else {
            emitter.instruction("mov esi, 0");                                  // select the first key
        }
        abi::emit_call_label(emitter, "__rt_array_edge_key");
        return Some(PhpType::Mixed);
    }

    if last {
        emitter.instruction("mov x1, #1");                                      // select the last key
    } else {
        emitter.instruction("mov x1, #0");                                      // select the first key
    }
    emitter.instruction("bl __rt_array_edge_key");                              // box the selected key as a Mixed value
    Some(PhpType::Mixed)
}
