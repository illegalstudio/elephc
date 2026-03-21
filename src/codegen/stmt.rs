use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use crate::parser::ast::Stmt;

pub fn emit_stmt(
    _stmt: &Stmt,
    _emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) {
    // TODO: implement in Phase 1-3
}
