//! Purpose:
//! Shares the storage layout of native activations and foreign reference pins.
//!
//! Called from:
//! - Native assembly emitters and Magician's stable record storage.
//!
//! Key details:
//! - The activation and argument records have different contracts despite their equal size.
//! - These layouts do not acquire cell owners or invoke runtime callbacks.

use core::mem::{offset_of, size_of};

/// The shared unwind prefix followed by the three extended activation words.
#[repr(C, align(16))]
#[derive(Default)]
pub struct ExtendedActivation {
    previous: u64,
    cleanup: u64,
    data: u64,
    reader_or_state: u64,
    line_or_stack: u64,
    owner_visitor: u64,
}

impl ExtendedActivation {
    /// Previous activation in the unwind chain.
    pub const PREVIOUS: usize = offset_of!(Self, previous);
    /// Cleanup callback used while unwinding.
    pub const CLEANUP: usize = offset_of!(Self, cleanup);
    /// Callback data, such as the address of the owning frame.
    pub const DATA: usize = offset_of!(Self, data);
    /// First word beyond the common unwind prefix.
    pub const EXTENSION: usize = offset_of!(Self, reader_or_state);
    /// Caller line or saved operand stack.
    pub const LINE_OR_STACK: usize = offset_of!(Self, line_or_stack);
    /// Pure callback that visits owner slots.
    pub const OWNER_VISITOR: usize = offset_of!(Self, owner_visitor);
    /// Total size of the extended record.
    pub const BYTES: usize = size_of::<Self>();
}

/// A stable lease linked into the native reference-validation chain.
#[repr(C)]
#[derive(Default)]
pub struct ValidationPin {
    previous: u64,
    next: u64,
    /// The sole cell or allocation owned by this record.
    pub owner: u64,
    /// The authenticated heap kind or argument transfer phase.
    pub kind: u64,
}

impl ValidationPin {
    /// Backlink from the following record toward the head.
    pub const PREVIOUS: usize = offset_of!(Self, previous);
    /// Link toward older leases.
    pub const NEXT: usize = offset_of!(Self, next);
    /// The address slot transferred back to the foreign caller on take.
    pub const OWNER: usize = offset_of!(Self, owner);
    /// The kind or phase word inspected by the native walkers.
    pub const KIND: usize = offset_of!(Self, kind);
    /// Total size of a validation lease.
    pub const BYTES: usize = size_of::<Self>();
}

/// A pending argument lease extending the validation record with its exact association.
#[repr(C)]
#[derive(Default)]
pub struct ArgumentPin {
    /// The common validation fields, including the owner slot.
    pub validation: ValidationPin,
    /// Native cell that may consume the owner.
    pub native_cell: u64,
    /// Identity of the staging lease that can cancel this association.
    pub staging: u64,
}

impl ArgumentPin {
    /// Source lease still pending parameter acquisition.
    pub const PENDING: i64 = 0x105;
    /// A native parameter already consumed the source lease.
    pub const ACTIVE: i64 = 0x205;
    /// Address of the owner slot in the shared prefix.
    pub const OWNER: usize = offset_of!(Self, validation.owner);
    /// Exact native-cell identity.
    pub const NATIVE_CELL: usize = offset_of!(Self, native_cell);
    /// Exact staging-record identity.
    pub const STAGING: usize = offset_of!(Self, staging);
}

const _: () = {
    assert!(ExtendedActivation::BYTES == 48);
    assert!(core::mem::align_of::<ExtendedActivation>() == 16);
    assert!(ExtendedActivation::OWNER_VISITOR == 40);
    assert!(ValidationPin::BYTES == 32);
    assert!(core::mem::align_of::<ValidationPin>() == 8);
    assert!(ArgumentPin::OWNER == 16);
    assert!(ArgumentPin::NATIVE_CELL == 32);
    assert!(ArgumentPin::STAGING == 40);
    assert!(size_of::<ArgumentPin>() == 48);
    assert!(core::mem::align_of::<ArgumentPin>() == 8);
};
