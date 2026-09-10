//! Purpose:
//! Emits the AArch64 stable linked-list merge sort used by hash-backed PHP arrays.
//!
//! Called from:
//! - `super::emit_hash_sort()` when the compilation target is AArch64.
//!
//! Key details:
//! - The bottom-up merge sort relinks only entry `prev`/`next` indices and uses no heap
//!   allocation, preserving bucket addresses, payload ownership, and COW boundaries.
//! - Equal operands are always taken from the left run, preserving PHP 8 sort stability.

use crate::codegen_support::runtime::arrays::hash_layout;
use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::sentinels::NULL_SENTINEL;

/// Emits the AArch64 entry stubs, merge-sort engine, comparator, and operand reader.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_entry_points(emitter);
    emit_sort_links(emitter);
    emit_compare_entries(emitter);
    emit_sort_triple(emitter);
}

/// Emits the four entry stubs that select a mode and tail-branch into the shared engine.
fn emit_entry_points(emitter: &mut Emitter) {
    for (label, mode, description) in super::entry_points() {
        emitter.blank();
        emitter.comment(&format!("--- runtime: {} ({}) ---", label, description));
        emitter.label_global(label);
        emitter.instruction(&format!("mov x1, #{}", mode));                     // select the sort mode
        emitter.instruction("b __rt_hash_sort_links");                          // enter the shared relinking engine
    }
}

/// Emits the allocation-free bottom-up merge sort over a hash table's order links.
///
/// Input `x0` is the table and `x1` is the mode. The 128-byte frame stores the table,
/// entries base, mode, run width, merge cursors and sizes, output head/tail, and merge
/// count. Entry indices use `-1` as the end-of-chain sentinel.
fn emit_sort_links(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_sort_links ---");
    emitter.label_global("__rt_hash_sort_links");

    emitter.instruction("cbz x0, __rt_hsort_ret");                              // null tables have no links to reorder
    abi::emit_load_int_immediate(emitter, "x9", NULL_SENTINEL);
    emitter.instruction("cmp x0, x9");                                          // test the in-band null-container sentinel
    emitter.instruction("b.eq __rt_hsort_ret");                                 // sentinel-null tables have no header
    emitter.instruction("ldr x9, [x0]");                                        // load the live entry count
    emitter.instruction("cmp x9, #2");                                          // fewer than two entries are already ordered
    emitter.instruction("b.ge __rt_hsort_begin");                               // allocate a frame only when sorting is needed
    emitter.label("__rt_hsort_ret");
    emitter.instruction("ret");                                                 // return without mutating the table

    emitter.label("__rt_hsort_begin");
    emitter.instruction("sub sp, sp, #128");                                    // allocate the aligned merge-sort frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the merge-sort frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the hash-table pointer
    hash_layout::emit_entries(emitter, "x9", "x0");
    emitter.instruction("str x9, [sp, #8]");                                    // save the entries-region base
    emitter.instruction("str x1, [sp, #16]");                                   // save direction and key/value mode bits
    emitter.instruction("mov x9, #1");                                          // seed the first pass with one-entry runs
    emitter.instruction("str x9, [sp, #24]");                                   // save the current run width

    emitter.label("__rt_hsort_pass");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the hash-table pointer
    emitter.instruction("ldr x10, [x9, #24]");                                  // start the pass at the current list head
    emitter.instruction("str x10, [sp, #32]");                                  // p = first left-run entry
    emitter.instruction("mov x10, #-1");                                        // initialize an empty output chain
    emitter.instruction("str x10, [sp, #64]");                                  // new head = none
    emitter.instruction("str x10, [sp, #72]");                                  // new tail = none
    emitter.instruction("str xzr, [sp, #80]");                                  // merge count = zero

    emitter.label("__rt_hsort_run");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the left-run cursor
    emitter.instruction("cmn x9, #1");                                          // did the pass consume the entire source chain?
    emitter.instruction("b.eq __rt_hsort_pass_done");                           // publish the completed pass
    emitter.instruction("ldr x10, [sp, #80]");                                  // reload the merge count
    emitter.instruction("add x10, x10, #1");                                    // count this pair of runs
    emitter.instruction("str x10, [sp, #80]");                                  // retain the count for termination detection
    emitter.instruction("str x9, [sp, #40]");                                   // q starts at p before splitting the runs
    emitter.instruction("str xzr, [sp, #48]");                                  // left-run size starts at zero

    emitter.label("__rt_hsort_split");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the discovered left-run size
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current run width
    emitter.instruction("cmp x9, x10");                                         // has the left run reached the requested width?
    emitter.instruction("b.ge __rt_hsort_split_done");                          // q now starts the right run
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the split cursor
    emitter.instruction("cmn x10, #1");                                         // did the source chain end inside the left run?
    emitter.instruction("b.eq __rt_hsort_split_done");                          // keep the shorter final run
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the entries-region base
    emitter.instruction("add x12, x11, x10, lsl #6");                           // address the split-cursor entry
    emitter.instruction("ldr x10, [x12, #56]");                                 // advance through the original next link
    emitter.instruction("str x10, [sp, #40]");                                  // q = next entry
    emitter.instruction("add x9, x9, #1");                                      // include the traversed entry in the left run
    emitter.instruction("str x9, [sp, #48]");                                   // persist the left-run size
    emitter.instruction("b __rt_hsort_split");                                  // continue locating the right run

    emitter.label("__rt_hsort_split_done");
    emitter.instruction("ldr x9, [sp, #24]");                                   // right runs are bounded by the same width
    emitter.instruction("str x9, [sp, #56]");                                   // save remaining right-run capacity

    emitter.label("__rt_hsort_merge");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload remaining left-run entries
    emitter.instruction("cbnz x9, __rt_hsort_have_left");                       // a nonempty left run needs comparison
    emitter.instruction("ldr x10, [sp, #56]");                                  // reload remaining right-run capacity
    emitter.instruction("cbz x10, __rt_hsort_run_done");                        // both runs are exhausted
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the right-run cursor
    emitter.instruction("cmn x11, #1");                                         // did the source chain end before the width bound?
    emitter.instruction("b.eq __rt_hsort_run_done");                            // no right entry remains to append
    emitter.instruction("b __rt_hsort_take_right");                             // only the right run has an entry

    emitter.label("__rt_hsort_have_left");
    emitter.instruction("ldr x10, [sp, #56]");                                  // reload remaining right-run capacity
    emitter.instruction("cbz x10, __rt_hsort_take_left");                       // an exhausted right run leaves the left entry
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the right-run cursor
    emitter.instruction("cmn x11, #1");                                         // is there a concrete right-run entry?
    emitter.instruction("b.eq __rt_hsort_take_left");                           // a short final right run leaves the left entry
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the entries-region base
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the left entry index
    emitter.instruction("add x0, x10, x9, lsl #6");                             // pass the left entry address
    emitter.instruction("add x1, x10, x11, lsl #6");                            // pass the right entry address
    emitter.instruction("ldr x2, [sp, #16]");                                   // pass the comparison mode
    emitter.instruction("bl __rt_hash_sort_compare_entries");                   // compare the two run heads
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the direction bit after the call
    emitter.instruction("tbnz x9, #0, __rt_hsort_choose_desc");                 // descending reverses the choice
    emitter.instruction("cmp x0, #0");                                          // compare left against right for ascending order
    emitter.instruction("b.le __rt_hsort_take_left");                           // take left on equality to preserve stability
    emitter.instruction("b __rt_hsort_take_right");                             // the right operand sorts first
    emitter.label("__rt_hsort_choose_desc");
    emitter.instruction("cmp x0, #0");                                          // compare left against right for descending order
    emitter.instruction("b.ge __rt_hsort_take_left");                           // take left on equality to preserve stability

    emitter.label("__rt_hsort_take_right");
    emitter.instruction("ldr x9, [sp, #40]");                                   // select the current right-run entry
    emitter.instruction("str x9, [sp, #88]");                                   // save the selected entry index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the entries-region base
    emitter.instruction("add x11, x10, x9, lsl #6");                            // address the selected entry
    emitter.instruction("ldr x12, [x11, #56]");                                 // read its original successor before relinking
    emitter.instruction("str x12, [sp, #40]");                                  // advance the right-run cursor
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload remaining right-run capacity
    emitter.instruction("sub x9, x9, #1");                                      // consume one right-run entry
    emitter.instruction("str x9, [sp, #56]");                                   // persist the remaining capacity
    emitter.instruction("b __rt_hsort_append");                                 // append the selected entry

    emitter.label("__rt_hsort_take_left");
    emitter.instruction("ldr x9, [sp, #32]");                                   // select the current left-run entry
    emitter.instruction("str x9, [sp, #88]");                                   // save the selected entry index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the entries-region base
    emitter.instruction("add x11, x10, x9, lsl #6");                            // address the selected entry
    emitter.instruction("ldr x12, [x11, #56]");                                 // read its original successor before relinking
    emitter.instruction("str x12, [sp, #32]");                                  // advance the left-run cursor
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload remaining left-run entries
    emitter.instruction("sub x9, x9, #1");                                      // consume one left-run entry
    emitter.instruction("str x9, [sp, #48]");                                   // persist the remaining size

    emitter.label("__rt_hsort_append");
    emitter.instruction("ldr x9, [sp, #88]");                                   // reload the selected entry index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the entries-region base
    emitter.instruction("add x11, x10, x9, lsl #6");                            // address the selected entry
    emitter.instruction("ldr x12, [sp, #72]");                                  // reload the output tail index
    emitter.instruction("str x12, [x11, #48]");                                 // selected entry prev = old output tail
    emitter.instruction("cmn x12, #1");                                         // is this the first output entry of the pass?
    emitter.instruction("b.eq __rt_hsort_first");                               // publish it as the output head
    emitter.instruction("add x13, x10, x12, lsl #6");                           // address the previous output tail
    emitter.instruction("str x9, [x13, #56]");                                  // previous tail next = selected entry
    emitter.instruction("b __rt_hsort_tail");                                   // retain the existing output head
    emitter.label("__rt_hsort_first");
    emitter.instruction("str x9, [sp, #64]");                                   // output head = first selected entry
    emitter.label("__rt_hsort_tail");
    emitter.instruction("str x9, [sp, #72]");                                   // output tail = selected entry
    emitter.instruction("b __rt_hsort_merge");                                  // merge the remaining run entries

    emitter.label("__rt_hsort_run_done");
    emitter.instruction("ldr x9, [sp, #40]");                                   // q now points to the following run pair
    emitter.instruction("str x9, [sp, #32]");                                   // p = q for the next merge
    emitter.instruction("b __rt_hsort_run");                                    // process the next pair of runs

    emitter.label("__rt_hsort_pass_done");
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the completed output tail
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the entries-region base
    emitter.instruction("add x11, x10, x9, lsl #6");                            // address the completed output tail
    emitter.instruction("mov x12, #-1");                                        // materialize the end-of-chain sentinel
    emitter.instruction("str x12, [x11, #56]");                                 // terminate the rebuilt forward chain
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the hash-table pointer
    emitter.instruction("ldr x10, [sp, #64]");                                  // reload the completed output head
    emitter.instruction("str x10, [x9, #24]");                                  // publish the pass head in the table header
    emitter.instruction("ldr x10, [sp, #72]");                                  // reload the completed output tail
    emitter.instruction("str x10, [x9, #32]");                                  // publish the pass tail in the table header
    emitter.instruction("ldr x10, [sp, #80]");                                  // reload the number of merged run pairs
    emitter.instruction("cmp x10, #1");                                         // one run means the list is fully sorted
    emitter.instruction("b.le __rt_hsort_finish");                              // finish after the converged pass
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current run width
    emitter.instruction("lsl x10, x10, #1");                                    // double the run width for the next pass
    emitter.instruction("str x10, [sp, #24]");                                  // persist the wider run size
    emitter.instruction("b __rt_hsort_pass");                                   // merge adjacent wider runs

    emitter.label("__rt_hsort_finish");
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the merge-sort frame
    emitter.instruction("ret");                                                 // return with the table reordered in place
}

/// Emits the comparator that reads two entry operands and applies the selected PHP rule.
fn emit_compare_entries(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_sort_compare_entries ---");
    emitter.label_global("__rt_hash_sort_compare_entries");
    emitter.instruction("sub sp, sp, #80");                                     // allocate an aligned comparison frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the comparison frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right entry address
    emitter.instruction("str x2, [sp, #8]");                                    // save the comparison mode
    emitter.instruction("mov x1, x2");                                          // pass the mode to the left operand reader
    emitter.instruction("bl __rt_hash_sort_triple");                            // materialize the left comparison triple
    emitter.instruction("str x0, [sp, #16]");                                   // save the left runtime tag
    emitter.instruction("str x1, [sp, #24]");                                   // save the left low payload word
    emitter.instruction("str x2, [sp, #32]");                                   // save the left high payload word
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the right entry address
    emitter.instruction("ldr x1, [sp, #8]");                                    // pass the comparison mode
    emitter.instruction("bl __rt_hash_sort_triple");                            // materialize the right comparison triple
    emitter.instruction("str x0, [sp, #40]");                                   // save the right runtime tag
    emitter.instruction("str x1, [sp, #48]");                                   // save the right low payload word
    emitter.instruction("str x2, [sp, #56]");                                   // save the right high payload word
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the key/value mode bit
    emitter.instruction("tbnz x9, #1, __rt_hsort_cmp_value");                   // value sorts use PHP comparison
    emitter.instruction("ldr x0, [sp, #24]");                                   // pass the left normalized key payload
    emitter.instruction("ldr x1, [sp, #32]");                                   // pass the left key length or integer sentinel
    emitter.instruction("ldr x2, [sp, #48]");                                   // pass the right normalized key payload
    emitter.instruction("ldr x3, [sp, #56]");                                   // pass the right key length or integer sentinel
    emitter.instruction("bl __rt_key_compare_regular");                         // apply exact SORT_REGULAR key ordering
    emitter.instruction("b __rt_hsort_cmp_done");                               // skip the general value comparator
    emitter.label("__rt_hsort_cmp_value");
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass the left runtime tag
    emitter.instruction("ldr x1, [sp, #24]");                                   // pass the left low payload word
    emitter.instruction("ldr x2, [sp, #32]");                                   // pass the left high payload word
    emitter.instruction("ldr x3, [sp, #40]");                                   // pass the right runtime tag
    emitter.instruction("ldr x4, [sp, #48]");                                   // pass the right low payload word
    emitter.instruction("ldr x5, [sp, #56]");                                   // pass the right high payload word
    emitter.instruction("bl __rt_php_compare");                                 // apply PHP's general value comparison table
    emitter.label("__rt_hsort_cmp_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the comparison frame
    emitter.instruction("ret");                                                 // return the signed comparison result in x0
}

/// Emits the operand reader that maps an entry key or value to a PHP comparison triple.
fn emit_sort_triple(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_sort_triple ---");
    emitter.label_global("__rt_hash_sort_triple");
    emitter.instruction("tbnz x1, #1, __rt_hsort_triple_value");                // select entry value for value sorts
    emitter.instruction("ldr x2, [x0, #16]");                                   // load key length or integer-key sentinel
    emitter.instruction("ldr x1, [x0, #8]");                                    // load key pointer or integer payload
    emitter.instruction("cmn x2, #1");                                          // test for a normalized integer key
    emitter.instruction("b.ne __rt_hsort_triple_key_str");                      // string keys retain pointer and length
    emitter.instruction("mov x0, #0");                                          // runtime tag zero denotes an integer
    emitter.instruction("mov x2, #-1");                                         // retain the integer-key sentinel
    emitter.instruction("ret");                                                 // return the integer key triple
    emitter.label("__rt_hsort_triple_key_str");
    emitter.instruction("mov x0, #1");                                          // runtime tag one denotes a string
    emitter.instruction("ret");                                                 // return the borrowed string key triple
    emitter.label("__rt_hsort_triple_value");
    emitter.instruction("ldr x3, [x0, #40]");                                   // load the per-entry value tag
    emitter.instruction("ldr x1, [x0, #24]");                                   // load the low value payload word
    emitter.instruction("ldr x2, [x0, #32]");                                   // load the high value payload word
    emitter.instruction("cmp x3, #7");                                          // test whether the entry holds a boxed Mixed cell
    emitter.instruction("b.eq __rt_hsort_triple_value_boxed");                  // boxed values need unboxing
    emitter.instruction("mov x0, x3");                                          // publish the concrete runtime tag
    emitter.instruction("ret");                                                 // return the borrowed unboxed value triple
    emitter.label("__rt_hsort_triple_value_boxed");
    emitter.instruction("mov x0, x1");                                          // pass the borrowed Mixed cell to the unboxer
    emitter.instruction("b __rt_mixed_unbox");                                  // tail-branch so the peeled triple returns directly
}
