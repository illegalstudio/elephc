//! Purpose:
//! Protects public serialize calls and their reference registries across nested hooks.
//!
//! Called from:
//! - The serializer emitter and EIR serialize builtin lowering.
//!
//! Key details:
//! - Legacy callbacks share references; other calls snapshot the populated registry on the stack.
//! - A guarded dispatch restores state before propagating a callback exception.
//! - Each call owns a growable output stream (`_ser_stream`, see `stream.rs`) whose descriptor
//!   lives in this frame, so a `serialize()` inside a hook gets its own stream. The enclosing
//!   stream is reinstated on every exit, and a grown buffer is freed when a hook throws.

use elephc_memoir::values::serialize_registry;
use elephc_memoir::{FrameOffset, strings::scoped_stream::StreamScope, values::stack_registry_snapshot::StackRegistrySnapshot};

use crate::codegen_support::{abi, emit::Emitter};
use crate::codegen_support::runtime::exceptions::emit_guarded_cleanup_call;

const FRAME: usize = 144;
const PENDING: usize = 8;
const LEGACY: usize = 16;
const COUNTER: usize = 24;
const COUNT: usize = 32;
const SNAPSHOT: usize = 40;
const LENGTH: usize = 48;
const POINTER: usize = 56;
const HIGH: usize = 64;
const LOW: usize = 72;
const TAG: usize = 80;
const STREAM: usize = 120; // 32-byte `__rt_strbuf_*` descriptor owned by this call (spans 120 down to 89)
const OUTER: usize = 128; // the enclosing call's `_ser_stream`, reinstated on every exit

const REGISTRY: StackRegistrySnapshot<'static> = StackRegistrySnapshot {
    pointers: serialize_registry::POINTERS.symbol,
    indices: serialize_registry::INDICES.symbol,
    count: FrameOffset::new(COUNT),
    saved: FrameOffset::new(SNAPSHOT),
    label_prefix: "__rt_serialize_context",
};

const OUTPUT_STREAM: StreamScope<'static> = StreamScope {
    active: super::stream::ACTIVE_STREAM,
    saved: FrameOffset::new(OUTER),
};

/// Emits an exception-safe public entry and its unary guarded dispatch adapter.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    // -- preserve public operands and select shared or isolated references --
    emitter.blank();
    emitter.label_global("__rt_serialize_call_value");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [TAG, LOW, HIGH].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_store_zero_to_local_slot(emitter, PENDING);
    // -- open this call's output stream at the scratch tail and install it as the active one --
    OUTPUT_STREAM.emit_enter(emitter, FrameOffset::new(STREAM));
    abi::emit_load_symbol_to_reg(emitter, result, "_ser_legacy_active", 0);
    abi::store_at_offset(emitter, result, LEGACY);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_serialize_context_ready");
    for (symbol, offset) in [("_ser_value_counter", COUNTER), (serialize_registry::COUNT, COUNT)] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    REGISTRY.emit_copy(emitter, false);
    abi::emit_call_label(emitter, "__rt_serialize_begin");
    emitter.label("__rt_serialize_context_ready");
    // -- guard callback execution, then restore state before returning or rethrowing --
    abi::emit_frame_slot_address(emitter, result, TAG);
    emit_guarded_cleanup_call(emitter, "__rt_serialize_guarded_dispatch", result, PENDING);
    // -- publish the finished stream, or free its grown buffer when a hook threw, then reinstate
    //    the enclosing stream --
    OUTPUT_STREAM.emit_close(
        emitter, FrameOffset::new(STREAM), FrameOffset::new(PENDING),
        [FrameOffset::new(POINTER), FrameOffset::new(LENGTH)],
        "__rt_serialize_stream_finish", "__rt_serialize_stream_closed",
    );
    abi::load_at_offset(emitter, result, LEGACY);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_serialize_context_restored");
    REGISTRY.emit_copy(emitter, true);
    for (symbol, offset) in [("_ser_value_counter", COUNTER), (serialize_registry::COUNT, COUNT)] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
    emitter.label("__rt_serialize_context_restored");
    abi::load_at_offset(emitter, result, LEGACY);
    abi::emit_store_reg_to_symbol(emitter, result, "_ser_legacy_active", 0);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_serialize_context_return");
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label("__rt_serialize_context_return");
    let (pointer, length) = abi::string_result_regs(emitter);
    abi::load_at_offset(emitter, pointer, POINTER);
    abi::load_at_offset(emitter, length, LENGTH);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);

    // -- adapt the protected unary callback to the raw serializer ABI --
    emitter.blank();
    emitter.label_global("__rt_serialize_guarded_dispatch");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::load_at_offset(emitter, scratch, 8);
    for (index, offset) in [0, 8, 16].into_iter().enumerate() {
        abi::emit_load_from_address(emitter, abi::int_arg_reg_name(emitter.target, index), scratch, offset);
    }
    abi::emit_call_label(emitter, "__rt_serialize_value");
    abi::load_at_offset(emitter, scratch, 8);
    let (pointer, length) = abi::string_result_regs(emitter);
    abi::emit_store_to_address(emitter, pointer, scratch, TAG - POINTER);
    abi::emit_store_to_address(emitter, length, scratch, TAG - LENGTH);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The public entry opens its own stream before the guarded dispatch, then either publishes
    /// the finished slice as an owned string or frees the grown buffer when a hook threw, and
    /// reinstates the enclosing stream on both exits, on every supported target.
    #[test]
    fn public_entry_owns_and_releases_its_output_stream() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            let (entry, _) = asm.split_once("__rt_serialize_guarded_dispatch:").unwrap();
            let init = entry.find("__rt_strbuf_init").unwrap();
            let dispatch = entry.find("__rt_cleanup_invoke").unwrap();
            let release = entry.find("__rt_strbuf_release").unwrap();
            let finish = entry.find("__rt_strbuf_finish_owned").unwrap();
            let closed = entry.find("__rt_serialize_stream_closed:").unwrap();
            assert!(init < dispatch && dispatch < release && release < finish && finish < closed, "{name}");
            // Three accesses: save the outer stream, install this one, reinstate the outer one.
            assert!(entry.matches("ser_stream").count() >= 3, "{name}: save, install, and reinstate the outer stream");
        }
    }
}
