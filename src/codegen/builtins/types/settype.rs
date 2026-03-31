use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("settype()");
    if let crate::parser::ast::ExprKind::Variable(vname) = &args[0].kind {
        if let crate::parser::ast::ExprKind::StringLiteral(type_name) = &args[1].kind {
            let var = ctx.variables.get(vname).expect("undefined variable");
            let offset = var.stack_offset;
            let old_ty = var.ty.clone();
            crate::codegen::abi::emit_load(emitter, &old_ty, offset);
            let new_ty = match type_name.as_str() {
                "int" | "integer" => {
                    // -- convert value to integer --
                    match &old_ty {
                        PhpType::Float => { emitter.instruction("fcvtzs x0, d0"); } //convert float to signed int (truncate toward zero)
                        PhpType::Bool | PhpType::Int => {}
                        _ => { emitter.instruction("mov x0, #0"); }             // unsupported types become 0
                    }
                    PhpType::Int
                }
                "float" | "double" => {
                    // -- convert value to float --
                    match &old_ty {
                        PhpType::Float => {}
                        _ => { emitter.instruction("scvtf d0, x0"); }           // convert signed int/bool to float
                    }
                    PhpType::Float
                }
                "string" => {
                    crate::codegen::expr::coerce_to_string(emitter, ctx, _data, &old_ty);
                    PhpType::Str
                }
                "bool" | "boolean" => {
                    // -- convert value to boolean --
                    crate::codegen::expr::coerce_null_to_zero(emitter, &old_ty);
                    emitter.instruction("cmp x0, #0");                          // compare value against zero
                    emitter.instruction("cset x0, ne");                         // x0 = 1 if truthy, 0 if falsy
                    PhpType::Bool
                }
                _ => old_ty.clone(),
            };
            crate::codegen::abi::emit_store(emitter, &new_ty, offset);
            ctx.update_var_type_and_ownership(
                vname,
                new_ty.clone(),
                HeapOwnership::local_owner_for_type(&new_ty),
            );
        }
    }
    // -- settype() always returns true --
    emitter.instruction("mov x0, #1");                                          // return true (settype always succeeds)
    Some(PhpType::Bool)
}
