//! Purpose:
//! Emits PHP `str_repeat` string transformation or formatting calls.
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

/// Emits the `str_repeat` builtin call.
///
/// Marshals the string operand (args[0]) and integer repeat count (args[1]) into
/// platform-specific argument registers, then calls `__rt_str_repeat` to produce
/// a repeated PHP string. Returns `PhpType::Str` indicating the result is a PHP string.
///
/// # Arguments
/// - `_name`: Ignored; present for dispatcher signature consistency.
/// - `args[0]`: The string to repeat.
/// - `args[1]`: The integer repeat count.
/// - `emitter`: Target-aware assembly emitter; receives register allocations and instructions.
/// - `ctx`: Codegen context carrying variable layout and function metadata.
/// - `data`: Data section for relocatable literals and runtime symbols.
///
/// # Register usage
/// - AArch64: string ptr/len in x1/x2, repeat count in x3; result ptr/len returned in x1/x2.
/// - x86_64: string ptr/len in rax/rdx, repeat count in rdi; result ptr/len returned in x1/x2.
///
/// # Side effects
/// - Caller-saves registers are clobbered by the runtime helper.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("str_repeat()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    // -- save string, evaluate repeat count --
    let (str_ptr_reg, str_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, str_ptr_reg, str_len_reg);                 // preserve the source string while the repeat-count expression is evaluated
    super::args::emit_int_arg(&args[1], emitter, ctx, data);

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x3, x0");                                  // move the repeat count into the third AArch64 string-helper argument register
            abi::emit_pop_reg_pair(emitter, "x1", "x2");                        // restore the source string into the AArch64 runtime string-argument registers
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // move the repeat count into the extra x86_64 runtime argument register used by str_repeat()
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the source string into the standard x86_64 string input registers expected by string helpers
        }
    }

    abi::emit_call_label(emitter, "__rt_str_repeat");                           // call the target-aware runtime helper that repeats the source string into concat storage

    Some(PhpType::Str)
}
