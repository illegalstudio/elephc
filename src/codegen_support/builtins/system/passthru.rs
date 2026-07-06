//! Purpose:
//! Emits PHP `passthru` process-control or shell execution builtin calls.
//! Marshals command/status arguments into runtime helpers with PHP-visible output and exit behavior.
//!
//! Called from:
//! - `crate::codegen_support::builtins::system::emit()`.
//!
//! Key details:
//! - Process calls are effectful and may terminate or emit output, so lowering must preserve evaluation order.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a PHP `passthru` call by executing a null-terminated command string via libc `system()`.
/// The command is evaluated and null-terminated through `__rt_cstr` before the call.
/// On x86_64 the null-terminated pointer is passed in `rdi` (SysV first-argument register).
/// Output from the command writes directly to stdout and is not captured or returned.
/// Returns `PhpType::Void`. The call is effectful and may terminate or emit output.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("passthru()");
    // -- evaluate command string --
    emit_expr(&args[0], emitter, ctx, data);
    // -- null-terminate and call libc system() which outputs directly to stdout --
    abi::emit_call_label(emitter, "__rt_cstr");                                 // null-terminate the command string through the target-aware C-string helper
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // pass the null-terminated command pointer in the SysV first-argument register
    }
    emitter.bl_c("system");                                          // execute command, output goes directly to stdout
    Some(PhpType::Void)
}
