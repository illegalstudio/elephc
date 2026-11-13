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

use crate::fast_hash::FastMap;

use crate::codegen_support::platform::{Platform, Target};
use crate::types::PhpType;

/// Alignment every common symbol is emitted with: 8 bytes, i.e. `2^3`.
///
/// Common storage holds pointers, `Mixed` boxes and 64-bit scalars, all of which are reached
/// through 64-bit loads and stores. On AArch64 those assemble to `R_AARCH64_LDST64_ABS_LO12_NC`,
/// whose displacement is encoded pre-shifted by 3 — so anything less than 8-byte alignment cannot
/// be represented and the link fails.
const COMM_ALIGN_BYTES: usize = 8;

/// Renders `.comm`'s third operand for one requested byte alignment and target format.
///
/// Mach-O's assembler documents the operand as `log2(alignment)`; GNU as on ELF documents it as
/// the alignment in bytes. `alignment_bytes` must be a non-zero power of two.
fn comm_alignment_operand(target: Target, alignment_bytes: usize) -> usize {
    debug_assert!(alignment_bytes.is_power_of_two() && alignment_bytes > 0);
    match target.platform {
        Platform::MacOS => alignment_bytes.ilog2() as usize,
        Platform::Linux | Platform::Windows => alignment_bytes,
    }
}

/// Renders one complete `.comm` directive line, alignment included, for `target`.
///
/// Every common symbol in the program must go through here rather than spelling the directive
/// inline: the alignment operand is the one part of it that is not portable, and a hardcoded
/// spelling is accepted by both assemblers while only being right for one of them.
pub(crate) fn comm_directive(label: &str, size: usize, target: Target) -> String {
    comm_directive_aligned(label, size, COMM_ALIGN_BYTES, target)
}

/// Renders a `.comm` directive with an explicit power-of-two byte alignment.
pub(crate) fn comm_directive_aligned(
    label: &str,
    size: usize,
    alignment_bytes: usize,
    target: Target,
) -> String {
    debug_assert!(alignment_bytes.is_power_of_two());
    let alignment = match target.platform {
        Platform::MacOS => alignment_bytes.trailing_zeros() as usize,
        Platform::Linux | Platform::Windows => alignment_bytes,
    };
    format!(
        ".comm {}, {}, {}\n",
        label,
        size,
        alignment
    )
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DataWord {
    U64(u64),
    Symbol(String),
}

/// A section's interned contents, shared read-only by the shards of a parallel pass.
pub struct DataBase {
    strings: FastMap<Vec<u8>, String>,
    floats: FastMap<u64, String>,
    words: FastMap<Vec<DataWord>, String>,
}

/// Labels already in a section, by kind, for `DataSection::merge_indexed`.
pub struct DataMergeIndex {
    entries: FastMap<String, usize>,
    floats: FastMap<String, u64>,
    words: FastMap<String, usize>,
}

/// 64-bit FNV-1a of `tag` then `bytes`: the name a content-addressed entry is given.
///
/// FNV rather than the compiler's hash-map hasher because the value is spelled into the assembly
/// and has to be identical on every run and host. A collision cannot pass silently: two entries
/// of one section would define the same label twice, which `as` rejects, and two sections are
/// compared by `merge` before one copy is dropped.
fn content_label(tag: &[u8], bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in tag.iter().chain(bytes) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// `content_label` of a word table, over an unambiguous encoding of its words.
fn words_content_label(words: &[DataWord]) -> u64 {
    let mut encoded = Vec::with_capacity(words.len() * 9);
    for word in words {
        match word {
            DataWord::U64(value) => {
                encoded.push(0);
                encoded.extend_from_slice(&value.to_le_bytes());
            }
            DataWord::Symbol(symbol) => {
                encoded.push(1);
                encoded.extend_from_slice(&(symbol.len() as u64).to_le_bytes());
                encoded.extend_from_slice(symbol.as_bytes());
            }
        }
    }
    content_label(b"w", &encoded)
}

/// Writes a word table's directives: one `.quad` per word, except that a run of two or more
/// zero words is one `.zero` of the run's bytes. The sparse hashed lookup tables are mostly
/// empty buckets: 9.4 M of the Symfony demo console's 121 M assembly lines were `.quad 0`.
fn write_words(out: &mut String, words: &[DataWord]) {
    use std::fmt::Write as _;
    let mut index = 0;
    while index < words.len() {
        match &words[index] {
            DataWord::U64(0) => {
                let run = words[index..]
                    .iter()
                    .take_while(|word| matches!(word, DataWord::U64(0)))
                    .count();
                if run >= 2 {
                    let _ = writeln!(out, "    .zero {}", run * 8);
                } else {
                    out.push_str("    .quad 0x0000000000000000\n");
                }
                index += run;
                continue;
            }
            DataWord::U64(value) => {
                let _ = writeln!(out, "    .quad 0x{:016x}", value);
            }
            DataWord::Symbol(symbol) => {
                let _ = writeln!(out, "    .quad {}", symbol);
            }
        }
        index += 1;
    }
}

/// Stops the build when one label names two different contents across merged sections.
fn assert_same_content<T: PartialEq + ?Sized>(label: &str, kept: &T, other: &T) {
    assert!(
        kept == other,
        "data label {label} names two different contents: content-address collision"
    );
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
/// The size of every collection in a `DataSection` at one moment, for `rollback_to`.
#[derive(Clone, Copy)]
pub struct DataSectionCheckpoint {
    entries: usize,
    float_entries: usize,
    word_entries: usize,
    comm_entries: usize,
    static_locals: usize,
    counter: usize,
}

pub struct DataSection {
    entries: Vec<(String, Vec<u8>)>,
    float_entries: Vec<(String, u64)>,
    word_entries: Vec<(String, Vec<DataWord>)>,
    comm_entries: Vec<(String, usize)>,
    private_identities: Vec<String>,
    counter: usize,
    /// Distinguishes the generated labels of one shard from another's, so two sections built
    /// independently can be merged without renaming anything. Empty for a whole-module section.
    label_shard: String,
    /// Name strings, tables and floats after their CONTENT instead of a counter. Set for the
    /// sections of parallel codegen workers: two workers that build the same table then spell the
    /// same label, and `merge` keeps one copy instead of one per worker. See `content_label`.
    content_labels: bool,
    /// The main section's contents as they stood when a parallel pass began; see `DataBase`.
    base: Option<std::sync::Arc<DataBase>>,
    dedup: FastMap<Vec<u8>, String>,
    float_dedup: FastMap<u64, String>,
    word_dedup: FastMap<Vec<DataWord>, String>,
    comm_dedup: FastMap<String, String>,
    static_locals: Vec<StaticLocalRecord>,
    static_local_dedup: FastMap<String, usize>,
    extern_callback_slots: Vec<String>,
}

impl DataSection {
    /// Creates a new empty data section. All collections start empty; the counter is zero.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            float_entries: Vec::new(),
            word_entries: Vec::new(),
            comm_entries: Vec::new(),
            private_identities: Vec::new(),
            counter: 0,
            label_shard: String::new(),
            content_labels: false,
            base: None,
            dedup: FastMap::default(),
            float_dedup: FastMap::default(),
            word_dedup: FastMap::default(),
            comm_dedup: FastMap::default(),
            static_locals: Vec::new(),
            static_local_dedup: FastMap::default(),
            extern_callback_slots: Vec::new(),
        }
    }

    /// Creates a section whose generated labels carry `shard`, for a body emitted independently.
    ///
    /// The prefix is what makes two sections mergeable without renaming: `_str_s3_17` belongs to
    /// shard 3 and cannot be `_str_s4_17`. Caller-named entries (`.comm`, named symbols, static
    /// locals) are untouched — they are already unique by name, and `merge` dedups them.
    ///
    /// `base` is what the main section already holds: content found there resolves to its label
    /// instead of becoming a second copy under a content address.
    pub fn for_shard(shard: usize, base: Option<std::sync::Arc<DataBase>>) -> Self {
        Self {
            label_shard: format!("s{}_", shard),
            content_labels: true,
            base,
            ..Self::new()
        }
    }

    /// Captures this section's interned contents for the shards of a parallel pass.
    ///
    /// The helpers emitted ahead of the bodies intern the module's big tables into the main
    /// section under counter labels; a shard that built the same table again would name it by
    /// content, the two labels would differ, and the merge would keep both.
    pub fn base(&self) -> std::sync::Arc<DataBase> {
        std::sync::Arc::new(DataBase {
            strings: self.dedup.clone(),
            floats: self.float_dedup.clone(),
            words: self.word_dedup.clone(),
        })
    }

    /// Records the current size of every collection, for a possible rollback.
    pub fn checkpoint(&self) -> DataSectionCheckpoint {
        DataSectionCheckpoint {
            entries: self.entries.len(),
            float_entries: self.float_entries.len(),
            word_entries: self.word_entries.len(),
            comm_entries: self.comm_entries.len(),
            static_locals: self.static_locals.len(),
            counter: self.counter,
        }
    }

    /// Discards everything recorded after `checkpoint`.
    ///
    /// Needed because a data entry can REFERENCE a text label — a callable descriptor points at
    /// the body that implements it — so text and data have to be discarded together or the
    /// assembler is handed a reference with no definition.
    ///
    /// Each dedup map is unwound from the entries being removed rather than by scanning what
    /// survives: every map is keyed by something the removed entry still carries.
    pub fn rollback_to(&mut self, checkpoint: DataSectionCheckpoint) {
        for (label, bytes) in self.entries.drain(checkpoint.entries.min(self.entries.len())..) {
            // A named symbol was never interned by bytes, so this is a no-op for those.
            if self.dedup.get(&bytes).is_some_and(|existing| existing == &label) {
                self.dedup.remove(&bytes);
            }
        }
        for (_, bits) in self
            .float_entries
            .drain(checkpoint.float_entries.min(self.float_entries.len())..)
        {
            self.float_dedup.remove(&bits);
        }
        for (_, words) in self
            .word_entries
            .drain(checkpoint.word_entries.min(self.word_entries.len())..)
        {
            self.word_dedup.remove(&words);
        }
        for (label, _) in self
            .comm_entries
            .drain(checkpoint.comm_entries.min(self.comm_entries.len())..)
        {
            self.comm_dedup.remove(&label);
        }
        for record in self
            .static_locals
            .drain(checkpoint.static_locals.min(self.static_locals.len())..)
        {
            self.static_local_dedup.remove(&record.symbol);
        }
        self.counter = checkpoint.counter;
    }

    /// Folds `other` into this section, keeping this section's entries first.
    ///
    /// Generated labels cannot collide when the two sections came from different shards, so
    /// those entries are appended as they are — including duplicate blobs, because each shard's
    /// labels are already baked into the assembly it emitted and repointing them would mean
    /// rewriting that text. Everything keyed by a caller-chosen name is deduplicated instead:
    /// two shards declaring the same `.comm`, the same named symbol or the same function static
    /// is a duplicate DEFINITION, and the first one wins.
    ///
    /// A content-addressed label (see `content_label`) that both sections define names the same
    /// bytes by construction, so it is kept once: that is what stops every codegen worker from
    /// contributing its own copy of the same lookup table. Should two different contents ever
    /// share one, that is a hash collision and the build stops rather than merge them.
    #[cfg(test)]
    pub fn merge(&mut self, other: Self) {
        let mut index = self.merge_index();
        self.merge_indexed(other, &mut index);
    }

    /// Indexes this section's labels for a run of `merge_indexed` calls.
    ///
    /// Built once for a whole parallel pass: rebuilding it per merged shard would cost the
    /// section's size again for every one of them.
    pub fn merge_index(&self) -> DataMergeIndex {
        DataMergeIndex {
            entries: self
                .entries
                .iter()
                .enumerate()
                .map(|(index, (label, _))| (label.clone(), index))
                .collect(),
            floats: self.float_entries.iter().cloned().collect(),
            words: self
                .word_entries
                .iter()
                .enumerate()
                .map(|(index, (label, _))| (label.clone(), index))
                .collect(),
        }
    }

    /// `merge`, against an index this section has not changed behind the back of.
    pub fn merge_indexed(&mut self, other: Self, index: &mut DataMergeIndex) {
        let labels = &mut index.entries;
        for (label, bytes) in other.entries {
            if let Some(&index) = labels.get(&label) {
                assert_same_content(&label, &self.entries[index].1, &bytes);
                continue;
            }
            labels.insert(label.clone(), self.entries.len());
            self.entries.push((label, bytes));
        }
        let float_labels = &mut index.floats;
        for (label, bits) in other.float_entries {
            if let Some(existing) = float_labels.get(&label) {
                assert_same_content(&label, existing, &bits);
                continue;
            }
            float_labels.insert(label.clone(), bits);
            self.float_entries.push((label, bits));
        }
        let word_labels = &mut index.words;
        for (label, words) in other.word_entries {
            if let Some(&index) = word_labels.get(&label) {
                assert_same_content(&label, &self.word_entries[index].1, &words);
                continue;
            }
            word_labels.insert(label.clone(), self.word_entries.len());
            self.word_entries.push((label, words));
        }
        for (label, size) in other.comm_entries {
            if self.comm_dedup.contains_key(&label) {
                continue;
            }
            self.comm_dedup.insert(label.clone(), label.clone());
            self.comm_entries.push((label, size));
        }
        for record in other.static_locals {
            self.record_static_local(record);
        }
        // Private identity tokens and extern callback slots are caller-named too: a shard that
        // recorded one hands it to the merged section exactly once.
        for label in other.private_identities {
            if !self.private_identities.contains(&label) {
                self.private_identities.push(label);
            }
        }
        for label in other.extern_callback_slots {
            if !self.extern_callback_slots.contains(&label) {
                self.extern_callback_slots.push(label);
            }
        }
    }

    /// Records one extern callback descriptor slot, so program exit and the `--web` request
    /// reset can release the descriptor it retains.
    pub fn record_extern_callback_slot(&mut self, label: String) {
        self.extern_callback_slots.push(label);
    }

    /// Returns the recorded extern callback descriptor slots in emission order.
    pub fn extern_callback_slots(&self) -> &[String] {
        &self.extern_callback_slots
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
        if let Some(label) = self.base.as_ref().and_then(|base| base.floats.get(&bits)) {
            return label.clone();
        }
        let label = if self.content_labels {
            format!("_float_h{bits:016x}")
        } else {
            let label = format!("_float_{}{}", self.label_shard, self.counter);
            self.counter += 1;
            label
        };
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
        if let Some(label) = self.base.as_ref().and_then(|base| base.strings.get(bytes)) {
            return (label.clone(), bytes.len());
        }

        let label = if self.content_labels {
            format!("_str_h{:016x}", content_label(&[b's'], bytes))
        } else {
            let label = format!("_str_{}{}", self.label_shard, self.counter);
            self.counter += 1;
            label
        };
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

    /// Reserves aligned, zero-initialized module-private storage for a runtime identity.
    /// No global symbol is emitted, so separately loaded images cannot share the token.
    pub fn add_private_identity(&mut self, label: &str) -> String {
        if !self.private_identities.iter().any(|existing| existing == label) {
            self.private_identities.push(label.to_owned());
        }
        label.to_owned()
    }

    /// Emits `words` under the caller-chosen symbol `name`, for a table other code reaches by a
    /// fixed name (the reflection rows). Idempotent by name; the table is part of the main
    /// section only, never of a parallel worker's shard.
    pub fn add_named_words(&mut self, name: &str, words: Vec<DataWord>) {
        if self.word_entries.iter().any(|(label, _)| label == name) {
            return;
        }
        self.word_entries.push((name.to_string(), words));
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

    /// Returns the declared common-symbol labels that start with `prefix`, in a stable order.
    pub fn comm_labels_with_prefix(&self, prefix: &str) -> Vec<String> {
        let mut labels: Vec<String> = self
            .comm_entries
            .iter()
            .filter(|(label, _)| label.starts_with(prefix))
            .map(|(label, _)| label.clone())
            .collect();
        labels.sort();
        labels
    }

    /// Adds words to the current runtime or metadata collection.
    pub fn add_words(&mut self, words: Vec<DataWord>) -> String {
        if let Some(label) = self.word_dedup.get(&words) {
            return label.clone();
        }
        if let Some(label) = self.base.as_ref().and_then(|base| base.words.get(&words)) {
            return label.clone();
        }
        let label = if self.content_labels {
            format!("_data_h{:016x}", words_content_label(&words))
        } else {
            let label = format!("_data_{}{}", self.label_shard, self.counter);
            self.counter += 1;
            label
        };
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
        // Lines are written straight into `out`: each `push_str(&format!(..))` built and copied a
        // temporary string, ~1,100 samples of a sampled Symfony demo console build.
        use std::fmt::Write as _;
        if self.entries.is_empty()
            && self.float_entries.is_empty()
            && self.word_entries.is_empty()
            && self.comm_entries.is_empty()
            && self.private_identities.is_empty()
        {
            return String::new();
        }

        let mut out = String::from(".data\n");
        for label in &self.private_identities {
            let _ = write!(out, ".p2align 3\n{}:\n    .quad 0\n", label);
        }
        let comm_align = comm_alignment_operand(target, COMM_ALIGN_BYTES);
        for (label, size) in &self.comm_entries {
            let _ = write!(out, ".comm {}, {}, {}\n", label, size, comm_align);
        }
        for (label, bytes) in &self.entries {
            let _ = write!(out, "{}:\n", label);
            out.push_str("    .ascii \"");
            for &b in bytes {
                match b {
                    b'\n' => out.push_str("\\n"),
                    b'\t' => out.push_str("\\t"),
                    b'\\' => out.push_str("\\\\"),
                    b'"' => out.push_str("\\\""),
                    0x20..=0x7e => out.push(b as char),
                    _ => {
                        let _ = write!(out, "\\{:03o}", b);
                    }
                }
            }
            out.push_str("\"\n");
        }
        for (label, bits) in &self.float_entries {
            let _ = write!(out, ".p2align 3\n{}:\n    .quad 0x{:016x}\n", label, bits);
        }
        for (label, words) in &self.word_entries {
            let _ = write!(out, ".p2align 3\n{}:\n", label);
            write_words(&mut out, words);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{comm_directive_aligned, DataSection, DataWord};
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// A Mach-O target, whose assembler reads `.comm`'s alignment operand as `log2(bytes)`.
    fn macos() -> Target {
        Target::new(Platform::MacOS, Arch::AArch64)
    }

    /// An ELF target, whose assembler reads `.comm`'s alignment operand as a byte count.
    fn linux(arch: Arch) -> Target {
        Target::new(Platform::Linux, arch)
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

        assert!(asm.contains(&format!("{}:\n", label)));
        assert!(!asm.contains(&format!(".globl {}\n", label)));
        assert!(asm.contains("    .quad 0x0000000000000001\n"));
        assert!(asm.contains("    .quad _fn_demo\n"));
    }

    /// A run of zero words is one `.zero` directive of the run's bytes and a lone zero stays a
    /// `.quad`, so the table keeps its exact byte layout in fewer lines.
    #[test]
    fn test_zero_word_runs_emit_one_zero_directive() {
        use super::DataWord::{Symbol, U64};
        let words = vec![U64(5), U64(0), U64(0), U64(0), Symbol("_fn_demo".to_string()), U64(0), U64(7), U64(0), U64(0)];
        let mut data = DataSection::new();
        let label = data.add_words(words);
        let asm = data.emit(macos());
        let block = asm.split(&format!("{label}:\n")).nth(1).expect("the table is emitted");
        assert_eq!(
            block,
            "    .quad 0x0000000000000005\n    .zero 24\n    .quad _fn_demo\n    .quad 0x0000000000000000\n    .quad 0x0000000000000007\n    .zero 16\n"
        );
        assert_eq!(block.lines().count(), 6, "nine words in six directives");
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

    /// Verifies callers can request the 16-byte storage alignment required by runtime heap cells.
    #[test]
    fn test_explicit_common_alignment_follows_the_object_format() {
        assert!(comm_directive_aligned("_heap_buf", 1024, 16, macos())
            .contains(".comm _heap_buf, 1024, 4\n"));
        assert!(comm_directive_aligned("_heap_buf", 1024, 16, linux(Arch::AArch64))
            .contains(".comm _heap_buf, 1024, 16\n"));
        assert!(comm_directive_aligned("_heap_buf", 1024, 16, linux(Arch::X86_64))
            .contains(".comm _heap_buf, 1024, 16\n"));
    }

    /// Verifies shards name what they intern by its content: two shards building the same
    /// string, table or float spell the same label, and different contents different ones.
    #[test]
    fn test_shards_name_contents_by_address() {
        let mut first = DataSection::for_shard(0, None);
        let mut second = DataSection::for_shard(1, None);
        let (first_label, _) = first.add_string(b"same bytes");
        let (second_label, _) = second.add_string(b"same bytes");
        assert_eq!(first_label, second_label);
        assert!(first_label.starts_with("_str_h"));
        assert_ne!(first.add_string(b"other bytes").0, first_label);
        assert_eq!(first.add_float(1.5), second.add_float(1.5));
        assert_eq!(
            first.add_words(vec![DataWord::U64(7), DataWord::Symbol("_x".to_string())]),
            second.add_words(vec![DataWord::U64(7), DataWord::Symbol("_x".to_string())])
        );
        assert_ne!(
            first.add_words(vec![DataWord::U64(7)]),
            first.add_words(vec![DataWord::U64(8)])
        );
    }

    /// Verifies a shard resolves content the main section already holds to the main label,
    /// without a copy of its own.
    #[test]
    fn test_a_shard_reuses_what_the_main_section_holds() {
        let mut main = DataSection::new();
        let (main_label, _) = main.add_string(b"interned ahead of the bodies");
        let mut shard = DataSection::for_shard(0, Some(main.base()));
        assert_eq!(shard.add_string(b"interned ahead of the bodies").0, main_label);
        main.merge(shard);
        assert_eq!(main.emit(macos()).matches("interned ahead of the bodies").count(), 1);
    }

    /// Verifies a whole-module section still emits the labels it emits today: the shard prefix
    /// is empty unless a caller asks for one.
    #[test]
    fn test_an_unsharded_section_keeps_its_plain_labels() {
        let mut data = DataSection::new();
        assert_eq!(data.add_string(b"x").0, "_str_0");
        assert_eq!(data.add_float(2.5), "_float_1");
    }

    /// Verifies merge keeps one copy of what two shards both interned, and collapses everything
    /// named by the caller.
    #[test]
    fn test_merge_keeps_one_copy_and_collapses_named_entries() {
        let mut first = DataSection::for_shard(0, None);
        let mut second = DataSection::for_shard(1, None);
        first.add_string(b"shared blob");
        second.add_string(b"shared blob");
        first.add_comm("_request_slot".to_string(), 8);
        second.add_comm("_request_slot".to_string(), 8);
        second.add_comm("_second_only".to_string(), 16);

        first.merge(second);
        let emitted = first.emit(macos());
        assert_eq!(emitted.matches("shared blob").count(), 1);
        assert_eq!(emitted.matches(".comm _request_slot").count(), 1);
        assert!(emitted.contains(".comm _second_only"));
        assert!(first.has_comm("_second_only"));
    }

    /// Verifies one label naming two different contents stops the merge instead of silently
    /// keeping one of them: that would be a content-address collision.
    #[test]
    #[should_panic(expected = "content-address collision")]
    fn test_merge_refuses_one_label_for_two_contents() {
        let mut first = DataSection::for_shard(0, None);
        let mut second = DataSection::for_shard(1, None);
        let (label, _) = first.add_string(b"one content");
        second.entries.push((label, b"another content".to_vec()));
        first.merge(second);
    }
}
