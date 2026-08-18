//! Purpose:
//! Collects constants and common storage declarations before serializing the assembly data section.
//! Deduplicates string, float, and common symbols used by expression and runtime-facing emitters.
//!
//! Called from:
//! - `crate::codegen` and shared expression/statement emitters.
//!
//! Key details:
//! - Labels must stay stable within one compilation because code emission references them before final serialization.
//! - `.comm`'s alignment operand is target-dependent and must follow the object format, not the
//!   host: Mach-O reads it as a power-of-two exponent, ELF as a byte count. Emitting one spelling
//!   everywhere silently under-aligns every common symbol on ELF, which the assembler accepts and
//!   the linker then rejects with `relocation truncated to fit` for any 64-bit access.

use std::collections::HashMap;

use crate::codegen_support::platform::{Platform, Target};
use crate::types::PhpType;

/// Alignment every common symbol is emitted with: 8 bytes, i.e. `2^3`.
///
/// Common storage holds pointers, `Mixed` boxes and 64-bit scalars, all of which are reached
/// through 64-bit loads and stores. On AArch64 those assemble to `R_AARCH64_LDST64_ABS_LO12_NC`,
/// whose displacement is encoded pre-shifted by 3 — so anything less than 8-byte alignment cannot
/// be represented and the link fails.
const COMM_ALIGN_BYTES: usize = 8;
const COMM_ALIGN_LOG2: usize = 3;

/// Renders `.comm`'s third operand for `target`'s object format.
///
/// Mach-O's assembler documents the operand as `log2(alignment)`; GNU as on ELF documents it as
/// the alignment in bytes. The same intended 8-byte alignment is therefore spelled `3` on Mach-O
/// and `8` on ELF.
fn comm_alignment_operand(target: Target) -> usize {
    match target.platform {
        Platform::MacOS => COMM_ALIGN_LOG2,
        Platform::Linux | Platform::Windows => COMM_ALIGN_BYTES,
    }
}

/// Renders one complete `.comm` directive line, alignment included, for `target`.
///
/// Every common symbol in the program must go through here rather than spelling the directive
/// inline: the alignment operand is the one part of it that is not portable, and a hardcoded
/// spelling is accepted by both assemblers while only being right for one of them.
pub(crate) fn comm_directive(label: &str, size: usize, target: Target) -> String {
    format!(
        ".comm {}, {}, {}\n",
        label,
        size,
        comm_alignment_operand(target)
    )
}

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

    /// Emits `bytes` under the caller-chosen global symbol `name` in the `.data`
    /// section (`.globl name` + `.ascii`), for a fixed-name blob other objects
    /// reference by symbol — e.g. the `--probe` build key. Idempotent by name.
    pub fn add_named_symbol(&mut self, name: String, bytes: &[u8]) {
        if self.entries.iter().any(|(label, _)| label == &name) {
            return;
        }
        self.entries.push((name, bytes.to_vec()));
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

    /// Returns true when common storage has been declared for `label`.
    pub fn has_comm(&self, label: &str) -> bool {
        self.comm_dedup.contains_key(label)
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
    ///
    /// `target` is required because `.comm`'s alignment operand is spelled differently per object
    /// format; see [`comm_alignment_operand`].
    pub fn emit(&self, target: Target) -> String {
        if self.entries.is_empty()
            && self.float_entries.is_empty()
            && self.word_entries.is_empty()
            && self.comm_entries.is_empty()
        {
            return String::new();
        }

        let mut out = String::from(".data\n");
        let comm_align = comm_alignment_operand(target);
        for (label, size) in &self.comm_entries {
            out.push_str(&format!(".comm {}, {}, {}\n", label, size, comm_align));
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
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// A Mach-O target, whose assembler reads `.comm`'s alignment operand as `log2(bytes)`.
    fn macos() -> Target {
        Target {
            platform: Platform::MacOS,
            arch: Arch::AArch64,
        }
    }

    /// An ELF target, whose assembler reads `.comm`'s alignment operand as a byte count.
    fn linux(arch: Arch) -> Target {
        Target {
            platform: Platform::Linux,
            arch,
        }
    }

    /// Verifies that float constants use power of two alignment directive.
    #[test]
    fn test_float_constants_use_power_of_two_alignment_directive() {
        let mut data = DataSection::new();
        data.add_float(3.14);

        let asm = data.emit(macos());

        assert!(asm.contains(".p2align 3\n"));
        assert!(!asm.contains(".align 3\n"));
    }

    /// Verifies that non printable string bytes use bounded octal escapes.
    #[test]
    fn test_non_printable_string_bytes_use_bounded_octal_escapes() {
        let mut data = DataSection::new();
        data.add_string(b"a\0b");

        let asm = data.emit(macos());

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

        let asm = data.emit(macos());

        assert!(asm.contains(&format!(".globl {}\n{}:\n", label, label)));
        assert!(asm.contains("    .quad 0x0000000000000001\n"));
        assert!(asm.contains("    .quad _fn_demo\n"));
    }

    /// Verifies `.comm` asks each object format for the same 8-byte alignment in the spelling
    /// that format's assembler understands: `log2` on Mach-O, bytes on ELF.
    ///
    /// Emitting the Mach-O spelling on ELF declares 3-byte alignment, which the assembler
    /// accepts and the linker then rejects — `R_AARCH64_LDST64_ABS_LO12_NC` encodes its
    /// displacement pre-shifted by 3, so a 64-bit load of an under-aligned common symbol fails
    /// with `relocation truncated to fit`. That took out every linux-aarch64 link once
    /// `_stack_limit` became a common symbol.
    #[test]
    fn test_comm_alignment_operand_follows_the_object_format() {
        let mut data = DataSection::new();
        data.add_comm("_stack_limit".to_string(), 8);

        assert!(data.emit(macos()).contains(".comm _stack_limit, 8, 3\n"));
        assert!(data
            .emit(linux(Arch::AArch64))
            .contains(".comm _stack_limit, 8, 8\n"));
        assert!(data
            .emit(linux(Arch::X86_64))
            .contains(".comm _stack_limit, 8, 8\n"));
    }
}
