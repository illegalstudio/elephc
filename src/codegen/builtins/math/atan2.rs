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
    emitter.comment("atan2()");
    // -- evaluate y (first arg) --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    if t0 != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize the atan2() y operand into the active floating-point result register before it is preserved
    }
    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));          // preserve the floating atan2() y operand while the x operand expression is evaluated
    // -- evaluate x (second arg) --
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    if t1 != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize the atan2() x operand into the active floating-point result register before the libc call
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fmov d1, d0");                                 // move the floating atan2() x operand into the second AArch64 floating-point argument register
            abi::emit_pop_float_reg(emitter, "d0");                             // restore the floating atan2() y operand into the first AArch64 floating-point argument register
            emitter.bl_c("atan2");                                              // delegate atan2(y, x) to libc on AArch64
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(emitter, "xmm1");                           // restore the floating atan2() y operand into a scratch floating-point register before ordering the SysV libc arguments
            emitter.instruction("movapd xmm2, xmm0");                           // preserve the floating atan2() x operand while the y operand is moved into the first SysV floating-point argument register
            emitter.instruction("movapd xmm0, xmm1");                           // move the floating atan2() y operand into the first SysV floating-point argument register
            emitter.instruction("movapd xmm1, xmm2");                           // move the floating atan2() x operand into the second SysV floating-point argument register
            emitter.instruction("call atan2");                                  // delegate atan2(y, x) to libc on linux-x86_64
        }
    }
    Some(PhpType::Float)
}
