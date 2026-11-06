//! Purpose:
//! Roots unfinished decoder containers in the existing exception cleanup chain.
//!
//! Called from:
//! - Target-specific recursive unserialize parsers and hydration-hook adaptation.
//!
//! Key details:
//! - Slots own the initial reference until normal transfer or exception cleanup.
//! - Stack records never retain values and unlink in reverse publication order.
//! - Hash shadows distinguish owning containers from the parser's reused raw-pointer scratch slot.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Publishes the three zeroed owner slots in the parser's preallocated frame.
pub(super) fn emit_enter(emitter: &mut Emitter) {
    let (slots, records, owner) = match emitter.target.arch {
        Arch::AArch64 => ([176, 168, 160], [128, 176, 224], "x11"),
        Arch::X86_64 => ([96, 104, 112], [0, 48, 96], "r11"),
    };
    for (slot, record) in slots.into_iter().zip(records) {
        abi::emit_store_zero_to_local_slot(emitter, slot);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("sub {owner}, x29, #{slot}"));     // address the parser's owning slot
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("lea {owner}, [rbp - {slot}]"));   // address the parser's owning slot
            }
        }
        abi::emit_link_call_operand_owner_at_stack(emitter, owner, false, record);
    }
}

/// Transfers completed containers to the caller before detaching their empty records.
pub(super) fn emit_leave(emitter: &mut Emitter) {
    let (slots, records) = match emitter.target.arch {
        Arch::AArch64 => ([176, 168, 160], [224, 176, 128]),
        Arch::X86_64 => ([96, 104, 112], [96, 48, 0]),
    };
    for slot in slots { abi::emit_store_zero_to_local_slot(emitter, slot); }
    for record in records { abi::emit_unlink_call_operand_owner_at_stack(emitter, record); }
}

/// Retires the raw-data owner after the context has acquired the adapted hook argument.
pub(super) fn emit_transfer_data(emitter: &mut Emitter) {
    let slot = match emitter.target.arch { Arch::AArch64 => 160, Arch::X86_64 => 112 };
    abi::emit_store_zero_to_local_slot(emitter, slot);
}
