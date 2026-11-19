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
            emit_persist_from_slots(emitter, self.low);
            emitter.instruction("mov x3, x1");                                  // pass the acquired pointer without spilling it
            emitter.instruction("mov x4, x2");                                  // pass its length directly
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
    emit_persist_from_slots(emitter, low);
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

/// Persists the string described by the two original frame words.
fn emit_persist_from_slots(emitter: &mut impl Emitter, low: usize) {
    let high = low + 8;
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x1, [sp, #{low}]"));              // load the borrowed string pointer
            emitter.instruction(&format!("ldr x2, [sp, #{high}]"));             // load the borrowed string length
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {low}]"));  // load the borrowed string pointer
            emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {high}]")); // load the borrowed string length
        }
    }
    abi::emit_call_label(emitter, "__rt_str_persist");
}
