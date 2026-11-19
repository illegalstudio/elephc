//! Purpose:
//! Acquires owned hash insertion payloads from staged value words.
//!
//! Called from:
//! - Hash rebuilding and spreading emitters after they select the payload's runtime tag.
//!
//! Key details:
//! - Words are staged in low, high and tag order.
//! - The caller selects the owner acquisition and may reload the staged words after a call.

use crate::{abi, Arch, Emitter};

/// Native slots for one hash value, including any reloaded copy.
#[derive(Clone, Copy)]
pub struct HashPayloadSlots {
    /// Offset of the low payload word, followed by high and tag at one-word strides.
    pub low: usize,
    /// Whether later calls require reloading the acquired payload from its slots.
    pub reload_later: bool,
}

impl HashPayloadSlots {
    /// Loads the staged words for insertion.
    pub fn emit_reload(self, emitter: &mut impl Emitter) {
        let high = self.low + 8;
        let low = self.low;
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x3, [sp, #{low}]"));          // reload the payload low word
                emitter.instruction(&format!("ldr x4, [sp, #{high}]"));         // reload the payload high word
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rcx, QWORD PTR [rbp - {low}]")); // reload the payload low word
                emitter.instruction(&format!("mov r8, QWORD PTR [rbp - {high}]")); // reload the payload high word
            }
        }
        self.emit_reload_tag(emitter);
    }

    /// Persists a borrowed string pair for a new hash owner.
    pub fn emit_persist_string(self, emitter: &mut impl Emitter) {
        if emitter.arch() == Arch::AArch64 && !self.reload_later {
            crate::strings::str_persist::emit_persist_from_frame(emitter, self.low);
            emit_string_result_to_insert(emitter);
            self.emit_reload_tag(emitter);
            return;
        }
        emit_persist_staged_string(emitter, self.low, StringSlotStore::Separate);
        self.emit_reload(emitter);
    }

    /// Retains a borrowed pointer for insertion, clearing the unused high payload word.
    pub fn emit_retain_pointer(self, emitter: &mut impl Emitter) {
        let low = self.low;
        let high = low + 8;
        emit_retain_staged_pointer(emitter, low);
        match emitter.arch() {
            Arch::AArch64 => {
                if self.reload_later {
                    emitter.instruction(&format!("str xzr, [sp, #{high}]"));    // canonicalize the high word for later reload
                }
                emitter.instruction(&format!("ldr x3, [sp, #{low}]"));          // provide the retained child pointer
                emitter.instruction("mov x4, xzr");                             // send the canonical unused high word
            }
            Arch::X86_64 => {
                if self.reload_later {
                    emitter.instruction(&format!("mov QWORD PTR [rbp - {high}], 0")); // canonicalize the high word for later reload
                }
                emitter.instruction(&format!("mov rcx, QWORD PTR [rbp - {low}]")); // provide the retained child pointer
                emitter.instruction("xor r8d, r8d");                            // send the canonical unused high word
            }
        }
        self.emit_reload_tag(emitter);
    }

    /// Retains the staged low word and reloads all three original value words.
    pub fn emit_retain_preserving_words(self, emitter: &mut impl Emitter) {
        emit_retain_staged_pointer(emitter, self.low);
        self.emit_reload(emitter);
    }

    /// Reloads the runtime tag paired with the acquired payload.
    fn emit_reload_tag(self, emitter: &mut impl Emitter) {
        let tag = self.low + 16;
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x5, [sp, #{tag}]"));          // preserve the runtime tag for insertion
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {tag}]")); // preserve the runtime tag for insertion
            }
        }
    }
}

/// Selects the native writeback used by the caller's string pair spill.
pub enum StringSlotStore {
    /// Writes each word independently.
    Separate,
    /// Writes both adjacent words in one AArch64 instruction.
    PairedOnAarch64,
}

/// Persists the borrowed string pair in the original low/length frame slots.
pub fn emit_persist_staged_string(emitter: &mut impl Emitter, low: usize, store: StringSlotStore) {
    crate::strings::str_persist::emit_persist_from_frame(emitter, low);
    emit_writeback_string(emitter, low, store);
}

/// Writes an independent string pair over its former borrowed frame words.
fn emit_writeback_string(emitter: &mut impl Emitter, low: usize, store: StringSlotStore) {
    let high = low + 8;
    match emitter.arch() {
        Arch::AArch64 => match store {
            StringSlotStore::Separate => {
                emitter.instruction(&format!("str x1, [sp, #{low}]"));          // replace the borrowed pointer with its owner
                emitter.instruction(&format!("str x2, [sp, #{high}]"));         // preserve the acquired length in the paired slot
            }
            StringSlotStore::PairedOnAarch64 => {
                emitter.instruction(&format!("stp x1, x2, [sp, #{low}]"));      // publish the owned pair together
            }
        },
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [rbp - {low}], rax"));  // replace the borrowed pointer with its owner
            emitter.instruction(&format!("mov QWORD PTR [rbp - {high}], rdx")); // preserve the acquired length in the paired slot
        }
    }
}

/// Retains a pointer in a staged frame slot without altering its physical payload.
pub fn emit_retain_staged_pointer(emitter: &mut impl Emitter, low: usize) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [sp, #{low}]"));              // load the borrowed ARM child
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {low}]"));  // load the borrowed x86 child
        }
    }
    abi::emit_call_label(emitter, "__rt_incref");
}

/// Persists a staged string and passes it directly to the hash insert ABI.
pub fn emit_persist_staged_string_for_insert(emitter: &mut impl Emitter, low: usize) {
    crate::strings::str_persist::emit_persist_from_frame(emitter, low);
    emit_string_result_to_insert(emitter);
    HashPayloadSlots { low, reload_later: false }.emit_reload_tag(emitter);
}

/// Persists an indexed string whose pointer is staged and length is still in its entry.
pub fn emit_persist_indexed_string_to_slots(emitter: &mut impl Emitter, low: usize) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x2, [x11, #8]");                           // load the borrowed entry's string length
            emitter.instruction(&format!("ldr x1, [sp, #{low}]"));              // load its saved string pointer
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                // load the borrowed entry's string length
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {low}]"));  // load its saved string pointer
        }
    }
    abi::emit_call_label(emitter, "__rt_str_persist");
    emit_writeback_string(emitter, low, StringSlotStore::Separate);
}

/// Retains an indexed payload already in the insertion low register and restores its staged tag.
pub fn emit_retain_indexed_pointer_for_insert(emitter: &mut impl Emitter, low: usize, tag: usize) {
    // -- indexed pointer values own only their low word --
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x3");                                  // pass the borrowed indexed pointer
            emitter.instruction("bl __rt_incref");                              // acquire an owner for the hash
            emitter.instruction(&format!("ldr x3, [sp, #{low}]"));              // reload the retained pointer
            emitter.instruction("mov x4, xzr");                                 // leave the unused high word empty
            emitter.instruction(&format!("ldr x5, [sp, #{tag}]"));              // reload the original runtime tag
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, rcx");                                // pass the borrowed indexed pointer
            emitter.instruction("call __rt_incref");                            // acquire an owner for the hash
            emitter.instruction(&format!("mov rcx, QWORD PTR [rbp - {low}]"));  // reload the retained pointer
            emitter.instruction("xor r8d, r8d");                                // leave the unused high word empty
            emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {tag}]"));   // reload the original runtime tag
        }
    }
}

/// Persists the pair at the indexed entry address and prepares its string tag for insertion.
pub fn emit_persist_indexed_pair_for_insert(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldp x1, x2, [x11]");                           // load the borrowed pointer and length
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [r11]");                    // load the borrowed string pointer
            emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                // load the borrowed string length
        }
    }
    abi::emit_call_label(emitter, "__rt_str_persist");
    emit_string_result_to_insert(emitter);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov x5, #1");                                  // keep the acquired pair tagged as a string
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, 1");                                   // keep the acquired pair tagged as a string
        }
    }
}

/// Moves the independent result pair into the native hash insertion arguments.
fn emit_string_result_to_insert(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov x3, x1");                                  // pass the acquired pointer without spilling it
            emitter.instruction("mov x4, x2");                                  // pass its length directly
        }
        Arch::X86_64 => {
            emitter.instruction("mov rcx, rax");                                // pass the acquired pointer without spilling it
            emitter.instruction("mov r8, rdx");                                 // pass its length directly
        }
    }
}
