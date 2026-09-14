//! Purpose:
//! Emits the x86_64 System V stable linked-list merge sort for hash-backed PHP arrays.
//!
//! Called from:
//! - `super::emit_hash_sort()` when the compilation target is x86_64.
//!
//! Key details:
//! - The target implementation mirrors AArch64's bottom-up merge passes and comparison
//!   contract while respecting the System V register ABI.
//! - Equal operands are taken from the left run, so relinking preserves PHP 8 stability.

use crate::codegen_support::runtime::arrays::hash_layout;
use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::sentinels::NULL_SENTINEL;

/// Emits the x86_64 entry stubs, merge-sort engine, comparator, and operand reader.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_entry_points(emitter);
    emit_sort_links(emitter);
    emit_compare_entries(emitter);
    emit_sort_triple(emitter);
}

/// Emits the four entry stubs that select a mode and tail-jump into the shared engine.
fn emit_entry_points(emitter: &mut Emitter) {
    for (label, mode, description) in super::entry_points() {
        emitter.blank();
        emitter.comment(&format!("--- runtime: {} ({}) ---", label, description));
        emitter.label_global(label);
        emitter.instruction(&format!("mov esi, {}", mode));                     // select the sort mode
        emitter.instruction("jmp __rt_hash_sort_links");                        // enter the shared relinking engine
    }
}

/// Emits the allocation-free bottom-up merge sort over a hash table's order links.
///
/// Input `rdi` is the table and `rsi` is the mode. The frame stores the table, entries
/// base, mode, run width, merge cursors and sizes, output head/tail, and merge count.
fn emit_sort_links(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_sort_links ---");
    emitter.label_global("__rt_hash_sort_links");

    emitter.instruction("test rdi, rdi");                                       // null tables have no links to reorder
    emitter.instruction("jz __rt_hsort_ret");                                   // return before dereferencing a null table
    abi::emit_load_int_immediate(emitter, "r10", NULL_SENTINEL);
    emitter.instruction("cmp rdi, r10");                                        // test the in-band null-container sentinel
    emitter.instruction("je __rt_hsort_ret");                                   // sentinel-null tables have no header
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the live entry count
    emitter.instruction("cmp r10, 2");                                          // fewer than two entries are already ordered
    emitter.instruction("jge __rt_hsort_begin");                                // allocate a frame only when sorting is needed
    emitter.label("__rt_hsort_ret");
    emitter.instruction("ret");                                                 // return without mutating the table

    emitter.label("__rt_hsort_begin");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the merge-sort frame pointer
    emitter.instruction("sub rsp, 96");                                         // allocate the aligned merge-sort frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash-table pointer
    hash_layout::emit_entries(emitter, "r10", "rdi");
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the entries-region base
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save direction and key/value mode bits
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // seed the first pass with one-entry runs

    emitter.label("__rt_hsort_pass");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer
    emitter.instruction("mov r11, QWORD PTR [r10 + 24]");                       // start the pass at the current list head
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // p = first left-run entry
    emitter.instruction("mov QWORD PTR [rbp - 72], -1");                        // new head = none
    emitter.instruction("mov QWORD PTR [rbp - 80], -1");                        // new tail = none
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // merge count = zero

    emitter.label("__rt_hsort_run");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the left-run cursor
    emitter.instruction("cmp r10, -1");                                         // did the pass consume the entire source chain?
    emitter.instruction("je __rt_hsort_pass_done");                             // publish the completed pass
    emitter.instruction("add QWORD PTR [rbp - 88], 1");                         // count this pair of runs
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // q starts at p before splitting the runs
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // left-run size starts at zero

    emitter.label("__rt_hsort_split");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the discovered left-run size
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // has the left run reached the width?
    emitter.instruction("jge __rt_hsort_split_done");                           // q now starts the right run
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the split cursor
    emitter.instruction("cmp r11, -1");                                         // did the source chain end inside the left run?
    emitter.instruction("je __rt_hsort_split_done");                            // keep the shorter final run
    emitter.instruction("mov rax, r11");                                        // copy the slot index before scaling
    emitter.instruction("shl rax, 6");                                          // convert the slot to a 64-byte entry offset
    emitter.instruction("add rax, QWORD PTR [rbp - 16]");                       // address the split-cursor entry
    emitter.instruction("mov r11, QWORD PTR [rax + 56]");                       // advance through the original next link
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // q = next entry
    emitter.instruction("add QWORD PTR [rbp - 56], 1");                         // include the traversed entry in the left run
    emitter.instruction("jmp __rt_hsort_split");                                // continue locating the right run

    emitter.label("__rt_hsort_split_done");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // right runs use the same width bound
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // save remaining right-run capacity

    emitter.label("__rt_hsort_merge");
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // does the left run still contain entries?
    emitter.instruction("jne __rt_hsort_have_left");                            // a nonempty left run needs comparison
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // does right-run capacity remain?
    emitter.instruction("je __rt_hsort_run_done");                              // both runs are exhausted
    emitter.instruction("cmp QWORD PTR [rbp - 48], -1");                        // is there a concrete right entry?
    emitter.instruction("je __rt_hsort_run_done");                              // the source chain ended early
    emitter.instruction("jmp __rt_hsort_take_right");                           // only the right run has an entry

    emitter.label("__rt_hsort_have_left");
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // does right-run capacity remain?
    emitter.instruction("je __rt_hsort_take_left");                             // an exhausted right run leaves the left entry
    emitter.instruction("cmp QWORD PTR [rbp - 48], -1");                        // is there a concrete right entry?
    emitter.instruction("je __rt_hsort_take_left");                             // a short final right run leaves the left entry
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // load the left entry index
    emitter.instruction("shl rdi, 6");                                          // convert the left index to an entry offset
    emitter.instruction("add rdi, QWORD PTR [rbp - 16]");                       // pass the left entry address
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // load the right entry index
    emitter.instruction("shl rsi, 6");                                          // convert the right index to an entry offset
    emitter.instruction("add rsi, QWORD PTR [rbp - 16]");                       // pass the right entry address
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // pass the comparison mode
    emitter.instruction("call __rt_hash_sort_compare_entries");                 // compare the two run heads
    emitter.instruction("test QWORD PTR [rbp - 24], 1");                        // inspect the descending-direction bit
    emitter.instruction("jnz __rt_hsort_choose_desc");                          // descending reverses the choice
    emitter.instruction("cmp rax, 0");                                          // compare left against right for ascending order
    emitter.instruction("jle __rt_hsort_take_left");                            // take left on equality to preserve stability
    emitter.instruction("jmp __rt_hsort_take_right");                           // the right operand sorts first
    emitter.label("__rt_hsort_choose_desc");
    emitter.instruction("cmp rax, 0");                                          // compare left against right for descending order
    emitter.instruction("jge __rt_hsort_take_left");                            // take left on equality to preserve stability

    emitter.label("__rt_hsort_take_right");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // select the current right-run entry
    emitter.instruction("mov QWORD PTR [rbp - 96], r10");                       // save the selected entry index
    emitter.instruction("mov r11, r10");                                        // copy the slot index before scaling
    emitter.instruction("shl r11, 6");                                          // convert the slot to a 64-byte entry offset
    emitter.instruction("add r11, QWORD PTR [rbp - 16]");                       // address the selected entry
    emitter.instruction("mov rax, QWORD PTR [r11 + 56]");                       // read its original successor before relinking
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // advance the right-run cursor
    emitter.instruction("sub QWORD PTR [rbp - 64], 1");                         // consume one right-run entry
    emitter.instruction("jmp __rt_hsort_append");                               // append the selected entry

    emitter.label("__rt_hsort_take_left");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // select the current left-run entry
    emitter.instruction("mov QWORD PTR [rbp - 96], r10");                       // save the selected entry index
    emitter.instruction("mov r11, r10");                                        // copy the slot index before scaling
    emitter.instruction("shl r11, 6");                                          // convert the slot to a 64-byte entry offset
    emitter.instruction("add r11, QWORD PTR [rbp - 16]");                       // address the selected entry
    emitter.instruction("mov rax, QWORD PTR [r11 + 56]");                       // read its original successor before relinking
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // advance the left-run cursor
    emitter.instruction("sub QWORD PTR [rbp - 56], 1");                         // consume one left-run entry

    emitter.label("__rt_hsort_append");
    emitter.instruction("mov r10, QWORD PTR [rbp - 96]");                       // reload the selected entry index
    emitter.instruction("mov r11, r10");                                        // copy the slot index before scaling
    emitter.instruction("shl r11, 6");                                          // convert the slot to a 64-byte entry offset
    emitter.instruction("add r11, QWORD PTR [rbp - 16]");                       // address the selected entry
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the output tail index
    emitter.instruction("mov QWORD PTR [r11 + 48], rax");                       // selected entry prev = old output tail
    emitter.instruction("cmp rax, -1");                                         // is this the first output entry of the pass?
    emitter.instruction("je __rt_hsort_first");                                 // publish it as the output head
    emitter.instruction("shl rax, 6");                                          // convert the old tail index to an entry offset
    emitter.instruction("add rax, QWORD PTR [rbp - 16]");                       // address the previous output tail
    emitter.instruction("mov QWORD PTR [rax + 56], r10");                       // previous tail next = selected entry
    emitter.instruction("jmp __rt_hsort_tail");                                 // retain the existing output head
    emitter.label("__rt_hsort_first");
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // output head = first selected entry
    emitter.label("__rt_hsort_tail");
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // output tail = selected entry
    emitter.instruction("jmp __rt_hsort_merge");                                // merge the remaining run entries

    emitter.label("__rt_hsort_run_done");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // q now points to the following run pair
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // p = q for the next merge
    emitter.instruction("jmp __rt_hsort_run");                                  // process the next pair of runs

    emitter.label("__rt_hsort_pass_done");
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the completed output tail
    emitter.instruction("mov r11, r10");                                        // copy the tail index before scaling
    emitter.instruction("shl r11, 6");                                          // convert the tail index to an entry offset
    emitter.instruction("add r11, QWORD PTR [rbp - 16]");                       // address the completed output tail
    emitter.instruction("mov QWORD PTR [r11 + 56], -1");                        // terminate the rebuilt forward chain
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 72]");                       // reload the completed output head
    emitter.instruction("mov QWORD PTR [r10 + 24], r11");                       // publish the pass head in the table header
    emitter.instruction("mov r11, QWORD PTR [rbp - 80]");                       // reload the completed output tail
    emitter.instruction("mov QWORD PTR [r10 + 32], r11");                       // publish the pass tail in the table header
    emitter.instruction("cmp QWORD PTR [rbp - 88], 1");                         // one run means the list is fully sorted
    emitter.instruction("jle __rt_hsort_finish");                               // finish after the converged pass
    emitter.instruction("shl QWORD PTR [rbp - 32], 1");                         // double the run width for the next pass
    emitter.instruction("jmp __rt_hsort_pass");                                 // merge adjacent wider runs

    emitter.label("__rt_hsort_finish");
    emitter.instruction("mov rsp, rbp");                                        // release the merge-sort frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with the table reordered in place
}

/// Emits the comparator that reads two entry operands and applies the selected PHP rule.
fn emit_compare_entries(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_sort_compare_entries ---");
    emitter.label_global("__rt_hash_sort_compare_entries");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the comparison frame pointer
    emitter.instruction("sub rsp, 64");                                         // allocate the aligned comparison frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right entry address
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the comparison mode
    emitter.instruction("mov rsi, rdx");                                        // pass the mode to the left operand reader
    emitter.instruction("call __rt_hash_sort_triple");                          // materialize the left comparison triple
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the left runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the left low payload word
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the left high payload word
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the right entry address
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the comparison mode
    emitter.instruction("call __rt_hash_sort_triple");                          // materialize the right comparison triple
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the right runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // save the right low payload word
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the right high payload word
    emitter.instruction("test QWORD PTR [rbp - 16], 2");                        // inspect the key/value mode bit
    emitter.instruction("jnz __rt_hsort_cmp_value");                            // value sorts use PHP comparison
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the left normalized key payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the left key length or integer sentinel
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // pass the right normalized key payload
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // pass the right key length or integer sentinel
    emitter.instruction("call __rt_key_compare_regular");                       // apply exact SORT_REGULAR key ordering
    emitter.instruction("jmp __rt_hsort_cmp_done");                             // skip the general value comparator
    emitter.label("__rt_hsort_cmp_value");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the left runtime tag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the left low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // pass the left high payload word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // pass the right runtime tag
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // pass the right low payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // pass the right high payload word
    emitter.instruction("call __rt_php_compare");                               // apply PHP's general value comparison table
    emitter.label("__rt_hsort_cmp_done");
    emitter.instruction("mov rsp, rbp");                                        // release the comparison frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the signed comparison result in rax
}

/// Emits the operand reader that maps an entry key or value to a PHP comparison triple.
fn emit_sort_triple(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_sort_triple ---");
    emitter.label_global("__rt_hash_sort_triple");
    emitter.instruction("test rsi, 2");                                         // select entry value for value sorts
    emitter.instruction("jnz __rt_hsort_triple_value");                         // branch to the value payload reader
    emitter.instruction("mov rdx, QWORD PTR [rdi + 16]");                       // load key length or integer-key sentinel
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load key pointer or integer payload
    emitter.instruction("cmp rdx, -1");                                         // test for a normalized integer key
    emitter.instruction("jne __rt_hsort_triple_key_str");                       // string keys retain pointer and length
    emitter.instruction("xor eax, eax");                                        // runtime tag zero denotes an integer
    emitter.instruction("mov rdi, r10");                                        // publish the integer key payload
    emitter.instruction("mov rdx, -1");                                         // retain the integer-key sentinel
    emitter.instruction("ret");                                                 // return the integer key triple
    emitter.label("__rt_hsort_triple_key_str");
    emitter.instruction("mov eax, 1");                                          // runtime tag one denotes a string
    emitter.instruction("mov rdi, r10");                                        // publish the borrowed string key pointer
    emitter.instruction("ret");                                                 // return the borrowed string key triple
    emitter.label("__rt_hsort_triple_value");
    emitter.instruction("mov r11, QWORD PTR [rdi + 40]");                       // load the per-entry value tag
    emitter.instruction("mov r10, QWORD PTR [rdi + 24]");                       // load the low value payload word
    emitter.instruction("mov rdx, QWORD PTR [rdi + 32]");                       // load the high value payload word
    emitter.instruction("cmp r11, 7");                                          // test whether the entry holds a boxed Mixed cell
    emitter.instruction("je __rt_hsort_triple_value_boxed");                    // boxed values need unboxing
    emitter.instruction("mov rax, r11");                                        // publish the concrete runtime tag
    emitter.instruction("mov rdi, r10");                                        // publish the borrowed low payload word
    emitter.instruction("ret");                                                 // return the borrowed unboxed value triple
    emitter.label("__rt_hsort_triple_value_boxed");
    emitter.instruction("mov rax, r10");                                        // pass the borrowed Mixed cell to the unboxer
    emitter.instruction("jmp __rt_mixed_unbox");                                // tail-jump so the peeled triple returns directly
}
