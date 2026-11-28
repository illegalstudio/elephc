//! Purpose:
//! Emits the `__rt_mixed_array_set` runtime helper for writes into boxed Mixed arrays.
//! Mutates JSON-decoded indexed-array and hash payloads reached through Mixed cells.
//!
//! Called from:
//! - `crate::codegen_support::runtime::objects::emit_mixed_array_set()`.
//!
//! Key details:
//! - Normalized int keys use `key_hi = -1`; object appends reserve `key_hi = -2` for null.
//! - The helper consumes the boxed Mixed value pointer when the write succeeds.
//! - A container payload that is null or the in-band null-container sentinel
//!   (`NULL_SENTINEL`, materialized by a missed read forwarded through a ternary merge)
//!   is autovivified as a real PHP array before the keyed write (issues #585/#592).
//! - String, negative, and non-contiguous numeric keys promote indexed payloads to hash storage
//!   via `__rt_mixed_cell_promote_to_hash` (PHP array-key semantics) instead
//!   of dropping the write.
//! - Canonical null and legacy null-container payloads autovivify through the
//!   shared cell helper before any container header is dereferenced.
//! - A string payload (tag 1) takes PHP's string offset write through `__rt_str_offset_set`
//!   and gets the updated string as a new payload; a string key throws `TypeError`.
//! - That write can warn, and a user error handler can then reassign the variable that owns
//!   the cell. The arm retains the cell across the helper and, like php's
//!   `zend_assign_to_string_offset`, abandons the write when its own reference is the last one.
//! - Packed invoker-marker overwrites promote to the shared hash setter to retain caller identity.
//! - Callable payloads are Closure objects, not class-table pointers; rejected writes name them directly.

use elephc_memoir::values::packed_slots::{self, AttachedPackedSlots};
use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::CLOSURE_CLASS_NAME;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

/// Emits the `__rt_mixed_array_set` runtime helper for writing into boxed Mixed arrays.
///
/// Dispatches to the target-specific implementation. On ARM64 the helper receives
/// arguments via registers `x0`–`x3`; on x86_64 it uses the SysV convention (`rdi`, `rsi`,
/// `rdx`, `rcx`). The key tuple encoding matches `emit_normalized_hash_key`: integer keys
/// use `key_hi = -1`. The helper consumes the boxed Mixed value pointer when the write
/// succeeds, autovivifies writable Mixed null cells, and releases the value if the target
/// pointer is absent or its non-null payload type is incompatible.
pub fn emit_mixed_array_set(emitter: &mut Emitter) {
    elephc_memoir::strings::offset_write_guard::emit_unwind(emitter);
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_array_set_x86_64(emitter);
        return;
    }
    emit_mixed_array_set_aarch64(emitter);
}

/// ARM64 implementation of the `__rt_mixed_array_set` runtime helper.
///
/// Input registers:
/// - `x0`: pointer to the target boxed `Mixed` cell (nullable)
/// - `x1`: key low word (array index or normalized hash low word)
/// - `x2`: key high word (integer-key sentinel `0xFFFF_FFFF` or hash high word)
/// - `x3`: pointer to the boxed `Mixed` value being written
///
/// Behavior:
/// - If `x0` is null, the value is released via `__rt_decref_mixed` and the helper returns.
/// - Canonical or legacy container-shaped null payloads autovivify as indexed arrays in place.
/// - Packed overwrites and appends mutate slots directly; all other keys first promote to hash
///   storage, while existing associative arrays call `__rt_hash_set`.
/// - Array capacity is grown via `__rt_array_grow` if the target index exceeds current capacity.
/// - Overwriting an existing slot releases the previous `Mixed` cell.
/// - Extending the logical length zero-fills all gap slots before the written position.
/// - The helper frame is 80 bytes; callee-saved registers `x29`/`x30` are preserved.
fn emit_mixed_array_set_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_set ---");
    emitter.label_global("__rt_mixed_array_set");

    // Inputs: x0 = mixed_ptr, x1 = key_lo, x2 = key_hi, x3 = value_mixed_ptr.
    emitter.instruction("sub sp, sp, #80");                                     // reserve frame for inputs, array state, and saved frame registers
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the target Mixed cell
    emitter.instruction("str x1, [sp, #8]");                                    // save key_lo for indexed-array addressing
    emitter.instruction("str x2, [sp, #16]");                                   // save key_hi so integer keys can be distinguished
    emitter.instruction("str x3, [sp, #24]");                                   // save the boxed value consumed by the write
    emitter.instruction("bl __rt_mixed_deref");                                 // follow a reference receiver to its writable current value
    emitter.instruction("str x0, [sp, #0]");                                    // save the concrete receiver for payload publication

    emitter.instruction("cbz x0, __rt_mixed_array_set_drop");                   // non-existent Mixed targets cannot be mutated
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed payload tag
    emitter.instruction("cmp x9, #4");                                          // is the Mixed payload an indexed array?
    emitter.instruction("b.eq __rt_mixed_array_set_indexed");                   // route indexed arrays to slot-based mutation
    emitter.instruction("cmp x9, #5");                                          // is the Mixed payload an associative array?
    emitter.instruction("b.eq __rt_mixed_array_set_assoc");                     // route hash arrays to key-based mutation
    emitter.instruction("cmp x9, #6");                                          // is the Mixed payload an object?
    emitter.instruction("b.eq __rt_mixed_array_set_object");                    // route runtime-managed ArrayAccess objects to offsetSet
    emitter.instruction("cmp x9, #10");                                         // callable payloads are PHP Closure objects
    emitter.instruction("b.eq __rt_mixed_array_set_closure_not_indexable");     // reject the write without interpreting a callable as a class-table object
    emitter.instruction("cmp x9, #8");                                          // is the target a canonical Mixed null?
    emitter.instruction("b.eq __rt_mixed_array_set_autovivify");                // PHP keyed writes autovivify null into an array
    emitter.instruction("cmp x9, #1");                                          // is the Mixed payload a string?
    emitter.instruction("b.eq __rt_mixed_array_set_string");                    // a string takes PHP's string offset write
    emitter.instruction("b __rt_mixed_array_set_drop");                         // non-array Mixed payloads cannot be mutated here
    emitter.label("__rt_mixed_array_set_indexed");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the indexed-array pointer from the Mixed payload
    emit_branch_if_null_container(emitter, "x10", "x9", "__rt_mixed_array_set_autovivify");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload key_hi
    emitter.instruction("cmn x11, #1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("b.ne __rt_mixed_array_set_promote");                   // string keys promote the indexed payload to hash storage (PHP semantics)
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the requested integer index
    emitter.instruction("cmp x9, #0");                                          // reject negative indexes before touching storage
    emitter.instruction("b.lt __rt_mixed_array_set_promote");                   // negative integer keys require associative PHP-array storage
    elephc_memoir::values::packed_slots::emit_load_length(emitter, "x12", "x10"); // inspect the packed array's current logical length
    emitter.instruction("cmp x9, x12");                                         // only overwrites and contiguous appends stay packed
    emitter.instruction("b.hi __rt_mixed_array_set_promote");                   // preserve absent positive keys by promoting sparse writes
    elephc_memoir::heap_kind::emit_load_metadata(emitter, "x12", "x10");
    elephc_memoir::values::array_header::emit_extract_value_tag_aarch64(emitter, "x1", "x12");
    emitter.instruction("mov x0, x10");                                         // pass the indexed array to the Mixed conversion helper
    emitter.instruction("bl __rt_array_to_mixed");                              // split shared arrays and normalize slots before writing Mixed values
    emitter.instruction("str x0, [sp, #32]");                                   // save the unique array pointer
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the target Mixed cell after the helper call
    elephc_memoir::layout::emit_store_mixed_low(emitter, "x0", "x10");
    emitter.instruction("ldr x11, [x0]");                                       // load the post-conversion logical length
    emitter.instruction("str x11, [sp, #48]");                                  // preserve the original length for overwrite and extension checks
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the requested integer index
    emitter.instruction("str x9, [sp, #40]");                                   // preserve the target index across growth and release helpers

    AttachedPackedSlots { array: 32, index: 40, owner: 0 }
        .emit_grow_to_index(emitter, "__rt_mixed_array_set");
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the final array pointer
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the target index
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the original logical length
    emitter.instruction("cmp x9, x11");                                         // does this write overwrite an existing slot?
    emitter.instruction("b.hs __rt_mixed_array_set_skip_release");              // writes past the old end do not replace an existing Mixed cell
    elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(emitter, "x12", "x10");
    emitter.instruction("ldr x0, [x12, x9, lsl #3]");                           // load the previous boxed Mixed pointer from the slot
    emitter.instruction("cbz x0, __rt_mixed_array_set_release_slot");           // absent slots contain no persistent reference
    emitter.instruction("ldr x9, [x0]");                                        // inspect the stored boxed tag
    emitter.instruction("cmp x9, #11");                                         // invoker-backed variadic slots must retain caller identity
    emitter.instruction("b.eq __rt_mixed_array_set_promote");                   // share hash_set's ownership-safe invoker writeback
    emitter.instruction("cmp x9, #7");                                          // recognize a nested Mixed wrapper
    emitter.instruction("b.ne __rt_mixed_array_set_release_slot");              // ordinary values use normal slot replacement
    emitter.instruction("ldr x9, [x0, #16]");                                   // inspect the persistent reference marker
    emitter.instruction("cmp x9, #1");                                          // recognize shared PHP reference storage
    emitter.instruction("b.eq __rt_mixed_array_set_reference");                 // write through the existing reference identity
    emitter.label("__rt_mixed_array_set_release_slot");
    elephc_memoir::values::packed_slots::emit_publish_mixed_overwrite_aarch64(emitter);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_throw_current");
    abi::emit_jump(emitter, "__rt_mixed_array_set_done");
    emitter.label("__rt_mixed_array_set_skip_release");

    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the array pointer after the release helper
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the target index after the release helper
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the new boxed Mixed value
    elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(emitter, "x12", "x10");
    emitter.instruction("str x0, [x12, x9, lsl #3]");                           // store the new boxed Mixed pointer into the target slot

    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the original logical length for extension checks
    emitter.instruction("cmp x9, x11");                                         // did the write extend beyond the old logical length?
    emitter.instruction("b.lo __rt_mixed_array_set_done");                      // overwrites leave the logical length unchanged
    packed_slots::emit_publish_mixed_extension(emitter, "__rt_mixed_array_set", false);
    emitter.instruction("b __rt_mixed_array_set_done");                         // finish after extending the array

    emitter.label("__rt_mixed_array_set_autovivify");
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the existing null-shaped Mixed cell
    emitter.instruction("bl __rt_mixed_cell_autovivify_array");                 // install a fresh empty indexed array in the cell
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the now-indexed receiver cell
    emitter.instruction("b __rt_mixed_array_set_indexed");                      // apply the original key/value through the normal setter

    emitter.label("__rt_mixed_array_set_promote");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the target Mixed cell for the promotion helper
    emitter.instruction("bl __rt_mixed_cell_promote_to_hash");                  // convert the indexed payload to hash storage in the cell
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the target Mixed cell (now tag 5)
    emitter.instruction("b __rt_mixed_array_set_assoc");                        // finish the write through the associative path

    emitter.label("__rt_mixed_array_set_assoc");
    emitter.instruction("ldr x0, [x0, #8]");                                    // pass the associative payload to its COW boundary
    emitter.instruction("bl __rt_hash_ensure_unique");                          // detach shared hash storage before examining reference slots
    emitter.instruction("ldr x9, [sp, #0]");                                    // recover the concrete owning cell
    elephc_memoir::layout::emit_store_mixed_low(emitter, "x0", "x9");
    emitter.instruction("mov x0, x9");                                          // restore the owning cell for associative dispatch
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the associative-array hash pointer from the Mixed payload
    emit_branch_if_null_container(emitter, "x10", "x9", "__rt_mixed_array_set_autovivify");
    emitter.instruction("mov x0, x10");                                         // look up the existing hash slot before replacement
    emitter.instruction("ldr x1, [sp, #8]");                                    // pass the normalized key low word
    emitter.instruction("ldr x2, [sp, #16]");                                   // pass the normalized key high word
    emitter.instruction("bl __rt_hash_get");                                    // read a borrowed entry and its storage tag
    emitter.instruction("cbz x0, __rt_mixed_array_set_assoc_store");            // new keys use ordinary insertion
    emitter.instruction("cmp x3, #7");                                          // only boxed entries can contain a reference
    emitter.instruction("b.ne __rt_mixed_array_set_assoc_store");               // typed entries use ordinary replacement
    emitter.instruction("mov x0, x1");                                          // inspect the borrowed boxed entry
    emitter.instruction("cbz x0, __rt_mixed_array_set_assoc_store");            // absent slots contain no persistent reference
    emitter.instruction("ldr x9, [x0]");                                        // inspect the stored boxed tag
    emitter.instruction("cmp x9, #7");                                          // recognize a nested Mixed wrapper
    emitter.instruction("b.ne __rt_mixed_array_set_assoc_store");               // ordinary values use normal slot replacement
    emitter.instruction("ldr x9, [x0, #16]");                                   // inspect the persistent reference marker
    emitter.instruction("cmp x9, #1");                                          // recognize shared PHP reference storage
    emitter.instruction("b.eq __rt_mixed_array_set_reference");                 // write through the existing reference identity
    emitter.label("__rt_mixed_array_set_assoc_store");
    emitter.instruction("ldr x10, [sp, #0]");                                   // recover the concrete owning cell
    emitter.instruction("ldr x10, [x10, #8]");                                  // reload its hash payload after lookup
    emitter.instruction("mov x0, x10");                                         // pass the current hash table to the hash-set helper
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the normalized key low word
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the normalized key high word
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the boxed Mixed value pointer
    emitter.instruction("mov x4, xzr");                                         // boxed Mixed hash values only use the low payload word
    emitter.instruction("mov x5, #7");                                          // runtime value tag 7 = boxed Mixed
    emitter.instruction("bl __rt_hash_set");                                    // insert or update the hash entry, preserving PHP key semantics
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the owning Mixed cell after hash mutation
    elephc_memoir::layout::emit_store_mixed_low(emitter, "x0", "x10");
    emitter.instruction("b __rt_mixed_array_set_done");                         // finish after mutating the associative array

    emitter.label("__rt_mixed_array_set_object");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load object payload from the Mixed cell
    emit_branch_if_null_container(emitter, "x10", "x9", "__rt_mixed_array_set_autovivify");
    emitter.instruction("str x10, [sp, #32]");                                  // save object receiver while boxing the offset
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "x11", "x10");
    abi::emit_symbol_address(emitter, "x12", "_spl_fixed_array_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // load SplFixedArray class id
    emitter.instruction("cmp x11, x12");                                        // is this a SplFixedArray object?
    emitter.instruction("b.eq __rt_mixed_array_set_spl_fixed");                 // dispatch to SplFixedArray::offsetSet
    abi::emit_symbol_address(emitter, "x12", "_spl_dll_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // load SplDoublyLinkedList class id
    emitter.instruction("cmp x11, x12");                                        // is this a SplDoublyLinkedList object?
    emitter.instruction("b.eq __rt_mixed_array_set_spl_dll");                   // dispatch to the shared list offsetSet helper
    abi::emit_symbol_address(emitter, "x12", "_spl_stack_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // load SplStack class id
    emitter.instruction("cmp x11, x12");                                        // is this a SplStack object?
    emitter.instruction("b.eq __rt_mixed_array_set_spl_dll");                   // SplStack shares list storage
    abi::emit_symbol_address(emitter, "x12", "_spl_queue_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // load SplQueue class id
    emitter.instruction("cmp x11, x12");                                        // is this a SplQueue object?
    emitter.instruction("b.eq __rt_mixed_array_set_spl_dll");                   // SplQueue shares list storage
    emitter.instruction("b __rt_mixed_array_set_php_offset_set");               // any other class may still be a PHP ArrayAccess
    emitter.label("__rt_mixed_array_set_spl_fixed");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload normalized key high word
    emitter.instruction("cmn x11, #2");                                         // does the internal append tuple require a null offset?
    emitter.instruction("b.eq __rt_mixed_array_set_spl_fixed_null_key");        // box append offsets as PHP null
    emitter.instruction("cmn x11, #1");                                         // restore the integer-key comparison
    emitter.instruction("b.eq __rt_mixed_array_set_spl_fixed_int_key");         // integer keys box as Mixed int
    emitter.instruction("mov x0, #1");                                          // tag = string for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key string pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // key string length
    emitter.instruction("b __rt_mixed_array_set_spl_fixed_box_key");            // share offsetSet after key boxing
    emitter.label("__rt_mixed_array_set_spl_fixed_int_key");
    emitter.instruction("mov x0, #0");                                          // tag = int for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key integer payload
    emitter.instruction("mov x2, #0");                                          // integer keys have no high payload
    emitter.instruction("b __rt_mixed_array_set_spl_fixed_box_key");            // integer offsets use the common boxing call
    emitter.label("__rt_mixed_array_set_spl_fixed_null_key");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 represents PHP null
    emitter.instruction("mov x1, #0");                                          // null has no low payload
    emitter.instruction("mov x2, #0");                                          // null has no high payload
    emitter.label("__rt_mixed_array_set_spl_fixed_box_key");
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the boxed ArrayAccess offset
    emitter.instruction("mov x1, x0");                                          // pass boxed offset as argument 1
    emitter.instruction("ldr x0, [sp, #32]");                                   // pass unboxed SplFixedArray receiver as argument 0
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass owned Mixed value as argument 2
    emitter.instruction("bl __rt_spl_fixed_offset_set");                        // write through SplFixedArray::offsetSet
    emitter.instruction("b __rt_mixed_array_set_done");                         // value ownership was consumed by offsetSet
    emitter.label("__rt_mixed_array_set_spl_dll");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload normalized key high word
    emitter.instruction("cmn x11, #2");                                         // does the internal append tuple require a null offset?
    emitter.instruction("b.eq __rt_mixed_array_set_spl_dll_null_key");          // box append offsets as PHP null
    emitter.instruction("cmn x11, #1");                                         // restore the integer-key comparison
    emitter.instruction("b.eq __rt_mixed_array_set_spl_dll_int_key");           // integer keys box as Mixed int
    emitter.instruction("mov x0, #1");                                          // tag = string for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key string pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // key string length
    emitter.instruction("b __rt_mixed_array_set_spl_dll_box_key");              // share offsetSet after key boxing
    emitter.label("__rt_mixed_array_set_spl_dll_int_key");
    emitter.instruction("mov x0, #0");                                          // tag = int for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key integer payload
    emitter.instruction("mov x2, #0");                                          // integer keys have no high payload
    emitter.instruction("b __rt_mixed_array_set_spl_dll_box_key");              // integer offsets use the common boxing call
    emitter.label("__rt_mixed_array_set_spl_dll_null_key");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 represents PHP null
    emitter.instruction("mov x1, #0");                                          // null has no low payload
    emitter.instruction("mov x2, #0");                                          // null has no high payload
    emitter.label("__rt_mixed_array_set_spl_dll_box_key");
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the boxed ArrayAccess offset
    emitter.instruction("mov x1, x0");                                          // pass boxed offset as argument 1
    emitter.instruction("ldr x0, [sp, #32]");                                   // pass unboxed SPL list receiver as argument 0
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass owned Mixed value as argument 2
    emitter.instruction("bl __rt_spl_dll_offset_set");                          // write through the shared SPL list offsetSet helper
    emitter.instruction("b __rt_mixed_array_set_done");                         // value ownership was consumed by offsetSet

    // Classes above are the runtime's OWN containers, recognised by class id. Everything else is a
    // PHP class, and its `ArrayAccess::offsetSet` is reachable only through the dense method table
    // — without this the write was DROPPED in silence: `$m["b"] = 9` on an `ArrayObject` held in a
    // `mixed` slot left the object untouched and said nothing.
    emitter.label("__rt_mixed_array_set_php_offset_set");
    emitter.instruction("tbnz x11, #63, __rt_mixed_array_set_not_indexable");   // synthetic negative ids cannot index metadata
    abi::emit_load_symbol_to_reg(emitter, "x12", "_class_iface_method_count", 0);
    emitter.instruction("cmp x11, x12");                                        // is the id within the dense table?
    emitter.instruction("b.hs __rt_mixed_array_set_not_indexable");             // out-of-range ids have no entry
    abi::emit_symbol_address(emitter, "x12", "_class_offsetset_ptrs");
    emitter.instruction("ldr x12, [x12, x11, lsl #3]");                         // resolve the concrete or inherited offsetSet
    emitter.instruction("cbz x12, __rt_mixed_array_set_not_indexable");         // 0 means the class is not ArrayAccess at all
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload normalized key high word
    emitter.instruction("cmn x11, #2");                                         // does the internal append tuple require a null offset?
    emitter.instruction("b.eq __rt_mixed_array_set_php_null_key");              // box append offsets as PHP null
    emitter.instruction("cmn x11, #1");                                         // restore the integer-key comparison
    emitter.instruction("b.eq __rt_mixed_array_set_php_int_key");               // integer keys box as Mixed int
    emitter.instruction("mov x0, #1");                                          // tag = string for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key string pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // key string length
    emitter.instruction("b __rt_mixed_array_set_php_box_key");                  // share offsetSet after key boxing
    emitter.label("__rt_mixed_array_set_php_int_key");
    emitter.instruction("mov x0, #0");                                          // tag = int for mixed_from_value
    emitter.instruction("ldr x1, [sp, #8]");                                    // key integer payload
    emitter.instruction("mov x2, #0");                                          // integer keys have no high payload
    emitter.instruction("b __rt_mixed_array_set_php_box_key");                  // integer offsets use the common boxing call
    emitter.label("__rt_mixed_array_set_php_null_key");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 represents PHP null
    emitter.instruction("mov x1, #0");                                          // null has no low payload
    emitter.instruction("mov x2, #0");                                          // null has no high payload
    emitter.label("__rt_mixed_array_set_php_box_key");
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the boxed ArrayAccess offset
    emitter.instruction("str x0, [sp, #8]");                                    // stash the box; key_lo has served its purpose
    // -- publish callback arguments until offsetSet returns or throws --
    emitter.instruction("add x11, sp, #24");                                    // address the consumed value owner
    abi::emit_push_call_operand_owner(emitter, "x11", false);
    emitter.instruction("add x11, sp, #56");                                    // address the boxed key above the first 48-byte record
    abi::emit_push_call_operand_owner(emitter, "x11", false);
    emitter.instruction("ldr x1, [sp, #104]");                                  // borrow the boxed key above two cleanup records
    emitter.instruction("ldr x0, [sp, #128]");                                  // pass unboxed receiver above both records
    emitter.instruction("ldr x2, [sp, #120]");                                  // borrow the consumed value above both records
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "x11", "x0");
    abi::emit_symbol_address(emitter, "x12", "_class_offsetset_ptrs");
    emitter.instruction("ldr x12, [x12, x11, lsl #3]");                         // re-resolve offsetSet for the same class
    emitter.instruction("blr x12");                                             // write through PHP's ArrayAccess::offsetSet
    // A PHP method BORROWS both arguments, where the SPL helpers CONSUME the value. This frame
    // therefore releases the offset it boxed and the value its own contract says it consumed.
    abi::emit_pop_call_operand_owner(emitter);
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the key above the remaining value-owner record
    emitter.instruction("bl __rt_decref_mixed");                                // release the scalar key while the value is still protected
    abi::emit_pop_call_operand_owner(emitter);
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed value
    emitter.instruction("bl __rt_decref_mixed");                                // release it too
    emitter.instruction("b __rt_mixed_array_set_done");                         // both releases done: leave through the shared epilogue

    // PHP refuses to index an object that is not ArrayAccess, on writes exactly as on reads.
    emitter.label("__rt_mixed_array_set_not_indexable");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed value
    emitter.instruction("bl __rt_decref_mixed");                                // release it before leaving through the Error
    emitter.instruction("ldr x0, [sp, #32]");                                   // pass the receiver to name its class
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame before the tail-call
    emitter.instruction("b __rt_throw_object_not_array");                       // never returns

    emit_string_payload_set_aarch64(emitter);
    // -- reject a Closure after retiring the consumed write value --
    // A callable descriptor has no class id to look up, but PHP still names it as Closure.
    emitter.label("__rt_mixed_array_set_closure_not_indexable");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the owned boxed value consumed by the rejected write
    emitter.instruction("bl __rt_decref_mixed");                                // retire the unused value before constructing its Error
    abi::emit_symbol_address(emitter, "x1", "_closure_class_name");
    abi::emit_load_int_immediate(emitter, "x2", CLOSURE_CLASS_NAME.len() as i64);
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame before the named-error tail call
    emitter.instruction("add sp, sp, #80");                                     // release the setter frame before unwinding
    emitter.instruction("b __rt_throw_object_not_array_name");                  // throw a catchable Error using the literal Closure name

    emitter.label("__rt_mixed_array_set_reference");
    emitter.instruction("ldr x1, [sp, #24]");                                   // borrow the incoming value for reference assignment
    emitter.instruction("bl __rt_reference_replace");                           // publish an independent value in the existing shared wrapper
    emitter.instruction("bl __rt_decref_any");                                  // release the replaced child after publication
    emitter.label("__rt_mixed_array_set_drop");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the unused boxed value
    emitter.instruction("bl __rt_decref_mixed");                                // release the boxed value when the write cannot be applied
    emitter.label("__rt_mixed_array_set_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to generated code
}

/// x86_64 implementation of the `__rt_mixed_array_set` runtime helper.
///
/// Input registers (SysV ABI):
/// - `rdi`: pointer to the target boxed `Mixed` cell (nullable)
/// - `rsi`: key low word (array index or normalized hash low word)
/// - `rdx`: key high word (integer-key sentinel `-1` or hash high word)
/// - `rcx`: pointer to the boxed `Mixed` value being written
///
/// Behavior mirrors the ARM64 version with x86_64-specific register and instruction encoding.
/// The helper frame is 64 bytes; callee-saved register `rbp` is preserved.
fn emit_mixed_array_set_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_set ---");
    emitter.label_global("__rt_mixed_array_set");

    // Inputs (SysV): rdi = mixed_ptr, rsi = key_lo, rdx = key_hi, rcx = value_mixed_ptr.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve slots for inputs, array state, and indexes
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the target Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save key_lo for indexed-array addressing
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save key_hi so integer keys can be distinguished
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the boxed value consumed by the write
    emitter.instruction("mov rax, rdi");                                        // pass the possibly referenced receiver
    emitter.instruction("call __rt_mixed_deref");                               // follow references to the writable current value
    emitter.instruction("mov rdi, rax");                                        // restore the concrete receiver argument
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the concrete receiver for payload publication

    emitter.instruction("test rdi, rdi");                                       // non-existent Mixed targets cannot be mutated
    emitter.instruction("je __rt_mixed_array_set_drop");                        // drop the value when the target is null
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed payload tag
    emitter.instruction("cmp r10, 4");                                          // is the Mixed payload an indexed array?
    emitter.instruction("je __rt_mixed_array_set_indexed");                     // route indexed arrays to slot-based mutation
    emitter.instruction("cmp r10, 5");                                          // is the Mixed payload an associative array?
    emitter.instruction("je __rt_mixed_array_set_assoc");                       // route hash arrays to key-based mutation
    emitter.instruction("cmp r10, 6");                                          // is the Mixed payload an object?
    emitter.instruction("je __rt_mixed_array_set_object");                      // route runtime-managed ArrayAccess objects to offsetSet
    emitter.instruction("cmp r10, 10");                                         // callable payloads are PHP Closure objects
    emitter.instruction("je __rt_mixed_array_set_closure_not_indexable");       // reject the write without treating its descriptor as a class-table object
    emitter.instruction("cmp r10, 8");                                          // is the target a canonical Mixed null?
    emitter.instruction("je __rt_mixed_array_set_autovivify");                  // PHP keyed writes autovivify null into an array
    emitter.instruction("cmp r10, 1");                                          // is the Mixed payload a string?
    emitter.instruction("je __rt_mixed_array_set_string");                      // a string takes PHP's string offset write
    emitter.instruction("jmp __rt_mixed_array_set_drop");                       // non-array Mixed payloads cannot be mutated here
    emitter.label("__rt_mixed_array_set_indexed");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the indexed-array pointer from the Mixed payload
    emit_branch_if_null_container(emitter, "r10", "r11", "__rt_mixed_array_set_autovivify");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload key_hi
    emitter.instruction("cmp r11, -1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("jne __rt_mixed_array_set_promote");                    // string keys promote the indexed payload to hash storage (PHP semantics)
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the requested integer index
    emitter.instruction("cmp r9, 0");                                           // reject negative indexes before touching storage
    emitter.instruction("jl __rt_mixed_array_set_promote");                     // negative integer keys require associative PHP-array storage
    emitter.instruction("cmp r9, QWORD PTR [r10]");                             // compare the requested key with the packed logical length
    emitter.instruction("ja __rt_mixed_array_set_promote");                     // sparse writes require hashes so gaps remain absent
    elephc_memoir::heap_kind::emit_load_metadata(emitter, "r8", "r10");
    elephc_memoir::values::array_header::emit_extract_value_tag_x86_64(emitter, "r8", "r8");
    emitter.instruction("mov rsi, r8");                                         // pass the source value_type tag to the Mixed conversion helper
    emitter.instruction("mov rdi, r10");                                        // pass the indexed array to the Mixed conversion helper
    emitter.instruction("call __rt_array_to_mixed");                            // split shared arrays and normalize slots before writing Mixed values
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the unique array pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the target Mixed cell after the helper call
    elephc_memoir::layout::emit_store_mixed_low(emitter, "rax", "r10");
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // load the post-conversion logical length
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the original length for overwrite and extension checks
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the requested integer index
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // preserve the target index across growth and release helpers

    AttachedPackedSlots { array: 40, index: 48, owner: 8 }
        .emit_grow_to_index(emitter, "__rt_mixed_array_set");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the final array pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the target index
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the original logical length
    emitter.instruction("cmp r9, r11");                                         // does this write overwrite an existing slot?
    emitter.instruction("jae __rt_mixed_array_set_skip_release");               // writes past the old end do not replace an existing Mixed cell
    elephc_memoir::values::packed_slots::emit_load_indexed_word(emitter, "r10", "r9", "rax", "r11");
    emitter.instruction("test rax, rax");                                       // recognize an absent previous slot
    emitter.instruction("jz __rt_mixed_array_set_release_slot");                // absent slots contain no persistent reference
    emitter.instruction("cmp QWORD PTR [rax], 11");                             // invoker-backed variadic slots must retain caller identity
    emitter.instruction("je __rt_mixed_array_set_promote");                     // share hash_set's ownership-safe invoker writeback
    emitter.instruction("cmp QWORD PTR [rax], 7");                              // recognize a nested Mixed wrapper
    emitter.instruction("jne __rt_mixed_array_set_release_slot");               // ordinary values use normal slot replacement
    emitter.instruction("cmp QWORD PTR [rax + 16], 1");                         // inspect the persistent reference marker
    emitter.instruction("je __rt_mixed_array_set_reference");                   // write through the existing reference identity
    emitter.label("__rt_mixed_array_set_release_slot");
    elephc_memoir::values::packed_slots::emit_publish_mixed_overwrite_x86_64(emitter);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_throw_current");
    abi::emit_jump(emitter, "__rt_mixed_array_set_done");
    emitter.label("__rt_mixed_array_set_skip_release");

    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the array pointer after the release helper
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the target index after the release helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the new boxed Mixed value
    elephc_memoir::values::packed_slots::emit_store_indexed_word(emitter, "r10", "r9", "rax", "r11");

    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the original logical length for extension checks
    emitter.instruction("cmp r9, r11");                                         // did the write extend beyond the old logical length?
    emitter.instruction("jb __rt_mixed_array_set_done");                        // overwrites leave the logical length unchanged
    packed_slots::emit_publish_mixed_extension(emitter, "__rt_mixed_array_set", false);
    emitter.instruction("jmp __rt_mixed_array_set_done");                       // finish after extending the array

    emitter.label("__rt_mixed_array_set_autovivify");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the existing null-shaped Mixed cell
    emitter.instruction("call __rt_mixed_cell_autovivify_array");               // install a fresh empty indexed array in the cell
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the now-indexed receiver cell
    emitter.instruction("jmp __rt_mixed_array_set_indexed");                    // apply the original key/value through the normal setter

    emitter.label("__rt_mixed_array_set_promote");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the target Mixed cell for the promotion helper
    emitter.instruction("call __rt_mixed_cell_promote_to_hash");                // convert the indexed payload to hash storage in the cell
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the target Mixed cell (now tag 5)
    emitter.instruction("jmp __rt_mixed_array_set_assoc");                      // finish the write through the associative path

    emitter.label("__rt_mixed_array_set_assoc");
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // pass the associative payload to its COW boundary
    emitter.instruction("call __rt_hash_ensure_unique");                        // detach shared hash storage before examining reference slots
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // recover the concrete owning cell
    elephc_memoir::layout::emit_store_mixed_low(emitter, "rax", "rdi");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the associative-array hash pointer from the Mixed payload
    emit_branch_if_null_container(emitter, "r10", "r11", "__rt_mixed_array_set_autovivify");
    emitter.instruction("mov rdi, r10");                                        // look up the existing hash slot before replacement
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the normalized key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // pass the normalized key high word
    emitter.instruction("call __rt_hash_get");                                  // read a borrowed entry and its storage tag
    emitter.instruction("test rax, rax");                                       // check whether this key already exists
    emitter.instruction("jz __rt_mixed_array_set_assoc_store");                 // new keys use ordinary insertion
    emitter.instruction("cmp rcx, 7");                                          // only boxed entries can contain a reference
    emitter.instruction("jne __rt_mixed_array_set_assoc_store");                // typed entries use ordinary replacement
    emitter.instruction("mov rax, rdi");                                        // inspect the borrowed boxed entry
    emitter.instruction("test rax, rax");                                       // recognize an absent previous slot
    emitter.instruction("jz __rt_mixed_array_set_assoc_store");                 // absent slots contain no persistent reference
    emitter.instruction("cmp QWORD PTR [rax], 7");                              // recognize a nested Mixed wrapper
    emitter.instruction("jne __rt_mixed_array_set_assoc_store");                // ordinary values use normal slot replacement
    emitter.instruction("cmp QWORD PTR [rax + 16], 1");                         // inspect the persistent reference marker
    emitter.instruction("je __rt_mixed_array_set_reference");                   // write through the existing reference identity
    emitter.label("__rt_mixed_array_set_assoc_store");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // recover the concrete owning cell
    emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                        // reload its hash payload after lookup
    emitter.instruction("mov rdi, r10");                                        // pass the current hash table to the hash-set helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the normalized key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the normalized key high word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the boxed Mixed value pointer
    emitter.instruction("xor r8, r8");                                          // boxed Mixed hash values only use the low payload word
    emitter.instruction("mov r9, 7");                                           // runtime value tag 7 = boxed Mixed
    emitter.instruction("call __rt_hash_set");                                  // insert or update the hash entry, preserving PHP key semantics
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the owning Mixed cell after hash mutation
    elephc_memoir::layout::emit_store_mixed_low(emitter, "rax", "r10");
    emitter.instruction("jmp __rt_mixed_array_set_done");                       // finish after mutating the associative array

    emitter.label("__rt_mixed_array_set_object");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load object payload from the Mixed cell
    emit_branch_if_null_container(emitter, "r10", "r11", "__rt_mixed_array_set_autovivify");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save object receiver while boxing the offset
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "r11", "r10");
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_fixed_array_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is this a SplFixedArray object?
    emitter.instruction("je __rt_mixed_array_set_spl_fixed");                   // dispatch to SplFixedArray::offsetSet
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_dll_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is this a SplDoublyLinkedList object?
    emitter.instruction("je __rt_mixed_array_set_spl_dll");                     // dispatch to the shared list offsetSet helper
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_stack_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is this a SplStack object?
    emitter.instruction("je __rt_mixed_array_set_spl_dll");                     // SplStack shares list storage
    abi::emit_load_symbol_to_reg(emitter, "r12", "_spl_queue_class_id", 0);
    emitter.instruction("cmp r11, r12");                                        // is this a SplQueue object?
    emitter.instruction("je __rt_mixed_array_set_spl_dll");                     // SplQueue shares list storage
    emitter.instruction("jmp __rt_mixed_array_set_php_offset_set");             // any other class may still be a PHP ArrayAccess
    emitter.label("__rt_mixed_array_set_spl_fixed");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload normalized key high word
    emitter.instruction("cmp r11, -2");                                         // does the internal append tuple require a null offset?
    emitter.instruction("je __rt_mixed_array_set_spl_fixed_null_key");          // box append offsets as PHP null
    emitter.instruction("cmp r11, -1");                                         // restore the integer-key comparison
    emitter.instruction("je __rt_mixed_array_set_spl_fixed_int_key");           // integer keys box as Mixed int
    emitter.instruction("mov rax, 1");                                          // tag = string for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // key string length
    emitter.instruction("jmp __rt_mixed_array_set_spl_fixed_box_key");          // share offsetSet after key boxing
    emitter.label("__rt_mixed_array_set_spl_fixed_int_key");
    emitter.instruction("mov rax, 0");                                          // tag = int for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key integer payload
    emitter.instruction("xor esi, esi");                                        // integer keys have no high payload
    emitter.instruction("jmp __rt_mixed_array_set_spl_fixed_box_key");          // integer offsets use the common boxing call
    emitter.label("__rt_mixed_array_set_spl_fixed_null_key");
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 represents PHP null
    emitter.instruction("xor edi, edi");                                        // null has no low payload
    emitter.instruction("xor esi, esi");                                        // null has no high payload
    emitter.label("__rt_mixed_array_set_spl_fixed_box_key");
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed ArrayAccess offset
    emitter.instruction("mov rsi, rax");                                        // pass boxed offset as argument 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // pass unboxed SplFixedArray receiver as argument 0
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // pass owned Mixed value as argument 2
    emitter.instruction("call __rt_spl_fixed_offset_set");                      // write through SplFixedArray::offsetSet
    emitter.instruction("jmp __rt_mixed_array_set_done");                       // value ownership was consumed by offsetSet
    emitter.label("__rt_mixed_array_set_spl_dll");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload normalized key high word
    emitter.instruction("cmp r11, -2");                                         // does the internal append tuple require a null offset?
    emitter.instruction("je __rt_mixed_array_set_spl_dll_null_key");            // box append offsets as PHP null
    emitter.instruction("cmp r11, -1");                                         // restore the integer-key comparison
    emitter.instruction("je __rt_mixed_array_set_spl_dll_int_key");             // integer keys box as Mixed int
    emitter.instruction("mov rax, 1");                                          // tag = string for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // key string length
    emitter.instruction("jmp __rt_mixed_array_set_spl_dll_box_key");            // share offsetSet after key boxing
    emitter.label("__rt_mixed_array_set_spl_dll_int_key");
    emitter.instruction("mov rax, 0");                                          // tag = int for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key integer payload
    emitter.instruction("xor esi, esi");                                        // integer keys have no high payload
    emitter.instruction("jmp __rt_mixed_array_set_spl_dll_box_key");            // integer offsets use the common boxing call
    emitter.label("__rt_mixed_array_set_spl_dll_null_key");
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 represents PHP null
    emitter.instruction("xor edi, edi");                                        // null has no low payload
    emitter.instruction("xor esi, esi");                                        // null has no high payload
    emitter.label("__rt_mixed_array_set_spl_dll_box_key");
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed ArrayAccess offset
    emitter.instruction("mov rsi, rax");                                        // pass boxed offset as argument 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // pass unboxed SPL list receiver as argument 0
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // pass owned Mixed value as argument 2
    emitter.instruction("call __rt_spl_dll_offset_set");                        // write through the shared SPL list offsetSet helper
    emitter.instruction("jmp __rt_mixed_array_set_done");                       // value ownership was consumed by offsetSet

    // See the ARM64 path: the ids above are the runtime's own containers, everything else is a PHP
    // class whose `ArrayAccess::offsetSet` is only reachable through the dense method table.
    emitter.label("__rt_mixed_array_set_php_offset_set");
    emitter.instruction("test r11, r11");                                       // reject negative synthetic class ids
    emitter.instruction("js __rt_mixed_array_set_not_indexable");               // synthetic ids cannot index metadata
    emitter.instruction("cmp r11, QWORD PTR [rip + _class_iface_method_count]"); // is the id within the dense table?
    emitter.instruction("jae __rt_mixed_array_set_not_indexable");              // out-of-range ids have no entry
    emitter.instruction("lea r12, [rip + _class_offsetset_ptrs]");              // dense ArrayAccess::offsetSet table
    emitter.instruction("mov r12, QWORD PTR [r12 + r11 * 8]");                  // resolve the concrete or inherited offsetSet
    emitter.instruction("test r12, r12");                                       // 0 means the class is not ArrayAccess at all
    emitter.instruction("jz __rt_mixed_array_set_not_indexable");               // refuse rather than drop the write
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload normalized key high word
    emitter.instruction("cmp r11, -2");                                         // does the internal append tuple require a null offset?
    emitter.instruction("je __rt_mixed_array_set_php_null_key");                // box append offsets as PHP null
    emitter.instruction("cmp r11, -1");                                         // restore the integer-key comparison
    emitter.instruction("je __rt_mixed_array_set_php_int_key");                 // integer keys box as Mixed int
    emitter.instruction("mov rax, 1");                                          // tag = string for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // key string length
    emitter.instruction("jmp __rt_mixed_array_set_php_box_key");                // share offsetSet after key boxing
    emitter.label("__rt_mixed_array_set_php_int_key");
    emitter.instruction("mov rax, 0");                                          // tag = int for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // key integer payload
    emitter.instruction("xor esi, esi");                                        // integer keys have no high payload
    emitter.instruction("jmp __rt_mixed_array_set_php_box_key");                // integer offsets use the common boxing call
    emitter.label("__rt_mixed_array_set_php_null_key");
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 represents PHP null
    emitter.instruction("xor edi, edi");                                        // null has no low payload
    emitter.instruction("xor esi, esi");                                        // null has no high payload
    emitter.label("__rt_mixed_array_set_php_box_key");
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed ArrayAccess offset
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // stash the box; key_lo has served its purpose
    // -- publish callback arguments until offsetSet returns or throws --
    emitter.instruction("lea r11, [rbp - 32]");                                 // address the consumed value owner
    abi::emit_push_call_operand_owner(emitter, "r11", false);
    emitter.instruction("lea r11, [rbp - 16]");                                 // address the boxed key owner through the stable frame
    abi::emit_push_call_operand_owner(emitter, "r11", false);
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // borrow the published key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // pass unboxed receiver as argument 0
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // pass the boxed value as argument 2
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "r11", "rdi");
    emitter.instruction("lea r12, [rip + _class_offsetset_ptrs]");              // dense ArrayAccess::offsetSet table
    emitter.instruction("mov r12, QWORD PTR [r12 + r11 * 8]");                  // re-resolve offsetSet for the same class
    emitter.instruction("call r12");                                            // write through PHP's ArrayAccess::offsetSet
    // A PHP method BORROWS both arguments, where the SPL helpers CONSUME the value. This frame
    // therefore releases the offset it boxed and the value its own contract says it consumed.
    abi::emit_pop_call_operand_owner(emitter);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the scalar key while the value is protected
    emitter.instruction("call __rt_decref_mixed");                              // release the boxed offset
    abi::emit_pop_call_operand_owner(emitter);
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed value
    emitter.instruction("call __rt_decref_mixed");                              // release it too
    emitter.instruction("jmp __rt_mixed_array_set_done");                       // both releases done: leave through the shared epilogue

    // PHP refuses to index an object that is not ArrayAccess, on writes exactly as on reads.
    emitter.label("__rt_mixed_array_set_not_indexable");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed value
    emitter.instruction("call __rt_decref_mixed");                              // release it before leaving through the Error
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // pass the receiver to name its class
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before the tail-call
    emitter.instruction("jmp __rt_throw_object_not_array");                     // never returns

    emit_string_payload_set_x86_64(emitter);
    // -- reject a Closure after retiring the consumed write value --
    // A callable descriptor has no class id to look up, but PHP still names it as Closure.
    emitter.label("__rt_mixed_array_set_closure_not_indexable");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the owned boxed value consumed by the rejected write
    emitter.instruction("call __rt_decref_mixed");                              // retire the unused value before constructing its Error
    abi::emit_symbol_address(emitter, "rax", "_closure_class_name");
    abi::emit_load_int_immediate(emitter, "rdx", CLOSURE_CLASS_NAME.len() as i64);
    emitter.instruction("mov rsp, rbp");                                        // release the setter frame before the tail call
    emitter.instruction("pop rbp");                                             // restore the caller frame before unwinding
    emitter.instruction("jmp __rt_throw_object_not_array_name");                // throw a catchable Error using the literal Closure name

    emitter.label("__rt_mixed_array_set_reference");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // borrow the incoming value for reference assignment
    emitter.instruction("call __rt_reference_replace");                         // publish an independent value in the existing shared wrapper
    emitter.instruction("call __rt_decref_any");                                // release the replaced child after publication
    emitter.label("__rt_mixed_array_set_drop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the unused boxed value
    emitter.instruction("call __rt_decref_mixed");                              // release the boxed value when the write cannot be applied
    emitter.label("__rt_mixed_array_set_done");
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code
}

const STRING_OFFSET_TYPE_ERROR_MSG_LEN: usize =
    "Cannot access offset of type string on string".len();

/// Emits the ARM64 string-payload arm of `__rt_mixed_array_set`: PHP's `$s[$i] = $v` when `$s`
/// is a boxed Mixed cell holding a string (a `mixed` parameter, a `global`, or a string local
/// whose `++` gave it boxed storage).
///
/// Runs inside the setter's 80-byte frame: `[sp, #0]` cell, `[sp, #8]`/`[sp, #16]` key words,
/// `[sp, #24]` boxed value; `[sp, #32..#56]` are free on this path, and `[sp, #16]` becomes the
/// retained-cell flag once the key is checked. The value is cast to a string,
/// `__rt_str_offset_set` builds the updated string, and the persisted result replaces the
/// cell's string payload, which the cell owns exclusively (as `__rt_mixed_free_deep` assumes).
/// The value box is consumed like every other arm. A string key has no integer offset and
/// throws PHP's `TypeError`.
///
/// Whenever the helper can warn, the cell is retained across it: a warning handler that
/// reassigns the variable releases the variable's reference, and the retained one then keeps
/// the cell alive. If it is the last one afterwards, the write is abandoned and the cell
/// released, which is what php does. An empty value at a reachable offset throws before any
/// warning, so that path takes no reference it could never give back.
fn emit_string_payload_set_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mixed_array_set_string");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload normalized key high word
    emitter.instruction("cmn x11, #1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("b.ne __rt_mixed_array_set_string_key_error");          // a string offset must be an integer
    emitter.instruction("ldr x0, [sp, #24]");                                   // pass the boxed value to the string cast
    emitter.instruction("bl __rt_mixed_cast_string");                           // x1/x2 = the value as a string
    emitter.instruction("stp x1, x2, [sp, #32]");                               // keep the cast value for the write and its release
    emitter.instruction("cbnz x2, __rt_mixed_array_set_string_write");          // a non-empty value cannot throw below
    emitter.instruction("mov x0, x1");                                          // release an empty cast value before the helper may throw
    emitter.instruction("bl __rt_heap_free_safe");                              // no-op for scratch, frees a persisted copy
    emitter.instruction("str xzr, [sp, #32]");                                  // nothing is left to release after the write
    emitter.label("__rt_mixed_array_set_string_write");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed value
    emitter.instruction("bl __rt_decref_mixed");                                // consume it now, before the helper may throw
    emitter.instruction("str xzr, [sp, #16]");                                  // no retained cell yet; the key high word is spent
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the cast value length
    emitter.instruction("cbnz x9, __rt_mixed_array_set_string_retain");         // a non-empty value returns from the helper, maybe after a warning
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the target Mixed cell
    emitter.instruction("ldr x11, [x10, #16]");                                 // subject string length
    emitter.instruction("ldr x12, [sp, #8]");                                   // the integer offset
    emitter.instruction("adds x12, x12, x11");                                  // the offset counted from the start; N/V keep the exact sum's sign
    emitter.instruction("b.ge __rt_mixed_array_set_string_call");               // it reaches the string (also past INT_MAX): an empty value throws before any warning
    emitter.label("__rt_mixed_array_set_string_retain");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the target Mixed cell
    emitter.instruction("bl __rt_heap_kind");                                   // is the cell heap-owned (refcounted)?
    emitter.instruction("cbz x0, __rt_mixed_array_set_string_call");            // a static cell cannot be freed by a handler
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the target Mixed cell
    emitter.instruction("bl __rt_incref");                                      // keep it alive across a warning handler
    emitter.instruction("mov x9, #1");                                          // remember the retained reference
    emitter.instruction("str x9, [sp, #16]");                                   // in the retained-cell flag
    emitter.label("__rt_mixed_array_set_string_call");
    use elephc_memoir::strings::offset_write_guard::{self, OffsetWriteGuardSlots};
    offset_write_guard::emit_link(emitter, OffsetWriteGuardSlots { cell: 0, retained: 16, cast: 32 });
    let guard = offset_write_guard::STRING_WRITE_GUARD_BYTES;
    emitter.instruction(&format!("ldr x10, [sp, #{guard}]"));                   // reload the target Mixed cell above the guard record
    emitter.instruction("ldr x1, [x10, #8]");                                   // subject string pointer
    emitter.instruction("ldr x2, [x10, #16]");                                  // subject string length
    emitter.instruction(&format!("ldr x0, [sp, #{}]", guard + 8));              // the integer offset
    emitter.instruction(&format!("ldp x3, x4, [sp, #{}]", guard + 32));         // the value string
    emitter.instruction("bl __rt_str_offset_set");                              // x1/x2 = the updated string (scratch or heap)
    offset_write_guard::emit_unlink(emitter, 0, 0);
    let replacement = elephc_memoir::strings::offset_result::OffsetResult { cell: 0, pointer: 48 };
    replacement.emit_persist(emitter, "__rt_mixed_array_set_string_owned");
    replacement.emit_cancel_if_detached(emitter, 16, 32);
    replacement.emit_publish(emitter);
    emitter.instruction("ldr x0, [sp, #32]");                                   // the cast value string
    emitter.instruction("bl __rt_heap_free_safe");                              // release it (no-op for scratch)
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the retained-cell flag
    emitter.instruction("cbz x9, __rt_mixed_array_set_done");                   // nothing was retained
    emitter.instruction("ldr x0, [sp, #0]");                                    // the written cell
    emitter.instruction("bl __rt_decref_mixed");                                // give the retained reference back; the variable keeps the cell
    emitter.instruction("b __rt_mixed_array_set_done");                         // the value was already consumed

    emitter.label("__rt_mixed_array_set_string_key_error");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed value
    emitter.instruction("bl __rt_decref_mixed");                                // release it before leaving through the TypeError
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame before throwing
    crate::codegen_support::runtime::arrays::value_error::emit_throw_static_message_aarch64(
        emitter,
        "_spl_type_error_class_id",
        "_string_offset_type_error_msg",
        STRING_OFFSET_TYPE_ERROR_MSG_LEN,
    );
}

/// Emits the x86_64 string-payload arm of `__rt_mixed_array_set`; see the ARM64 variant.
///
/// Runs inside the setter's 64-byte frame: `[rbp - 8]` cell, `[rbp - 16]`/`[rbp - 24]` key
/// words, `[rbp - 32]` boxed value; `[rbp - 40..-64]` are free on this path.
fn emit_string_payload_set_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mixed_array_set_string");
    emitter.instruction("cmp QWORD PTR [rbp - 24], -1");                        // does key_hi carry the integer-key sentinel?
    emitter.instruction("jne __rt_mixed_array_set_string_key_error");           // a string offset must be an integer
    // `__rt_mixed_cast_string` unboxes whatever `rax` holds (its `__rt_mixed_unbox` input), so
    // the box goes in both registers; `rax` still holds the receiver cell from the deref above.
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // pass the boxed value to the string cast
    emitter.instruction("mov rdi, rax");                                        // and in the documented argument register
    emitter.instruction("call __rt_mixed_cast_string");                         // rax/rdx = the value as a string
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // keep the cast value pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // keep the cast value length
    emitter.instruction("test rdx, rdx");                                       // is the cast value empty?
    emitter.instruction("jnz __rt_mixed_array_set_string_write");               // a non-empty value cannot throw below
    emitter.instruction("call __rt_heap_free_safe");                            // release an empty cast value before the helper may throw
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // nothing is left to release after the write
    emitter.label("__rt_mixed_array_set_string_write");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed value
    emitter.instruction("call __rt_decref_mixed");                              // consume it now, before the helper may throw
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // no retained cell yet; the key high word is spent
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // is the cast value empty?
    emitter.instruction("jne __rt_mixed_array_set_string_retain");              // a non-empty value returns from the helper, maybe after a warning
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the target Mixed cell
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // the integer offset
    emitter.instruction("add r11, QWORD PTR [r10 + 16]");                       // the offset counted from the start
    emitter.instruction("jge __rt_mixed_array_set_string_call");                // an empty value there throws before any warning
    emitter.label("__rt_mixed_array_set_string_retain");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the target Mixed cell
    emitter.instruction("call __rt_heap_kind");                                 // is the cell heap-owned (refcounted)?
    emitter.instruction("test eax, eax");                                       // kind 0 means static storage
    emitter.instruction("jz __rt_mixed_array_set_string_call");                 // a static cell cannot be freed by a handler
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the target Mixed cell
    emitter.instruction("call __rt_incref");                                    // keep it alive across a warning handler
    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // remember the retained reference
    emitter.label("__rt_mixed_array_set_string_call");
    use elephc_memoir::strings::offset_write_guard::{self, OffsetWriteGuardSlots};
    offset_write_guard::emit_link(emitter, OffsetWriteGuardSlots { cell: 8, retained: 24, cast: 40 });
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the target Mixed cell
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // subject string pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // subject string length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // the integer offset
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // the value string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // the value string length
    emitter.instruction("call __rt_str_offset_set");                            // rax/rdx = the updated string (scratch or heap)
    offset_write_guard::emit_unlink(emitter, 56, 64);
    let replacement = elephc_memoir::strings::offset_result::OffsetResult { cell: 8, pointer: 56 };
    replacement.emit_persist(emitter, "__rt_mixed_array_set_string_owned");
    replacement.emit_cancel_if_detached(emitter, 24, 40);
    replacement.emit_publish(emitter);
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the cast value string
    emitter.instruction("call __rt_heap_free_safe");                            // release it (no-op for scratch)
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // was the cell retained?
    emitter.instruction("je __rt_mixed_array_set_done");                        // nothing was retained
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the written cell
    emitter.instruction("call __rt_decref_mixed");                              // give the retained reference back; the variable keeps the cell
    emitter.instruction("jmp __rt_mixed_array_set_done");                       // the value was already consumed

    emitter.label("__rt_mixed_array_set_string_key_error");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed value
    emitter.instruction("call __rt_decref_mixed");                              // release it before leaving through the TypeError
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    crate::codegen_support::runtime::arrays::value_error::emit_throw_static_message_x86_64(
        emitter,
        "_spl_type_error_class_id",
        "_string_offset_type_error_msg",
        STRING_OFFSET_TYPE_ERROR_MSG_LEN,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Reentrant retirement observes the replacement and cannot resume a stale slot store.
    #[test]
    fn mixed_indexed_overwrites_publish_before_protected_retirement_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_mixed_array_set(&mut emitter);
            let assembly = emitter.output();
            let overwrite = assembly.split_once("__rt_mixed_array_set_release_slot:").unwrap().1
                .split_once("__rt_mixed_array_set_skip_release:").unwrap().0;
            let store = if name == "linux-x86_64" { "mov QWORD PTR [r10 + r9 * 8 + 24], r11" }
                else { "str x11, [x12, x9, lsl #3]" };
            let cleanup = overwrite.find("__rt_cleanup_preserve_exception").unwrap();
            assert!(overwrite.find(store).unwrap() < cleanup, "{name}: {overwrite}");
            let after = &overwrite[cleanup..];
            assert!(after.contains("__rt_throw_current") && after.contains("__rt_mixed_array_set_done"), "{name}");
            assert!(!after.contains("[sp, #32]") && !after.contains("[rbp - 40]"),
                "{name}: no saved destination pointer remains valid after a destructor");
        }
    }
}
