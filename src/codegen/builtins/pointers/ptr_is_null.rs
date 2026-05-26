//! Purpose:
//! Emits compiler-extension `ptr_is_null` null-pointer operations.
//! Materializes or tests raw pointer sentinel values in the target integer register convention.
//!
//! Called from:
//! - `crate::codegen::builtins::pointers::emit()`.
//!
//! Key details:
//! - Null pointers are raw addresses, not PHP null Mixed cells.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `ptr_is_null` builtin, which tests whether a raw pointer is null.
///
/// # Arguments
/// - `args[0]`: the pointer expression to test (already emitted into the result register).
///
/// # Result
/// Returns `PhpType::Bool`: 1 if the pointer is null (sentinel `0x0`), 0 otherwise.
/// The result is materialized in the integer register convention (`x0` on AArch64, `rax` on x86_64).
///
/// # ABI
/// The pointer payload is assumed to already reside in the integer result register (`x0`/`rax`).
/// The null comparison and boolean materialization are emitted inline using `cmp`/`cset` or `test`/`sete`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ptr_is_null()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- check if pointer is null (0x0) --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // compare the pointer payload against the null sentinel on AArch64
            emitter.instruction("cset x0, eq");                                 // materialize 1 when the pointer is null and 0 otherwise on AArch64
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // compare the pointer payload against the null sentinel on x86_64
            emitter.instruction("sete al");                                     // materialize the boolean null result in the low byte register
            emitter.instruction("movzx rax, al");                               // widen the boolean null result back into the x86_64 integer result register
        }
    }
    Some(PhpType::Bool)
}
