//! Purpose:
//! Emits PHP `exit` process-control or shell execution builtin calls.
//! Marshals command/status arguments into runtime helpers with PHP-visible output and exit behavior.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Process calls are effectful and may terminate or emit output, so lowering must preserve evaluation order.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::{Arch, Platform};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `exit` builtin, which terminates the current process.
///
/// If `args` contains an expression, it is evaluated first and its integer value is used as
/// the exit code. If `args` is empty, the exit code is 0.
///
/// Arguments are evaluated in source order before the exit syscall/instruction is emitted,
/// ensuring any side effects (e.g. `echo`) are observable. After evaluation the process is
/// terminated via the target's native exit ABI — there is no return.
///
/// Returns `PhpType::Void`. All platforms set the integer result register to the exit code
/// before invoking the exit syscall/instruction.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("exit()");
    if let Some(arg) = args.first() {
        emit_expr(arg, emitter, ctx, data);
    } else {
        // -- default exit code when no argument given --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #0");                              // set exit code to 0 (success)
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, 0");                              // set exit code to 0 (success) in the native integer result register
            }
        }
    }
    // -- terminate the process using the target's native exit ABI --
    match (emitter.target.platform, emitter.target.arch) {
        (Platform::MacOS, Arch::AArch64) | (Platform::Linux, Arch::AArch64) => {
            emitter.syscall(1);                                                 // invoke the platform exit syscall using the integer result register as the code
        }
        (Platform::Linux, Arch::X86_64) => {
            emitter.instruction("mov rdi, rax");                                // move the computed exit code into the SysV first-argument register
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate the process through the Linux x86_64 syscall ABI
        }
        (Platform::MacOS, Arch::X86_64) => {
            panic!("exit() is not implemented yet for target macos-x86_64");
        }
    }

    Some(PhpType::Void)
}
