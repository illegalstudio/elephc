//! Purpose:
//! Lowers the two builtins behind `curl_setopt()`'s callback options:
//! `__elephc_curl_easy_set_callback($handle, $slot, $descriptor, $self, $adapter)` and
//! `__elephc_curl_adapter_addr()`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - THE `$self` OPERAND IS AN OBJECT AND IS PASSED AS A RAW POINTER, not boxed. Its
//!   static type is `Object("CurlHandle")` (the prelude hands `curl_setopt()`'s own
//!   `$handle` parameter straight down), and an object value's codegen representation
//!   already IS the pointer, so `load_value_to_reg` is the whole marshalling step. This
//!   mirrors `__elephc_callable_ptr`, which reinterprets a `Callable` the same way.
//!   Boxing it here would hand the bridge a temporary Mixed cell that dies at the end of
//!   the statement; the bridge needs the object itself, and only borrows it (see
//!   `crates/elephc-curl/src/callbacks.rs` for why the borrow is safe and why an owning
//!   reference would be a refcount cycle).
//! - Staging is the same stack dance the setopt lowerings document at length: unboxing
//!   the handle calls `__rt_mixed_unbox`, which clobbers every caller-saved register
//!   including argument registers already filled, and loading N operands straight into N
//!   argument registers is a parallel move that can read a register an earlier load
//!   already overwrote. Four values are staged, an even count, so the pushes leave the
//!   stack pointer where the ABI wants it.
//! - `__elephc_curl_adapter_addr()` reaches NO bridge symbol: it materializes the address
//!   of the runtime adapter `__rt_curl_invoke_callback` and nothing else, so unlike every
//!   other curl lowering it does not publish the bridge function-pointer table.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::{curl_arg_reg, ensure_curl_arg_count, load_handle_to_first_arg};

/// Lowers `__elephc_curl_easy_set_callback($handle, $slot, $descriptor, $self, $adapter)`.
///
/// Five C arguments: the handle id, the `crate::callbacks::SLOT_*` index, the normalized
/// callable's descriptor pointer (`0` to clear the slot), the `CurlHandle` object to pass
/// to PHP as `$ch`, and the address of `__rt_curl_invoke_callback`.
pub(crate) fn lower_curl_easy_set_callback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_set_callback", 5)?;
    let slot = super::super::super::expect_operand(inst, 1)?;
    let descriptor = super::super::super::expect_operand(inst, 2)?;
    let self_object = super::super::super::expect_operand(inst, 3)?;
    let adapter = super::super::super::expect_operand(inst, 4)?;

    // Stage in reverse argument order, so the pops below restore them in order.
    let scratch = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(adapter, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);
    ctx.load_value_to_reg(self_object, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);
    ctx.load_value_to_reg(descriptor, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);
    ctx.load_value_to_reg(slot, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);

    load_handle_to_first_arg(ctx, inst, 0, "curl_setopt")?;
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 1)); // C ABI slot = the callback slot index
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 2)); // C ABI descriptor = the callable record
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 3)); // C ABI self = the CurlHandle object
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 4)); // C ABI adapter = the runtime adapter

    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_easy_set_callback");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_adapter_addr()` — the address of the shared codegen callback
/// adapter, which the prelude hands to the bridge so no bridge extern ever declares a
/// `callable` parameter (the same decomposition `__elephc_pdo_adapter_addr` performs for
/// PDO's SQLite callbacks).
///
/// `__rt_curl_invoke_callback` is a raw runtime assembly label emitted verbatim by
/// `label_global`, NOT a C symbol, so it must be referenced without the leading-underscore
/// mangling Mach-O applies to extern C names. Its address is taken through the GOT because
/// the helper lives in the separately-assembled runtime object.
pub(crate) fn lower_curl_adapter_addr(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_adapter_addr", 0)?;
    abi::emit_extern_symbol_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "__rt_curl_invoke_callback",
    );
    store_if_result(ctx, inst)
}
