//! Purpose:
//! Lowers include and require statements using resolver-produced labels and active variant symbols.
//! Marks loaded files and invokes include bodies at the correct PHP-observable point.
//!
//! Called from:
//! - `crate::codegen::stmt`
//!
//! Key details:
//! - Include state controls function variant activation and must preserve PHP load-order semantics.

use super::super::abi;
use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use crate::names::{function_symbol, function_variant_active_symbol};
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

pub(super) fn emit_function_variant_mark(
    name: &str,
    variant: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let active_symbol = function_variant_active_symbol(name);
    data.add_comm(active_symbol.clone(), 8);

    emitter.blank();
    emitter.comment(&format!("activate include-loaded function variant for {}", name));
    let variant_reg = abi::temp_int_reg(emitter.target);
    abi::emit_symbol_address(emitter, variant_reg, &function_symbol(variant));
    abi::emit_store_reg_to_symbol(emitter, variant_reg, &active_symbol, 0);
}

fn mark_included(label: &str, emitter: &mut Emitter) {
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_load_int_immediate(emitter, result_reg, 1);
    abi::emit_store_reg_to_symbol(emitter, result_reg, label, 0);
}
