//! Purpose:
//! Lowers scoped constant reads that remain dynamic at EIR codegen time.
//! Currently covers enum case singleton loads for Phase 04 parity.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Enum cases live in global singleton slots that are filled LAZILY, on the first
//!   evaluation of the case, by `crate::codegen::enum_singletons`. The load result
//!   is an object pointer, and every read of the same case returns the same one.

use crate::codegen::abi;
use crate::codegen_support;
use crate::ir::Instruction;

use super::super::context::FunctionContext;
use super::{builtins, expect_data, store_if_result};
use crate::codegen::{CodegenIrError, Result};

/// Lowers a scoped enum-case read into the current object pointer result register.
pub(super) fn lower_scoped_constant_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (enum_name, case_name) = scoped_constant_label(ctx, inst)?;
    let class_name = enum_name.to_string();
    let constant_name = case_name.to_string();
    if let Some(enum_info) = ctx.module.enum_infos.get(class_name.as_str()) {
        if enum_info
            .cases
            .iter()
            .any(|case| case.name == constant_name.as_str())
        {
            crate::codegen::enum_singletons::emit_lazy_case_load(
                ctx,
                &class_name,
                &constant_name,
            );
            // A case read hands out an OWNED reference, exactly like `cases()` and
            // `from()`/`tryFrom()` already do (see `enums::emit_load_enum_case_singleton`
            // and issue #349). Without this incref the singleton's refcount drifts
            // down by one per read — the consumer's destination acquires the value
            // and then releases the temporary — so a case passed into a typed
            // parameter or returned from a typed function is FREED while its global
            // slot still points at it.
            //
            // That under-retention predates lazy materialization; eager creation
            // merely hid the consequence, because a freed case object's memory was
            // not handed to another case. Lazily, the next case materialized after
            // the free reuses the very block that was released, so both slots end up
            // pointing at ONE object and `E::A === E::B` starts returning true. The
            // failing shape was `f(D::Ascending)` through a typed parameter followed
            // by a `D::Descending` read.
            //
            // The enum lowering marks this reference Owned, so ordinary temporary
            // retirement balances this retain while the singleton slot keeps its
            // own reference until terminal script cleanup.
            abi::emit_incref_if_refcounted(
                ctx.emitter,
                &crate::types::PhpType::Object(class_name.clone()),
            );
            return store_if_result(ctx, inst);
        }
    }
    if builtins::has_eval_context(ctx) {
        return builtins::lower_eval_class_constant_fetch(ctx, inst, &class_name, &constant_name);
    }
    emit_unknown_class_fatal(ctx, &class_name);
    store_if_result(ctx, inst)
}

/// Emits PHP's runtime fatal for a scoped constant whose receiver class was unavailable during
/// closed-world compilation, preserving lazy class resolution for paths that never execute.
fn emit_unknown_class_fatal(ctx: &mut FunctionContext<'_>, class_name: &str) {
    let message = format!("Fatal error: Uncaught Error: Class \"{}\" not found\n", class_name);
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    codegen_support::emit_write_literal_stderr(ctx.emitter, &message_label, message_len);
    abi::emit_exit(ctx.emitter, 1);
}

/// Resolves the string immediate `Enum::Case` attached to a scoped constant read.
fn scoped_constant_label<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Result<(&'a str, &'a str)> {
    let data = expect_data(inst)?;
    let label = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    let (enum_name, case_name) = label.rsplit_once("::").ok_or_else(|| {
        CodegenIrError::invalid_module(format!("invalid scoped constant label '{}'", label))
    })?;
    Ok((enum_name.trim_start_matches('\\'), case_name))
}
