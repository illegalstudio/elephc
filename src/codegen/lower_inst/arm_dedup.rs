//! Purpose:
//! Merges class-ladder arms whose emitted code is identical, so a ladder costs one arm per
//! distinct body instead of one per class.
//!
//! Called from:
//! - `method_dispatch::lower_mixed_method_call`, around each candidate class's arm.
//!
//! Key details:
//! - A ladder dispatches on the receiver's class id and then runs that class's arm. Most arms do
//!   not depend on the class at all: they convert the arguments for the shared signature and call
//!   through the vtable slot. On the Symfony build, one `has()` ladder held 145 copies of the
//!   same 80-instruction arm.
//! - The test is textual and therefore sound: an arm is merged only when its code, with the
//!   labels it DEFINES renamed by order of appearance, is the same text as an arm already
//!   emitted. Labels it only references (the ladder's done and error exits) keep their names, so
//!   two arms that leave to different places never compare equal. Identical code behaves the same
//!   whichever class id led to it.
//! - A merged arm is rolled back and replaced by one jump to the canonical arm. Its class still
//!   keeps its own entry in the dispatch, which jumps to the arm's label as before.

use std::collections::HashMap;

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;

/// The canonical arm label for every distinct arm body of one ladder.
#[derive(Default)]
pub(super) struct ArmDeduper {
    canonical: HashMap<String, String>,
}

impl ArmDeduper {
    /// Finishes the arm whose body was emitted after `body_start` (just past its own label).
    /// Returns `true` when the body duplicated an earlier arm and was replaced by a jump to it.
    pub(super) fn finish_arm(&mut self, emitter: &mut Emitter, body_start: usize, label: &str) -> bool {
        let key = normalized_arm_text(emitter.text_since(body_start));
        if let Some(canonical) = self.canonical.get(&key) {
            let canonical = canonical.clone();
            emitter.rollback_to(body_start);
            abi::emit_jump(emitter, &canonical);
            return true;
        }
        self.canonical.insert(key, label.to_string());
        false
    }
}

/// Whether `byte` can appear inside an assembler symbol.
fn is_symbol_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'.')
}

/// The arm text with every label it defines replaced by `@<n>`, numbered by first definition.
pub(in crate::codegen) fn normalized_arm_text(text: &str) -> String {
    let mut defined: HashMap<&str, usize> = HashMap::new();
    for line in text.lines() {
        if let Some(name) = line.strip_suffix(':') {
            if !name.is_empty() && name.bytes().all(is_symbol_byte) {
                let next = defined.len();
                defined.entry(name).or_insert(next);
            }
        }
    }
    if defined.is_empty() {
        return text.to_string();
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if is_symbol_byte(bytes[i]) {
            let start = i;
            while i < bytes.len() && is_symbol_byte(bytes[i]) {
                i += 1;
            }
            let token = &text[start..i];
            match defined.get(token) {
                Some(index) => {
                    out.push('@');
                    out.push_str(&index.to_string());
                }
                None => out.push_str(token),
            }
        } else {
            let start = i;
            while i < bytes.len() && !is_symbol_byte(bytes[i]) {
                i += 1;
            }
            out.push_str(&text[start..i]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Defined labels are renamed by order; referenced-only labels keep their names.
    #[test]
    fn normalization_renames_only_the_labels_an_arm_defines() {
        let a = "    cmp x0, #1\n    b.eq L_aa_7\n    bl _fn_x\nL_aa_7:\n    b L_done_1\n";
        let b = "    cmp x0, #1\n    b.eq L_bb_9\n    bl _fn_x\nL_bb_9:\n    b L_done_1\n";
        let c = "    cmp x0, #1\n    b.eq L_cc_3\n    bl _fn_x\nL_cc_3:\n    b L_other_2\n";
        assert_eq!(normalized_arm_text(a), normalized_arm_text(b));
        assert_ne!(normalized_arm_text(a), normalized_arm_text(c), "different exits never merge");
        assert!(normalized_arm_text(a).contains("L_done_1"));
        assert!(!normalized_arm_text(a).contains("L_aa_7"));
    }

    /// A different call target or operand keeps arms apart.
    #[test]
    fn normalization_keeps_code_differences() {
        let a = "    bl _method_A_has\n";
        let b = "    bl _method_B_has\n";
        assert_ne!(normalized_arm_text(a), normalized_arm_text(b));
        assert_eq!(normalized_arm_text(a), a);
    }
}
