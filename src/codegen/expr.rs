use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use crate::parser::ast::Expr;

/// After emit_expr, the result is available as:
/// - Strings: x1 = pointer, x2 = length
/// - Integers: x0 = value
pub fn emit_expr(
    expr: &Expr,
    emitter: &mut Emitter,
    _ctx: &mut Context,
    data: &mut DataSection,
) {
    match expr {
        Expr::StringLiteral(s) => {
            let bytes = s.as_bytes();
            let (label, len) = data.add_string(bytes);
            emitter.comment(&format!("load string \"{}\"", s.escape_default()));
            emitter.instruction(&format!("adrp x1, {}@PAGE", label));
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));
            emitter.instruction(&format!("mov x2, #{}", len));
        }
        _ => {
            // TODO: implement remaining expressions in Phase 2-3
        }
    }
}
