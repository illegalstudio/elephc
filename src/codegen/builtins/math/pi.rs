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
    emitter.adrp("x9", &format!("{}", label));                   // load page address of M_PI constant
    emitter.ldr_lo12("d0", "x9", &format!("{}", label));           // load M_PI into d0
    Some(PhpType::Float)
}
