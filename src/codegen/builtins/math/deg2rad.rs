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
    emitter.comment("deg2rad()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        emitter.instruction("scvtf d0, x0");                                    // convert int to float
    }
    // -- multiply by M_PI / 180.0 to convert degrees to radians --
    let label = data.add_float(std::f64::consts::PI / 180.0);
    emitter.adrp("x9", &format!("{}", label));                   // load page address of conversion factor
    emitter.ldr_lo12("d1", "x9", &format!("{}", label));           // load M_PI/180 into d1
    emitter.instruction("fmul d0, d0, d1");                                     // multiply degrees by conversion factor
    Some(PhpType::Float)
}
