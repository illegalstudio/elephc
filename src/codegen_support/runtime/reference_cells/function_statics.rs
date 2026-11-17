//! Purpose:
//! Detaches persistent function static owners before their final cleanup can execute PHP.
//!
//! Called from:
//! - Process epilogues, request resets and application shutdown emission.
//!
//! Key details:
//! - The returned cell keeps its owner until the caller selects its cleanup boundary.
//! - Semantic source records borrow storage and retire independently of PHP ownership.

use crate::codegen_support::{abi, data_section::StaticLocalRecord, emit::Emitter};

/// Clears persistent storage and returns its displaced managed owner in the result register.
pub(crate) fn detach(emitter: &mut Emitter, record: &StaticLocalRecord, native_validation: bool) {
    let result = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, &record.symbol, 0);
    abi::emit_store_zero_to_symbol(emitter, &record.symbol, 0);
    abi::emit_store_zero_to_symbol(emitter, &record.symbol, 8);
    abi::emit_store_zero_to_symbol(emitter, &record.init_symbol, 0);
    if native_validation {
        abi::emit_push_reg(emitter, result);
        abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 0), &record.symbol);
        abi::emit_call_label(emitter, "__rt_reference_sources_unbind_local");
        abi::emit_pop_reg(emitter, result);
    }
}
