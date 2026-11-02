//! Purpose:
//! Contains a declaration validator's native Throwable before returning through Rust.
//!
//! Called from:
//! - Native reference validation operation tables and the executable ABI regression harness.
//!
//! Key details:
//! - Context words are caller flags and an independently owned pending raw Throwable.
//! - A validator descriptor's first word is a C-ABI entry(context, original RHS).
//! - Entries return an owned box, without PHP hidden collectors or scalar return markers.
//! - Only a successful validator transfers one owned Mixed candidate to its output slot.

use super::*;

/// Minimum context storage: caller flags followed by the owned contained Throwable.
pub(crate) const CONTEXT_BYTES: usize = 16;
/// Offset of the contained native Throwable, separate from the caller's active exception.
pub(crate) const PENDING_THROWABLE: usize = CONTEXT_BYTES - 8;

const FRAME: usize = 96;
const ENTRY: usize = 64;
const CONTEXT: usize = 56;
const ORIGINAL: usize = 48;
const OUTPUT: usize = 40;

/// Emits coerce(context, descriptor, original, output), returning success0, failure1 or Throwable2.
/// Context and output must remain alive through re-entry; each selected declaration runs once.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let failed = "__rt_reference_validation_coerce_failed";
    let pending = "__rt_reference_validation_coerce_pending";
    let done = "__rt_reference_validation_coerce_done";
    emitter.blank();
    emitter.label_global("__rt_reference_validation_coerce");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [CONTEXT, ENTRY, ORIGINAL, OUTPUT].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    // Initialize the candidate before inspecting any callback metadata or pending exception.
    abi::load_at_offset(emitter, result, OUTPUT);
    abi::emit_branch_if_int_result_zero(emitter, failed);
    abi::emit_store_zero_to_address(emitter, result, 0);
    abi::load_at_offset(emitter, result, CONTEXT);
    abi::emit_branch_if_int_result_zero(emitter, failed);
    abi::emit_load_from_address(emitter, result, result, PENDING_THROWABLE);
    abi::emit_branch_if_int_result_nonzero(emitter, pending);
    abi::load_at_offset(emitter, result, ENTRY);
    abi::emit_branch_if_int_result_zero(emitter, failed);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_branch_if_int_result_zero(emitter, failed);
    abi::store_at_offset(emitter, result, ENTRY);
    abi::load_at_offset(emitter, result, ORIGINAL);
    abi::emit_branch_if_int_result_zero(emitter, failed);
    abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 0),
        "__rt_reference_validation_coerce_body");
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 1), ENTRY);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 2), CONTEXT);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("add x2, x2, #8");                              // contain a new Throwable in this validation's owned context slot
        }
        Arch::X86_64 => {
            emitter.instruction("add rdx, 8");                                  // contain a new Throwable in this validation's owned context slot
        }
    }
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    abi::emit_branch_if_int_result_nonzero(emitter, pending);
    abi::load_at_offset(emitter, result, OUTPUT);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_branch_if_int_result_zero(emitter, failed);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_jump(emitter, done);
    emitter.label(failed);
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_jump(emitter, done);
    emitter.label(pending);
    abi::emit_load_int_immediate(emitter, result, 2);
    emitter.label(done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);

    emit_body(emitter);
}

/// Calls the declaration only inside cleanup_invoke, then publishes its successful candidate.
fn emit_body(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let (record, entry) = if emitter.target.arch == Arch::AArch64 { ("x11", "x10") }
        else { ("r11", "r10") };
    emitter.label("__rt_reference_validation_coerce_body");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::emit_reg_move(emitter, record, abi::int_arg_reg_name(emitter.target, 0));
    abi::emit_load_from_address(emitter, entry, record, 0);
    abi::emit_load_from_address(emitter, abi::int_arg_reg_name(emitter.target, 1), record, 16);
    abi::emit_load_from_address(emitter, abi::int_arg_reg_name(emitter.target, 0), record, 8);
    abi::emit_call_reg(emitter, entry);
    abi::load_at_offset(emitter, record, 8);
    abi::emit_load_from_address(emitter, entry, record, 24);
    abi::emit_store_to_address(emitter, result, entry, 0);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// No declaration runs outside the native exception boundary on any supported ABI.
    #[test]
    fn native_reference_coercion_contains_throws_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            let (outer, body) = assembly.split_once("__rt_reference_validation_coerce_body:\n").unwrap();
            assert!(outer.contains("__rt_cleanup_invoke"), "{name}: contain PHP before returning through Rust");
            assert!(!outer.contains("blr ") && !outer.contains("call r10"), "{name}: the outer C entry cannot invoke the declaration directly");
            assert!(body.contains(if name == "linux-x86_64" { "call r10" } else { "blr x10" }), "{name}: invoke exactly inside the protected body");
            assert!(!assembly.contains("__rt_throw_current"), "{name}: failures return a status and owned context Throwable");
        }
    }
}
