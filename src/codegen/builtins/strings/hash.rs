//! Purpose:
//! Emits PHP `hash` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a PHP `hash($algo, $data)` call as a runtime helper invocation.
///
/// The algorithm string is evaluated first and preserved (on stack for AArch64,
/// or in registers for x86_64) while the data string is evaluated second.
/// After both arguments are materialised in the correct ABI registers, dispatches
/// to `__rt_hash` which handles md5/sha1/sha256 and returns a lowercase hex string.
///
/// # Arguments
/// - `_name`: Unused; the runtime helper handles algorithm dispatch internally.
/// - `args`: Two expressions — algorithm name and data string.
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context carrying variable layout and metadata.
/// - `data`: Data section for relocatable constants.
///
/// # Returns
/// `Some(PhpType::Str)` indicating the result is a PHP string.
///
/// # Side effects
/// - Clobbers caller-saved registers appropriate to each target's ABI.
/// - The runtime helper allocates a PHP string; caller owns the returned value.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hash()");
    // hash($algo, $data) — evaluate algo string first
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the algorithm string while evaluating the data string expression
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the data string pointer into the secondary runtime argument register pair on AArch64
            emitter.instruction("mov x4, x2");                                  // move the data string length into the secondary runtime argument register pair on AArch64
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the algorithm string after evaluating the data string expression
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the algorithm string ptr/len while evaluating the data string expression on x86_64
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the data string pointer into the secondary x86_64 runtime argument register
            emitter.instruction("mov rsi, rdx");                                // move the data string length into the secondary x86_64 runtime argument register
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the algorithm string ptr/len after evaluating the data string expression
        }
    }
    abi::emit_call_label(emitter, "__rt_hash");                                 // call the target-aware runtime helper that dispatches between md5/sha1/sha256 and returns lowercase hex
    Some(PhpType::Str)
}
