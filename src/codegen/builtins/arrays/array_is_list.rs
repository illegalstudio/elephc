//! Purpose:
//! Emits PHP `array_is_list` builtin calls.
//! Returns a boolean indicating whether an array has sequential 0..n-1 integer keys.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Indexed arrays are lists by construction; associative arrays and Mixed values defer to the runtime walk.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `array_is_list` builtin.
///
/// `array_is_list($array)` returns `true` when the array's keys are exactly the
/// integers `0..count-1` in order (the empty array is a list), and `false`
/// otherwise.
///
/// # Codegen
/// - Evaluates `args[0]` into the container register.
/// - For a statically indexed `PhpType::Array`, loads the constant `1`: indexed
///   arrays always have sequential keys, so the runtime walk is skipped.
/// - For associative arrays and `Mixed` values, calls `__rt_array_is_list`, which
///   reads the heap kind, walks the hash insertion-order chain, and unwraps boxed
///   array payloads.
///
/// # Returns
/// `Some(PhpType::Bool)` — the list-shape predicate result in the integer result register.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_is_list()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(arr_ty, PhpType::Array(_)) {
        if emitter.target.arch == Arch::X86_64 {
            emitter.instruction("mov eax, 1");                                  // indexed arrays always have sequential 0..n-1 keys
        } else {
            emitter.instruction("mov x0, #1");                                  // indexed arrays always have sequential 0..n-1 keys
        }
        return Some(PhpType::Bool);
    }

    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the container pointer into the first x86_64 argument register
        abi::emit_call_label(emitter, "__rt_array_is_list");
        return Some(PhpType::Bool);
    }

    emitter.instruction("bl __rt_array_is_list");                               // walk the hash insertion order to test list shape
    Some(PhpType::Bool)
}
