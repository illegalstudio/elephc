//! Purpose:
//! Emits PHP `rtrim` string transformation or formatting calls.
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
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `rtrim(chars?)` builtin.
///
/// Dispatches to `__rt_rtrim` (1-arg: strip ASCII whitespace from the right)
/// or `__rt_rtrim_mask` (2-arg: strip each character in `chars` from the right).
///
/// # Arguments
/// - `_name`: Unused; present to match the builtin emitter signature.
/// - `args`: Either one argument (string to trim) or two (string + character mask).
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context carrying variable layout and class metadata.
/// - `data`: Writable data section for string literals.
///
/// # Returns
/// Always returns `Some(PhpType::Str)`; the caller does not need to handle null.
///
/// # ABI / Register Usage
/// Two-argument calls preserve the first string's ptr/len pair while evaluating
/// the second argument expression, then load both into the callee parameter
/// registers. AArch64 uses `x1`/`x2` for the first string and `x3`/`x4` for
/// the mask; x86_64 uses `rdi`/`rdx` and `rdi`/`rsi` respectively via a
/// push/pop register-pair protocol.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("rtrim()");

    if args.len() == 1 {
        super::args::emit_string_arg(&args[0], emitter, ctx, data);
        // -- strip whitespace from the right --
        abi::emit_call_label(emitter, "__rt_rtrim");                            // call the target-aware runtime helper that trims ASCII whitespace from the end of the current string slice
    } else {
        // -- rtrim with character mask --
        super::args::emit_string_arg(&args[0], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("str x1, [sp, #-16]!");                     // preserve the source string pointer while the trim-mask expression is evaluated
                emitter.instruction("str x2, [sp, #-16]!");                     // preserve the source string length while the trim-mask expression is evaluated
                super::args::emit_string_arg(&args[1], emitter, ctx, data);
                emitter.instruction("mov x3, x1");                              // move the trim-mask pointer into the secondary AArch64 trim-mask argument register pair
                emitter.instruction("mov x4, x2");                              // move the trim-mask length into the secondary AArch64 trim-mask argument register pair
                emitter.instruction("ldr x2, [sp], #16");                       // restore the source string length after evaluating the trim-mask expression
                emitter.instruction("ldr x1, [sp], #16");                       // restore the source string pointer after evaluating the trim-mask expression
            }
            Arch::X86_64 => {
                abi::emit_push_reg_pair(emitter, "rax", "rdx");                 // preserve the source string ptr/len while the trim-mask expression is evaluated on x86_64
                super::args::emit_string_arg(&args[1], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the trim-mask pointer into the secondary x86_64 trim-mask argument register
                emitter.instruction("mov rsi, rdx");                            // move the trim-mask length into the secondary x86_64 trim-mask argument register
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore the source string ptr/len after evaluating the trim-mask expression
            }
        }
        abi::emit_call_label(emitter, "__rt_rtrim_mask");                       // call the target-aware runtime helper that trims mask bytes from the end of the current string slice
    }

    Some(PhpType::Str)
}
