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
    emitter.comment("rad2deg()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        emitter.instruction("scvtf d0, x0");                                    // convert int to float
    }
    // -- multiply by 180.0 / M_PI to convert radians to degrees --
    let label = data.add_float(180.0 / std::f64::consts::PI);
    emitter.adrp("x9", &format!("{}", label));                   // load page address of conversion factor
    emitter.ldr_lo12("d1", "x9", &format!("{}", label));           // load 180/M_PI into d1
    emitter.instruction("fmul d0, d0, d1");                                     // multiply radians by conversion factor
    Some(PhpType::Float)
}
