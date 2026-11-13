//! Purpose:
//! Protects native temporaries allocated while adapting PHP calls.
//!
//! Called from:
//! - Callable lowering after normalizing and saving the argument container.
//! - Dynamic constructor argument materialization after creating caller-owned conversions.
//!
//! Key details:
//! - The argument box and optional descriptor already have one caller-owned reference.
//! - Stack records transfer those references only during exceptional unwinding.
//! - Conversion guards are published immediately so a later argument conversion may throw safely.
//! - Failed dynamic construction marks the raw object as destructor-suppressed before release.

use super::*;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::try_handlers::EXCEPTION_GUARD_SLOT_SIZE;

/// Protects a temporary boxed value saved in the stack until its operation returns.
pub(super) fn guard_saved_owner(emitter: &mut Emitter, record: usize, owner: usize) {
    register(emitter, record, owner, "__rt_exception_release_owned");
}

/// Removes a saved-owner record while leaving its owner for normal cleanup.
pub(super) fn unguard_saved_owner(emitter: &mut Emitter, guard_offset: usize) {
    abi::emit_unguard_owned_at_stack(emitter, guard_offset);
}

/// Guards a raw object until its dynamic constructor completes successfully.
pub(super) fn begin_unconstructed_object(emitter: &mut Emitter) -> usize {
    let bytes = EXCEPTION_GUARD_SLOT_SIZE;
    abi::emit_reserve_temporary_stack(emitter, bytes);
    register(
        emitter,
        0,
        bytes,
        "__rt_exception_release_unconstructed_object",
    );
    bytes
}

/// Removes a raw-object construction guard after the constructor returns normally.
pub(super) fn end_unconstructed_object(emitter: &mut Emitter, bytes: usize) {
    abi::emit_temporary_stack_address(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        0,
    );
    abi::emit_call_label(emitter, "__rt_exception_unguard_owned");
    abi::emit_release_temporary_stack(emitter, bytes);
}

/// Publishes one cleanup record for an owner already preserved in the temporary stack.
fn register(emitter: &mut Emitter, record: usize, owner: usize, cleanup: &str) {
    let scratch = abi::temp_int_reg(emitter.target);
    abi::emit_load_temporary_stack_slot(emitter, scratch, owner);
    abi::emit_guard_owned_at_stack(emitter, scratch, cleanup, record);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Saved guards keep record and owner offsets distinct on every supported ABI.
    #[test]
    fn saved_owner_guard_uses_record_then_owner_offsets_on_all_targets() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            guard_saved_owner(&mut emitter, 96, 16);
            let asm = emitter.output();
            let offsets = if target == "linux-x86_64" {
                ["[rsp + 16]", "[rsp + 120]"]
            } else { ["[sp, #16]", "[sp, #120]"] };
            for offset in offsets { assert!(asm.contains(offset), "{target}: {asm}"); }
            assert!(asm.contains("__rt_exception_guard_owned"), "{target}: {asm}");
        }
    }
}
