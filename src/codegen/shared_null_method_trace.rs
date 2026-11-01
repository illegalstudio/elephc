//! Purpose:
//! Emits the module's one copy of the eval-bridge trace that records a method call on null, so
//! the two bridge calls behind every typed method call's null-receiver path exist once per
//! program instead of once per call site.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()` before the module's own functions.
//! - `crate::codegen::lower_inst::method_dispatch::guard_static_method_receiver()` and
//!   `crate::codegen::lower_inst::method_intrinsics::emit_method_call_on_null_fatal()`.
//!
//! Key details:
//! - Measured on the compiled Symfony `--web` app: 22 500 receiver guards, each carrying about
//!   seventeen instructions of cold trace setup (six argument loads and a bridge call, twice)
//!   before the already-shared throw. All of it is a function name, a method name, a line and
//!   the raw receiver word; the site becomes the address of one interned record, the receiver
//!   word and a call.
//! - The sequence is MOVED: the same two extern calls in the same order with the same arguments,
//!   the first one only for the guard variant (the record's last word says which).
//! - Only emitted when `eval_bridge` is on, since the bridge calls are the whole body.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::data_section::{DataSection, DataWord};
use crate::codegen::emit::Emitter;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::{Module, ValueId};
use crate::types::PhpType;

use super::shared_helper::emit_shared_helper;
use super::Result;

/// Label of the shared trace helper (record address in the int result register, raw receiver
/// word in the second argument register).
pub(super) const NULL_METHOD_TRACE_LABEL: &str = "_eir_shared_null_method_trace";

/// Byte offsets of the record the helper reads.
mod field {
    pub(super) const FUNCTION: usize = 0;
    pub(super) const FUNCTION_LEN: usize = 8;
    pub(super) const METHOD: usize = 16;
    pub(super) const METHOD_LEN: usize = 24;
    pub(super) const LINE: usize = 32;
    /// Nonzero when the raw receiver word is traced first.
    pub(super) const RAW: usize = 40;
}

/// Whether this module routes null-receiver traces through the shared helper.
///
/// Follows the shared static throw's gate (`--counters` / `--instrument` keep inlined forms) and
/// additionally needs the eval bridge, the only reason the traces exist.
pub(super) fn module_shares_null_method_trace(
    module: &Module,
    shared: &mut SharedCodegenState,
) -> bool {
    module.required_runtime_features.eval_bridge
        && super::shared_static_throw::module_shares_static_throw(module, shared)
}

/// Emits the trace for a null method receiver at the current site through the shared helper.
///
/// `raw_receiver` is the receiver whose raw word is traced first (the typed-call guard), or
/// `None` for the plain trace. Returns `false`, having emitted nothing, when this module does not
/// share the helper (or this is the helper's own body) and the caller keeps its inline form.
pub(in crate::codegen) fn emit_shared_null_method_trace(
    ctx: &mut FunctionContext<'_>,
    method_name: &str,
    raw_receiver: Option<ValueId>,
) -> Result<bool> {
    if ctx.function.name == NULL_METHOD_TRACE_LABEL {
        return Ok(false);
    }
    let module = ctx.module;
    if !module_shares_null_method_trace(module, ctx.shared) {
        return Ok(false);
    }
    let (function_label, function_len) = ctx.data.add_string(ctx.function.name.as_bytes());
    let (method_label, method_len) = ctx.data.add_string(method_name.as_bytes());
    let line = ctx
        .current_instruction_span()
        .map(|span| span.source_line() as u64)
        .unwrap_or_default();
    let record = ctx.data.add_words(vec![
        DataWord::Symbol(function_label),
        DataWord::U64(function_len as u64),
        DataWord::Symbol(method_label),
        DataWord::U64(method_len as u64),
        DataWord::U64(line),
        DataWord::U64(u64::from(raw_receiver.is_some())),
    ]);
    if let Some(receiver) = raw_receiver {
        let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
        ctx.load_value_to_reg(receiver, receiver_arg)?;
    }
    abi::emit_symbol_address(ctx.emitter, abi::int_result_reg(ctx.emitter), &record);
    abi::emit_call_label(ctx.emitter, NULL_METHOD_TRACE_LABEL);
    Ok(true)
}

/// Emits the shared helper when the module uses it.
pub(super) fn emit_shared_null_method_trace_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    if !module_shares_null_method_trace(module, shared) {
        return Ok(());
    }
    emit_shared_helper(
        module,
        emitter,
        data,
        shared,
        regalloc_linear,
        NULL_METHOD_TRACE_LABEL,
        PhpType::Void,
        &format!(
            "--- shared null-method-receiver trace: {} (record in the int result register, raw receiver word in the second argument register) ---",
            NULL_METHOD_TRACE_LABEL
        ),
        emit_trace_from_record,
    )
}

/// Runs the bridge traces the record in the int result register describes.
fn emit_trace_from_record(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let target = ctx.emitter.target;
    let record = abi::nested_call_reg(ctx.emitter);
    let result = abi::int_result_reg(ctx.emitter);
    let receiver_arg = abi::int_arg_reg_name(target, 1);
    let args: Vec<&str> = (0..6).map(|n| abi::int_arg_reg_name(target, n)).collect();
    let skip_raw = ctx.next_label("null_method_trace_plain");
    // Keep the record, then stage the raw receiver word in the sixth argument register before
    // the record loads overwrite the second.
    ctx.emitter.instruction(&format!("mov {}, {}", record, result)); // keep the record across both bridge calls
    ctx.emitter.instruction(&format!("mov {}, {}", args[5], receiver_arg)); // stage the raw receiver word for the raw trace
    let load_common = |ctx: &mut FunctionContext<'_>| {
        for (index, offset) in [
            field::FUNCTION,
            field::FUNCTION_LEN,
            field::METHOD,
            field::METHOD_LEN,
            field::LINE,
        ]
        .into_iter()
        .enumerate()
        {
            abi::emit_load_from_address(ctx.emitter, args[index], record, offset);
        }
    };
    abi::emit_load_from_address(ctx.emitter, result, record, field::RAW);
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", result, skip_raw)); // the plain trace has no raw receiver
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {0}, {0}", result)); // does this record ask for the raw trace?
            ctx.emitter.instruction(&format!("jz {}", skip_raw));       // the plain trace has no raw receiver
        }
    }
    load_common(ctx);
    let raw_symbol = target.extern_symbol("__elephc_eval_trace_aot_raw_null_method_receiver");
    abi::emit_call_label(ctx.emitter, &raw_symbol);
    ctx.emitter.label(&skip_raw);
    load_common(ctx);
    let plain_symbol = target.extern_symbol("__elephc_eval_trace_aot_null_method_receiver");
    abi::emit_call_label(ctx.emitter, &plain_symbol);
    Ok(())
}
