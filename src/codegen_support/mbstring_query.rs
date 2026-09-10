//! Purpose:
//! Stages the shared native query policy used by AOT and eval mb_parse_str calls.
//!
//! Called from:
//! - Typed mbstring builtin lowering and the boxed eval runtime dispatcher.
//!
//! Key details:
//! - Both routes use the live Core INI provider and no SAPI-specific input filter.
//! - The caller retains aligned stack storage until the V5 coordinator retires deferred ownership.

use super::{abi, emit::Emitter, platform::Arch};
use elephc_builtin_contract::mbstring_abi::invoke::MbNativeQueryV1;

/// Stack-aligned storage for one invocation's native query policy and displaced output owner.
pub(crate) const STATE_BYTES: usize = (std::mem::size_of::<MbNativeQueryV1>() + 15) & !15;

/// Initializes untyped output publication and supplies the live query policy as the sixth C argument.
pub(crate) fn stage(emitter: &mut Emitter, offset: usize) {
    let target = emitter.target;
    let scratch = match target.arch { Arch::AArch64 => "x9", Arch::X86_64 => "r10" };
    abi::emit_load_int_immediate(emitter, scratch, 0);
    for field in [0, 8, 16, 32] {
        abi::emit_store_to_sp(emitter, scratch, offset + field);
    }
    abi::emit_extern_symbol_address(emitter, scratch, "elephc_mbstring_query_configuration_v1");
    abi::emit_store_to_sp(emitter, scratch, offset + 24);
    abi::emit_temporary_stack_address(emitter, abi::int_arg_reg_name(target, 5), offset);
}
