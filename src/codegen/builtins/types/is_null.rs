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
    emitter.comment("is_null()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        // Mixed/Union values are boxed cells — peel nested mixed wrappers first
        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // normalize boxed mixed payloads to their concrete runtime tag
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #8");                              // runtime tag 8 = null
                emitter.instruction("cset x0, eq");                             // x0 = 1 if the unboxed payload is null, 0 otherwise
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 8");                              // runtime tag 8 = null
                emitter.instruction("sete al");                                 // set al when the unboxed payload is null
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
            }
        }
    } else {
        // Scalar types — check directly against the null sentinel
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("movz x9, #0xFFFE");                        // load null sentinel bits [15:0]
                emitter.instruction("movk x9, #0xFFFF, lsl #16");               // load null sentinel bits [31:16]
                emitter.instruction("movk x9, #0xFFFF, lsl #32");               // load null sentinel bits [47:32]
                emitter.instruction("movk x9, #0x7FFF, lsl #48");               // load null sentinel bits [63:48]
                emitter.instruction("cmp x0, x9");                              // compare value against null sentinel
                emitter.instruction("cset x0, eq");                             // x0 = 1 if value is null, 0 otherwise
            }
            Arch::X86_64 => {
                abi::emit_load_int_immediate(emitter, "r10", 0x7FFF_FFFF_FFFF_FFFEu64 as i64);
                emitter.instruction("cmp rax, r10");                            // compare value against the runtime null sentinel
                emitter.instruction("sete al");                                 // set al when the value is null
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
            }
        }
    }

    Some(PhpType::Bool)
}
