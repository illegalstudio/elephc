//! Purpose:
//! Publishes staged hash payloads and links fresh buckets into insertion order.
//!
//! Called from:
//! - Ordinary normalized-key insertion and owned bucket transfer.
//!
//! Key details:
//! - Both emitters stage the value words in the same target-specific frame slots.
//! - Publication does not retire an overwritten owner.
//! - Linking updates the live count and the next automatic index.

use crate::{Arch, Emitter, hash_layout};

/// Copies the staged low, high and tag words to the selected bucket.
/// The entry address is in x12/r12; the old owner register remains intact.
pub fn emit_publish_payload(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x13, [sp, #24]");                          // load value_lo
            emitter.instruction("str x13, [x12, #24]");                         // update value_lo in entry
            emitter.instruction("ldr x13, [sp, #32]");                          // load value_hi
            emitter.instruction("str x13, [x12, #32]");                         // update value_hi in entry
            emitter.instruction("ldr x13, [sp, #40]");                          // load value_tag
            emitter.instruction("str x13, [x12, #40]");                         // update value_tag in entry
        }
        Arch::X86_64 => {
            emitter.instruction("mov r13, QWORD PTR [rbp - 32]");               // reload the replacement low payload word for the existing key slot
            emitter.instruction("mov QWORD PTR [r12 + 24], r13");               // overwrite the stored low payload word in the existing hash entry
            emitter.instruction("mov r13, QWORD PTR [rbp - 40]");               // reload the replacement high payload word for the existing key slot
            emitter.instruction("mov QWORD PTR [r12 + 32], r13");               // overwrite the stored high payload word in the existing hash entry
            emitter.instruction("mov r13, QWORD PTR [rbp - 48]");               // reload the replacement runtime value tag for the existing key slot
            emitter.instruction("mov QWORD PTR [r12 + 40], r13");               // overwrite the stored runtime value tag in the existing hash entry
        }
    }
}

/// Appends an initialized bucket, updates the live count and records its integer key.
/// The caller supplies its original join label and the prefix for local labels.
pub fn emit_link_new_bucket(emitter: &mut impl Emitter, prefix: &str, linked: &str) {
    let link = format!("{prefix}_link_tail");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x14, [x5, #32]");                          // load the previous tail slot for insertion-order linking
            emitter.instruction("str x14, [x12, #48]");                         // store prev = old tail on the new entry
            emitter.instruction("mov x15, #-1");                                // sentinel index for end of the insertion-order chain
            emitter.instruction("str x15, [x12, #56]");                         // store next = none on the new tail entry
            emitter.instruction("ldr x15, [x5, #24]");                          // load the current head slot
            emitter.instruction("cmp x15, #-1");                                // is this the first inserted slot in the destination hash?
            emitter.instruction(&format!("b.ne {link}"));                       // existing hashes append after the previous tail
            emitter.instruction("str x9, [x5, #24]");                           // initialize head = inserted slot
            emitter.instruction("str x9, [x5, #32]");                           // initialize tail = inserted slot
            emitter.instruction(&format!("b {linked}"));                        // skip the tail-link update for the first entry
            emitter.label(&link);
            emitter.instruction("mov x16, #64");                                // x16 = hash entry size for tail-slot addressing
            hash_layout::emit_entry_address(emitter, "x17", "x5", "x14");
            emitter.instruction("str x9, [x17, #56]");                          // link old tail.next = inserted slot
            emitter.instruction("str x9, [x5, #32]");                           // update tail = inserted slot
            emitter.label(linked);
            emitter.instruction("ldr x5, [sp, #0]");                            // reload destination table pointer
            emitter.instruction("ldr x13, [x5]");                               // load current entry count
            emitter.instruction("add x13, x13, #1");                            // count the newly inserted slot
            emitter.instruction("str x13, [x5]");                               // store updated entry count
            super::hash_next_index::record_insert(emitter, "x5", "x12", prefix);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r13, QWORD PTR [r10 + 32]");               // load the previous insertion-order tail slot from the hash header
            emitter.instruction("mov QWORD PTR [r12 + 48], r13");               // link the new entry back to the previous tail slot
            emitter.instruction("mov r14, -1");                                 // materialize the end-of-chain sentinel for the inserted tail entry
            emitter.instruction("mov QWORD PTR [r12 + 56], r14");               // initialize the new entry next-pointer as the tail sentinel
            emitter.instruction("cmp r13, -1");                                 // detect the first insertion so the hash header head/tail can be initialized together
            emitter.instruction(&format!("jne {link}"));                        // existing tables need an extra step to wire the previous tail forward to the inserted slot
            emitter.instruction("mov r11, QWORD PTR [rbp - 56]");               // reload the inserted slot index to seed the insertion-order head pointer
            emitter.instruction("mov QWORD PTR [r10 + 24], r11");               // header[24]: initialize the insertion-order head for the first occupied entry
            emitter.instruction("mov QWORD PTR [r10 + 32], r11");               // header[32]: initialize the insertion-order tail for the first occupied entry
            emitter.instruction(&format!("jmp {linked}"));                      // skip the previous-tail forward-link update on the very first insertion

            emitter.label(&link);
            hash_layout::emit_entry_address(emitter, "r14", "r10", "r13");
            emitter.instruction("mov r11, QWORD PTR [rbp - 56]");               // reload the inserted slot index for the previous-tail forward-link store
            emitter.instruction("mov QWORD PTR [r14 + 56], r11");               // update the previous tail entry to point at the inserted slot as its logical successor
            emitter.instruction("mov QWORD PTR [r10 + 32], r11");               // publish the inserted slot as the new insertion-order tail in the hash header

            emitter.label(linked);
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // load the current live-entry count from the hash header before incrementing it
            emitter.instruction("add r11, 1");                                  // increment the live-entry count after claiming a previously empty hash slot
            emitter.instruction("mov QWORD PTR [r10], r11");                    // store the updated live-entry count back into the hash header
            super::hash_next_index::record_insert(emitter, "r10", "r12", prefix);
        }
    }
}

/// Replaces an occupied entry before the previous owner can run its destructor.
pub fn emit_replace_and_release(emitter: &mut impl Emitter, release: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [x12, #24]");                          // hold the displaced owner's pointer across publication
            emit_publish_payload(emitter);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r14, QWORD PTR [r12 + 24]");               // hold the displaced owner's pointer across publication
            emit_publish_payload(emitter);
            emitter.instruction("mov rax, r14");                                // pass the displaced owner in the runtime result register
        }
    }
    emitter.emit_call_label(release);
}

/// Branches when a hash lookup found a cell with more owners than the bucket itself.
///
/// The hash lookup leaves the entry pointer in x4 or r8, with zero for a missing key.
pub fn emit_branch_if_shared_reference(emitter: &mut impl Emitter, absent: &str, shared: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x4, {absent}"));                  // a missing key cannot own a reference
            emitter.instruction("mov x6, x4");                                  // keep the matching bucket during the probe
            emitter.instruction("ldr x3, [x6, #24]");                           // load the managed cell pointer for a reference entry
            emitter.instruction("ldr x5, [x6, #40]");                           // load the stored payload tag
            emitter.instruction("cmp x5, #11");                                 // check for the managed reference-cell tag
            emitter.instruction(&format!("b.ne {absent}"));                     // other entries carry values
            emitter.instruction("ldr w10, [x3, #-12]");                         // read its unadjusted physical owner count
            emitter.instruction("cmp w10, #1");                                 // count owners beyond the entry itself
            emitter.instruction(&format!("b.hi {shared}"));                     // preserve a reference used by another owner
        }
        Arch::X86_64 => {
            emitter.instruction("test r8, r8");                                 // a missing key cannot own a reference
            emitter.instruction(&format!("jz {absent}"));                       // leave the probe false on a missing key
            emitter.instruction("mov r10, r8");                                 // keep the matching bucket during the probe
            emitter.instruction("mov rcx, QWORD PTR [r10 + 24]");               // load the managed cell pointer for a reference entry
            emitter.instruction("mov r9, QWORD PTR [r10 + 40]");                // load the stored payload tag
            emitter.instruction("cmp r9, 11");                                  // check for the managed reference-cell tag
            emitter.instruction(&format!("jne {absent}"));                      // other entries carry values
            emitter.instruction("mov r10d, DWORD PTR [rcx - 12]");              // read the unadjusted physical owner count
            emitter.instruction("cmp r10d, 1");                                 // count owners beyond the entry itself
            emitter.instruction(&format!("ja {shared}"));                       // preserve a reference used by another owner
        }
    }
}
