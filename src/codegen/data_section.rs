//! Purpose:
//! Collects constants and common storage declarations before serializing the assembly data section.
//! Deduplicates string, float, and common symbols used by expression and runtime-facing emitters.
//!
//! Called from:
//! - `crate::codegen::generate()` and expression/statement emitters
//!
//! Key details:
//! - Labels must stay stable within one compilation because code emission references them before final serialization.

use std::collections::HashMap;

use crate::types::PhpType;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DataWord {
    U64(u64),
    Symbol(String),
}

/// Symbol-backed metadata for one function static local recorded during EIR
/// lowering: the value symbol, the one-time init-marker symbol, and the codegen
/// PHP type. Consumed only by the `--web` `__rt_web_reset` generator, which must
/// release/zero every persistent static between requests.
#[derive(Clone, Debug)]
pub struct StaticLocalRecord {
    /// `.comm` value symbol (`_static_<fn>_<name>`, 16 bytes).
    pub symbol: String,
    /// `.comm` init-marker symbol (`<symbol>_init`, 8 bytes; 0 = not yet run).
    pub init_symbol: String,
    /// Codegen representation of the static's PHP type (drives release shape).
    pub php_type: PhpType,
}

/// Tracks constants and common symbols for the assembly `.data` section.
///
/// - `entries`: string constants as `(label, bytes)` pairs
/// - `float_entries`: float constants as `(label, IEEE-754 bits)` pairs
/// - `comm_entries`: common symbols as `(label, size)` pairs
/// - `counter`: monotonically increasing integer for generating unique labels
/// - `dedup`/`float_dedup`/`comm_dedup`: deduplication maps to avoid emitting duplicate constants
pub struct DataSection {
    entries: Vec<(String, Vec<u8>)>,
    float_entries: Vec<(String, u64)>,
    word_entries: Vec<(String, Vec<DataWord>)>,
    comm_entries: Vec<(String, usize)>,
    counter: usize,
    dedup: HashMap<Vec<u8>, String>,
    float_dedup: HashMap<u64, String>,
    word_dedup: HashMap<Vec<DataWord>, String>,
    comm_dedup: HashMap<String, String>,
    static_locals: Vec<StaticLocalRecord>,
    static_local_dedup: HashMap<String, usize>,
}

impl DataSection {
    /// Creates a new empty data section. All collections start empty; the counter is zero.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            float_entries: Vec::new(),
            word_entries: Vec::new(),
            comm_entries: Vec::new(),
            counter: 0,
            dedup: HashMap::new(),
            float_dedup: HashMap::new(),
            word_dedup: HashMap::new(),
            comm_dedup: HashMap::new(),
            static_locals: Vec::new(),
            static_local_dedup: HashMap::new(),
        }
    }

    /// Records one function static local's storage metadata for the `--web`
    /// per-request reset routine. Deduplicates by value symbol because the same
    /// static is resolved on every load/store/init of that variable; only the
    /// first record per symbol is kept, preserving first-seen order.
    pub fn record_static_local(&mut self, record: StaticLocalRecord) {
        if self.static_local_dedup.contains_key(&record.symbol) {
            return;
        }
        self.static_local_dedup
            .insert(record.symbol.clone(), self.static_locals.len());
        self.static_locals.push(record);
    }

    /// Returns the recorded function static locals in first-seen order, used by
    /// the `--web` `__rt_web_reset` generator after all functions are emitted.
    pub fn static_locals(&self) -> &[StaticLocalRecord] {
        &self.static_locals
    }

    /// Looks up `value` in the float deduplication map; if found, returns the existing label.
    /// Otherwise generates `_float_N`, stores the IEEE-754 bit representation, and returns the new label.
    pub fn add_float(&mut self, value: f64) -> String {
        let bits = value.to_bits();
        if let Some(label) = self.float_dedup.get(&bits) {
            return label.clone();
        }
        let label = format!("_float_{}", self.counter);
        self.counter += 1;
        self.float_dedup.insert(bits, label.clone());
        self.float_entries.push((label.clone(), bits));
        label
    }

    /// Looks up `bytes` in the string deduplication map; if found, returns the existing label and length.
    /// Otherwise generates `_str_N`, clones the bytes into `entries`, and returns the new label and length.
    pub fn add_string(&mut self, bytes: &[u8]) -> (String, usize) {
        if let Some(label) = self.dedup.get(bytes) {
            return (label.clone(), bytes.len());
        }

        let label = format!("_str_{}", self.counter);
        self.counter += 1;
        let owned = bytes.to_vec();
        self.dedup.insert(owned.clone(), label.clone());
        self.entries.push((label.clone(), owned));
        (label, bytes.len())
    }

    /// Looks up `label` in the common-symbol deduplication map; if found, returns the existing label.
    /// Otherwise inserts `label` into `comm_entries` with the given `size` and returns `label` unchanged.
    pub fn add_comm(&mut self, label: String, size: usize) -> String {
        if let Some(existing) = self.comm_dedup.get(&label) {
            return existing.clone();
        }

        self.comm_dedup.insert(label.clone(), label.clone());
        self.comm_entries.push((label.clone(), size));
        label
    }

    /// Adds words to the current runtime or metadata collection.
    pub fn add_words(&mut self, words: Vec<DataWord>) -> String {
        if let Some(label) = self.word_dedup.get(&words) {
            return label.clone();
        }
        let label = format!("_data_{}", self.counter);
        self.counter += 1;
        self.word_dedup.insert(words.clone(), label.clone());
        self.word_entries.push((label.clone(), words));
        label
    }

    /// Serializes all entries into a GNU assembly `.data` section string.
    /// Returns an empty string when no entries have been collected.
    /// Emits `.comm` directives first, then `.ascii` string literals, then `.p2align 3`/`quad` float entries.
    pub fn emit(&self) -> String {
        if self.entries.is_empty()
            && self.float_entries.is_empty()
            && self.word_entries.is_empty()
            && self.comm_entries.is_empty()
        {
            return String::new();
        }

        let mut out = String::from(".data\n");
        for (label, size) in &self.comm_entries {
            out.push_str(&format!(".comm {}, {}, 3\n", label, size));
        }
        for (label, bytes) in &self.entries {
            out.push_str(&format!(".globl {}\n{}:\n", label, label));
            out.push_str("    .ascii \"");
            for &b in bytes {
                match b {
                    b'\n' => out.push_str("\\n"),
                    b'\t' => out.push_str("\\t"),
                    b'\\' => out.push_str("\\\\"),
                    b'"' => out.push_str("\\\""),
                    0x20..=0x7e => out.push(b as char),
                    _ => out.push_str(&format!("\\{:03o}", b)),
                }
            }
            out.push_str("\"\n");
        }
        for (label, bits) in &self.float_entries {
            out.push_str(&format!(".p2align 3\n.globl {}\n{}:\n    .quad 0x{:016x}\n", label, label, bits));
        }
        for (label, words) in &self.word_entries {
            out.push_str(&format!(".p2align 3\n.globl {}\n{}:\n", label, label));
            for word in words {
                match word {
                    DataWord::U64(value) => {
                        out.push_str(&format!("    .quad 0x{:016x}\n", value));
                    }
                    DataWord::Symbol(symbol) => {
                        out.push_str(&format!("    .quad {}\n", symbol));
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::DataSection;

    /// Verifies that float constants use power of two alignment directive.
    #[test]
    fn test_float_constants_use_power_of_two_alignment_directive() {
        let mut data = DataSection::new();
        data.add_float(3.14);

        let asm = data.emit();

        assert!(asm.contains(".p2align 3\n"));
        assert!(!asm.contains(".align 3\n"));
    }

    /// Verifies that non printable string bytes use bounded octal escapes.
    #[test]
    fn test_non_printable_string_bytes_use_bounded_octal_escapes() {
        let mut data = DataSection::new();
        data.add_string(b"a\0b");

        let asm = data.emit();

        assert!(asm.contains(r#".ascii "a\000b""#));
        assert!(!asm.contains(r#"\x00b"#));
    }

    /// Verifies that symbol word records emit quad symbols.
    #[test]
    fn test_symbol_word_records_emit_quad_symbols() {
        let mut data = DataSection::new();
        let label = data.add_words(vec![
            super::DataWord::U64(1),
            super::DataWord::Symbol("_fn_demo".to_string()),
        ]);

        let asm = data.emit();

        assert!(asm.contains(&format!(".globl {}\n{}:\n", label, label)));
        assert!(asm.contains("    .quad 0x0000000000000001\n"));
        assert!(asm.contains("    .quad _fn_demo\n"));
    }
}
