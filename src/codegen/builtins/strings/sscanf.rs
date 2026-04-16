use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
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
    emitter.comment("sscanf()");
    // sscanf($string, $format) → returns array of matched values as strings
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the input string while the format string expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the format pointer into the secondary runtime string-argument pair
            emitter.instruction("mov x4, x2");                                  // move the format length into the secondary runtime string-argument pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the input string into the primary runtime string-argument pair
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // push the input string while the format string expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the format pointer into the secondary x86_64 runtime string-argument pair
            emitter.instruction("mov rsi, rdx");                                // move the format length into the secondary x86_64 runtime string-argument pair
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the input string into the primary x86_64 runtime string-argument pair
        }
    }
    abi::emit_call_label(emitter, "__rt_sscanf");                               // parse the input string according to the format string through the target-aware runtime helper
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
