//! Purpose:
//! Emits runtime helpers for `SplFixedArray`.
//! The helpers back fixed-size storage, ArrayAccess, Countable, and JsonSerializable methods.
//!
//! Called from:
//! - `crate::codegen::runtime::spl::emit_fixed_array_runtime()`.
//!
//! Key details:
//! - Slots store owned boxed `Mixed` cells or null pointers for unset/null entries.
//! - Resize and overwrite paths release replaced Mixed cells before losing ownership.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

use super::SPL_FIXED_STORAGE_OFFSET;

const SPL_FIXED_OBJECT_SIZE: i64 = 16;
const INT_TAG: i64 = 0;
const NULL_TAG: i64 = 8;
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;
const SPL_FIXED_CONSTRUCT_SIZE_MSG_LEN: usize =
    "SplFixedArray::__construct(): Argument #1 ($size) must be greater than or equal to 0".len();
const SPL_FIXED_SET_SIZE_MSG_LEN: usize =
    "SplFixedArray::setSize(): Argument #1 ($size) must be greater than or equal to 0".len();
const SPL_FIXED_OFFSET_TYPE_MSG_LEN: usize =
    "Cannot access offset of type non-int on SplFixedArray".len();
const SPL_FIXED_OFFSET_RANGE_MSG_LEN: usize = "Index invalid or out of range".len();
const SPL_FIXED_FROM_ARRAY_KEYS_MSG_LEN: usize =
    "array must contain only positive integer keys".len();

/// Emits the complete `SplFixedArray` runtime helpers for the target architecture.
/// Routes to either aarch64 or x86_64 emitters based on `emitter.target.arch`.
pub(crate) fn emit_fixed_array_runtime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits all aarch64 `SplFixedArray` runtime helpers via a blank line, section comment,
/// and sequential emission of constructor, Countable, ArrayAccess, and import/export helpers.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl fixed array ---");
    emit_new_aarch64(emitter);
    emit_count_aarch64(emitter);
    emit_set_size_aarch64(emitter);
    emit_offset_exists_aarch64(emitter);
    emit_offset_get_aarch64(emitter);
    emit_offset_set_aarch64(emitter);
    emit_offset_unset_aarch64(emitter);
    emit_to_array_aarch64(emitter);
    emit_from_array_aarch64(emitter);
    emit_unserialize_aarch64(emitter);
    emit_copy_from_array_aarch64(emitter);
}

/// Emits `__rt_spl_fixed_new` on aarch64: constructs an initialized SplFixedArray object.
/// Allocates the object header and fixed-size storage array, stamps the heap kind, stores
/// the class id, zeroes all slots to unset/null, and returns the object pointer.
/// Clobbers: x0, x1, x2, x9, x10, x11, x12. Saves and restores x29, x30.
fn emit_new_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_new");
    emitter.instruction("sub sp, sp, #48");                                     // reserve constructor spill slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish constructor frame
    emitter.instruction("str x0, [sp, #0]");                                    // save concrete SplFixedArray class id
    emitter.instruction("cmp x1, #0");                                          // reject negative sizes
    emitter.instruction("b.lt __rt_spl_fixed_new_size_throw");                  // negative sizes raise ValueError
    emitter.instruction("str x1, [sp, #8]");                                    // save requested fixed size
    emitter.instruction(&format!("mov x0, #{}", SPL_FIXED_OBJECT_SIZE));        // request fixed-array object payload size
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate SplFixedArray object payload
    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp allocation as an object instance
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload concrete class id
    emitter.instruction("str x9, [x0]");                                        // store class id at object header
    emitter.instruction("str x0, [sp, #16]");                                   // save object pointer while allocating storage
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload requested fixed size as array capacity
    emitter.instruction("mov x1, #8");                                          // each slot holds one Mixed pointer
    emitter.instruction("bl __rt_array_new");                                   // allocate fixed-array storage
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load storage packed kind word
    emitter.instruction("orr x9, x9, #0x700");                                  // mark storage as containing Mixed cells
    emitter.instruction("str x9, [x0, #-8]");                                   // persist Mixed value_type tag
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload fixed size
    emitter.instruction("str x10, [x0]");                                       // logical length is the fixed size
    emitter.instruction("add x11, x0, #24");                                    // point at first storage slot
    emitter.instruction("mov x12, #0");                                         // initialize slot-zeroing cursor
    emitter.label("__rt_spl_fixed_new_zero_loop");
    emitter.instruction("cmp x12, x10");                                        // have all slots been initialized?
    emitter.instruction("b.ge __rt_spl_fixed_new_done");                        // finish once every slot is zeroed
    emitter.instruction("str xzr, [x11, x12, lsl #3]");                         // initialize slot as unset/null
    emitter.instruction("add x12, x12, #1");                                    // advance zeroing cursor
    emitter.instruction("b __rt_spl_fixed_new_zero_loop");                      // continue zeroing storage slots
    emitter.label("__rt_spl_fixed_new_done");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload object pointer
    emitter.instruction(&format!("str x0, [x9, #{}]", SPL_FIXED_STORAGE_OFFSET)); // object.storage = fixed-array storage
    emitter.instruction("mov x0, x9");                                          // return initialized SplFixedArray object
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release constructor frame
    emitter.instruction("ret");                                                 // return object pointer
    emitter.label("__rt_spl_fixed_new_size_throw");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #48");                                     // release constructor frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_value_error_class_id",
        "_spl_fixed_construct_size_msg",
        SPL_FIXED_CONSTRUCT_SIZE_MSG_LEN,
    );
}

/// Emits `__rt_spl_fixed_count` on aarch64: returns the logical fixed size of the array.
/// x0 = receiver SplFixedArray object. Returns the fixed size in x0.
fn emit_count_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_count");
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_FIXED_STORAGE_OFFSET)); // load fixed-array storage
    emitter.instruction("ldr x0, [x9]");                                        // return logical fixed size
    emitter.instruction("ret");                                                 // return count/getSize result
}

/// Emits `__rt_spl_fixed_set_size` on aarch64: resizes the fixed array.
/// x0 = receiver SplFixedArray object, x1 = new size (must be >= 0).
/// Grows storage if capacity is insufficient; releases truncated tail slots via `__rt_decref_mixed`;
/// zero-fills newly exposed slots. Throws ValueError if size is negative.
fn emit_set_size_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_set_size");
    emitter.instruction("sub sp, sp, #64");                                     // reserve resize state and call frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish resize frame
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver
    emitter.instruction("cmp x1, #0");                                          // reject negative sizes
    emitter.instruction("b.lt __rt_spl_fixed_set_size_throw");                  // negative sizes raise ValueError
    emitter.instruction("str x1, [sp, #8]");                                    // save requested new size
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_FIXED_STORAGE_OFFSET)); // load current storage
    emitter.instruction("str x9, [sp, #16]");                                   // save current storage pointer
    emitter.instruction("ldr x10, [x9]");                                       // load old size
    emitter.instruction("str x10, [sp, #24]");                                  // save old size
    emitter.label("__rt_spl_fixed_set_size_grow_check");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload storage pointer
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload requested size
    emitter.instruction("ldr x11, [x9, #8]");                                   // load storage capacity
    emitter.instruction("cmp x10, x11");                                        // does storage need more capacity?
    emitter.instruction("b.le __rt_spl_fixed_set_size_release_tail");           // skip growth when capacity is enough
    emitter.instruction("mov x0, x9");                                          // pass current storage to array_grow
    emitter.instruction("bl __rt_array_grow");                                  // grow fixed-array storage
    emitter.instruction("str x0, [sp, #16]");                                   // save grown storage pointer
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver
    emitter.instruction(&format!("str x0, [x9, #{}]", SPL_FIXED_STORAGE_OFFSET)); // publish grown storage on receiver
    emitter.instruction("b __rt_spl_fixed_set_size_grow_check");                // grow again if requested size still exceeds capacity
    emitter.label("__rt_spl_fixed_set_size_release_tail");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload storage pointer
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload requested new size
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload old size
    emitter.instruction("add x12, x9, #24");                                    // point at first storage slot
    emitter.instruction("mov x13, x10");                                        // tail-release cursor starts at new size
    emitter.label("__rt_spl_fixed_set_size_tail_loop");
    emitter.instruction("cmp x13, x11");                                        // have all truncated slots been released?
    emitter.instruction("b.ge __rt_spl_fixed_set_size_zero_new");               // stop tail release at old size
    emitter.instruction("ldr x0, [x12, x13, lsl #3]");                          // load truncated Mixed cell
    emitter.instruction("str x9, [sp, #16]");                                   // preserve storage across release
    emitter.instruction("str x10, [sp, #8]");                                   // preserve requested size across release
    emitter.instruction("str x11, [sp, #24]");                                  // preserve old size across release
    emitter.instruction("str x13, [sp, #32]");                                  // preserve tail-release cursor
    emitter.instruction("bl __rt_decref_mixed");                                // release truncated Mixed cell if present
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload storage after release
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload requested size after release
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload old size after release
    emitter.instruction("ldr x13, [sp, #32]");                                  // reload tail-release cursor after release
    emitter.instruction("add x12, x9, #24");                                    // restore storage slot base
    emitter.instruction("str xzr, [x12, x13, lsl #3]");                         // clear released slot
    emitter.instruction("add x13, x13, #1");                                    // advance tail-release cursor
    emitter.instruction("b __rt_spl_fixed_set_size_tail_loop");                 // continue releasing truncated slots
    emitter.label("__rt_spl_fixed_set_size_zero_new");
    emitter.instruction("ldr x13, [sp, #24]");                                  // zero-fill starts at old size
    emitter.label("__rt_spl_fixed_set_size_zero_loop");
    emitter.instruction("cmp x13, x10");                                        // have all newly exposed slots been zeroed?
    emitter.instruction("b.ge __rt_spl_fixed_set_size_done");                   // finish after zero-filling to requested size
    emitter.instruction("str xzr, [x12, x13, lsl #3]");                         // initialize grown slot as unset/null
    emitter.instruction("add x13, x13, #1");                                    // advance zero-fill cursor
    emitter.instruction("b __rt_spl_fixed_set_size_zero_loop");                 // continue zero-filling new slots
    emitter.label("__rt_spl_fixed_set_size_done");
    emitter.instruction("str x10, [x9]");                                       // store new logical fixed size
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release resize frame
    emitter.instruction("ret");                                                 // return void
    emitter.label("__rt_spl_fixed_set_size_throw");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release resize frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_value_error_class_id",
        "_spl_fixed_set_size_msg",
        SPL_FIXED_SET_SIZE_MSG_LEN,
    );
}

/// Emits `__rt_spl_fixed_offset_exists` on aarch64: ArrayAccess `offsetExists`.
/// x0 = receiver, x1 = boxed offset. Returns true (1) if the offset is within range
/// and the slot is neither unset nor explicitly null; otherwise returns false.
/// Throws TypeError if offset is not an integer.
fn emit_offset_exists_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_offset_exists");
    emit_offset_prefix_aarch64(
        emitter,
        "__rt_spl_fixed_offset_exists_type_throw",
        "__rt_spl_fixed_offset_exists_false",
    );
    emitter.instruction("add x11, x9, #24");                                    // point at first storage slot
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load candidate Mixed cell pointer
    emitter.instruction("cbz x12, __rt_spl_fixed_offset_exists_false");         // unset slots do not exist for isset()
    emitter.instruction("ldr x13, [x12]");                                      // load boxed Mixed tag
    emitter.instruction(&format!("cmp x13, #{}", NULL_TAG));                    // explicit null behaves like unset for isset()
    emitter.instruction("cset x0, ne");                                         // return true when slot is non-null
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release offset frame
    emitter.instruction("ret");                                                 // return boolean result
    emitter.label("__rt_spl_fixed_offset_exists_false");
    emitter.instruction("mov x0, #0");                                          // invalid/unset offsets return false
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release offset frame
    emitter.instruction("ret");                                                 // return false
    emitter.label("__rt_spl_fixed_offset_exists_type_throw");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release offset frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_fixed_offset_type_msg",
        SPL_FIXED_OFFSET_TYPE_MSG_LEN,
    );
}

/// Emits `__rt_spl_fixed_offset_get` on aarch64: ArrayAccess `offsetGet`.
/// x0 = receiver, x1 = boxed offset. Returns the retained Mixed cell at the offset,
/// or a newly boxed null if the slot is unset. Throws TypeError if offset is not an integer;
/// throws OutOfBoundsException if offset is negative or >= fixed size.
fn emit_offset_get_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_offset_get");
    emit_offset_prefix_aarch64(
        emitter,
        "__rt_spl_fixed_offset_get_type_throw",
        "__rt_spl_fixed_offset_get_range_throw",
    );
    emitter.instruction("add x11, x9, #24");                                    // point at first storage slot
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // load selected Mixed cell
    emitter.instruction("cbz x0, __rt_spl_fixed_offset_get_null_loaded");       // unset slots read as null
    emitter.instruction("bl __rt_incref");                                      // retain selected Mixed cell for caller
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release offset frame
    emitter.instruction("ret");                                                 // return retained Mixed cell
    emitter.label("__rt_spl_fixed_offset_get_null");
    emitter.label("__rt_spl_fixed_offset_get_null_loaded");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before null return
    emitter.instruction("add sp, sp, #64");                                     // release offset frame before null return
    emit_tail_boxed_null_aarch64(emitter);
    emitter.label("__rt_spl_fixed_offset_get_type_throw");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release offset frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_fixed_offset_type_msg",
        SPL_FIXED_OFFSET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_fixed_offset_get_range_throw");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release offset frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_out_of_bounds_exception_class_id",
        "_spl_fixed_offset_range_msg",
        SPL_FIXED_OFFSET_RANGE_MSG_LEN,
    );
}

/// Emits `__rt_spl_fixed_offset_set` on aarch64: ArrayAccess `offsetSet`.
/// x0 = receiver, x1 = boxed offset, x2 = owned Mixed value to store.
/// Releases the previous slot value via `__rt_decref_mixed` before overwriting.
/// Throws TypeError if offset is not an integer; throws OutOfBoundsException if out of range.
fn emit_offset_set_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_offset_set");
    emitter.instruction("sub sp, sp, #80");                                     // reserve offset-set frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish offset-set frame
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver
    emitter.instruction("str x1, [sp, #8]");                                    // save boxed offset
    emitter.instruction("str x2, [sp, #16]");                                   // save owned Mixed value
    emit_unbox_saved_offset_aarch64(emitter);
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload offset tag
    emitter.instruction(&format!("cmp x12, #{}", INT_TAG));                     // fixed-array offsets must be integers
    emitter.instruction("b.ne __rt_spl_fixed_offset_set_type_throw");           // reject non-integer offsets
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload integer offset
    emitter.instruction("cmp x10, #0");                                         // reject negative offsets
    emitter.instruction("b.lt __rt_spl_fixed_offset_set_range_throw");          // negative offsets are out of range
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload receiver
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_FIXED_STORAGE_OFFSET)); // load fixed-array storage
    emitter.instruction("ldr x11, [x9]");                                       // load fixed size
    emitter.instruction("cmp x10, x11");                                        // compare offset against fixed size
    emitter.instruction("b.hs __rt_spl_fixed_offset_set_range_throw");          // reject offsets outside fixed range
    emitter.instruction("add x12, x9, #24");                                    // point at first storage slot
    emitter.instruction("ldr x0, [x12, x10, lsl #3]");                          // load previous Mixed cell
    emitter.instruction("str x9, [sp, #40]");                                   // preserve storage across release
    emitter.instruction("str x10, [sp, #48]");                                  // preserve offset across release
    emitter.instruction("bl __rt_decref_mixed");                                // release previous slot value if present
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload storage after release
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload offset after release
    emitter.instruction("add x12, x9, #24");                                    // restore storage slot base
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload owned Mixed replacement
    emitter.instruction("str x13, [x12, x10, lsl #3]");                         // store replacement Mixed cell
    emitter.instruction("b __rt_spl_fixed_offset_set_done");                    // finish offsetSet
    emitter.label("__rt_spl_fixed_offset_set_type_throw");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload rejected owned Mixed value
    emitter.instruction("bl __rt_decref_mixed");                                // release rejected value before throwing
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #80");                                     // release offset-set frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_fixed_offset_type_msg",
        SPL_FIXED_OFFSET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_fixed_offset_set_range_throw");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload rejected owned Mixed value
    emitter.instruction("bl __rt_decref_mixed");                                // release rejected value
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #80");                                     // release offset-set frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_out_of_bounds_exception_class_id",
        "_spl_fixed_offset_range_msg",
        SPL_FIXED_OFFSET_RANGE_MSG_LEN,
    );
    emitter.label("__rt_spl_fixed_offset_set_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release offset-set frame
    emitter.instruction("ret");                                                 // return void
}

/// Emits `__rt_spl_fixed_offset_unset` on aarch64: ArrayAccess `offsetUnset`.
/// x0 = receiver, x1 = boxed offset. Releases the existing Mixed cell at the slot
/// via `__rt_decref_mixed` and marks the slot as unset/null.
/// Throws TypeError if offset is not an integer; throws OutOfBoundsException if out of range.
fn emit_offset_unset_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_offset_unset");
    emit_offset_prefix_aarch64(
        emitter,
        "__rt_spl_fixed_offset_unset_type_throw",
        "__rt_spl_fixed_offset_unset_range_throw",
    );
    emitter.instruction("add x11, x9, #24");                                    // point at first storage slot
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // load existing Mixed cell
    emitter.instruction("str x11, [sp, #32]");                                  // preserve storage slot base across release
    emitter.instruction("str x10, [sp, #40]");                                  // preserve offset across release
    emitter.instruction("bl __rt_decref_mixed");                                // release existing Mixed cell if present
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload storage slot base
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload offset
    emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // mark slot unset/null
    emitter.label("__rt_spl_fixed_offset_unset_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release offset frame
    emitter.instruction("ret");                                                 // return void
    emitter.label("__rt_spl_fixed_offset_unset_type_throw");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release offset frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_fixed_offset_type_msg",
        SPL_FIXED_OFFSET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_fixed_offset_unset_range_throw");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #64");                                     // release offset frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_out_of_bounds_exception_class_id",
        "_spl_fixed_offset_range_msg",
        SPL_FIXED_OFFSET_RANGE_MSG_LEN,
    );
}

/// Emits the common offset validation prefix for aarch64 offset operations.
/// Saves frame state, unboxes the offset argument, validates it is a non-negative integer
/// within the fixed array range, and branches to `type_label` on TypeError or `range_label`
/// on out-of-range. On success, x9 = storage pointer, x10 = integer offset.
/// Clobbers: x0, x1, x9, x10, x11, x12. Saves and restores x29, x30.
fn emit_offset_prefix_aarch64(emitter: &mut Emitter, type_label: &str, range_label: &str) {
    emitter.instruction("sub sp, sp, #64");                                     // reserve common offset frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish common offset frame
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver
    emitter.instruction("str x1, [sp, #8]");                                    // save boxed offset
    emit_unbox_saved_offset_aarch64(emitter);
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload offset tag
    emitter.instruction(&format!("cmp x12, #{}", INT_TAG));                     // fixed-array offsets must be integers
    emitter.instruction(&format!("b.ne {}", type_label));                       // reject non-integer offsets
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload integer offset
    emitter.instruction("cmp x10, #0");                                         // reject negative offsets
    emitter.instruction(&format!("b.lt {}", range_label));                      // negative offsets are invalid
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload receiver
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_FIXED_STORAGE_OFFSET)); // load fixed-array storage
    emitter.instruction("ldr x11, [x9]");                                       // load fixed size
    emitter.instruction("cmp x10, x11");                                        // compare offset against fixed size
    emitter.instruction(&format!("b.hs {}", range_label));                      // reject offsets outside fixed range
}

/// Emits the aarch64 helper that unboxes the saved boxed offset argument.
/// Loads the boxed offset from [sp+#8], calls `__rt_mixed_unbox` to produce tag (x0) and
/// integer payload candidate (x1), saves them to [sp+#24] and [sp+#32], then releases
/// the boxed offset via `__rt_decref_mixed`. Result: offset tag at [sp+#24], integer at [sp+#32].
fn emit_unbox_saved_offset_aarch64(emitter: &mut Emitter) {
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload boxed offset argument
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox offset into tag and payload words
    emitter.instruction("str x0, [sp, #24]");                                   // save unboxed offset tag
    emitter.instruction("str x1, [sp, #32]");                                   // save unboxed integer payload candidate
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload boxed offset argument
    emitter.instruction("bl __rt_decref_mixed");                                // release owned boxed offset argument
}

/// Emits `__rt_spl_fixed_to_array` on aarch64: converts the SplFixedArray to a PHP array.
/// Allocates a PHP array of the same logical length, copies each slot's Mixed cell
/// (retaining it for the result array), or boxed null for unset slots. Returns the new array in x0.
fn emit_to_array_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_to_array");
    emitter.instruction("sub sp, sp, #64");                                     // reserve toArray frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish toArray frame
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SPL_FIXED_STORAGE_OFFSET)); // load fixed-array storage
    emitter.instruction("str x9, [sp, #0]");                                    // save source storage
    emitter.instruction("ldr x10, [x9]");                                       // load fixed size
    emitter.instruction("str x10, [sp, #8]");                                   // save fixed size
    emitter.instruction("mov x0, x10");                                         // result array capacity equals fixed size
    emitter.instruction("mov x1, #8");                                          // result slots hold Mixed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate result PHP array
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load result packed kind word
    emitter.instruction("orr x9, x9, #0x700");                                  // mark result as containing Mixed cells
    emitter.instruction("str x9, [x0, #-8]");                                   // persist result Mixed value_type tag
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload fixed size
    emitter.instruction("str x10, [x0]");                                       // result logical length equals fixed size
    emitter.instruction("str x0, [sp, #16]");                                   // save result array
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize copy cursor
    emitter.label("__rt_spl_fixed_to_array_loop");
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload fixed size
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload copy cursor
    emitter.instruction("cmp x11, x10");                                        // have all slots been copied?
    emitter.instruction("b.ge __rt_spl_fixed_to_array_done");                   // finish after copying fixed size slots
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload source storage
    emitter.instruction("add x9, x9, #24");                                     // point at source slot base
    emitter.instruction("ldr x0, [x9, x11, lsl #3]");                           // load source Mixed cell pointer
    emitter.instruction("cbz x0, __rt_spl_fixed_to_array_null");                // unset slots become boxed null
    emitter.instruction("bl __rt_incref");                                      // retain source Mixed cell for result array
    emitter.instruction("b __rt_spl_fixed_to_array_store");                     // store retained slot
    emitter.label("__rt_spl_fixed_to_array_null");
    emit_boxed_null_call_aarch64(emitter);
    emitter.label("__rt_spl_fixed_to_array_store");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload result array
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload copy cursor
    emitter.instruction("add x9, x9, #24");                                     // point at result slot base
    emitter.instruction("str x0, [x9, x11, lsl #3]");                           // store owned Mixed cell into result array
    emitter.instruction("add x11, x11, #1");                                    // advance copy cursor
    emitter.instruction("str x11, [sp, #24]");                                  // save updated copy cursor
    emitter.instruction("b __rt_spl_fixed_to_array_loop");                      // continue copying slots
    emitter.label("__rt_spl_fixed_to_array_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // return result array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release toArray frame
    emitter.instruction("ret");                                                 // return PHP array
}

/// Emits `__rt_spl_fixed_from_array` on aarch64: constructs a SplFixedArray from a PHP array.
/// x0 = SplFixedArray class id, x1 = source PHP array, x2 = preserveKeys flag.
/// Computes the required size from indexed or hash sources, allocates via `__rt_spl_fixed_new`,
/// then imports values via `__rt_spl_fixed_copy_from_array`. Throws InvalidArgumentException
/// if a hash source with preserveKeys=true contains non-integer or negative keys.
fn emit_from_array_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_from_array");
    emitter.instruction("sub sp, sp, #96");                                     // reserve fromArray frame and hash sizing cursor
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish fromArray frame
    emitter.instruction("str x0, [sp, #0]");                                    // save SplFixedArray class id
    emitter.instruction("str x1, [sp, #8]");                                    // save source PHP array
    emitter.instruction("str x2, [sp, #16]");                                   // save preserveKeys flag
    emitter.instruction("ldr x9, [x1, #-8]");                                   // load source heap kind metadata
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low heap kind byte
    emitter.instruction("cmp x9, #3");                                          // is the source an associative array hash?
    emitter.instruction("b.eq __rt_spl_fixed_from_array_hash_size");            // hash sources need key-aware sizing
    emitter.instruction("ldr x10, [x1]");                                       // indexed source size equals logical array length
    emitter.instruction("str x10, [sp, #24]");                                  // save constructor size
    emitter.instruction("b __rt_spl_fixed_from_array_alloc");                   // allocate with the indexed source length
    emitter.label("__rt_spl_fixed_from_array_hash_size");
    emitter.instruction("ldr x10, [x1]");                                       // load hash entry count for packed import
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload preserveKeys flag
    emitter.instruction("cbnz x11, __rt_spl_fixed_from_array_hash_preserve_size"); // preserveKeys=true sizes by the maximum numeric key
    emitter.instruction("str x10, [sp, #24]");                                  // packed hash import size equals entry count
    emitter.instruction("b __rt_spl_fixed_from_array_alloc");                   // allocate with packed hash size
    emitter.label("__rt_spl_fixed_from_array_hash_preserve_size");
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize max numeric key plus one
    emitter.instruction("str xzr, [sp, #32]");                                  // initialize hash iterator cursor
    emitter.label("__rt_spl_fixed_from_array_hash_size_loop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload source hash for sizing iteration
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload current hash iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next hash entry in insertion order
    emitter.instruction("cmn x0, #1");                                          // did sizing iteration reach the end sentinel?
    emitter.instruction("b.eq __rt_spl_fixed_from_array_alloc");                // allocate once every numeric key has been inspected
    emitter.instruction("str x0, [sp, #32]");                                   // save next hash iterator cursor
    emitter.instruction("cmn x2, #1");                                          // integer keys use key_len=-1
    emitter.instruction("b.ne __rt_spl_fixed_from_array_keys_throw");           // preserveKeys rejects string keys
    emitter.instruction("cmp x1, #0");                                          // negative keys are not fixed-array offsets
    emitter.instruction("b.lt __rt_spl_fixed_from_array_keys_throw");           // preserveKeys rejects negative keys
    emitter.instruction("add x1, x1, #1");                                      // candidate size is numeric key + 1
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload current maximum size
    emitter.instruction("cmp x1, x9");                                          // is this key beyond the current size?
    emitter.instruction("csel x9, x1, x9, gt");                                 // keep the larger size
    emitter.instruction("str x9, [sp, #24]");                                   // save updated maximum size
    emitter.instruction("b __rt_spl_fixed_from_array_hash_size_loop");          // continue sizing preserved hash keys
    emitter.label("__rt_spl_fixed_from_array_alloc");
    emitter.instruction("ldr x1, [sp, #24]");                                   // pass computed constructor size
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload SplFixedArray class id
    emitter.instruction("bl __rt_spl_fixed_new");                               // allocate a fixed array sized like the source
    emitter.instruction("str x0, [sp, #40]");                                   // save new fixed-array object
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source PHP array
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload preserveKeys flag
    emitter.instruction("bl __rt_spl_fixed_copy_from_array");                   // copy source values into the fixed array
    emitter.instruction("ldr x0, [sp, #40]");                                   // return the populated fixed-array object
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release fromArray frame
    emitter.instruction("ret");                                                 // return populated SplFixedArray
    emitter.label("__rt_spl_fixed_from_array_keys_throw");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer before throwing
    emitter.instruction("add sp, sp, #96");                                     // release fromArray frame before throwing
    emit_throw_exception_aarch64(
        emitter,
        "_spl_invalid_argument_exception_class_id",
        "_spl_fixed_from_array_keys_msg",
        SPL_FIXED_FROM_ARRAY_KEYS_MSG_LEN,
    );
}

/// Emits `__rt_spl_fixed_unserialize` on aarch64: serialization entry point.
/// Sets x2=0 (ignore PHP array keys) and tail-calls `__rt_spl_fixed_copy_from_array`.
fn emit_unserialize_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_unserialize");
    emitter.instruction("mov x2, xzr");                                         // __unserialize packs source values and ignores PHP array keys
    emitter.instruction("b __rt_spl_fixed_copy_from_array");                    // __unserialize reuses the generic array import path
}

/// Emits `__rt_spl_fixed_copy_from_array` on aarch64: populates a SplFixedArray from a PHP array.
/// x0 = receiver SplFixedArray, x1 = source PHP array, x2 = preserveKeys flag.
/// For indexed sources: normalizes slots to boxed Mixed, resizes receiver, copies slot values.
/// For hash sources (preserveKeys=false): imports values in insertion order at packed indices.
/// For hash sources (preserveKeys=true): resizes to max numeric key + 1, copies only integer-keyed
/// entries at their preserved offsets. Releases any overwritten destination cells via `__rt_decref_mixed`.
fn emit_copy_from_array_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_copy_from_array");
    emitter.instruction("sub sp, sp, #128");                                    // reserve import frame, hash cursor, and value spills
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish import frame
    emitter.instruction("str x0, [sp, #0]");                                    // save receiver
    emitter.instruction("str x1, [sp, #8]");                                    // save source PHP array
    emitter.instruction("str x2, [sp, #16]");                                   // save preserveKeys flag
    emitter.instruction("ldr x9, [x1, #-8]");                                   // load source heap kind metadata
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low heap kind byte
    emitter.instruction("cmp x9, #3");                                          // is the source an associative array hash?
    emitter.instruction("b.eq __rt_spl_fixed_copy_from_hash");                  // hash sources need insertion-order import
    emitter.instruction("mov x0, x1");                                          // pass source array to mixed conversion
    emitter.instruction("ldr x1, [x0, #-8]");                                   // load packed array kind and value-type metadata
    emitter.instruction("lsr x1, x1, #8");                                      // move value_type tag down to the low byte
    emitter.instruction("and x1, x1, #0x7f");                                   // isolate the indexed-array value_type tag without COW metadata
    emitter.instruction("bl __rt_array_to_mixed");                              // normalize source slots to boxed Mixed cells
    emitter.instruction("str x0, [sp, #8]");                                    // save possibly-converted source array
    emitter.instruction("ldr x1, [x0]");                                        // load source logical length
    emitter.instruction("str x1, [sp, #24]");                                   // save source length
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload receiver for resize
    emitter.instruction("bl __rt_spl_fixed_set_size");                          // resize receiver to exactly the source length
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver after resize
    emitter.instruction(&format!("ldr x9, [x9, #{}]", SPL_FIXED_STORAGE_OFFSET)); // load destination fixed storage
    emitter.instruction("str x9, [sp, #32]");                                   // save destination storage
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize copy cursor
    emitter.label("__rt_spl_fixed_copy_from_array_loop");
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload copy cursor
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload source length
    emitter.instruction("cmp x10, x11");                                        // have all source slots been copied?
    emitter.instruction("b.ge __rt_spl_fixed_copy_from_array_done");            // finish once every slot is imported
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload destination storage
    emitter.instruction("add x12, x9, #24");                                    // point at destination slot base
    emitter.instruction("ldr x0, [x12, x10, lsl #3]");                          // load existing destination Mixed cell
    emitter.instruction("str x10, [sp, #40]");                                  // preserve cursor across release
    emitter.instruction("bl __rt_decref_mixed");                                // release any overwritten destination cell
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload copy cursor after release
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload source array
    emitter.instruction("add x9, x9, #24");                                     // point at source Mixed slot base
    emitter.instruction("ldr x0, [x9, x10, lsl #3]");                           // load source Mixed cell
    emitter.instruction("bl __rt_incref");                                      // retain source cell for fixed-array storage
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload copy cursor after retain
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload destination storage after retain
    emitter.instruction("add x12, x9, #24");                                    // point at destination slot base
    emitter.instruction("str x0, [x12, x10, lsl #3]");                          // store retained Mixed cell into fixed-array slot
    emitter.instruction("add x10, x10, #1");                                    // advance copy cursor
    emitter.instruction("str x10, [sp, #40]");                                  // save updated copy cursor
    emitter.instruction("b __rt_spl_fixed_copy_from_array_loop");               // continue copying source slots
    emitter.label("__rt_spl_fixed_copy_from_hash");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload preserveKeys flag
    emitter.instruction("cbnz x9, __rt_spl_fixed_copy_from_hash_preserve");     // preserve numeric keys when requested
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload source hash
    emitter.instruction("ldr x1, [x9]");                                        // packed hash import size equals live entry count
    emitter.instruction("str x1, [sp, #24]");                                   // save destination size
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload receiver for resize
    emitter.instruction("bl __rt_spl_fixed_set_size");                          // resize receiver for packed hash import
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver after resize
    emitter.instruction(&format!("ldr x9, [x9, #{}]", SPL_FIXED_STORAGE_OFFSET)); // load destination fixed storage
    emitter.instruction("str x9, [sp, #32]");                                   // save destination storage
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize packed destination index
    emitter.instruction("str xzr, [sp, #48]");                                  // initialize hash iterator cursor
    emitter.label("__rt_spl_fixed_copy_hash_packed_loop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload source hash for iteration
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload current hash iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next hash entry in insertion order
    emitter.instruction("cmn x0, #1");                                          // did iteration reach the end sentinel?
    emitter.instruction("b.eq __rt_spl_fixed_copy_from_array_done");            // finish after every hash value has been copied
    emitter.instruction("str x0, [sp, #48]");                                   // save next hash iterator cursor
    emitter.instruction("str x3, [sp, #56]");                                   // save hash value low payload
    emitter.instruction("str x4, [sp, #64]");                                   // save hash value high payload
    emitter.instruction("str x5, [sp, #72]");                                   // save hash value runtime tag
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload packed destination index
    emitter.instruction("b __rt_spl_fixed_copy_hash_store");                    // store this value at the packed destination index
    emitter.label("__rt_spl_fixed_copy_from_hash_preserve");
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize max numeric key plus one
    emitter.instruction("str xzr, [sp, #48]");                                  // initialize hash sizing cursor
    emitter.label("__rt_spl_fixed_copy_hash_preserve_size_loop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload source hash for sizing
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload current hash iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next hash entry for preserved sizing
    emitter.instruction("cmn x0, #1");                                          // did sizing reach the end sentinel?
    emitter.instruction("b.eq __rt_spl_fixed_copy_hash_preserve_resize");       // resize once numeric keys have been inspected
    emitter.instruction("str x0, [sp, #48]");                                   // save next hash iterator cursor
    emitter.instruction("cmn x2, #1");                                          // integer keys use key_len=-1
    emitter.instruction("b.ne __rt_spl_fixed_copy_hash_preserve_size_loop");    // string keys do not map to fixed offsets
    emitter.instruction("cmp x1, #0");                                          // negative keys are not fixed-array offsets
    emitter.instruction("b.lt __rt_spl_fixed_copy_hash_preserve_size_loop");    // ignore negative keys for now
    emitter.instruction("add x1, x1, #1");                                      // candidate size is numeric key + 1
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload current maximum size
    emitter.instruction("cmp x1, x9");                                          // is this key beyond the current size?
    emitter.instruction("csel x9, x1, x9, gt");                                 // keep the larger size
    emitter.instruction("str x9, [sp, #24]");                                   // save updated maximum size
    emitter.instruction("b __rt_spl_fixed_copy_hash_preserve_size_loop");       // continue sizing preserved hash keys
    emitter.label("__rt_spl_fixed_copy_hash_preserve_resize");
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload preserved destination size
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload receiver for resize
    emitter.instruction("bl __rt_spl_fixed_set_size");                          // resize receiver for preserved numeric keys
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload receiver after resize
    emitter.instruction(&format!("ldr x9, [x9, #{}]", SPL_FIXED_STORAGE_OFFSET)); // load destination fixed storage
    emitter.instruction("str x9, [sp, #32]");                                   // save destination storage
    emitter.instruction("str xzr, [sp, #48]");                                  // reset hash iterator cursor for value copy
    emitter.label("__rt_spl_fixed_copy_hash_preserve_loop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload source hash for preserved import
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload current hash iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next hash entry in insertion order
    emitter.instruction("cmn x0, #1");                                          // did iteration reach the end sentinel?
    emitter.instruction("b.eq __rt_spl_fixed_copy_from_array_done");            // finish after every preserved numeric key has been copied
    emitter.instruction("str x0, [sp, #48]");                                   // save next hash iterator cursor
    emitter.instruction("cmn x2, #1");                                          // integer keys use key_len=-1
    emitter.instruction("b.ne __rt_spl_fixed_copy_hash_preserve_loop");         // string keys are skipped by this fixed-offset import path
    emitter.instruction("cmp x1, #0");                                          // negative keys are not fixed-array offsets
    emitter.instruction("b.lt __rt_spl_fixed_copy_hash_preserve_loop");         // skip negative keys
    emitter.instruction("str x1, [sp, #40]");                                   // save numeric destination index
    emitter.instruction("str x3, [sp, #56]");                                   // save hash value low payload
    emitter.instruction("str x4, [sp, #64]");                                   // save hash value high payload
    emitter.instruction("str x5, [sp, #72]");                                   // save hash value runtime tag
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload preserved destination index
    emitter.label("__rt_spl_fixed_copy_hash_store");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload destination storage
    emitter.instruction("add x12, x9, #24");                                    // point at destination slot base
    emitter.instruction("ldr x0, [x12, x10, lsl #3]");                          // load existing destination Mixed cell
    emitter.instruction("str x10, [sp, #80]");                                  // preserve destination index across release
    emitter.instruction("bl __rt_decref_mixed");                                // release any overwritten destination cell
    emitter.instruction("ldr x5, [sp, #72]");                                   // reload hash value runtime tag
    emitter.instruction("cmp x5, #7");                                          // is the hash value already a boxed Mixed cell?
    emitter.instruction("b.eq __rt_spl_fixed_copy_hash_retain_mixed");          // retain existing boxes instead of nesting them
    emitter.instruction("mov x0, x5");                                          // pass runtime tag to mixed boxing helper
    emitter.instruction("ldr x1, [sp, #56]");                                   // pass value low payload to mixed boxing helper
    emitter.instruction("ldr x2, [sp, #64]");                                   // pass value high payload to mixed boxing helper
    emitter.instruction("bl __rt_mixed_from_value");                            // box and retain/persist the hash value for fixed storage
    emitter.instruction("b __rt_spl_fixed_copy_hash_store_box");                // skip existing-box retain path
    emitter.label("__rt_spl_fixed_copy_hash_retain_mixed");
    emitter.instruction("ldr x0, [sp, #56]");                                   // load existing boxed Mixed value
    emitter.instruction("bl __rt_incref");                                      // retain existing boxed Mixed value for fixed storage
    emitter.label("__rt_spl_fixed_copy_hash_store_box");
    emitter.instruction("ldr x10, [sp, #80]");                                  // reload destination index after boxing
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload destination storage after boxing
    emitter.instruction("add x12, x9, #24");                                    // point at destination slot base
    emitter.instruction("str x0, [x12, x10, lsl #3]");                          // store owned Mixed value into fixed-array slot
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload preserveKeys flag
    emitter.instruction("cbnz x9, __rt_spl_fixed_copy_hash_preserve_loop");     // preserved imports keep source numeric keys
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload packed destination index
    emitter.instruction("add x10, x10, #1");                                    // advance packed destination index
    emitter.instruction("str x10, [sp, #40]");                                  // save updated packed destination index
    emitter.instruction("b __rt_spl_fixed_copy_hash_packed_loop");              // continue packed hash import
    emitter.label("__rt_spl_fixed_copy_from_array_done");
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release import frame
    emitter.instruction("ret");                                                 // return void
}

/// Emits a tail-call sequence to construct a boxed null Mixed cell on aarch64.
/// Sets NULL_TAG (8) as runtime tag, zero payload words, and tail-calls `__rt_mixed_from_value`.
/// Used when returning null from a leaf call path (e.g., offsetGet on unset slot).
fn emit_tail_boxed_null_aarch64(emitter: &mut Emitter) {
    emitter.instruction(&format!("mov x0, #{}", NULL_TAG));                     // runtime tag 8 = null
    emitter.instruction("mov x1, xzr");                                         // null payload low word is empty
    emitter.instruction("mov x2, xzr");                                         // null payload high word is empty
    emitter.instruction("b __rt_mixed_from_value");                             // tail-call boxed Mixed construction
}

/// Emits a call sequence to construct a boxed null Mixed cell on aarch64.
/// Sets NULL_TAG (8) as runtime tag, zero payload words, and calls `__rt_mixed_from_value`.
/// Used when a null must be allocated (e.g., unset slot in toArray).
fn emit_boxed_null_call_aarch64(emitter: &mut Emitter) {
    emitter.instruction(&format!("mov x0, #{}", NULL_TAG));                     // runtime tag 8 = null
    emitter.instruction("mov x1, xzr");                                         // null payload low word is empty
    emitter.instruction("mov x2, xzr");                                         // null payload high word is empty
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate boxed null Mixed cell
}

/// Emits all x86_64 `SplFixedArray` runtime helpers via a blank line, section comment,
/// and sequential emission of constructor, Countable, ArrayAccess, and import/export helpers.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl fixed array ---");
    emit_new_x86_64(emitter);
    emit_count_x86_64(emitter);
    emit_set_size_x86_64(emitter);
    emit_offset_exists_x86_64(emitter);
    emit_offset_get_x86_64(emitter);
    emit_offset_set_x86_64(emitter);
    emit_offset_unset_x86_64(emitter);
    emit_to_array_x86_64(emitter);
    emit_from_array_x86_64(emitter);
    emit_unserialize_x86_64(emitter);
    emit_copy_from_array_x86_64(emitter);
}

/// Emits `__rt_spl_fixed_new` on x86_64: constructs an initialized SplFixedArray object.
/// Allocates the object header and fixed-size storage array, stamps the heap kind, stores
/// the class id, zeroes all slots to unset/null, and returns the object pointer in rax.
/// Clobbers: rax, r9, r10, r11, r12. Preserves rbp.
fn emit_new_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_new");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for constructor spills
    emitter.instruction("mov rbp, rsp");                                        // establish constructor frame
    emitter.instruction("sub rsp, 24");                                         // reserve class id, size, and object spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save concrete SplFixedArray class id
    emitter.instruction("cmp rsi, 0");                                          // reject negative sizes
    emitter.instruction("jge __rt_spl_fixed_new_size_ok");                      // keep non-negative sizes
    emitter.instruction("add rsp, 24");                                         // release constructor spills before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_value_error_class_id",
        "_spl_fixed_construct_size_msg",
        SPL_FIXED_CONSTRUCT_SIZE_MSG_LEN,
    );
    emitter.label("__rt_spl_fixed_new_size_ok");
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save requested fixed size
    emitter.instruction(&format!("mov rax, {}", SPL_FIXED_OBJECT_SIZE));        // request fixed-array object payload size
    emitter.instruction("call __rt_heap_alloc");                                // allocate SplFixedArray object payload
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize object heap kind with x86 marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp allocation as an object instance
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload concrete class id
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class id at object header
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save object pointer while allocating storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass requested size as storage capacity
    emitter.instruction("mov rsi, 8");                                          // each slot holds one Mixed pointer
    emitter.instruction("call __rt_array_new");                                 // allocate fixed-array storage
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load storage packed kind word
    emitter.instruction("or r9, 0x700");                                        // mark storage as containing Mixed cells
    emitter.instruction("mov QWORD PTR [rax - 8], r9");                         // persist Mixed value_type tag
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload fixed size
    emitter.instruction("mov QWORD PTR [rax], r10");                            // logical length is fixed size
    emitter.instruction("lea r11, [rax + 24]");                                 // point at first storage slot
    emitter.instruction("xor r12, r12");                                        // initialize slot-zeroing cursor
    emitter.label("__rt_spl_fixed_new_zero_loop");
    emitter.instruction("cmp r12, r10");                                        // have all slots been initialized?
    emitter.instruction("jge __rt_spl_fixed_new_done");                         // finish once every slot is zeroed
    emitter.instruction("mov QWORD PTR [r11 + r12 * 8], 0");                    // initialize slot as unset/null
    emitter.instruction("add r12, 1");                                          // advance zeroing cursor
    emitter.instruction("jmp __rt_spl_fixed_new_zero_loop");                    // continue zeroing storage slots
    emitter.label("__rt_spl_fixed_new_done");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload object pointer
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", SPL_FIXED_STORAGE_OFFSET)); // object.storage = fixed-array storage
    emitter.instruction("mov rax, r11");                                        // return initialized SplFixedArray object
    emitter.instruction("add rsp, 24");                                         // release constructor spills
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return object pointer
}

/// Emits `__rt_spl_fixed_count` on x86_64: returns the logical fixed size of the array.
/// rdi = receiver SplFixedArray object. Returns the fixed size in rax.
fn emit_count_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_count");
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", SPL_FIXED_STORAGE_OFFSET)); // load fixed-array storage
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // return logical fixed size
    emitter.instruction("ret");                                                 // return count/getSize result
}

/// Emits `__rt_spl_fixed_set_size` on x86_64: resizes the fixed array.
/// rdi = receiver SplFixedArray object, rsi = new size (must be >= 0).
/// Grows storage if capacity is insufficient; releases truncated tail slots via `__rt_decref_mixed`;
/// zero-fills newly exposed slots. Throws ValueError if size is negative.
fn emit_set_size_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_set_size");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for resize state
    emitter.instruction("mov rbp, rsp");                                        // establish resize frame
    emitter.instruction("sub rsp, 48");                                         // reserve receiver, size, storage, old size, and cursor spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver
    emitter.instruction("cmp rsi, 0");                                          // reject negative sizes
    emitter.instruction("jge __rt_spl_fixed_set_size_nonnegative");             // keep non-negative sizes
    emitter.instruction("add rsp, 48");                                         // release resize frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_value_error_class_id",
        "_spl_fixed_set_size_msg",
        SPL_FIXED_SET_SIZE_MSG_LEN,
    );
    emitter.label("__rt_spl_fixed_set_size_nonnegative");
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save requested new size
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_FIXED_STORAGE_OFFSET)); // load current storage
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // save current storage
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load old size
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save old size
    emitter.label("__rt_spl_fixed_set_size_grow_check");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload requested size
    emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                         // load capacity
    emitter.instruction("cmp r10, r11");                                        // does storage need more capacity?
    emitter.instruction("jle __rt_spl_fixed_set_size_release_tail");            // skip growth when capacity is enough
    emitter.instruction("mov rdi, r9");                                         // pass current storage to array_grow
    emitter.instruction("call __rt_array_grow");                                // grow fixed-array storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save grown storage
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver
    emitter.instruction(&format!("mov QWORD PTR [r9 + {}], rax", SPL_FIXED_STORAGE_OFFSET)); // publish grown storage on receiver
    emitter.instruction("jmp __rt_spl_fixed_set_size_grow_check");              // grow again if requested size still exceeds capacity
    emitter.label("__rt_spl_fixed_set_size_release_tail");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload requested size
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload old size
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at first storage slot
    emitter.instruction("mov r13, r10");                                        // tail-release cursor starts at new size
    emitter.label("__rt_spl_fixed_set_size_tail_loop");
    emitter.instruction("cmp r13, r11");                                        // have all truncated slots been released?
    emitter.instruction("jge __rt_spl_fixed_set_size_zero_new");                // stop tail release at old size
    emitter.instruction("mov rax, QWORD PTR [r12 + r13 * 8]");                  // load truncated Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // preserve tail-release cursor
    emitter.instruction("call __rt_decref_mixed");                              // release truncated Mixed cell if present
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload storage after release
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload requested size after release
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload old size after release
    emitter.instruction("lea r12, [r9 + 24]");                                  // restore storage slot base
    emitter.instruction("mov r13, QWORD PTR [rbp - 40]");                       // reload tail-release cursor
    emitter.instruction("mov QWORD PTR [r12 + r13 * 8], 0");                    // clear released slot
    emitter.instruction("add r13, 1");                                          // advance tail-release cursor
    emitter.instruction("jmp __rt_spl_fixed_set_size_tail_loop");               // continue releasing truncated slots
    emitter.label("__rt_spl_fixed_set_size_zero_new");
    emitter.instruction("mov r13, QWORD PTR [rbp - 32]");                       // zero-fill starts at old size
    emitter.label("__rt_spl_fixed_set_size_zero_loop");
    emitter.instruction("cmp r13, r10");                                        // have all newly exposed slots been zeroed?
    emitter.instruction("jge __rt_spl_fixed_set_size_done");                    // finish after zero-filling to requested size
    emitter.instruction("mov QWORD PTR [r12 + r13 * 8], 0");                    // initialize grown slot as unset/null
    emitter.instruction("add r13, 1");                                          // advance zero-fill cursor
    emitter.instruction("jmp __rt_spl_fixed_set_size_zero_loop");               // continue zero-filling new slots
    emitter.label("__rt_spl_fixed_set_size_done");
    emitter.instruction("mov QWORD PTR [r9], r10");                             // store new logical fixed size
    emitter.instruction("add rsp, 48");                                         // release resize frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return void
}

/// Emits `__rt_spl_fixed_offset_exists` on x86_64: ArrayAccess `offsetExists`.
/// rdi = receiver, rsi = boxed offset. Returns true (1) if the offset is within range
/// and the slot is neither unset nor explicitly null; otherwise returns false.
/// Throws TypeError if offset is not an integer.
fn emit_offset_exists_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_offset_exists");
    emit_offset_prefix_x86_64(
        emitter,
        "__rt_spl_fixed_offset_exists_type_throw",
        "__rt_spl_fixed_offset_exists_false",
    );
    emitter.instruction("lea r11, [r9 + 24]");                                  // point at first storage slot
    emitter.instruction("mov r12, QWORD PTR [r11 + r10 * 8]");                  // load candidate Mixed cell pointer
    emitter.instruction("test r12, r12");                                       // is the slot unset?
    emitter.instruction("jz __rt_spl_fixed_offset_exists_false");               // unset slots do not exist for isset()
    emitter.instruction("cmp QWORD PTR [r12], 8");                              // explicit null behaves like unset for isset()
    emitter.instruction("setne al");                                            // return true when slot is non-null
    emitter.instruction("movzx rax, al");                                       // widen boolean result
    emitter.instruction("add rsp, 48");                                         // release offset frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return boolean result
    emitter.label("__rt_spl_fixed_offset_exists_false");
    emitter.instruction("xor rax, rax");                                        // invalid/unset offsets return false
    emitter.instruction("add rsp, 48");                                         // release offset frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return false
    emitter.label("__rt_spl_fixed_offset_exists_type_throw");
    emitter.instruction("add rsp, 48");                                         // release offset frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_fixed_offset_type_msg",
        SPL_FIXED_OFFSET_TYPE_MSG_LEN,
    );
}

/// Emits `__rt_spl_fixed_offset_get` on x86_64: ArrayAccess `offsetGet`.
/// rdi = receiver, rsi = boxed offset. Returns the retained Mixed cell at the offset,
/// or a newly boxed null if the slot is unset. Throws TypeError if offset is not an integer;
/// throws OutOfBoundsException if offset is negative or >= fixed size.
fn emit_offset_get_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_offset_get");
    emit_offset_prefix_x86_64(
        emitter,
        "__rt_spl_fixed_offset_get_type_throw",
        "__rt_spl_fixed_offset_get_range_throw",
    );
    emitter.instruction("lea r11, [r9 + 24]");                                  // point at first storage slot
    emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");                  // load selected Mixed cell
    emitter.instruction("test rax, rax");                                       // is the slot unset?
    emitter.instruction("jz __rt_spl_fixed_offset_get_null_loaded");            // unset slots read as null
    emitter.instruction("call __rt_incref");                                    // retain selected Mixed cell for caller
    emitter.instruction("add rsp, 48");                                         // release offset frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return retained Mixed cell
    emitter.label("__rt_spl_fixed_offset_get_null");
    emitter.label("__rt_spl_fixed_offset_get_null_loaded");
    emitter.instruction("add rsp, 48");                                         // release offset frame before null return
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before null return
    emit_tail_boxed_null_x86_64(emitter);
    emitter.label("__rt_spl_fixed_offset_get_type_throw");
    emitter.instruction("add rsp, 48");                                         // release offset frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_fixed_offset_type_msg",
        SPL_FIXED_OFFSET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_fixed_offset_get_range_throw");
    emitter.instruction("add rsp, 48");                                         // release offset frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_out_of_bounds_exception_class_id",
        "_spl_fixed_offset_range_msg",
        SPL_FIXED_OFFSET_RANGE_MSG_LEN,
    );
}

/// Emits `__rt_spl_fixed_offset_set` on x86_64: ArrayAccess `offsetSet`.
/// rdi = receiver, rsi = boxed offset, rdx = owned Mixed value to store.
/// Releases the previous slot value via `__rt_decref_mixed` before overwriting.
/// Throws TypeError if offset is not an integer; throws OutOfBoundsException if out of range.
fn emit_offset_set_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_offset_set");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for offsetSet
    emitter.instruction("mov rbp, rsp");                                        // establish offsetSet frame
    emitter.instruction("sub rsp, 64");                                         // reserve receiver, offset, value, tag, payload, and cursor spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save boxed offset
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save owned Mixed value
    emit_unbox_saved_offset_x86_64(emitter);
    emitter.instruction("mov r12, QWORD PTR [rbp - 32]");                       // reload offset tag
    emitter.instruction(&format!("cmp r12, {}", INT_TAG));                      // fixed-array offsets must be integers
    emitter.instruction("jne __rt_spl_fixed_offset_set_type_throw");            // reject non-integer offsets
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload integer offset
    emitter.instruction("cmp r10, 0");                                          // reject negative offsets
    emitter.instruction("jl __rt_spl_fixed_offset_set_range_throw");            // negative offsets are out of range
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload receiver
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_FIXED_STORAGE_OFFSET)); // load fixed-array storage
    emitter.instruction("mov r11, QWORD PTR [r9]");                             // load fixed size
    emitter.instruction("cmp r10, r11");                                        // compare offset against fixed size
    emitter.instruction("jae __rt_spl_fixed_offset_set_range_throw");           // reject offsets outside fixed range
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at first storage slot
    emitter.instruction("mov rax, QWORD PTR [r12 + r10 * 8]");                  // load previous Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // preserve storage across release
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // preserve offset across release
    emitter.instruction("call __rt_decref_mixed");                              // release previous slot value if present
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload storage after release
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload offset after release
    emitter.instruction("lea r12, [r9 + 24]");                                  // restore storage slot base
    emitter.instruction("mov r13, QWORD PTR [rbp - 24]");                       // reload owned Mixed replacement
    emitter.instruction("mov QWORD PTR [r12 + r10 * 8], r13");                  // store replacement Mixed cell
    emitter.instruction("jmp __rt_spl_fixed_offset_set_done");                  // finish offsetSet
    emitter.label("__rt_spl_fixed_offset_set_type_throw");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload rejected owned Mixed value
    emitter.instruction("call __rt_decref_mixed");                              // release rejected value before throwing
    emitter.instruction("add rsp, 64");                                         // release offsetSet frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_fixed_offset_type_msg",
        SPL_FIXED_OFFSET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_fixed_offset_set_range_throw");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload rejected owned Mixed value
    emitter.instruction("call __rt_decref_mixed");                              // release rejected value
    emitter.instruction("add rsp, 64");                                         // release offsetSet frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_out_of_bounds_exception_class_id",
        "_spl_fixed_offset_range_msg",
        SPL_FIXED_OFFSET_RANGE_MSG_LEN,
    );
    emitter.label("__rt_spl_fixed_offset_set_done");
    emitter.instruction("add rsp, 64");                                         // release offsetSet frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return void
}

/// Emits `__rt_spl_fixed_offset_unset` on x86_64: ArrayAccess `offsetUnset`.
/// rdi = receiver, rsi = boxed offset. Releases the existing Mixed cell at the slot
/// via `__rt_decref_mixed` and marks the slot as unset/null.
/// Throws TypeError if offset is not an integer; throws OutOfBoundsException if out of range.
fn emit_offset_unset_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_offset_unset");
    emit_offset_prefix_x86_64(
        emitter,
        "__rt_spl_fixed_offset_unset_type_throw",
        "__rt_spl_fixed_offset_unset_range_throw",
    );
    emitter.instruction("lea r11, [r9 + 24]");                                  // point at first storage slot
    emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");                  // load existing Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // preserve storage slot base across release
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // preserve offset across release
    emitter.instruction("call __rt_decref_mixed");                              // release existing Mixed cell if present
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload storage slot base
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload offset
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8], 0");                    // mark slot unset/null
    emitter.label("__rt_spl_fixed_offset_unset_done");
    emitter.instruction("add rsp, 48");                                         // release offset frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return void
    emitter.label("__rt_spl_fixed_offset_unset_type_throw");
    emitter.instruction("add rsp, 48");                                         // release offset frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_type_error_class_id",
        "_spl_fixed_offset_type_msg",
        SPL_FIXED_OFFSET_TYPE_MSG_LEN,
    );
    emitter.label("__rt_spl_fixed_offset_unset_range_throw");
    emitter.instruction("add rsp, 48");                                         // release offset frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_out_of_bounds_exception_class_id",
        "_spl_fixed_offset_range_msg",
        SPL_FIXED_OFFSET_RANGE_MSG_LEN,
    );
}

/// Emits the common offset validation prefix for x86_64 offset operations.
/// Saves frame state, unboxes the offset argument, validates it is a non-negative integer
/// within the fixed array range, and branches to `type_label` on TypeError or `range_label`
/// on out-of-range. On success, r9 = storage pointer, r10 = integer offset.
/// Clobbers: rax, rdi, r9, r10, r11, r12. Preserves rbp.
fn emit_offset_prefix_x86_64(emitter: &mut Emitter, type_label: &str, range_label: &str) {
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for offset helper
    emitter.instruction("mov rbp, rsp");                                        // establish common offset frame
    emitter.instruction("sub rsp, 48");                                         // reserve receiver, offset, tag, and payload spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save boxed offset
    emit_unbox_saved_offset_x86_64(emitter);
    emitter.instruction("mov r12, QWORD PTR [rbp - 32]");                       // reload offset tag
    emitter.instruction(&format!("cmp r12, {}", INT_TAG));                      // fixed-array offsets must be integers
    emitter.instruction(&format!("jne {}", type_label));                        // reject non-integer offsets
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload integer offset
    emitter.instruction("cmp r10, 0");                                          // reject negative offsets
    emitter.instruction(&format!("jl {}", range_label));                        // negative offsets are invalid
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload receiver
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_FIXED_STORAGE_OFFSET)); // load fixed-array storage
    emitter.instruction("mov r11, QWORD PTR [r9]");                             // load fixed size
    emitter.instruction("cmp r10, r11");                                        // compare offset against fixed size
    emitter.instruction(&format!("jae {}", range_label));                       // reject offsets outside fixed range
}

/// Emits the x86_64 helper that unboxes the saved boxed offset argument.
/// Loads the boxed offset from [rbp-16], calls `__rt_mixed_unbox` to produce tag (rax) and
/// integer payload candidate (rdi), saves them to [rbp-32] and [rbp-40], then releases
/// the boxed offset via `__rt_decref_mixed`. Result: offset tag at [rbp-32], integer at [rbp-40].
fn emit_unbox_saved_offset_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload boxed offset argument
    emitter.instruction("call __rt_mixed_unbox");                               // unbox offset into tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save unboxed offset tag
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save unboxed integer payload candidate
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload boxed offset argument
    emitter.instruction("call __rt_decref_mixed");                              // release owned boxed offset argument
}

/// Emits `__rt_spl_fixed_to_array` on x86_64: converts the SplFixedArray to a PHP array.
/// Allocates a PHP array of the same logical length, copies each slot's Mixed cell
/// (retaining it for the result array), or boxed null for unset slots. Returns the new array in rax.
fn emit_to_array_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_to_array");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for toArray
    emitter.instruction("mov rbp, rsp");                                        // establish toArray frame
    emitter.instruction("sub rsp, 48");                                         // reserve source, size, result, and cursor spills
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", SPL_FIXED_STORAGE_OFFSET)); // load fixed-array storage
    emitter.instruction("mov QWORD PTR [rbp - 8], r9");                         // save source storage
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load fixed size
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save fixed size
    emitter.instruction("mov rdi, r10");                                        // result array capacity equals fixed size
    emitter.instruction("mov rsi, 8");                                          // result slots hold Mixed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate result PHP array
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load result packed kind word
    emitter.instruction("or r9, 0x700");                                        // mark result as containing Mixed cells
    emitter.instruction("mov QWORD PTR [rax - 8], r9");                         // persist result Mixed value_type tag
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload fixed size
    emitter.instruction("mov QWORD PTR [rax], r10");                            // result logical length equals fixed size
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save result array
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize copy cursor
    emitter.label("__rt_spl_fixed_to_array_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload fixed size
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload copy cursor
    emitter.instruction("cmp r11, r10");                                        // have all slots been copied?
    emitter.instruction("jge __rt_spl_fixed_to_array_done");                    // finish after copying fixed size slots
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload source storage
    emitter.instruction("lea r9, [r9 + 24]");                                   // point at source slot base
    emitter.instruction("mov rax, QWORD PTR [r9 + r11 * 8]");                   // load source Mixed cell pointer
    emitter.instruction("test rax, rax");                                       // is the source slot unset?
    emitter.instruction("jz __rt_spl_fixed_to_array_null");                     // unset slots become boxed null
    emitter.instruction("call __rt_incref");                                    // retain source Mixed cell for result array
    emitter.instruction("jmp __rt_spl_fixed_to_array_store");                   // store retained slot
    emitter.label("__rt_spl_fixed_to_array_null");
    emit_boxed_null_call_x86_64(emitter);
    emitter.label("__rt_spl_fixed_to_array_store");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload result array
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload copy cursor
    emitter.instruction("lea r9, [r9 + 24]");                                   // point at result slot base
    emitter.instruction("mov QWORD PTR [r9 + r11 * 8], rax");                   // store owned Mixed cell into result array
    emitter.instruction("add r11, 1");                                          // advance copy cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save updated copy cursor
    emitter.instruction("jmp __rt_spl_fixed_to_array_loop");                    // continue copying slots
    emitter.label("__rt_spl_fixed_to_array_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return result array pointer
    emitter.instruction("add rsp, 48");                                         // release toArray frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return PHP array
}

/// Emits `__rt_spl_fixed_from_array` on x86_64: constructs a SplFixedArray from a PHP array.
/// rdi = SplFixedArray class id, rsi = source PHP array, rdx = preserveKeys flag.
/// Computes the required size from indexed or hash sources, allocates via `__rt_spl_fixed_new`,
/// then imports values via `__rt_spl_fixed_copy_from_array`. Throws InvalidArgumentException
/// if a hash source with preserveKeys=true contains non-integer or negative keys.
fn emit_from_array_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_from_array");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for fromArray
    emitter.instruction("mov rbp, rsp");                                        // establish fromArray frame
    emitter.instruction("sub rsp, 48");                                         // reserve class id, source array, preserve flag, size, object, and cursor spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save SplFixedArray class id
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save source PHP array
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save preserveKeys flag
    emitter.instruction("mov r10, QWORD PTR [rsi - 8]");                        // load source heap kind metadata
    emitter.instruction("and r10, 0xff");                                       // isolate the low heap kind byte
    emitter.instruction("cmp r10, 3");                                          // is the source an associative array hash?
    emitter.instruction("je __rt_spl_fixed_from_array_hash_size_x86");          // hash sources need key-aware sizing
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // indexed source size equals logical array length
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save constructor size
    emitter.instruction("jmp __rt_spl_fixed_from_array_alloc_x86");             // allocate with the indexed source length
    emitter.label("__rt_spl_fixed_from_array_hash_size_x86");
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load hash entry count for packed import
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // is preserveKeys false?
    emitter.instruction("jne __rt_spl_fixed_from_array_hash_preserve_size_x86"); // preserveKeys=true sizes by maximum numeric key
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // packed hash import size equals entry count
    emitter.instruction("jmp __rt_spl_fixed_from_array_alloc_x86");             // allocate with packed hash size
    emitter.label("__rt_spl_fixed_from_array_hash_preserve_size_x86");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize max numeric key plus one
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize hash iterator cursor
    emitter.label("__rt_spl_fixed_from_array_hash_size_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload source hash for sizing iteration
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload current hash iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next hash entry in insertion order
    emitter.instruction("cmp rax, -1");                                         // did sizing iteration reach the end sentinel?
    emitter.instruction("je __rt_spl_fixed_from_array_alloc_x86");              // allocate once every numeric key has been inspected
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save next hash iterator cursor
    emitter.instruction("cmp rdx, -1");                                         // integer keys use key_len=-1
    emitter.instruction("jne __rt_spl_fixed_from_array_keys_throw_x86");        // preserveKeys rejects string keys
    emitter.instruction("cmp rdi, 0");                                          // negative keys are not fixed-array offsets
    emitter.instruction("jl __rt_spl_fixed_from_array_keys_throw_x86");         // preserveKeys rejects negative keys
    emitter.instruction("add rdi, 1");                                          // candidate size is numeric key + 1
    emitter.instruction("cmp rdi, QWORD PTR [rbp - 32]");                       // is this key beyond the current size?
    emitter.instruction("jle __rt_spl_fixed_from_array_hash_size_loop_x86");    // keep current size when it is already large enough
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save updated maximum size
    emitter.instruction("jmp __rt_spl_fixed_from_array_hash_size_loop_x86");    // continue sizing preserved hash keys
    emitter.label("__rt_spl_fixed_from_array_alloc_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload SplFixedArray class id
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass computed constructor size
    emitter.instruction("call __rt_spl_fixed_new");                             // allocate a fixed array sized like the source
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save new fixed-array object
    emitter.instruction("mov rdi, rax");                                        // pass receiver to import helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass source PHP array to import helper
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // pass preserveKeys flag to import helper
    emitter.instruction("call __rt_spl_fixed_copy_from_array");                 // copy source values into the fixed array
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return populated fixed-array object
    emitter.instruction("add rsp, 48");                                         // release fromArray frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return populated SplFixedArray
    emitter.label("__rt_spl_fixed_from_array_keys_throw_x86");
    emitter.instruction("add rsp, 48");                                         // release fromArray frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emit_throw_exception_x86_64(
        emitter,
        "_spl_invalid_argument_exception_class_id",
        "_spl_fixed_from_array_keys_msg",
        SPL_FIXED_FROM_ARRAY_KEYS_MSG_LEN,
    );
}

/// Emits `__rt_spl_fixed_unserialize` on x86_64: serialization entry point.
/// Sets edx=0 (ignore PHP array keys) and jumps to `__rt_spl_fixed_copy_from_array`.
fn emit_unserialize_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_unserialize");
    emitter.instruction("xor edx, edx");                                        // __unserialize packs source values and ignores PHP array keys
    emitter.instruction("jmp __rt_spl_fixed_copy_from_array");                  // __unserialize reuses the generic array import path
}

/// Emits `__rt_spl_fixed_copy_from_array` on x86_64: populates a SplFixedArray from a PHP array.
/// rdi = receiver SplFixedArray, rsi = source PHP array, rdx = preserveKeys flag.
/// For indexed sources: normalizes slots to boxed Mixed, resizes receiver, copies slot values.
/// For hash sources (preserveKeys=false): imports values in insertion order at packed indices.
/// For hash sources (preserveKeys=true): resizes to max numeric key + 1, copies only integer-keyed
/// entries at their preserved offsets. Releases any overwritten destination cells via `__rt_decref_mixed`.
fn emit_copy_from_array_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_spl_fixed_copy_from_array");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for import
    emitter.instruction("mov rbp, rsp");                                        // establish import frame
    emitter.instruction("sub rsp, 96");                                         // reserve receiver, source, preserve flag, size, storage, cursor, and value spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save receiver
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save source PHP array
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save preserveKeys flag
    emitter.instruction("mov r10, QWORD PTR [rsi - 8]");                        // load source heap kind metadata
    emitter.instruction("and r10, 0xff");                                       // isolate the low heap kind byte
    emitter.instruction("cmp r10, 3");                                          // is the source an associative array hash?
    emitter.instruction("je __rt_spl_fixed_copy_from_hash_x86");                // hash sources need insertion-order import
    emitter.instruction("mov rax, rsi");                                        // pass source array to mixed conversion
    emitter.instruction("mov rsi, QWORD PTR [rax - 8]");                        // load packed array kind and value-type metadata
    emitter.instruction("shr rsi, 8");                                          // move value_type tag down to the low byte
    emitter.instruction("and rsi, 0x7f");                                       // isolate the indexed-array value_type tag without COW metadata
    emitter.instruction("mov rdi, rax");                                        // pass the source indexed array pointer to the converter
    emitter.instruction("call __rt_array_to_mixed");                            // normalize source slots to boxed Mixed cells
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save possibly-converted source array
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // load source logical length
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save source length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload receiver for resize
    emitter.instruction("call __rt_spl_fixed_set_size");                        // resize receiver to exactly the source length
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver after resize
    emitter.instruction(&format!("mov r9, QWORD PTR [r9 + {}]", SPL_FIXED_STORAGE_OFFSET)); // load destination fixed storage
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save destination storage
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize copy cursor
    emitter.label("__rt_spl_fixed_copy_from_array_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload copy cursor
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have all source slots been copied?
    emitter.instruction("jge __rt_spl_fixed_copy_from_array_done");             // finish once every slot is imported
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload destination storage
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at destination slot base
    emitter.instruction("mov rax, QWORD PTR [r12 + r10 * 8]");                  // load existing destination Mixed cell
    emitter.instruction("call __rt_decref_mixed");                              // release any overwritten destination cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload copy cursor after release
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload source array
    emitter.instruction("mov rax, QWORD PTR [r9 + 24 + r10 * 8]");              // load source Mixed cell
    emitter.instruction("call __rt_incref");                                    // retain source cell for fixed-array storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload copy cursor after retain
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload destination storage after retain
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at destination slot base
    emitter.instruction("mov QWORD PTR [r12 + r10 * 8], rax");                  // store retained Mixed cell into fixed-array slot
    emitter.instruction("add r10, 1");                                          // advance copy cursor
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save updated copy cursor
    emitter.instruction("jmp __rt_spl_fixed_copy_from_array_loop");             // continue copying source slots
    emitter.label("__rt_spl_fixed_copy_from_hash_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // should numeric keys be preserved?
    emitter.instruction("jne __rt_spl_fixed_copy_from_hash_preserve_x86");      // preserve numeric keys when requested
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload source hash
    emitter.instruction("mov rsi, QWORD PTR [r9]");                             // packed hash import size equals live entry count
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save destination size
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload receiver for resize
    emitter.instruction("call __rt_spl_fixed_set_size");                        // resize receiver for packed hash import
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver after resize
    emitter.instruction(&format!("mov r9, QWORD PTR [r9 + {}]", SPL_FIXED_STORAGE_OFFSET)); // load destination fixed storage
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save destination storage
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize packed destination index
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // initialize hash iterator cursor
    emitter.label("__rt_spl_fixed_copy_hash_packed_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload source hash for iteration
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload current hash iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next hash entry in insertion order
    emitter.instruction("cmp rax, -1");                                         // did iteration reach the end sentinel?
    emitter.instruction("je __rt_spl_fixed_copy_from_array_done");              // finish after every hash value has been copied
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save next hash iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // save hash value low payload
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // save hash value high payload
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");                        // save hash value runtime tag
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload packed destination index
    emitter.instruction("jmp __rt_spl_fixed_copy_hash_store_x86");              // store this value at the packed destination index
    emitter.label("__rt_spl_fixed_copy_from_hash_preserve_x86");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize max numeric key plus one
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // initialize hash sizing cursor
    emitter.label("__rt_spl_fixed_copy_hash_preserve_size_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload source hash for sizing
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload current hash iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next hash entry for preserved sizing
    emitter.instruction("cmp rax, -1");                                         // did sizing reach the end sentinel?
    emitter.instruction("je __rt_spl_fixed_copy_hash_preserve_resize_x86");     // resize once numeric keys have been inspected
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save next hash iterator cursor
    emitter.instruction("cmp rdx, -1");                                         // integer keys use key_len=-1
    emitter.instruction("jne __rt_spl_fixed_copy_hash_preserve_size_loop_x86"); // string keys do not map to fixed offsets
    emitter.instruction("cmp rdi, 0");                                          // negative keys are not fixed-array offsets
    emitter.instruction("jl __rt_spl_fixed_copy_hash_preserve_size_loop_x86");  // ignore negative keys for now
    emitter.instruction("add rdi, 1");                                          // candidate size is numeric key + 1
    emitter.instruction("cmp rdi, QWORD PTR [rbp - 32]");                       // is this key beyond the current size?
    emitter.instruction("jle __rt_spl_fixed_copy_hash_preserve_size_loop_x86"); // keep current size when it is already large enough
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save updated maximum size
    emitter.instruction("jmp __rt_spl_fixed_copy_hash_preserve_size_loop_x86"); // continue sizing preserved hash keys
    emitter.label("__rt_spl_fixed_copy_hash_preserve_resize_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload preserved destination size
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload receiver for resize
    emitter.instruction("call __rt_spl_fixed_set_size");                        // resize receiver for preserved numeric keys
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload receiver after resize
    emitter.instruction(&format!("mov r9, QWORD PTR [r9 + {}]", SPL_FIXED_STORAGE_OFFSET)); // load destination fixed storage
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save destination storage
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // reset hash iterator cursor for value copy
    emitter.label("__rt_spl_fixed_copy_hash_preserve_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload source hash for preserved import
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload current hash iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next hash entry in insertion order
    emitter.instruction("cmp rax, -1");                                         // did iteration reach the end sentinel?
    emitter.instruction("je __rt_spl_fixed_copy_from_array_done");              // finish after every preserved numeric key has been copied
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save next hash iterator cursor
    emitter.instruction("cmp rdx, -1");                                         // integer keys use key_len=-1
    emitter.instruction("jne __rt_spl_fixed_copy_hash_preserve_loop_x86");      // string keys are skipped by this fixed-offset import path
    emitter.instruction("cmp rdi, 0");                                          // negative keys are not fixed-array offsets
    emitter.instruction("jl __rt_spl_fixed_copy_hash_preserve_loop_x86");       // skip negative keys
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save numeric destination index
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // save hash value low payload
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // save hash value high payload
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");                        // save hash value runtime tag
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload preserved destination index
    emitter.label("__rt_spl_fixed_copy_hash_store_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload destination storage
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at destination slot base
    emitter.instruction("mov rax, QWORD PTR [r12 + r10 * 8]");                  // load existing destination Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 88], r10");                       // preserve destination index across release
    emitter.instruction("call __rt_decref_mixed");                              // release any overwritten destination cell
    emitter.instruction("cmp QWORD PTR [rbp - 80], 7");                         // is the hash value already a boxed Mixed cell?
    emitter.instruction("je __rt_spl_fixed_copy_hash_retain_mixed_x86");        // retain existing boxes instead of nesting them
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // pass runtime tag to mixed boxing helper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // pass value low payload to mixed boxing helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // pass value high payload to mixed boxing helper
    emitter.instruction("call __rt_mixed_from_value");                          // box and retain/persist the hash value for fixed storage
    emitter.instruction("jmp __rt_spl_fixed_copy_hash_store_box_x86");          // skip existing-box retain path
    emitter.label("__rt_spl_fixed_copy_hash_retain_mixed_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load existing boxed Mixed value
    emitter.instruction("call __rt_incref");                                    // retain existing boxed Mixed value for fixed storage
    emitter.label("__rt_spl_fixed_copy_hash_store_box_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // reload destination index after boxing
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload destination storage after boxing
    emitter.instruction("lea r12, [r9 + 24]");                                  // point at destination slot base
    emitter.instruction("mov QWORD PTR [r12 + r10 * 8], rax");                  // store owned Mixed value into fixed-array slot
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // was this a preserveKeys import?
    emitter.instruction("jne __rt_spl_fixed_copy_hash_preserve_loop_x86");      // preserved imports keep source numeric keys
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload packed destination index
    emitter.instruction("add r10, 1");                                          // advance packed destination index
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save updated packed destination index
    emitter.instruction("jmp __rt_spl_fixed_copy_hash_packed_loop_x86");        // continue packed hash import
    emitter.label("__rt_spl_fixed_copy_from_array_done");
    emitter.instruction("add rsp, 96");                                         // release import frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return void
}

/// Emits a tail-call sequence to construct a boxed null Mixed cell on x86_64.
/// Sets NULL_TAG (8) as runtime tag, zero payload registers, and tail-calls `__rt_mixed_from_value`.
/// Used when returning null from a leaf call path (e.g., offsetGet on unset slot).
fn emit_tail_boxed_null_x86_64(emitter: &mut Emitter) {
    emitter.instruction(&format!("mov rax, {}", NULL_TAG));                     // runtime tag 8 = null
    emitter.instruction("xor rdi, rdi");                                        // null payload low word is empty
    emitter.instruction("xor rsi, rsi");                                        // null payload high word is empty
    emitter.instruction("jmp __rt_mixed_from_value");                           // tail-call boxed Mixed construction
}

/// Emits a call sequence to construct a boxed null Mixed cell on x86_64.
/// Sets NULL_TAG (8) as runtime tag, zero payload registers, and calls `__rt_mixed_from_value`.
/// Used when a null must be allocated (e.g., unset slot in toArray).
fn emit_boxed_null_call_x86_64(emitter: &mut Emitter) {
    emitter.instruction(&format!("mov rax, {}", NULL_TAG));                     // runtime tag 8 = null
    emitter.instruction("xor rdi, rdi");                                        // null payload low word is empty
    emitter.instruction("xor rsi, rsi");                                        // null payload high word is empty
    emitter.instruction("call __rt_mixed_from_value");                          // allocate boxed null Mixed cell
}

/// Emits the aarch64 exception-throw sequence for SplFixedArray runtime errors.
/// Allocates a 32-byte Throwable payload, stamps it as heap kind 6 (object), stores the
/// class id from `class_id_symbol`, the static message pointer, the message length,
/// zero exception code, publishes the exception to `_exc_value`, and branches to `__rt_throw_current`.
/// Clobbers: x0, x1, x2, x9.
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

/// Emits the x86_64 exception-throw sequence for SplFixedArray runtime errors.
/// Allocates a 32-byte Throwable payload, stamps it with the x86_64 heap-kind word (HEAP_MAGIC_LO
/// + kind 6), stores the class id via RIP-relative Lea, the static message pointer,
/// the message length, zero exception code, publishes the exception to `_exc_value`,
/// and jumps to `__rt_throw_current`. Preserves rbp, keeps stack 16-byte aligned.
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
