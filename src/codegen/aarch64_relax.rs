//! Purpose:
//! Relaxes only AArch64 conditional branches whose generated targets exceed the
//! architecture's short branch encodings.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` after user text and metadata are complete.
//!
//! Key details:
//! - Symbolic source distance is a conservative proxy for instruction distance;
//!   nearby branches remain compact, while far branches use an inverse-condition island.

use crate::fast_hash::FastMap as HashMap;
use std::fmt::Write;

use crate::codegen::platform::{Arch, Platform, Target};

const CONDITIONAL_SOURCE_DISTANCE: usize = 768 * 1024;
const TEST_BIT_SOURCE_DISTANCE: usize = 24 * 1024;
const WIDE_BRANCH_SOURCE_DISTANCE: usize = 96 * 1024 * 1024;

/// Parsed symbolic AArch64 conditional branch.
struct ConditionalBranch<'a> {
    inverse_mnemonic: &'static str,
    leading_operands: &'a str,
    target: &'a str,
    source_limit: usize,
}

/// Rewrites only symbolic AArch64 conditional branches whose targets are conservatively far.
#[cfg(test)]
pub(super) fn relax_conditional_branches(assembly: String, target: Target) -> String {
    let edits = conditional_branch_relaxations(&[&assembly], target);
    super::user_assembly::UserAssembly::new(None, assembly, edits, String::new()).into_string()
}

/// Returns the out-of-range conditional branches of the concatenation of `pieces` as
/// replacement edits, with offsets into that concatenation. Each piece ends on a line boundary.
///
/// Edits instead of a rewritten copy: the text is the whole user program (1.5 GB on the Symfony
/// build), and a copy made while the original is still live doubles the code generator's peak.
/// `UserAssembly` applies the edits while streaming the text to disk.
pub(super) fn conditional_branch_relaxations(
    pieces: &[&str],
    target: Target,
) -> Vec<super::user_assembly::TextEdit> {
    if target.arch != Arch::AArch64 {
        return Vec::new();
    }

    // The house hasher, not SipHash: this map takes one insert per label in a 1.4 GB listing
    // and is only ever probed by `get`, never iterated, so nothing depends on its order. A
    // sample of the late codegen window caught `sip::Hasher::write` here.
    // Each label keeps both its source offset (the conservative proxy conditional branches use)
    // and its instruction index (what unconditional `b`/`bl` reach is measured in).
    let mut labels = HashMap::default();
    // Sized up front: 2.6 million labels in a 1 GB listing (about one per 400 bytes) otherwise
    // rehash the table a dozen times as it grows.
    labels.reserve(pieces.iter().map(|piece| piece.len()).sum::<usize>() / 400);
    let mut source_offset = 0usize;
    let mut instruction_index = 0usize;
    for line in pieces.iter().flat_map(|piece| lines_of(piece)) {
        match classify(line) {
            Line::Label(label) => {
                if is_symbol(label) {
                    labels.insert(label, (source_offset, instruction_index));
                }
            }
            Line::Instruction(_) => instruction_index += 1,
            Line::Other => {}
        }
        source_offset += line.len();
    }

    // Only the relaxed branches differ from the text; each becomes one edit covering its line.
    let mut edits = Vec::new();
    source_offset = 0;
    instruction_index = 0;
    for line in pieces.iter().flat_map(|piece| lines_of(piece)) {
        let Line::Instruction(trimmed) = classify(line) else {
            source_offset += line.len();
            continue;
        };
        if let Some(far) = far_unconditional_branch(trimmed, &labels, instruction_index) {
            let mut replacement = String::new();
            emit_far_unconditional_branch(&mut replacement, &far, target);
            edits.push(super::user_assembly::TextEdit {
                start: source_offset,
                end: source_offset + line.len(),
                replacement,
            });
            source_offset += line.len();
            instruction_index += 1;
            continue;
        }
        let expansion = maybe_conditional_branch(trimmed)
            .then(|| parse_conditional_branch(trimmed))
            .flatten()
            .and_then(|branch| {
                let (target_offset, _) = labels.get(branch.target).copied()?;
                let distance = source_offset.abs_diff(target_offset);
                (distance > branch.source_limit).then_some((branch, distance))
            });

        if let Some((branch, distance)) = expansion {
            let mut replacement = String::new();
            emit_relaxed_branch(&mut replacement, &branch, target, source_offset, distance);
            edits.push(super::user_assembly::TextEdit {
                start: source_offset,
                end: source_offset + line.len(),
                replacement,
            });
        }
        source_offset += line.len();
        instruction_index += 1;
    }
    edits
}

/// One assembly line, classified without trimming lines that need no trimmed form.
enum Line<'a> {
    /// `name:` -- the label name.
    Label(&'a str),
    /// A line that assembles to code, trimmed.
    Instruction(&'a str),
    /// Blank, comment or directive.
    Other,
}

/// Iterates the lines of `text`, each with its trailing `\n`, splitting on `memchr`.
///
/// `str::split_inclusive('\n')` runs a char searcher; on the 2.7 GB Symfony demo listing it and
/// the per-line `trim` were 3.3 s of the 4.8 s these two passes took.
fn lines_of(text: &str) -> impl Iterator<Item = &str> {
    let bytes = text.as_bytes();
    let mut start = 0usize;
    std::iter::from_fn(move || {
        if start >= bytes.len() {
            return None;
        }
        let end = memchr::memchr(b'\n', &bytes[start..]).map_or(bytes.len(), |at| start + at + 1);
        let line = &text[start..end];
        start = end;
        Some(line)
    })
}

/// Classifies one line the way the two passes need it.
fn classify(line: &str) -> Line<'_> {
    let bytes = line.as_bytes();
    let Some(first) = bytes.iter().position(|byte| !byte.is_ascii_whitespace()) else {
        return Line::Other;
    };
    let last = bytes.iter().rposition(|byte| !byte.is_ascii_whitespace()).unwrap_or(first);
    if bytes[last] == b':' {
        return Line::Label(&line[first..last]);
    }
    match bytes[first] {
        b';' | b'.' | b'#' => Line::Other,
        b'/' if bytes.get(first + 1) == Some(&b'/') => Line::Other,
        _ => Line::Instruction(&line[first..=last]),
    }
}

/// Unconditional `b`/`bl` reach +/-128 MB. A program whose text is larger than that relies on
/// the linker's branch islands, and Apple's `ld` stops placing them ("branch islands: maximum
/// cluster count reached") once the text passes roughly 2 GB: the Symfony demo failed to link
/// with a `bl` to a closure 224 MB away. Branches to a label of the same user text that sits
/// more than this many bytes of CODE away are rewritten to an address-materializing jump, which
/// reaches +/-4 GB. Code distance is measured in instruction lines, a slight UNDER-estimate of
/// the real distance (a directive or literal inside text adds bytes), so the margin to 128 MB
/// stays generous.
const FAR_UNCONDITIONAL_CODE_DISTANCE: usize = 100 * 1024 * 1024;

/// An unconditional branch whose target is too far for its 26-bit immediate.
struct FarUnconditionalBranch<'a> {
    /// `bl` keeps the return address (`blr`), `b` does not (`br`).
    links: bool,
    target: &'a str,
}

/// Returns the unconditional branch on `trimmed` when its user-text target is out of reach.
///
/// Targets outside the user text (runtime helpers, libc stubs) are not in `labels` and stay as
/// they are: the linker still islands those, and a libc symbol would need a GOT load instead.
fn far_unconditional_branch<'a>(
    trimmed: &'a str,
    labels: &HashMap<&str, (usize, usize)>,
    instruction_index: usize,
) -> Option<FarUnconditionalBranch<'a>> {
    // Byte tests first: this runs for every instruction of the listing, and only `b ` / `bl `
    // can match, so the char-searching split below is for the few lines that can.
    let bytes = trimmed.as_bytes();
    let starts_branch = bytes.first() == Some(&b'b')
        && match bytes.get(1) {
            Some(b' ' | b'\t') => true,
            Some(b'l') => matches!(bytes.get(2), Some(b' ' | b'\t')),
            _ => false,
        };
    if !starts_branch {
        return None;
    }
    let (mnemonic, operand) = split_mnemonic(trimmed)?;
    let links = match mnemonic {
        "b" => false,
        "bl" => true,
        _ => return None,
    };
    let target = operand.split(';').next()?.trim();
    if !is_symbol(target) {
        return None;
    }
    let (_, target_index) = labels.get(target).copied()?;
    let distance = instruction_index.abs_diff(target_index).saturating_mul(4);
    (distance > FAR_UNCONDITIONAL_CODE_DISTANCE).then_some(FarUnconditionalBranch { links, target })
}

/// Writes the +/-4 GB form of one far unconditional branch through the intra-procedure-call
/// scratch register `x16`, which the AAPCS64 leaves to exactly this kind of veneer.
fn emit_far_unconditional_branch(output: &mut String, branch: &FarUnconditionalBranch<'_>, target: Target) {
    match target.platform {
        Platform::MacOS => {
            let _ = writeln!(output, "    adrp x16, {}@PAGE", branch.target);
            let _ = writeln!(output, "    add x16, x16, {}@PAGEOFF", branch.target);
        }
        Platform::Linux => {
            let _ = writeln!(output, "    adrp x16, {}", branch.target);
            let _ = writeln!(output, "    add x16, x16, :lo12:{}", branch.target);
        }
        Platform::Windows => unreachable!("AArch64 Windows codegen is unsupported"),
    }
    let _ = writeln!(output, "    {} x16", if branch.links { "blr" } else { "br" });
}

/// Returns whether a line could possibly be one of the branches the parser accepts.
///
/// Every mnemonic `parse_conditional_branch` handles -- `b.<cond>`, `cbz`, `cbnz`, `tbz`,
/// `tbnz` -- starts with one of three letters, so anything else cannot parse and does not need
/// the trim, the whitespace split and the mnemonic match the parser would spend on it. That
/// work ran once per line of a 42-million-line listing.
fn maybe_conditional_branch(line: &str) -> bool {
    matches!(
        line.as_bytes().iter().find(|byte| !byte.is_ascii_whitespace()),
        Some(b'b' | b'c' | b't')
    )
}

/// Parses the condition, preserved operands, and symbolic target from one assembly line.
fn parse_conditional_branch(line: &str) -> Option<ConditionalBranch<'_>> {
    let trimmed = line.trim();
    let (mnemonic, operands) = split_mnemonic(trimmed)?;
    let operands = operands.trim();

    let branch = if let Some(condition) = mnemonic.strip_prefix("b.") {
        let inverse_mnemonic = match condition {
            "eq" => "b.ne",
            "ne" => "b.eq",
            "cs" => "b.cc",
            "cc" => "b.cs",
            "hs" => "b.lo",
            "lo" => "b.hs",
            "mi" => "b.pl",
            "pl" => "b.mi",
            "vs" => "b.vc",
            "vc" => "b.vs",
            "hi" => "b.ls",
            "ls" => "b.hi",
            "ge" => "b.lt",
            "lt" => "b.ge",
            "gt" => "b.le",
            "le" => "b.gt",
            _ => return None,
        };
        ConditionalBranch {
            inverse_mnemonic,
            leading_operands: "",
            target: operands,
            source_limit: CONDITIONAL_SOURCE_DISTANCE,
        }
    } else if matches!(mnemonic, "cbz" | "cbnz") {
        let (register, target) = operands.split_once(',')?;
        ConditionalBranch {
            inverse_mnemonic: if mnemonic == "cbz" { "cbnz" } else { "cbz" },
            leading_operands: register.trim(),
            target: target.trim(),
            source_limit: CONDITIONAL_SOURCE_DISTANCE,
        }
    } else if matches!(mnemonic, "tbz" | "tbnz") {
        let (register_and_bit, target) = operands.rsplit_once(',')?;
        ConditionalBranch {
            inverse_mnemonic: if mnemonic == "tbz" { "tbnz" } else { "tbz" },
            leading_operands: register_and_bit.trim(),
            target: target.trim(),
            source_limit: TEST_BIT_SOURCE_DISTANCE,
        }
    } else {
        return None;
    };

    is_symbol(branch.target).then_some(branch)
}

/// Emits an inverse-condition island and selects a register-free or absolute far jump.
fn emit_relaxed_branch(
    output: &mut String,
    branch: &ConditionalBranch<'_>,
    target: Target,
    branch_offset: usize,
    distance: usize,
) {
    let skip_label = match target.platform {
        Platform::MacOS => format!("L_elephc_relaxed_branch_{}", branch_offset),
        Platform::Linux | Platform::Windows => {
            format!(".L_elephc_relaxed_branch_{}", branch_offset)
        }
    };
    if branch.leading_operands.is_empty() {
        let _ = writeln!(output, "    {} {}", branch.inverse_mnemonic, skip_label);
    } else {
        let _ = writeln!(
            output,
            "    {} {}, {}",
            branch.inverse_mnemonic, branch.leading_operands, skip_label
        );
    }

    if distance <= WIDE_BRANCH_SOURCE_DISTANCE {
        let _ = writeln!(output, "    b {}", branch.target);
    } else {
        match target.platform {
            Platform::MacOS => {
                let _ = writeln!(output, "    adrp x16, {}@PAGE", branch.target);
                let _ = writeln!(output, "    add x16, x16, {}@PAGEOFF", branch.target);
            }
            Platform::Linux => {
                let _ = writeln!(output, "    adrp x16, {}", branch.target);
                let _ = writeln!(output, "    add x16, x16, :lo12:{}", branch.target);
            }
            Platform::Windows => unreachable!("AArch64 Windows codegen is unsupported"),
        }
        let _ = writeln!(output, "    br x16");
    }
    let _ = writeln!(output, "{}:", skip_label);
}

/// Splits an instruction at its first space or tab: the mnemonic and the rest.
///
/// A byte search, not `split_once(char::is_whitespace)`: the char searcher decodes every byte
/// of a line the relaxation scan visits once per branch of a 1 GB listing.
fn split_mnemonic(line: &str) -> Option<(&str, &str)> {
    let at = line.bytes().position(|byte| byte == b' ' || byte == b'\t')?;
    Some((&line[..at], &line[at + 1..]))
}

/// Returns whether a branch operand is a plain symbolic assembly label.
fn is_symbol(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|first| !first.is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies nearby conditional branches remain byte-for-byte compact.
    #[test]
    fn nearby_conditional_branch_is_unchanged() {
        let assembly = "    b.eq target\n    nop\ntarget:\n".to_string();
        let target = Target::new(Platform::MacOS, Arch::AArch64);
        assert_eq!(relax_conditional_branches(assembly.clone(), target), assembly);
    }

    /// Verifies a distant condition-code branch uses a register-free branch island.
    #[test]
    fn distant_condition_branch_is_relaxed() {
        let mut assembly = "    b.eq target\n".to_string();
        assembly.push_str(&"    nop\n".repeat(CONDITIONAL_SOURCE_DISTANCE / 8 + 2));
        assembly.push_str("target:\n");
        let target = Target::new(Platform::MacOS, Arch::AArch64);
        let relaxed = relax_conditional_branches(assembly, target);
        assert!(relaxed.starts_with(
            "    b.ne L_elephc_relaxed_branch_0\n    b target\nL_elephc_relaxed_branch_0:\n"
        ));
    }

    /// Verifies bit-test operands and Linux-local label syntax survive relaxation.
    #[test]
    fn distant_bit_test_branch_preserves_operands() {
        let mut assembly = "    tbnz x4, #63, target\n".to_string();
        assembly.push_str(&"    nop\n".repeat(TEST_BIT_SOURCE_DISTANCE / 8 + 2));
        assembly.push_str("target:\n");
        let target = Target::new(Platform::Linux, Arch::AArch64);
        let relaxed = relax_conditional_branches(assembly, target);
        assert!(relaxed.starts_with(
            "    tbz x4, #63, .L_elephc_relaxed_branch_0\n    b target\n.L_elephc_relaxed_branch_0:\n"
        ));
    }

    /// Verifies a `bl` to a user label more than 128 MB of code away becomes an `x16` call, and a
    /// `b` an `x16` jump, while a nearby call and a call into the runtime keep their short form.
    #[test]
    fn distant_unconditional_branches_use_x16() {
        let filler = FAR_UNCONDITIONAL_CODE_DISTANCE / 4 + 2;
        let mut assembly = "    bl far_target\n    b far_target\n    bl near_target\n    bl __rt_heap_alloc\nnear_target:\n".to_string();
        assembly.push_str(&"    nop\n".repeat(filler));
        assembly.push_str("far_target:\n    ret\n");
        let target = Target::new(Platform::MacOS, Arch::AArch64);
        let relaxed = relax_conditional_branches(assembly, target);
        assert!(relaxed.starts_with(
            "    adrp x16, far_target@PAGE\n    add x16, x16, far_target@PAGEOFF\n    blr x16\n\
             \x20   adrp x16, far_target@PAGE\n    add x16, x16, far_target@PAGEOFF\n    br x16\n\
             \x20   bl near_target\n    bl __rt_heap_alloc\nnear_target:\n"
        ), "{}", &relaxed[..300]);
    }

    /// The byte-level mnemonic split accepts a tab as well as a space, and the first-byte gate
    /// lets neither `bl` nor `b` slip past nor admits look-alikes such as `blr` or `bic`.
    #[test]
    fn mnemonic_split_and_branch_gate() {
        assert_eq!(split_mnemonic("bl\tfar"), Some(("bl", "far")));
        assert_eq!(split_mnemonic("b.eq target"), Some(("b.eq", "target")));
        assert_eq!(split_mnemonic("ret"), None);
        let labels: HashMap<&str, (usize, usize)> = [("far", (0, usize::MAX / 8))].into_iter().collect();
        assert!(far_unconditional_branch("bl\tfar", &labels, 0).is_some_and(|branch| branch.links));
        assert!(far_unconditional_branch("b far", &labels, 0).is_some_and(|branch| !branch.links));
        assert!(far_unconditional_branch("blr x16", &labels, 0).is_none());
        assert!(far_unconditional_branch("bic x0, x0, x1", &labels, 0).is_none());
    }

    /// Times the relaxation scan over a real listing: `ELEPHC_RELAX_BENCH=<file.s> cargo test
    /// --release --lib relaxation_bench -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn relaxation_bench() {
        let Ok(path) = std::env::var("ELEPHC_RELAX_BENCH") else {
            return;
        };
        let text = std::fs::read_to_string(path).expect("listing");
        let target = Target::new(Platform::MacOS, Arch::AArch64);
        for _ in 0..3 {
            let started = std::time::Instant::now();
            let edits = conditional_branch_relaxations(&[&text], target);
            eprintln!("{} edits in {:.2}s", edits.len(), started.elapsed().as_secs_f64());
        }
    }

    /// Verifies non-AArch64 assembly is returned unchanged.
    #[test]
    fn other_architecture_is_unchanged() {
        let assembly = "    b.eq target\ntarget:\n".to_string();
        let target = Target::new(Platform::Linux, Arch::X86_64);
        assert_eq!(relax_conditional_branches(assembly.clone(), target), assembly);
    }
}
