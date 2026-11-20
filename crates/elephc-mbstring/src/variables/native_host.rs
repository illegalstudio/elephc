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
use std::ptr;

use elephc_builtin_contract::mbstring_abi::variables::{
    MbVariableChildV1, MbVariableHandleV1, MbVariableViewV1, VARIABLE_ARRAY,
    VARIABLE_CHILD_END, VARIABLE_CHILD_ENTRY, VARIABLE_OBJECT, VARIABLE_OTHER,
    VARIABLE_STRING,
};

const ROOT: u64 = 1;
const CHILD: u64 = 2;
const NULL_SENTINEL: u64 = 0x7fff_ffff_ffff_fffe;
const MAX_REFERENCE_DEPTH: usize = 1024;

/// The first two words are also the context understood by older mbstring callbacks.
#[repr(C)]
struct Context {
    eval_context: *mut c_void,
    packed_variadic: u64,
    class_count: *const u64,
    class_sizes: *const u64,
    dynamic_flags: *const u64,
    class_descriptors: *const *const u8,
}

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
    box_ptr: *mut u8,
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

unsafe extern "C" {
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

/// Reads a word from potentially byte-aligned native array or hash storage.
unsafe fn word(base: *const u8, offset: usize) -> u64 {
    unsafe { base.add(offset).cast::<u64>().read_unaligned() }
}

/// Writes a word without assuming a compiler-created array slot is aligned.
unsafe fn put(base: *mut u8, offset: usize, value: u64) {
    unsafe { base.add(offset).cast::<u64>().write_unaligned(value); }
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
    if unsafe { word(reference, 0) } == 7 && unsafe { word(reference, 16) } == 1 {
        return Some(Slot { low: unsafe { reference.add(8) }, tag: 7 });
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
    let mut low = unsafe { word(slot.low, 0) };
    let mut high = if tag == 1 { unsafe { word(slot.low, 8) } } else { 0 };
    let mut box_ptr = ptr::null_mut();
    for _ in 0..MAX_REFERENCE_DEPTH {
        if tag == 11 {
            let reference = low as usize as *mut u8;
            if reference.is_null() { return None; }
            low = unsafe { word(reference, 0) };
            tag = 7;
        }
        if tag != 7 { return Some(Value { tag, low, high, box_ptr }); }
        box_ptr = low as usize as *mut u8;
        if box_ptr.is_null() { return None; }
        tag = unsafe { word(box_ptr, 0) };
        low = unsafe { word(box_ptr, 8) };
        high = unsafe { word(box_ptr, 16) };
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

/// Computes fixed and dynamic property storage from emitted class descriptors.
unsafe fn object_layout(context: &Context, object: *mut u8) -> Option<(usize, *const u8, *mut u8)> {
    if context.class_count.is_null() || context.class_sizes.is_null()
        || context.dynamic_flags.is_null() || context.class_descriptors.is_null() { return None; }
    let class = usize::try_from(unsafe { word(object, 0) }).ok()?;
    if class >= unsafe { context.class_count.read() } as usize { return None; }
    let size = usize::try_from(unsafe { context.class_sizes.add(class).read() }).ok()?;
    let dynamic = unsafe { context.dynamic_flags.add(class).read() } != 0;
    if size < 8 + usize::from(dynamic) * 8 || (size - 8 - usize::from(dynamic) * 8) % 16 != 0 {
        return None;
    }
    let descriptors = unsafe { context.class_descriptors.add(class).read() };
    if descriptors.is_null() { return None; }
    let count = (size - 8 - usize::from(dynamic) * 8) / 16;
    let hash = if dynamic { (unsafe { word(object, size - 8) }) as usize as *mut u8 }
        else { ptr::null_mut() };
    Some((count, descriptors, hash))
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
    let (Some(context), Some(cursor), Some(output)) = (
        unsafe { (context as *const Context).as_ref() }, unsafe { cursor.as_mut() },
        unsafe { output.as_mut() },
    ) else { return 1; };
    let container = identity as usize as *mut u8;
    if container.is_null() || identity == NULL_SENTINEL { return 1; }
    let Ok(index) = usize::try_from(*cursor) else { return 1; };
    let child = match kind {
        VARIABLE_ARRAY => match unsafe { heap_kind(container) } {
            2 => unsafe { indexed_child(container, index) },
            3 => unsafe { hash_child(container, index) },
            _ => None,
        },
        VARIABLE_OBJECT => {
            let Some((fixed, descriptors, dynamic)) = (unsafe { object_layout(context, container) })
                else { return 1; };
            if index < fixed {
                let low = unsafe { container.add(8 + index * 16) };
                let tag = unsafe { descriptors.add(index).read() } as u64;
                Some(Some(MbVariableHandleV1 { words: [low as usize as u64, CHILD, tag, 0] }))
            } else if dynamic.is_null() { Some(None) }
            else { unsafe { hash_child(dynamic, index - fixed) } }
        },
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
        let first = unsafe { word(slot.low, 0) } as usize as *mut u8;
        if first.is_null() { return 1; }
        let first_tag = unsafe { word(first, 0) };
        let owners = unsafe { elephc_mbstring_variable_owner_count_v1(first) };
        if first_tag != 7 && owners > 1 {
            let replacement = unsafe { elephc_mbstring_variable_box_v1(
                first_tag, word(first, 8), word(first, 16),
            ) };
            if replacement.is_null() { return 1; }
            unsafe { elephc_mbstring_variable_replace_pointer_v1(slot.low, replacement); }
            let Some(reloaded) = (unsafe { value(&slot) }) else { return 1; };
            current = reloaded;
        }
    }
    let owner = if current.box_ptr.is_null() { slot.low } else { unsafe { current.box_ptr.add(8) } };
    let next = unsafe { elephc_mbstring_variable_make_unique_v1(owner) };
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
        unsafe { put(reference, 0, replacement as usize as u64); }
        unsafe { elephc_mbstring_variable_replace_pointer_v1(slot.low, reference); }
        return 0;
    }
    if slot.tag != 7 { unsafe { elephc_mbstring_variable_release_v1(replacement); } return 1; }
    unsafe { elephc_mbstring_variable_replace_pointer_v1(slot.low, replacement); }
    0
}
