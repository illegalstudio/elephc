//! Purpose:
//! Reads function symbols and the load bias out of an ELF64 image, so a process
//! can be symbolised from the outside without DWARF and without an in-process
//! helper.
//!
//! Called from:
//! - `crate::monitor` on Linux, where `--attach` is handed a pid that is already
//!   running under someone else's control and has no channel to answer on.
//!
//! Key details:
//! - Parsing only. Every function here takes bytes and returns values, so the
//!   whole of it is exercised by ordinary tests on any host, which matters for
//!   code whose real use is on a platform its author may not be able to run.
//! - Little-endian ELF64 only, which is what both supported Linux targets are.
//!   A file that is anything else is refused rather than misread.

/// One function the target's image defines, in the addresses the FILE uses.
///
/// Runtime addresses are these plus the load bias; keeping the two apart is
/// deliberate, because mixing them is the mistake this module exists to avoid
/// and the one that produces a plausible, wrong symbol rather than none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FuncSymbol {
    pub(crate) value: u64,
    pub(crate) size: u64,
    pub(crate) name: String,
}

const EI_NIDENT: usize = 16;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const SHT_SYMTAB: u32 = 2;
const SHT_DYNSYM: u32 = 11;
const PT_LOAD: u32 = 1;
const STT_FUNC: u8 = 2;

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    bytes.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    bytes.get(at..at + 8).map(|b| {
        u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
    })
}

/// Whether `bytes` is a little-endian ELF64 image.
///
/// Checked before anything is indexed. Every read below is bounds-checked too,
/// but a wrong-class file would otherwise be parsed field by field into numbers
/// that mean nothing — and a symbol table built from those is worse than an
/// empty one, because it answers.
fn is_elf64_le(bytes: &[u8]) -> bool {
    bytes.len() > EI_NIDENT
        && bytes[0..4] == [0x7f, b'E', b'L', b'F']
        && bytes[4] == ELFCLASS64
        && bytes[5] == ELFDATA2LSB
}

/// The lowest virtual address any `PT_LOAD` segment asks for.
///
/// This is what a runtime mapping is biased AGAINST: the loader places the image
/// somewhere, and the distance from that placement to this value is the bias
/// every file address needs. A non-PIE image asks for a fixed address and its
/// bias comes out zero, which is the same arithmetic rather than a special case.
pub(crate) fn first_load_vaddr(bytes: &[u8]) -> Option<u64> {
    if !is_elf64_le(bytes) {
        return None;
    }
    let phoff = u64_at(bytes, 0x20)? as usize;
    let phentsize = u16_at(bytes, 0x36)? as usize;
    let phnum = u16_at(bytes, 0x38)? as usize;
    let mut lowest: Option<u64> = None;
    for index in 0..phnum {
        let at = phoff.checked_add(index.checked_mul(phentsize)?)?;
        if u32_at(bytes, at)? != PT_LOAD {
            continue;
        }
        let vaddr = u64_at(bytes, at + 0x10)?;
        lowest = Some(lowest.map_or(vaddr, |low: u64| low.min(vaddr)));
    }
    lowest
}

/// Every `STT_FUNC` symbol the image defines, with its file address.
///
/// `.symtab` is preferred and `.dynsym` is the fallback: a stripped binary keeps
/// only the latter, and half a symbol table still names more frames than none.
/// Symbols with no address are dropped — an undefined symbol names a function
/// this image does not contain, and letting one through would attribute a frame
/// to a body that is somewhere else entirely.
pub(crate) fn function_symbols(bytes: &[u8]) -> Vec<FuncSymbol> {
    let mut best: Vec<FuncSymbol> = Vec::new();
    for wanted in [SHT_SYMTAB, SHT_DYNSYM] {
        let found = symbols_from_section_kind(bytes, wanted);
        if !found.is_empty() {
            best = found;
            break;
        }
    }
    // A symbol table is in whatever order the linker wrote it, which is not
    // address order — and `symbolize` binary-searches. Sorting HERE rather than
    // asking every caller to is what makes the two halves fit: the requirement
    // belongs to the search, so the guarantee belongs to the thing that feeds
    // it. Unsorted, the search does not fail loudly; it silently answers `None`
    // for nearly every address, and a profile of a real program comes back
    // naming nothing at all.
    //
    // By name on a tie, so two symbols at one address resolve the same way on
    // every run rather than on symbol-table order.
    best.sort_by(|a, b| a.value.cmp(&b.value).then_with(|| a.name.cmp(&b.name)));
    best
}

fn symbols_from_section_kind(bytes: &[u8], kind: u32) -> Vec<FuncSymbol> {
    let mut out = Vec::new();
    if !is_elf64_le(bytes) {
        return out;
    }
    let Some(shoff) = u64_at(bytes, 0x28).map(|v| v as usize) else {
        return out;
    };
    let Some(shentsize) = u16_at(bytes, 0x3a).map(|v| v as usize) else {
        return out;
    };
    let Some(shnum) = u16_at(bytes, 0x3c).map(|v| v as usize) else {
        return out;
    };
    for index in 0..shnum {
        let Some(at) = shoff.checked_add(index.wrapping_mul(shentsize)) else {
            return out;
        };
        if u32_at(bytes, at + 4) != Some(kind) {
            continue;
        }
        // `sh_link` on a symbol table names the string table its names live in.
        // Following it rather than guessing `.strtab` is what makes this work on
        // a stripped image, where the only pair left is `.dynsym`/`.dynstr`.
        let (Some(off), Some(size), Some(entsize), Some(link)) = (
            u64_at(bytes, at + 0x18).map(|v| v as usize),
            u64_at(bytes, at + 0x20).map(|v| v as usize),
            u64_at(bytes, at + 0x38).map(|v| v as usize),
            u32_at(bytes, at + 0x28).map(|v| v as usize),
        ) else {
            continue;
        };
        if entsize == 0 || size == 0 {
            continue;
        }
        let str_at = shoff.wrapping_add(link.wrapping_mul(shentsize));
        let Some(str_off) = u64_at(bytes, str_at + 0x18).map(|v| v as usize) else {
            continue;
        };
        let Some(str_size) = u64_at(bytes, str_at + 0x20).map(|v| v as usize) else {
            continue;
        };
        for entry in 0..(size / entsize) {
            let sym = off + entry * entsize;
            let (Some(name_off), Some(info), Some(value), Some(sym_size)) = (
                u32_at(bytes, sym).map(|v| v as usize),
                bytes.get(sym + 4).copied(),
                u64_at(bytes, sym + 8),
                u64_at(bytes, sym + 16),
            ) else {
                break;
            };
            if info & 0xf != STT_FUNC || value == 0 {
                continue;
            }
            let Some(name) = c_string_at(bytes, str_off, str_size, name_off) else {
                continue;
            };
            out.push(FuncSymbol { value, size: sym_size, name });
        }
    }
    out
}

/// Reads one NUL-terminated name out of a string table, refusing anything that
/// would run past its end.
fn c_string_at(bytes: &[u8], table_off: usize, table_size: usize, at: usize) -> Option<String> {
    if at >= table_size {
        return None;
    }
    let start = table_off.checked_add(at)?;
    let end = table_off.checked_add(table_size)?;
    let slice = bytes.get(start..end.min(bytes.len()))?;
    let len = slice.iter().position(|byte| *byte == 0)?;
    if len == 0 {
        return None;
    }
    std::str::from_utf8(&slice[..len]).ok().map(str::to_string)
}

/// The distance between where the loader put the image and where the file asked
/// to be, read from `/proc/<pid>/maps`.
///
/// Matched on the executable mapping of `exe`, not merely the first line naming
/// it: a binary's data and read-only segments are mapped from the same path, and
/// taking one of those as the base gives a bias that is wrong by a segment —
/// which symbolises every frame to a plausible neighbour rather than failing.
pub(crate) fn load_bias(maps: &str, exe: &str, first_vaddr: u64) -> Option<u64> {
    for line in maps.lines() {
        // `<range> <perms> <offset> <dev> <inode> <path>`, and the fields are
        // taken by INDEX. Reading them off a half-consumed iterator is how the
        // device number was once used as the file offset — a bias wrong by a
        // whole segment, which names every frame after a plausible neighbour
        // instead of failing.
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (Some(range), Some(perms), Some(offset), Some(path)) =
            (fields.first(), fields.get(1), fields.get(2), fields.get(5))
        else {
            continue;
        };
        if !perms.contains('x') || *path != exe {
            continue;
        }
        let start = u64::from_str_radix(range.split('-').next()?, 16).ok()?;
        let offset = u64::from_str_radix(offset, 16).ok()?;
        // The mapping offset says which part of the file this is; the bias is
        // measured against the segment's own request, not the file's start.
        return Some(start.wrapping_sub(first_vaddr).wrapping_sub(offset));
    }
    None
}

/// Names the function containing `addr`, given file-address symbols and a bias.
///
/// Sized symbols answer only for addresses inside them. An unsized one — which
/// hand-written assembly often produces — answers for anything from it up to the
/// next symbol, because refusing would leave the runtime's own helpers unnamed
/// and those are exactly the frames a reader needs to recognise.
pub(crate) fn symbolize(sorted: &[FuncSymbol], bias: u64, addr: u64) -> Option<&str> {
    let file_addr = addr.checked_sub(bias)?;
    let index = match sorted.binary_search_by(|entry| entry.value.cmp(&file_addr)) {
        Ok(exact) => exact,
        Err(0) => return None,
        Err(after) => after - 1,
    };
    let entry = sorted.get(index)?;
    if entry.size > 0 && file_addr >= entry.value + entry.size {
        // Past the end of a sized symbol: the address belongs to whatever the
        // linker put next, and naming it after this one would be a guess.
        return None;
    }
    Some(&entry.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the smallest ELF64 image that carries one `PT_LOAD` and one
    /// symbol table, so the parser is exercised on bytes rather than on a file
    /// this host may not be able to produce.
    fn image(sym_value: u64, sym_size: u64, name: &str, load_vaddr: u64) -> Vec<u8> {
        image_of(&[(sym_value, sym_size, name)], load_vaddr)
    }

    /// The same, carrying several symbols in the order given — which is how a
    /// real symbol table arrives, and never sorted by address.
    fn image_of(symbols: &[(u64, u64, &str)], load_vaddr: u64) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x40];
        bytes[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
        bytes[4] = ELFCLASS64;
        bytes[5] = ELFDATA2LSB;

        // One program header, one PT_LOAD.
        let phoff = 0x40usize;
        bytes.resize(phoff + 56, 0);
        bytes[0x20..0x28].copy_from_slice(&(phoff as u64).to_le_bytes());
        bytes[0x36..0x38].copy_from_slice(&56u16.to_le_bytes());
        bytes[0x38..0x3a].copy_from_slice(&1u16.to_le_bytes());
        bytes[phoff..phoff + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        bytes[phoff + 0x10..phoff + 0x18].copy_from_slice(&load_vaddr.to_le_bytes());

        // A string table holding a leading NUL and every name.
        let str_off = bytes.len();
        bytes.push(0);
        let mut name_offsets = Vec::new();
        for (_, _, name) in symbols {
            name_offsets.push(bytes.len() - str_off);
            bytes.extend_from_slice(name.as_bytes());
            bytes.push(0);
        }
        let str_size = bytes.len() - str_off;

        // The symbol entries, in the order given.
        let sym_off = bytes.len();
        for ((value, size, _), name_off) in symbols.iter().zip(&name_offsets) {
            let mut sym = vec![0u8; 24];
            sym[0..4].copy_from_slice(&(*name_off as u32).to_le_bytes());
            sym[4] = STT_FUNC;
            sym[8..16].copy_from_slice(&value.to_le_bytes());
            sym[16..24].copy_from_slice(&size.to_le_bytes());
            bytes.extend_from_slice(&sym);
        }
        let sym_size = bytes.len() - sym_off;

        // Two sections: the symbol table, and the string table it links to.
        let shoff = bytes.len();
        let shentsize = 64usize;
        let mut sections = vec![0u8; shentsize * 2];
        sections[4..8].copy_from_slice(&SHT_SYMTAB.to_le_bytes());
        sections[0x18..0x20].copy_from_slice(&(sym_off as u64).to_le_bytes());
        sections[0x20..0x28].copy_from_slice(&(sym_size as u64).to_le_bytes());
        sections[0x28..0x2c].copy_from_slice(&1u32.to_le_bytes());
        sections[0x38..0x40].copy_from_slice(&24u64.to_le_bytes());
        let second = shentsize;
        sections[second + 0x18..second + 0x20].copy_from_slice(&(str_off as u64).to_le_bytes());
        sections[second + 0x20..second + 0x28].copy_from_slice(&(str_size as u64).to_le_bytes());
        bytes.extend_from_slice(&sections);
        bytes[0x28..0x30].copy_from_slice(&(shoff as u64).to_le_bytes());
        bytes[0x3a..0x3c].copy_from_slice(&(shentsize as u16).to_le_bytes());
        bytes[0x3c..0x3e].copy_from_slice(&2u16.to_le_bytes());
        bytes
    }

    /// A symbol table arrives in the linker's order, and the search over it
    /// needs address order.
    ///
    /// The two halves were written together and never met: nothing called both
    /// until an attached sampler did, and then the failure was silent in the
    /// worst way. A binary search over unsorted input does not error — it just
    /// answers `None` for nearly every address. A real program came back
    /// profiled as 251 samples of `<native>`, which reads as "this program is
    /// not PHP" rather than as "the names were never looked up".
    ///
    /// So the order is asserted where it is PRODUCED. A test that sorted first
    /// and then searched would have passed against the bug.
    #[test]
    fn symbols_come_out_in_address_order_whatever_order_they_were_written_in() {
        // Descending, which is close to what the table looked like in practice.
        let bytes = image_of(
            &[(0x9240, 0, "_php_last"), (0x8100, 0, "_php_middle"), (0x2000, 0, "_php_first")],
            0,
        );
        let symbols = function_symbols(&bytes);
        let values: Vec<u64> = symbols.iter().map(|entry| entry.value).collect();
        assert_eq!(values, vec![0x2000, 0x8100, 0x9240], "{symbols:?}");

        // And the consequence: addresses resolve, including ones BETWEEN
        // symbols, which is where an interrupted program actually is.
        assert_eq!(symbolize(&symbols, 0, 0x2000), Some("_php_first"));
        assert_eq!(symbolize(&symbols, 0, 0x8104), Some("_php_middle"));
        assert_eq!(symbolize(&symbols, 0, 0x9300), Some("_php_last"));
        // Below the first symbol there is nothing to name an address after.
        assert_eq!(symbolize(&symbols, 0, 0x100), None);
    }

    #[test]
    fn reads_a_function_symbol_and_the_first_load_address() {
        let bytes = image(0x1200, 0x40, "php_hot", 0x1000);
        assert_eq!(first_load_vaddr(&bytes), Some(0x1000));
        assert_eq!(
            function_symbols(&bytes),
            vec![FuncSymbol { value: 0x1200, size: 0x40, name: "php_hot".into() }]
        );
    }

    /// Anything that is not a little-endian ELF64 is refused rather than parsed
    /// into numbers that mean nothing. A symbol table built from those would
    /// answer, and an answer is worse than nothing here.
    #[test]
    fn refuses_what_is_not_a_little_endian_elf64() {
        let mut wrong_class = image(0x1200, 0x40, "php_hot", 0x1000);
        wrong_class[4] = 1; // ELFCLASS32
        assert!(function_symbols(&wrong_class).is_empty());
        assert_eq!(first_load_vaddr(&wrong_class), None);

        let mut big_endian = image(0x1200, 0x40, "php_hot", 0x1000);
        big_endian[5] = 2; // ELFDATA2MSB
        assert!(function_symbols(&big_endian).is_empty());

        assert!(function_symbols(b"not an elf at all").is_empty());
    }

    /// The bias is measured against the EXECUTABLE mapping of the image.
    ///
    /// A binary's read-only and data segments are mapped from the same path, so
    /// a reader that takes the first line naming the file gets a bias that is
    /// wrong by a segment — and a wrong bias does not fail, it names every frame
    /// after a plausible neighbour.
    #[test]
    fn the_bias_comes_from_the_executable_mapping_not_the_first_one() {
        let maps = "\
aaaa00000000-aaaa00001000 r--p 00000000 00:01 42 /app/hot
aaaa00001000-aaaa00002000 r-xp 00001000 00:01 42 /app/hot
aaaa00002000-aaaa00003000 rw-p 00002000 00:01 42 /app/hot
ffff00000000-ffff00001000 rw-p 00000000 00:00 0 [stack]
";
        // The text segment asks for 0x1000 and was placed at 0xaaaa00001000 with
        // a file offset of 0x1000, so everything shifted by 0xaaaa00000000.
        assert_eq!(load_bias(maps, "/app/hot", 0x0), Some(0xaaaa_0000_0000));
        assert_eq!(load_bias(maps, "/app/other", 0x0), None);
    }

    /// A non-PIE image asks for a fixed address, so its bias is zero — the same
    /// arithmetic, not a special case.
    #[test]
    fn a_fixed_address_image_has_no_bias() {
        let maps = "400000-401000 r-xp 00000000 00:01 7 /app/fixed\n";
        assert_eq!(load_bias(maps, "/app/fixed", 0x400000), Some(0));
    }

    #[test]
    fn names_the_function_containing_an_address_and_nothing_past_it() {
        let symbols = vec![
            FuncSymbol { value: 0x1000, size: 0x20, name: "first".into() },
            FuncSymbol { value: 0x2000, size: 0x10, name: "second".into() },
        ];
        let bias = 0x1_0000_0000;
        assert_eq!(symbolize(&symbols, bias, bias + 0x1000), Some("first"));
        assert_eq!(symbolize(&symbols, bias, bias + 0x101f), Some("first"));
        // One past the end of `first` is not `first`, and `second` has not
        // started: naming it either way would be a guess.
        assert_eq!(symbolize(&symbols, bias, bias + 0x1020), None);
        assert_eq!(symbolize(&symbols, bias, bias + 0x2008), Some("second"));
        // Below every symbol, and below the bias itself.
        assert_eq!(symbolize(&symbols, bias, bias + 0x10), None);
        assert_eq!(symbolize(&symbols, bias, 0x10), None);
    }

    /// An unsized symbol answers up to the next one. Hand-written runtime
    /// helpers are emitted without a size, and refusing them would leave exactly
    /// the frames a reader needs to recognise unnamed.
    #[test]
    fn an_unsized_symbol_answers_until_the_next_one() {
        let symbols = vec![
            FuncSymbol { value: 0x1000, size: 0, name: "__rt_helper".into() },
            FuncSymbol { value: 0x2000, size: 0x10, name: "next".into() },
        ];
        assert_eq!(symbolize(&symbols, 0, 0x1000), Some("__rt_helper"));
        assert_eq!(symbolize(&symbols, 0, 0x1fff), Some("__rt_helper"));
        assert_eq!(symbolize(&symbols, 0, 0x2000), Some("next"));
    }
}
