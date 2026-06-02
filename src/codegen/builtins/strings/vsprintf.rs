//! Purpose:
//! Emits PHP `vsprintf($format, $values)` — `sprintf` with the arguments
//! supplied as an array instead of a variadic list. Delegates to the
//! `__rt_vsprintf` runtime bridge, which pushes one tagged record per array
//! element and tail-calls `__rt_sprintf`.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - PHP evaluates `$format` before `$values`; the format string is preserved
//!   across the array evaluation, then both are handed to `__rt_vsprintf`
//!   (array pointer in the SysV first arg / x0, format in the elephc string
//!   pair). Returns `PhpType::Str`.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a `vsprintf($format, $values)` call. Evaluates the format (preserved
/// across the array evaluation) and the arguments array, then calls
/// `__rt_vsprintf`. Returns `Some(PhpType::Str)`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("vsprintf()");
    emit_expr(&args[0], emitter, ctx, data); // format string → string-result pair
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // scratch slot for the format string
            emitter.instruction("stp x1, x2, [sp, #0]");                        // save the format ptr/len across the array evaluation
            emit_expr(&args[1], emitter, ctx, data); // arguments array → x0
            emitter.instruction("ldp x1, x2, [sp, #0]");                        // restore the format ptr/len
            emitter.instruction("add sp, sp, #16");                             // release the scratch slot
            abi::emit_call_label(emitter, "__rt_vsprintf");                     // bridge to __rt_sprintf via the per-element records
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // scratch slot for the format string
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // save the format ptr across the array evaluation
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save the format len across the array evaluation
            emit_expr(&args[1], emitter, ctx, data); // arguments array → rax
            emitter.instruction("mov rdi, rax");                                // array pointer → __rt_vsprintf first argument
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // restore the format ptr
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // restore the format len
            emitter.instruction("add rsp, 16");                                 // release the scratch slot
            abi::emit_call_label(emitter, "__rt_vsprintf");                     // bridge to __rt_sprintf via the per-element records
        }
    }
    Some(PhpType::Str)
}
