use super::super::abi;
use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use crate::parser::ast::Stmt;

pub(super) fn emit_include_once_mark(
    label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    data.add_comm(label.to_string(), 8);

    emitter.blank();
    emitter.comment("mark include/require file as loaded");
    mark_included(label, emitter);
}

pub(super) fn emit_include_once_guard(
    label: &str,
    body: &[Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    data.add_comm(label.to_string(), 8);

    let skip_label = ctx.next_label("include_once_skip");

    emitter.blank();
    emitter.comment("include_once/require_once guard");
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result_reg, label, 0);
    abi::emit_branch_if_int_result_nonzero(emitter, &skip_label);
    mark_included(label, emitter);

    for stmt in body {
        super::emit_stmt(stmt, emitter, ctx, data);
    }

    emitter.label(&skip_label);
}

fn mark_included(label: &str, emitter: &mut Emitter) {
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_load_int_immediate(emitter, result_reg, 1);
    abi::emit_store_reg_to_symbol(emitter, result_reg, label, 0);
}
