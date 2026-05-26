//! Purpose:
//! Emits runtime helpers for `SplDoublyLinkedList`, `SplStack`, and `SplQueue`.
//! The helpers back PHP-visible mutation, iteration, count, and ArrayAccess methods.
//!
//! Called from:
//! - `crate::codegen::runtime::spl::emit_doubly_linked_list_runtime()`.
//!
//! Key details:
//! - The object stores a class id, an owned mixed-cell indexed array, iterator index, and iterator mode.
//! - Mutating methods take ownership of boxed `Mixed` arguments prepared by call lowering.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

use super::{SPL_DLL_ITER_INDEX_OFFSET, SPL_DLL_ITER_MODE_OFFSET, SPL_DLL_STORAGE_OFFSET};

const SPL_DLL_OBJECT_SIZE: i64 = 32;
const SPL_DLL_INITIAL_CAPACITY: i64 = 4;
const NULL_TAG: i64 = 8;
const INT_TAG: i64 = 0;
const STR_TAG: i64 = 1;
const BOOL_TAG: i64 = 3;
const ITER_MODE_DELETE: i64 = 1;
const ITER_MODE_LIFO: i64 = 2;
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;
const SPL_DLL_POP_EMPTY_MSG_LEN: usize = "Can't pop from an empty datastructure".len();
const SPL_DLL_SHIFT_EMPTY_MSG_LEN: usize = "Can't shift from an empty datastructure".len();
const SPL_DLL_PEEK_EMPTY_MSG_LEN: usize = "Can't peek at an empty datastructure".len();
const SPL_DLL_ADD_RANGE_MSG_LEN: usize =
    "SplDoublyLinkedList::add(): Argument #1 ($index) is out of range".len();
const SPL_DLL_OFFSET_EXISTS_TYPE_MSG_LEN: usize =
    "SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, non-int given"
        .len();
const SPL_DLL_OFFSET_GET_RANGE_MSG_LEN: usize =
    "SplDoublyLinkedList::offsetGet(): Argument #1 ($index) is out of range".len();
const SPL_DLL_OFFSET_GET_TYPE_MSG_LEN: usize =
    "SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, non-int given"
        .len();
const SPL_DLL_OFFSET_SET_RANGE_MSG_LEN: usize =
    "SplDoublyLinkedList::offsetSet(): Argument #1 ($index) is out of range".len();
const SPL_DLL_OFFSET_SET_TYPE_MSG_LEN: usize =
    "SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, non-int given"
        .len();
const SPL_DLL_OFFSET_UNSET_RANGE_MSG_LEN: usize =
    "SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) is out of range".len();
const SPL_DLL_OFFSET_UNSET_TYPE_MSG_LEN: usize =
    "SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, non-int given"
        .len();

/// Emits all SplDoublyLinkedList, SplStack, and SplQueue runtime helpers for the target architecture.
/// Entry point called by `emit_doubly_linked_list_runtime()` which routes to the correct architecture.
pub(crate) fn emit_doubly_linked_list_runtime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits all ARM64 doubly linked list runtime helpers: constructor, mutators, iterators,
/// serialization, ArrayAccess methods, and exception helpers.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl doubly linked list ---");
    emit_new_aarch64(emitter);
    emit_count_aarch64(emitter);
    emit_is_empty_aarch64(emitter);
    emit_push_aarch64(emitter);
    emit_pop_aarch64(emitter);
    emit_shift_aarch64(emitter);
    emit_unshift_aarch64(emitter);
    emit_insert_aarch64(emitter);
    emit_top_aarch64(emitter);
    emit_bottom_aarch64(emitter);
    emit_iterator_mode_aarch64(emitter);
    emit_serialize_aarch64(emitter);
    emit_unserialize_aarch64(emitter);
    emit_serialize_array_aarch64(emitter);
    emit_rewind_aarch64(emitter);
    emit_next_prev_aarch64(emitter);
    emit_valid_aarch64(emitter);
    emit_current_aarch64(emitter);
    emit_key_aarch64(emitter);
    emit_offset_exists_aarch64(emitter);
    emit_offset_get_aarch64(emitter);
    emit_offset_set_aarch64(emitter);
    emit_offset_unset_aarch64(emitter);
}

/// Emits `__rt_spl_dll_new` on ARM64: allocates an SPL list object, initializes internal Mixed-array
/// storage with capacity 4, and stores iterator index/mode at their respective offsets. Returns
/// the initialized object pointer in x0.
fn emit_new_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_new");
    emitter.instruction("sub sp, sp, #32");                                     // reserve constructor spill slots
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a frame for nested allocator calls
    emitter.instruction("str x0, [sp, #0]");                                    // save the concrete SPL class id
    emitter.instruction(&format!("mov x0, #{}", SPL_DLL_OBJECT_SIZE));          // request the fixed SPL list object payload size
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the SPL list object payload
    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation as an object instance
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the concrete SPL class id
    emitter.instruction("str x9, [x0]");                                        // store the class id at the object header
    emitter.instruction("str x0, [sp, #8]");                                    // save the object pointer while allocating storage
    emitter.instruction(&format!("mov x0, #{}", SPL_DLL_INITIAL_CAPACITY));     // initial internal storage capacity
    emitter.instruction("mov x1, #8");                                          // each internal storage slot holds one Mixed pointer
    emitter.instruction("bl __rt_array_new");                                   // allocate the owned internal mixed-pointer storage
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the internal array packed kind word
    emitter.instruction("mov x10, #0x700");                                     // runtime value_type tag 7 = boxed Mixed cells
    emitter.instruction("orr x9, x9, x10");                                     // mark internal storage as an array of Mixed cells
    emitter.instruction("str x9, [x0, #-8]");                                   // persist the Mixed value_type tag on internal storage
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the object pointer
    emitter.instruction(&format!("str x0, [x9, #{}]", SPL_DLL_STORAGE_OFFSET)); // object.storage = internal Mixed array
    emitter.instruction(&format!("str xzr, [x9, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // iterator index starts at zero
    emitter.instruction(&format!("str xzr, [x9, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // iterator mode starts FIFO/KEEP
    emitter.instruction("mov x0, x9");                                          // return the initialized SPL object
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release constructor spill slots
    emitter.instruction("ret");                                                 // return the object pointer
}

/// Emits `__rt_spl_dll_count` on ARM64: loads the internal storage array and returns its length
/// as an integer in x0.
fn emit_count_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_count");
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load the internal storage array
    emitter.instruction("ldr x0, [x9]");                                        // return the internal storage length
    emitter.instruction("ret");                                                 // return count
}

/// Emits `__rt_spl_dll_is_empty` on ARM64: reads storage length, compares to zero, and returns
/// a boolean (1 when empty, 0 when non-empty) in x0.
fn emit_is_empty_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_is_empty");
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load the internal storage array
    emitter.instruction("ldr x9, [x9]");                                        // read the storage length
    emitter.instruction("cmp x9, #0");                                          // compare length with zero
    emitter.instruction("cset x0, eq");                                         // return true when the list is empty
    emitter.instruction("ret");                                                 // return boolean result
}

/// Emits `__rt_spl_dll_push` on ARM64: receiver in x0, owned Mixed value in x1. Appends the value
/// to internal storage via `__rt_array_push_int`, handles growth, and returns void.
fn emit_push_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_push");
    emitter.instruction("sub sp, sp, #32");                                     // reserve spill slots for receiver and return address
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a frame for the nested array append
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver while appending to internal storage
    emitter.instruction(&format!("ldr x0, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // pass internal storage as array_push receiver
    emitter.instruction("bl __rt_array_push_int");                              // append the owned Mixed pointer without retaining it again
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver after append
    emitter.instruction(&format!("str x0, [x9, #{}]", SPL_DLL_STORAGE_OFFSET)); // store possibly-grown internal storage
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release spill slots
    emitter.instruction("ret");                                                 // return void
}

/// Emits `__rt_spl_dll_pop` on ARM64: receiver in x0. Removes and returns the last owned Mixed
/// cell, transferring ownership to the caller. Throws RuntimeException on an empty list.
fn emit_pop_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_pop");
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("ldr x10, [x9]");                                       // read current storage length
    emitter.instruction("cbz x10, __rt_spl_dll_pop_empty");                     // empty list raises PHP's RuntimeException
    emitter.instruction("sub x10, x10, #1");                                    // compute the last occupied index
    emitter.instruction("str x10, [x9]");                                       // shrink storage length by one
    emitter.instruction("add x11, x9, #24");                                    // point at the first storage element
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // return the removed Mixed cell, transferring ownership
    emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // clear the stale slot beyond the new length
    emitter.instruction("ret");                                                 // return removed Mixed cell
    emitter.label("__rt_spl_dll_pop_empty");
    emit_throw_exception_aarch64(
        emitter,
        "_spl_runtime_exception_class_id",
        "_spl_dll_pop_empty_msg",
        SPL_DLL_POP_EMPTY_MSG_LEN,
    );
}

/// Emits `__rt_spl_dll_shift` on ARM64: receiver in x0. Removes and returns the first owned
/// Mixed cell, shifting all remaining elements left by one slot. Transfers ownership to the
/// caller. Throws RuntimeException on an empty list.
fn emit_shift_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_shift");
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("ldr x10, [x9]");                                       // read current storage length
    emitter.instruction("cbz x10, __rt_spl_dll_shift_empty");                   // empty list raises PHP's RuntimeException
    emitter.instruction("add x11, x9, #24");                                    // point at the first storage element
    emitter.instruction("ldr x0, [x11]");                                       // capture the removed first Mixed cell
    emitter.instruction("mov x12, #1");                                         // start shifting from element index 1
    emitter.label("__rt_spl_dll_shift_loop");
    emitter.instruction("cmp x12, x10");                                        // have all live elements been shifted left?
    emitter.instruction("b.ge __rt_spl_dll_shift_done");                        // finish once the cursor reaches the old length
    emitter.instruction("ldr x13, [x11, x12, lsl #3]");                         // load the next Mixed pointer
    emitter.instruction("sub x14, x12, #1");                                    // compute the destination index one slot earlier
    emitter.instruction("str x13, [x11, x14, lsl #3]");                         // move the Mixed pointer down by one slot
    emitter.instruction("add x12, x12, #1");                                    // advance the shift cursor
    emitter.instruction("b __rt_spl_dll_shift_loop");                           // continue compacting storage
    emitter.label("__rt_spl_dll_shift_done");
    emitter.instruction("sub x10, x10, #1");                                    // compute the new storage length
    emitter.instruction("str x10, [x9]");                                       // persist the shortened length
    emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // clear the stale tail slot
    emitter.instruction("ret");                                                 // return removed Mixed cell
    emitter.label("__rt_spl_dll_shift_empty");
    emit_throw_exception_aarch64(
        emitter,
        "_spl_runtime_exception_class_id",
        "_spl_dll_shift_empty_msg",
        SPL_DLL_SHIFT_EMPTY_MSG_LEN,
    );
}

/// Emits `__rt_spl_dll_unshift` on ARM64: receiver in x0, owned value in x1. Moves x1 to
/// the insert helper's value argument and sets index to zero before tail-calling
/// `__rt_spl_dll_insert`.
fn emit_unshift_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_unshift");
    emitter.instruction("mov x2, x1");                                          // move value to the insert helper's value argument
    emitter.instruction("mov x1, #0");                                          // unshift inserts at index zero
    emitter.instruction("b __rt_spl_dll_insert");                               // tail-call the shared insertion helper
}

/// Emits `__rt_spl_dll_insert` on ARM64: receiver in x0, index in x1, owned value in x2.
/// Validates index >= 0 and index <= length, converts LIFO logical index to physical slot,
/// grows storage if needed, shifts elements right, and stores the value. Releases the value
/// and throws OutOfRangeException on invalid index.
fn emit_insert_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_insert");
    emitter.instruction("sub sp, sp, #64");                                     // reserve insertion state and call frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a frame for array growth
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver
    emitter.instruction("str x1, [sp, #8]");                                    // save requested insertion index
    emitter.instruction("str x2, [sp, #16]");                                   // save owned Mixed value to insert
    emitter.instruction("cmp x1, #0");                                          // is the requested index negative?
    emitter.instruction("b.lt __rt_spl_dll_insert_range_throw");                // negative indexes are out of range in PHP
    emitter.label("__rt_spl_dll_insert_index_nonnegative");
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("str x9, [sp, #24]");                                   // save current storage pointer
    emitter.instruction("ldr x10, [x9]");                                       // read current storage length
    emitter.instruction("ldr x11, [x9, #8]");                                   // read current storage capacity
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload requested insertion index
    emitter.instruction("cmp x12, x10");                                        // is the index past the end?
    emitter.instruction("b.le __rt_spl_dll_insert_index_in_range");             // keep indexes within the append boundary
    emitter.instruction("b __rt_spl_dll_insert_range_throw");                   // indexes past the end are out of range
    emitter.label("__rt_spl_dll_insert_index_in_range");
    emitter.instruction(&format!("ldr x13, [x0, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode bits for logical index mapping
    emitter.instruction(&format!("tst x13, #{}", ITER_MODE_LIFO));              // does logical indexing run in LIFO order?
    emitter.instruction("b.eq __rt_spl_dll_insert_physical_index_ready");       // FIFO indexes already match physical storage
    emitter.instruction("cmp x12, x10");                                        // does LIFO insertion target the logical end?
    emitter.instruction("b.eq __rt_spl_dll_insert_physical_index_ready");       // logical end still appends physically
    emitter.instruction("sub x12, x10, x12");                                   // convert logical LIFO index to one-based physical slot
    emitter.instruction("sub x12, x12, #1");                                    // finish zero-based physical insertion index
    emitter.instruction("str x12, [sp, #8]");                                   // save mapped physical insertion index
    emitter.label("__rt_spl_dll_insert_physical_index_ready");
    emitter.instruction("cmp x10, x11");                                        // is storage full?
    emitter.instruction("b.ne __rt_spl_dll_insert_have_capacity");              // skip growth when capacity remains
    emitter.instruction("mov x0, x9");                                          // pass current storage to array_grow
    emitter.instruction("bl __rt_array_grow");                                  // grow internal storage
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver after growth
    emitter.instruction(&format!("str x0, [x9, #{}]", SPL_DLL_STORAGE_OFFSET)); // store possibly-grown internal storage
    emitter.instruction("str x0, [sp, #24]");                                   // save grown storage for insertion
    emitter.label("__rt_spl_dll_insert_have_capacity");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload storage pointer
    emitter.instruction("ldr x10, [x9]");                                       // reload current length
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload clamped insertion index
    emitter.instruction("add x11, x9, #24");                                    // point at the first storage element
    emitter.instruction("mov x13, x10");                                        // start right-shift cursor at the old length
    emitter.label("__rt_spl_dll_insert_shift_loop");
    emitter.instruction("cmp x13, x12");                                        // has the cursor reached the insertion index?
    emitter.instruction("b.le __rt_spl_dll_insert_store");                      // stop once the insertion slot is free
    emitter.instruction("sub x14, x13, #1");                                    // source index is one slot before the cursor
    emitter.instruction("ldr x15, [x11, x14, lsl #3]");                         // load the Mixed pointer being shifted right
    emitter.instruction("str x15, [x11, x13, lsl #3]");                         // store the Mixed pointer one slot to the right
    emitter.instruction("sub x13, x13, #1");                                    // move the shift cursor left
    emitter.instruction("b __rt_spl_dll_insert_shift_loop");                    // continue shifting until the insert slot is open
    emitter.label("__rt_spl_dll_insert_store");
    emitter.instruction("ldr x14, [sp, #16]");                                  // reload owned Mixed value to insert
    emitter.instruction("str x14, [x11, x12, lsl #3]");                         // store the owned Mixed value in the insertion slot
    emitter.instruction("add x10, x10, #1");                                    // increase storage length
    emitter.instruction("str x10, [x9]");                                       // persist new storage length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release insertion state
    emitter.instruction("ret");                                                 // return void
    emitter.label("__rt_spl_dll_insert_range_throw");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload rejected Mixed value before throwing
    emitter.instruction("bl __rt_decref_mixed");                                // release the rejected insertion value
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release insertion state before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_out_of_range_exception_class_id",
        "_spl_dll_add_range_msg",
        SPL_DLL_ADD_RANGE_MSG_LEN,
    );
}

/// Emits `__rt_spl_dll_top` on ARM64: receiver in x0. Loads the last occupied Mixed cell,
/// retains it with `__rt_incref` for the caller, and returns it. Throws RuntimeException
/// on an empty list by tail-calling `emit_peek_index_aarch64`.
fn emit_top_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_top");
    emit_peek_index_aarch64(emitter, "__rt_spl_dll_top_null", true);
}

/// Emits `__rt_spl_dll_bottom` on ARM64: receiver in x0. Loads the first occupied Mixed cell,
/// retains it with `__rt_incref` for the caller, and returns it. Throws RuntimeException
/// on an empty list by tail-calling `emit_peek_index_aarch64`.
fn emit_bottom_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_bottom");
    emit_peek_index_aarch64(emitter, "__rt_spl_dll_bottom_null", false);
}

/// Emits the shared peek helper on ARM64: receiver in x0, null_label and last flag select
/// which element to load (last=true picks index length-1, last=false picks index 0).
/// Retains the selected Mixed cell with `__rt_incref` before returning it. Jumps to
/// null_label when storage is empty, which throws RuntimeException via
/// `emit_throw_exception_aarch64`.
fn emit_peek_index_aarch64(emitter: &mut Emitter, null_label: &str, last: bool) {
    emitter.instruction("sub sp, sp, #32");                                     // reserve frame for the incref call
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a frame for nested incref
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("ldr x10, [x9]");                                       // read storage length
    emitter.instruction(&format!("cbz x10, {}", null_label));                   // empty storage returns null
    if last {
        emitter.instruction("sub x10, x10, #1");                                // choose the last occupied index
    } else {
        emitter.instruction("mov x10, #0");                                     // choose the first occupied index
    }
    emitter.instruction("add x11, x9, #24");                                    // point at the first storage element
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // load the selected Mixed cell
    emitter.instruction("bl __rt_incref");                                      // retain the Mixed cell for the caller while storage keeps its owner
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release frame
    emitter.instruction("ret");                                                 // return retained Mixed cell
    emitter.label(null_label);
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #32");                                     // release frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_runtime_exception_class_id",
        "_spl_dll_peek_empty_msg",
        SPL_DLL_PEEK_EMPTY_MSG_LEN,
    );
}

/// Emits `__rt_spl_dll_set_iterator_mode` and `__rt_spl_dll_get_iterator_mode` on ARM64.
/// Set: receiver in x0, mode bits in x1; stores x1 at the iterator mode offset and returns void.
/// Get: receiver in x0; returns iterator mode bits in x0.
fn emit_iterator_mode_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_set_iterator_mode");
    emitter.instruction(&format!("str x1, [x0, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // store iterator mode bits on the receiver
    emitter.instruction("ret");                                                 // return void
    emitter.label_global("__rt_spl_dll_get_iterator_mode");
    emitter.instruction(&format!("ldr x0, [x0, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // return iterator mode bits
    emitter.instruction("ret");                                                 // return integer mode
}

/// Emits `__rt_spl_dll_serialize_array` on ARM64: receiver in x0. Copies internal storage
/// items into a new array with retained Mixed cells, boxes iterator flags and both arrays,
/// and returns a boxed Mixed array (tag 4) containing [boxed flags, boxed items array,
/// boxed empty properties array] for high-level serialization.
fn emit_serialize_array_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_serialize_array");
    emitter.instruction("sub sp, sp, #112");                                    // reserve serialization arrays, boxes, and cursor spills
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish serialization frame
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal Mixed storage
    emitter.instruction("str x9, [sp, #8]");                                    // save internal storage pointer
    emitter.instruction("ldr x10, [x9]");                                       // load list length
    emitter.instruction("str x10, [sp, #16]");                                  // save list length
    emitter.instruction("mov x0, x10");                                         // items array capacity equals list length
    emitter.instruction("mov x1, #8");                                          // items array stores boxed Mixed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate serialized dllist array
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load items array packed kind word
    emitter.instruction("orr x9, x9, #0x700");                                  // stamp items array as boxed Mixed slots
    emitter.instruction("str x9, [x0, #-8]");                                   // persist items Mixed value_type tag
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload list length
    emitter.instruction("str x10, [x0]");                                       // publish exact items array length
    emitter.instruction("str x0, [sp, #24]");                                   // save items array pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // initialize item copy cursor
    emitter.label("__rt_spl_dll_serialize_items_loop");
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload item copy cursor
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload list length
    emitter.instruction("cmp x10, x11");                                        // have all list items been copied?
    emitter.instruction("b.ge __rt_spl_dll_serialize_items_done");              // stop once every item is serialized
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload internal storage pointer
    emitter.instruction("add x9, x9, #24");                                     // point at internal Mixed slot base
    emitter.instruction("ldr x0, [x9, x10, lsl #3]");                           // load source Mixed cell
    emitter.instruction("bl __rt_incref");                                      // retain source Mixed cell for the serialized items array
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload item copy cursor after retain
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload items array pointer
    emitter.instruction("add x9, x9, #24");                                     // point at serialized items slot base
    emitter.instruction("str x0, [x9, x10, lsl #3]");                           // store retained Mixed cell into serialized items
    emitter.instruction("add x10, x10, #1");                                    // advance item copy cursor
    emitter.instruction("str x10, [sp, #32]");                                  // save updated item copy cursor
    emitter.instruction("b __rt_spl_dll_serialize_items_loop");                 // continue copying list items
    emitter.label("__rt_spl_dll_serialize_items_done");
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 = indexed array
    emitter.instruction("ldr x1, [sp, #24]");                                   // pass serialized items array as mixed payload
    emitter.instruction("mov x2, xzr");                                         // array mixed payload uses one word
    emitter.instruction("bl __rt_mixed_from_value");                            // box serialized items array
    emitter.instruction("str x0, [sp, #40]");                                   // save boxed dllist value
    emitter.instruction("mov x0, #0");                                          // empty properties array capacity
    emitter.instruction("mov x1, #8");                                          // properties array stores boxed Mixed slots
    emitter.instruction("bl __rt_array_new");                                   // allocate empty serialized properties array
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load properties array packed kind word
    emitter.instruction("orr x9, x9, #0x700");                                  // stamp properties as boxed Mixed slots
    emitter.instruction("str x9, [x0, #-8]");                                   // persist properties Mixed value_type tag
    emitter.instruction("mov x1, x0");                                          // pass properties array pointer as mixed payload
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 = indexed array
    emitter.instruction("mov x2, xzr");                                         // array mixed payload uses one word
    emitter.instruction("bl __rt_mixed_from_value");                            // box empty properties array
    emitter.instruction("str x0, [sp, #48]");                                   // save boxed properties value
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver for iterator flags
    emitter.instruction(&format!("ldr x1, [x9, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode flags
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("mov x2, xzr");                                         // integer mixed payload uses one word
    emitter.instruction("bl __rt_mixed_from_value");                            // box iterator flags
    emitter.instruction("str x0, [sp, #56]");                                   // save boxed flags value
    emitter.instruction("mov x0, #3");                                          // serialized state has flags, dllist, and properties
    emitter.instruction("mov x1, #8");                                          // outer serialized array stores boxed Mixed slots
    emitter.instruction("bl __rt_array_new");                                   // allocate outer serialized state array
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load outer array packed kind word
    emitter.instruction("orr x9, x9, #0x700");                                  // stamp outer array as boxed Mixed slots
    emitter.instruction("str x9, [x0, #-8]");                                   // persist outer Mixed value_type tag
    emitter.instruction("mov x9, #3");                                          // outer serialized state length
    emitter.instruction("str x9, [x0]");                                        // publish exact serialized state length
    emitter.instruction("add x10, x0, #24");                                    // point at outer serialized state slots
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload boxed flags value
    emitter.instruction("str x9, [x10]");                                       // state[0] = flags
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload boxed dllist value
    emitter.instruction("str x9, [x10, #8]");                                   // state[1] = dllist
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload boxed properties value
    emitter.instruction("str x9, [x10, #16]");                                  // state[2] = properties
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release serialization frame
    emitter.instruction("ret");                                                 // return serialized state array
}

/// Emits `__rt_spl_dll_serialize` on ARM64: receiver in x0, pointer to serialized output
/// buffer in x1, length in x2. Writes PHP legacy serialized form "i:<mode>;<item>..." using
/// the global concat buffer. Returns (pointer, length) of the serialized string. Unboxes
/// each item to detect int/string/bool/null tags and encodes accordingly. Appends to the
/// global `_concat_buf` and updates `_concat_off`.
fn emit_serialize_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_serialize");
    emitter.instruction("sub sp, sp, #128");                                    // reserve serialization cursor, item, and spill slots
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish legacy serialization frame
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver across item unboxing
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current concat-buffer offset
    abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("str x11, [sp, #72]");                                  // save concat-buffer base for final offset update
    emitter.instruction("add x12, x11, x10");                                   // compute start pointer for this serialized string
    emitter.instruction("str x12, [sp, #32]");                                  // save string start pointer
    emitter.instruction("str x12, [sp, #40]");                                  // initialize write cursor
    emitter.instruction("mov w13, #105");                                       // ASCII 'i' starts the legacy flags field
    emitter.instruction("strb w13, [x12], #1");                                 // write 'i' and advance cursor
    emitter.instruction("mov w13, #58");                                        // ASCII ':' separates type and payload
    emitter.instruction("strb w13, [x12], #1");                                 // write ':' after the integer tag
    emitter.instruction("str x12, [sp, #40]");                                  // save cursor before decimal writer call
    emitter.instruction(&format!("ldr x0, [x0, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode flags for the legacy payload
    emitter.instruction("ldr x1, [sp, #40]");                                   // pass current cursor to decimal writer
    emitter.instruction("bl __rt_spl_dll_write_dec");                           // append decimal flags text
    emitter.instruction("str x1, [sp, #40]");                                   // save cursor returned by decimal writer
    emitter.instruction("mov w13, #59");                                        // ASCII ';' closes the flags integer
    emitter.instruction("strb w13, [x1], #1");                                  // write ';' after the flags value
    emitter.instruction("str x1, [sp, #40]");                                   // save cursor after flags field
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver for internal storage
    emitter.instruction(&format!("ldr x9, [x9, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal Mixed storage
    emitter.instruction("str x9, [sp, #8]");                                    // save storage pointer
    emitter.instruction("ldr x10, [x9]");                                       // read list length
    emitter.instruction("str x10, [sp, #16]");                                  // save list length
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize serialized item cursor
    emitter.label("__rt_spl_dll_serialize_loop");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload item cursor
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload item count
    emitter.instruction("cmp x10, x11");                                        // have all list items been serialized?
    emitter.instruction("b.ge __rt_spl_dll_serialize_done");                    // finish once every item was emitted
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload output cursor
    emitter.instruction("mov w13, #58");                                        // ASCII ':' prefixes each legacy item payload
    emitter.instruction("strb w13, [x12], #1");                                 // write item separator
    emitter.instruction("str x12, [sp, #40]");                                  // save cursor before unboxing item
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload internal storage pointer
    emitter.instruction("add x9, x9, #24");                                     // point at first storage slot
    emitter.instruction("ldr x0, [x9, x10, lsl #3]");                           // load source Mixed cell pointer
    emitter.instruction("bl __rt_mixed_unbox");                                 // inspect the concrete runtime tag and payload
    emitter.instruction("str x0, [sp, #48]");                                   // save unboxed tag
    emitter.instruction("str x1, [sp, #56]");                                   // save unboxed low payload
    emitter.instruction("str x2, [sp, #64]");                                   // save unboxed high payload
    emitter.instruction(&format!("cmp x0, #{}", INT_TAG));                      // is this item an integer?
    emitter.instruction("b.eq __rt_spl_dll_serialize_int");                     // encode integer values as i:<n>;
    emitter.instruction(&format!("cmp x0, #{}", STR_TAG));                      // is this item a string?
    emitter.instruction("b.eq __rt_spl_dll_serialize_string");                  // encode string values as s:<len>:\"...\";
    emitter.instruction(&format!("cmp x0, #{}", BOOL_TAG));                     // is this item a boolean?
    emitter.instruction("b.eq __rt_spl_dll_serialize_bool");                    // encode boolean values as b:0; or b:1;
    emitter.instruction("b __rt_spl_dll_serialize_null");                       // unsupported and null-like values serialize as N;
    emitter.label("__rt_spl_dll_serialize_int");
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload output cursor
    emitter.instruction("mov w13, #105");                                       // ASCII 'i' marks integer payloads
    emitter.instruction("strb w13, [x12], #1");                                 // write integer tag
    emitter.instruction("mov w13, #58");                                        // ASCII ':' separates integer tag and digits
    emitter.instruction("strb w13, [x12], #1");                                 // write integer separator
    emitter.instruction("ldr x0, [sp, #56]");                                   // pass integer payload to decimal writer
    emitter.instruction("mov x1, x12");                                         // pass output cursor to decimal writer
    emitter.instruction("bl __rt_spl_dll_write_dec");                           // append signed decimal integer text
    emitter.instruction("mov w13, #59");                                        // ASCII ';' closes the integer payload
    emitter.instruction("strb w13, [x1], #1");                                  // write integer terminator
    emitter.instruction("str x1, [sp, #40]");                                   // save updated output cursor
    emitter.instruction("b __rt_spl_dll_serialize_next_item");                  // continue with the next list item
    emitter.label("__rt_spl_dll_serialize_string");
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload output cursor
    emitter.instruction("mov w13, #115");                                       // ASCII 's' marks string payloads
    emitter.instruction("strb w13, [x12], #1");                                 // write string tag
    emitter.instruction("mov w13, #58");                                        // ASCII ':' separates string tag and length
    emitter.instruction("strb w13, [x12], #1");                                 // write string length separator
    emitter.instruction("ldr x0, [sp, #64]");                                   // pass string length to decimal writer
    emitter.instruction("mov x1, x12");                                         // pass output cursor to decimal writer
    emitter.instruction("bl __rt_spl_dll_write_dec");                           // append string byte length
    emitter.instruction("mov w13, #58");                                        // ASCII ':' separates length from quoted bytes
    emitter.instruction("strb w13, [x1], #1");                                  // write separator before opening quote
    emitter.instruction("mov w13, #34");                                        // ASCII '\"' opens the serialized string bytes
    emitter.instruction("strb w13, [x1], #1");                                  // write opening quote
    emitter.instruction("ldr x10, [sp, #56]");                                  // load source string pointer
    emitter.instruction("ldr x11, [sp, #64]");                                  // load remaining source string length
    emitter.label("__rt_spl_dll_serialize_string_copy");
    emitter.instruction("cbz x11, __rt_spl_dll_serialize_string_done");         // stop copying once all string bytes are written
    emitter.instruction("ldrb w13, [x10], #1");                                 // read next source string byte
    emitter.instruction("strb w13, [x1], #1");                                  // append raw string byte to serialized payload
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_spl_dll_serialize_string_copy");                // continue copying raw bytes
    emitter.label("__rt_spl_dll_serialize_string_done");
    emitter.instruction("mov w13, #34");                                        // ASCII '\"' closes the serialized string bytes
    emitter.instruction("strb w13, [x1], #1");                                  // write closing quote
    emitter.instruction("mov w13, #59");                                        // ASCII ';' closes the string payload
    emitter.instruction("strb w13, [x1], #1");                                  // write string terminator
    emitter.instruction("str x1, [sp, #40]");                                   // save updated output cursor
    emitter.instruction("b __rt_spl_dll_serialize_next_item");                  // continue with the next list item
    emitter.label("__rt_spl_dll_serialize_bool");
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload output cursor
    emitter.instruction("mov w13, #98");                                        // ASCII 'b' marks boolean payloads
    emitter.instruction("strb w13, [x12], #1");                                 // write boolean tag
    emitter.instruction("mov w13, #58");                                        // ASCII ':' separates boolean tag and value
    emitter.instruction("strb w13, [x12], #1");                                 // write boolean separator
    emitter.instruction("ldr x10, [sp, #56]");                                  // load boolean payload
    emitter.instruction("cmp x10, #0");                                         // choose ASCII 0 or 1 from the boolean value
    emitter.instruction("mov w13, #48");                                        // default to ASCII '0'
    emitter.instruction("cinc w13, w13, ne");                                   // turn it into ASCII '1' when payload is truthy
    emitter.instruction("strb w13, [x12], #1");                                 // write boolean digit
    emitter.instruction("mov w13, #59");                                        // ASCII ';' closes the boolean payload
    emitter.instruction("strb w13, [x12], #1");                                 // write boolean terminator
    emitter.instruction("str x12, [sp, #40]");                                  // save updated output cursor
    emitter.instruction("b __rt_spl_dll_serialize_next_item");                  // continue with the next list item
    emitter.label("__rt_spl_dll_serialize_null");
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload output cursor
    emitter.instruction("mov w13, #78");                                        // ASCII 'N' marks null payloads
    emitter.instruction("strb w13, [x12], #1");                                 // write null tag
    emitter.instruction("mov w13, #59");                                        // ASCII ';' closes null payloads
    emitter.instruction("strb w13, [x12], #1");                                 // write null terminator
    emitter.instruction("str x12, [sp, #40]");                                  // save updated output cursor
    emitter.label("__rt_spl_dll_serialize_next_item");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload item cursor
    emitter.instruction("add x10, x10, #1");                                    // advance to the next physical storage slot
    emitter.instruction("str x10, [sp, #24]");                                  // save updated item cursor
    emitter.instruction("b __rt_spl_dll_serialize_loop");                       // serialize the next item
    emitter.label("__rt_spl_dll_serialize_done");
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload concat-buffer base
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload final output cursor
    emitter.instruction("sub x11, x10, x9");                                    // compute new global concat-buffer offset
    abi::emit_symbol_address(emitter, "x12", "_concat_off");
    emitter.instruction("str x11, [x12]");                                      // publish updated concat-buffer offset
    emitter.instruction("ldr x1, [sp, #32]");                                   // return serialized string pointer
    emitter.instruction("sub x2, x10, x1");                                     // return serialized string length
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release legacy serialization frame
    emitter.instruction("ret");                                                 // return serialized pointer/length pair
    emit_write_dec_aarch64(emitter);
}

/// Emits `__rt_spl_dll_write_dec` on ARM64: tail-call helper for decimal formatting.
/// Called from the serializer with value in x0 and output cursor in x1. Writes optional '-'
/// for negative values, then digits in base-10, using the stack to reverse digit order.
/// Returns the advanced output cursor in x1.
fn emit_write_dec_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_spl_dll_write_dec");
    emitter.instruction("sub sp, sp, #32");                                     // reserve temporary reversed digit storage
    emitter.instruction("mov x9, x1");                                          // keep output cursor in x9
    emitter.instruction("cmp x0, #0");                                          // check whether the value needs a leading minus sign
    emitter.instruction("b.ge __rt_spl_dll_write_dec_abs");                     // positive values can be encoded directly
    emitter.instruction("mov w10, #45");                                        // ASCII '-' marks negative decimal values
    emitter.instruction("strb w10, [x9], #1");                                  // write minus sign and advance output cursor
    emitter.instruction("neg x0, x0");                                          // convert value to positive magnitude
    emitter.label("__rt_spl_dll_write_dec_abs");
    emitter.instruction("cbnz x0, __rt_spl_dll_write_dec_loop_init");           // nonzero values use repeated division
    emitter.instruction("mov w10, #48");                                        // ASCII '0' handles the zero special case
    emitter.instruction("strb w10, [x9], #1");                                  // write the single zero digit
    emitter.instruction("b __rt_spl_dll_write_dec_done");                       // finish without using reversed storage
    emitter.label("__rt_spl_dll_write_dec_loop_init");
    emitter.instruction("mov x10, #0");                                         // digit count starts at zero
    emitter.label("__rt_spl_dll_write_dec_loop");
    emitter.instruction("mov x11, #10");                                        // divisor for base-10 formatting
    emitter.instruction("udiv x12, x0, x11");                                   // compute quotient
    emitter.instruction("msub x13, x12, x11, x0");                              // compute decimal remainder
    emitter.instruction("add x13, x13, #48");                                   // convert remainder to ASCII digit
    emitter.instruction("strb w13, [sp, x10]");                                 // store digit in reverse order
    emitter.instruction("add x10, x10, #1");                                    // count stored digit
    emitter.instruction("mov x0, x12");                                         // continue with quotient
    emitter.instruction("cbnz x0, __rt_spl_dll_write_dec_loop");                // keep extracting until quotient reaches zero
    emitter.label("__rt_spl_dll_write_dec_copy");
    emitter.instruction("subs x10, x10, #1");                                   // move to the previous reversed digit
    emitter.instruction("b.lt __rt_spl_dll_write_dec_done");                    // finish once all digits were copied forward
    emitter.instruction("ldrb w13, [sp, x10]");                                 // load next forward-order digit
    emitter.instruction("strb w13, [x9], #1");                                  // append digit to output
    emitter.instruction("b __rt_spl_dll_write_dec_copy");                       // continue copying forward-order digits
    emitter.label("__rt_spl_dll_write_dec_done");
    emitter.instruction("mov x1, x9");                                          // return advanced output cursor
    emitter.instruction("add sp, sp, #32");                                     // release temporary reversed digit storage
    emitter.instruction("ret");                                                 // return to serializer
}

/// Emits `__rt_spl_dll_unserialize` on ARM64: receiver in x0, input pointer in x1,
/// input length in x2. Clears existing storage by releasing all owned Mixed cells,
/// parses the legacy serialized format "i:<mode>;<item>..." using `__rt_spl_dll_parse_dec`,
/// and appends each parsed value via `__rt_spl_dll_push`. Throws on malformed input.
fn emit_unserialize_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_unserialize");
    emitter.instruction("sub sp, sp, #144");                                    // reserve parser, cursor, and clear-loop state
    emitter.instruction("stp x29, x30, [sp, #128]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #128");                                   // establish legacy unserialization frame
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver
    emitter.instruction("add x9, x1, x2");                                      // compute input end pointer
    emitter.instruction("str x9, [sp, #16]");                                   // save input end pointer
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load current internal storage
    emitter.instruction("str x9, [sp, #24]");                                   // save current storage pointer for clearing
    emitter.instruction("ldr x10, [x9]");                                       // read current list length
    emitter.instruction("str x10, [sp, #32]");                                  // save current list length
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize clear-loop cursor
    emitter.label("__rt_spl_dll_unserialize_clear_loop");
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload clear-loop cursor
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload old list length
    emitter.instruction("cmp x10, x11");                                        // has every old cell been released?
    emitter.instruction("b.ge __rt_spl_dll_unserialize_clear_done");            // finish clearing once cursor reaches old length
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload storage pointer
    emitter.instruction("add x9, x9, #24");                                     // point at first storage cell
    emitter.instruction("ldr x0, [x9, x10, lsl #3]");                           // load old Mixed cell pointer
    emitter.instruction("str x9, [sp, #72]");                                   // save slot base across decref
    emitter.instruction("str x10, [sp, #40]");                                  // save clear-loop cursor across decref
    emitter.instruction("cbz x0, __rt_spl_dll_unserialize_clear_next");         // skip empty slots defensively
    emitter.instruction("bl __rt_decref_mixed");                                // release old storage-owned Mixed cell
    emitter.label("__rt_spl_dll_unserialize_clear_next");
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload slot base after decref
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload clear-loop cursor after decref
    emitter.instruction("str xzr, [x9, x10, lsl #3]");                          // clear stale slot
    emitter.instruction("add x10, x10, #1");                                    // advance clear-loop cursor
    emitter.instruction("str x10, [sp, #40]");                                  // save updated clear-loop cursor
    emitter.instruction("b __rt_spl_dll_unserialize_clear_loop");               // continue releasing old cells
    emitter.label("__rt_spl_dll_unserialize_clear_done");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload storage pointer after clearing
    emitter.instruction("str xzr, [x9]");                                       // reset internal storage length to zero
    emitter.instruction("add x9, x1, #2");                                      // skip the leading legacy 'i:' flags prefix
    emitter.instruction("mov x0, x9");                                          // pass cursor to decimal parser
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass input end pointer to decimal parser
    emitter.instruction("bl __rt_spl_dll_parse_dec");                           // parse iterator mode flags
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver for mode update
    emitter.instruction(&format!("str x0, [x9, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // restore serialized iterator mode
    emitter.instruction("mov x10, x1");                                         // move parser cursor to scratch
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload input end pointer
    emitter.instruction("cmp x10, x11");                                        // is there a semicolon after flags?
    emitter.instruction("b.hs __rt_spl_dll_unserialize_store_cursor");          // avoid reading past input
    emitter.instruction("ldrb w12, [x10]");                                     // inspect flags terminator
    emitter.instruction("cmp w12, #59");                                        // ASCII ';' closes the flags integer
    emitter.instruction("cinc x10, x10, eq");                                   // skip the terminator when present
    emitter.label("__rt_spl_dll_unserialize_store_cursor");
    emitter.instruction("str x10, [sp, #8]");                                   // save item parser cursor
    emitter.label("__rt_spl_dll_unserialize_loop");
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload parser cursor
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload input end pointer
    emitter.instruction("cmp x10, x11");                                        // did parser reach the input end?
    emitter.instruction("b.hs __rt_spl_dll_unserialize_done");                  // finish once cursor reaches input end
    emitter.instruction("ldrb w12, [x10]");                                     // read next serialized byte
    emitter.instruction("cmp w12, #58");                                        // items are prefixed by ASCII ':'
    emitter.instruction("b.ne __rt_spl_dll_unserialize_done");                  // stop on malformed trailing data
    emitter.instruction("add x10, x10, #1");                                    // skip item separator
    emitter.instruction("ldrb w12, [x10]");                                     // read item type tag
    emitter.instruction("cmp w12, #105");                                       // ASCII 'i' marks integer payloads
    emitter.instruction("b.eq __rt_spl_dll_unserialize_int");                   // parse integer payload
    emitter.instruction("cmp w12, #115");                                       // ASCII 's' marks string payloads
    emitter.instruction("b.eq __rt_spl_dll_unserialize_string");                // parse string payload
    emitter.instruction("cmp w12, #98");                                        // ASCII 'b' marks boolean payloads
    emitter.instruction("b.eq __rt_spl_dll_unserialize_bool");                  // parse boolean payload
    emitter.instruction("cmp w12, #78");                                        // ASCII 'N' marks null payloads
    emitter.instruction("b.eq __rt_spl_dll_unserialize_null");                  // parse null payload
    emitter.instruction("b __rt_spl_dll_unserialize_done");                     // stop on unsupported malformed payloads
    emitter.label("__rt_spl_dll_unserialize_int");
    emitter.instruction("add x10, x10, #2");                                    // skip 'i:' before integer digits
    emitter.instruction("mov x0, x10");                                         // pass integer digit cursor
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass input end pointer
    emitter.instruction("bl __rt_spl_dll_parse_dec");                           // parse signed integer value
    emitter.instruction("str x0, [sp, #56]");                                   // save parsed integer payload
    emitter.instruction("mov x10, x1");                                         // move parser cursor to scratch
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload input end pointer
    emitter.instruction("cmp x10, x11");                                        // can the parser inspect an integer terminator?
    emitter.instruction("b.hs __rt_spl_dll_unserialize_int_box");               // avoid reading past input
    emitter.instruction("ldrb w12, [x10]");                                     // inspect integer terminator
    emitter.instruction("cmp w12, #59");                                        // ASCII ';' closes integer payloads
    emitter.instruction("cinc x10, x10, eq");                                   // skip integer terminator when present
    emitter.label("__rt_spl_dll_unserialize_int_box");
    emitter.instruction("str x10, [sp, #8]");                                   // save parser cursor after integer payload
    emitter.instruction(&format!("mov x0, #{}", INT_TAG));                      // runtime tag 0 = integer
    emitter.instruction("ldr x1, [sp, #56]");                                   // pass parsed integer payload
    emitter.instruction("mov x2, xzr");                                         // integer payload has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box integer for list storage
    emit_unserialize_append_aarch64(emitter);
    emitter.instruction("b __rt_spl_dll_unserialize_loop");                     // continue parsing following items
    emitter.label("__rt_spl_dll_unserialize_string");
    emitter.instruction("add x10, x10, #2");                                    // skip 's:' before byte length
    emitter.instruction("mov x0, x10");                                         // pass string length cursor
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass input end pointer
    emitter.instruction("bl __rt_spl_dll_parse_dec");                           // parse string byte length
    emitter.instruction("str x0, [sp, #64]");                                   // save parsed string length
    emitter.instruction("mov x10, x1");                                         // move parser cursor to scratch
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload input end pointer
    emitter.instruction("cmp x10, x11");                                        // can the parser skip the length separator?
    emitter.instruction("b.hs __rt_spl_dll_unserialize_string_box");            // malformed short input becomes an empty tail
    emitter.instruction("ldrb w12, [x10]");                                     // inspect length separator
    emitter.instruction("cmp w12, #58");                                        // ASCII ':' follows the byte length
    emitter.instruction("cinc x10, x10, eq");                                   // skip length separator when present
    emitter.instruction("cmp x10, x11");                                        // can the parser skip the opening quote?
    emitter.instruction("b.hs __rt_spl_dll_unserialize_string_box");            // stop skipping on short input
    emitter.instruction("ldrb w12, [x10]");                                     // inspect opening quote
    emitter.instruction("cmp w12, #34");                                        // ASCII '\"' opens raw string bytes
    emitter.instruction("cinc x10, x10, eq");                                   // skip opening quote when present
    emitter.instruction("str x10, [sp, #72]");                                  // save source string payload pointer
    emitter.instruction("ldr x12, [sp, #64]");                                  // reload string byte length
    emitter.instruction("add x10, x10, x12");                                   // skip raw string payload bytes
    emitter.instruction("cmp x10, x11");                                        // can the parser skip the closing quote?
    emitter.instruction("b.hs __rt_spl_dll_unserialize_string_save_cursor");    // avoid reading past input
    emitter.instruction("ldrb w12, [x10]");                                     // inspect closing quote
    emitter.instruction("cmp w12, #34");                                        // ASCII '\"' closes raw string bytes
    emitter.instruction("cinc x10, x10, eq");                                   // skip closing quote when present
    emitter.instruction("cmp x10, x11");                                        // can the parser skip the string terminator?
    emitter.instruction("b.hs __rt_spl_dll_unserialize_string_save_cursor");    // avoid reading past input
    emitter.instruction("ldrb w12, [x10]");                                     // inspect string terminator
    emitter.instruction("cmp w12, #59");                                        // ASCII ';' closes string payloads
    emitter.instruction("cinc x10, x10, eq");                                   // skip string terminator when present
    emitter.label("__rt_spl_dll_unserialize_string_save_cursor");
    emitter.instruction("str x10, [sp, #8]");                                   // save parser cursor after string payload
    emitter.label("__rt_spl_dll_unserialize_string_box");
    emitter.instruction(&format!("mov x0, #{}", STR_TAG));                      // runtime tag 1 = string
    emitter.instruction("ldr x1, [sp, #72]");                                   // pass raw string payload pointer
    emitter.instruction("ldr x2, [sp, #64]");                                   // pass raw string payload length
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box string for list storage
    emit_unserialize_append_aarch64(emitter);
    emitter.instruction("b __rt_spl_dll_unserialize_loop");                     // continue parsing following items
    emitter.label("__rt_spl_dll_unserialize_bool");
    emitter.instruction("ldrb w12, [x10, #2]");                                 // read boolean digit after 'b:'
    emitter.instruction("cmp w12, #49");                                        // ASCII '1' means true
    emitter.instruction("cset x1, eq");                                         // boolean payload is 1 for true, 0 for false
    emitter.instruction("add x10, x10, #4");                                    // skip the fixed b:<digit>; payload
    emitter.instruction("str x10, [sp, #8]");                                   // save parser cursor after boolean payload
    emitter.instruction(&format!("mov x0, #{}", BOOL_TAG));                     // runtime tag 3 = bool
    emitter.instruction("mov x2, xzr");                                         // boolean payload has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box boolean for list storage
    emit_unserialize_append_aarch64(emitter);
    emitter.instruction("b __rt_spl_dll_unserialize_loop");                     // continue parsing following items
    emitter.label("__rt_spl_dll_unserialize_null");
    emitter.instruction("add x10, x10, #2");                                    // skip the fixed N; payload
    emitter.instruction("str x10, [sp, #8]");                                   // save parser cursor after null payload
    emitter.instruction(&format!("mov x0, #{}", NULL_TAG));                     // runtime tag 8 = null
    emitter.instruction("mov x1, xzr");                                         // null payload low word is empty
    emitter.instruction("mov x2, xzr");                                         // null payload high word is empty
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for list storage
    emit_unserialize_append_aarch64(emitter);
    emitter.instruction("b __rt_spl_dll_unserialize_loop");                     // continue parsing following items
    emitter.label("__rt_spl_dll_unserialize_done");
    emitter.instruction("ldp x29, x30, [sp, #128]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #144");                                    // release parser frame
    emitter.instruction("ret");                                                 // return void
    emit_parse_dec_aarch64(emitter);
}

/// Emits the append helper on ARM64 for unserialization: moves the boxed parsed value
/// from x0 into x1 for the push call, reloads the receiver from the stack, and tail-calls
/// `__rt_spl_dll_push` to append the value to the list.
fn emit_unserialize_append_aarch64(emitter: &mut Emitter) {
    emitter.instruction("mov x1, x0");                                          // move boxed parsed value into push value argument
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload receiver for shared list append
    emitter.instruction("bl __rt_spl_dll_push");                                // append parsed value to internal storage
}

/// Emits `__rt_spl_dll_parse_dec` on ARM64: cursor in x0, input end in x1. Parses an optional
/// leading '-' for negative values, then reads consecutive ASCII decimal digits. Returns
/// the parsed integer in x0 and the advanced cursor in x1. Empty input returns zero.
fn emit_parse_dec_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_spl_dll_parse_dec");
    emitter.instruction("mov x9, x0");                                          // keep parser cursor in x9
    emitter.instruction("mov x10, xzr");                                        // initialize decimal accumulator
    emitter.instruction("mov x11, xzr");                                        // initialize negative flag
    emitter.instruction("cmp x9, x1");                                          // is the cursor already at the input end?
    emitter.instruction("b.hs __rt_spl_dll_parse_dec_finish");                  // empty digit runs parse as zero
    emitter.instruction("ldrb w12, [x9]");                                      // inspect optional sign byte
    emitter.instruction("cmp w12, #45");                                        // ASCII '-' starts negative values
    emitter.instruction("b.ne __rt_spl_dll_parse_dec_loop");                    // unsigned values start directly at digit parsing
    emitter.instruction("mov x11, #1");                                         // remember that the parsed number is negative
    emitter.instruction("add x9, x9, #1");                                      // skip the minus sign
    emitter.label("__rt_spl_dll_parse_dec_loop");
    emitter.instruction("cmp x9, x1");                                          // stop if cursor reached the input end
    emitter.instruction("b.hs __rt_spl_dll_parse_dec_sign");                    // finish and apply the sign
    emitter.instruction("ldrb w12, [x9]");                                      // load next potential digit
    emitter.instruction("sub w12, w12, #48");                                   // convert ASCII byte to digit candidate
    emitter.instruction("cmp w12, #9");                                         // check whether digit candidate is in range
    emitter.instruction("b.hi __rt_spl_dll_parse_dec_sign");                    // stop at the first non-digit byte
    emitter.instruction("mov x13, #10");                                        // decimal multiplier
    emitter.instruction("mul x10, x10, x13");                                   // shift accumulator by one decimal place
    emitter.instruction("add x10, x10, x12");                                   // append parsed digit
    emitter.instruction("add x9, x9, #1");                                      // advance parser cursor
    emitter.instruction("b __rt_spl_dll_parse_dec_loop");                       // continue parsing digits
    emitter.label("__rt_spl_dll_parse_dec_sign");
    emitter.instruction("cbz x11, __rt_spl_dll_parse_dec_finish");              // skip negation for positive values
    emitter.instruction("neg x10, x10");                                        // apply negative sign
    emitter.label("__rt_spl_dll_parse_dec_finish");
    emitter.instruction("mov x0, x10");                                         // return parsed integer value
    emitter.instruction("mov x1, x9");                                          // return cursor positioned after digits
    emitter.instruction("ret");                                                 // return to parser
}

/// Emits `__rt_spl_dll_rewind` on ARM64: receiver in x0. Resets the iterator index based on
/// the iterator mode. FIFO mode sets index to 0. LIFO mode sets index to length-1 (the last
/// element). Empty storage resets index to zero. Returns void.
fn emit_rewind_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_rewind");
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("ldr x10, [x9]");                                       // read storage length
    emitter.instruction("cbz x10, __rt_spl_dll_rewind_empty");                  // empty storage rewinds to zero
    emitter.instruction(&format!("ldr x11, [x0, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode bits
    emitter.instruction(&format!("tst x11, #{}", ITER_MODE_LIFO));              // is LIFO traversal requested?
    emitter.instruction("b.eq __rt_spl_dll_rewind_fifo");                       // FIFO traversal starts at index zero
    emitter.instruction("sub x10, x10, #1");                                    // LIFO traversal starts at the last element
    emitter.instruction(&format!("str x10, [x0, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // store starting LIFO index
    emitter.instruction("ret");                                                 // return void
    emitter.label("__rt_spl_dll_rewind_fifo");
    emitter.instruction(&format!("str xzr, [x0, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // store starting FIFO index
    emitter.instruction("ret");                                                 // return void
    emitter.label("__rt_spl_dll_rewind_empty");
    emitter.instruction(&format!("str xzr, [x0, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // reset empty iterators to index zero
    emitter.instruction("ret");                                                 // return void
}

/// Emits `__rt_spl_dll_next` and `__rt_spl_dll_prev` on ARM64. Both dispatch to
/// `emit_iterator_step_aarch64` with forward=true for next and forward=false for prev.
/// Receivers and index/mode registers are loaded in `emit_iterator_step_aarch64`.
fn emit_next_prev_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_next");
    emit_iterator_step_aarch64(emitter, true);
    emitter.label_global("__rt_spl_dll_prev");
    emit_iterator_step_aarch64(emitter, false);
}

/// Emits the shared iterator step logic on ARM64. Forward=true is `next()`, false is `prev()`.
/// Loads iterator index and mode from the receiver. Handles FIFO vs LIFO direction, delete-mode
/// foreach (ITER_MODE_DELETE), and boundary exhaustion. For delete-mode, calls `shift()` or
/// `pop()` and releases the removed cell. Persists the updated index and returns void.
fn emit_iterator_step_aarch64(emitter: &mut Emitter, forward: bool) {
    let fifo_label = if forward {
        "__rt_spl_dll_next_fifo"
    } else {
        "__rt_spl_dll_prev_fifo"
    };
    let delete_label = if forward {
        "__rt_spl_dll_next_delete"
    } else {
        ""
    };
    let done_label = if forward {
        "__rt_spl_dll_next_done"
    } else {
        "__rt_spl_dll_prev_done"
    };
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // load current iterator index
    emitter.instruction(&format!("ldr x10, [x0, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode bits
    if forward {
        emitter.instruction(&format!("tst x10, #{}", ITER_MODE_DELETE));        // does next() need to delete the current element?
        emitter.instruction(&format!("b.ne {}", delete_label));                 // delete-mode foreach advances by removing the current slot
    }
    emitter.instruction(&format!("tst x10, #{}", ITER_MODE_LIFO));              // is traversal currently LIFO?
    emitter.instruction(&format!("b.eq {}", fifo_label));                       // FIFO and LIFO move in opposite directions
    if forward {
        emitter.instruction("cbz x9, __rt_spl_dll_next_lifo_exhaust");          // moving forward in LIFO from zero exhausts the iterator
        emitter.instruction("sub x9, x9, #1");                                  // otherwise move to the previous numeric index
        emitter.instruction(&format!("str x9, [x0, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // persist decremented LIFO iterator index
        emitter.instruction("ret");                                             // return void
        emitter.label("__rt_spl_dll_next_lifo_exhaust");
        emitter.instruction(&format!("ldr x10, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load storage to compute exhausted sentinel
        emitter.instruction("ldr x10, [x10]");                                  // storage length is the invalid sentinel
        emitter.instruction(&format!("str x10, [x0, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // store exhausted LIFO sentinel
        emitter.instruction("ret");                                             // return void
    } else {
        emitter.instruction("add x9, x9, #1");                                  // moving prev in LIFO increases the numeric index
        emitter.instruction(&format!("str x9, [x0, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // persist incremented LIFO iterator index
        emitter.instruction("ret");                                             // return void
    }
    emitter.label(fifo_label);
    if forward {
        emitter.instruction("add x9, x9, #1");                                  // moving forward in FIFO increases the index
    } else {
        emitter.instruction("cbz x9, __rt_spl_dll_prev_fifo_done");             // moving before zero leaves the iterator exhausted at zero
        emitter.instruction("sub x9, x9, #1");                                  // otherwise move one FIFO slot backward
    }
    emitter.instruction(&format!("str x9, [x0, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // persist updated FIFO iterator index
    emitter.label(done_label);
    if !forward {
        emitter.label("__rt_spl_dll_prev_fifo_done");
    }
    emitter.instruction("ret");                                                 // return void
    if forward {
        emit_iterator_delete_step_aarch64(emitter, delete_label);
    }
}

/// Emits the delete-mode iterator step helper on ARM64, called from `emit_iterator_step_aarch64`
/// when ITER_MODE_DELETE is set. If LIFO mode: calls `__rt_spl_dll_pop`, releases the cell, and
/// sets index to the new tail. If FIFO mode: calls `__rt_spl_dll_shift`, releases the cell, and
/// resets index to zero. Empty storage after deletion rewinds to index zero.
fn emit_iterator_delete_step_aarch64(emitter: &mut Emitter, delete_label: &str) {
    emitter.label(delete_label);
    emitter.instruction("sub sp, sp, #32");                                     // reserve delete-mode iterator frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish delete-mode iterator frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve receiver across pop/shift and release
    emitter.instruction(&format!("tst x10, #{}", ITER_MODE_LIFO));              // choose which end delete-mode traversal removes from
    emitter.instruction("b.ne __rt_spl_dll_next_delete_lifo");                  // LIFO delete removes the tail element
    emitter.instruction("bl __rt_spl_dll_shift");                               // FIFO delete removes the head element and compacts storage
    emitter.instruction("bl __rt_decref_mixed");                                // release the removed storage-owned Mixed cell
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver after deletion
    emitter.instruction(&format!("str xzr, [x9, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // FIFO delete keeps iteration at the new head
    emitter.instruction("b __rt_spl_dll_next_delete_done");                     // finish delete-mode next()
    emitter.label("__rt_spl_dll_next_delete_lifo");
    emitter.instruction("bl __rt_spl_dll_pop");                                 // LIFO delete removes the current tail element
    emitter.instruction("bl __rt_decref_mixed");                                // release the removed storage-owned Mixed cell
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver after deletion
    emitter.instruction(&format!("ldr x10, [x9, #{}]", SPL_DLL_STORAGE_OFFSET)); // load storage to find the new tail
    emitter.instruction("ldr x10, [x10]");                                      // read storage length after deletion
    emitter.instruction("cbz x10, __rt_spl_dll_next_delete_empty");             // empty storage rewinds to index zero
    emitter.instruction("sub x10, x10, #1");                                    // new LIFO current index is the new tail
    emitter.instruction(&format!("str x10, [x9, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // persist new LIFO delete cursor
    emitter.instruction("b __rt_spl_dll_next_delete_done");                     // finish non-empty LIFO delete
    emitter.label("__rt_spl_dll_next_delete_empty");
    emitter.instruction(&format!("str xzr, [x9, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // reset exhausted delete-mode iterator to zero
    emitter.label("__rt_spl_dll_next_delete_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release delete-mode iterator frame
    emitter.instruction("ret");                                                 // return void
}

/// Emits `__rt_spl_dll_valid` on ARM64: receiver in x0. Reads iterator index and storage
/// length, returns a boolean in x0 (1 when index < length, 0 otherwise).
fn emit_valid_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_valid");
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("ldr x9, [x9]");                                        // read storage length
    emitter.instruction(&format!("ldr x10, [x0, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // read current iterator index
    emitter.instruction("cmp x10, x9");                                         // valid when index is below length
    emitter.instruction("cset x0, lo");                                         // return boolean validity
    emitter.instruction("ret");                                                 // return boolean result
}

/// Emits `__rt_spl_dll_current` on ARM64: receiver in x0. Loads the current Mixed cell
/// at the iterator index, retains it with `__rt_incref`, and returns it. Returns a boxed
/// null (via `emit_tail_boxed_null_aarch64`) when the index is out of range.
fn emit_current_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_current");
    emitter.instruction("sub sp, sp, #32");                                     // reserve frame for the incref call
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a frame for nested incref
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("ldr x10, [x9]");                                       // read storage length
    emitter.instruction(&format!("ldr x11, [x0, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // read iterator index
    emitter.instruction("cmp x11, x10");                                        // is the iterator index inside storage?
    emitter.instruction("b.hs __rt_spl_dll_current_null");                      // invalid current() returns null
    emitter.instruction("add x12, x9, #24");                                    // point at first storage element
    emitter.instruction("ldr x0, [x12, x11, lsl #3]");                          // load the current Mixed cell
    emitter.instruction("bl __rt_incref");                                      // retain current Mixed cell for the caller
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release frame
    emitter.instruction("ret");                                                 // return retained Mixed cell
    emitter.label("__rt_spl_dll_current_null");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer before null tail-call
    emitter.instruction("add sp, sp, #32");                                     // release frame before null tail-call
    emit_tail_boxed_null_aarch64(emitter);
}

/// Emits `__rt_spl_dll_key` on ARM64: receiver in x0. Reads the iterator index as the
/// integer key, boxes it as a tagged Mixed (INT_TAG), and returns it. Returns a boxed null
/// (via `emit_tail_boxed_null_aarch64`) when the index is out of range.
fn emit_key_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_key");
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("ldr x9, [x9]");                                        // read storage length
    emitter.instruction(&format!("ldr x1, [x0, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // read iterator index as integer key payload
    emitter.instruction("cmp x1, x9");                                          // is iterator index valid?
    emitter.instruction("b.hs __rt_spl_dll_key_null");                          // invalid key() returns null
    emitter.instruction(&format!("mov x0, #{}", INT_TAG));                      // runtime tag 0 = int key
    emitter.instruction("mov x2, xzr");                                         // integer keys do not use a high payload word
    emitter.instruction("b __rt_mixed_from_value");                             // box and return the integer key
    emitter.label("__rt_spl_dll_key_null");
    emit_tail_boxed_null_aarch64(emitter);
}

/// Emits `__rt_spl_dll_offset_exists` on ARM64: receiver in x0, boxed offset in x1.
/// Unboxes the offset, validates it is a non-negative integer within storage bounds,
/// converts LIFO logical offset to physical slot, and returns boolean in x0.
/// Throws TypeError for non-integer offsets.
fn emit_offset_exists_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_offset_exists");
    emit_offset_index_prefix_aarch64(
        emitter,
        "__rt_spl_dll_offset_exists_type_throw",
        "__rt_spl_dll_offset_exists_false",
        "__rt_spl_dll_offset_exists_index_ready",
    );
    emitter.instruction("mov x0, #1");                                          // return true for any in-range offset
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release offset helper frame
    emitter.instruction("ret");                                                 // return boolean true
    emitter.label("__rt_spl_dll_offset_exists_false");
    emitter.instruction("mov x0, #0");                                          // return false for invalid offsets
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release offset helper frame
    emitter.instruction("ret");                                                 // return boolean false
    emitter.label("__rt_spl_dll_offset_exists_type_throw");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release offset helper frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_dll_offset_exists_type_msg",
        SPL_DLL_OFFSET_EXISTS_TYPE_MSG_LEN,
    );
}

/// Emits `__rt_spl_dll_offset_get` on ARM64: receiver in x0, boxed offset in x1.
/// Unboxes the offset, validates it is a non-negative integer within storage bounds,
/// converts LIFO logical offset to physical slot, loads the selected Mixed cell,
/// retains it with `__rt_incref`, and returns it. Throws TypeError or OutOfRangeException
/// on invalid offset.
fn emit_offset_get_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_offset_get");
    emit_offset_index_prefix_aarch64(
        emitter,
        "__rt_spl_dll_offset_get_type_throw",
        "__rt_spl_dll_offset_get_range_throw",
        "__rt_spl_dll_offset_get_index_ready",
    );
    emitter.instruction("add x11, x9, #24");                                    // point at first storage element
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // load selected Mixed cell
    emitter.instruction("bl __rt_incref");                                      // retain selected Mixed cell for caller
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release offset helper frame
    emitter.instruction("ret");                                                 // return retained Mixed cell
    emitter.label("__rt_spl_dll_offset_get_type_throw");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release offset helper frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_dll_offset_get_type_msg",
        SPL_DLL_OFFSET_GET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_dll_offset_get_range_throw");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release offset helper frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_out_of_range_exception_class_id",
        "_spl_dll_offset_get_range_msg",
        SPL_DLL_OFFSET_GET_RANGE_MSG_LEN,
    );
}

/// Emits the shared offset validation preamble on ARM64: receiver in x0, boxed offset in x1.
/// Unboxes the offset, validates it is an integer and non-negative, loads storage and length,
/// and checks bounds. Converts LIFO logical offset to physical slot using ITER_MODE_LIFO.
/// Sets x9 = storage, x10 = physical index on success; jumps to type_label, range_label on
/// errors. Returns with the offset helper frame established but not yet cleaned up.
fn emit_offset_index_prefix_aarch64(
    emitter: &mut Emitter,
    type_label: &str,
    range_label: &str,
    ready_label: &str,
) {
    emitter.instruction("sub sp, sp, #64");                                     // reserve common offset helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a frame for Mixed unbox/release
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver
    emitter.instruction("str x1, [sp, #8]");                                    // save boxed offset argument
    emitter.instruction("mov x0, x1");                                          // pass boxed offset to mixed_unbox
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox offset into tag and payload words
    emitter.instruction("str x0, [sp, #16]");                                   // save unboxed offset tag
    emitter.instruction("str x1, [sp, #24]");                                   // save unboxed integer payload candidate
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload boxed offset argument
    emitter.instruction("bl __rt_decref_mixed");                                // release the owned boxed offset argument
    emitter.instruction("ldr x12, [sp, #16]");                                  // reload unboxed offset tag
    emitter.instruction(&format!("cmp x12, #{}", INT_TAG));                     // offset must be an integer for list addressing
    emitter.instruction(&format!("b.ne {}", type_label));                       // reject non-integer offsets
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload integer offset payload
    emitter.instruction("cmp x10, #0");                                         // reject negative offsets
    emitter.instruction(&format!("b.lt {}", range_label));                      // negative offsets are invalid
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload receiver
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("ldr x11, [x9]");                                       // read storage length
    emitter.instruction("cmp x10, x11");                                        // compare offset with length
    emitter.instruction(&format!("b.hs {}", range_label));                      // offsets past the end are invalid
    emitter.instruction(&format!("ldr x12, [x0, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode bits for logical indexing
    emitter.instruction(&format!("tst x12, #{}", ITER_MODE_LIFO));              // does the list expose offsets in LIFO order?
    emitter.instruction(&format!("b.eq {}", ready_label));                      // FIFO offsets already match physical storage
    emitter.instruction("sub x10, x11, x10");                                   // convert logical LIFO offset to one-based physical offset
    emitter.instruction("sub x10, x10, #1");                                    // finish zero-based physical offset
    emitter.label(ready_label);
}

/// Emits `__rt_spl_dll_offset_set` on ARM64: receiver in x0, boxed offset in x1, owned
/// value in x2. When offset is null, appends via `__rt_spl_dll_push`. Otherwise unboxes
/// offset, validates it is a non-negative integer within storage bounds, releases the old
/// Mixed cell at that slot, stores the replacement, and returns void. Converts LIFO
/// logical offset to physical slot. Throws TypeError or OutOfRangeException on invalid offset.
fn emit_offset_set_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_offset_set");
    emitter.instruction("sub sp, sp, #80");                                     // reserve offset-set helper frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a frame for nested release/append calls
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver
    emitter.instruction("str x1, [sp, #8]");                                    // save boxed offset argument
    emitter.instruction("str x2, [sp, #16]");                                   // save owned Mixed value argument
    emitter.instruction("mov x0, x1");                                          // pass boxed offset to mixed_unbox
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox offset into tag and payload words
    emitter.instruction("str x0, [sp, #24]");                                   // save offset tag
    emitter.instruction("str x1, [sp, #32]");                                   // save integer offset payload candidate
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload boxed offset argument
    emitter.instruction("bl __rt_decref_mixed");                                // release boxed offset argument
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload offset tag
    emitter.instruction(&format!("cmp x12, #{}", NULL_TAG));                    // null offset means append
    emitter.instruction("b.eq __rt_spl_dll_offset_set_append");                 // append when offset is null
    emitter.instruction(&format!("cmp x12, #{}", INT_TAG));                     // explicit offset must be integer
    emitter.instruction("b.ne __rt_spl_dll_offset_set_type_throw");             // reject non-integer offsets
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload integer offset
    emitter.instruction("cmp x10, #0");                                         // reject negative offsets
    emitter.instruction("b.lt __rt_spl_dll_offset_set_range_throw");            // negative offsets are out of range
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver
    emitter.instruction(&format!("ldr x14, [x9, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode bits for logical index mapping
    emitter.instruction(&format!("ldr x9, [x9, #{}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("ldr x11, [x9]");                                       // read storage length
    emitter.instruction("cmp x10, x11");                                        // compare explicit offset with current length
    emitter.instruction("b.hs __rt_spl_dll_offset_set_range_throw");            // explicit offsets at/past length are out of range
    emitter.instruction(&format!("tst x14, #{}", ITER_MODE_LIFO));              // does logical indexing run in LIFO order?
    emitter.instruction("b.eq __rt_spl_dll_offset_set_physical_index_ready");   // FIFO offsets already match physical storage
    emitter.instruction("sub x10, x11, x10");                                   // convert logical LIFO offset to one-based physical offset
    emitter.instruction("sub x10, x10, #1");                                    // finish zero-based physical offset
    emitter.label("__rt_spl_dll_offset_set_physical_index_ready");
    emitter.instruction("add x12, x9, #24");                                    // point at first storage element
    emitter.instruction("ldr x0, [x12, x10, lsl #3]");                          // load the old Mixed cell at this offset
    emitter.instruction("str x9, [sp, #40]");                                   // preserve storage across old-value release
    emitter.instruction("str x10, [sp, #48]");                                  // preserve offset across old-value release
    emitter.instruction("bl __rt_decref_mixed");                                // release old Mixed cell before overwriting
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload storage after release
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload offset after release
    emitter.instruction("add x12, x9, #24");                                    // point at first storage element
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload owned replacement Mixed cell
    emitter.instruction("str x13, [x12, x10, lsl #3]");                         // store replacement Mixed cell
    emitter.instruction("b __rt_spl_dll_offset_set_done");                      // finish offsetSet
    emitter.label("__rt_spl_dll_offset_set_append");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload receiver for append
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload owned Mixed value for append
    emitter.instruction("bl __rt_spl_dll_push");                                // append value using shared push helper
    emitter.instruction("b __rt_spl_dll_offset_set_done");                      // finish offsetSet after append
    emitter.label("__rt_spl_dll_offset_set_type_throw");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload rejected owned Mixed value
    emitter.instruction("bl __rt_decref_mixed");                                // release rejected value before throwing
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #80");                                     // release offset-set frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_dll_offset_set_type_msg",
        SPL_DLL_OFFSET_SET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_dll_offset_set_range_throw");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload owned Mixed value rejected by invalid offset
    emitter.instruction("bl __rt_decref_mixed");                                // release rejected value to avoid leaking argument ownership
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #80");                                     // release offset-set frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_out_of_range_exception_class_id",
        "_spl_dll_offset_set_range_msg",
        SPL_DLL_OFFSET_SET_RANGE_MSG_LEN,
    );
    emitter.label("__rt_spl_dll_offset_set_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release offset-set helper frame
    emitter.instruction("ret");                                                 // return void
}

/// Emits `__rt_spl_dll_offset_unset` on ARM64: receiver in x0, boxed offset in x1.
/// Validates offset using `emit_offset_index_prefix_aarch64`, releases the old Mixed cell
/// at that slot, compacts storage by shifting subsequent elements left, and returns void.
/// Throws TypeError or OutOfRangeException on invalid offset.
fn emit_offset_unset_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_offset_unset");
    emit_offset_index_prefix_aarch64(
        emitter,
        "__rt_spl_dll_offset_unset_type_throw",
        "__rt_spl_dll_offset_unset_range_throw",
        "__rt_spl_dll_offset_unset_index_ready",
    );
    emitter.instruction("add x12, x9, #24");                                    // point at first storage element
    emitter.instruction("ldr x0, [x12, x10, lsl #3]");                          // load removed Mixed cell
    emitter.instruction("str x9, [sp, #32]");                                   // preserve storage across removed-value release
    emitter.instruction("str x10, [sp, #40]");                                  // preserve removed index across release
    emitter.instruction("bl __rt_decref_mixed");                                // release removed Mixed cell
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload storage after release
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload removed index after release
    emitter.instruction("ldr x11, [x9]");                                       // reload old storage length
    emitter.instruction("add x12, x9, #24");                                    // point at first storage element
    emitter.instruction("add x13, x10, #1");                                    // start compaction after the removed slot
    emitter.label("__rt_spl_dll_offset_unset_shift_loop");
    emitter.instruction("cmp x13, x11");                                        // have all following elements shifted left?
    emitter.instruction("b.ge __rt_spl_dll_offset_unset_shrink");               // shrink once compaction is complete
    emitter.instruction("ldr x14, [x12, x13, lsl #3]");                         // load next Mixed pointer
    emitter.instruction("sub x15, x13, #1");                                    // compute destination index
    emitter.instruction("str x14, [x12, x15, lsl #3]");                         // shift Mixed pointer left by one slot
    emitter.instruction("add x13, x13, #1");                                    // advance compaction cursor
    emitter.instruction("b __rt_spl_dll_offset_unset_shift_loop");              // continue compaction
    emitter.label("__rt_spl_dll_offset_unset_shrink");
    emitter.instruction("sub x11, x11, #1");                                    // compute new length
    emitter.instruction("str x11, [x9]");                                       // persist shortened length
    emitter.instruction("str xzr, [x12, x11, lsl #3]");                         // clear stale tail slot
    emitter.label("__rt_spl_dll_offset_unset_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release offset helper frame
    emitter.instruction("ret");                                                 // return void
    emitter.label("__rt_spl_dll_offset_unset_type_throw");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release offset helper frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_dll_offset_unset_type_msg",
        SPL_DLL_OFFSET_UNSET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_dll_offset_unset_range_throw");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release offset helper frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_out_of_range_exception_class_id",
        "_spl_dll_offset_unset_range_msg",
        SPL_DLL_OFFSET_UNSET_RANGE_MSG_LEN,
    );
}

/// Emits the tail-call sequence to return a boxed null on ARM64: sets x0 to NULL_TAG (8),
/// x1 and x2 to zero, and tail-calls `__rt_mixed_from_value` to construct the boxed null.
fn emit_tail_boxed_null_aarch64(emitter: &mut Emitter) {
    emitter.instruction(&format!("mov x0, #{}", NULL_TAG));                     // runtime tag 8 = null
    emitter.instruction("mov x1, xzr");                                         // null payload low word is empty
    emitter.instruction("mov x2, xzr");                                         // null payload high word is empty
    emitter.instruction("b __rt_mixed_from_value");                             // tail-call boxed Mixed construction
}

/// Emits all x86_64 doubly linked list runtime helpers: constructor, mutators, iterators,
/// serialization, ArrayAccess methods, and exception helpers.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl doubly linked list ---");
    emit_new_x86_64(emitter);
    emit_count_x86_64(emitter);
    emit_is_empty_x86_64(emitter);
    emit_push_x86_64(emitter);
    emit_pop_x86_64(emitter);
    emit_shift_x86_64(emitter);
    emit_unshift_x86_64(emitter);
    emit_insert_x86_64(emitter);
    emit_top_x86_64(emitter);
    emit_bottom_x86_64(emitter);
    emit_iterator_mode_x86_64(emitter);
    emit_serialize_x86_64(emitter);
    emit_unserialize_x86_64(emitter);
    emit_serialize_array_x86_64(emitter);
    emit_rewind_x86_64(emitter);
    emit_next_prev_x86_64(emitter);
    emit_valid_x86_64(emitter);
    emit_current_x86_64(emitter);
    emit_key_x86_64(emitter);
    emit_offset_exists_x86_64(emitter);
    emit_offset_get_x86_64(emitter);
    emit_offset_set_x86_64(emitter);
    emit_offset_unset_x86_64(emitter);
}

/// Emits `__rt_spl_dll_new` on x86_64: allocates an SPL list object, initializes internal
/// Mixed-array storage with capacity 4, and stores iterator index/mode at their respective
/// offsets. Returns the initialized object pointer in rax.
fn emit_new_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_new");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for constructor spills
    emitter.instruction("mov rbp, rsp");                                        // establish constructor frame
    emitter.instruction("sub rsp, 16");                                         // reserve class-id and object-pointer spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save concrete SPL class id
    emitter.instruction(&format!("mov rax, {}", SPL_DLL_OBJECT_SIZE));          // request fixed SPL list object payload size
    emitter.instruction("call __rt_heap_alloc");                                // allocate the SPL list object payload
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize object heap kind with x86 marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp allocation as an object instance
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload concrete SPL class id
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class id at object header
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save object pointer while allocating storage
    emitter.instruction(&format!("mov rdi, {}", SPL_DLL_INITIAL_CAPACITY));     // initial internal storage capacity
    emitter.instruction("mov rsi, 8");                                          // each internal slot holds one Mixed pointer
    emitter.instruction("call __rt_array_new");                                 // allocate internal mixed-pointer storage
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load internal array packed kind word
    emitter.instruction("or r10, 0x700");                                       // mark internal storage as an array of Mixed cells
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // persist Mixed value_type tag on storage
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload object pointer
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", SPL_DLL_STORAGE_OFFSET)); // object.storage = internal Mixed array
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", SPL_DLL_ITER_INDEX_OFFSET)); // iterator index starts at zero
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", SPL_DLL_ITER_MODE_OFFSET)); // iterator mode starts FIFO/KEEP
    emitter.instruction("mov rax, r11");                                        // return initialized SPL object
    emitter.instruction("add rsp, 16");                                         // release constructor spills
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return object pointer
}

/// Emits `__rt_spl_dll_count` on x86_64: receiver in rdi. Loads the internal storage array
/// and returns its length as an integer in rax.
fn emit_count_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_count");
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // return internal storage length
    emitter.instruction("ret");                                                 // return count
}

/// Emits `__rt_spl_dll_is_empty` on x86_64: receiver in rdi. Reads storage length, compares
/// to zero, and returns a widened boolean (1 when empty, 0 when non-empty) in rax.
fn emit_is_empty_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_is_empty");
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("cmp QWORD PTR [r10], 0");                              // compare storage length with zero
    emitter.instruction("sete al");                                             // set low byte when list is empty
    emitter.instruction("movzx rax, al");                                       // widen boolean result
    emitter.instruction("ret");                                                 // return boolean result
}

/// Emits `__rt_spl_dll_push` on x86_64: receiver in rdi, owned Mixed value in rsi. Appends
/// the value to internal storage via `__rt_array_push_int`, handles growth, and returns void.
fn emit_push_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_push");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for append spill
    emitter.instruction("mov rbp, rsp");                                        // establish append frame
    emitter.instruction("sub rsp, 16");                                         // reserve receiver spill
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver while appending
    emitter.instruction(&format!("mov rdi, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // pass internal storage as array_push receiver
    emitter.instruction("call __rt_array_push_int");                            // append owned Mixed pointer without retaining it again
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload receiver after append
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], rax", SPL_DLL_STORAGE_OFFSET)); // store possibly-grown storage
    emitter.instruction("add rsp, 16");                                         // release append spill
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return void
}

/// Emits `__rt_spl_dll_pop` on x86_64: receiver in rdi. Removes and returns the last owned
/// Mixed cell, transferring ownership to the caller. Throws RuntimeException on an empty list.
fn emit_pop_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_pop");
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // read current storage length
    emitter.instruction("test r10, r10");                                       // is the list empty?
    emitter.instruction("jz __rt_spl_dll_pop_empty");                           // empty list raises PHP's RuntimeException
    emitter.instruction("sub r10, 1");                                          // compute last occupied index
    emitter.instruction("mov QWORD PTR [r9], r10");                             // shrink storage length by one
    emitter.instruction("lea r11, [r9 + 24]");                                  // point at first storage element
    emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");                  // return removed Mixed cell, transferring ownership
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8], 0");                    // clear stale slot beyond new length
    emitter.instruction("ret");                                                 // return removed Mixed cell
    emitter.label("__rt_spl_dll_pop_empty");
    emit_throw_exception_x86_64(
        emitter,
        "_spl_runtime_exception_class_id",
        "_spl_dll_pop_empty_msg",
        SPL_DLL_POP_EMPTY_MSG_LEN,
    );
}

/// Emits `__rt_spl_dll_shift` on x86_64: receiver in rdi. Removes and returns the first
/// owned Mixed cell, shifting all remaining elements left by one slot. Transfers ownership
/// to the caller. Throws RuntimeException on an empty list.
fn emit_shift_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_shift");
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // read current storage length
    emitter.instruction("test r10, r10");                                       // is the list empty?
    emitter.instruction("jz __rt_spl_dll_shift_empty");                         // empty list raises PHP's RuntimeException
    emitter.instruction("lea r11, [r9 + 24]");                                  // point at first storage element
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // capture removed first Mixed cell
    emitter.instruction("mov r12, 1");                                          // start shifting from element index 1
    emitter.label("__rt_spl_dll_shift_loop");
    emitter.instruction("cmp r12, r10");                                        // have all live elements been shifted left?
    emitter.instruction("jge __rt_spl_dll_shift_done");                         // finish once cursor reaches old length
    emitter.instruction("mov r13, QWORD PTR [r11 + r12 * 8]");                  // load next Mixed pointer
    emitter.instruction("mov r14, r12");                                        // copy source index for destination calculation
    emitter.instruction("sub r14, 1");                                          // destination index is one slot earlier
    emitter.instruction("mov QWORD PTR [r11 + r14 * 8], r13");                  // move Mixed pointer down by one slot
    emitter.instruction("add r12, 1");                                          // advance shift cursor
    emitter.instruction("jmp __rt_spl_dll_shift_loop");                         // continue compacting storage
    emitter.label("__rt_spl_dll_shift_done");
    emitter.instruction("sub r10, 1");                                          // compute new storage length
    emitter.instruction("mov QWORD PTR [r9], r10");                             // persist shortened length
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8], 0");                    // clear stale tail slot
    emitter.instruction("ret");                                                 // return removed Mixed cell
    emitter.label("__rt_spl_dll_shift_empty");
    emit_throw_exception_x86_64(
        emitter,
        "_spl_runtime_exception_class_id",
        "_spl_dll_shift_empty_msg",
        SPL_DLL_SHIFT_EMPTY_MSG_LEN,
    );
}

/// Emits `__rt_spl_dll_unshift` on x86_64: receiver in rdi, owned value in rsi. Moves rsi
/// to rdx for the insert helper's value argument and sets rsi to zero (index zero) before
/// tail-calling `__rt_spl_dll_insert`.
fn emit_unshift_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_unshift");
    emitter.instruction("mov rdx, rsi");                                        // move value to insert helper's value argument
    emitter.instruction("xor rsi, rsi");                                        // unshift inserts at index zero
    emitter.instruction("jmp __rt_spl_dll_insert");                             // tail-call shared insertion helper
}

/// Emits `__rt_spl_dll_insert` on x86_64: receiver in rdi, index in rsi, owned value in rdx.
/// Validates index >= 0 and index <= length, converts LIFO logical index to physical slot,
/// grows storage if needed, shifts elements right, and stores the value. Releases the value
/// and throws OutOfRangeException on invalid index.
fn emit_insert_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_insert");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for insertion state
    emitter.instruction("mov rbp, rsp");                                        // establish insertion frame
    emitter.instruction("sub rsp, 48");                                         // reserve receiver, index, value, storage, and length spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save requested insertion index
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save owned Mixed value to insert
    emitter.instruction("cmp rsi, 0");                                          // is requested index negative?
    emitter.instruction("jl __rt_spl_dll_insert_range_throw");                  // negative indexes are out of range in PHP
    emitter.label("__rt_spl_dll_insert_index_nonnegative");
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // save current storage pointer
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // read current storage length
    emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                         // read current storage capacity
    emitter.instruction("mov r12, QWORD PTR [rbp - 16]");                       // reload requested insertion index
    emitter.instruction("cmp r12, r10");                                        // is index past the end?
    emitter.instruction("jle __rt_spl_dll_insert_index_in_range");              // keep indexes within append boundary
    emitter.instruction("jmp __rt_spl_dll_insert_range_throw");                 // indexes past the end are out of range
    emitter.label("__rt_spl_dll_insert_index_in_range");
    emitter.instruction(&format!("mov r13, QWORD PTR [rdi + {}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode bits for logical index mapping
    emitter.instruction(&format!("test r13, {}", ITER_MODE_LIFO));              // does logical indexing run in LIFO order?
    emitter.instruction("jz __rt_spl_dll_insert_physical_index_ready");         // FIFO indexes already match physical storage
    emitter.instruction("cmp r12, r10");                                        // does LIFO insertion target the logical end?
    emitter.instruction("je __rt_spl_dll_insert_physical_index_ready");         // logical end still appends physically
    emitter.instruction("mov r12, r10");                                        // start converting logical LIFO index to physical slot
    emitter.instruction("sub r12, QWORD PTR [rbp - 16]");                       // compute one-based physical slot
    emitter.instruction("sub r12, 1");                                          // finish zero-based physical insertion index
    emitter.instruction("mov QWORD PTR [rbp - 16], r12");                       // save mapped physical insertion index
    emitter.label("__rt_spl_dll_insert_physical_index_ready");
    emitter.instruction("cmp r10, r11");                                        // is storage full?
    emitter.instruction("jne __rt_spl_dll_insert_have_capacity");               // skip growth when capacity remains
    emitter.instruction("mov rdi, r9");                                         // pass current storage to array_grow
    emitter.instruction("call __rt_array_grow");                                // grow internal storage
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver after growth
    emitter.instruction(&format!("mov QWORD PTR [r9 + {}], rax", SPL_DLL_STORAGE_OFFSET)); // store possibly-grown storage
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save grown storage
    emitter.label("__rt_spl_dll_insert_have_capacity");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload storage pointer
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // reload current length
    emitter.instruction("mov r12, QWORD PTR [rbp - 16]");                       // reload clamped insertion index
    emitter.instruction("lea r11, [r9 + 24]");                                  // point at first storage element
    emitter.instruction("mov r13, r10");                                        // start right-shift cursor at old length
    emitter.label("__rt_spl_dll_insert_shift_loop");
    emitter.instruction("cmp r13, r12");                                        // has cursor reached insertion index?
    emitter.instruction("jle __rt_spl_dll_insert_store");                       // stop once insertion slot is free
    emitter.instruction("mov r14, r13");                                        // copy destination index for source calculation
    emitter.instruction("sub r14, 1");                                          // source index is one slot before cursor
    emitter.instruction("mov r15, QWORD PTR [r11 + r14 * 8]");                  // load Mixed pointer being shifted right
    emitter.instruction("mov QWORD PTR [r11 + r13 * 8], r15");                  // store Mixed pointer one slot to the right
    emitter.instruction("sub r13, 1");                                          // move shift cursor left
    emitter.instruction("jmp __rt_spl_dll_insert_shift_loop");                  // continue shifting until insert slot opens
    emitter.label("__rt_spl_dll_insert_store");
    emitter.instruction("mov r14, QWORD PTR [rbp - 24]");                       // reload owned Mixed value to insert
    emitter.instruction("mov QWORD PTR [r11 + r12 * 8], r14");                  // store owned Mixed value in insertion slot
    emitter.instruction("add r10, 1");                                          // increase storage length
    emitter.instruction("mov QWORD PTR [r9], r10");                             // persist new storage length
    emitter.instruction("add rsp, 48");                                         // release insertion state
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return void
    emitter.label("__rt_spl_dll_insert_range_throw");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload rejected Mixed value before throwing
    emitter.instruction("call __rt_decref_mixed");                              // release the rejected insertion value
    emitter.instruction("add rsp, 48");                                         // release insertion state before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_out_of_range_exception_class_id",
        "_spl_dll_add_range_msg",
        SPL_DLL_ADD_RANGE_MSG_LEN,
    );
}

/// Emits `__rt_spl_dll_top` on x86_64: receiver in rdi. Loads the last occupied Mixed cell,
/// retains it with `__rt_incref` for the caller, and returns it. Throws RuntimeException on an
/// empty list by tail-calling `emit_peek_index_x86_64`.
fn emit_top_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_top");
    emit_peek_index_x86_64(emitter, "__rt_spl_dll_top_null", true);
}

/// Emits `__rt_spl_dll_bottom` on x86_64: receiver in rdi. Loads the first occupied Mixed
/// cell, retains it with `__rt_incref` for the caller, and returns it. Throws RuntimeException
/// on an empty list by tail-calling `emit_peek_index_x86_64`.
fn emit_bottom_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_bottom");
    emit_peek_index_x86_64(emitter, "__rt_spl_dll_bottom_null", false);
}

/// Emits the shared peek helper on x86_64: receiver in rdi, null_label and last flag select
/// which element to load (last=true picks index length-1, last=false picks index 0). Retains
/// the selected Mixed cell with `__rt_incref` before returning it. Jumps to null_label when
/// storage is empty, which throws RuntimeException via `emit_throw_exception_x86_64`.
fn emit_peek_index_x86_64(emitter: &mut Emitter, null_label: &str, last: bool) {
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // read storage length
    emitter.instruction("test r10, r10");                                       // is storage empty?
    emitter.instruction(&format!("jz {}", null_label));                         // empty storage returns null
    if last {
        emitter.instruction("sub r10, 1");                                      // choose last occupied index
    } else {
        emitter.instruction("xor r10, r10");                                    // choose first occupied index
    }
    emitter.instruction("lea r11, [r9 + 24]");                                  // point at first storage element
    emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");                  // load selected Mixed cell
    emitter.instruction("call __rt_incref");                                    // retain Mixed cell for caller while storage keeps owner
    emitter.instruction("ret");                                                 // return retained Mixed cell
    emitter.label(null_label);
    emit_throw_exception_x86_64(
        emitter,
        "_spl_runtime_exception_class_id",
        "_spl_dll_peek_empty_msg",
        SPL_DLL_PEEK_EMPTY_MSG_LEN,
    );
}

/// Emits `__rt_spl_dll_set_iterator_mode` and `__rt_spl_dll_get_iterator_mode` on x86_64.
/// Set: receiver in rdi, mode bits in rsi; stores rsi at the iterator mode offset and returns void.
/// Get: receiver in rdi; returns iterator mode bits in rax.
fn emit_iterator_mode_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_set_iterator_mode");
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], rsi", SPL_DLL_ITER_MODE_OFFSET)); // store iterator mode bits on receiver
    emitter.instruction("ret");                                                 // return void
    emitter.label_global("__rt_spl_dll_get_iterator_mode");
    emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {}]", SPL_DLL_ITER_MODE_OFFSET)); // return iterator mode bits
    emitter.instruction("ret");                                                 // return integer mode
}

/// Emits `__rt_spl_dll_serialize_array` on x86_64: receiver in rdi. Copies internal storage
/// items into a new array with retained Mixed cells, boxes iterator flags and both arrays,
/// and returns a boxed Mixed array (tag 4) containing [boxed flags, boxed items array,
/// boxed empty properties array] for high-level serialization.
fn emit_serialize_array_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_serialize_array");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for serialization
    emitter.instruction("mov rbp, rsp");                                        // establish serialization frame
    emitter.instruction("sub rsp, 80");                                         // reserve receiver, arrays, boxed values, and cursor spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal Mixed storage
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // save internal storage pointer
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load list length
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save list length
    emitter.instruction("mov rdi, r10");                                        // items array capacity equals list length
    emitter.instruction("mov rsi, 8");                                          // items array stores boxed Mixed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate serialized dllist array
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load items array packed kind word
    emitter.instruction("or r9, 0x700");                                        // stamp items array as boxed Mixed slots
    emitter.instruction("mov QWORD PTR [rax - 8], r9");                         // persist items Mixed value_type tag
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload list length
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish exact items array length
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save items array pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize item copy cursor
    emitter.label("__rt_spl_dll_serialize_items_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload item copy cursor
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // have all list items been copied?
    emitter.instruction("jge __rt_spl_dll_serialize_items_done_x86");           // stop once every item is serialized
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload internal storage pointer
    emitter.instruction("mov rax, QWORD PTR [r9 + 24 + r10 * 8]");              // load source Mixed cell
    emitter.instruction("call __rt_incref");                                    // retain source Mixed cell for serialized items
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload item copy cursor after retain
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload items array pointer
    emitter.instruction("mov QWORD PTR [r9 + 24 + r10 * 8], rax");              // store retained Mixed cell into serialized items
    emitter.instruction("add r10, 1");                                          // advance item copy cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save updated item copy cursor
    emitter.instruction("jmp __rt_spl_dll_serialize_items_loop_x86");           // continue copying list items
    emitter.label("__rt_spl_dll_serialize_items_done_x86");
    emitter.instruction("mov rax, 4");                                          // runtime tag 4 = indexed array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass serialized items array as mixed payload
    emitter.instruction("xor rsi, rsi");                                        // array mixed payload uses one word
    emitter.instruction("call __rt_mixed_from_value");                          // box serialized items array
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save boxed dllist value
    emitter.instruction("xor edi, edi");                                        // empty properties array capacity
    emitter.instruction("mov rsi, 8");                                          // properties array stores boxed Mixed slots
    emitter.instruction("call __rt_array_new");                                 // allocate empty serialized properties array
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load properties array packed kind word
    emitter.instruction("or r9, 0x700");                                        // stamp properties as boxed Mixed slots
    emitter.instruction("mov QWORD PTR [rax - 8], r9");                         // persist properties Mixed value_type tag
    emitter.instruction("mov rdi, rax");                                        // pass properties array pointer as mixed payload
    emitter.instruction("mov rax, 4");                                          // runtime tag 4 = indexed array
    emitter.instruction("xor rsi, rsi");                                        // array mixed payload uses one word
    emitter.instruction("call __rt_mixed_from_value");                          // box empty properties array
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save boxed properties value
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver for iterator flags
    emitter.instruction(&format!("mov rdi, QWORD PTR [r9 + {}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode flags
    emitter.instruction("xor esi, esi");                                        // integer mixed payload uses one word
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box iterator flags
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save boxed flags value
    emitter.instruction("mov rdi, 3");                                          // serialized state has flags, dllist, and properties
    emitter.instruction("mov rsi, 8");                                          // outer serialized array stores boxed Mixed slots
    emitter.instruction("call __rt_array_new");                                 // allocate outer serialized state array
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load outer array packed kind word
    emitter.instruction("or r9, 0x700");                                        // stamp outer array as boxed Mixed slots
    emitter.instruction("mov QWORD PTR [rax - 8], r9");                         // persist outer Mixed value_type tag
    emitter.instruction("mov QWORD PTR [rax], 3");                              // publish exact serialized state length
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // reload boxed flags value
    emitter.instruction("mov QWORD PTR [rax + 24], r10");                       // state[0] = flags
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload boxed dllist value
    emitter.instruction("mov QWORD PTR [rax + 32], r10");                       // state[1] = dllist
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload boxed properties value
    emitter.instruction("mov QWORD PTR [rax + 40], r10");                       // state[2] = properties
    emitter.instruction("add rsp, 80");                                         // release serialization frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return serialized state array
}

/// Emits `__rt_spl_dll_serialize` on x86_64: receiver in rdi, pointer to serialized output
/// buffer in rsi, length in rdx. Writes PHP legacy serialized form "i:<mode>;<item>..." using
/// the global concat buffer. Returns (pointer, length) of the serialized string. Unboxes
/// each item to detect int/string/bool/null tags and encodes accordingly. Appends to the
/// global `_concat_buf` and updates `_concat_off`.
fn emit_serialize_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_serialize");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for legacy serialization
    emitter.instruction("mov rbp, rsp");                                        // establish legacy serialization frame
    emitter.instruction("sub rsp, 96");                                         // reserve receiver, cursor, payload, and loop spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver across item unboxing
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load current concat-buffer offset
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize concat-buffer base
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // save concat-buffer base for final offset update
    emitter.instruction("lea r12, [r11 + r10]");                                // compute start pointer for serialized string
    emitter.instruction("mov QWORD PTR [rbp - 32], r12");                       // save serialized string start pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], r12");                       // initialize output cursor
    emitter.instruction("mov BYTE PTR [r12], 105");                             // write ASCII 'i' for legacy flags integer
    emitter.instruction("add r12, 1");                                          // advance output cursor after flags tag
    emitter.instruction("mov BYTE PTR [r12], 58");                              // write ASCII ':' after flags tag
    emitter.instruction("add r12, 1");                                          // advance output cursor after flags separator
    emitter.instruction("mov QWORD PTR [rbp - 40], r12");                       // save cursor before decimal writer
    emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode flags
    emitter.instruction("mov rdi, r12");                                        // pass cursor to decimal writer
    emitter.instruction("call __rt_spl_dll_write_dec_x86");                     // append decimal flags text
    emitter.instruction("mov BYTE PTR [rdi], 59");                              // write ASCII ';' after flags value
    emitter.instruction("add rdi, 1");                                          // advance cursor after flags terminator
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save cursor after flags field
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver for storage access
    emitter.instruction(&format!("mov r9, QWORD PTR [r9 + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal Mixed storage
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // save storage pointer
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // read list length
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save list length
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize serialized item cursor
    emitter.label("__rt_spl_dll_serialize_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload item cursor
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // have all list items been serialized?
    emitter.instruction("jge __rt_spl_dll_serialize_done_x86");                 // finish after the last item
    emitter.instruction("mov r12, QWORD PTR [rbp - 40]");                       // reload output cursor
    emitter.instruction("mov BYTE PTR [r12], 58");                              // write ASCII ':' item separator
    emitter.instruction("add r12, 1");                                          // advance cursor after item separator
    emitter.instruction("mov QWORD PTR [rbp - 40], r12");                       // save cursor before unboxing
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload internal storage pointer
    emitter.instruction("mov rax, QWORD PTR [r9 + 24 + r10 * 8]");              // load source Mixed cell pointer
    emitter.instruction("call __rt_mixed_unbox");                               // inspect concrete runtime tag and payload
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save unboxed tag
    emitter.instruction("mov QWORD PTR [rbp - 72], rdi");                       // save unboxed low payload
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // save unboxed high payload
    emitter.instruction(&format!("cmp rax, {}", INT_TAG));                      // is this item an integer?
    emitter.instruction("je __rt_spl_dll_serialize_int_x86");                   // encode integer values as i:<n>;
    emitter.instruction(&format!("cmp rax, {}", STR_TAG));                      // is this item a string?
    emitter.instruction("je __rt_spl_dll_serialize_string_x86");                // encode string values as s:<len>:\"...\";
    emitter.instruction(&format!("cmp rax, {}", BOOL_TAG));                     // is this item a boolean?
    emitter.instruction("je __rt_spl_dll_serialize_bool_x86");                  // encode boolean values as b:0; or b:1;
    emitter.instruction("jmp __rt_spl_dll_serialize_null_x86");                 // unsupported and null-like values serialize as N;
    emitter.label("__rt_spl_dll_serialize_int_x86");
    emitter.instruction("mov r12, QWORD PTR [rbp - 40]");                       // reload output cursor
    emitter.instruction("mov BYTE PTR [r12], 105");                             // write ASCII 'i' integer tag
    emitter.instruction("add r12, 1");                                          // advance cursor after integer tag
    emitter.instruction("mov BYTE PTR [r12], 58");                              // write ASCII ':' integer separator
    emitter.instruction("add r12, 1");                                          // advance cursor after integer separator
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // pass integer payload to decimal writer
    emitter.instruction("mov rdi, r12");                                        // pass output cursor to decimal writer
    emitter.instruction("call __rt_spl_dll_write_dec_x86");                     // append signed decimal integer text
    emitter.instruction("mov BYTE PTR [rdi], 59");                              // write ASCII ';' integer terminator
    emitter.instruction("add rdi, 1");                                          // advance cursor after integer terminator
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save updated output cursor
    emitter.instruction("jmp __rt_spl_dll_serialize_next_item_x86");            // continue with the next list item
    emitter.label("__rt_spl_dll_serialize_string_x86");
    emitter.instruction("mov r12, QWORD PTR [rbp - 40]");                       // reload output cursor
    emitter.instruction("mov BYTE PTR [r12], 115");                             // write ASCII 's' string tag
    emitter.instruction("add r12, 1");                                          // advance cursor after string tag
    emitter.instruction("mov BYTE PTR [r12], 58");                              // write ASCII ':' string length separator
    emitter.instruction("add r12, 1");                                          // advance cursor after string separator
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // pass string length to decimal writer
    emitter.instruction("mov rdi, r12");                                        // pass output cursor to decimal writer
    emitter.instruction("call __rt_spl_dll_write_dec_x86");                     // append string byte length
    emitter.instruction("mov BYTE PTR [rdi], 58");                              // write ASCII ':' before quoted bytes
    emitter.instruction("add rdi, 1");                                          // advance cursor after length separator
    emitter.instruction("mov BYTE PTR [rdi], 34");                              // write opening quote
    emitter.instruction("add rdi, 1");                                          // advance cursor after opening quote
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // load source string pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 80]");                       // load remaining source string length
    emitter.label("__rt_spl_dll_serialize_string_copy_x86");
    emitter.instruction("test r11, r11");                                       // are there string bytes left to copy?
    emitter.instruction("jz __rt_spl_dll_serialize_string_done_x86");           // finish copying once length reaches zero
    emitter.instruction("mov al, BYTE PTR [r10]");                              // read next source string byte
    emitter.instruction("mov BYTE PTR [rdi], al");                              // append raw string byte
    emitter.instruction("add r10, 1");                                          // advance source string cursor
    emitter.instruction("add rdi, 1");                                          // advance output cursor
    emitter.instruction("sub r11, 1");                                          // decrement remaining byte count
    emitter.instruction("jmp __rt_spl_dll_serialize_string_copy_x86");          // continue copying raw bytes
    emitter.label("__rt_spl_dll_serialize_string_done_x86");
    emitter.instruction("mov BYTE PTR [rdi], 34");                              // write closing quote
    emitter.instruction("add rdi, 1");                                          // advance cursor after closing quote
    emitter.instruction("mov BYTE PTR [rdi], 59");                              // write string terminator
    emitter.instruction("add rdi, 1");                                          // advance cursor after string terminator
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save updated output cursor
    emitter.instruction("jmp __rt_spl_dll_serialize_next_item_x86");            // continue with the next list item
    emitter.label("__rt_spl_dll_serialize_bool_x86");
    emitter.instruction("mov r12, QWORD PTR [rbp - 40]");                       // reload output cursor
    emitter.instruction("mov BYTE PTR [r12], 98");                              // write ASCII 'b' boolean tag
    emitter.instruction("add r12, 1");                                          // advance cursor after boolean tag
    emitter.instruction("mov BYTE PTR [r12], 58");                              // write ASCII ':' boolean separator
    emitter.instruction("add r12, 1");                                          // advance cursor after boolean separator
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // load boolean payload
    emitter.instruction("cmp r10, 0");                                          // choose ASCII 0 or 1 from payload
    emitter.instruction("mov r11b, 48");                                        // default boolean byte is ASCII '0'
    emitter.instruction("sete al");                                             // record whether payload is zero
    emitter.instruction("cmp r10, 0");                                          // restore flags for truthy selection
    emitter.instruction("setne r11b");                                          // produce 1 when payload is truthy
    emitter.instruction("add r11b, 48");                                        // convert boolean bit to ASCII digit
    emitter.instruction("mov BYTE PTR [r12], r11b");                            // write boolean digit
    emitter.instruction("add r12, 1");                                          // advance cursor after boolean digit
    emitter.instruction("mov BYTE PTR [r12], 59");                              // write boolean terminator
    emitter.instruction("add r12, 1");                                          // advance cursor after boolean terminator
    emitter.instruction("mov QWORD PTR [rbp - 40], r12");                       // save updated output cursor
    emitter.instruction("jmp __rt_spl_dll_serialize_next_item_x86");            // continue with the next list item
    emitter.label("__rt_spl_dll_serialize_null_x86");
    emitter.instruction("mov r12, QWORD PTR [rbp - 40]");                       // reload output cursor
    emitter.instruction("mov BYTE PTR [r12], 78");                              // write ASCII 'N' null tag
    emitter.instruction("add r12, 1");                                          // advance cursor after null tag
    emitter.instruction("mov BYTE PTR [r12], 59");                              // write null terminator
    emitter.instruction("add r12, 1");                                          // advance cursor after null terminator
    emitter.instruction("mov QWORD PTR [rbp - 40], r12");                       // save updated output cursor
    emitter.label("__rt_spl_dll_serialize_next_item_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload item cursor
    emitter.instruction("add r10, 1");                                          // advance to the next physical storage slot
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save updated item cursor
    emitter.instruction("jmp __rt_spl_dll_serialize_loop_x86");                 // serialize the next item
    emitter.label("__rt_spl_dll_serialize_done_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // reload concat-buffer base
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload final output cursor
    emitter.instruction("mov r12, r11");                                        // copy final cursor for global offset calculation
    emitter.instruction("sub r12, r10");                                        // compute new concat-buffer offset
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r12");              // publish updated concat-buffer offset
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return serialized string pointer
    emitter.instruction("mov rdx, r11");                                        // copy final cursor for length calculation
    emitter.instruction("sub rdx, rax");                                        // return serialized string length
    emitter.instruction("add rsp, 96");                                         // release legacy serialization spills
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return serialized pointer/length pair
    emit_write_dec_x86_64(emitter);
}

/// Emits `__rt_spl_dll_write_dec_x86` on x86_64: value in rax (sign-extended), output cursor
/// in rdi. Writes optional '-' for negative values, then digits in base-10, using the stack
/// to reverse digit order. Returns the advanced output cursor in rdi.
fn emit_write_dec_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_spl_dll_write_dec_x86");
    emitter.instruction("sub rsp, 32");                                         // reserve temporary reversed digit storage
    emitter.instruction("mov r8, rdi");                                         // keep output cursor in r8
    emitter.instruction("test rax, rax");                                       // check whether the value is negative
    emitter.instruction("jns __rt_spl_dll_write_dec_abs_x86");                  // non-negative values can be encoded directly
    emitter.instruction("mov BYTE PTR [r8], 45");                               // write ASCII '-' for negative values
    emitter.instruction("add r8, 1");                                           // advance cursor after minus sign
    emitter.instruction("neg rax");                                             // convert value to positive magnitude
    emitter.label("__rt_spl_dll_write_dec_abs_x86");
    emitter.instruction("test rax, rax");                                       // is the value zero?
    emitter.instruction("jne __rt_spl_dll_write_dec_loop_init_x86");            // nonzero values use repeated division
    emitter.instruction("mov BYTE PTR [r8], 48");                               // write ASCII '0' for the zero special case
    emitter.instruction("add r8, 1");                                           // advance cursor after zero digit
    emitter.instruction("jmp __rt_spl_dll_write_dec_done_x86");                 // finish without reversed storage
    emitter.label("__rt_spl_dll_write_dec_loop_init_x86");
    emitter.instruction("xor r9, r9");                                          // digit count starts at zero
    emitter.label("__rt_spl_dll_write_dec_loop_x86");
    emitter.instruction("mov r10, 10");                                         // divisor for base-10 formatting
    emitter.instruction("xor rdx, rdx");                                        // clear high dividend before unsigned division
    emitter.instruction("div r10");                                             // divide value by 10
    emitter.instruction("add dl, 48");                                          // convert remainder to ASCII digit
    emitter.instruction("mov BYTE PTR [rsp + r9], dl");                         // store digit in reverse order
    emitter.instruction("add r9, 1");                                           // count stored digit
    emitter.instruction("test rax, rax");                                       // does quotient still have digits?
    emitter.instruction("jne __rt_spl_dll_write_dec_loop_x86");                 // continue extracting digits
    emitter.label("__rt_spl_dll_write_dec_copy_x86");
    emitter.instruction("sub r9, 1");                                           // move to previous reversed digit
    emitter.instruction("js __rt_spl_dll_write_dec_done_x86");                  // finish once all digits were copied
    emitter.instruction("mov al, BYTE PTR [rsp + r9]");                         // load next forward-order digit
    emitter.instruction("mov BYTE PTR [r8], al");                               // append digit to output
    emitter.instruction("add r8, 1");                                           // advance output cursor
    emitter.instruction("jmp __rt_spl_dll_write_dec_copy_x86");                 // continue copying digits
    emitter.label("__rt_spl_dll_write_dec_done_x86");
    emitter.instruction("mov rdi, r8");                                         // return advanced output cursor
    emitter.instruction("add rsp, 32");                                         // release temporary reversed digit storage
    emitter.instruction("ret");                                                 // return to serializer
}

/// Emits `__rt_spl_dll_unserialize` on x86_64: receiver in rdi, input pointer in rsi,
/// input length in rdx. Clears existing storage by releasing all owned Mixed cells,
/// parses the legacy serialized format "i:<mode>;<item>..." using `__rt_spl_dll_parse_dec_x86`,
/// and appends each parsed value via `__rt_spl_dll_push`. Throws on malformed input.
fn emit_unserialize_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_unserialize");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for legacy parser
    emitter.instruction("mov rbp, rsp");                                        // establish legacy unserialization frame
    emitter.instruction("sub rsp, 112");                                        // reserve parser, cursor, and clear-loop state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver
    emitter.instruction("lea r9, [rsi + rdx]");                                 // compute input end pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // save input end pointer
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load current internal storage
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // save current storage pointer for clearing
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // read current list length
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save current list length
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize clear-loop cursor
    emitter.label("__rt_spl_dll_unserialize_clear_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload clear-loop cursor
    emitter.instruction("cmp r10, QWORD PTR [rbp - 40]");                       // has every old cell been released?
    emitter.instruction("jge __rt_spl_dll_unserialize_clear_done_x86");         // finish clearing old storage
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload storage pointer
    emitter.instruction("mov rax, QWORD PTR [r9 + 24 + r10 * 8]");              // load old Mixed cell pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // save clear-loop cursor across decref
    emitter.instruction("test rax, rax");                                       // skip empty slots defensively
    emitter.instruction("jz __rt_spl_dll_unserialize_clear_next_x86");          // avoid decref on null slot
    emitter.instruction("call __rt_decref_mixed");                              // release old storage-owned Mixed cell
    emitter.label("__rt_spl_dll_unserialize_clear_next_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload storage pointer after decref
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload clear-loop cursor
    emitter.instruction("mov QWORD PTR [r9 + 24 + r10 * 8], 0");                // clear stale slot
    emitter.instruction("add r10, 1");                                          // advance clear-loop cursor
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save updated clear-loop cursor
    emitter.instruction("jmp __rt_spl_dll_unserialize_clear_loop_x86");         // continue clearing old cells
    emitter.label("__rt_spl_dll_unserialize_clear_done_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload storage pointer after clearing
    emitter.instruction("mov QWORD PTR [r9], 0");                               // reset internal storage length to zero
    emitter.instruction("lea rdi, [rsi + 2]");                                  // skip leading legacy 'i:' flags prefix
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass input end pointer to decimal parser
    emitter.instruction("call __rt_spl_dll_parse_dec_x86");                     // parse iterator mode flags
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver for mode update
    emitter.instruction(&format!("mov QWORD PTR [r9 + {}], rax", SPL_DLL_ITER_MODE_OFFSET)); // restore serialized iterator mode
    emitter.instruction("mov r10, rdi");                                        // move parser cursor to scratch
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // can parser inspect the flags terminator?
    emitter.instruction("jae __rt_spl_dll_unserialize_store_cursor_x86");       // avoid reading past input
    emitter.instruction("cmp BYTE PTR [r10], 59");                              // ASCII ';' closes the flags integer
    emitter.instruction("jne __rt_spl_dll_unserialize_store_cursor_x86");       // leave cursor unchanged when terminator is absent
    emitter.instruction("add r10, 1");                                          // skip flags terminator
    emitter.label("__rt_spl_dll_unserialize_store_cursor_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save item parser cursor
    emitter.label("__rt_spl_dll_unserialize_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload parser cursor
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // did parser reach the input end?
    emitter.instruction("jae __rt_spl_dll_unserialize_done_x86");               // finish once cursor reaches input end
    emitter.instruction("cmp BYTE PTR [r10], 58");                              // items are prefixed by ASCII ':'
    emitter.instruction("jne __rt_spl_dll_unserialize_done_x86");               // stop on malformed trailing data
    emitter.instruction("add r10, 1");                                          // skip item separator
    emitter.instruction("movzx r11, BYTE PTR [r10]");                           // read item type tag
    emitter.instruction("cmp r11, 105");                                        // ASCII 'i' marks integer payloads
    emitter.instruction("je __rt_spl_dll_unserialize_int_x86");                 // parse integer payload
    emitter.instruction("cmp r11, 115");                                        // ASCII 's' marks string payloads
    emitter.instruction("je __rt_spl_dll_unserialize_string_x86");              // parse string payload
    emitter.instruction("cmp r11, 98");                                         // ASCII 'b' marks boolean payloads
    emitter.instruction("je __rt_spl_dll_unserialize_bool_x86");                // parse boolean payload
    emitter.instruction("cmp r11, 78");                                         // ASCII 'N' marks null payloads
    emitter.instruction("je __rt_spl_dll_unserialize_null_x86");                // parse null payload
    emitter.instruction("jmp __rt_spl_dll_unserialize_done_x86");               // stop on unsupported malformed payloads
    emitter.label("__rt_spl_dll_unserialize_int_x86");
    emitter.instruction("add r10, 2");                                          // skip 'i:' before integer digits
    emitter.instruction("mov rdi, r10");                                        // pass integer digit cursor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass input end pointer
    emitter.instruction("call __rt_spl_dll_parse_dec_x86");                     // parse signed integer value
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save parsed integer payload
    emitter.instruction("mov r10, rdi");                                        // move parser cursor to scratch
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // can parser inspect integer terminator?
    emitter.instruction("jae __rt_spl_dll_unserialize_int_box_x86");            // avoid reading past input
    emitter.instruction("cmp BYTE PTR [r10], 59");                              // ASCII ';' closes integer payloads
    emitter.instruction("jne __rt_spl_dll_unserialize_int_box_x86");            // keep cursor when terminator is absent
    emitter.instruction("add r10, 1");                                          // skip integer terminator
    emitter.label("__rt_spl_dll_unserialize_int_box_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save parser cursor after integer payload
    emitter.instruction(&format!("mov rax, {}", INT_TAG));                      // runtime tag 0 = integer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // pass parsed integer payload
    emitter.instruction("xor rsi, rsi");                                        // integer payload has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // box integer for list storage
    emit_unserialize_append_x86_64(emitter);
    emitter.instruction("jmp __rt_spl_dll_unserialize_loop_x86");               // continue parsing following items
    emitter.label("__rt_spl_dll_unserialize_string_x86");
    emitter.instruction("add r10, 2");                                          // skip 's:' before byte length
    emitter.instruction("mov rdi, r10");                                        // pass string length cursor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass input end pointer
    emitter.instruction("call __rt_spl_dll_parse_dec_x86");                     // parse string byte length
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save parsed string length
    emitter.instruction("mov r10, rdi");                                        // move parser cursor to scratch
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // can parser skip length separator?
    emitter.instruction("jae __rt_spl_dll_unserialize_string_box_x86");         // malformed short input becomes an empty tail
    emitter.instruction("cmp BYTE PTR [r10], 58");                              // ASCII ':' follows string byte length
    emitter.instruction("jne __rt_spl_dll_unserialize_string_quote_x86");       // continue even when separator is absent
    emitter.instruction("add r10, 1");                                          // skip length separator
    emitter.label("__rt_spl_dll_unserialize_string_quote_x86");
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // can parser inspect opening quote?
    emitter.instruction("jae __rt_spl_dll_unserialize_string_box_x86");         // avoid reading past input
    emitter.instruction("cmp BYTE PTR [r10], 34");                              // ASCII '\"' opens raw string bytes
    emitter.instruction("jne __rt_spl_dll_unserialize_string_payload_x86");     // continue when quote is absent
    emitter.instruction("add r10, 1");                                          // skip opening quote
    emitter.label("__rt_spl_dll_unserialize_string_payload_x86");
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // save raw string payload pointer
    emitter.instruction("add r10, QWORD PTR [rbp - 72]");                       // skip raw string payload bytes
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // can parser skip closing quote?
    emitter.instruction("jae __rt_spl_dll_unserialize_string_save_cursor_x86"); // avoid reading past input
    emitter.instruction("cmp BYTE PTR [r10], 34");                              // ASCII '\"' closes raw string bytes
    emitter.instruction("jne __rt_spl_dll_unserialize_string_term_x86");        // continue when quote is absent
    emitter.instruction("add r10, 1");                                          // skip closing quote
    emitter.label("__rt_spl_dll_unserialize_string_term_x86");
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // can parser inspect string terminator?
    emitter.instruction("jae __rt_spl_dll_unserialize_string_save_cursor_x86"); // avoid reading past input
    emitter.instruction("cmp BYTE PTR [r10], 59");                              // ASCII ';' closes string payloads
    emitter.instruction("jne __rt_spl_dll_unserialize_string_save_cursor_x86"); // keep cursor when terminator is absent
    emitter.instruction("add r10, 1");                                          // skip string terminator
    emitter.label("__rt_spl_dll_unserialize_string_save_cursor_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save parser cursor after string payload
    emitter.label("__rt_spl_dll_unserialize_string_box_x86");
    emitter.instruction(&format!("mov rax, {}", STR_TAG));                      // runtime tag 1 = string
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // pass raw string payload pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // pass raw string payload length
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box string for list storage
    emit_unserialize_append_x86_64(emitter);
    emitter.instruction("jmp __rt_spl_dll_unserialize_loop_x86");               // continue parsing following items
    emitter.label("__rt_spl_dll_unserialize_bool_x86");
    emitter.instruction("movzx r11, BYTE PTR [r10 + 2]");                       // read boolean digit after 'b:'
    emitter.instruction("cmp r11, 49");                                         // ASCII '1' means true
    emitter.instruction("sete dil");                                            // produce 1 for true, 0 for false
    emitter.instruction("movzx rdi, dil");                                      // widen boolean payload
    emitter.instruction("add r10, 4");                                          // skip fixed b:<digit>; payload
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save parser cursor after boolean payload
    emitter.instruction(&format!("mov rax, {}", BOOL_TAG));                     // runtime tag 3 = bool
    emitter.instruction("xor rsi, rsi");                                        // boolean payload has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // box boolean for list storage
    emit_unserialize_append_x86_64(emitter);
    emitter.instruction("jmp __rt_spl_dll_unserialize_loop_x86");               // continue parsing following items
    emitter.label("__rt_spl_dll_unserialize_null_x86");
    emitter.instruction("add r10, 2");                                          // skip fixed N; payload
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save parser cursor after null payload
    emitter.instruction(&format!("mov rax, {}", NULL_TAG));                     // runtime tag 8 = null
    emitter.instruction("xor rdi, rdi");                                        // null payload low word is empty
    emitter.instruction("xor rsi, rsi");                                        // null payload high word is empty
    emitter.instruction("call __rt_mixed_from_value");                          // box null for list storage
    emit_unserialize_append_x86_64(emitter);
    emitter.instruction("jmp __rt_spl_dll_unserialize_loop_x86");               // continue parsing following items
    emitter.label("__rt_spl_dll_unserialize_done_x86");
    emitter.instruction("add rsp, 112");                                        // release parser state
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return void
    emit_parse_dec_x86_64(emitter);
}

/// Emits the append helper on x86_64 for unserialization: moves the boxed parsed value
/// from rax into rsi for the push call, reloads the receiver from the stack at rbp-8,
/// and calls `__rt_spl_dll_push` to append the value to the list.
fn emit_unserialize_append_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov rsi, rax");                                        // move boxed parsed value into push value argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload receiver for shared list append
    emitter.instruction("call __rt_spl_dll_push");                              // append parsed value to internal storage
}

/// Emits `__rt_spl_dll_parse_dec_x86` on x86_64: cursor in rdi, input end in rsi. Parses an
/// optional leading '-' for negative values, then reads consecutive ASCII decimal digits.
/// Returns the parsed integer in rax and the advanced cursor in rdi. Empty input returns zero.
fn emit_parse_dec_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_spl_dll_parse_dec_x86");
    emitter.instruction("mov r8, rdi");                                         // keep parser cursor in r8
    emitter.instruction("xor r9, r9");                                          // initialize decimal accumulator
    emitter.instruction("xor r10, r10");                                        // initialize negative flag
    emitter.instruction("cmp r8, rsi");                                         // is cursor already at input end?
    emitter.instruction("jae __rt_spl_dll_parse_dec_finish_x86");               // empty digit runs parse as zero
    emitter.instruction("cmp BYTE PTR [r8], 45");                               // ASCII '-' starts negative values
    emitter.instruction("jne __rt_spl_dll_parse_dec_loop_x86");                 // unsigned values start directly at digit parsing
    emitter.instruction("mov r10, 1");                                          // remember that parsed number is negative
    emitter.instruction("add r8, 1");                                           // skip minus sign
    emitter.label("__rt_spl_dll_parse_dec_loop_x86");
    emitter.instruction("cmp r8, rsi");                                         // stop if cursor reached input end
    emitter.instruction("jae __rt_spl_dll_parse_dec_sign_x86");                 // finish and apply sign
    emitter.instruction("movzx r11, BYTE PTR [r8]");                            // load next potential digit
    emitter.instruction("sub r11, 48");                                         // convert ASCII byte to digit candidate
    emitter.instruction("cmp r11, 9");                                          // check whether digit candidate is in range
    emitter.instruction("ja __rt_spl_dll_parse_dec_sign_x86");                  // stop at first non-digit byte
    emitter.instruction("imul r9, r9, 10");                                     // shift accumulator by one decimal place
    emitter.instruction("add r9, r11");                                         // append parsed digit
    emitter.instruction("add r8, 1");                                           // advance parser cursor
    emitter.instruction("jmp __rt_spl_dll_parse_dec_loop_x86");                 // continue parsing digits
    emitter.label("__rt_spl_dll_parse_dec_sign_x86");
    emitter.instruction("test r10, r10");                                       // was a negative sign parsed?
    emitter.instruction("jz __rt_spl_dll_parse_dec_finish_x86");                // skip negation for positive values
    emitter.instruction("neg r9");                                              // apply negative sign
    emitter.label("__rt_spl_dll_parse_dec_finish_x86");
    emitter.instruction("mov rax, r9");                                         // return parsed integer value
    emitter.instruction("mov rdi, r8");                                         // return cursor positioned after digits
    emitter.instruction("ret");                                                 // return to parser
}

/// Emits `__rt_spl_dll_rewind` on x86_64: receiver in rdi. Resets the iterator index based on
/// the iterator mode. FIFO mode sets index to 0. LIFO mode sets index to length-1 (the last
/// element). Empty storage resets index to zero. Returns void.
fn emit_rewind_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_rewind");
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // read storage length
    emitter.instruction("test r10, r10");                                       // is storage empty?
    emitter.instruction("jz __rt_spl_dll_rewind_empty");                        // empty storage rewinds to zero
    emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode bits
    emitter.instruction(&format!("test r11, {}", ITER_MODE_LIFO));              // is LIFO traversal requested?
    emitter.instruction("jz __rt_spl_dll_rewind_fifo");                         // FIFO traversal starts at zero
    emitter.instruction("sub r10, 1");                                          // LIFO traversal starts at last element
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r10", SPL_DLL_ITER_INDEX_OFFSET)); // store starting LIFO index
    emitter.instruction("ret");                                                 // return void
    emitter.label("__rt_spl_dll_rewind_fifo");
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], 0", SPL_DLL_ITER_INDEX_OFFSET)); // store starting FIFO index
    emitter.instruction("ret");                                                 // return void
    emitter.label("__rt_spl_dll_rewind_empty");
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], 0", SPL_DLL_ITER_INDEX_OFFSET)); // reset empty iterator to zero
    emitter.instruction("ret");                                                 // return void
}

/// Emits `__rt_spl_dll_next` and `__rt_spl_dll_prev` on x86_64. Both dispatch to
/// `emit_iterator_step_x86_64` with forward=true for next and forward=false for prev.
/// Receivers and index/mode registers are loaded in `emit_iterator_step_x86_64`.
fn emit_next_prev_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_next");
    emit_iterator_step_x86_64(emitter, true);
    emitter.label_global("__rt_spl_dll_prev");
    emit_iterator_step_x86_64(emitter, false);
}

/// Emits the shared iterator step logic on x86_64. Forward=true is `next()`, false is `prev()`.
/// Loads iterator index and mode from the receiver. Handles FIFO vs LIFO direction, delete-mode
/// foreach (ITER_MODE_DELETE), and boundary exhaustion. For delete-mode, calls `shift()` or
/// `pop()` and releases the removed cell. Persists the updated index and returns void.
fn emit_iterator_step_x86_64(emitter: &mut Emitter, forward: bool) {
    let fifo_label = if forward {
        "__rt_spl_dll_next_fifo"
    } else {
        "__rt_spl_dll_prev_fifo"
    };
    let delete_label = if forward {
        "__rt_spl_dll_next_delete"
    } else {
        ""
    };
    let done_label = if forward {
        "__rt_spl_dll_next_done"
    } else {
        "__rt_spl_dll_prev_done"
    };
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_ITER_INDEX_OFFSET)); // load current iterator index
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode bits
    if forward {
        emitter.instruction(&format!("test r10, {}", ITER_MODE_DELETE));        // does next() need to delete the current element?
        emitter.instruction(&format!("jnz {}", delete_label));                  // delete-mode foreach advances by removing the current slot
    }
    emitter.instruction(&format!("test r10, {}", ITER_MODE_LIFO));              // is traversal currently LIFO?
    emitter.instruction(&format!("jz {}", fifo_label));                         // FIFO and LIFO move in opposite directions
    if forward {
        emitter.instruction("test r9, r9");                                     // is LIFO traversal at numeric index zero?
        emitter.instruction("jz __rt_spl_dll_next_lifo_exhaust");               // moving forward from zero exhausts the iterator
        emitter.instruction("sub r9, 1");                                       // otherwise move to previous numeric index
        emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r9", SPL_DLL_ITER_INDEX_OFFSET)); // persist decremented LIFO iterator index
        emitter.instruction("ret");                                             // return void
        emitter.label("__rt_spl_dll_next_lifo_exhaust");
        emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load storage to compute exhausted sentinel
        emitter.instruction("mov r10, QWORD PTR [r10]");                        // storage length is the invalid sentinel
        emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r10", SPL_DLL_ITER_INDEX_OFFSET)); // store exhausted LIFO sentinel
        emitter.instruction("ret");                                             // return void
    } else {
        emitter.instruction("add r9, 1");                                       // moving prev in LIFO increases numeric index
        emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r9", SPL_DLL_ITER_INDEX_OFFSET)); // persist incremented LIFO iterator index
        emitter.instruction("ret");                                             // return void
    }
    emitter.label(fifo_label);
    if forward {
        emitter.instruction("add r9, 1");                                       // moving forward in FIFO increases index
    } else {
        emitter.instruction("test r9, r9");                                     // is FIFO traversal already at zero?
        emitter.instruction("jz __rt_spl_dll_prev_fifo_done");                  // moving before zero leaves iterator at zero
        emitter.instruction("sub r9, 1");                                       // otherwise move one FIFO slot backward
    }
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r9", SPL_DLL_ITER_INDEX_OFFSET)); // persist updated FIFO iterator index
    emitter.label(done_label);
    if !forward {
        emitter.label("__rt_spl_dll_prev_fifo_done");
    }
    emitter.instruction("ret");                                                 // return void
    if forward {
        emit_iterator_delete_step_x86_64(emitter, delete_label);
    }
}

/// Emits the delete-mode iterator step helper on x86_64, called from `emit_iterator_step_x86_64`
/// when ITER_MODE_DELETE is set. If LIFO mode: calls `__rt_spl_dll_pop`, releases the cell, and
/// sets index to the new tail. If FIFO mode: calls `__rt_spl_dll_shift`, releases the cell, and
/// resets index to zero. Empty storage after deletion rewinds to index zero.
fn emit_iterator_delete_step_x86_64(emitter: &mut Emitter, delete_label: &str) {
    emitter.label(delete_label);
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for delete-mode next()
    emitter.instruction("mov rbp, rsp");                                        // establish delete-mode iterator frame
    emitter.instruction("sub rsp, 16");                                         // reserve receiver spill slot
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve receiver across pop/shift and release
    emitter.instruction(&format!("test r10, {}", ITER_MODE_LIFO));              // choose which end delete-mode traversal removes from
    emitter.instruction("jnz __rt_spl_dll_next_delete_lifo");                   // LIFO delete removes the tail element
    emitter.instruction("call __rt_spl_dll_shift");                             // FIFO delete removes the head element and compacts storage
    emitter.instruction("call __rt_decref_mixed");                              // release the removed storage-owned Mixed cell
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver after deletion
    emitter.instruction(&format!("mov QWORD PTR [r9 + {}], 0", SPL_DLL_ITER_INDEX_OFFSET)); // FIFO delete keeps iteration at the new head
    emitter.instruction("jmp __rt_spl_dll_next_delete_done");                   // finish delete-mode next()
    emitter.label("__rt_spl_dll_next_delete_lifo");
    emitter.instruction("call __rt_spl_dll_pop");                               // LIFO delete removes the current tail element
    emitter.instruction("call __rt_decref_mixed");                              // release the removed storage-owned Mixed cell
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver after deletion
    emitter.instruction(&format!("mov r10, QWORD PTR [r9 + {}]", SPL_DLL_STORAGE_OFFSET)); // load storage to find the new tail
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // read storage length after deletion
    emitter.instruction("test r10, r10");                                       // did deletion empty the storage?
    emitter.instruction("jz __rt_spl_dll_next_delete_empty");                   // empty storage rewinds to index zero
    emitter.instruction("sub r10, 1");                                          // new LIFO current index is the new tail
    emitter.instruction(&format!("mov QWORD PTR [r9 + {}], r10", SPL_DLL_ITER_INDEX_OFFSET)); // persist new LIFO delete cursor
    emitter.instruction("jmp __rt_spl_dll_next_delete_done");                   // finish non-empty LIFO delete
    emitter.label("__rt_spl_dll_next_delete_empty");
    emitter.instruction(&format!("mov QWORD PTR [r9 + {}], 0", SPL_DLL_ITER_INDEX_OFFSET)); // reset exhausted delete-mode iterator to zero
    emitter.label("__rt_spl_dll_next_delete_done");
    emitter.instruction("add rsp, 16");                                         // release receiver spill slot
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return void
}

/// Emits `__rt_spl_dll_valid` on x86_64: receiver in rdi. Reads iterator index and storage
/// length, returns a widened boolean in rax (1 when index < length, 0 otherwise).
fn emit_valid_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_valid");
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // read storage length
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", SPL_DLL_ITER_INDEX_OFFSET)); // read current iterator index
    emitter.instruction("cmp r10, r9");                                         // valid when index is below length
    emitter.instruction("setb al");                                             // set boolean for unsigned index < length
    emitter.instruction("movzx rax, al");                                       // widen boolean result
    emitter.instruction("ret");                                                 // return boolean result
}

/// Emits `__rt_spl_dll_current` on x86_64: receiver in rdi. Loads the current Mixed cell
/// at the iterator index, retains it with `__rt_incref`, and returns it. Returns a boxed
/// null (via `emit_tail_boxed_null_x86_64`) when the index is out of range.
fn emit_current_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_current");
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // read storage length
    emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", SPL_DLL_ITER_INDEX_OFFSET)); // read iterator index
    emitter.instruction("cmp r11, r10");                                        // is iterator index inside storage?
    emitter.instruction("jae __rt_spl_dll_current_null");                       // invalid current returns null
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at first storage element
    emitter.instruction("mov rax, QWORD PTR [r12 + r11 * 8]");                  // load current Mixed cell
    emitter.instruction("call __rt_incref");                                    // retain current Mixed cell for caller
    emitter.instruction("ret");                                                 // return retained Mixed cell
    emitter.label("__rt_spl_dll_current_null");
    emit_tail_boxed_null_x86_64(emitter);
}

/// Emits `__rt_spl_dll_key` on x86_64: receiver in rdi. Reads the iterator index as the
/// integer key, boxes it as a tagged Mixed (INT_TAG), and returns it. Returns a boxed null
/// (via `emit_tail_boxed_null_x86_64`) when the index is out of range.
fn emit_key_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_key");
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // read storage length
    emitter.instruction(&format!("mov rdi, QWORD PTR [rdi + {}]", SPL_DLL_ITER_INDEX_OFFSET)); // read iterator index as integer key payload
    emitter.instruction("cmp rdi, r9");                                         // is iterator index valid?
    emitter.instruction("jae __rt_spl_dll_key_null");                           // invalid key returns null
    emitter.instruction(&format!("mov rax, {}", INT_TAG));                      // runtime tag 0 = int key
    emitter.instruction("xor rsi, rsi");                                        // integer keys do not use a high payload word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box and return the integer key
    emitter.label("__rt_spl_dll_key_null");
    emit_tail_boxed_null_x86_64(emitter);
}

/// Emits `__rt_spl_dll_offset_exists` on x86_64: receiver in rdi, boxed offset in rsi.
/// Unboxes the offset, validates it is a non-negative integer within storage bounds,
/// converts LIFO logical offset to physical slot, and returns boolean in rax.
/// Throws TypeError for non-integer offsets.
fn emit_offset_exists_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_offset_exists");
    emit_offset_index_prefix_x86_64(
        emitter,
        "__rt_spl_dll_offset_exists_type_throw",
        "__rt_spl_dll_offset_exists_false",
        "__rt_spl_dll_offset_exists_index_ready",
    );
    emitter.instruction("mov rax, 1");                                          // return true for any in-range offset
    emitter.instruction("add rsp, 48");                                         // release offset helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return boolean true
    emitter.label("__rt_spl_dll_offset_exists_false");
    emitter.instruction("xor rax, rax");                                        // return false for invalid offsets
    emitter.instruction("add rsp, 48");                                         // release offset helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return boolean false
    emitter.label("__rt_spl_dll_offset_exists_type_throw");
    emitter.instruction("add rsp, 48");                                         // release offset helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_dll_offset_exists_type_msg",
        SPL_DLL_OFFSET_EXISTS_TYPE_MSG_LEN,
    );
}

/// Emits `__rt_spl_dll_offset_get` on x86_64: receiver in rdi, boxed offset in rsi.
/// Unboxes the offset, validates it is a non-negative integer within storage bounds,
/// converts LIFO logical offset to physical slot, loads the selected Mixed cell,
/// retains it with `__rt_incref`, and returns it. Throws TypeError or OutOfRangeException
/// on invalid offset.
fn emit_offset_get_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_offset_get");
    emit_offset_index_prefix_x86_64(
        emitter,
        "__rt_spl_dll_offset_get_type_throw",
        "__rt_spl_dll_offset_get_range_throw",
        "__rt_spl_dll_offset_get_index_ready",
    );
    emitter.instruction("lea r11, [r9 + 24]");                                  // point at first storage element
    emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");                  // load selected Mixed cell
    emitter.instruction("call __rt_incref");                                    // retain selected Mixed cell for caller
    emitter.instruction("add rsp, 48");                                         // release offset helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return retained Mixed cell
    emitter.label("__rt_spl_dll_offset_get_type_throw");
    emitter.instruction("add rsp, 48");                                         // release offset helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_dll_offset_get_type_msg",
        SPL_DLL_OFFSET_GET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_dll_offset_get_range_throw");
    emitter.instruction("add rsp, 48");                                         // release offset helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_out_of_range_exception_class_id",
        "_spl_dll_offset_get_range_msg",
        SPL_DLL_OFFSET_GET_RANGE_MSG_LEN,
    );
}

/// Emits the shared offset validation preamble on x86_64: receiver in rdi, boxed offset in rsi.
/// Unboxes the offset, validates it is an integer and non-negative, loads storage and length,
/// and checks bounds. Converts LIFO logical offset to physical slot using ITER_MODE_LIFO.
/// Sets r9 = storage, r10 = physical index on success; jumps to type_label, range_label on
/// errors. Returns with the offset helper frame established but not yet cleaned up.
fn emit_offset_index_prefix_x86_64(
    emitter: &mut Emitter,
    type_label: &str,
    range_label: &str,
    ready_label: &str,
) {
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for offset helper
    emitter.instruction("mov rbp, rsp");                                        // establish offset helper frame
    emitter.instruction("sub rsp, 48");                                         // reserve receiver, offset, tag, and payload spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save boxed offset argument
    emitter.instruction("mov rax, rsi");                                        // pass boxed offset to mixed_unbox
    emitter.instruction("call __rt_mixed_unbox");                               // unbox offset into tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save unboxed offset tag
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save unboxed integer payload candidate
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload boxed offset argument
    emitter.instruction("call __rt_decref_mixed");                              // release owned boxed offset argument
    emitter.instruction("mov r12, QWORD PTR [rbp - 24]");                       // reload unboxed offset tag
    emitter.instruction(&format!("cmp r12, {}", INT_TAG));                      // offset must be an integer for list addressing
    emitter.instruction(&format!("jne {}", type_label));                        // reject non-integer offsets
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload integer offset payload
    emitter.instruction("cmp r10, 0");                                          // reject negative offsets
    emitter.instruction(&format!("jl {}", range_label));                        // negative offsets are invalid
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload receiver
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("mov r11, QWORD PTR [r9]");                             // read storage length
    emitter.instruction("cmp r10, r11");                                        // compare offset with length
    emitter.instruction(&format!("jae {}", range_label));                       // offsets past end are invalid
    emitter.instruction(&format!("mov r12, QWORD PTR [rdi + {}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode bits for logical indexing
    emitter.instruction(&format!("test r12, {}", ITER_MODE_LIFO));              // does the list expose offsets in LIFO order?
    emitter.instruction(&format!("jz {}", ready_label));                        // FIFO offsets already match physical storage
    emitter.instruction("mov r10, r11");                                        // start converting logical LIFO offset to physical offset
    emitter.instruction("sub r10, QWORD PTR [rbp - 32]");                       // compute one-based physical offset
    emitter.instruction("sub r10, 1");                                          // finish zero-based physical offset
    emitter.label(ready_label);
}

/// Emits `__rt_spl_dll_offset_set` on x86_64: receiver in rdi, boxed offset in rsi, owned
/// value in rdx. When offset is null, appends via `__rt_spl_dll_push`. Otherwise unboxes
/// offset, validates it is a non-negative integer within storage bounds, releases the old
/// Mixed cell at that slot, stores the replacement, and returns void. Converts LIFO
/// logical offset to physical slot. Throws TypeError or OutOfRangeException on invalid offset.
fn emit_offset_set_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_offset_set");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for offsetSet
    emitter.instruction("mov rbp, rsp");                                        // establish offsetSet frame
    emitter.instruction("sub rsp, 64");                                         // reserve receiver, offset, value, tag, payload, and storage spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save boxed offset argument
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save owned Mixed value argument
    emitter.instruction("mov rax, rsi");                                        // pass boxed offset to mixed_unbox
    emitter.instruction("call __rt_mixed_unbox");                               // unbox offset into tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save offset tag
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save integer offset payload candidate
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload boxed offset argument
    emitter.instruction("call __rt_decref_mixed");                              // release boxed offset argument
    emitter.instruction("mov r12, QWORD PTR [rbp - 32]");                       // reload offset tag
    emitter.instruction(&format!("cmp r12, {}", NULL_TAG));                     // null offset means append
    emitter.instruction("je __rt_spl_dll_offset_set_append");                   // append when offset is null
    emitter.instruction(&format!("cmp r12, {}", INT_TAG));                      // explicit offset must be integer
    emitter.instruction("jne __rt_spl_dll_offset_set_type_throw");              // reject non-integer offsets
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload integer offset
    emitter.instruction("cmp r10, 0");                                          // reject negative offsets
    emitter.instruction("jl __rt_spl_dll_offset_set_range_throw");              // negative offsets are out of range
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver
    emitter.instruction(&format!("mov r14, QWORD PTR [r9 + {}]", SPL_DLL_ITER_MODE_OFFSET)); // load iterator mode bits for logical index mapping
    emitter.instruction(&format!("mov r9, QWORD PTR [r9 + {}]", SPL_DLL_STORAGE_OFFSET)); // load internal storage
    emitter.instruction("mov r11, QWORD PTR [r9]");                             // read storage length
    emitter.instruction("cmp r10, r11");                                        // compare explicit offset with length
    emitter.instruction("jae __rt_spl_dll_offset_set_range_throw");             // explicit offsets at/past length are out of range
    emitter.instruction(&format!("test r14, {}", ITER_MODE_LIFO));              // does logical indexing run in LIFO order?
    emitter.instruction("jz __rt_spl_dll_offset_set_physical_index_ready");     // FIFO offsets already match physical storage
    emitter.instruction("mov r10, r11");                                        // start converting logical LIFO offset to physical offset
    emitter.instruction("sub r10, QWORD PTR [rbp - 40]");                       // compute one-based physical offset
    emitter.instruction("sub r10, 1");                                          // finish zero-based physical offset
    emitter.label("__rt_spl_dll_offset_set_physical_index_ready");
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at first storage element
    emitter.instruction("mov rax, QWORD PTR [r12 + r10 * 8]");                  // load old Mixed cell at this offset
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // preserve storage across old-value release
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // preserve offset across old-value release
    emitter.instruction("call __rt_decref_mixed");                              // release old Mixed cell before overwriting
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload storage after release
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload offset after release
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at first storage element
    emitter.instruction("mov r13, QWORD PTR [rbp - 24]");                       // reload owned replacement Mixed cell
    emitter.instruction("mov QWORD PTR [r12 + r10 * 8], r13");                  // store replacement Mixed cell
    emitter.instruction("jmp __rt_spl_dll_offset_set_done");                    // finish offsetSet
    emitter.label("__rt_spl_dll_offset_set_append");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload receiver for append
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload owned Mixed value for append
    emitter.instruction("call __rt_spl_dll_push");                              // append value using shared push helper
    emitter.instruction("jmp __rt_spl_dll_offset_set_done");                    // finish offsetSet after append
    emitter.label("__rt_spl_dll_offset_set_type_throw");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload rejected owned Mixed value
    emitter.instruction("call __rt_decref_mixed");                              // release rejected value before throwing
    emitter.instruction("add rsp, 64");                                         // release offsetSet frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_dll_offset_set_type_msg",
        SPL_DLL_OFFSET_SET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_dll_offset_set_range_throw");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload owned Mixed value rejected by invalid offset
    emitter.instruction("call __rt_decref_mixed");                              // release rejected value to avoid leaking argument ownership
    emitter.instruction("add rsp, 64");                                         // release offsetSet frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_out_of_range_exception_class_id",
        "_spl_dll_offset_set_range_msg",
        SPL_DLL_OFFSET_SET_RANGE_MSG_LEN,
    );
    emitter.label("__rt_spl_dll_offset_set_done");
    emitter.instruction("add rsp, 64");                                         // release offsetSet frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return void
}

/// Emits `__rt_spl_dll_offset_unset` on x86_64: receiver in rdi, boxed offset in rsi.
/// Validates offset using `emit_offset_index_prefix_x86_64`, releases the old Mixed cell
/// at that slot, compacts storage by shifting subsequent elements left, and returns void.
/// Throws TypeError or OutOfRangeException on invalid offset.
fn emit_offset_unset_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_dll_offset_unset");
    emit_offset_index_prefix_x86_64(
        emitter,
        "__rt_spl_dll_offset_unset_type_throw",
        "__rt_spl_dll_offset_unset_range_throw",
        "__rt_spl_dll_offset_unset_index_ready",
    );
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at first storage element
    emitter.instruction("mov rax, QWORD PTR [r12 + r10 * 8]");                  // load removed Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // preserve storage across removed-value release
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // preserve removed index across release
    emitter.instruction("call __rt_decref_mixed");                              // release removed Mixed cell
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload storage after release
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload removed index after release
    emitter.instruction("mov r11, QWORD PTR [r9]");                             // reload old storage length
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at first storage element
    emitter.instruction("lea r13, [r10 + 1]");                                  // start compaction after removed slot
    emitter.label("__rt_spl_dll_offset_unset_shift_loop");
    emitter.instruction("cmp r13, r11");                                        // have all following elements shifted left?
    emitter.instruction("jge __rt_spl_dll_offset_unset_shrink");                // shrink once compaction is complete
    emitter.instruction("mov r14, QWORD PTR [r12 + r13 * 8]");                  // load next Mixed pointer
    emitter.instruction("mov r15, r13");                                        // copy source index for destination calculation
    emitter.instruction("sub r15, 1");                                          // compute destination index
    emitter.instruction("mov QWORD PTR [r12 + r15 * 8], r14");                  // shift Mixed pointer left by one slot
    emitter.instruction("add r13, 1");                                          // advance compaction cursor
    emitter.instruction("jmp __rt_spl_dll_offset_unset_shift_loop");            // continue compaction
    emitter.label("__rt_spl_dll_offset_unset_shrink");
    emitter.instruction("sub r11, 1");                                          // compute new length
    emitter.instruction("mov QWORD PTR [r9], r11");                             // persist shortened length
    emitter.instruction("mov QWORD PTR [r12 + r11 * 8], 0");                    // clear stale tail slot
    emitter.label("__rt_spl_dll_offset_unset_done");
    emitter.instruction("add rsp, 48");                                         // release offset helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return void
    emitter.label("__rt_spl_dll_offset_unset_type_throw");
    emitter.instruction("add rsp, 48");                                         // release offset helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_dll_offset_unset_type_msg",
        SPL_DLL_OFFSET_UNSET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_dll_offset_unset_range_throw");
    emitter.instruction("add rsp, 48");                                         // release offset helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_out_of_range_exception_class_id",
        "_spl_dll_offset_unset_range_msg",
        SPL_DLL_OFFSET_UNSET_RANGE_MSG_LEN,
    );
}

/// Emits the tail-call sequence to return a boxed null on x86_64: sets rax to NULL_TAG (8),
/// rdi and rsi to zero, and tail-calls `__rt_mixed_from_value` to construct the boxed null.
fn emit_tail_boxed_null_x86_64(emitter: &mut Emitter) {
    emitter.instruction(&format!("mov rax, {}", NULL_TAG));                     // runtime tag 8 = null
    emitter.instruction("xor rdi, rdi");                                        // null payload low word is empty
    emitter.instruction("xor rsi, rsi");                                        // null payload high word is empty
    emitter.instruction("jmp __rt_mixed_from_value");                           // tail-call boxed Mixed construction
}

/// Emits the exception throw helper on ARM64: allocates a Throwable object (kind 6),
/// stores the class id from class_id_symbol, the static message from message_symbol,
/// the message length, and a default code of zero. Publishes the exception object to
/// `_exc_value` and jumps to `__rt_throw_current` to enter the standard exception unwinder.
fn emit_throw_exception_aarch64(
    emitter: &mut Emitter,
    class_id_symbol: &str,
    message_symbol: &str,
    message_len: usize,
) {
    emitter.instruction("mov x0, #32");                                         // request Throwable payload storage
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the exception object payload
    emitter.instruction("mov x9, #6");                                          // heap kind 6 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp allocation as a runtime object
    abi::emit_symbol_address(emitter, "x9", class_id_symbol);
    emitter.instruction("ldr x9, [x9]");                                        // load the exception class id for this program
    emitter.instruction("str x9, [x0]");                                        // store class id at object header
    abi::emit_symbol_address(emitter, "x9", message_symbol);
    emitter.instruction("str x9, [x0, #8]");                                    // store static exception message pointer
    emitter.instruction(&format!("mov x9, #{}", message_len));                  // load static exception message length
    emitter.instruction("str x9, [x0, #16]");                                   // store exception message length
    emitter.instruction("str xzr, [x0, #24]");                                  // exception code defaults to zero
    abi::emit_symbol_address(emitter, "x9", "_exc_value");
    emitter.instruction("str x0, [x9]");                                        // publish the active exception object
    emitter.instruction("b __rt_throw_current");                                // enter the standard exception unwinder
}

/// Emits the exception throw helper on x86_64: allocates a Throwable object (kind 6),
/// stores the class id from class_id_symbol (via RIP-relative load), the static message
/// from message_symbol (via RIP-relativeLEA), the message length, and a default code of zero.
/// Publishes the exception object to `_exc_value` and jumps to `__rt_throw_current` to enter
/// the standard exception unwinder.
fn emit_throw_exception_x86_64(
    emitter: &mut Emitter,
    class_id_symbol: &str,
    message_symbol: &str,
    message_len: usize,
) {
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for exception allocation
    emitter.instruction("mov rbp, rsp");                                        // establish aligned helper frame
    emitter.instruction("sub rsp, 16");                                         // keep the nested heap allocation call 16-byte aligned
    emitter.instruction("mov rax, 32");                                         // request Throwable payload storage
    emitter.instruction("call __rt_heap_alloc");                                // allocate the exception object payload
    emitter.instruction("mov r10, 0x4548504c00000006");                         // x86_64 heap-kind word: HE LP magic + kind 6 object
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp allocation as a runtime object
    emitter.instruction(&format!("mov r10, QWORD PTR [rip + {}]", class_id_symbol)); // load the exception class id for this program
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class id at object header
    emitter.instruction(&format!("lea r10, [rip + {}]", message_symbol));       // materialize static exception message pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store static exception message pointer
    emitter.instruction(&format!("mov QWORD PTR [rax + 16], {}", message_len)); // store static exception message length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // exception code defaults to zero
    emitter.instruction("mov QWORD PTR [rip + _exc_value], rax");               // publish the active exception object
    emitter.instruction("mov rsp, rbp");                                        // release helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emitter.instruction("jmp __rt_throw_current");                              // enter the standard exception unwinder
}
