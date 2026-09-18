//! Purpose:
//! Tracks exception ownership and nested GC suppression throughout one deep heap release.
//!
//! Called from:
//! - Array, hash, Mixed, object, and callable cleanup, including coroutine storage branches.
//!
//! Key details:
//! - All participating cleanup frames reserve 64 bytes including frame linkage.
//! - Child cleanup returns normally; only the completed outer frame propagates its pending throw.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Locates the pending raw Throwable owner within the active target-specific cleanup frame.
fn pending_offset(emitter: &Emitter) -> usize {
    match emitter.target.arch { Arch::AArch64 => 16, Arch::X86_64 => 40 }
}

/// Locates the enclosing GC suppression value without disturbing existing object-loop slots.
fn suppression_offset(emitter: &Emitter) -> usize {
    match emitter.target.arch { Arch::AArch64 => 8, Arch::X86_64 => 48 }
}

/// Initializes pending ownership and suppresses collection without discarding an outer suppression.
pub(in crate::codegen_support::runtime) fn begin(emitter: &mut Emitter) {
    let result = abi::secondary_scratch_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, "_gc_release_suppressed", 0);
    abi::store_at_offset(emitter, result, suppression_offset(emitter));
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::store_at_offset(emitter, result, pending_offset(emitter));
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_store_reg_to_symbol(emitter, result, "_gc_release_suppressed", 0);
}

/// Invokes one potentially throwing unary cleanup while preserving this frame's pending owner.
pub(in crate::codegen_support::runtime) fn invoke(emitter: &mut Emitter, entry: &str, payload: &str) {
    super::super::exceptions::emit_guarded_cleanup_call(emitter, entry, payload, pending_offset(emitter));
}

/// Restores GC state and returns or propagates only after every child and the container are freed.
pub(in crate::codegen_support::runtime) fn finish(emitter: &mut Emitter, return_label: &str) {
    let result = abi::int_result_reg(emitter);
    let older = match emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
    abi::load_at_offset(emitter, result, suppression_offset(emitter));
    abi::emit_store_reg_to_symbol(emitter, result, "_gc_release_suppressed", 0);
    abi::load_at_offset(emitter, result, pending_offset(emitter));
    abi::emit_branch_if_int_result_zero(emitter, return_label);
    abi::emit_load_symbol_to_reg(emitter, older, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, pending_offset(emitter));
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label(return_label);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Each container reserves separate pending/suppression slots and frees itself before rethrowing.
    #[test]
    fn deep_cleanup_preserves_nested_state_and_defers_throw_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            abi::emit_frame_prologue(&mut emitter, 64);
            begin(&mut emitter);
            let payload = abi::int_result_reg(&emitter);
            invoke(&mut emitter, "__rt_decref_any", payload);
            invoke(&mut emitter, "__rt_callable_descriptor_release", payload);
            abi::emit_call_label(&mut emitter, "__rt_heap_free");
            finish(&mut emitter, "__rt_deep_cleanup_test_return");
            assert_ne!(pending_offset(&emitter), suppression_offset(&emitter), "{name}");
            let asm = emitter.output();
            assert_eq!(asm.matches("__rt_cleanup_invoke").count(), 2, "{name}");
            let first = asm.find("__rt_decref_any").unwrap();
            let second = asm.find("__rt_callable_descriptor_release").unwrap();
            let free = asm.find("__rt_heap_free").unwrap();
            let restore = asm.rfind("_gc_release_suppressed").unwrap();
            let chain = asm.find("__rt_exception_chain").unwrap();
            let throw = asm.find("__rt_throw_current").unwrap();
            assert!(first < second && second < free && free < restore, "{name}");
            assert!(restore < chain && chain < throw, "{name}");
            assert!(!asm.contains("setjmp"), "{name}: child boundaries own their jump buffers");
        }
    }
}
