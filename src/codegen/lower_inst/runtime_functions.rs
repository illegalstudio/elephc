//! Purpose:
//! Routes typed EIR runtime-function IDs to bounded backend implementation groups.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_calls` for typed runtime operations.
//!
//! Key details:
//! - No PHP-name lookup participates in backend dispatch.
//! - Group files keep dispatch bounded while reusing the target-aware emitters.

use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{RuntimeFnId, Instruction};

mod group_00;
mod group_01;
mod group_02;
mod group_03;
mod group_04;
mod group_05;
mod group_06;
mod group_07;
mod group_08;
mod group_09;
mod group_10;
mod group_11;
mod group_12;

/// Lowers one typed runtime function through its backend implementation dispatcher.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Result<()> {
    if let Some(result) = group_00::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_01::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_02::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_03::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_04::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_05::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_06::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_07::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_08::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_09::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_10::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_11::lower(ctx, inst, target) {
        return result;
    }
    if let Some(result) = group_12::lower(ctx, inst, target) {
        return result;
    }
    Err(CodegenIrError::invalid_module(format!(
        "missing backend dispatch for typed builtin target {}",
        target.as_eir(),
    )))
}
