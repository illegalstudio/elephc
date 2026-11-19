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
        let low = self.low;
        let high = low + 8;
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x1, [sp, #{low}]"));          // load the borrowed string pointer
                emitter.instruction(&format!("ldr x2, [sp, #{high}]"));         // load the borrowed string length
                abi::emit_call_label(emitter, "__rt_str_persist");
                if self.reload_later {
                    emitter.instruction(&format!("str x1, [sp, #{low}]"));      // stage the acquired string owner
                    emitter.instruction(&format!("str x2, [sp, #{high}]"));     // stage its length across helper calls
                    self.emit_reload(emitter);
                } else {
                    emitter.instruction("mov x3, x1");                          // pass the acquired string without a spill
                    emitter.instruction("mov x4, x2");                          // pass the acquired length directly
                    self.emit_reload_tag(emitter);
                }
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {low}]")); // load the borrowed string pointer
                emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {high}]")); // load the borrowed string length
                abi::emit_call_label(emitter, "__rt_str_persist");
                emitter.instruction(&format!("mov QWORD PTR [rbp - {low}], rax")); // stage the acquired string owner
                emitter.instruction(&format!("mov QWORD PTR [rbp - {high}], rdx")); // stage the acquired string length
                self.emit_reload(emitter);
            }
        }
    }

    /// Retains a borrowed pointer for insertion, clearing the unused high payload word.
    pub fn emit_retain_pointer(self, emitter: &mut impl Emitter) {
        let low = self.low;
        let high = low + 8;
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x0, [sp, #{low}]"));          // load the borrowed child pointer
                abi::emit_call_label(emitter, "__rt_incref");
                if self.reload_later {
                    emitter.instruction(&format!("str xzr, [sp, #{high}]"));    // canonicalize the high word for later reload
                }
                emitter.instruction(&format!("ldr x3, [sp, #{low}]"));          // provide the retained child pointer
                emitter.instruction("mov x4, xzr");                             // send the canonical unused high word
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {low}]")); // load the borrowed child pointer
                abi::emit_call_label(emitter, "__rt_incref");
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
