//! Purpose:
//! Assembles one slice of the user program's AArch64 Mach-O listing into an object file in
//! process, in place of the system `as`.
//!
//! Called from:
//! - `crate::linker::assemble_parallel` for every slice, when `ELEPHC_ASM=inhouse`.
//!
//! Key details:
//! - The listing the compiler writes uses a small, fixed subset of the assembler language: two
//!   sections (the default text section and `.data`), `.globl`/`.private_extern`, `.p2align`/
//!   `.align`, `.quad`/`.long`/`.byte`/`.ascii`/`.inst`, `.comm`, symbolic labels, `L`-local
//!   labels and numeric `1:` labels referenced as `1f`/`1b`. Anything else is
//!   [`Unsupported`]: the caller assembles the slice with `as`, so a gap costs speed, never
//!   correctness.
//! - Two passes over the text, nothing stored per line: the first places every line (sizes,
//!   alignment, label offsets), the second encodes it.
//! - Like `as`, a branch to a label this object defines is resolved here, page-relative and GOT
//!   accesses always become relocations, and a branch to an undefined symbol becomes
//!   `ARM64_RELOC_BRANCH26`.
//! - The object carries the `LC_BUILD_VERSION` `as` writes on this machine, read once from an
//!   object `as` itself produced, so the linker sees the same platform and versions.

mod encode;

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

use object::write::{Object, Relocation, StandardSection, Symbol, SymbolId, SymbolSection};
use object::{
    macho, Architecture, BinaryFormat, Endianness, RelocationFlags, SymbolFlags, SymbolKind,
    SymbolScope,
};

use encode::{encode_instruction, FixupKind};

/// A slice the in-tree assembler does not handle; the message names the first offending line.
#[derive(Debug)]
pub(crate) struct Unsupported(pub(crate) String);

type Result<T> = std::result::Result<T, Unsupported>;

fn unsupported<T>(what: &str, line: &str) -> Result<T> {
    Err(Unsupported(format!("{what}: {line}")))
}

/// The two sections a generated listing writes into.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
enum Sect {
    Text,
    Data,
}

/// `nop`, the padding `as` inserts when it aligns code.
const NOP: u32 = 0xD503_201F;

/// One classified line.
enum Line<'a> {
    Label(&'a str),
    Instruction(&'a str),
    Directive(&'a str, &'a str),
}

/// Classifies one trimmed, non-empty, non-comment line.
fn classify(line: &str) -> Line<'_> {
    if let Some(name) = line.strip_suffix(':') {
        return Line::Label(name);
    }
    if line.starts_with('.') {
        let (name, rest) = match line.bytes().position(|byte| byte.is_ascii_whitespace()) {
            Some(at) => (&line[..at], &line[at + 1..]),
            None => (line, ""),
        };
        return Line::Directive(name, rest.trim_ascii());
    }
    Line::Instruction(line)
}

/// Iterates the trimmed lines worth looking at (blank and comment lines are skipped).
///
/// One `memchr_iter` sweep instead of `str::split`: the split restarted a search per line, and
/// on ~26-byte lines that per-call setup was a third of the assembler's time.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    let mut start = 0usize;
    memchr::memchr_iter(b'\n', text.as_bytes())
        .chain(std::iter::once(text.len()))
        .map(move |end| {
            let line = &text[start..end];
            start = end + 1;
            line
        })
        .map(str::trim_ascii)
        .filter(|line| {
        !line.is_empty() && !line.starts_with("//") && !line.starts_with(';') && !line.starts_with('#')
    })
}

/// A numeric local label: `1:` defines one, `1f`/`1b` refer to the next or previous definition.
fn numeric_label(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit())
}

/// Where a label landed.
#[derive(Clone, Copy, Debug)]
struct Place {
    sect: Sect,
    offset: u64,
}

/// What the first pass learns: every label's place and every symbol attribute.
#[derive(Default)]
struct Layout<'a> {
    labels: HashMap<&'a str, Place>,
    /// Per numeric label, its definitions in order: (index of the line, place).
    numeric: HashMap<&'a str, Vec<(usize, Place)>>,
    /// `.globl` (false) or `.private_extern` (true) names.
    exported: HashMap<&'a str, bool>,
    /// `.comm name, size, align_log2`.
    commons: Vec<(&'a str, u64, u64)>,
    text_align: u64,
    data_align: u64,
    subsections_via_symbols: bool,
}

/// Bytes a `.ascii` operand decodes to (`\n`, `\t`, `\\`, `\"`, `\NNN` octal), appended to `out`.
fn decode_ascii(operand: &str, out: &mut Vec<u8>) -> Option<()> {
    let body = operand.strip_prefix('"')?.strip_suffix('"')?;
    let bytes = body.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte != b'\\' {
            out.push(byte);
            index += 1;
            continue;
        }
        let escaped = *bytes.get(index + 1)?;
        match escaped {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'\\' => out.push(b'\\'),
            b'"' => out.push(b'"'),
            b'\'' => out.push(b'\''),
            b'0'..=b'7' => {
                let mut value = 0u32;
                let mut digits = 0;
                while digits < 3 {
                    match bytes.get(index + 1 + digits) {
                        Some(digit @ b'0'..=b'7') => value = value * 8 + u32::from(digit - b'0'),
                        _ => break,
                    }
                    digits += 1;
                }
                out.push(u8::try_from(value).ok()?);
                index += 1 + digits;
                continue;
            }
            _ => return None,
        }
        index += 2;
    }
    Some(())
}

/// Splits a data directive's operand list at commas (none of its operands contain one).
fn data_operands(rest: &str) -> impl Iterator<Item = &str> {
    rest.split(',').map(str::trim_ascii)
}

/// Parses a plain integer operand: decimal or `0x` hexadecimal, optionally negative.
fn integer(text: &str) -> Option<i64> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let value = match digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok()? as i64,
        None => digits.parse::<u64>().ok()? as i64,
    };
    Some(if negative { value.wrapping_neg() } else { value })
}

/// The size in bytes a data directive occupies (`.ascii` is decoded to count it).
fn data_size(name: &str, rest: &str, scratch: &mut Vec<u8>) -> Option<u64> {
    let width = match name {
        ".quad" | ".xword" => 8,
        ".long" | ".word" | ".inst" | ".4byte" => 4,
        ".short" | ".hword" | ".2byte" => 2,
        ".byte" => 1,
        ".ascii" | ".asciz" => {
            scratch.clear();
            decode_ascii(rest, scratch)?;
            return Some(scratch.len() as u64 + u64::from(name == ".asciz"));
        }
        ".space" | ".zero" | ".skip" => return u64::try_from(integer(rest)?).ok(),
        _ => return None,
    };
    Some(width * data_operands(rest).count() as u64)
}

/// The padding to bring `offset` to a multiple of `1 << log2`.
fn padding(offset: u64, log2: u64) -> u64 {
    let align = 1u64 << log2;
    (align - offset % align) % align
}

/// First pass: places every line and records labels and symbol attributes.
fn layout(text: &str) -> Result<Layout<'_>> {
    let mut layout = Layout { text_align: 2, data_align: 0, ..Layout::default() };
    let mut sect = Sect::Text;
    let mut offsets = [0u64; 2];
    let mut scratch = Vec::new();
    for (index, line) in lines(text).enumerate() {
        let offset = &mut offsets[sect as usize];
        match classify(line) {
            Line::Label(name) => {
                let place = Place { sect, offset: *offset };
                if numeric_label(name) {
                    layout.numeric.entry(name).or_default().push((index, place));
                } else if layout.labels.insert(name, place).is_some() {
                    return unsupported("label defined twice", line);
                }
            }
            Line::Instruction(_) => {
                if sect != Sect::Text {
                    return unsupported("instruction outside the text section", line);
                }
                *offset += 4;
            }
            Line::Directive(name, rest) => match name {
                ".text" => sect = Sect::Text,
                ".data" => sect = Sect::Data,
                ".globl" | ".global" => {
                    layout.exported.entry(rest).or_insert(false);
                }
                ".private_extern" => {
                    layout.exported.insert(rest, true);
                }
                ".p2align" | ".align" => {
                    let log2 = rest.split(',').next().and_then(integer).and_then(|v| u64::try_from(v).ok());
                    let Some(log2) = log2.filter(|log2| *log2 <= 15) else {
                        return unsupported("alignment", line);
                    };
                    *offset += padding(*offset, log2);
                    let align = match sect {
                        Sect::Text => &mut layout.text_align,
                        Sect::Data => &mut layout.data_align,
                    };
                    *align = (*align).max(log2);
                }
                ".comm" => {
                    let mut parts = data_operands(rest);
                    let (Some(symbol), Some(size)) = (parts.next(), parts.next().and_then(integer)) else {
                        return unsupported("common symbol", line);
                    };
                    let align = parts.next().and_then(integer).unwrap_or(0);
                    layout.commons.push((symbol, size as u64, align as u64));
                }
                ".subsections_via_symbols" => layout.subsections_via_symbols = true,
                _ => match data_size(name, rest, &mut scratch) {
                    Some(size) => *offset += size,
                    None => return unsupported("directive", line),
                },
            },
        }
    }
    Ok(layout)
}

/// A relocation the second pass records, applied once every symbol exists.
struct PendingReloc<'a> {
    sect: Sect,
    offset: u64,
    target: &'a str,
    addend: i64,
    r_type: u8,
    r_pcrel: bool,
    r_length: u8,
}

/// Resolves a numeric reference (`1f`, `1b`) made on line `index`.
fn numeric_target(layout: &Layout<'_>, reference: &str, index: usize) -> Option<Place> {
    let (digits, direction) = reference.split_at(reference.len().checked_sub(1)?);
    if !numeric_label(digits) {
        return None;
    }
    let definitions = layout.numeric.get(digits)?;
    // Definitions are in line order, so the nearest one on either side is a binary search away:
    // a Symfony slice defines `1:` over a hundred thousand times.
    let after = definitions.partition_point(|(at, _)| *at <= index);
    match direction {
        "f" => definitions.get(after).map(|(_, place)| *place),
        "b" => {
            let before = definitions.partition_point(|(at, _)| *at < index);
            before.checked_sub(1).and_then(|at| definitions.get(at)).map(|(_, place)| *place)
        }
        _ => None,
    }
}

/// Patches a PC-relative field of `word` for a target `distance` bytes away, or `None` when the
/// distance does not fit.
fn patch_branch(word: u32, kind: FixupKind, distance: i64) -> Option<u32> {
    let fits = |bits: u32, value: i64| value >= -(1 << (bits - 1)) && value < (1 << (bits - 1));
    match kind {
        FixupKind::Branch26 => {
            let words = distance / 4;
            (distance % 4 == 0 && fits(26, words)).then(|| word | (words as u32 & 0x03FF_FFFF))
        }
        FixupKind::Branch19 => {
            let words = distance / 4;
            (distance % 4 == 0 && fits(19, words)).then(|| word | ((words as u32 & 0x7_FFFF) << 5))
        }
        FixupKind::Branch14 => {
            let words = distance / 4;
            (distance % 4 == 0 && fits(14, words)).then(|| word | ((words as u32 & 0x3FFF) << 5))
        }
        FixupKind::Adr21 => fits(21, distance).then(|| {
            let value = distance as u32;
            word | (value & 0x3) << 29 | ((value >> 2) & 0x7_FFFF) << 5
        }),
        _ => None,
    }
}

/// The value of `A - B` for two labels this object defines in one section.
fn label_difference(layout: &Layout<'_>, operand: &str) -> Option<i64> {
    let (left, right) = operand.split_once(" - ")?;
    let (left, right) = (layout.labels.get(left.trim_ascii())?, layout.labels.get(right.trim_ascii())?);
    (left.sect == right.sect).then(|| left.offset as i64 - right.offset as i64)
}

/// Splits `sym+8` / `sym-8` into the symbol and its addend.
fn symbol_and_addend(operand: &str) -> (&str, i64) {
    if let Some(at) = operand.rfind(['+', '-']).filter(|at| *at > 0) {
        if let Some(addend) = integer(operand[at..].trim_start_matches('+')) {
            return (operand[..at].trim_ascii(), addend);
        }
    }
    (operand, 0)
}

/// Whether `text` is a plausible symbol or label name.
fn is_symbol(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_alphabetic() || matches!(first, b'_' | b'.' | b'$'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$'))
}

/// Assembles one slice into a Mach-O arm64 object.
pub(crate) fn assemble(text: &str) -> Result<Vec<u8>> {
    let layout = layout(text)?;
    let mut sections: [Vec<u8>; 2] = [Vec::new(), Vec::new()];
    let mut relocs: Vec<PendingReloc<'_>> = Vec::new();
    let mut sect = Sect::Text;
    for (index, line) in lines(text).enumerate() {
        let bytes = &mut sections[sect as usize];
        match classify(line) {
            Line::Label(_) => {}
            Line::Instruction(instruction) => {
                let Some(encoded) = encode_instruction(instruction) else {
                    return unsupported("instruction", line);
                };
                let offset = bytes.len() as u64;
                let mut word = encoded.word;
                if let Some(fixup) = encoded.fixup {
                    let local = if numeric_label(fixup.target.trim_end_matches(['f', 'b'])) {
                        Some(numeric_target(&layout, fixup.target, index).ok_or_else(|| {
                            Unsupported(format!("numeric label: {line}"))
                        })?)
                    } else {
                        layout.labels.get(fixup.target).copied()
                    };
                    let (r_type, r_pcrel) = match fixup.kind {
                        FixupKind::Page21 => (macho::ARM64_RELOC_PAGE21, true),
                        FixupKind::GotPage21 => (macho::ARM64_RELOC_GOT_LOAD_PAGE21, true),
                        FixupKind::PageOff12 => (macho::ARM64_RELOC_PAGEOFF12, false),
                        FixupKind::GotPageOff12 => (macho::ARM64_RELOC_GOT_LOAD_PAGEOFF12, false),
                        FixupKind::Branch26 | FixupKind::Branch19 | FixupKind::Branch14 | FixupKind::Adr21 => {
                            match local {
                                Some(place) if place.sect == Sect::Text => {
                                    let distance = place.offset as i64 + fixup.addend - offset as i64;
                                    word = patch_branch(word, fixup.kind, distance)
                                        .ok_or_else(|| Unsupported(format!("branch out of range: {line}")))?;
                                    bytes.extend_from_slice(&word.to_le_bytes());
                                    continue;
                                }
                                Some(_) => return unsupported("branch into data", line),
                                None if fixup.kind == FixupKind::Branch26 => {
                                    (macho::ARM64_RELOC_BRANCH26, true)
                                }
                                None => return unsupported("short branch to an undefined symbol", line),
                            }
                        }
                    };
                    if numeric_label(fixup.target.trim_end_matches(['f', 'b'])) {
                        return unsupported("page relocation to a numeric label", line);
                    }
                    relocs.push(PendingReloc {
                        sect,
                        offset,
                        target: fixup.target,
                        addend: fixup.addend,
                        r_type,
                        r_pcrel,
                        r_length: 2,
                    });
                }
                bytes.extend_from_slice(&word.to_le_bytes());
            }
            Line::Directive(name, rest) => match name {
                ".text" => sect = Sect::Text,
                ".data" => sect = Sect::Data,
                ".globl" | ".global" | ".private_extern" | ".comm" | ".subsections_via_symbols" => {}
                ".p2align" | ".align" => {
                    let log2 = rest.split(',').next().and_then(integer).unwrap_or(0) as u64;
                    let pad = padding(bytes.len() as u64, log2);
                    if sect == Sect::Text && pad % 4 == 0 {
                        for _ in 0..pad / 4 {
                            bytes.extend_from_slice(&NOP.to_le_bytes());
                        }
                    } else {
                        bytes.resize(bytes.len() + pad as usize, 0);
                    }
                }
                ".ascii" | ".asciz" => {
                    if decode_ascii(rest, bytes).is_none() {
                        return unsupported("string", line);
                    }
                    if name == ".asciz" {
                        bytes.push(0);
                    }
                }
                ".space" | ".zero" | ".skip" => {
                    let size = integer(rest).and_then(|size| usize::try_from(size).ok());
                    let Some(size) = size else { return unsupported("space", line) };
                    bytes.resize(bytes.len() + size, 0);
                }
                _ => {
                    let width: usize = match name {
                        ".quad" | ".xword" => 8,
                        ".long" | ".word" | ".inst" | ".4byte" => 4,
                        ".short" | ".hword" | ".2byte" => 2,
                        ".byte" => 1,
                        _ => return unsupported("directive", line),
                    };
                    for operand in data_operands(rest) {
                        let offset = bytes.len() as u64;
                        let value = if let Some(value) = integer(operand) {
                            value
                        } else if let Some(value) = label_difference(&layout, operand) {
                            value
                        } else {
                            let (symbol, addend) = symbol_and_addend(operand);
                            if width != 8 || !is_symbol(symbol) {
                                return unsupported("data operand", line);
                            }
                            relocs.push(PendingReloc {
                                sect,
                                offset,
                                target: symbol,
                                addend,
                                r_type: macho::ARM64_RELOC_UNSIGNED,
                                r_pcrel: false,
                                r_length: 3,
                            });
                            0
                        };
                        bytes.extend_from_slice(&value.to_le_bytes()[..width]);
                    }
                }
            },
        }
    }
    write_object(&layout, &sections, relocs)
}

/// Builds the Mach-O object from the placed sections, the labels and the relocations.
fn write_object(layout: &Layout<'_>, sections: &[Vec<u8>; 2], relocs: Vec<PendingReloc<'_>>) -> Result<Vec<u8>> {
    let mut object = Object::new(BinaryFormat::MachO, Architecture::Aarch64, Endianness::Little);
    // The listing already spells Mach-O names (`_main`); the writer must not prefix them again.
    object.set_mangling(object::write::Mangling::None);
    if let Some(version) = build_version() {
        object.set_macho_build_version(version);
    }
    if layout.subsections_via_symbols {
        object.set_subsections_via_symbols();
    }
    let text = object.section_id(StandardSection::Text);
    object.append_section_data(text, &sections[Sect::Text as usize], 1 << layout.text_align);
    let data = (!sections[Sect::Data as usize].is_empty()).then(|| {
        let data = object.section_id(StandardSection::Data);
        object.append_section_data(data, &sections[Sect::Data as usize], 1 << layout.data_align);
        data
    });
    let section_of = |sect: Sect| match sect {
        Sect::Text => Some(text),
        Sect::Data => data,
    };

    let referenced: HashSet<&str> = relocs.iter().map(|reloc| reloc.target).collect();
    let mut symbols: HashMap<&str, SymbolId> = HashMap::default();
    // Defined labels, in a stable order: by section, then offset, then name.
    let mut defined: Vec<(&&str, &Place)> = layout.labels.iter().collect();
    defined.sort_by(|(left_name, left), (right_name, right)| {
        (left.sect as u8, left.offset, **left_name).cmp(&(right.sect as u8, right.offset, **right_name))
    });
    for (name, place) in defined {
        let exported = layout.exported.get(*name).copied();
        // An `L` label is assembler-local: `as` keeps it out of the symbol table unless a
        // relocation needs a symbol for it.
        if exported.is_none() && name.starts_with('L') && !referenced.contains(*name) {
            continue;
        }
        let Some(section) = section_of(place.sect) else {
            return unsupported("label in an empty section", name);
        };
        let scope = match exported {
            Some(false) => SymbolScope::Dynamic,
            Some(true) => SymbolScope::Linkage,
            None => SymbolScope::Compilation,
        };
        let id = object.add_symbol(Symbol {
            name: name.as_bytes().to_vec(),
            value: place.offset,
            size: 0,
            kind: if place.sect == Sect::Text { SymbolKind::Text } else { SymbolKind::Data },
            scope,
            weak: false,
            section: SymbolSection::Section(section),
            flags: SymbolFlags::None,
        });
        symbols.insert(name, id);
    }
    for (name, size, align_log2) in &layout.commons {
        let scope = match layout.exported.get(name) {
            Some(true) => SymbolScope::Linkage,
            _ => SymbolScope::Dynamic,
        };
        let symbol = Symbol {
            name: name.as_bytes().to_vec(),
            value: 0,
            size: 0,
            kind: SymbolKind::Data,
            scope,
            weak: false,
            section: SymbolSection::Undefined,
            flags: SymbolFlags::None,
        };
        let id = object.add_common_symbol(symbol, *size, 1 << align_log2);
        symbols.insert(name, id);
    }
    for reloc in relocs {
        let symbol = match symbols.get(reloc.target) {
            Some(id) => *id,
            None => {
                if !is_symbol(reloc.target) {
                    return unsupported("relocation target", reloc.target);
                }
                let id = object.add_symbol(Symbol {
                    name: reloc.target.as_bytes().to_vec(),
                    value: 0,
                    size: 0,
                    kind: SymbolKind::Unknown,
                    scope: SymbolScope::Dynamic,
                    weak: false,
                    section: SymbolSection::Undefined,
                    flags: SymbolFlags::None,
                });
                symbols.insert(reloc.target, id);
                id
            }
        };
        let Some(section) = section_of(reloc.sect) else {
            return unsupported("relocation in an empty section", reloc.target);
        };
        object
            .add_relocation(
                section,
                Relocation {
                    offset: reloc.offset,
                    symbol,
                    addend: reloc.addend,
                    flags: RelocationFlags::MachO {
                        r_type: reloc.r_type,
                        r_pcrel: reloc.r_pcrel,
                        r_length: reloc.r_length,
                    },
                },
            )
            .map_err(|error| Unsupported(format!("relocation: {error}")))?;
    }
    object.write().map_err(|error| Unsupported(format!("object: {error}")))
}

/// The `LC_BUILD_VERSION` the system assembler writes on this machine, read once from an object
/// it produced for an empty source.
fn build_version() -> Option<object::write::MachOBuildVersion> {
    static VERSION: std::sync::OnceLock<Option<(u32, u32, u32)>> = std::sync::OnceLock::new();
    let (platform, minos, sdk) = (*VERSION.get_or_init(read_assembler_build_version))?;
    // The struct is `#[non_exhaustive]`, so it is built from its default.
    let mut version = object::write::MachOBuildVersion::default();
    version.platform = platform;
    version.minos = minos;
    version.sdk = sdk;
    Some(version)
}

/// Assembles an empty source with `as -arch arm64` and reads its `LC_BUILD_VERSION`.
fn read_assembler_build_version() -> Option<(u32, u32, u32)> {
    let dir = std::env::temp_dir().join(format!("elephc-inhouse-version-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let source = dir.join("empty.s");
    let output = dir.join("empty.o");
    std::fs::write(&source, "").ok()?;
    let status = std::process::Command::new("as")
        .args(["-arch", "arm64", "-o"])
        .arg(&output)
        .arg(&source)
        .status()
        .ok()?;
    let bytes = status.success().then(|| std::fs::read(&output).ok()).flatten();
    let _ = std::fs::remove_dir_all(&dir);
    let bytes = bytes?;
    let word = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let ncmds = word(16)? as usize;
    let mut at = 32usize;
    for _ in 0..ncmds {
        let (cmd, size) = (word(at)?, word(at + 4)? as usize);
        if cmd == macho::LC_BUILD_VERSION {
            return Some((word(at + 8)?, word(at + 12)?, word(at + 16)?));
        }
        at += size;
    }
    None
}

#[cfg(test)]
mod tests;
