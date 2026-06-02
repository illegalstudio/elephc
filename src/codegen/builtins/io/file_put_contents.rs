//! Purpose:
//! Emits PHP `file_put_contents` filesystem mutation builtin calls.
//! Passes path and mode/owner arguments to runtime helpers that perform observable OS operations.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `file_put_contents` builtin call.
///
/// Saves the path argument (args[0]) on the stack/caller-saved registers, evaluates
/// the data argument (args[1]) in source order, then materializes all four string-argument
/// registers and calls `__rt_file_put_contents`. Returns `PhpType::Int` (byte count or false).
///
/// # Arguments
/// - `_name`: ignored (always `file_put_contents`)
/// - `args[0]`: path string
/// - `args[1]`: data string
///
/// # Side effects
/// - Performs observable filesystem writes via the runtime helper.
/// - Clobbers caller-saved registers (`x0`-`x7`/`rdi`-`rsi` pairs) per ABI.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("file_put_contents()");
    // file_put_contents("phar://archive/entry", $data) assembles a signed
    // single-entry phar (same runtime as fopen+fwrite+fclose). A literal phar://
    // URL that resolves to a write target is handled here; anything else (or an
    // unresolvable URL) falls through to the normal file write below.
    if let crate::parser::ast::ExprKind::StringLiteral(url) = &args[0].kind {
        if url.starts_with("phar://") && args.len() >= 2 {
            if let Some(ty) =
                super::phar_stream::emit_file_put_contents_write(url, &args[1], emitter, ctx, data)
            {
                return Some(ty);
            }
        }
    }
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push filename ptr and length onto the temporary stack while the data expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the data pointer into the third string-argument register pair slot
            emitter.instruction("mov x4, x2");                                  // move the data length into the fourth string-argument register pair slot
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the filename pointer and length after evaluating the data expression
            abi::emit_call_label(emitter, "__rt_file_put_contents");            // call the target-aware runtime helper that writes the string payload to disk
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the filename pointer and length while the data expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the data pointer into the third x86_64 string-argument slot
            emitter.instruction("mov rsi, rdx");                                // move the data length into the fourth x86_64 string-argument slot
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the filename pointer and length after evaluating the data expression
            abi::emit_call_label(emitter, "__rt_file_put_contents");            // call the target-aware runtime helper that writes the string payload to disk
        }
    }
    Some(PhpType::Int)
}
