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
    emitter.comment("is_infinite()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    // -- check if |value| equals infinity --
    if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }            // convert int to float if needed
    emitter.instruction("fabs d0, d0");                                         // take absolute value (catches +/- inf)
    let inf_label = data.add_float(f64::INFINITY);
    emitter.adrp("x9", &format!("{}", inf_label));               // load page address of infinity constant
    emitter.add_lo12("x9", "x9", &format!("{}", inf_label));         // add page offset of infinity constant
    emitter.instruction("ldr d1, [x9]");                                        // load infinity value into d1
    emitter.instruction("fcmp d0, d1");                                         // compare |value| against infinity
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if value is infinite, 0 otherwise
    Some(PhpType::Bool)
}
