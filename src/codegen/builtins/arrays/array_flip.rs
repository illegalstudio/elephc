use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_flip()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let result_ty = match arr_ty {
        PhpType::Array(value) => PhpType::AssocArray {
            key: value,
            value: Box::new(PhpType::Int),
        },
        PhpType::AssocArray { key, value } => PhpType::AssocArray {
            key: value,
            value: key,
        },
        _ => PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Int),
        },
    };
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the source indexed array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_array_flip");                       // flip the indexed integer array into an associative array through the x86_64 runtime helper
        return Some(result_ty);
    }

    // -- call runtime to swap keys and values --
    emitter.instruction("bl __rt_array_flip");                                  // call runtime: flip array → x0=new assoc array

    Some(result_ty)
}
