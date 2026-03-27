use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("pi()");
    let label = data.add_float(std::f64::consts::PI);
    emitter.instruction(&format!("adrp x9, {}@PAGE", label));                   // load page address of M_PI constant
    emitter.instruction(&format!("ldr d0, [x9, {}@PAGEOFF]", label));           // load M_PI into d0
    Some(PhpType::Float)
}
