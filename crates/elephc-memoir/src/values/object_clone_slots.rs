//! Purpose:
//! Copies object property storage and acquires the owners of the copied slots.
//!
//! Called from:
//! - Direct object clone lowering and the runtime adapter for boxed objects.
//!
//! Key details:
//! - The compiler selects the property offsets or provides the native tag table.
//! - Direct clone may retain strings. Tagged clone uses the string persistence ABI.
//! - Reference cells preserve aliasing through the managed reference clone entry.

use crate::{Arch, Emitter};

/// Describes how direct lowering acquires ownership of a copied property slot.
pub enum SlotOwner {
    /// The slot contains only non-owning data.
    Plain,
    /// The low word identifies a heap-backed value that must be retained.
    Retain,
    /// The low word is a managed reference cell that must be cloned.
    Reference,
}

/// Copies two words and acquires the selected owner, preserving both object registers.
pub fn emit_direct_slot(
    emitter: &mut impl Emitter,
    source: &str,
    destination: &str,
    offset: usize,
    owner: SlotOwner,
) {
    let low = emitter.int_result_reg();
    let high = emitter.tertiary_scratch_reg();
    emitter.emit_load_from_address(low, source, offset);
    emitter.emit_load_from_address(high, source, offset + 8);
    emitter.emit_store_to_address(low, destination, offset);
    emitter.emit_store_to_address(high, destination, offset + 8);

    let entry = match owner {
        SlotOwner::Plain => return,
        SlotOwner::Retain => "__rt_incref",
        SlotOwner::Reference => "__rt_reference_cell_clone",
    };
    emitter.emit_push_reg(source);
    emitter.emit_push_reg(destination);
    emitter.emit_load_from_address(low, destination, offset);
    emitter.emit_call_label(entry);
    emitter.emit_pop_reg(destination);
    emitter.emit_pop_reg(source);
    if matches!(owner, SlotOwner::Reference) {
        emitter.emit_store_to_address(low, destination, offset);
    }
}

/// Clones declared property slots using the tag table staged by the boxed adapter.
/// The incoming frame contains source, destination, tags, count, index and offset spills.
/// Control falls through to the host's dynamic-property block after the loop completes.
pub fn emit_tagged_slots(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => aarch64_tagged_slots(emitter),
        Arch::X86_64 => x86_64_tagged_slots(emitter),
    }
}

/// Copies and acquires slots described by the boxed clone frame on aarch64.
fn aarch64_tagged_slots(emitter: &mut impl Emitter) {
    emitter.label("__rt_object_clone_shallow_boxed_prop_loop");
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the current property index
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the declared-property slot count
    emitter.instruction("cmp x12, x13");                                        // has every declared property slot been copied?
    emitter.instruction("b.ge __rt_object_clone_shallow_boxed_dyn");            // move on to the optional dynamic-property hash
    emitter.instruction("mov x10, #16");                                        // each declared-property slot is two 8-byte words
    emitter.instruction("mul x10, x12, x10");                                   // compute the byte offset inside the property region
    emitter.instruction("add x10, x10, #8");                                    // skip the leading class id to reach this slot
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the source object pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the clone object pointer
    emitter.instruction("ldr x15, [x9, x10]");                                  // copy the source property low word and keep it for retains
    emitter.instruction("str x15, [x11, x10]");                                 // store the property low word on the clone
    emitter.instruction("add x10, x10, #8");                                    // advance to the high word of the property slot
    emitter.instruction("ldr x14, [x9, x10]");                                  // copy the source property high word
    emitter.instruction("str x14, [x11, x10]");                                 // store the property high word on the clone
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the property-tag descriptor pointer
    emitter.instruction("ldrb w14, [x11, x12]");                                // load the compile-time ownership tag for this slot
    emitter.instruction("cmp x14, #11");                                        // owned property references require PHP reference-aware cloning
    emitter.instruction("b.eq __rt_object_clone_shallow_boxed_reference");      // share live aliases and separate singleton cells
    emitter.instruction("cmp x14, #1");                                         // does the slot hold an owned string payload?
    emitter.instruction("b.eq __rt_object_clone_shallow_boxed_string");         // string slots need an independent payload copy
    emitter.instruction("cmp x14, #4");                                         // does the slot hold a retained indexed-array payload?
    emitter.instruction("b.eq __rt_object_clone_shallow_boxed_retain");         // retained array slots need an extra owner reference
    emitter.instruction("cmp x14, #5");                                         // does the slot hold a retained associative-array payload?
    emitter.instruction("b.eq __rt_object_clone_shallow_boxed_retain");         // retained hash slots need an extra owner reference
    emitter.instruction("cmp x14, #6");                                         // does the slot hold a retained object payload?
    emitter.instruction("b.eq __rt_object_clone_shallow_boxed_retain");         // retained object slots need an extra owner reference
    emitter.instruction("cmp x14, #7");                                         // does the slot hold a retained boxed Mixed payload?
    emitter.instruction("b.eq __rt_object_clone_shallow_boxed_retain");         // retained Mixed slots need an extra owner reference
    emitter.instruction("b __rt_object_clone_shallow_boxed_next");              // scalar slots are copied without ownership changes
    emitter.label("__rt_object_clone_shallow_boxed_reference");
    emitter.instruction("str x12, [sp, #32]");                                  // preserve the property index across reference-cell cloning
    emitter.instruction("str x10, [sp, #40]");                                  // preserve the high-word property offset
    emitter.instruction("mov x0, x15");                                         // pass the owned reference cell to its clone helper
    emitter.instruction("bl __rt_reference_cell_clone");                        // retain shared aliases or copy singleton cell storage
    emitter.instruction("ldr x10, [sp, #40]");                                  // restore the high-word property offset
    emitter.instruction("sub x10, x10, #8");                                    // locate the clone's cell pointer word
    emitter.instruction("ldr x11, [sp, #8]");                                   // restore the cloned object pointer
    emitter.instruction("str x0, [x11, x10]");                                  // replace the copied slot with its correctly owned cell
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore the property traversal index
    emitter.instruction("b __rt_object_clone_shallow_boxed_next");              // continue after reference-aware slot cloning

    emitter.label("__rt_object_clone_shallow_boxed_string");
    emitter.instruction("str x12, [sp, #32]");                                  // preserve property index across string persistence
    emitter.instruction("str x10, [sp, #40]");                                  // preserve the high-word slot offset across the helper
    emitter.instruction("mov x1, x15");                                         // pass the source string pointer to the persistence helper
    emitter.instruction("ldr x2, [x9, x10]");                                   // pass the source string length to the persistence helper
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string payload for clone ownership
    emitter.instruction("ldr x10, [sp, #40]");                                  // restore the high-word slot offset after persistence
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the clone object pointer after persistence
    emitter.instruction("sub x10, x10, #8");                                    // move back to the low-word string pointer slot
    emitter.instruction("str x1, [x11, x10]");                                  // install the persisted string pointer on the clone
    emitter.instruction("add x10, x10, #8");                                    // move to the high-word string length slot
    emitter.instruction("str x2, [x11, x10]");                                  // install the persisted string length on the clone
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore property index after string persistence
    emitter.instruction("b __rt_object_clone_shallow_boxed_next");              // continue with the next declared property

    emitter.label("__rt_object_clone_shallow_boxed_retain");
    emitter.instruction("str x12, [sp, #32]");                                  // preserve property index across the retain helper
    emitter.instruction("mov x0, x15");                                         // pass the copied heap payload to the retain helper
    emitter.instruction("bl __rt_incref");                                      // retain the shared property payload for the cloned object
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore property index after the retain helper

    emitter.label("__rt_object_clone_shallow_boxed_next");
    emitter.instruction("add x12, x12, #1");                                    // advance to the next declared-property slot
    emitter.instruction("str x12, [sp, #32]");                                  // save the advanced property-copy index
    emitter.instruction("b __rt_object_clone_shallow_boxed_prop_loop");         // continue copying declared properties
}

/// Copies and acquires slots described by the boxed clone frame on x86_64.
fn x86_64_tagged_slots(emitter: &mut impl Emitter) {
    emitter.label("__rt_object_clone_shallow_boxed_prop_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current property index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // has every declared property slot been copied?
    emitter.instruction("jae __rt_object_clone_shallow_boxed_dyn_x86");         // move on to the optional dynamic-property hash
    emitter.instruction("mov rcx, r10");                                        // copy property index before scaling it into a byte offset
    emitter.instruction("shl rcx, 4");                                          // each declared-property slot is two 8-byte words
    emitter.instruction("add rcx, 8");                                          // skip the leading class id to reach this slot
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the source object pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the clone object pointer
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx]");                      // copy the source property low word
    emitter.instruction("mov rdx, QWORD PTR [r11 + rcx + 8]");                  // copy the source property high word
    emitter.instruction("mov QWORD PTR [r8 + rcx], rax");                       // store the property low word on the clone
    emitter.instruction("mov QWORD PTR [r8 + rcx + 8], rdx");                   // store the property high word on the clone
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the property-tag descriptor pointer
    emitter.instruction("movzx r11, BYTE PTR [r9 + r10]");                      // load the compile-time ownership tag for this slot
    emitter.instruction("cmp r11, 11");                                         // owned property references require PHP reference-aware cloning
    emitter.instruction("je __rt_object_clone_shallow_boxed_reference_x86");    // share live aliases and separate singleton cells
    emitter.instruction("cmp r11, 1");                                          // does the slot hold an owned string payload?
    emitter.instruction("je __rt_object_clone_shallow_boxed_string_x86");       // string slots need an independent payload copy
    emitter.instruction("cmp r11, 4");                                          // does the slot hold a retained indexed-array payload?
    emitter.instruction("je __rt_object_clone_shallow_boxed_retain_x86");       // retained array slots need an extra owner reference
    emitter.instruction("cmp r11, 5");                                          // does the slot hold a retained associative-array payload?
    emitter.instruction("je __rt_object_clone_shallow_boxed_retain_x86");       // retained hash slots need an extra owner reference
    emitter.instruction("cmp r11, 6");                                          // does the slot hold a retained object payload?
    emitter.instruction("je __rt_object_clone_shallow_boxed_retain_x86");       // retained object slots need an extra owner reference
    emitter.instruction("cmp r11, 7");                                          // does the slot hold a retained boxed Mixed payload?
    emitter.instruction("je __rt_object_clone_shallow_boxed_retain_x86");       // retained Mixed slots need an extra owner reference
    emitter.instruction("jmp __rt_object_clone_shallow_boxed_next_x86");        // scalar slots are copied without ownership changes
    emitter.label("__rt_object_clone_shallow_boxed_reference_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the property index across reference-cell cloning
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // preserve the property pointer offset
    emitter.instruction("call __rt_reference_cell_clone");                      // retain shared aliases or copy singleton cell storage
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // restore the property pointer offset
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // restore the cloned object pointer
    emitter.instruction("mov QWORD PTR [r8 + rcx], rax");                       // replace the copied slot with its correctly owned cell
    emitter.instruction("jmp __rt_object_clone_shallow_boxed_next_x86");        // continue after reference-aware slot cloning

    emitter.label("__rt_object_clone_shallow_boxed_string_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve property index across string persistence
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // preserve the low-word slot offset across the helper
    emitter.instruction("call __rt_str_persist");                               // duplicate the string payload for clone ownership
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // restore the low-word slot offset after persistence
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the clone object pointer after persistence
    emitter.instruction("mov QWORD PTR [r8 + rcx], rax");                       // install the persisted string pointer on the clone
    emitter.instruction("mov QWORD PTR [r8 + rcx + 8], rdx");                   // install the persisted string length on the clone
    emitter.instruction("jmp __rt_object_clone_shallow_boxed_next_x86");        // continue with the next declared property

    emitter.label("__rt_object_clone_shallow_boxed_retain_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve property index across the retain helper
    emitter.instruction("mov rdi, rax");                                        // pass the copied heap payload to the retain helper
    emitter.instruction("call __rt_incref");                                    // retain the shared property payload for the cloned object

    emitter.label("__rt_object_clone_shallow_boxed_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // advance to the next declared-property slot
    emitter.instruction("jmp __rt_object_clone_shallow_boxed_prop_loop_x86");   // continue copying declared properties
}
