//! Purpose:
//! Merges two borrowed PHP arrays into an independently owned Mixed-value hash.
//!
//! Called from:
//! - The typed ArrayMerge codegen path when an operand uses boxed PHP array storage.
//!
//! Key details:
//! - Inputs are validated payload/tag pairs, not assumed packed-array pointers.
//! - Integer keys are renumbered across both sources; later string keys overwrite in place.
//! - Sources remain rooted by the caller, including while an overwritten result cell is released.

use crate::codegen_support::{emit::Emitter, platform::Arch, sentinels::emit_branch_if_null_container};
use elephc_memoir::{
    hash_layout::{self, EntryField},
    values::packed_slots,
};

/// Emits the two-layout merge helper for every supported native target.
pub fn emit_array_merge_boxed(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_array_merge_boxed");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Borrows payload/tag pairs in x0/x1 and x2/x3; returns an owned hash or zero with the bad argument in x1.
fn emit_aarch64(emitter: &mut Emitter) {
    // -- validate both inputs before allocating the result --
    emitter.instruction("sub sp, sp, #96");                                     // reserve ten state slots and the saved frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller across boxing and hash helpers
    emitter.instruction("add x29, sp, #80");                                    // establish an aligned native helper frame
    emitter.instruction("stp x0, x1, [sp]");                                    // preserve the first payload and its actual layout
    emitter.instruction("stp x2, x3, [sp, #16]");                               // preserve the second payload until the first source is exhausted
    emitter.instruction("sub x9, x1, #4");                                      // packed and hash tags map onto zero and one
    emitter.instruction("cmp x9, #1");                                          // reject every non-array tag
    emitter.instruction("b.hi __rt_array_merge_boxed_bad_first");               // report the first invalid argument without allocating
    emit_branch_if_null_container(emitter, "x0", "x9", "__rt_array_merge_boxed_bad_first");
    emitter.instruction("sub x9, x3, #4");                                      // check the second source independently
    emitter.instruction("cmp x9, #1");                                          // the second source must also be a PHP array
    emitter.instruction("b.hi __rt_array_merge_boxed_bad_second");              // keep invalid second operands out of the traversal loop
    emit_branch_if_null_container(emitter, "x2", "x9", "__rt_array_merge_boxed_bad_second");
    emitter.instruction("mov x0, #8");                                          // start with a nonzero capacity and grow amortized as needed
    emitter.instruction("mov x1, #7");                                          // every result entry owns a Mixed cell
    emitter.instruction("bl __rt_hash_new");                                    // allocate the independent result hash
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the result across value boxing and relocation
    emitter.instruction("str xzr, [sp, #48]");                                  // numeric output keys start at zero
    emitter.instruction("str xzr, [sp, #56]");                                  // begin with the first input

    // -- initialize the current source's insertion-order cursor --
    emitter.label("__rt_array_merge_boxed_start");
    emitter.instruction("ldp x10, x9, [sp]");                                   // borrow the current source and its layout
    emitter.instruction("mov x11, #0");                                         // packed arrays start at numeric slot zero
    emitter.instruction("cmp x9, #4");                                          // hashes use their first live bucket instead
    emitter.instruction("b.eq __rt_array_merge_boxed_cursor");                  // keep the packed zero-based cursor
    hash_layout::emit_order_edge(emitter, "x11", "x10", false);
    emitter.label("__rt_array_merge_boxed_cursor");
    emitter.instruction("str x11, [sp, #40]");                                  // preserve the next source position

    // -- read one source entry without changing either source container --
    emitter.label("__rt_array_merge_boxed_loop");
    emitter.instruction("ldp x11, x9, [sp]");                                   // recover the source pointer and layout discriminator
    emitter.instruction("ldr x10, [sp, #40]");                                  // recover the packed index or hash bucket
    emitter.instruction("cmp x9, #4");                                          // choose the physical source layout
    emitter.instruction("b.ne __rt_array_merge_boxed_hash");                    // hashes follow live insertion-order links
    emitter.instruction("ldr x9, [x11]");                                       // packed length bounds the read
    emitter.instruction("cmp x10, x9");                                         // check the index before reading its payload
    emitter.instruction("b.hs __rt_array_merge_boxed_source_done");             // advance to the next source at the end
    emitter.instruction("add x9, x10, #1");                                     // advance before any allocation can clobber scratch registers
    emitter.instruction("str x9, [sp, #40]");                                   // preserve the next packed index
    emitter.instruction("str x10, [sp, #64]");                                  // record the integer source key before renumbering
    emitter.instruction("mov x9, #-1");                                         // the negative high word identifies integer keys
    emitter.instruction("str x9, [sp, #72]");                                   // preserve the key kind across value boxing
    elephc_memoir::heap_kind::emit_load_metadata(emitter, "x0", "x11");
    elephc_memoir::values::array_header::emit_extract_value_tag_aarch64(emitter, "x0", "x0");
    elephc_memoir::values::array_header::emit_load_element_stride(emitter, "x11", "x12");
    emitter.instruction("mul x9, x10, x12");                                    // convert the logical index to a packed payload byte offset
    packed_slots::emit_load_packed_byte_offset_word(emitter, "x1", "x11", "x9");
    emitter.instruction("mov x2, #0");                                          // single-word payloads have no high word
    emitter.instruction("cmp x12, #16");                                        // only paired slots contain a second word
    emitter.instruction("b.ne __rt_array_merge_boxed_value");                   // avoid reading past an eight-byte final slot
    packed_slots::emit_load_packed_byte_offset_word_delta(emitter, "x2", "x11", "x9", 1);
    emitter.instruction("b __rt_array_merge_boxed_value");                      // acquire an independent result owner

    emitter.label("__rt_array_merge_boxed_hash");
    emitter.instruction("tbnz x10, #63, __rt_array_merge_boxed_source_done");   // minus one terminates the live insertion-order chain
    hash_layout::emit_entries(emitter, "x11", "x11");
    hash_layout::emit_consume_index_entry_address(emitter, "x11", "x10");
    hash_layout::emit_load_entry_field(emitter, "x9", "x11", EntryField::Next);
    emitter.instruction("str x9, [sp, #40]");                                   // preserve traversal across helper calls
    hash_layout::emit_entry_key(emitter, "x11", "x9", "x10");
    emitter.instruction("stp x9, x10, [sp, #64]");                              // retain the source key words while the value is boxed
    hash_layout::emit_entry_value(emitter, "x11", "x0", "x1", "x2");
    emitter.instruction("cmp x0, #11");                                         // does this entry belong to a PHP reference set?
    emitter.instruction("b.eq __rt_array_merge_boxed_reference");               // retain the shared cell instead of copying its value
    super::hash_entry_reference::emit_inline_entry_deref(emitter, "__rt_array_merge_boxed_deref_done", "x0", "x1", "x2");

    // -- preserve value ownership and PHP merge key rules --
    emitter.label("__rt_array_merge_boxed_value");
    emitter.instruction("cmp x0, #7");                                          // an existing Mixed cell already has the correct representation
    emitter.instruction("b.ne __rt_array_merge_boxed_box");                     // raw values need an independently owned box
    emitter.instruction("mov x0, x1");                                          // borrow the source Mixed cell
    emitter.instruction("bl __rt_incref");                                      // acquire exactly one reference for the result
    emitter.instruction("b __rt_array_merge_boxed_key");                        // avoid nesting a second Mixed wrapper
    emitter.label("__rt_array_merge_boxed_box");
    emitter.instruction("bl __rt_mixed_from_value");                            // persist strings and retain other heap-backed payloads
    emitter.label("__rt_array_merge_boxed_key");
    emitter.instruction("mov x3, x0");                                          // transfer this owned cell to the result entry
    emitter.instruction("ldp x1, x2, [sp, #64]");                               // reload the source key
    emitter.instruction("cmn x2, #1");                                          // integer keys are always renumbered by array_merge
    emitter.instruction("b.ne __rt_array_merge_boxed_insert");                  // string keys retain their spelling and overwrite earlier matches
    emitter.instruction("ldr x1, [sp, #48]");                                   // use the next output integer key across both arrays
    emitter.instruction("add x9, x1, #1");                                      // reserve the following integer key
    emitter.instruction("str x9, [sp, #48]");                                   // string keys do not affect numeric numbering
    emitter.label("__rt_array_merge_boxed_insert");
    emitter.instruction("ldr x0, [sp, #32]");                                   // recover the unique result hash
    emitter.instruction("mov x4, #0");                                          // Mixed pointers occupy one payload word
    emitter.instruction("mov x5, #7");                                          // the result now owns the boxed value
    emitter.instruction("bl __rt_hash_set_value");                              // persist string keys and release an overwritten result owner
    emitter.instruction("str x0, [sp, #32]");                                   // preserve any hash relocation
    emitter.instruction("b __rt_array_merge_boxed_loop");                       // copy the next live source entry

    emitter.label("__rt_array_merge_boxed_reference");
    emitter.instruction("mov x0, x1");                                          // pass the borrowed cell to the singleton-aware clone
    emitter.instruction("bl __rt_reference_cell_clone");                        // separate an unaliased cell, share a live reference
    emitter.instruction("mov x3, x0");                                          // transfer the cloned or shared cell as the entry value
    emitter.instruction("ldp x1, x2, [sp, #64]");                               // reload the source key
    emitter.instruction("cmn x2, #1");                                          // integer keys are always renumbered
    emitter.instruction("b.ne __rt_array_merge_boxed_reference_insert");        // string keys retain their spelling
    emitter.instruction("ldr x1, [sp, #48]");                                   // use the next output integer key
    emitter.instruction("add x9, x1, #1");                                      // reserve the following integer key
    emitter.instruction("str x9, [sp, #48]");                                   // the integer path advances the next output key
    emitter.label("__rt_array_merge_boxed_reference_insert");
    emitter.instruction("ldr x0, [sp, #32]");                                   // recover the unique result hash
    emitter.instruction("mov x4, #0");                                          // reference entries carry no high payload word
    emitter.instruction("mov x5, #11");                                         // preserve the managed reference tag
    emitter.instruction("bl __rt_hash_set_value");                              // persist the retained reference cell
    emitter.instruction("str x0, [sp, #32]");                                   // preserve any hash relocation
    emitter.instruction("b __rt_array_merge_boxed_loop");                       // copy the next live source entry

    // -- move to the second input without resetting result order or numeric keys --
    emitter.label("__rt_array_merge_boxed_source_done");
    emitter.instruction("ldr x9, [sp, #56]");                                   // inspect which input just finished
    emitter.instruction("cbnz x9, __rt_array_merge_boxed_done");                // both inputs have been merged
    emitter.instruction("mov x9, #1");                                          // mark the transition to the second input
    emitter.instruction("str x9, [sp, #56]");                                   // ensure the next completed source returns
    emitter.instruction("ldp x10, x11, [sp, #16]");                             // recover the second borrowed payload and layout
    emitter.instruction("stp x10, x11, [sp]");                                  // make the second array the active source
    emitter.instruction("b __rt_array_merge_boxed_start");                      // initialize only the source cursor

    emitter.label("__rt_array_merge_boxed_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // transfer the completed hash owner
    emitter.instruction("b __rt_array_merge_boxed_return");                     // share the balanced helper epilogue
    emitter.label("__rt_array_merge_boxed_bad_first");
    emitter.instruction("mov x1, #1");                                          // identify the invalid first operand
    emitter.instruction("b __rt_array_merge_boxed_invalid");                    // do not allocate a result for invalid input
    emitter.label("__rt_array_merge_boxed_bad_second");
    emitter.instruction("mov x1, #2");                                          // identify the invalid second operand
    emitter.label("__rt_array_merge_boxed_invalid");
    emitter.instruction("mov x0, #0");                                          // let codegen raise a catchable TypeError after returning
    emitter.label("__rt_array_merge_boxed_return");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller's frame and continuation
    emitter.instruction("add sp, sp, #96");                                     // discard the borrowed traversal state
    emitter.instruction("ret");                                                 // return without consuming either source
}

/// Borrows pairs in rdi/rsi and rdx/rcx; returns an owned hash or zero with the bad argument in rdx.
fn emit_x86_64(emitter: &mut Emitter) {
    // -- validate both source layouts before allocating --
    emitter.instruction("push rbp");                                            // preserve the caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // use a stable frame across boxing helpers
    emitter.instruction("sub rsp, 80");                                         // reserve ten state slots with SysV call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the first borrowed payload
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve its layout discriminator
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // keep the second payload until the first source is exhausted
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve the second source's actual layout
    emitter.instruction("lea r10, [rsi - 4]");                                  // map packed and hash tags onto zero and one
    emitter.instruction("cmp r10, 1");                                          // the first operand must be a PHP array
    emitter.instruction("ja __rt_array_merge_boxed_bad_first");                 // reject without creating a partial result
    emit_branch_if_null_container(emitter, "rdi", "r10", "__rt_array_merge_boxed_bad_first");
    emitter.instruction("lea r10, [rcx - 4]");                                  // validate the second source independently
    emitter.instruction("cmp r10, 1");                                          // reject non-array second operands before traversal
    emitter.instruction("ja __rt_array_merge_boxed_bad_second");                // report the invalid argument index
    emit_branch_if_null_container(emitter, "rdx", "r10", "__rt_array_merge_boxed_bad_second");
    emitter.instruction("mov rdi, 8");                                          // start with usable capacity and amortized growth
    emitter.instruction("mov rsi, 7");                                          // result entries own Mixed cells
    emitter.instruction("call __rt_hash_new");                                  // allocate an independent result hash
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the result across value boxing and insertion
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // output integer keys start at zero
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // begin with the first source

    // -- initialize the current source's insertion-order cursor --
    emitter.label("__rt_array_merge_boxed_start");
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // borrow the current source pointer
    emitter.instruction("xor r10d, r10d");                                      // packed sources begin at numeric slot zero
    emitter.instruction("cmp QWORD PTR [rbp - 16], 4");                         // hashes begin at their first live bucket
    emitter.instruction("je __rt_array_merge_boxed_cursor");                    // keep the zero-based packed cursor
    hash_layout::emit_order_edge(emitter, "r10", "r11", false);
    emitter.label("__rt_array_merge_boxed_cursor");
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // preserve the current source position

    // -- read a packed slot or a live hash entry --
    emitter.label("__rt_array_merge_boxed_loop");
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // recover the borrowed source pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // recover its packed index or hash bucket
    emitter.instruction("cmp QWORD PTR [rbp - 16], 4");                         // select the source's physical layout
    emitter.instruction("jne __rt_array_merge_boxed_hash");                     // hashes follow live insertion-order links
    emitter.instruction("cmp r10, QWORD PTR [r11]");                            // guard the packed read with the source length
    emitter.instruction("jae __rt_array_merge_boxed_source_done");              // move to the next source at the end
    emitter.instruction("lea r9, [r10 + 1]");                                   // advance before an allocation clobbers scratch registers
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // preserve the following packed index
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // retain the integer source key before renumbering
    emitter.instruction("mov QWORD PTR [rbp - 80], -1");                        // mark this as an integer key
    elephc_memoir::heap_kind::emit_load_metadata(emitter, "rax", "r11");
    elephc_memoir::values::array_header::emit_extract_value_tag_x86_64(emitter, "rax", "rax");
    elephc_memoir::values::array_header::emit_load_element_stride(emitter, "r11", "rcx");
    emitter.instruction("mov r9, r10");                                         // preserve the logical source index while forming its byte offset
    emitter.instruction("imul r9, rcx");                                        // locate this slot within the packed payload
    packed_slots::emit_load_packed_byte_offset_word(emitter, "rdi", "r11", "r9");
    emitter.instruction("xor esi, esi");                                        // single-word payloads have no high word
    emitter.instruction("cmp rcx, 16");                                         // only paired slots contain a second word
    emitter.instruction("jne __rt_array_merge_boxed_value");                    // avoid reading outside the final eight-byte slot
    packed_slots::emit_load_packed_byte_offset_word_delta(emitter, "rsi", "r11", "r9", 1);
    emitter.instruction("jmp __rt_array_merge_boxed_value");                    // acquire the result's value owner

    emitter.label("__rt_array_merge_boxed_hash");
    emitter.instruction("test r10, r10");                                       // minus one terminates the insertion-order chain
    emitter.instruction("js __rt_array_merge_boxed_source_done");               // no live entries remain in this source
    hash_layout::emit_entries(emitter, "r11", "r11");
    hash_layout::emit_consume_index_entry_address(emitter, "r11", "r10");
    hash_layout::emit_load_entry_field(emitter, "r10", "r11", EntryField::Next);
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // preserve traversal across allocations
    hash_layout::emit_load_entry_field(emitter, "r10", "r11", EntryField::KeyLow);
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // preserve the integer key or string pointer
    hash_layout::emit_load_entry_field(emitter, "r10", "r11", EntryField::KeyHigh);
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // preserve the source key kind
    hash_layout::emit_entry_value(emitter, "r11", "rax", "rdi", "rsi");
    emitter.instruction("cmp rax, 11");                                         // does this entry belong to a PHP reference set?
    emitter.instruction("je __rt_array_merge_boxed_reference");                 // retain the shared cell instead of copying its value
    super::hash_entry_reference::emit_inline_entry_deref(emitter, "__rt_array_merge_boxed_deref_done", "rax", "rdi", "rsi");

    // -- acquire a value owner and apply PHP merge key rules --
    emitter.label("__rt_array_merge_boxed_value");
    emitter.instruction("cmp rax, 7");                                          // existing Mixed cells can be shared directly
    emitter.instruction("jne __rt_array_merge_boxed_box");                      // raw values need a new tagged box
    emitter.instruction("mov rax, rdi");                                        // use the retain helper's result-register input ABI
    emitter.instruction("call __rt_incref");                                    // acquire exactly one result reference
    emitter.instruction("jmp __rt_array_merge_boxed_key");                      // avoid creating a redundant nested wrapper
    emitter.label("__rt_array_merge_boxed_box");
    emitter.instruction("call __rt_mixed_from_value");                          // persist strings and retain heap-backed raw payloads
    emitter.label("__rt_array_merge_boxed_key");
    emitter.instruction("mov rcx, rax");                                        // transfer this owned cell into the result
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // reload the source key's low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // reload its integer marker or string length
    emitter.instruction("cmp rdx, -1");                                         // integer keys are always renumbered
    emitter.instruction("jne __rt_array_merge_boxed_insert");                   // string keys retain their spelling and overwrite earlier matches
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // use the next output integer key across both inputs
    emitter.instruction("add QWORD PTR [rbp - 56], 1");                         // the integer path advances the next output key
    emitter.label("__rt_array_merge_boxed_insert");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // recover the unique result hash
    emitter.instruction("xor r8d, r8d");                                        // Mixed cells use one payload word
    emitter.instruction("mov r9, 7");                                           // transfer the boxed value owner into the hash entry
    emitter.instruction("call __rt_hash_set_value");                            // persist borrowed keys and release overwritten result cells
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve any relocation
    emitter.instruction("jmp __rt_array_merge_boxed_loop");                     // copy the next live source entry

    emitter.label("__rt_array_merge_boxed_reference");
    emitter.instruction("mov rax, rdi");                                        // pass the borrowed cell to the singleton-aware clone
    emitter.instruction("call __rt_reference_cell_clone");                      // separate an unaliased cell, share a live reference
    emitter.instruction("mov rcx, rax");                                        // transfer the cloned or shared cell as the entry value
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // reload the source key's low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // reload its integer marker or string length
    emitter.instruction("cmp rdx, -1");                                         // integer keys are always renumbered
    emitter.instruction("jne __rt_array_merge_boxed_reference_insert");         // string keys retain their spelling
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // use the next output integer key
    emitter.instruction("add QWORD PTR [rbp - 56], 1");                         // the integer path advances the next output key
    emitter.label("__rt_array_merge_boxed_reference_insert");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // recover the unique result hash
    emitter.instruction("xor r8d, r8d");                                        // reference entries carry no high payload word
    emitter.instruction("mov r9, 11");                                          // preserve the managed reference tag
    emitter.instruction("call __rt_hash_set_value");                            // persist the retained reference cell
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve any relocation
    emitter.instruction("jmp __rt_array_merge_boxed_loop");                     // copy the next live source entry

    // -- switch sources without resetting the result or integer-key counter --
    emitter.label("__rt_array_merge_boxed_source_done");
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // inspect which source just finished
    emitter.instruction("jne __rt_array_merge_boxed_done");                     // both inputs have been processed
    emitter.instruction("mov QWORD PTR [rbp - 64], 1");                         // the second completed source will return
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // recover the second borrowed payload
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // make it the active source
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // recover the second layout discriminator
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // switch the physical traversal layout if needed
    emitter.instruction("jmp __rt_array_merge_boxed_start");                    // reset only the source cursor

    emitter.label("__rt_array_merge_boxed_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // transfer the completed hash owner
    emitter.instruction("jmp __rt_array_merge_boxed_return");                   // share the balanced helper epilogue
    emitter.label("__rt_array_merge_boxed_bad_first");
    emitter.instruction("mov rdx, 1");                                          // identify the invalid first argument
    emitter.instruction("jmp __rt_array_merge_boxed_invalid");                  // leave result allocation untouched
    emitter.label("__rt_array_merge_boxed_bad_second");
    emitter.instruction("mov rdx, 2");                                          // identify the invalid second argument
    emitter.label("__rt_array_merge_boxed_invalid");
    emitter.instruction("xor eax, eax");                                        // let codegen raise TypeError after this helper returns
    emitter.label("__rt_array_merge_boxed_return");
    emitter.instruction("mov rsp, rbp");                                        // release all temporary traversal state
    emitter.instruction("pop rbp");                                             // restore the caller's frame pointer
    emitter.instruction("ret");                                                 // return without consuming either source
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target validates both inputs before allocation and retains boxed values before insertion.
    #[test]
    fn boxed_merge_validates_both_sources_and_transfers_owned_values() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_array_merge_boxed(&mut emitter);
            let asm = emitter.output();
            let allocate = asm.find("__rt_hash_new").unwrap();
            for label in ["__rt_array_merge_boxed_bad_first", "__rt_array_merge_boxed_bad_second"] {
                assert!(asm.find(label).unwrap() < allocate, "{name}: {label}");
            }
            for symbol in ["__rt_incref", "__rt_mixed_from_value", "__rt_hash_set", "__rt_hash_set_value", "__rt_reference_cell_clone"] {
                assert!(asm.contains(symbol), "{name}: {symbol}");
            }
            assert!(asm.contains("__rt_array_merge_boxed_hash:"), "{name}");
            assert!(asm.contains("__rt_array_merge_boxed_source_done:"), "{name}");
            assert!(!asm.contains("__rt_mixed_unbox"), "{name}: sources arrive as payload/tag pairs");
        }
    }
}
