//! Purpose:
//! Assembles a whole user listing with the in-tree assembler, cutting it into objects itself,
//! instead of having `asm_split` analyze every line of it first.
//!
//! Called from:
//! - `crate::linker::assemble_parallel()` (macOS AArch64 with the in-tree assembler).
//!
//! Key details:
//! - `asm_split` reads the whole listing up front -- every label definition, every reference,
//!   every conditional branch -- to know where it may cut and which labels a cut forces to become
//!   symbols. On the Symfony demo that analysis was about 107 G instructions per build, beside an
//!   assembler that reads every line again anyway. Here the assembler's own pass learns it: each
//!   slice is assembled on its own, and the slices are joined afterwards.
//! - Cuts are looked for only near N even byte offsets: a top-level non-temporary label at column
//!   0 that control cannot fall into (the previous meaningful line is `ret`, `b`, `br`, `brk`, data
//!   or a section change), with the `.align`/`.globl`/comment lines that introduce it moved into
//!   the next slice, as `asm_split` does.
//! - What the up-front analysis guaranteed is CHECKED instead: a conditional branch or numeric
//!   label that a cut separates leaves its slice with an unresolved short branch (refused), an
//!   `A - B` across a cut is refused, a label defined twice is refused, and every slice must have
//!   started in the section the previous one ended in. Any refusal hands the whole listing back to
//!   `asm_split` unchanged.
//! - Joining: a reference whose label another slice defines makes that label a symbol. An
//!   assembler temporary (`L...`) is renamed with `asm_split`'s promoted prefix in every slice and
//!   published `.private_extern`; any other label is published `.private_extern` under its name.
//!   The file-global visibility footer is applied to whichever slice defines each name.

use super::*;

/// Where a listing is cut, decided without reading it whole.
pub(crate) struct ListingPlan<'a> {
    /// The listing without its visibility footer.
    body: &'a str,
    /// Names the visibility footer marks `.private_extern`.
    footer_names: Vec<&'a str>,
    /// Byte offset of each slice's first line in `body`; the first is 0.
    starts: Vec<usize>,
    /// The section in effect where each slice starts, as found by the plan; checked against the
    /// section the previous slice really ended in.
    start_sections: Vec<Sect>,
}

impl ListingPlan<'_> {
    /// Number of slices.
    pub(crate) fn slices(&self) -> usize {
        self.starts.len()
    }

    fn range(&self, slice: usize) -> std::ops::Range<usize> {
        let end = self.starts.get(slice + 1).copied().unwrap_or(self.body.len());
        self.starts[slice]..end
    }
}

/// Cuts `source` into at most `slices` pieces at legal boundaries near even byte offsets, or
/// `None` when it should not be split (too small to need it, or no legal cut found).
pub(crate) fn plan_listing(source: &str, slices: usize) -> Option<ListingPlan<'_>> {
    let marker = super::super::asm_split::VISIBILITY_FOOTER_MARKER;
    let (body, footer) = match memchr::memmem::rfind(source.as_bytes(), marker.as_bytes()) {
        // The marker opens its own line.
        Some(at) => {
            let line_start = source[..at].rfind('\n').map_or(0, |newline| newline + 1);
            (&source[..line_start], &source[line_start..])
        }
        None => (source, ""),
    };
    let footer_names = footer
        .lines()
        .filter_map(|line| {
            let line = line.trim_ascii();
            line.strip_prefix(".private_extern ")
                .or_else(|| line.strip_prefix(".hidden "))
                .map(str::trim_ascii)
        })
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    let sections = section_changes(body);
    let section_at = |offset: usize| match sections.partition_point(|(at, _)| *at < offset) {
        0 => Sect::Text,
        position => sections[position - 1].1,
    };
    let mut starts = vec![0usize];
    if slices > 1 {
        for part in 1..slices {
            let target = body.len() / slices * part;
            let floor = *starts.last().expect("starts holds the first slice");
            if let Some(cut) = cut_near(body, target, &section_at).filter(|&cut| cut > floor) {
                starts.push(cut);
            }
        }
    }
    if starts.len() < 2 {
        return None;
    }
    let start_sections = starts.iter().map(|&start| section_at(start)).collect();
    Some(ListingPlan { body, footer_names, starts, start_sections })
}

/// Every column-0 `.text`/`.data` line of `body`, as (byte offset, section), ascending.
///
/// Only these two sections are modelled by the in-tree assembler, which refuses anything else;
/// an indented section change it does honour is caught by the end-of-slice section check.
fn section_changes(body: &str) -> Vec<(usize, Sect)> {
    let bytes = body.as_bytes();
    let mut changes = Vec::new();
    for (pattern, sect) in [(&b"\n.text"[..], Sect::Text), (&b"\n.data"[..], Sect::Data)] {
        for at in memchr::memmem::find_iter(bytes, pattern) {
            let after = at + pattern.len();
            if matches!(bytes.get(after), None | Some(b'\n' | b'\r' | b' ' | b'\t')) {
                changes.push((at + 1, sect));
            }
        }
        if bytes.starts_with(&pattern[1..]) && matches!(bytes.get(pattern.len() - 1), None | Some(b'\n')) {
            changes.push((0, sect));
        }
    }
    changes.sort_unstable_by_key(|(at, _)| *at);
    changes
}

/// How far on either side of its target a cut is looked for before that slice is given up. A
/// slice is then at most twice this longer than an even share, which the caller budgets for.
pub(crate) const CUT_SEARCH_BYTES: usize = 16 << 20;

/// The nearest legal cut to `target`, looking forward first and then back: the start of the
/// lines introducing a top-level non-temporary label in `.text` that control cannot fall into.
fn cut_near(body: &str, target: usize, section_at: &impl Fn(usize) -> Sect) -> Option<usize> {
    let bytes = body.as_bytes();
    let target = target.min(bytes.len());
    let legal = |position: usize, line: &str| {
        opens_cuttable_label(line)
            && section_at(position) == Sect::Text
            && cannot_fall_into(body, position)
            && !short_reference_straddles(body, introduction_start(body, position))
    };
    let limit = target.saturating_add(CUT_SEARCH_BYTES).min(bytes.len());
    // Forward, from the first line beginning at or after `target`.
    let mut position = match target {
        0 => 0,
        _ => memchr::memchr(b'\n', &bytes[target..]).map_or(bytes.len(), |at| target + at + 1),
    };
    while position < limit {
        let end = memchr::memchr(b'\n', &bytes[position..]).map_or(bytes.len(), |at| position + at);
        if legal(position, &body[position..end]) {
            return Some(introduction_start(body, position));
        }
        position = end + 1;
    }
    // Back, from the line `target` falls in.
    let floor = target.saturating_sub(CUT_SEARCH_BYTES);
    let line_start = body[..target].rfind('\n').map_or(0, |newline| newline + 1);
    for (start, line) in std::iter::once((line_start, &body[line_start..memchr::memchr(b'\n', &bytes[line_start..]).map_or(bytes.len(), |at| line_start + at)]))
        .chain(lines_before(body, line_start))
    {
        if start < floor || start == 0 {
            break;
        }
        if legal(start, line) {
            return Some(introduction_start(body, start));
        }
    }
    None
}

/// How far apart, in source bytes, a short-range branch and its target can be once
/// `codegen::aarch64_relax` has run: it rewrites every conditional branch farther than 768 KB of
/// source into an inverse branch over an unconditional one. A margin above that.
const SHORT_REACH_BYTES: usize = 1 << 20;

/// Whether a cut at `cut` would separate a short-range reference from its target: a conditional
/// branch, `cbz`/`cbnz`, `tbz`/`tbnz` or `adr` on one side naming a label defined on the other,
/// or a numeric `1f`/`1b` whose nearest definition lies across the cut.
///
/// Only the window within [`SHORT_REACH_BYTES`] of the cut is read: a pair can be no farther
/// apart. These references have no Mach-O relocation, so the pair must stay in one object; a
/// shared helper reached by `b.eq` from many functions is the case that makes it more than
/// "within one function". What the window misses (an unrelaxed numeric branch farther away) is
/// still refused when its slice is assembled.
fn short_reference_straddles(body: &str, cut: usize) -> bool {
    let start = cut.saturating_sub(SHORT_REACH_BYTES);
    let start = if start == 0 { 0 } else { body[start..].find('\n').map_or(body.len(), |at| start + at + 1) };
    let end = cut.saturating_add(SHORT_REACH_BYTES).min(body.len());
    let mut defined_before: HashSet<&str> = HashSet::default();
    let mut defined_after: HashSet<&str> = HashSet::default();
    // (target, referenced before the cut)
    let mut references: Vec<(&str, bool)> = Vec::new();
    // Numeric labels: (digits, position, is a definition, direction for a reference).
    let mut numeric: Vec<(&str, usize, Option<u8>)> = Vec::new();
    let mut position = start;
    while position < end {
        let line_end = body[position..].find('\n').map_or(body.len(), |at| position + at);
        let line = body[position..line_end].trim_ascii();
        let before = position < cut;
        if let Some(name) = line.strip_suffix(':') {
            if !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit()) {
                numeric.push((name, position, None));
            } else if before {
                defined_before.insert(name);
            } else {
                defined_after.insert(name);
            }
        } else if let Some(target) = short_branch_target(line) {
            if target.len() > 1
                && target[..target.len() - 1].bytes().all(|byte| byte.is_ascii_digit())
                && matches!(target.as_bytes()[target.len() - 1], b'f' | b'b')
            {
                numeric.push((&target[..target.len() - 1], position, Some(target.as_bytes()[target.len() - 1])));
            } else {
                references.push((target, before));
            }
        } else if let Some(target) = unconditional_numeric_target(line) {
            numeric.push((&target[..target.len() - 1], position, Some(target.as_bytes()[target.len() - 1])));
        }
        position = line_end + 1;
    }
    if references.iter().any(|(target, before)| {
        if *before { defined_after.contains(target) } else { defined_before.contains(target) }
    }) {
        return true;
    }
    // A numeric reference resolves to its nearest definition in its direction.
    numeric.iter().any(|&(digits, at, direction)| match direction {
        Some(b'f') if at < cut => numeric
            .iter()
            .filter(|(name, def_at, kind)| kind.is_none() && *name == digits && *def_at > at)
            .map(|(_, def_at, _)| *def_at)
            .min()
            .is_some_and(|target| target >= cut),
        Some(b'b') if at >= cut => numeric
            .iter()
            .filter(|(name, def_at, kind)| kind.is_none() && *name == digits && *def_at < at)
            .map(|(_, def_at, _)| *def_at)
            .max()
            .is_some_and(|target| target < cut),
        _ => false,
    })
}

/// The label a short-range branch or `adr` names, if the trimmed line is one.
fn short_branch_target(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    if !(bytes.starts_with(b"b.") || bytes.starts_with(b"cb") || bytes.starts_with(b"tb") || bytes.starts_with(b"adr ")) {
        return None;
    }
    let (mnemonic, operands) = line.split_once(|c: char| c.is_ascii_whitespace())?;
    if !(mnemonic.starts_with("b.") || matches!(mnemonic, "cbz" | "cbnz" | "tbz" | "tbnz" | "adr")) {
        return None;
    }
    let target = operands.rsplit(',').next()?.trim_ascii();
    let target = target.split(|c: char| c == ';' || c.is_ascii_whitespace()).next()?;
    (!target.is_empty()).then_some(target)
}

/// The numeric label (`1f`, `2b`) an unconditional `b`/`bl` names, if the trimmed line is one.
fn unconditional_numeric_target(line: &str) -> Option<&str> {
    let (mnemonic, operand) = line.split_once(|c: char| c.is_ascii_whitespace())?;
    if !matches!(mnemonic, "b" | "bl") {
        return None;
    }
    let target = operand.trim_ascii().split(|c: char| c == ';' || c.is_ascii_whitespace()).next()?;
    let digits = target.strip_suffix(['f', 'b'])?;
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())).then_some(target)
}

/// Whether a raw line is a column-0 label a slice may start with: a plain name, not an assembler
/// temporary (`L...`) and not a numeric local.
fn opens_cuttable_label(line: &str) -> bool {
    let bytes = line.as_bytes();
    if !matches!(bytes.first(), Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_') || bytes[0] == b'L' {
        return false;
    }
    let Some(name) = line.trim_ascii_end().strip_suffix(':') else {
        return false;
    };
    !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'.'))
}

/// The lines `asm_split` moves into the next slice with the label they introduce.
fn introduces_a_label(line: &str) -> bool {
    line.is_empty()
        || line.starts_with(".align")
        || line.starts_with(".p2align")
        || line.starts_with(".globl")
        || line.starts_with(".private_extern")
        || line.starts_with(';')
        || line.starts_with("//")
        || line.starts_with('#')
}

/// The lines before the one at `position`, nearest first.
fn lines_before(body: &str, position: usize) -> impl Iterator<Item = (usize, &str)> {
    let mut end = position;
    std::iter::from_fn(move || {
        if end == 0 {
            return None;
        }
        let line_end = end - 1;
        let start = body[..line_end].rfind('\n').map_or(0, |newline| newline + 1);
        end = start;
        Some((start, &body[start..line_end]))
    })
}

/// Whether control cannot run into the line at `position` from the line before it, as
/// `asm_split::cannot_fall_through` decides.
fn cannot_fall_into(body: &str, position: usize) -> bool {
    for (_, line) in lines_before(body, position) {
        let line = line.trim_ascii();
        if introduces_a_label(line) {
            continue;
        }
        let mnemonic = line.split_ascii_whitespace().next().unwrap_or("");
        return matches!(mnemonic, "ret" | "b" | "br" | "brk")
            || line.starts_with(".quad")
            || line.starts_with(".long")
            || line.starts_with(".short")
            || line.starts_with(".byte")
            || line.starts_with(".ascii")
            || line.starts_with(".space")
            || line.starts_with(".zero")
            || line.starts_with(".comm")
            || matches!(line, ".text" | ".data" | ".bss" | ".const" | ".cstring")
            || line.starts_with(".section");
    }
    true
}

/// The start of the run of introducing lines right before the label at `position`.
fn introduction_start(body: &str, position: usize) -> usize {
    let mut start = position;
    for (line_start, line) in lines_before(body, position) {
        if !introduces_a_label(line.trim_ascii()) {
            break;
        }
        start = line_start;
    }
    start
}

/// One slice, assembled but not yet written.
pub(crate) struct ListingSlice<'a>(Assembled<'a>);

/// Assembles slice `slice` of `plan` on its own.
pub(crate) fn assemble_slice<'a>(plan: &ListingPlan<'a>, slice: usize) -> Result<ListingSlice<'a>> {
    let range = plan.range(slice);
    let body = &plan.body[range];
    let mut prologue = String::new();
    if slice > 0 {
        // As `asm_split::slice_prologue`: restate the section (HAZARD 4) and its alignment unless
        // the slice opens with its own (HAZARD 3).
        let sect = plan.start_sections[slice];
        prologue.push_str(match sect {
            Sect::Text => ".text\n",
            Sect::Data => ".data\n",
        });
        let opening = body.lines().map(str::trim_ascii).find(|line| !line.is_empty()).unwrap_or("");
        if !opening.starts_with(".align") && !opening.starts_with(".p2align") {
            prologue.push_str(match sect {
                Sect::Text => ".p2align 2\n",
                Sect::Data => ".p2align 3\n",
            });
        }
    }
    let parts: [&'a str; 2] = [leak_str(prologue), body];
    assemble_one_pass(&parts, Names::IDENTITY).map(ListingSlice)
}

/// A string kept for the life of the process: a slice's prologue and a promoted name live in
/// layouts that borrow the listing for its own lifetime. A build makes a few dozen of them.
fn leak_str(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// What joining the slices had to do, for the `--timings` note.
pub(crate) struct JoinSummary {
    pub(crate) promoted: usize,
    pub(crate) published: usize,
}

/// Writes one joined slice's object.
pub(crate) fn write_slice(slice: ListingSlice<'_>) -> Result<Vec<u8>> {
    let ListingSlice(assembled) = slice;
    write_object(&assembled.layout, assembled.sections, assembled.relocs)
}

/// Joins independently assembled slices: checks they agree, makes every cross-slice reference
/// a symbol and applies the visibility footer. Each joined slice is then written on its own
/// (`write_slice`).
pub(crate) fn join_slices<'a>(
    plan: &ListingPlan<'a>,
    mut slices: Vec<ListingSlice<'a>>,
) -> Result<(Vec<ListingSlice<'a>>, JoinSummary)> {
    for slice in 1..slices.len() {
        if slices[slice - 1].0.final_sect != plan.start_sections[slice] {
            return Err(Unsupported(format!("slice {slice} starts in another section than the previous ended in")));
        }
    }
    let mut definer: HashMap<&'a str, usize> = HashMap::default();
    for (index, slice) in slices.iter().enumerate() {
        for name in slice.0.layout.labels.keys() {
            if definer.insert(name, index).is_some() {
                return Err(Unsupported(format!("label {name} defined in two slices")));
            }
        }
        for (name, _, _) in &slice.0.layout.commons {
            definer.entry(name).or_insert(index);
        }
    }
    for name in &plan.footer_names {
        if let Some(&index) = definer.get(name) {
            slices[index].0.layout.exported.insert(name, true);
        }
    }
    let mut promoted: HashMap<&'a str, &'a str> = HashMap::default();
    let mut published = 0usize;
    for index in 0..slices.len() {
        for reloc_index in 0..slices[index].0.relocs.len() {
            let target = slices[index].0.relocs[reloc_index].target;
            if slices[index].0.layout.labels.contains_key(target) || promoted.contains_key(target) {
                continue;
            }
            let Some(&owner) = definer.get(target) else {
                continue;
            };
            if owner == index {
                continue;
            }
            if target.as_bytes().first() == Some(&b'L') {
                let renamed = leak_str(format!("{}{target}", super::super::asm_split::PROMOTED_PREFIX));
                if definer.contains_key(renamed) {
                    return Err(Unsupported(format!("promoted name {renamed} already defined")));
                }
                promoted.insert(target, renamed);
            } else if !slices[owner].0.layout.exported.contains_key(target) {
                slices[owner].0.layout.exported.insert(target, true);
                published += 1;
            }
        }
    }
    for (&original, &renamed) in &promoted {
        let owner = definer[original];
        let layout = &mut slices[owner].0.layout;
        let place = layout.labels.remove(original).expect("the owner defines the promoted label");
        layout.labels.insert(renamed, place);
        layout.exported.insert(renamed, true);
    }
    if !promoted.is_empty() {
        for slice in &mut slices {
            for reloc in &mut slice.0.relocs {
                if let Some(&renamed) = promoted.get(reloc.target) {
                    reloc.target = renamed;
                }
            }
        }
    }
    Ok((slices, JoinSummary { promoted: promoted.len(), published }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A listing of `functions` functions calling each other and sharing a temporary, with data
    /// and a visibility footer: big enough for several cuts.
    fn listing(functions: usize) -> String {
        let mut text = String::new();
        for index in 0..functions {
            text.push_str(&format!(".p2align 2\n.globl _f{index}\n_f{index}:\n"));
            text.push_str("    mov x0, #1\n    cbz x0, Ldone_");
            text.push_str(&format!("{index}\n"));
            text.push_str(&format!("    bl _f{}\n", (index + 1) % functions));
            text.push_str("    adrp x9, Lshared@PAGE\n    add x9, x9, Lshared@PAGEOFF\n");
            text.push_str(&format!("    b _local{}\n", (index + 3) % functions));
            text.push_str(&format!("Ldone_{index}:\n    ret\n"));
            text.push_str(&format!("_local{index}:\n    ret\n"));
        }
        text.push_str(".data\n.p2align 3\nLshared:\n    .quad _f0\n    .quad 7\n");
        text.push_str(&format!("{}\n.private_extern _f1\n", super::super::super::asm_split::VISIBILITY_FOOTER_MARKER));
        text
    }

    /// The plan cuts only before a top-level label nothing falls into, keeping its introducing
    /// lines with it, and never inside `.data`.
    #[test]
    fn cuts_open_labels_control_cannot_fall_into() {
        let text = listing(40);
        let plan = plan_listing(&text, 4).expect("a listing this size is cut");
        assert!(plan.slices() > 1);
        for (slice, &start) in plan.starts.iter().enumerate().skip(1) {
            let rest = &plan.body[start..];
            // The introducing lines come with the label, which is a plain top-level one.
            let label = rest.lines().find(|line| !introduces_a_label(line.trim_ascii())).unwrap();
            assert!(opens_cuttable_label(label), "cut at {:?}", &rest[..40.min(rest.len())]);
            assert!(cannot_fall_into(plan.body, start), "control falls into the cut at {start}");
            assert_eq!(plan.start_sections[slice], Sect::Text);
        }
        assert_eq!(plan.footer_names, vec!["_f1"]);
    }

    /// Slices assembled on their own and joined: a temporary referenced across a cut is promoted
    /// in every slice, a plain label is published, the footer is applied, and the whole thing
    /// produces one object per slice.
    #[test]
    fn joined_slices_resolve_cross_slice_references() {
        let text = listing(40);
        let plan = plan_listing(&text, 4).expect("cut");
        let slices = (0..plan.slices()).map(|slice| assemble_slice(&plan, slice)).collect::<Result<Vec<_>>>().expect("assembles");
        let (joined, summary) = join_slices(&plan, slices).expect("joins");
        let objects = joined.into_iter().map(write_slice).collect::<Result<Vec<_>>>().expect("writes");
        assert_eq!(objects.len(), plan.slices());
        assert!(summary.promoted >= 1, "Lshared lives in the last slice and is referenced from all");
    }

    /// A program cut into objects here, linked and run, behaves as the same listing assembled
    /// whole by `as`: calls between slices, a branch to a temporary another slice defines
    /// (promoted), data in the last slice pointing at code in the first, a function pointer
    /// called through it, and the visibility footer.
    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn a_cut_program_runs_like_the_whole_one() {
        let mut text = String::from(".p2align 2\nLhelper:\n    add x0, x0, #1\n    ret\n");
        for index in 0..60 {
            text.push_str(&format!(".p2align 2\n.globl _f{index}\n_f{index}:\n"));
            text.push_str("    stp x29, x30, [sp, #-16]!\n");
            text.push_str(&format!("    mov x0, #{index}\n    bl Lhelper\n"));
            text.push_str("    ldp x29, x30, [sp], #16\n    ret\n");
        }
        text.push_str(".p2align 2\n.globl _main\n_main:\n    stp x29, x30, [sp, #-32]!\n");
        text.push_str("    bl _f3\n    mov x19, x0\n    bl _f50\n    add x19, x19, x0\n");
        text.push_str("    adrp x9, Ltable@PAGE\n    add x9, x9, Ltable@PAGEOFF\n    ldr x10, [x9, #8]\n");
        text.push_str("    blr x10\n    add x0, x19, x0\n    ldp x29, x30, [sp], #32\n    ret\n");
        text.push_str(".data\n.p2align 3\nLtable:\n    .quad 0\n    .quad _f7\n");
        text.push_str(&format!("{}\n.private_extern _f1\n", super::super::super::asm_split::VISIBILITY_FOOTER_MARKER));

        let plan = plan_listing(&text, 4).expect("cut");
        assert!(plan.slices() > 2, "the program spans several objects");
        let slices = (0..plan.slices()).map(|slice| assemble_slice(&plan, slice)).collect::<Result<Vec<_>>>().expect("assembles");
        let (joined, summary) = join_slices(&plan, slices).expect("joins");
        let objects = joined.into_iter().map(write_slice).collect::<Result<Vec<_>>>().expect("writes");
        assert!(summary.promoted >= 1, "Lhelper is promoted");

        let dir = std::env::temp_dir().join(format!("elephc-listing-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut ours = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            let path = dir.join(format!("slice{index}.o"));
            std::fs::write(&path, object).unwrap();
            ours.push(path);
        }
        let source = dir.join("whole.s");
        std::fs::write(&source, &text).unwrap();
        let theirs = dir.join("whole.o");
        let status = std::process::Command::new("as").args(["-arch", "arm64", "-o"]).arg(&theirs).arg(&source).status().unwrap();
        assert!(status.success());
        let mut codes = Vec::new();
        for (inputs, exe) in [(ours, dir.join("ours")), (vec![theirs], dir.join("theirs"))] {
            let link = std::process::Command::new("cc").args(&inputs).arg("-o").arg(&exe).output().unwrap();
            assert!(link.status.success(), "link: {}", String::from_utf8_lossy(&link.stderr));
            codes.push(std::process::Command::new(&exe).status().unwrap().code());
        }
        let _ = std::fs::remove_dir_all(&dir);
        // f3 + f50 + f7, each `index + 1`.
        assert_eq!(codes[0], Some(4 + 51 + 8));
        assert_eq!(codes[0], codes[1]);
    }

    /// No cut separates a short-range branch from its target -- a temporary, or a shared global
    /// helper several functions reach with `b.eq` -- so every slice the plan makes assembles.
    #[test]
    fn cuts_keep_short_branches_with_their_targets() {
        let mut text = String::new();
        for index in 0..300 {
            text.push_str(&format!(".p2align 2\n.globl _f{index}\n_f{index}:\n    cmp x0, #1\n    b.eq _shared\n"));
            if index % 50 == 0 {
                text.push_str("    cbz x0, Ltail\n");
            }
            text.push_str("    ret\n");
            if index == 150 {
                text.push_str(".p2align 2\n.globl _shared\n_shared:\n    mov x0, #0\n    ret\n");
            }
        }
        text.push_str("Ltail:\n    ret\n");
        if let Some(plan) = plan_listing(&text, 6) {
            for &start in &plan.starts[1..] {
                assert!(!short_reference_straddles(plan.body, start), "a cut at {start} splits a pair");
            }
            for slice in 0..plan.slices() {
                assert!(assemble_slice(&plan, slice).is_ok(), "slice {slice} has a branch it cannot reach");
            }
        }
        // The window check itself, on a cut that would split the helper from its callers.
        let helper = text.find(".p2align 2\n.globl _shared").unwrap();
        assert!(short_reference_straddles(&text, helper));
    }

    /// A slice that still holds a short branch to a label it does not define is refused when
    /// assembled, so the listing goes back to the analysed split.
    #[test]
    fn a_short_branch_out_of_its_slice_is_refused() {
        let body = ".p2align 2\n_a:\n    cbz x0, Lfar\n    ret\n.p2align 2\n_b:\nLfar:\n    ret\n";
        let cut = body.find(".p2align 2\n_b:").unwrap();
        let plan = ListingPlan {
            body,
            footer_names: Vec::new(),
            starts: vec![0, cut],
            start_sections: vec![Sect::Text, Sect::Text],
        };
        assert!(assemble_slice(&plan, 0).is_err(), "the short branch cannot leave its slice");
    }
}
