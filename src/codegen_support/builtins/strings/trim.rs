//! Purpose:
//! Emits PHP `trim` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `trim` builtin, which strips whitespace (or a specified
/// character mask) from the beginning and end of a string.
///
/// # Arguments
/// - `_name`: Unused; caller dispatches by name so this param is ignored.
/// - `args`: Either 1 argument (string to strip) or 2 arguments (string + mask).
/// - `emitter`: Target-aware instruction emitter.
/// - `ctx`: Codegen context carrying variable layout and metadata.
/// - `data`: Writable data section for string literals and runtime symbols.
///
/// # Behavior
/// - 1 arg: evaluates the string argument, then calls `__rt_trim` to strip
///   ASCII whitespace from both ends.
/// - 2 args: evaluates the string argument, then evaluates the mask argument
///   (source string is preserved on the stack during mask evaluation), then
///   calls `__rt_trim_mask` to strip the given character mask from both ends.
///
/// # Returns
/// Always returns `Some(PhpType::Str)` — the result is an owned PHP string.
///
/// # ABI notes
/// ARM64: source string ptr/len live in x1/x2; mask ptr/len are loaded into x3/x4
///   before the call. x86_64: source string ptr/len are pushed on the stack during
///   mask evaluation, then moved to rdi/rsi for the call.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("trim()");

    if args.len() == 1 {
        super::args::emit_string_arg(&args[0], emitter, ctx, data);
        abi::emit_call_label(emitter, "__rt_trim");                             // strip whitespace from both ends through the target-aware trim runtime helper
    } else {
        super::args::emit_string_arg(&args[0], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("str x1, [sp, #-16]!");                     // push the source string pointer while the trim mask expression is evaluated
                emitter.instruction("str x2, [sp, #-16]!");                     // push the source string length while the trim mask expression is evaluated
                super::args::emit_string_arg(&args[1], emitter, ctx, data);
                emitter.instruction("mov x3, x1");                              // move the trim mask pointer into the secondary trim-mask runtime string-argument pair
                emitter.instruction("mov x4, x2");                              // move the trim mask length into the secondary trim-mask runtime string-argument pair
                emitter.instruction("ldr x2, [sp], #16");                       // restore the source string length after evaluating the trim mask expression
                emitter.instruction("ldr x1, [sp], #16");                       // restore the source string pointer after evaluating the trim mask expression
            }
            Arch::X86_64 => {
                abi::emit_push_reg_pair(emitter, "rax", "rdx");                 // preserve the source string ptr/len while the trim mask expression is evaluated
                super::args::emit_string_arg(&args[1], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the trim mask pointer into the secondary x86_64 trim-mask runtime string-argument slot
                emitter.instruction("mov rsi, rdx");                            // move the trim mask length into the secondary x86_64 trim-mask runtime string-argument slot
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore the source string ptr/len after evaluating the trim mask expression
            }
        }
        abi::emit_call_label(emitter, "__rt_trim_mask");                        // strip the requested character mask from both sides through the target-aware trim runtime helper
    }

    Some(PhpType::Str)
}
