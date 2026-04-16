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
    emitter.comment("ucfirst()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- copy string then uppercase the first character --
    abi::emit_call_label(emitter, "__rt_strcopy");                              // copy the source string into concat storage before mutating its first byte in place
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x2, 1f");                                  // skip the ASCII-case tweak when ucfirst() receives an empty string
            emitter.instruction("ldrb w9, [x1]");                               // load the first byte of the copied string so ucfirst() can classify its ASCII case
            emitter.instruction("cmp w9, #97");                                 // compare the copied first byte against 'a' to detect lowercase ASCII input
            emitter.instruction("b.lt 1f");                                     // leave bytes below 'a' unchanged because they are not lowercase ASCII letters
            emitter.instruction("cmp w9, #122");                                // compare the copied first byte against 'z' to bound the lowercase ASCII range
            emitter.instruction("b.gt 1f");                                     // leave bytes above 'z' unchanged because they are not lowercase ASCII letters
            emitter.instruction("sub w9, w9, #32");                             // convert lowercase ASCII to uppercase by subtracting the standard ASCII case delta
            emitter.instruction("strb w9, [x1]");                               // store the uppercased first byte back into the copied string in concat storage
            emitter.raw("1:");
        }
        Arch::X86_64 => {
            emitter.instruction("test rdx, rdx");                               // skip the ASCII-case tweak when ucfirst() receives an empty string
            emitter.instruction("jz 1f");                                       // leave empty strings unchanged because there is no first byte to uppercase
            emitter.instruction("movzx ecx, BYTE PTR [rax]");                   // load the first byte of the copied string so ucfirst() can classify its ASCII case
            emitter.instruction("cmp cl, 97");                                  // compare the copied first byte against 'a' to detect lowercase ASCII input
            emitter.instruction("jb 1f");                                       // leave bytes below 'a' unchanged because they are not lowercase ASCII letters
            emitter.instruction("cmp cl, 122");                                 // compare the copied first byte against 'z' to bound the lowercase ASCII range
            emitter.instruction("ja 1f");                                       // leave bytes above 'z' unchanged because they are not lowercase ASCII letters
            emitter.instruction("sub cl, 32");                                  // convert lowercase ASCII to uppercase by subtracting the standard ASCII case delta
            emitter.instruction("mov BYTE PTR [rax], cl");                      // store the uppercased first byte back into the copied string in concat storage
            emitter.raw("1:");
        }
    }

    Some(PhpType::Str)
}
