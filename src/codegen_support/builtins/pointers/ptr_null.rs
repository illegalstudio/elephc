//! Purpose:
//! Emits compiler-extension `ptr_null` null-pointer operations.
//! Materializes or tests raw pointer sentinel values in the target integer register convention.
//!
//! Called from:
//! - `crate::codegen_support::builtins::pointers::emit()`.
//!
//! Key details:
//! - Null pointers are raw addresses, not PHP null Mixed cells.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `ptr_null` builtin: materializes a null pointer sentinel.
/// Returns `PhpType::Pointer(None)` — the raw address 0x0.
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ptr_null()");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #0");                                  // materialize the null pointer sentinel in the AArch64 integer result register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 0");                                  // materialize the null pointer sentinel in the x86_64 integer result register
        }
    }
    Some(PhpType::Pointer(None))
}
