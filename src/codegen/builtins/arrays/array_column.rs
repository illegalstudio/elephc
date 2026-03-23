use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_column()");
    // -- evaluate array of assoc arrays --
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let val_ty = match &arr_ty {
        PhpType::Array(inner) => match inner.as_ref() {
            PhpType::AssocArray { value, .. } => *value.clone(),
            _ => PhpType::Str,
        },
        _ => PhpType::Str,
    };
    emitter.instruction("str x0, [sp, #-16]!");                                 // save outer array pointer
    // -- evaluate column key (string) --
    emit_expr(&args[1], emitter, ctx, data);
    // x1/x2 = column key string
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // save column key ptr/len
    // -- call runtime --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload outer array pointer
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload column key
    emitter.instruction("bl __rt_array_column");                                // extract column → x0=new array
    emitter.instruction("add sp, sp, #32");                                     // clean up stack

    Some(PhpType::Array(Box::new(val_ty)))
}
