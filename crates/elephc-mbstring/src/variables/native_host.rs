//! Purpose:
//! Walks native PHP array, object, and reference storage for mb_convert_variables.
//!
//! Called from:
//! - The protected V6 mbstring host table emitted by the native backend.
//!
//! Key details:
//! - The host borrows original slots and never turns array keys into conversion values.
//! - Array payloads separate before mutation; child strings detach nested references.
//! - Runtime allocation and release stay behind target-aware C ABI adapter functions.

use std::ffi::c_void;

use elephc_builtin_contract::mbstring_abi::variables::{
    MbVariableChildV1, MbVariableHandleV1, MbVariableViewV1, VARIABLE_ARRAY,
    VARIABLE_CHILD_END, VARIABLE_CHILD_ENTRY, VARIABLE_OBJECT, VARIABLE_OTHER,
    VARIABLE_STRING,
};

const ROOT: u64 = 1;
const CHILD: u64 = 2;
const NULL_SENTINEL: u64 = 0x7fff_ffff_ffff_fffe;
const MAX_REFERENCE_DEPTH: usize = 1024;

/// A borrowed native slot and its static or per-entry runtime tag.
struct Slot {
    low: *mut u8,
    tag: u64,
}

/// A terminal value reached by following boxed Mixed and managed reference cells.
struct Value {
    tag: u64,
    low: u64,
    high: u64,
    owner: *mut u8,
}

/// Public result shape from the code that owns the native heap header.
#[repr(C)]
pub(crate) struct HeapShape {
    pub(crate) kind: u64,
    pub(crate) value_tag: u64,
}

/// Populated by the native container accessor only on a successful lookup.
#[repr(C)]
#[derive(Default)]
struct BorrowedSlot {
    address: u64,
    tag: u64,
    flags: u64,
}

/// Decoded words and a borrowed pointer to the native boxed owner's payload slot.
#[repr(C)]
#[derive(Default)]
pub(crate) struct NativeCell {
    pub(crate) tag: u64,
    pub(crate) low: u64,
    pub(crate) high: u64,
    owner: *mut u8,
}

/// Borrowed words from a native slot whose available width was selected by the caller.
#[repr(C)]
pub(crate) struct NativeSlotValue {
    pub(crate) low: u64,
    pub(crate) high: u64,
}

unsafe extern "C" {
    /// Decodes the private boxed-cell layout into a borrowed result.
    fn elephc_mbstring_variable_cell_v1(cell: *const u8, output: *mut NativeCell);
    /// Decodes a native value slot with either one or two payload words.
    pub(crate) fn elephc_mbstring_variable_slot_value_v1(slot: *const u8, wide: u64) -> NativeSlotValue;
    /// Finds a declared property slot or the remaining position in its dynamic hash.
    fn elephc_mbstring_variable_object_slot_v1(
        context: *const c_void, object: *const u8, ordinal: usize, output: *mut BorrowedSlot,
    ) -> i32;
    /// Transfers a value into a fresh reference cell with no previous owner.
    pub(crate) fn elephc_mbstring_variable_initialize_reference_v1(reference: *mut u8, value: *mut u8);
    /// Returns the number of live entries from a validated array allocation.
    pub(crate) fn elephc_mbstring_variable_array_count_v1(value: *const u8) -> u64;
    /// Reads a borrowed packed slot and its value shape without exposing storage offsets.
    fn elephc_mbstring_variable_indexed_slot_v1(value: *mut u8, ordinal: usize, output: *mut BorrowedSlot) -> i32;
    /// Reads a borrowed associative slot in insertion order.
    fn elephc_mbstring_variable_hash_slot_v1(value: *mut u8, ordinal: usize, output: *mut BorrowedSlot) -> i32;
    /// Inspects the live allocation without exposing the underlying heap header.
    pub(crate) fn elephc_mbstring_variable_heap_shape_v1(value: *const u8) -> HeapShape;
    /// Returns the physical number of owners of this native allocation.
    fn elephc_mbstring_variable_owner_count_v1(value: *const u8) -> u32;
    /// Separates the array currently stored in the slot and publishes its current identity.
    fn elephc_mbstring_variable_make_unique_v1(slot: *mut u8) -> *mut u8;
    /// Publishes a transferred pointer, then drops the old slot owner.
    fn elephc_mbstring_variable_replace_pointer_v1(slot: *mut u8, value: *mut u8);
    /// Publishes a transferred string pair, then drops the old slot owner.
    fn elephc_mbstring_variable_replace_string_v1(slot: *mut u8, value: *mut u8, length: u64);
    /// Allocates native reference storage for the requested payload tag.
    fn elephc_mbstring_variable_reference_new_v1(tag: u64) -> *mut u8;
    /// Allocates a native boxed value from its runtime tag and payload words.
    fn elephc_mbstring_variable_box_v1(tag: u64, low: u64, high: u64) -> *mut u8;
    /// Copies converted string bytes into native storage that survives the conversion call.
    fn elephc_mbstring_variable_persist_v1(bytes: *const u8, length: u64) -> *mut u8;
    /// Releases one owner of the supplied native boxed or reference value.
    fn elephc_mbstring_variable_release_v1(value: *mut u8);
}

/// Projects a live native cell whose storage stays borrowed by the caller.
pub(crate) unsafe fn read_native_cell(cell: *const u8) -> NativeCell {
    let mut output = NativeCell::default();
    unsafe { elephc_mbstring_variable_cell_v1(cell, &mut output); }
    output
}

/// Reads the low-byte kind from a managed allocation's header.
unsafe fn heap_kind(payload: *const u8) -> u64 {
    unsafe { elephc_mbstring_variable_heap_shape_v1(payload) }.kind
}

/// Resolves native reference cells and eval's persistent reference wrapper.
unsafe fn root_slot(reference: *mut u8) -> Option<Slot> {
    if reference.is_null() { return None; }
    let shape = unsafe { elephc_mbstring_variable_heap_shape_v1(reference) };
    if shape.kind == 8 {
        return Some(Slot { low: reference, tag: shape.value_tag });
    }
    let cell = unsafe { read_native_cell(reference) };
    if cell.tag == 7 && cell.high == 1 {
        return Some(Slot { low: cell.owner, tag: 7 });
    }
    None
}

/// Interprets one four-word handle without following its current PHP value.
unsafe fn slot(handle: &MbVariableHandleV1) -> Option<Slot> {
    let pointer = handle.words[0] as usize as *mut u8;
    if pointer.is_null() { return None; }
    match handle.words[1] {
        ROOT => unsafe { root_slot(pointer) },
        CHILD => Some(Slot { low: pointer, tag: handle.words[2] }),
        _ => None,
    }
}

/// Reads the dereferenced PHP value while retaining the first boxed cell for COW decisions.
unsafe fn value(slot: &Slot) -> Option<Value> {
    let mut tag = slot.tag;
    let mut payload = unsafe { elephc_mbstring_variable_slot_value_v1(slot.low, u64::from(tag == 1)) };
    let mut owner = slot.low;
    for _ in 0..MAX_REFERENCE_DEPTH {
        if tag == 11 {
            let reference = payload.low as usize as *mut u8;
            if reference.is_null() { return None; }
            payload.low = unsafe { elephc_mbstring_variable_slot_value_v1(reference, 0) }.low;
            tag = 7;
        }
        if tag != 7 {
            return Some(Value { tag, low: payload.low, high: payload.high, owner });
        }
        let cell = payload.low as usize as *const u8;
        if cell.is_null() { return None; }
        let value = unsafe { read_native_cell(cell) };
        tag = value.tag;
        payload = NativeSlotValue { low: value.low, high: value.high };
        owner = value.owner;
    }
    None
}

/// Returns the ordinal indexed value through the native packed-storage contract.
pub(crate) unsafe fn indexed_child(array: *mut u8, ordinal: usize) -> Option<Option<MbVariableHandleV1>> {
    unsafe { array_child(array, ordinal, elephc_mbstring_variable_indexed_slot_v1) }
}

/// Returns the ordinal live hash entry in insertion order without converting its key.
pub(crate) unsafe fn hash_child(hash: *mut u8, ordinal: usize) -> Option<Option<MbVariableHandleV1>> {
    unsafe { array_child(hash, ordinal, elephc_mbstring_variable_hash_slot_v1) }
}

/// Maps a native slot query to the bridge's reusable opaque handle.
unsafe fn array_child(
    array: *mut u8,
    ordinal: usize,
    query: unsafe extern "C" fn(*mut u8, usize, *mut BorrowedSlot) -> i32,
) -> Option<Option<MbVariableHandleV1>> {
    let mut output = BorrowedSlot::default();
    match unsafe { query(array, ordinal, &mut output) } {
        0 => Some(Some(MbVariableHandleV1 {
            words: [output.address, CHILD, output.tag, output.flags],
        })),
        1 => Some(None),
        _ => None,
    }
}

/// Returns the ordinal property, continuing into the dynamic hash when present.
unsafe fn object_child(
    context: *const c_void, object: *const u8, ordinal: usize,
) -> Option<Option<MbVariableHandleV1>> {
    let mut slot = BorrowedSlot::default();
    match unsafe { elephc_mbstring_variable_object_slot_v1(context, object, ordinal, &mut slot) } {
        0 => Some(Some(MbVariableHandleV1 { words: [slot.address, CHILD, slot.tag, 0] })),
        1 if slot.address == 0 => Some(None),
        1 => unsafe { hash_child(slot.address as usize as *mut u8, slot.tag as usize) },
        _ => None,
    }
}

/// Publishes a borrowed string or stable array/object identity to the conversion engine.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_variable_inspect_v1(
    _context: *mut c_void, handle: *const MbVariableHandleV1, output: *mut MbVariableViewV1,
) -> i32 {
    let (Some(handle), Some(output)) = (unsafe { handle.as_ref() }, unsafe { output.as_mut() })
        else { return 1; };
    let Some(slot) = (unsafe { slot(handle) }) else { return 1; };
    let Some(value) = (unsafe { value(&slot) }) else { return 1; };
    *output = MbVariableViewV1::default();
    match value.tag {
        1 => {
            if value.low == 0 && value.high != 0 { return 1; }
            output.kind = VARIABLE_STRING;
            output.bytes = value.low as usize as *const u8;
            output.len = value.high;
        },
        4 | 5 if value.low != 0 && value.low != NULL_SENTINEL => {
            output.kind = VARIABLE_ARRAY;
            output.identity = value.low;
        },
        6 if value.low != 0 && value.low != NULL_SENTINEL => {
            output.kind = VARIABLE_OBJECT;
            output.identity = value.low;
        },
        _ => output.kind = VARIABLE_OTHER,
    }
    0
}

/// Advances through indexed values, insertion-ordered hash entries, or all object properties.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_variable_child_next_v1(
    context: *mut c_void, kind: u64, identity: u64, cursor: *mut u64,
    output: *mut MbVariableChildV1,
) -> i32 {
    let (Some(cursor), Some(output)) = (unsafe { cursor.as_mut() }, unsafe { output.as_mut() })
        else { return 1; };
    if context.is_null() { return 1; }
    let container = identity as usize as *mut u8;
    if container.is_null() || identity == NULL_SENTINEL { return 1; }
    let Ok(index) = usize::try_from(*cursor) else { return 1; };
    let child = match kind {
        VARIABLE_ARRAY => match unsafe { heap_kind(container) } {
            2 => unsafe { indexed_child(container, index) },
            3 => unsafe { hash_child(container, index) },
            _ => None,
        },
        VARIABLE_OBJECT => unsafe { object_child(context, container, index) },
        _ => None,
    };
    match child {
        Some(Some(handle)) => {
            let Some(next) = cursor.checked_add(1) else { return 1; };
            *cursor = next;
            output.kind = VARIABLE_CHILD_ENTRY;
            output.handle = handle;
            0
        },
        Some(None) => { output.kind = VARIABLE_CHILD_END; 0 },
        None => 1,
    }
}

/// Separates the array owned by this slot and publishes its current identity.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_variable_prepare_write_v1(
    _context: *mut c_void, handle: *const MbVariableHandleV1, kind: u64, identity: u64,
    output: *mut u64,
) -> i32 {
    let (Some(handle), Some(output)) = (unsafe { handle.as_ref() }, unsafe { output.as_mut() })
        else { return 1; };
    let Some(slot) = (unsafe { slot(handle) }) else { return 1; };
    let Some(mut current) = (unsafe { value(&slot) }) else { return 1; };
    if current.low != identity { return 1; }
    if kind == VARIABLE_OBJECT && current.tag == 6 {
        *output = identity;
        return 0;
    }
    if kind != VARIABLE_ARRAY || !matches!(current.tag, 4 | 5) { return 1; }
    // A shallow array clone may share a boxed value without sharing the PHP reference itself.
    // Give this bucket its own box before changing the array payload inside that box.
    if handle.words[1] == CHILD && slot.tag == 7 {
        let first = unsafe { elephc_mbstring_variable_slot_value_v1(slot.low, 0) }.low as usize as *mut u8;
        if first.is_null() { return 1; }
        let cell = unsafe { read_native_cell(first) };
        let owners = unsafe { elephc_mbstring_variable_owner_count_v1(first) };
        if cell.tag != 7 && owners > 1 {
            let replacement = unsafe { elephc_mbstring_variable_box_v1(cell.tag, cell.low, cell.high) };
            if replacement.is_null() { return 1; }
            unsafe { elephc_mbstring_variable_replace_pointer_v1(slot.low, replacement); }
            let Some(reloaded) = (unsafe { value(&slot) }) else { return 1; };
            current = reloaded;
        }
    }
    let next = unsafe { elephc_mbstring_variable_make_unique_v1(current.owner) };
    if next.is_null() { return 1; }
    *output = next as usize as u64;
    0
}

/// Replaces only this root or child slot, preserving sibling keys and prior writes.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_variable_write_string_v1(
    _context: *mut c_void, handle: *const MbVariableHandleV1,
    bytes: *const u8, length: u64,
) -> i32 {
    let Some(handle) = (unsafe { handle.as_ref() }) else { return 1; };
    if bytes.is_null() && length != 0 { return 1; }
    let Some(slot) = (unsafe { slot(handle) }) else { return 1; };
    if !matches!(unsafe { value(&slot) }, Some(Value { tag: 1, .. })) { return 1; }
    if handle.words[3] == 1 { return 1; }
    if slot.tag == 1 {
        let replacement = unsafe { elephc_mbstring_variable_persist_v1(bytes, length) };
        if replacement.is_null() { return 1; }
        unsafe { elephc_mbstring_variable_replace_string_v1(slot.low, replacement, length); }
        return 0;
    }
    let replacement = unsafe { elephc_mbstring_variable_box_v1(1, bytes as usize as u64, length) };
    if replacement.is_null() { return 1; }
    if handle.words[1] == CHILD && slot.tag == 11 {
        let reference = unsafe { elephc_mbstring_variable_reference_new_v1(7) };
        if reference.is_null() {
            unsafe { elephc_mbstring_variable_release_v1(replacement); }
            return 1;
        }
        unsafe { elephc_mbstring_variable_initialize_reference_v1(reference, replacement); }
        unsafe { elephc_mbstring_variable_replace_pointer_v1(slot.low, reference); }
        return 0;
    }
    if slot.tag != 7 { unsafe { elephc_mbstring_variable_release_v1(replacement); } return 1; }
    unsafe { elephc_mbstring_variable_replace_pointer_v1(slot.low, replacement); }
    0
}
