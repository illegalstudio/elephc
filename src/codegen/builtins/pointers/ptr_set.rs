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
    emitter.comment("ptr_set() — write value at pointer address");
    // -- evaluate pointer --
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");                    // abort with fatal error on null pointer dereference before writing through the pointer
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the validated destination pointer while the stored value expression is evaluated

    // -- evaluate value to write --
    emit_expr(&args[1], emitter, ctx, data);

    // -- store value at pointer address --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // copy the stored integer payload into a scratch register before restoring the destination pointer on AArch64
            abi::emit_pop_reg(emitter, "x0");                                   // restore the validated destination pointer after evaluating the stored value on AArch64
            emitter.instruction("str x1, [x0]");                                // store one machine-word integer payload through the validated pointer on AArch64
        }
        Arch::X86_64 => {
            emitter.instruction("mov rcx, rax");                                // copy the stored integer payload into a scratch register before restoring the destination pointer on x86_64
            abi::emit_pop_reg(emitter, "rax");                                  // restore the validated destination pointer after evaluating the stored value on x86_64
            emitter.instruction("mov QWORD PTR [rax], rcx");                    // store one machine-word integer payload through the validated pointer on x86_64
        }
    }
    Some(PhpType::Void)
}
