//! Purpose:
//! Emits the AArch64 bounded JSON integer-tree probe, allocator, and writer.
//! Produces one exact transient string without using the shared concat scratch buffer.
//!
//! Called from:
//! - `super::emit_json_integer_tree()` and `super::emit_json_integer_tree_entry()`.
//!
//! Key details:
//! - The probe accepts packed integer, packed array, and boxed Mixed integer/array slots only.
//! - No allocation or observable JSON state change occurs before the complete tree is accepted.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::sentinels::{
    emit_branch_if_null_container, CONCAT_TEMP_HEAP_KIND,
};

const PROBE_FRAME: usize = 64;
const WRITE_FRAME: usize = 96;
const TRY_FRAME: usize = 48;

/// Emits all AArch64 helper bodies used by the integer-tree fast path.
pub(super) fn emit_helpers(emitter: &mut Emitter) {
    emit_integer_width(emitter);
    emit_probe(emitter);
    emit_writer(emitter);
    emit_try(emitter);
}

/// Emits the top-level flag and depth guards plus the fast-path call site.
pub(super) fn emit_entry(emitter: &mut Emitter, prefix: &str) {
    let fallback = format!("{prefix}_fallback");
    let unsupported = format!("{prefix}_unsupported");
    let attempt = format!("{prefix}_attempt");
    let max_scratch_integer_slots = (emitter.concat_capacity - 1) / 21;

    // -- keep flags, recursive encodes, and unsupported trees on the general encoder --
    abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x9, [x9]");                                        // load the current JSON option mask
    emitter.instruction(&format!("cbnz x9, {fallback}"));                       // flags need the general formatter semantics
    abi::emit_symbol_address(emitter, "x9", "_json_active_depth");
    emitter.instruction("ldr x9, [x9]");                                        // load the current recursive container depth
    emitter.instruction(&format!("cbnz x9, {fallback}"));                       // only the top-level container owns the exact result buffer

    // -- keep provably bounded direct integer arrays on the existing one-pass encoder --
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed array value type from the heap header
    emitter.instruction("lsr x9, x9, #8");                                      // move the value type into the low bits
    emitter.instruction("and x9, x9, #0x7f");                                   // discard copy-on-write and other header flags
    emitter.instruction(&format!("cbnz x9, {attempt}"));                        // nested and boxed layouts still need the complete proof
    emitter.instruction("ldr x9, [x0]");                                        // load the direct integer array length
    abi::emit_load_int_immediate(emitter, "x10", max_scratch_integer_slots as i64);
    emitter.instruction("cmp x9, x10");                                         // test the twenty-byte-per-value upper bound against scratch capacity
    emitter.instruction(&format!("b.ls {fallback}"));                           // bounded direct arrays retain the faster single-pass encoder
    emitter.label(&attempt);

    abi::emit_frame_prologue(emitter, 32);
    emitter.instruction("str x0, [x29, #-8]");                                  // preserve the source array across the eligibility attempt
    emitter.instruction("bl __rt_json_integer_tree_try");                       // return x1/x2 on success or x1=0 when unsupported
    emitter.instruction(&format!("cbz x1, {unsupported}"));                     // restore the source and continue through the old encoder on rejection
    abi::emit_frame_restore(emitter, 32);
    emitter.instruction("ret");                                                 // return the exact heap-backed JSON slice

    emitter.label(&unsupported);
    emitter.instruction("ldr x0, [x29, #-8]");                                  // restore the original array pointer for the existing encoder
    abi::emit_frame_restore(emitter, 32);
    emitter.label(&fallback);
}

/// Emits a leaf helper that returns the decimal width of one signed integer.
fn emit_integer_width(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_integer_width ---");
    emitter.label_global("__rt_json_integer_width");

    // -- derive the unsigned magnitude and optional sign width --
    emitter.instruction("cmp x0, #0");                                          // inspect the sign before deriving an unsigned magnitude
    emitter.instruction("cneg x9, x0, lt");                                     // take a wrapping magnitude so PHP_INT_MIN remains representable
    emitter.instruction("cset x10, lt");                                        // reserve one byte when the value is negative

    // -- classify the magnitude without repeated integer division --
    for (digits, threshold) in (1_i64..=18).zip([
        10_i64,
        100,
        1_000,
        10_000,
        100_000,
        1_000_000,
        10_000_000,
        100_000_000,
        1_000_000_000,
        10_000_000_000,
        100_000_000_000,
        1_000_000_000_000,
        10_000_000_000_000,
        100_000_000_000_000,
        1_000_000_000_000_000,
        10_000_000_000_000_000,
        100_000_000_000_000_000,
        1_000_000_000_000_000_000,
    ]) {
        emitter.instruction(&format!("mov x11, #{digits}"));                    // retain the width selected when the next threshold contains the magnitude
        abi::emit_load_int_immediate(emitter, "x12", threshold);
        emitter.instruction("cmp x9, x12");                                     // compare the magnitude with the next decimal power
        emitter.instruction("b.lo __rt_json_integer_width_done");               // finish as soon as the magnitude fits this width
    }
    emitter.instruction("mov x11, #19");                                        // every remaining signed magnitude occupies nineteen digits
    emitter.label("__rt_json_integer_width_done");
    emitter.instruction("add x0, x11, x10");                                    // return digits plus the optional sign byte
    emitter.instruction("ret");                                                 // leaf helper preserves no frame state
}

/// Emits the recursive, side-effect-free shape and exact-size probe.
fn emit_probe(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_integer_tree_probe ---");
    emitter.label_global("__rt_json_integer_tree_probe");

    // -- validate the root container and initialize its exact byte count --
    abi::emit_frame_prologue(emitter, PROBE_FRAME);
    emitter.instruction("str x0, [x29, #-8]");                                  // save the candidate indexed-array pointer
    emitter.instruction("str x1, [x29, #-16]");                                 // save its one-based JSON nesting depth
    emit_branch_if_null_container(emitter, "x0", "x9", "__rt_json_integer_tree_probe_unsupported");
    abi::emit_symbol_address(emitter, "x9", "_json_depth_limit");
    emitter.instruction("ldr x9, [x9]");                                        // load the caller-selected JSON depth limit
    emitter.instruction("cmp x1, x9");                                          // would this container exceed the PHP depth budget?
    emitter.instruction("b.gt __rt_json_integer_tree_probe_unsupported");       // let the general encoder publish the depth error
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load packed heap kind and indexed-array value type
    emitter.instruction("and x10, x9, #0xff");                                  // isolate the uniform top-level heap kind
    emitter.instruction("cmp x10, #2");                                         // kind 2 identifies indexed-array storage
    emitter.instruction("b.ne __rt_json_integer_tree_probe_unsupported");       // hashes and other containers need the general encoder
    emitter.instruction("ldr x10, [x0]");                                       // load the logical array length
    emitter.instruction("tbnz x10, #63, __rt_json_integer_tree_probe_unsupported"); // reject invalid negative metadata defensively
    emitter.instruction("str x10, [x29, #-32]");                                // retain the length for the element loop
    emitter.instruction("mov x11, #2");                                         // every accepted array contributes '[' and ']'
    emitter.instruction("str x11, [x29, #-24]");                                // initialize the exact serialized byte count
    emitter.instruction("cbz x10, __rt_json_integer_tree_probe_success");       // every valid empty indexed array serializes as []
    emitter.instruction("ldr x11, [x0, #16]");                                  // load the array element stride
    emitter.instruction("cmp x11, #8");                                         // accepted payload families occupy one machine word per slot
    emitter.instruction("b.ne __rt_json_integer_tree_probe_unsupported");       // wider layouts belong to the general encoder
    emitter.instruction("lsr x9, x9, #8");                                      // move the packed value type into the low bits
    emitter.instruction("and x9, x9, #0x7f");                                   // discard copy-on-write and other header flags
    emitter.instruction("cmp x9, #0");                                          // direct integer slots are eligible
    emitter.instruction("b.eq __rt_json_integer_tree_probe_type_ok");           // accept direct integer storage without a per-slot tag
    emitter.instruction("cmp x9, #4");                                          // direct indexed-array slots are eligible
    emitter.instruction("b.eq __rt_json_integer_tree_probe_type_ok");           // accept direct nested indexed-array storage
    emitter.instruction("cmp x9, #7");                                          // boxed Mixed slots need per-cell tag checks
    emitter.instruction("b.ne __rt_json_integer_tree_probe_unsupported");       // reject every remaining payload layout
    emitter.label("__rt_json_integer_tree_probe_type_ok");
    emitter.instruction("str x9, [x29, #-40]");                                 // save the accepted array value type
    emitter.instruction("str xzr, [x29, #-48]");                                // initialize the element index

    // -- classify and size every accepted element without changing runtime state --
    emitter.label("__rt_json_integer_tree_probe_loop");
    emitter.instruction("ldr x10, [x29, #-48]");                                // reload the current element index
    emitter.instruction("ldr x11, [x29, #-32]");                                // reload the array length
    emitter.instruction("cmp x10, x11");                                        // have all elements been sized?
    emitter.instruction("b.hs __rt_json_integer_tree_probe_success");           // finish after sizing the complete array
    emitter.instruction("cbz x10, __rt_json_integer_tree_probe_value");         // the first element has no comma prefix
    emitter.instruction("ldr x11, [x29, #-24]");                                // reload the accumulated byte count
    emitter.instruction("adds x11, x11, #1");                                   // include one comma separator
    emitter.instruction("b.cs __rt_json_integer_tree_probe_unsupported");       // reject wrapped totals before allocation
    emitter.instruction("tbnz x11, #63, __rt_json_integer_tree_probe_unsupported"); // heap sizes must remain non-negative
    emitter.instruction("str x11, [x29, #-24]");                                // publish the updated byte count

    emitter.label("__rt_json_integer_tree_probe_value");
    emitter.instruction("ldr x11, [x29, #-8]");                                 // reload the array base
    emitter.instruction("add x12, x10, #3");                                    // skip the 24-byte array header
    emitter.instruction("ldr x12, [x11, x12, lsl #3]");                         // load the one-word element payload
    emitter.instruction("ldr x9, [x29, #-40]");                                 // reload the packed array value type
    emitter.instruction("cbz x9, __rt_json_integer_tree_probe_int");            // direct integer slots need only decimal width
    emitter.instruction("cmp x9, #4");                                          // direct nested arrays recurse without boxing
    emitter.instruction("b.eq __rt_json_integer_tree_probe_array");             // recurse for direct nested indexed-array slots

    emit_branch_if_null_container(emitter, "x12", "x13", "__rt_json_integer_tree_probe_unsupported");
    emitter.instruction("ldr x13, [x12, #-8]");                                 // load the boxed-cell heap kind
    emitter.instruction("and x13, x13, #0xff");                                 // isolate the uniform kind byte
    emitter.instruction("cmp x13, #5");                                         // accepted boxed elements must be real Mixed cells
    emitter.instruction("b.ne __rt_json_integer_tree_probe_unsupported");       // reference cells and other owners keep general semantics
    emitter.instruction("ldr x13, [x12]");                                      // load the Mixed runtime value tag
    emitter.instruction("cbz x13, __rt_json_integer_tree_probe_mixed_int");     // boxed integers are eligible
    emitter.instruction("cmp x13, #4");                                         // boxed indexed arrays are eligible
    emitter.instruction("b.ne __rt_json_integer_tree_probe_unsupported");       // strings, floats, bools, nulls and objects fall back
    emitter.instruction("ldr x12, [x12, #8]");                                  // load the boxed nested-array pointer
    emitter.instruction("b __rt_json_integer_tree_probe_array");                // reuse the nested-array probe path for the boxed payload

    emitter.label("__rt_json_integer_tree_probe_mixed_int");
    emitter.instruction("ldr x0, [x12, #8]");                                   // load the boxed integer payload
    emitter.instruction("bl __rt_json_integer_width");                          // compute its exact decimal byte width
    emitter.instruction("b __rt_json_integer_tree_probe_add");                  // add the boxed integer width to the parent total

    emitter.label("__rt_json_integer_tree_probe_int");
    emitter.instruction("mov x0, x12");                                         // pass the direct integer payload to the width helper
    emitter.instruction("bl __rt_json_integer_width");                          // compute its exact decimal byte width
    emitter.instruction("b __rt_json_integer_tree_probe_add");                  // add the direct integer width to the parent total

    emitter.label("__rt_json_integer_tree_probe_array");
    emit_branch_if_null_container(emitter, "x12", "x13", "__rt_json_integer_tree_probe_unsupported");
    emitter.instruction("mov x0, x12");                                         // pass the nested indexed-array pointer
    emitter.instruction("ldr x1, [x29, #-16]");                                 // reload the current one-based nesting depth
    emitter.instruction("add x1, x1, #1");                                      // recurse at the child depth
    emitter.instruction("bl __rt_json_integer_tree_probe");                     // return the child size or zero when unsupported
    emitter.instruction("cbz x0, __rt_json_integer_tree_probe_unsupported");    // propagate child rejection to the root attempt

    emitter.label("__rt_json_integer_tree_probe_add");
    emitter.instruction("ldr x10, [x29, #-24]");                                // reload the accumulated byte count
    emitter.instruction("adds x10, x10, x0");                                   // include this scalar or child container
    emitter.instruction("b.cs __rt_json_integer_tree_probe_unsupported");       // reject unsigned addition overflow
    emitter.instruction("tbnz x10, #63, __rt_json_integer_tree_probe_unsupported"); // reject sizes the allocator reads as negative
    emitter.instruction("str x10, [x29, #-24]");                                // save the new exact total
    emitter.instruction("ldr x10, [x29, #-48]");                                // reload the element index
    emitter.instruction("add x10, x10, #1");                                    // advance to the next element
    emitter.instruction("str x10, [x29, #-48]");                                // persist the updated index
    emitter.instruction("b __rt_json_integer_tree_probe_loop");                 // size the next array slot

    // -- return the exact size or a marker that preserves the general encoder --
    emitter.label("__rt_json_integer_tree_probe_success");
    emitter.instruction("ldr x0, [x29, #-24]");                                 // return the exact non-zero serialized size
    abi::emit_frame_restore(emitter, PROBE_FRAME);
    emitter.instruction("ret");                                                 // return the admitted tree size to the caller
    emitter.label("__rt_json_integer_tree_probe_unsupported");
    emitter.instruction("mov x0, #0");                                          // zero tells the caller to use the general encoder
    abi::emit_frame_restore(emitter, PROBE_FRAME);
    emitter.instruction("ret");                                                 // return the rejection marker to the caller
}

/// Emits the accepted-tree writer into a caller-owned exact output allocation.
fn emit_writer(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_integer_tree_write ---");
    emitter.label_global("__rt_json_integer_tree_write");

    // -- initialize the caller-owned output cursor and array traversal state --
    abi::emit_frame_prologue(emitter, WRITE_FRAME);
    emitter.instruction("str x0, [x29, #-8]");                                  // save the source indexed-array pointer
    emitter.instruction("str x1, [x29, #-16]");                                 // save the current output cursor
    emitter.instruction("mov w9, #91");                                         // ASCII '['
    emitter.instruction("strb w9, [x1]");                                       // write the opening bracket
    emitter.instruction("add x1, x1, #1");                                      // advance the output cursor
    emitter.instruction("str x1, [x29, #-16]");                                 // persist the advanced cursor
    emitter.instruction("ldr x10, [x0]");                                       // load the source array length
    emitter.instruction("str x10, [x29, #-32]");                                // save it for the element loop
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load packed indexed-array metadata
    emitter.instruction("lsr x9, x9, #8");                                      // move the value type into the low bits
    emitter.instruction("and x9, x9, #0x7f");                                   // isolate the stable value type
    emitter.instruction("str x9, [x29, #-40]");                                 // save the value type for dispatch
    emitter.instruction("str xzr, [x29, #-24]");                                // initialize the element index

    // -- serialize each previously accepted slot into the exact allocation --
    emitter.label("__rt_json_integer_tree_write_loop");
    emitter.instruction("ldr x10, [x29, #-24]");                                // reload the current element index
    emitter.instruction("ldr x11, [x29, #-32]");                                // reload the array length
    emitter.instruction("cmp x10, x11");                                        // have all elements been written?
    emitter.instruction("b.hs __rt_json_integer_tree_write_close");             // close the array after its final slot
    emitter.instruction("cbz x10, __rt_json_integer_tree_write_value");         // the first element has no comma prefix
    emitter.instruction("ldr x11, [x29, #-16]");                                // reload the current output cursor
    emitter.instruction("mov w9, #44");                                         // ASCII ','
    emitter.instruction("strb w9, [x11]");                                      // write one separator byte
    emitter.instruction("add x11, x11, #1");                                    // advance past the comma
    emitter.instruction("str x11, [x29, #-16]");                                // persist the new cursor

    emitter.label("__rt_json_integer_tree_write_value");
    emitter.instruction("ldr x11, [x29, #-8]");                                 // reload the source array pointer
    emitter.instruction("add x12, x10, #3");                                    // skip the 24-byte array header
    emitter.instruction("ldr x12, [x11, x12, lsl #3]");                         // load the one-word element payload
    emitter.instruction("ldr x9, [x29, #-40]");                                 // reload the array value type
    emitter.instruction("cbz x9, __rt_json_integer_tree_write_int");            // direct integer slots format in place
    emitter.instruction("cmp x9, #4");                                          // direct nested arrays recurse into the same destination
    emitter.instruction("b.eq __rt_json_integer_tree_write_array");             // recurse for direct nested indexed-array slots
    emitter.instruction("ldr x9, [x12]");                                       // load the already-probed Mixed runtime tag
    emitter.instruction("cbnz x9, __rt_json_integer_tree_write_mixed_array");   // the only other accepted tag is indexed array 4
    emitter.instruction("ldr x12, [x12, #8]");                                  // load the boxed integer payload
    emitter.instruction("b __rt_json_integer_tree_write_int");                  // format the boxed integer through the shared path
    emitter.label("__rt_json_integer_tree_write_mixed_array");
    emitter.instruction("ldr x12, [x12, #8]");                                  // load the boxed nested-array pointer

    emitter.label("__rt_json_integer_tree_write_array");
    emitter.instruction("mov x0, x12");                                         // pass the nested indexed-array pointer
    emitter.instruction("ldr x1, [x29, #-16]");                                 // pass the current output cursor
    emitter.instruction("bl __rt_json_integer_tree_write");                     // return the cursor after the child closing bracket
    emitter.instruction("str x0, [x29, #-16]");                                 // continue writing after the nested array
    emitter.instruction("b __rt_json_integer_tree_write_next");                 // advance after the nested writer returned

    emitter.label("__rt_json_integer_tree_write_int");
    emitter.instruction("mov x0, x12");                                         // pass the accepted integer payload to the formatter
    abi::emit_frame_slot_address(emitter, "x1", 72);
    emitter.instruction("bl __rt_itoa_into");                                   // return a decimal slice inside the local 21-byte window
    emitter.instruction("ldr x10, [x29, #-16]");                                // reload the destination cursor
    emitter.instruction("mov x11, #0");                                         // initialize the formatted-slice copy index
    emitter.label("__rt_json_integer_tree_write_copy");
    emitter.instruction("cmp x11, x1");                                         // have all formatted bytes been copied?
    emitter.instruction("b.hs __rt_json_integer_tree_write_copy_done");         // stop after copying the complete decimal slice
    emitter.instruction("ldrb w12, [x0, x11]");                                 // load the next decimal byte
    emitter.instruction("strb w12, [x10, x11]");                                // write it into the exact result allocation
    emitter.instruction("add x11, x11, #1");                                    // advance the copy index
    emitter.instruction("b __rt_json_integer_tree_write_copy");                 // copy the next decimal byte
    emitter.label("__rt_json_integer_tree_write_copy_done");
    emitter.instruction("add x10, x10, x1");                                    // advance past the formatted integer
    emitter.instruction("str x10, [x29, #-16]");                                // persist the new output cursor

    emitter.label("__rt_json_integer_tree_write_next");
    emitter.instruction("ldr x10, [x29, #-24]");                                // reload the element index
    emitter.instruction("add x10, x10, #1");                                    // advance to the next slot
    emitter.instruction("str x10, [x29, #-24]");                                // persist the updated index
    emitter.instruction("b __rt_json_integer_tree_write_loop");                 // encode the next array slot

    // -- close the container and return its final output cursor --
    emitter.label("__rt_json_integer_tree_write_close");
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the final cursor before the closing bracket
    emitter.instruction("mov w9, #93");                                         // ASCII ']'
    emitter.instruction("strb w9, [x0]");                                       // close the current JSON array
    emitter.instruction("add x0, x0, #1");                                      // return the cursor just after the closing byte
    abi::emit_frame_restore(emitter, WRITE_FRAME);
    emitter.instruction("ret");                                                 // return the final output cursor to the parent writer
}

/// Emits the allocating public attempt used by both indexed-array encoders.
fn emit_try(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_integer_tree_try ---");
    emitter.label_global("__rt_json_integer_tree_try");

    // -- admit the complete tree before allocating and filling one result string --
    abi::emit_frame_prologue(emitter, TRY_FRAME);
    emitter.instruction("str x0, [x29, #-8]");                                  // preserve the root array across the size probe
    emitter.instruction("mov x1, #1");                                          // the root array occupies JSON depth one
    emitter.instruction("bl __rt_json_integer_tree_probe");                     // return the exact size or zero when the tree is unsupported
    emitter.instruction("cbz x0, __rt_json_integer_tree_try_unsupported");      // allocate only after the entire tree is admitted
    emitter.instruction("str x0, [x29, #-16]");                                 // preserve the exact byte count across allocation and writing
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate one exact output payload
    emitter.instruction(&format!("mov x9, #{}", CONCAT_TEMP_HEAP_KIND));        // mark the unowned result for one-time string adoption
    emitter.instruction("str x9, [x0, #-8]");                                   // publish the transient string heap kind
    emitter.instruction("str x0, [x29, #-24]");                                 // preserve the output base across the recursive writer
    emitter.instruction("mov x1, x0");                                          // pass the exact output base as the initial cursor
    emitter.instruction("ldr x0, [x29, #-8]");                                  // reload the admitted root array
    emitter.instruction("bl __rt_json_integer_tree_write");                     // fill the allocation without further allocation or callbacks
    emitter.instruction("ldr x1, [x29, #-24]");                                 // return the string base in the JSON string ABI
    emitter.instruction("ldr x2, [x29, #-16]");                                 // return the exact serialized length
    abi::emit_frame_restore(emitter, TRY_FRAME);
    emitter.instruction("ret");                                                 // return the completed JSON string pair

    // -- leave unsupported trees to the existing stateful encoder --
    emitter.label("__rt_json_integer_tree_try_unsupported");
    emitter.instruction("mov x1, #0");                                          // a zero result pointer requests the general encoder
    emitter.instruction("mov x2, #0");                                          // unsupported attempts have no result bytes
    abi::emit_frame_restore(emitter, TRY_FRAME);
    emitter.instruction("ret");                                                 // return the unsupported marker to the guarded call site
}
