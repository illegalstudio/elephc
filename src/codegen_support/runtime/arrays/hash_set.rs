//! Purpose:
//! Emits the `__rt_hash_set`, `__rt_hash_ensure_unique` runtime helper assembly for hash set.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Hash helpers must normalize PHP keys and preserve bucket layout, ownership, and iteration conventions.
//! - An ordinary overwrite stores the replacement BEFORE releasing the old value and never
//!   touches the table afterwards: the release can run a destructor that reads, grows or
//!   replaces the table, as PHP's own assignment order allows.
//! - `__rt_hash_set` is the PHP element-assignment entry point. Overwriting an entry that
//!   carries runtime value tag 11 writes through its managed reference cell: the previous cell
//!   payload is released and the replacement is published to every alias in the set, so a live
//!   `&$v` is never detached by `$a[$k] = x`.
//! - A Mixed-valued variadic argument hash can instead contain a boxed invoker marker. Its outer
//!   entry tag is 7 and its inner tag is 11. Updates write through the marker's caller-storage
//!   pointer while leaving the argument container intact.
//! - `__rt_hash_set_value` is the bucket-replacing sibling for array-building helpers whose
//!   destination was cloned from a user array. It detaches a reference entry first, matching
//!   `zend_hash_update`, so building a new array never mutates the array it was copied from.
//! - `__rt_hash_set_shared` is restricted to a pending append selected before a diagnostic
//!   callback. It preserves the exact payload retained by a surviving child copy.
//! - `__rt_hash_set_owned_refusal` keeps ordinary COW and retires only the separate helper
//!   owner when a mixed-key storeback refuses a nullable-reference assignment.

use elephc_memoir::values::hash_bucket;

use crate::codegen_support::abi;
use crate::codegen_support::callable_invoker_args::{INVOKER_ARG_REF_CELL_TAG, INVOKER_ARG_TAGGED_SCALAR_SOURCE};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use super::hash_layout;

mod persistent_references;

/// hash_set: insert or update a normalized int/string key-value pair in the hash table.
/// Grows the table automatically if load factor exceeds 75%, persists newly
/// inserted string keys, and releases overwritten heap-backed values on update.
/// Input:  x0=hash_table_ptr, x1=key_lo, x2=key_hi (-1 means integer key), x3=value_lo, x4=value_hi, x5=value_tag
/// Output: x0=hash_table_ptr (may differ if table was reallocated)
pub fn emit_hash_set(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_set_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_set ---");
    emitter.label_global("__rt_hash_set");
    emitter.instruction("mov x9, #0");                                          // ordinary writes retain the hash COW boundary
    emitter.instruction("b __rt_hash_set_entry");                               // share insertion and update machinery
    emitter.label_global("__rt_hash_set_shared");
    emitter.instruction("mov x9, #1");                                          // pending append mutates its pre-callback selected payload
    emitter.instruction("b __rt_hash_set_entry");                               // keep selected-payload mode distinct from exceptional owner retirement
    emitter.label_global("__rt_hash_set_owned_refusal");
    emitter.instruction("mov x9, #2");                                          // a mixed-key caller supplied a separate owner to retire on refusal
    emitter.label_shared("__rt_hash_set_entry");

    // -- set up stack frame, save all inputs --
    // Stack layout:
    //   [sp, #0]  = hash_table_ptr (x0)
    //   [sp, #8]  = key_ptr (x1)
    //   [sp, #16] = key_len (x2)
    //   [sp, #24] = value_lo (x3)
    //   [sp, #32] = value_hi (x4)
    //   [sp, #40] = value_tag (x5)
    //   [sp, #48] = probe index (saved across calls)
    //   [sp, #56] = selected shared-payload mode
    //   [sp, #64] = saved x29
    //   [sp, #72] = saved x30
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save key_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save key_len
    emitter.instruction("str x3, [sp, #24]");                                   // save value_lo
    emitter.instruction("str x4, [sp, #32]");                                   // save value_hi
    emitter.instruction("str x5, [sp, #40]");                                   // save value_tag
    emitter.instruction("str x9, [sp, #56]");                                   // preserve the pending-append selection mode
    emitter.instruction("str x0, [sp, #0]");                                    // save the candidate hash table before null normalization
    super::hash_set_reference_layout::emit_shared_tagged_reference_selection(emitter);
    emitter.instruction("cbnz x0, __rt_hash_set_have_table");                   // a live table skips the null-hash allocation
    emitter.instruction("mov x0, #4");                                          // allocate a four-entry hash for a null dynamic-property slot
    emitter.instruction("ldr x1, [sp, #40]");                                   // preserve the caller's value tag as the hash value type
    emitter.instruction("bl __rt_hash_new");                                    // create the missing hash table
    emitter.instruction("str x0, [sp, #0]");                                    // publish the freshly allocated table
    emitter.label("__rt_hash_set_have_table");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the candidate table
    emitter.instruction("tbnz x9, #0, __rt_hash_set_ready");                    // only selected-payload mode bypasses the ordinary COW boundary
    emitter.instruction("bl __rt_hash_ensure_unique");                          // split ordinary shared hash tables before mutation
    emitter.label("__rt_hash_set_ready");
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique hash_table_ptr

    elephc_memoir::values::hash_bucket_write::emit_aarch64(emitter);
    emitter.instruction("b __rt_hash_set_done");                                // done inserting

    // -- update existing entry's value --
    emitter.label("__rt_hash_set_update");
    emitter.instruction("mov x0, x5");                                          // inspect ownership in the actual separated hash
    emitter.instruction("ldr x1, [sp, #8]");                                    // recover the mutating key payload
    emitter.instruction("ldr x2, [sp, #16]");                                   // recover its normalized high word
    emitter.instruction("bl __rt_hash_write_guard_claim");                      // avoid releasing an owner already consumed by an outer construction write
    emitter.instruction("ldr x5, [sp, #0]");                                    // recover the stable header after key comparison
    emitter.instruction("ldr x9, [sp, #48]");                                   // recover the previously selected slot index
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");
    emitter.instruction("cbz x0, __rt_hash_set_write_value");                   // replace the borrowing entry without releasing its old value twice
    emitter.instruction("ldr x13, [x12, #40]");                                 // load the overwritten entry's per-entry value_tag
    emitter.instruction("cmp x13, #7");                                         // can this Mixed entry contain an invoker reference marker?
    emitter.instruction("b.ne __rt_hash_set_check_reference");                  // concrete entries cannot contain the boxed marker shape
    emitter.instruction("ldr x14, [x12, #24]");                                 // load the existing boxed Mixed payload
    emitter.instruction("cbz x14, __rt_hash_set_check_reference");              // a null Mixed handle is an ordinary overwrite
    emitter.instruction("ldr x15, [x14]");                                      // inspect the boxed Mixed runtime tag
    emitter.instruction(&format!("cmp x15, #{}", INVOKER_ARG_REF_CELL_TAG));    // does this box alias caller storage for a ref argument?
    emitter.instruction("b.eq __rt_hash_set_invoker_reference_write");          // preserve the marker and write through to its caller local
    persistent_references::emit_probe(emitter);
    emitter.label("__rt_hash_set_check_reference");
    emitter.instruction(&format!("cmp x13, #{}", INVOKER_ARG_REF_CELL_TAG));    // does the existing entry belong to a PHP reference set?
    emitter.instruction("b.eq __rt_hash_set_reference_write");                  // reference entries are written through, never detached
    emitter.instruction("cmp x13, #8");                                         // is the overwritten value null?
    emitter.instruction("b.eq __rt_hash_set_write_value");                      // null has no heap pointer, skip release
    emitter.instruction("cmp x13, #1");                                         // is the overwritten value a string?
    emitter.instruction("b.eq __rt_hash_set_release_any");                      // strings release through the uniform dispatcher
    emitter.instruction("cmp x13, #10");                                        // is the overwritten value a callable descriptor?
    emitter.instruction("b.eq __rt_hash_set_release_callable");                 // callable descriptors release through the descriptor helper
    emitter.instruction("cmp x13, #4");                                         // is the overwritten value a heap-backed payload?
    emitter.instruction("b.hs __rt_hash_set_release_any");                      // tags 4-7 all release through the uniform dispatcher
    emitter.instruction("b __rt_hash_set_write_value");                         // scalars/bools/floats do not need release before overwrite

    // -- publish the replacement, then release the overwritten payload last --
    // The release can run a destructor that reads, grows or replaces this very table, so the
    // entry must already hold the replacement and nothing may touch the table afterwards.
    emitter.label("__rt_hash_set_release_any");
    hash_bucket::emit_replace_and_release(emitter, "__rt_decref_any");
    emitter.instruction("b __rt_hash_set_done");                                // the entry already holds the replacement

    emitter.label("__rt_hash_set_release_callable");
    hash_bucket::emit_replace_and_release(emitter, "__rt_callable_descriptor_release");
    emitter.instruction("b __rt_hash_set_done");                                // the entry already holds the replacement

    // -- write through a boxed descriptor-invoker marker stored in a Mixed hash entry --
    persistent_references::emit_write(emitter);
    emitter.label("__rt_hash_set_invoker_reference_write");
    elephc_memoir::layout::emit_load_mixed_high(emitter, "x15", "x14");
    emitter.instruction("cmp x15, #7");                                         // does the caller local hold a boxed Mixed handle?
    emitter.instruction("b.ne __rt_hash_set_invoker_reference_raw");            // concrete caller slots receive their native payload words
    emitter.instruction("ldr x13, [sp, #40]");                                  // load the replacement runtime value tag
    emitter.instruction("cmp x13, #7");                                         // is the replacement already an owned boxed Mixed cell?
    emitter.instruction("b.eq __rt_hash_set_invoker_reference_mixed_ready");    // owned Mixed values can transfer directly to caller storage
    emitter.instruction("mov x0, x13");                                         // pass the replacement runtime tag to the owned-box helper
    emitter.instruction("ldr x1, [sp, #24]");                                   // pass the replacement low payload word
    emitter.instruction("ldr x2, [sp, #32]");                                   // pass the replacement high payload word
    emitter.instruction("bl __rt_hash_to_mixed_box_owned");                     // consume the replacement into one owned Mixed cell
    emitter.instruction("str x0, [sp, #24]");                                   // save the boxed replacement across entry reconstruction
    emitter.label("__rt_hash_set_invoker_reference_mixed_ready");
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload the hash table after optional boxing
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the current probe index
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");
    emitter.instruction("ldr x14, [x12, #24]");                                 // reload the boxed invoker marker
    elephc_memoir::layout::emit_load_mixed_low(emitter, "x14", "x14");
    crate::codegen_support::runtime::reference_cells::follow_mixed_reference_slot(
        emitter, "x14", "x13", "x15", "__rt_hash_set_invoker_phpref_payload",
    );
    elephc_memoir::reference_cells::mixed_payload::emit_publish_staged_payload(
        emitter, "x14", "x13", "[sp, #24]", None,
    );
    emitter.instruction("bl __rt_decref_mixed");                                // retire the previous caller-local Mixed owner
    emitter.instruction("b __rt_hash_set_done");                                // the argument hash still owns the unchanged marker wrapper

    emitter.label("__rt_hash_set_invoker_reference_raw");
    emitter.instruction(&format!("cmp x15, #{INVOKER_ARG_TAGGED_SCALAR_SOURCE}")); // source shape12 stores a nullable integer payload and tag
    emitter.instruction("b.eq __rt_hash_set_invoker_reference_tagged");         // decode boxed replacements before publishing both words
    elephc_memoir::layout::emit_load_mixed_low(emitter, "x14", "x14");
    elephc_memoir::reference_cells::mixed_payload::emit_publish_staged_payload(
        emitter, "x14", "x13", "[sp, #24]", Some("[sp, #32]"),
    );
    emitter.instruction("cmp x15, #1");                                         // strings own their low payload
    emitter.instruction("b.eq __rt_hash_set_invoker_reference_release_any");    // release the overwritten string after publication
    emitter.instruction("cmp x15, #10");                                        // callable descriptors use their dedicated release helper
    emitter.instruction("b.eq __rt_hash_set_invoker_reference_release_callable"); // retire the overwritten callable descriptor
    emitter.instruction("cmp x15, #4");                                         // arrays, hashes and objects own their low payload
    emitter.instruction("b.lo __rt_hash_set_done");                             // scalar caller slots need no release
    emitter.instruction("cmp x15, #6");                                         // tag 7 was handled by the boxed-Mixed path above
    emitter.instruction("b.ls __rt_hash_set_invoker_reference_release_any");    // release tags 4 through 6 uniformly
    emitter.instruction("b __rt_hash_set_done");                                // null and other non-owning tags are complete
    emitter.label("__rt_hash_set_invoker_reference_release_any");
    emitter.instruction("bl __rt_decref_any");                                  // release the overwritten heap payload after publication
    emitter.instruction("b __rt_hash_set_done");                                // return the unchanged argument hash
    emitter.label("__rt_hash_set_invoker_reference_release_callable");
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the overwritten callable after publication
    emitter.instruction("b __rt_hash_set_done");                                // return the unchanged argument hash

    emitter.label("__rt_hash_set_invoker_reference_tagged");
    elephc_memoir::layout::emit_load_mixed_low(emitter, "x14", "x14");
    emitter.instruction("ldr x0, [sp, #40]");                                   // inspect the replacement PHP representation
    emitter.instruction("cmp x0, #7");                                          // a Mixed replacement owns a box that must be decoded
    emitter.instruction("b.eq __rt_hash_set_invoker_reference_tagged_box");     // preserve the boxed value until both words are published
    crate::codegen_support::callable_invoker_args::emit_tagged_scalar_ref_assignment_guard(
        emitter, "x0", "__rt_hash_set_invoker_reference_tagged_valid",
        "__rt_hash_set_invoker_reference_tagged_invalid",
    );
    emitter.label("__rt_hash_set_invoker_reference_tagged_invalid");
    emitter.instruction("ldr x1, [sp, #24]");                                   // consume the rejected direct payload with its original PHP tag
    emitter.instruction("ldr x2, [sp, #32]");                                   // preserve string lengths and other high words during cleanup
    abi::emit_call_label(emitter, "__rt_hash_to_mixed_box_owned");
    super::container_refusal_owner::emit_throw(emitter);
    emitter.label("__rt_hash_set_invoker_reference_tagged_valid");
    emitter.instruction("ldr x1, [sp, #24]");                                   // load the direct nullable-integer payload
    elephc_memoir::reference_cells::mixed_payload::emit_publish_payload_pair(emitter, "x14", "x1", "x0");
    emitter.instruction("b __rt_hash_set_done");                                // the marker remains owned by the argument hash
    emitter.label("__rt_hash_set_invoker_reference_tagged_box");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load the replacement box owned by this write
    abi::emit_push_reg_pair(emitter, "x0", "x14");
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    crate::codegen_support::callable_invoker_args::emit_tagged_scalar_ref_assignment_guard(
        emitter, "x0", "__rt_hash_set_invoker_reference_tagged_box_valid",
        "__rt_hash_set_invoker_reference_tagged_box_invalid",
    );
    emitter.label("__rt_hash_set_invoker_reference_tagged_box_invalid");
    abi::emit_pop_reg_pair(emitter, "x0", "x14");
    super::container_refusal_owner::emit_throw(emitter);
    emitter.label("__rt_hash_set_invoker_reference_tagged_box_valid");
    abi::emit_load_temporary_stack_slot(emitter, "x14", 8);
    elephc_memoir::reference_cells::mixed_payload::emit_publish_payload_pair(emitter, "x14", "x1", "x0");
    abi::emit_pop_reg_pair(emitter, "x0", "x14");
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.instruction("b __rt_hash_set_done");                                // retire the consumed box without changing marker identity

    // -- write through an existing PHP reference set instead of replacing the entry --
    emitter.label("__rt_hash_set_reference_write");
    emitter.instruction("ldr x13, [sp, #40]");                                  // load the replacement runtime value tag
    emitter.instruction("cmp x13, #7");                                         // is the replacement already an owned boxed Mixed cell?
    emitter.instruction("b.eq __rt_hash_set_reference_release");                // an owned box transfers straight into the reference cell
    emitter.instruction("mov x0, x13");                                         // pass the replacement runtime value tag to the owned-box helper
    emitter.instruction("ldr x1, [sp, #24]");                                   // pass the replacement low payload word to the owned-box helper
    emitter.instruction("ldr x2, [sp, #32]");                                   // pass the replacement high payload word to the owned-box helper
    emitter.instruction("bl __rt_hash_to_mixed_box_owned");                     // box the replacement without adding a retain
    emitter.instruction("str x0, [sp, #24]");                                   // save the boxed replacement as the new reference payload

    emitter.label("__rt_hash_set_reference_release");
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash table pointer after the boxing helper
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload probe index after the boxing helper
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");
    emitter.instruction("ldr x9, [x12, #24]");                                  // load the managed reference cell this entry owns
    emitter.instruction("ldr x13, [sp, #24]");                                  // load the boxed replacement payload
    elephc_memoir::reference_cells::emit_replace_boxed_child(emitter, "x9", "x13", "x10");
    emitter.instruction("b __rt_hash_set_done");                                // value_lo, value_hi and value_tag keep describing the cell

    emitter.label("__rt_hash_set_write_value");
    hash_bucket::emit_publish_payload(emitter);

    // -- tear down stack frame and return --
    emitter.label("__rt_hash_set_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return table pointer (may be new after grow)
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    emit_hash_set_value_aarch64(emitter);
}

/// Emits the ARM64 `__rt_hash_set_value`, the bucket-replacing sibling of `__rt_hash_set`.
///
/// Same ABI as `__rt_hash_set`. The difference is the reference rule: this entry point DETACHES
/// an existing reference entry instead of writing through it, which is what an array-building
/// helper needs when it populates a destination that was cloned from a user array. The clone
/// shares its reference cells with the source, so writing through would mutate the source too.
///
/// Copy-on-write runs first so the detach can never touch a table another owner still shares,
/// and the unique table is what the tail call to `__rt_hash_set` then updates.
fn emit_hash_set_value_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_set_value ---");
    emitter.label_global("__rt_hash_set_value");
    emitter.instruction("sub sp, sp, #64");                                     // reserve slots for the forwarded arguments and the unique table
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the detach frame
    emitter.instruction("str x1, [sp, #0]");                                    // save key_lo across the split and probe
    emitter.instruction("str x2, [sp, #8]");                                    // save key_hi across the split and probe
    emitter.instruction("str x3, [sp, #16]");                                   // save value_lo across the split and probe
    emitter.instruction("str x4, [sp, #24]");                                   // save value_hi across the split and probe
    emitter.instruction("str x5, [sp, #32]");                                   // save value_tag across the split and probe
    emitter.instruction("bl __rt_hash_ensure_unique");                          // split a shared table before detaching anything in it
    emitter.instruction("str x0, [sp, #40]");                                   // save the unique table pointer
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload key_lo for the entry probe
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload key_hi for the entry probe
    emitter.instruction("bl __rt_hash_get");                                    // x0 = found, x4 = matching entry address
    emitter.instruction("cbz x0, __rt_hash_set_value_forward");                 // a missing key is an ordinary insert
    emitter.instruction("ldr x9, [x4, #40]");                                   // load the existing entry's runtime value tag
    persistent_references::emit_value_detach_probe(emitter);
    hash_bucket::emit_detach_reference_owner(emitter);
    emitter.label("__rt_hash_set_value_forward");
    emitter.instruction("ldr x0, [sp, #40]");                                   // forward the unique table pointer
    emitter.instruction("ldr x1, [sp, #0]");                                    // forward key_lo
    emitter.instruction("ldr x2, [sp, #8]");                                    // forward key_hi
    emitter.instruction("ldr x3, [sp, #16]");                                   // forward value_lo
    emitter.instruction("ldr x4, [sp, #24]");                                   // forward value_hi
    emitter.instruction("ldr x5, [sp, #32]");                                   // forward value_tag
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the detach frame
    emitter.instruction("b __rt_hash_set");                                     // the entry is now ordinary, so the shared path applies
}

/// Emits the x86_64 Linux implementation of `__rt_hash_set`.
///
/// Implements the same linear-probe insert-or-update algorithm as the ARM64
/// variant using the SystemV AMD64 ABI: rdi=hash_table_ptr, rsi=key_ptr,
/// rdx=key_len, rcx=value_lo, r8=value_hi, r9=value_tag. Returns the table
/// pointer (possibly new after grow) in rax. Preserves r12/r13/r14 as
/// callee-saved registers; all other caller-saved regs are clobbered.
fn emit_hash_set_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_set ---");
    emitter.label_global("__rt_hash_set");
    emitter.instruction("xor r10d, r10d");                                      // ordinary writes retain the hash COW boundary
    emitter.instruction("jmp __rt_hash_set_x86_entry");                         // share insertion and update machinery
    emitter.label_global("__rt_hash_set_shared");
    emitter.instruction("mov r10d, 1");                                         // pending append mutates its pre-callback selected payload
    emitter.instruction("jmp __rt_hash_set_x86_entry");                         // keep selected-payload mode independent from refusal ownership
    emitter.label_global("__rt_hash_set_owned_refusal");
    emitter.instruction("mov r10d, 2");                                         // identify the caller's extra destination owner on nullable refusal
    emitter.label("__rt_hash_set_x86_entry");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving hash-insert spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved table/key/value tuple
    emitter.instruction("sub rsp, 96");                                         // reserve local storage plus callee-saved register spills while keeping nested calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 72], r12");                       // preserve r12 because SysV requires callees to restore it before returning
    emitter.instruction("mov QWORD PTR [rbp - 80], r13");                       // preserve r13 because the hash probe logic reuses it as a long-lived scratch register
    emitter.instruction("mov QWORD PTR [rbp - 88], r14");                       // preserve r14 because the insertion-order linker uses it as a callee-saved scratch register
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash-table pointer across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the incoming key pointer across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the incoming key length across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the low payload word across the probe and optional update paths
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the high payload word across the probe and optional update paths
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the runtime value tag across the probe and optional update paths
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // preserve the pending-append selection mode
    super::hash_set_reference_layout::emit_shared_tagged_reference_selection(emitter);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the candidate hash-table pointer before null normalization
    emitter.instruction("test rdi, rdi");                                       // is the dynamic-property hash slot still null?
    emitter.instruction("jnz __rt_hash_set_x86_have_table");                    // a live table skips the null-hash allocation
    emitter.instruction("mov rdi, 4");                                          // allocate a four-entry hash for a null dynamic-property slot
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // preserve the caller's value tag as the hash value type
    emitter.instruction("call __rt_hash_new");                                  // create the missing hash table
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // publish the freshly allocated table
    emitter.instruction("mov rdi, rax");                                        // keep the new table as the mutation target
    emitter.label("__rt_hash_set_x86_have_table");
    emitter.instruction("mov rax, rdi");                                        // selected mode returns the original table without a COW helper call
    emitter.instruction("test r10, 1");                                         // only pending-append selection bypasses COW
    emitter.instruction("jnz __rt_hash_set_x86_ready");                         // yes, preserve that exact payload identity
    emitter.instruction("call __rt_hash_ensure_unique");                        // split ordinary shared associative arrays before mutation
    emitter.label("__rt_hash_set_x86_ready");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the unique hash-table pointer that subsequent probe logic must mutate

    elephc_memoir::values::hash_bucket_write::emit_x86_64(emitter);
    emitter.instruction("mov rax, r10");                                        // return the original hash-table pointer after a successful insertion
    emitter.instruction("mov r14, QWORD PTR [rbp - 88]");                       // restore the caller's r14 before leaving the hash-set helper
    emitter.instruction("mov r13, QWORD PTR [rbp - 80]");                       // restore the caller's r13 before leaving the hash-set helper
    emitter.instruction("mov r12, QWORD PTR [rbp - 72]");                       // restore the caller's r12 before leaving the hash-set helper
    emitter.instruction("add rsp, 96");                                         // release the local spill area that held the saved table/key/value tuple
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the insertion caller
    emitter.instruction("ret");                                                 // return to the caller with the hash-table pointer in rax

    emitter.label("__rt_hash_set_update");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // inspect ownership in the actual separated hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // recover the mutating key payload
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // recover its normalized high word
    emitter.instruction("call __rt_hash_write_guard_claim");                    // avoid releasing an owner already consumed by an outer construction write
    emitter.instruction("test rax, rax");                                       // determine whether this update owns its previous value
    emitter.instruction("jz __rt_hash_set_write_value_x");                      // retain the callee-saved selected entry while skipping duplicate release
    emitter.instruction("mov r13, QWORD PTR [r12 + 40]");                       // load the overwritten entry's runtime value tag before replacing it
    emitter.instruction("cmp r13, 7");                                          // can this Mixed entry contain an invoker reference marker?
    emitter.instruction("jne __rt_hash_set_check_reference_x");                 // concrete entries cannot contain the boxed marker shape
    emitter.instruction("mov r14, QWORD PTR [r12 + 24]");                       // load the existing boxed Mixed payload
    emitter.instruction("test r14, r14");                                       // guard the boxed marker dereference
    emitter.instruction("jz __rt_hash_set_check_reference_x");                  // a null Mixed handle is an ordinary overwrite
    let boxed_marker_compare = format!("cmp QWORD PTR [r14], {}", INVOKER_ARG_REF_CELL_TAG);
    emitter.instruction(&boxed_marker_compare);                                 // does this box alias caller storage for a ref argument?
    emitter.instruction("je __rt_hash_set_invoker_reference_write_x");          // preserve the marker and write through to its caller local
    persistent_references::emit_probe(emitter);
    emitter.label("__rt_hash_set_check_reference_x");
    emitter.instruction(&format!("cmp r13, {}", INVOKER_ARG_REF_CELL_TAG));     // does the existing entry belong to a PHP reference set?
    emitter.instruction("je __rt_hash_set_reference_write_x");                  // reference entries are written through, never detached
    emitter.instruction("cmp r13, 8");                                          // check whether the overwritten value is PHP null
    emitter.instruction("je __rt_hash_set_write_value_x");                      // null owns no heap payload and can be overwritten directly
    emitter.instruction("cmp r13, 1");                                          // check whether the overwritten value is an owned string
    emitter.instruction("je __rt_hash_set_release_any_x");                      // string values release through the uniform dispatcher
    emitter.instruction("cmp r13, 10");                                         // check whether the overwritten value is a callable descriptor
    emitter.instruction("je __rt_hash_set_release_callable_x");                 // callable descriptors release through the descriptor helper
    emitter.instruction("cmp r13, 4");                                          // check whether the overwritten value is heap backed
    emitter.instruction("jae __rt_hash_set_release_any_x");                     // arrays, hashes, objects, and boxed mixed values need release
    emitter.instruction("jmp __rt_hash_set_write_value_x");                     // scalar values own no heap payload

    // -- publish the replacement, then release the overwritten payload last --
    // The release can run a destructor that reads, grows or replaces this very table, so the
    // entry must already hold the replacement and nothing may touch the table afterwards.
    emitter.label("__rt_hash_set_release_any_x");
    hash_bucket::emit_replace_and_release(emitter, "__rt_decref_any");
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // the entry already holds the replacement

    emitter.label("__rt_hash_set_release_callable_x");
    hash_bucket::emit_replace_and_release(emitter, "__rt_callable_descriptor_release");
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // the entry already holds the replacement

    persistent_references::emit_write(emitter);
    emitter.label("__rt_hash_set_invoker_reference_write_x");
    elephc_memoir::layout::emit_load_mixed_high(emitter, "r13", "r14");
    emitter.instruction("cmp r13, 7");                                          // does the caller local hold a boxed Mixed handle?
    emitter.instruction("jne __rt_hash_set_invoker_reference_raw_x");           // concrete caller slots receive their native payload words
    emitter.instruction("cmp QWORD PTR [rbp - 48], 7");                         // is the replacement already an owned boxed Mixed cell?
    emitter.instruction("je __rt_hash_set_invoker_reference_mixed_ready_x");    // owned Mixed values can transfer directly to caller storage
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // pass the replacement runtime tag to the owned-box helper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the replacement low payload word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the replacement high payload word
    emitter.instruction("call __rt_hash_to_mixed_x86_box_owned");               // consume the replacement into one owned Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the boxed replacement across entry reconstruction
    emitter.label("__rt_hash_set_invoker_reference_mixed_ready_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash table after optional boxing
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current probe index
    hash_layout::emit_entry_address(emitter, "r12", "r10", "r11");
    emitter.instruction("mov r14, QWORD PTR [r12 + 24]");                       // reload the boxed invoker marker
    elephc_memoir::layout::emit_load_mixed_low(emitter, "r14", "r14");
    crate::codegen_support::runtime::reference_cells::follow_mixed_reference_slot(
        emitter, "r14", "r10", "r11", "__rt_hash_set_invoker_phpref_payload_x",
    );
    elephc_memoir::reference_cells::mixed_payload::emit_publish_staged_payload(
        emitter, "r14", "r13", "[rbp - 32]", None,
    );
    emitter.instruction("call __rt_decref_mixed");                              // retire the previous caller-local Mixed owner
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // the argument hash still owns the unchanged marker wrapper

    emitter.label("__rt_hash_set_invoker_reference_raw_x");
    emitter.instruction(&format!("cmp r13, {INVOKER_ARG_TAGGED_SCALAR_SOURCE}")); // source shape12 stores a nullable integer payload and tag
    emitter.instruction("je __rt_hash_set_invoker_reference_tagged_x");         // decode boxed replacements before publishing both words
    elephc_memoir::layout::emit_load_mixed_low(emitter, "r14", "r14");
    elephc_memoir::reference_cells::mixed_payload::emit_publish_staged_payload(
        emitter, "r14", "r11", "[rbp - 32]", Some("[rbp - 40]"),
    );
    emitter.instruction("cmp r13, 1");                                          // strings own their low payload
    emitter.instruction("je __rt_hash_set_invoker_reference_release_any_x");    // release the overwritten string after publication
    emitter.instruction("cmp r13, 10");                                         // callable descriptors use their dedicated release helper
    emitter.instruction("je __rt_hash_set_invoker_reference_release_callable_x"); // retire the overwritten callable descriptor
    emitter.instruction("cmp r13, 4");                                          // arrays, hashes and objects own their low payload
    emitter.instruction("jb __rt_hash_set_reference_done_x");                   // scalar caller slots need no release
    emitter.instruction("cmp r13, 6");                                          // tag 7 was handled by the boxed-Mixed path above
    emitter.instruction("jbe __rt_hash_set_invoker_reference_release_any_x");   // release tags 4 through 6 uniformly
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // null and other non-owning tags are complete
    emitter.label("__rt_hash_set_invoker_reference_release_any_x");
    emitter.instruction("call __rt_decref_any");                                // release the overwritten heap payload after publication
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // return the unchanged argument hash
    emitter.label("__rt_hash_set_invoker_reference_release_callable_x");
    emitter.instruction("call __rt_callable_descriptor_release");               // release the overwritten callable after publication
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // return the unchanged argument hash

    emitter.label("__rt_hash_set_invoker_reference_tagged_x");
    elephc_memoir::layout::emit_load_mixed_low(emitter, "r14", "r14");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // inspect the replacement PHP representation
    emitter.instruction("cmp rax, 7");                                          // a Mixed replacement owns a box that must be decoded
    emitter.instruction("je __rt_hash_set_invoker_reference_tagged_box_x");     // preserve the boxed value until both words are published
    crate::codegen_support::callable_invoker_args::emit_tagged_scalar_ref_assignment_guard(
        emitter, "rax", "__rt_hash_set_invoker_reference_tagged_valid_x",
        "__rt_hash_set_invoker_reference_tagged_invalid_x",
    );
    emitter.label("__rt_hash_set_invoker_reference_tagged_invalid_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // consume the rejected direct payload with its original PHP tag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // preserve string lengths and other high words during cleanup
    abi::emit_call_label(emitter, "__rt_hash_to_mixed_x86_box_owned");
    super::container_refusal_owner::emit_throw(emitter);
    emitter.label("__rt_hash_set_invoker_reference_tagged_valid_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // load the direct nullable-integer payload
    elephc_memoir::reference_cells::mixed_payload::emit_publish_payload_pair(emitter, "r14", "rdi", "rax");
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // the marker remains owned by the argument hash
    emitter.label("__rt_hash_set_invoker_reference_tagged_box_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // load the replacement box owned by this write
    abi::emit_push_reg_pair(emitter, "rax", "r14");
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    crate::codegen_support::callable_invoker_args::emit_tagged_scalar_ref_assignment_guard(
        emitter, "rax", "__rt_hash_set_invoker_reference_tagged_box_valid_x",
        "__rt_hash_set_invoker_reference_tagged_box_invalid_x",
    );
    emitter.label("__rt_hash_set_invoker_reference_tagged_box_invalid_x");
    abi::emit_pop_reg_pair(emitter, "rax", "r14");
    super::container_refusal_owner::emit_throw(emitter);
    emitter.label("__rt_hash_set_invoker_reference_tagged_box_valid_x");
    abi::emit_load_temporary_stack_slot(emitter, "r14", 8);
    elephc_memoir::reference_cells::mixed_payload::emit_publish_payload_pair(emitter, "r14", "rdi", "rax");
    abi::emit_pop_reg_pair(emitter, "rax", "r14");
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.instruction("jmp __rt_hash_set_reference_done_x");                  // retire the consumed box without changing marker identity

    emitter.label("__rt_hash_set_reference_write_x");
    emitter.instruction("mov r13, QWORD PTR [rbp - 48]");                       // reload the replacement runtime value tag
    emitter.instruction("cmp r13, 7");                                          // is the replacement already an owned boxed Mixed cell?
    emitter.instruction("je __rt_hash_set_reference_release_x");                // an owned box transfers straight into the reference cell
    emitter.instruction("mov rax, r13");                                        // pass the replacement runtime value tag to the owned-box helper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the replacement low payload word to the owned-box helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the replacement high payload word to the owned-box helper
    emitter.instruction("call __rt_hash_to_mixed_x86_box_owned");               // box the replacement without adding a retain
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the boxed replacement as the new reference payload

    emitter.label("__rt_hash_set_reference_release_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the boxing helper
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current probe index for entry-address reconstruction
    hash_layout::emit_entry_address(emitter, "r12", "r10", "r11");
    emitter.instruction("mov r10, QWORD PTR [r12 + 24]");                       // load the managed reference cell this entry owns
    emitter.instruction("mov r13, QWORD PTR [rbp - 32]");                       // load the boxed replacement payload
    elephc_memoir::reference_cells::emit_replace_boxed_child(emitter, "r10", "r13", "r11");
    emitter.label("__rt_hash_set_reference_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the unchanged hash-table pointer after a write-through
    emitter.instruction("mov r14, QWORD PTR [rbp - 88]");                       // restore the caller's r14 before leaving the write-through path
    emitter.instruction("mov r13, QWORD PTR [rbp - 80]");                       // restore the caller's r13 before leaving the write-through path
    emitter.instruction("mov r12, QWORD PTR [rbp - 72]");                       // restore the caller's r12 before leaving the write-through path
    emitter.instruction("add rsp, 96");                                         // release the local spill area before leaving the write-through path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // value_lo, value_hi and value_tag keep describing the cell

    emitter.label("__rt_hash_set_write_value_x");
    hash_bucket::emit_publish_payload(emitter);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the unchanged hash-table pointer after an in-place value update
    emitter.instruction("mov r14, QWORD PTR [rbp - 88]");                       // restore the caller's r14 before leaving the update path
    emitter.instruction("mov r13, QWORD PTR [rbp - 80]");                       // restore the caller's r13 before leaving the update path
    emitter.instruction("mov r12, QWORD PTR [rbp - 72]");                       // restore the caller's r12 before leaving the update path
    emitter.instruction("add rsp, 96");                                         // release the local spill area before leaving the update path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the insertion caller
    emitter.instruction("ret");                                                 // return to the caller with the existing hash-table pointer in rax

    emit_hash_set_value_linux_x86_64(emitter);
}

/// Emits the x86_64 `__rt_hash_set_value`, mirroring [`emit_hash_set_value_aarch64`].
fn emit_hash_set_value_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_set_value ---");
    emitter.label_global("__rt_hash_set_value");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the split and probe
    emitter.instruction("mov rbp, rsp");                                        // establish the detach frame
    emitter.instruction("sub rsp, 64");                                         // reserve slots for the forwarded arguments and the unique table
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save key_lo across the split and probe
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save key_hi across the split and probe
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save value_lo across the split and probe
    emitter.instruction("mov QWORD PTR [rbp - 32], r8");                        // save value_hi across the split and probe
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save value_tag across the split and probe
    emitter.instruction("call __rt_hash_ensure_unique");                        // split a shared table before detaching anything in it
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the unique table pointer
    emitter.instruction("mov rdi, rax");                                        // probe the unique table for the destination key
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload key_lo for the entry probe
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload key_hi for the entry probe
    emitter.instruction("call __rt_hash_get");                                  // rax = found, r8 = matching entry address
    emitter.instruction("test rax, rax");                                       // did the destination already contain this key?
    emitter.instruction("jz __rt_hash_set_value_forward_x");                    // a missing key is an ordinary insert
    persistent_references::emit_value_detach_probe(emitter);
    hash_bucket::emit_detach_reference_owner(emitter);
    emitter.label("__rt_hash_set_value_forward_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // forward the unique table pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // forward key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // forward key_hi
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // forward value_lo
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // forward value_hi
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // forward value_tag
    emitter.instruction("add rsp, 64");                                         // release the detach frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_hash_set");                                   // the entry is now ordinary, so the shared path applies
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Mixed variadic hashes preserve boxed invoker markers and update caller storage on every ABI.
    #[test]
    fn hash_set_writes_through_boxed_invoker_reference_markers_on_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_hash_set(&mut emitter);
            let assembly = emitter.output();
            let prelookup = assembly.find("__rt_hash_get").unwrap();
            let cow = assembly.find("__rt_hash_ensure_unique").unwrap();
            assert!(prelookup < cow, "{name}: shared nullable references must refuse writes before COW consumes the caller owner");
            let update = assembly.split("__rt_hash_set_update:\n").nth(1).unwrap();
            let (marker_branch, write_label, publish, release) = if name == "linux-x86_64" {
                (
                    "je __rt_hash_set_invoker_reference_write_x",
                    "__rt_hash_set_invoker_reference_write_x:",
                    "mov QWORD PTR [r14], r13",
                    "call __rt_decref_mixed",
                )
            } else {
                (
                    "b.eq __rt_hash_set_invoker_reference_write",
                    "__rt_hash_set_invoker_reference_write:",
                    "str x13, [x14]",
                    "bl __rt_decref_mixed",
                )
            };
            assert!(update.contains(marker_branch), "{name}: missing nested marker detection");
            let write_through = update.split(write_label).nth(1).unwrap();
            assert!(write_through.contains("__rt_hash_to_mixed"), "{name}: replacement must be boxed for Mixed caller storage");
            assert!(
                write_through.find(publish).unwrap() < write_through.find(release).unwrap(),
                "{name}: publish replacement before releasing the previous Mixed owner"
            );
            let (tagged_label, tagged_store) = if name == "linux-x86_64" {
                ("__rt_hash_set_invoker_reference_tagged_x:", "mov QWORD PTR [r14 + 8], rax")
            } else {
                ("__rt_hash_set_invoker_reference_tagged:", "str x0, [x14, #8]")
            };
            let tagged = write_through.split(tagged_label).nth(1).unwrap();
            assert!(tagged.contains(tagged_store), "{name}: nullable sources publish their actual PHP tag");
            assert!(tagged.contains("__rt_mixed_unbox"), "{name}: boxed replacements must be decoded");
            assert!(tagged.contains("__rt_decref_mixed"), "{name}: the consumed replacement box must retire");
            let suffix = if name == "linux-x86_64" { "_x" } else { "" };
            for shape in ["tagged", "tagged_box"] {
                let valid = format!("__rt_hash_set_invoker_reference_{shape}_valid{suffix}:");
                let invalid = format!("__rt_hash_set_invoker_reference_{shape}_invalid{suffix}:");
                let guard_end = tagged.find(&invalid).unwrap();
                let publication = tagged.find(&valid).unwrap();
                let rejected = &tagged[guard_end..publication];
                assert!(rejected.find("__rt_tagged_reference_assignment_error_new").unwrap()
                    < rejected.find("__rt_decref_mixed").unwrap(),
                    "{name}: {shape} must root the refusal before destructor cleanup");
                assert!(rejected.find("__rt_decref_mixed").unwrap()
                    < rejected.find("__rt_throw_current").unwrap(),
                    "{name}: {shape} must release a rejected replacement before throwing");
                assert!(!rejected.contains(tagged_store), "{name}: {shape} modified the caller on refusal");
                let guard = &tagged[..guard_end];
                for tag in [0, 8] {
                    let compare = if name == "linux-x86_64" { format!("cmp rax, {tag}") }
                        else { format!("cmp x0, #{tag}") };
                    assert!(guard.contains(&compare), "{name}: {shape} accepts only integer and null tags");
                }
            }
        }
    }
}
