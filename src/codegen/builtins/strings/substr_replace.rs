//! Purpose:
//! Emits PHP `substr_replace` string transformation or formatting calls.
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

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("substr_replace()");
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the subject string while the replacement, offset, and optional length are evaluated
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the replacement string while the offset and optional length are evaluated
            super::args::push_int_arg(&args[2], emitter, ctx, data);
            if args.len() >= 4 {
                super::args::emit_int_arg(&args[3], emitter, ctx, data);
                emitter.instruction("mov x7, x0");                              // move the optional replacement length into the scalar runtime argument register
            } else {
                emitter.instruction("mov x7, #-1");                             // set sentinel -1 so the runtime replaces through the end of the subject string
            }
            emitter.instruction("ldr x0, [sp], #16");                           // restore the replacement offset after evaluating the optional length argument
            emitter.instruction("ldp x3, x4, [sp], #16");                       // restore the replacement string into the secondary runtime string-argument pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the subject string into the primary runtime string-argument pair
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // push the subject string while the replacement, offset, and optional length are evaluated
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // push the replacement string while the offset and optional length are evaluated
            super::args::push_int_arg(&args[2], emitter, ctx, data);
            if args.len() >= 4 {
                super::args::emit_int_arg(&args[3], emitter, ctx, data);
                emitter.instruction("mov r8, rax");                             // move the optional replacement length into the scalar x86_64 runtime argument register
            } else {
                abi::emit_load_int_immediate(emitter, "r8", -1);                // set sentinel -1 so the runtime replaces through the end of the subject string
            }
            abi::emit_pop_reg(emitter, "rcx");                                  // restore the replacement offset after evaluating the optional length argument
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the replacement string into the secondary x86_64 runtime string-argument pair
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the subject string into the primary x86_64 runtime string-argument pair
        }
    }
    abi::emit_call_label(emitter, "__rt_substr_replace");                       // replace the requested subject substring through the target-aware runtime helper
    Some(PhpType::Str)
}
