//! Purpose:
//! Shares the storage layout of native activations, guards and foreign reference slots.
//!
//! Called from:
//! - Native assembly emitters and Magician's stable record storage.
//!
//! Key details:
//! - The activation and argument records have different contracts despite their equal size.
//! - These layouts do not acquire cell owners or invoke runtime callbacks.

use core::mem::{offset_of, size_of};

/// Internal Mixed tag identifying an invoker's reference marker.
pub const INVOKER_ARG_REF_CELL_TAG: u64 = 11;
/// Source representation of an inline nullable integer.
pub const INVOKER_ARG_TAGGED_SCALAR_SOURCE: u64 = 12;

/// Full writable storage used to stage native invoker arguments.
///
/// The second word can hold a string length or nullable integer tag. Staged
/// scalars also reserve both words because a native variadic writer can
/// publish a complete pair. A borrowed native scalar may have only one word.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvokerSlotWords {
    /// Scalar value, heap address, or first word of an inline pair.
    pub word: u64,
    /// Optional second word of an inline pair.
    pub high: u64,
}

impl InvokerSlotWords {
    /// Offset of the first native word.
    pub const WORD: usize = offset_of!(Self, word);
    /// Offset of the second word of a complete pair.
    pub const HIGH: usize = offset_of!(Self, high);
    /// Size of a complete writable pair.
    pub const BYTES: usize = size_of::<Self>();

    /// Forms an initialized pair for a native writer.
    pub const fn new(word: u64, high: u64) -> Self { Self { word, high } }
}

/// The native guard's borrowed receiver and property name, linked on its caller's stack.
#[repr(C)]
#[derive(Default)]
struct MagicSetGuardWords {
    next: u64,
    receiver: u64,
    name_pointer: u64,
    name_length: u64,
}

/// Pinned storage written by the native magic-setter guard chain.
#[repr(transparent)]
#[derive(Default)]
pub struct MagicSetGuard {
    words: core::cell::UnsafeCell<MagicSetGuardWords>,
    _pin: core::marker::PhantomPinned,
}

impl MagicSetGuard {
    /// Link toward the older guard on the same Fiber.
    pub const NEXT: usize = offset_of!(MagicSetGuardWords, next);
    /// Identity of the borrowed receiver.
    pub const RECEIVER: usize = offset_of!(MagicSetGuardWords, receiver);
    /// Address of the borrowed property bytes.
    pub const NAME_POINTER: usize = offset_of!(MagicSetGuardWords, name_pointer);
    /// Length of the borrowed property name.
    pub const NAME_LENGTH: usize = offset_of!(MagicSetGuardWords, name_length);
    /// Size of the complete native guard.
    pub const BYTES: usize = size_of::<Self>();

    /// Exposes stable writable storage until the caller unlinks the native guard.
    pub fn as_ptr(self: core::pin::Pin<&Self>) -> *mut core::ffi::c_void {
        self.words.get().cast()
    }
}

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
    assert!(InvokerSlotWords::WORD == 0);
    assert!(InvokerSlotWords::HIGH == 8);
    assert!(InvokerSlotWords::BYTES == 16);
    assert!(core::mem::align_of::<InvokerSlotWords>() == 8);
    assert!(MagicSetGuard::NEXT == 0);
    assert!(MagicSetGuard::RECEIVER == 8);
    assert!(MagicSetGuard::NAME_POINTER == 16);
    assert!(MagicSetGuard::NAME_LENGTH == 24);
    assert!(MagicSetGuard::BYTES == 32);
    assert!(core::mem::align_of::<MagicSetGuard>() == 8);
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

/// A native count word followed by that many borrowed cell addresses.
pub struct EvalMethodArgPack;

impl EvalMethodArgPack {
    /// Number of bytes before the first cell address.
    pub const HEADER: usize = size_of::<usize>();

    /// Offset of a cell address relative to the first cell address.
    pub const fn cell_offset(index: usize) -> usize {
        index * size_of::<usize>()
    }

    /// Total record size for a count known to fit the native stack.
    pub const fn bytes(count: usize) -> usize {
        Self::HEADER + Self::cell_offset(count)
    }

    /// Reads the cells of a native pack as borrowed handles.
    ///
    /// # Safety
    /// A non-null, aligned pack must point to a readable count followed by that many
    /// initialized cell pointers in the same allocation. Their leases belong to the caller.
    #[cfg(feature = "runtime")]
    #[allow(unsafe_code)]
    pub unsafe fn decode(pack: *const usize) -> Option<Vec<crate::runtime_owners::RuntimeCellHandle>> {
        use crate::runtime_owners::{RuntimeCell, RuntimeCellHandle};

        if pack.is_null() || !pack.is_aligned() {
            return None;
        }
        let count = unsafe { *pack };
        if count > (isize::MAX as usize - Self::HEADER) / size_of::<usize>() {
            return None;
        }
        let cells = unsafe {
            core::slice::from_raw_parts(pack.add(1).cast::<*mut RuntimeCell>(), count)
        };
        Some(cells.iter().map(|&cell| RuntimeCellHandle::from_raw(cell).borrowed()).collect())
    }
}

/// A dynamic property write's borrowed cell and writable Throwable slot.
#[repr(C)]
pub struct EvalPropertyWriteWords {
    throwable: u64,
    reserved: u64,
    value: u64,
}

impl EvalPropertyWriteWords {
    /// Offset of the Throwable slot initialized and written by the bridge.
    pub const THROWABLE: usize = offset_of!(Self, throwable);
    /// Offset of the borrowed input cell.
    pub const VALUE: usize = offset_of!(Self, value);
    /// Stack space reserved for the record, including native stack alignment.
    pub const STACK_BYTES: usize = (size_of::<Self>() + 15) & !15;
}

/// Result storage written by the eval bridge.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EvalResultWords {
    /// Status-specific result kind.
    pub kind: u32,
    /// Boxed value returned by a successful operation.
    pub value_cell: *mut core::ffi::c_void,
    /// Error payload or boxed Throwable, interpreted by the caller's status.
    pub error: *mut core::ffi::c_void,
}

impl EvalResultWords {
    /// Offset of the kind field.
    pub const KIND: usize = offset_of!(Self, kind);
    /// Offset of the value cell.
    pub const VALUE_CELL: usize = offset_of!(Self, value_cell);
    /// Offset of the error payload.
    pub const ERROR: usize = offset_of!(Self, error);
    /// Size of the complete result record.
    pub const BYTES: usize = size_of::<Self>();

    /// Resets result storage to the bridge's normal-null placeholder.
    pub fn clear(&mut self) {
        self.kind = 0;
        self.value_cell = core::ptr::null_mut();
        self.error = core::ptr::null_mut();
    }
}

/// Borrowed name bytes and optional boxed constant payload for one dynamic declaration.
#[repr(C)]
#[derive(Default)]
pub struct EvalInventoryWords {
    /// Start of the name bytes.
    pub name_ptr: *const u8,
    /// Length of the name bytes.
    pub name_len: u64,
    /// Optional borrowed constant cell.
    pub value_cell: *mut core::ffi::c_void,
}

impl EvalInventoryWords {
    /// Offset of the borrowed name pointer.
    pub const NAME_PTR: usize = offset_of!(Self, name_ptr);
    /// Offset of the name length.
    pub const NAME_LEN: usize = offset_of!(Self, name_len);
    /// Offset of the borrowed cell.
    pub const VALUE_CELL: usize = offset_of!(Self, value_cell);
}
