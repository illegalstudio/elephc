//! Purpose:
//! Cuts one finished user assembly file into N CONTIGUOUS slices that assemble
//! independently, so `as` can run N times in parallel instead of once on a
//! single multi-hundred-megabyte input.
//!
//! Called from:
//! - `crate::linker::assemble_parallel()`, on the exact bytes already written to
//!   `<stem>.s`. Never on the emitter's intermediate state.
//!
//! Key details:
//! - The split is a POST-PASS over the finished text, not an emitter change. It has to be:
//!   `codegen::aarch64_relax::relax_conditional_branches` (src/codegen/aarch64_relax.rs:30-71)
//!   decides whether a conditional branch needs an inverse-condition island from the SOURCE
//!   BYTE DISTANCE to its target in the whole file, so splitting before it would change which
//!   branches are relaxed. Splitting after it cannot: every slice is a byte range of the exact
//!   string that pass produced.
//! - Slices are contiguous ranges of the emission order and are handed to `ld` in that order.
//!   They are NEVER reordered or bin-packed by size: emitted `__data` records are adjacent by
//!   construction, and a size-greedy repack links fine and dies at startup.
//! - `jobs <= 1` returns the input unchanged, byte for byte. That is the identity gate.

use crate::fast_hash::{FastMap, FastSet};

use crate::codegen::platform::{AppleVariant, Arch, Platform, Target};
use crate::codegen::Emit;

/// Header line `codegen_support::visibility::append_visibility_directives`
/// (src/codegen_support/visibility.rs:87) writes before the FILE-GLOBAL block of
/// `.private_extern` directives it appends to every finished user object.
///
/// That block names every `.globl`/`.comm` symbol in the file except the export
/// allowlist, so it cannot be copied verbatim into a slice that does not define
/// those symbols: a `.private_extern` naming a symbol the object does not define
/// is a private external reference with no definition. It is therefore stripped
/// from the body and re-emitted per slice over exactly the symbols that slice
/// defines.
const VISIBILITY_FOOTER_MARKER: &str =
    "// -- internal symbols are local to the cdylib public ABI --";

/// Prefix given to an assembler temporary that must become a real symbol because
/// the split put its definition and one of its references in different slices.
///
/// A Mach-O `L`-prefixed name is an assembler temporary: it never reaches the
/// object's symbol table, so a reference to it from another object would be an
/// undefined temporary, and `.globl`-ing one is rejected outright ("non-local
/// symbol required" — the error `Emitter::label_global` already asserts against,
/// src/codegen_support/emit.rs:145-151). The only way to let such a reference
/// cross a slice boundary is to rename the label to a non-temporary name in
/// EVERY slice and publish it `.private_extern` in the defining one. The prefix
/// is deliberately unlike anything the emitter mints, so the rename cannot
/// collide, and greppable, so a symbolicated profile explains itself.
const PROMOTED_PREFIX: &str = "_elephc_xslice_";

/// Result of one collected split (tests only; production renders a `SplitPlan` slice by slice).
#[cfg(test)]
pub(super) struct SplitOutcome {
    /// Slice bodies in emission order. Exactly one entry when the input was not split.
    pub(super) slices: Vec<String>,
    /// Assembler temporaries renamed because their span crossed a cut.
    pub(super) promoted: Vec<String>,
    /// Plain (non-`.globl`) labels published `.private_extern` because another
    /// slice references them.
    pub(super) published: usize,
}

#[cfg(test)]
impl SplitOutcome {
    /// Returns the untouched input as a single slice.
    fn unsplit(source: &str) -> Self {
        Self {
            slices: vec![source.to_string()],
            promoted: Vec::new(),
            published: 0,
        }
    }

    /// Returns whether the input was actually cut.
    pub(super) fn is_split(&self) -> bool {
        self.slices.len() > 1
    }
}

/// Returns whether this target/artifact pair is one the splitter is implemented for.
///
/// macOS AArch64 executables only. The measured cost is the single blocking `as`
/// on the macOS user object, and every hazard below was established against the
/// Mach-O assembler: ELF splits text per symbol (`Emitter::label_global`,
/// src/codegen_support/emit.rs:152-157) and would need its own `.section` replay
/// and its own `.L` locality rules, and a `Staticlib` goes through `ar` with a
/// fixed two-member argument list rather than through the link line. Every other
/// combination keeps today's exact single-`as` path.
pub(super) fn supports_split(target: Target, emit: Emit) -> bool {
    target.platform == Platform::MacOS
        && target.arch == Arch::AArch64
        && target.apple_variant == AppleVariant::MacOS
        && emit == Emit::Executable
}

/// One label definition and the span of lines that mention it.
#[derive(Clone, Copy)]
struct LabelSpan {
    /// Line index of the defining `name:` (or `.comm name, …`).
    def: usize,
    /// Lowest line index that defines or references the name.
    lo: usize,
    /// Highest line index that defines or references the name.
    hi: usize,
}

/// Sources at least this large are indexed and scanned for boundary candidates by several threads.
/// A test lowers it to exercise the parallel paths on a small fixture.
#[cfg(not(test))]
const PARALLEL_INDEX_BYTES: usize = 32 << 20;
#[cfg(test)]
const PARALLEL_INDEX_BYTES: usize = 1 << 10;

/// Threads used by the whole-file passes.
fn worker_threads() -> usize {
    std::thread::available_parallelism().map(|count| count.get()).unwrap_or(1).clamp(1, 32)
}

/// How many threads the line index and the candidate scan use for a source of `bytes` bytes.
fn index_chunks(bytes: usize) -> usize {
    if bytes >= PARALLEL_INDEX_BYTES { worker_threads() } else { 1 }
}

/// The body's lines as byte offsets into the source.
///
/// One `u64` per line instead of a `&str` (16 bytes) plus a separate `usize` offset table: the
/// Symfony `--web` build has 44.7 million lines, and the two `Vec<&str>` copies and the offset
/// table this replaces cost 1.8 GB of the assembly phase's peak. The offsets are 64-bit because
/// the preload build's body passes 4 GiB, and assembling such a body whole puts `bl` targets
/// out of their +-128 MB range ("fixup value out of range").
struct Lines<'a> {
    source: &'a str,
    /// Start of every line, plus one final entry: the end of the last line.
    starts: Vec<u64>,
}

impl<'a> Lines<'a> {
    /// Indexes every line of `source`, `\n` included.
    fn new(source: &'a str) -> Self {
        let bytes = source.as_bytes();
        let chunks = index_chunks(bytes.len());
        // Each worker indexes the newlines of one byte range; the ranges are joined in order, so
        // the table is the one a single pass would build.
        let per = bytes.len().div_ceil(chunks).max(1);
        let parts: Vec<Vec<u64>> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..chunks)
                .map(|chunk| {
                    let lo = (chunk * per).min(bytes.len());
                    let hi = ((chunk + 1) * per).min(bytes.len());
                    scope.spawn(move || {
                        let mut found = Vec::with_capacity((hi - lo) / 32 + 1);
                        found.extend(
                            memchr::memchr_iter(b'\n', &bytes[lo..hi])
                                .map(|index| (lo + index + 1) as u64),
                        );
                        found
                    })
                })
                .collect();
            workers.into_iter().map(|w| w.join().expect("line index panicked")).collect()
        });
        let mut starts = Vec::with_capacity(parts.iter().map(Vec::len).sum::<usize>() + 2);
        starts.push(0u64);
        for part in parts {
            starts.extend(part);
        }
        if *starts.last().expect("seeded above") as usize != source.len() {
            starts.push(source.len() as u64);
        }
        Self { source, starts }
    }

    /// Number of lines.
    fn len(&self) -> usize {
        self.starts.len() - 1
    }

    /// Line `index`, `\n` included.
    fn get(&self, index: usize) -> &'a str {
        &self.source[self.starts[index] as usize..self.starts[index + 1] as usize]
    }

    /// Byte offset of line `index`; `offset(len())` is the end of the body.
    fn offset(&self, index: usize) -> usize {
        self.starts[index] as usize
    }

    /// Lines in order.
    fn iter(&self) -> impl Iterator<Item = &'a str> + '_ {
        (0..self.len()).map(move |index| self.get(index))
    }

    /// Lines `range.start..range.end` in order.
    #[cfg(test)]
    fn range(&self, range: std::ops::Range<usize>) -> impl Iterator<Item = &'a str> + '_ {
        range.map(move |index| self.get(index))
    }

    /// Keeps only the first `len` lines.
    fn truncate(&mut self, len: usize) {
        self.starts.truncate(len + 1);
    }
}

impl std::ops::Index<usize> for Lines<'_> {
    type Output = str;

    fn index(&self, index: usize) -> &str {
        self.get(index)
    }
}

/// Everything one scan of the body hands to cut selection and rendering.
struct Analysis<'a> {
    /// Body lines, `\n` included, in emission order. Concatenating them reproduces the body.
    lines: Lines<'a>,
    /// Names in first-definition order; the index into this vector is a label id.
    names: Vec<&'a str>,
    /// Label id per name.
    ids: FastMap<&'a str, usize>,
    /// Span per label id, parallel to `names`.
    spans: Vec<LabelSpan>,
    /// Line indices a slice may start at, ascending.
    candidates: Vec<usize>,
    /// Name of the label each candidate opens, parallel to `candidates`.
    ///
    /// The stable cut policy hashes THIS rather than a byte offset, so a boundary's
    /// identity does not depend on the size of anything else in the file.
    candidate_names: Vec<&'a str>,
    /// `(line, directive)` for every section change, ascending by line.
    sections: Vec<(usize, &'a str)>,
    /// File-global directives replayed at the top of every later slice.
    file_prologue: Vec<&'a str>,
    /// Names the file-global visibility footer marks, in footer order.
    footer: Vec<&'a str>,
    /// The same names, for membership tests.
    footer_set: FastSet<&'a str>,
    /// Names declared `.globl` anywhere in the body.
    globl: FastSet<&'a str>,
    /// Per label id, whether some line takes its ADDRESS (`@PAGE`, `@PAGEOFF`, `@GOTPAGE`...).
    address_taken: Vec<bool>,
}

impl Analysis<'_> {
    /// Byte offset of body line `index`; `offset(lines.len())` is the body's size.
    fn offset(&self, index: usize) -> usize {
        self.lines.offset(index)
    }

    /// The body's size in bytes.
    fn total(&self) -> usize {
        self.lines.offset(self.lines.len())
    }
}

/// What a streamed split produced, without the slice texts themselves.
pub(super) struct SplitSummary {
    /// Assembler temporaries renamed because their span crossed a cut.
    pub(super) promoted: Vec<String>,
    /// Plain labels published `.private_extern` because another slice references them.
    pub(super) published: usize,
}

/// Splits one finished assembly text into at most `jobs` contiguous slices.
///
/// `jobs <= 1`, an input with no legal cut, a promotion name collision, and a
/// visibility footer naming a symbol the body does not define all return the
/// input as a single slice, so the caller's fallback is always "do exactly what
/// we do today".
#[cfg(test)]
pub(super) fn split_assembly(source: &str, jobs: usize, local_prefix: &str) -> SplitOutcome {
    let mut slices = Vec::new();
    let streamed = split_assembly_streaming(source, jobs, local_prefix, |_, slice| {
        slices.push(slice);
        Ok(())
    })
    .expect("collecting slices cannot fail");
    match streamed {
        Some(summary) => SplitOutcome {
            slices,
            promoted: summary.promoted,
            published: summary.published,
        },
        None => SplitOutcome::unsplit(source),
    }
}

/// Splits `source` like `split_assembly`, handing each slice to `sink` in order.
///
/// Returns `None`, having called `sink` zero times, whenever `split_assembly` would return the
/// input whole.
#[cfg(test)]
pub(super) fn split_assembly_streaming(
    source: &str,
    jobs: usize,
    local_prefix: &str,
    mut sink: impl FnMut(usize, String) -> std::io::Result<()>,
) -> std::io::Result<Option<SplitSummary>> {
    let Some(plan) = plan_split(source, jobs, local_prefix, false) else {
        return Ok(None);
    };
    for part in 0..plan.slices() {
        let mut out = String::new();
        plan.render_slice(part, &mut |text| {
            out.push_str(text);
            Ok(())
        })?;
        sink(part, out)?;
    }
    Ok(Some(plan.summary))
}

/// Decides how `source` is split, or `None` when it has to be assembled whole.
///
/// `jobs <= 1`, an input with no legal cut, a promotion name collision, and a visibility footer
/// naming a symbol the body does not define all return `None`, so the caller's fallback is always
/// "assemble the file as it is".
///
/// `encode` hands the instructions `aarch64_encode` knows to the assembler as `.inst` words.
pub(super) fn plan_split<'a>(
    source: &'a str,
    jobs: usize,
    local_prefix: &'a str,
    encode: bool,
) -> Option<SplitPlan<'a>> {
    if jobs <= 1 || source.is_empty() {
        return None;
    }
    let analysis = analyze(source, local_prefix);
    if analysis.footer.iter().any(|name| !analysis.ids.contains_key(name)) {
        // The footer is authoritative about visibility. If a name in it has no
        // definition this pass can find, partitioning it would silently drop a
        // directive; refuse instead.
        return None;
    }
    let cuts = match cut_policy() {
        CutPolicy::Balanced => choose_cuts(&analysis, jobs),
        CutPolicy::Stable => choose_cuts_stable(&analysis, jobs),
    };
    if cuts.is_empty() {
        return None;
    }
    let slice_of = |line: usize| cuts.partition_point(|&cut| cut <= line);
    let parts = cuts.len() + 1;

    // HAZARD 2 — a top-level symbol the emitter wrote as a plain `name:` is a
    // Mach-O local: it is not in the object's symbol table, so a reference to it
    // from another slice would not resolve. Every such label that IS referenced
    // from another slice is published `.private_extern` in its defining slice —
    // external enough for the linker to bind, still not an export, therefore
    // still not a `-dead_strip` root (src/codegen_support/visibility.rs:18-21).
    let mut promoted: FastMap<&'a str, String> = FastMap::default();
    let mut promoted_names: Vec<String> = Vec::new();
    let mut published: Vec<Vec<&'a str>> = vec![Vec::new(); parts];
    let mut published_count = 0usize;
    // HAZARD 5 — a Mach-O relocation to an assembler temporary is expressed as the section plus
    // an addend, and ARM64_RELOC_ADDEND holds only 24 bits: `adrp xN, L_x@PAGE` fails with
    // "addend too big for relocation" once `L_x` sits more than 8 MB into its section. The
    // Symfony demo's preload build has a 1.7 GB slice no cut can divide (conditional-branch
    // intervals), and its shared helpers are that far in. Promoting such a label makes the
    // relocation symbol-based with a zero addend. Only ADDRESS-TAKEN temporaries are promoted: a
    // branch target inside a function must stay a temporary, because a real symbol starts a new
    // atom and fallthrough into it would not survive `-dead_strip`.
    const ADDEND_SAFE_SLICE_BYTES: usize = 6 << 20;
    let slice_bytes: Vec<usize> = (0..parts)
        .map(|part| {
            let start = if part == 0 { 0 } else { cuts[part - 1] };
            let end = cuts.get(part).copied().unwrap_or(analysis.lines.len());
            analysis.offset(end) - analysis.offset(start)
        })
        .collect();
    for (id, &name) in analysis.names.iter().enumerate() {
        let span = analysis.spans[id];
        let too_far_for_an_addend = analysis.address_taken[id]
            && name.starts_with(local_prefix)
            && slice_bytes[slice_of(span.def)] > ADDEND_SAFE_SLICE_BYTES;
        if slice_of(span.lo) == slice_of(span.hi) && !too_far_for_an_addend {
            continue;
        }
        if name.starts_with(local_prefix) {
            let renamed = format!("{PROMOTED_PREFIX}{name}");
            if analysis.ids.contains_key(renamed.as_str()) {
                // A collision would silently merge two symbols. Refuse the split
                // rather than guess a second name.
                return None;
            }
            published[slice_of(span.def)].push(name);
            promoted.insert(name, renamed.clone());
            promoted_names.push(renamed);
            continue;
        }
        if analysis.footer_set.contains(name) || analysis.globl.contains(name) {
            continue;
        }
        published[slice_of(span.def)].push(name);
        published_count += 1;
    }
    promoted_names.sort();
    Some(SplitPlan {
        source,
        analysis,
        cuts,
        local_prefix,
        encode,
        promoted,
        published,
        summary: SplitSummary {
            promoted: promoted_names,
            published: published_count,
        },
        compact: None,
    })
}

/// Where a split cuts `source`, and what each slice renames and publishes to stay linkable.
///
/// Every slice renders from this alone, so slices can be written and assembled independently
/// and at the same time: rendering them one after another, before any assembler started, was a
/// serial stretch of the Symfony build's assembly phase.
pub(super) struct SplitPlan<'a> {
    source: &'a str,
    analysis: Analysis<'a>,
    cuts: Vec<usize>,
    local_prefix: &'a str,
    /// Whether AArch64 instructions `aarch64_encode` knows are written as `.inst` words.
    encode: bool,
    /// Assembler temporaries renamed because their span crosses a cut.
    promoted: FastMap<&'a str, String>,
    /// Per slice, the names it defines that another slice references.
    published: Vec<Vec<&'a str>>,
    pub(super) summary: SplitSummary,
    /// The target's full-line comment prefix when slices are written compact (see
    /// [`SplitPlan::compact`]); `None` writes every line as the `.s` has it.
    compact: Option<&'static str>,
}

impl<'a> SplitPlan<'a> {
    /// Writes the slices COMPACT: no full-line comments, no indentation, and every assembler
    /// temporary renamed to a short name built from its definition index.
    ///
    /// The slices exist only for `as`; the `.s` the build wrote, which `--emit-asm` users and the
    /// tests read, keeps every byte. On the Symfony demo indentation was 12% of the text, comment
    /// lines 6%, and temporaries such as `L_eir_Symfony_Component_Mime_M_array_push_done_<hash>_9`
    /// another 28% between their definitions and their references -- all of it lexed, hashed and
    /// stored by `as`. The index comes from the whole-file analysis every slice shares, so a
    /// temporary gets the same short name in every slice, and two temporaries can never share one.
    /// A promoted temporary keeps its promoted (exported) name.
    pub(super) fn compact(&mut self, comment_prefix: &'static str) {
        self.compact = Some(comment_prefix);
    }

    /// Number of slices.
    pub(super) fn slices(&self) -> usize {
        self.cuts.len() + 1
    }

    /// Copies lines `start..end` as the `.s` has them, renaming promoted temporaries and writing
    /// encodable AArch64 instructions as `.inst` words.
    fn render_verbatim_body(
        &self,
        start: usize,
        end: usize,
        out: &mut SliceWriter<'_>,
    ) -> std::io::Result<()> {
        let analysis = &self.analysis;
        let mut run = start;
        let mut rewritten = String::new();
        for index in start..end {
            let line = analysis.lines.get(index);
            let rename = !self.promoted.is_empty() && line.contains(self.local_prefix);
            let word = if self.encode && !rename {
                super::aarch64_encode::encode(line)
            } else {
                None
            };
            if !rename && word.is_none() {
                continue;
            }
            if index > run {
                out.put(&self.source[analysis.offset(run)..analysis.offset(index)])?;
            }
            rewritten.clear();
            match word {
                Some(word) => {
                    use std::fmt::Write as _;
                    let _ = writeln!(rewritten, "    .inst 0x{word:08x}");
                }
                None => push_renamed(&mut rewritten, line, &self.promoted, self.local_prefix),
            }
            out.put(&rewritten)?;
            run = index + 1;
        }
        if end > run {
            out.put(&self.source[analysis.offset(run)..analysis.offset(end)])?;
        }
        Ok(())
    }

    /// Writes lines `start..end` compact (see [`SplitPlan::compact`]).
    fn render_compact_body(
        &self,
        start: usize,
        end: usize,
        comment_prefix: &str,
        out: &mut SliceWriter<'_>,
    ) -> std::io::Result<()> {
        use std::fmt::Write as _;
        let analysis = &self.analysis;
        let mut buffer = String::with_capacity(1 << 20);
        // One searcher per slice: `str::contains` built a fresh one for each of the module's
        // tens of millions of lines, a top frame of the assembler phase.
        let local_label = memchr::memmem::Finder::new(self.local_prefix.as_bytes());
        for index in start..end {
            let line = analysis.lines.get(index);
            let text = ascii_trim(line);
            if text.is_empty() || text.starts_with(comment_prefix) {
                continue;
            }
            let word = if self.encode && local_label.find(text.as_bytes()).is_none() {
                super::aarch64_encode::encode(line)
            } else {
                None
            };
            match word {
                Some(word) => {
                    let _ = writeln!(buffer, ".inst 0x{word:08x}");
                }
                None => {
                    push_compacted(&mut buffer, text, &self.promoted, self.local_prefix, &analysis.ids);
                    buffer.push('\n');
                }
            }
            if buffer.len() >= 1 << 20 {
                out.put(&buffer)?;
                buffer.clear();
            }
        }
        if !buffer.is_empty() {
            out.put(&buffer)?;
        }
        Ok(())
    }

    fn slice_of(&self, line: usize) -> usize {
        self.cuts.partition_point(|&cut| cut <= line)
    }

    /// Body lines `start..end` of slice `part`.
    fn line_range(&self, part: usize) -> (usize, usize) {
        let start = if part == 0 { 0 } else { self.cuts[part - 1] };
        let end = self.cuts.get(part).copied().unwrap_or(self.analysis.lines.len());
        (start, end)
    }

    /// What a later slice restates before its first body line: empty for the first slice.
    fn slice_prologue(&self, part: usize) -> String {
        let analysis = &self.analysis;
        let mut prologue = String::new();
        if part == 0 {
            return prologue;
        }
        let (start, _) = self.line_range(part);
        // HAZARD 4 — `.section`/`.data` state and the `.intel_syntax` switch
        // are file-global. A slice that starts mid-`.data` would otherwise
        // assemble its records into `__text`.
        for directive in &analysis.file_prologue {
            prologue.push_str(directive);
            prologue.push('\n');
        }
        let section = section_in_effect(&analysis.sections, start);
        prologue.push_str(section);
        prologue.push('\n');
        // HAZARD 3 — a `.quad` record whose own alignment directive sits in
        // the previous slice starts the new object unaligned, and `ld`
        // rejects the object outright ("pointer not aligned"). Restate the
        // section's alignment unless the slice already opens with one.
        let opening = analysis.lines[start].trim_start();
        if !opening.starts_with(".align") && !opening.starts_with(".p2align") {
            prologue.push_str(if section == ".text" { ".p2align 2\n" } else { ".p2align 3\n" });
        }
        prologue
    }

    /// Slice `part` for the in-tree assembler, as three consecutive parts -- the prologue, the
    /// body exactly as the `.s` has it, and the visibility footer -- plus the renaming its promoted
    /// temporaries need (`inhouse::Names`).
    ///
    /// The body is borrowed, not rendered: on the Symfony demo rendering the compact slices copied
    /// 2.7 GB and cost 19 s of the assembly phase, and the in-tree assembler, which skips comments
    /// and indentation itself and keys its labels by name, gains nothing from the copy.
    pub(super) fn assembler_parts(&self, part: usize) -> (String, &'a str, String) {
        let (start, end) = self.line_range(part);
        let body = &self.source[self.analysis.offset(start)..self.analysis.offset(end)];
        let mut footer = self.visibility_footer(part);
        if !footer.is_empty() {
            footer.insert(0, '\n');
        }
        (self.slice_prologue(part), body, footer)
    }

    /// The temporaries a cut promoted, by their name in the `.s`.
    pub(super) fn promoted(&self) -> &FastMap<&'a str, String> {
        &self.promoted
    }

    /// The target's assembler-temporary prefix.
    pub(super) fn local_prefix(&self) -> &'a str {
        self.local_prefix
    }

    /// Writes slice `part` through `write`, in order, as a sequence of text chunks.
    pub(super) fn render_slice(
        &self,
        part: usize,
        write: &mut dyn FnMut(&str) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        let analysis = &self.analysis;
        let start = if part == 0 { 0 } else { self.cuts[part - 1] };
        let end = self.cuts.get(part).copied().unwrap_or(analysis.lines.len());
        let mut out = SliceWriter {
            write,
            ends_with_newline: false,
        };
        out.put(&self.slice_prologue(part))?;
        // `lines` indexes `source` itself, so `source[offset(a)..offset(b)]` IS lines `a..b` joined --
        // the same bytes the per-line loop pushed, in one copy. On this module that loop ran
        // 42 million times per build to rebuild a string it already had.
        //
        // A line is copied as part of such a run unless it must be renamed or can be handed to the
        // assembler already encoded. The encoded word is exactly what `as` would produce, so the
        // object does not change; the assembler only skips matching the instruction, which is
        // most of its time (see `aarch64_encode`).
        if let Some(comment_prefix) = self.compact {
            self.render_compact_body(start, end, comment_prefix, &mut out)?;
        } else {
            self.render_verbatim_body(start, end, &mut out)?;
        }
        let footer = self.visibility_footer(part);
        if !footer.is_empty() {
            if !out.ends_with_newline {
                out.put("\n")?;
            }
            out.put("\n")?;
            out.put(VISIBILITY_FOOTER_MARKER)?;
            out.put("\n")?;
            out.put(&footer)?;
        }
        Ok(())
    }

    /// This slice's share of the file-global visibility footer, marker excluded.
    fn visibility_footer(&self, part: usize) -> String {
        let analysis = &self.analysis;
        let mut written: FastSet<&str> = FastSet::default();
        let mut block = String::new();
        for &name in analysis.footer.iter().chain(self.published[part].iter()) {
            let Some(&id) = analysis.ids.get(name) else {
                continue;
            };
            if self.slice_of(analysis.spans[id].def) != part || !written.insert(name) {
                continue;
            }
            block.push_str(".private_extern ");
            block.push_str(self.promoted.get(name).map(String::as_str).unwrap_or(name));
            block.push('\n');
        }
        block
    }
}

/// Forwards slice text to a writer, remembering whether it last ended a line.
struct SliceWriter<'w> {
    write: &'w mut dyn FnMut(&str) -> std::io::Result<()>,
    ends_with_newline: bool,
}

impl SliceWriter<'_> {
    fn put(&mut self, text: &str) -> std::io::Result<()> {
        if let Some(&last) = text.as_bytes().last() {
            self.ends_with_newline = last == b'\n';
        }
        (self.write)(text)
    }
}

/// Runs the single body scan that every later decision reads.
fn analyze<'a>(source: &'a str, local_prefix: &str) -> Analysis<'a> {
    let mut lines = Lines::new(source);
    let body_end = footer_start(&lines);
    let footer = footer_names((body_end..lines.len()).map(|index| lines.get(index)));
    let footer_set: FastSet<&str> = footer.iter().copied().collect();
    lines.truncate(body_end);

    // Sized up front: the Symfony body defines about one label per 400 bytes, and growing a
    // 3.6-million-entry map by doubling rehashes all of it a dozen times.
    let expected_labels = source.len() / 400;
    let mut names: Vec<&str> = Vec::with_capacity(expected_labels);
    let mut ids: FastMap<&str, usize> =
        FastMap::with_capacity_and_hasher(expected_labels, Default::default());
    let mut spans: Vec<LabelSpan> = Vec::with_capacity(expected_labels);
    let mut sections: Vec<(usize, &str)> = Vec::new();
    let mut file_prologue: Vec<&str> = Vec::new();
    let mut globl: FastSet<&str> = FastSet::default();
    let mut numeric: FastMap<&str, Vec<usize>> = FastMap::default();

    for (index, raw) in lines.iter().enumerate() {
        let line = ascii_trim(raw);
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix(".globl ") {
            globl.insert(rest.trim());
            continue;
        }
        if let Some(rest) = line.strip_prefix(".comm ") {
            let name = rest.split(',').next().unwrap_or("").trim();
            define(name, index, &mut names, &mut ids, &mut spans);
            continue;
        }
        if let Some(name) = line.strip_suffix(':') {
            if is_label_name(name) {
                if name.bytes().all(|byte| byte.is_ascii_digit()) {
                    numeric.entry(name).or_default().push(index);
                } else {
                    define(name, index, &mut names, &mut ids, &mut spans);
                }
                continue;
            }
        }
        if is_section_directive(line) {
            sections.push((index, line));
            continue;
        }
        if line == ".intel_syntax noprefix" && !file_prologue.contains(&line) {
            file_prologue.push(line);
        }
    }

    // Every reference widens its label's span; nothing else in this pass writes. Workers take
    // blocks of lines from a shared counter, record the lowest and highest line they saw per
    // label, and those are folded into `spans` by min/max -- which is what the single loop
    // computed, in any order. This pass was 15 of the 17 seconds the Symfony split took, spent
    // hashing every token of 44 million lines, mnemonics and registers included. Blocks rather
    // than one range per thread: an efficiency core given a fixed share finished last.
    let starts_a_label = label_first_bytes(&names);
    let threads = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(1, 32);
    const BLOCK: usize = 1 << 16;
    let next_block = std::sync::atomic::AtomicUsize::new(0);
    type Partial = (FastMap<u32, (u32, u32)>, Vec<(usize, usize)>, Vec<u32>);
    let partials: Vec<Partial> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                let (lines, ids, spans, numeric) = (&lines, &ids, &spans, &numeric);
                let (next_block, starts_a_label) = (&next_block, &starts_a_label);
                scope.spawn(move || {
                    let mut touched = FastMap::default();
                    let mut hard = Vec::new();
                    let mut taken = Vec::new();
                    loop {
                        let first =
                            next_block.fetch_add(BLOCK, std::sync::atomic::Ordering::Relaxed);
                        if first >= lines.len() {
                            break;
                        }
                        let range = first..(first + BLOCK).min(lines.len());
                        scan_references(
                            lines,
                            range,
                            ids,
                            spans,
                            numeric,
                            starts_a_label,
                            &mut touched,
                            &mut hard,
                            &mut taken,
                        );
                    }
                    (touched, hard, taken)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("reference scan panicked"))
            .collect()
    });
    let mut hard: Vec<(usize, usize)> = Vec::new();
    let mut address_taken = vec![false; names.len()];
    for (touched, intervals, taken) in partials {
        for id in taken {
            address_taken[id as usize] = true;
        }
        for (id, (lo, hi)) in touched {
            let span = &mut spans[id as usize];
            span.lo = span.lo.min(lo as usize);
            span.hi = span.hi.max(hi as usize);
        }
        hard.extend(intervals);
    }
    hard.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(hard.len());
    for (lo, hi) in hard {
        let overlaps = merged.last().is_some_and(|&(_, last_hi)| lo <= last_hi);
        if overlaps {
            let last = merged.last_mut().expect("checked non-empty above");
            last.1 = last.1.max(hi);
        } else {
            merged.push((lo, hi));
        }
    }

    let (candidates, candidate_names) = boundary_candidates(&lines, local_prefix, &merged);
    Analysis {
        lines,
        names,
        ids,
        spans,
        candidates,
        candidate_names,
        sections,
        file_prologue,
        footer,
        footer_set,
        globl,
        address_taken,
    }
}

/// Trims ASCII whitespace off both ends of one listing line.
///
/// `str::trim` decodes every edge character to ask Unicode whether it is whitespace; the listing
/// is ASCII outside string literals, and the per-line passes over its ~40 M lines spent 13% of
/// the assembling phase in that decoding.
pub(super) fn ascii_trim(line: &str) -> &str {
    line.trim_ascii()
}

/// Which bytes some defined label begins with; a token starting with any other byte is not one.
fn label_first_bytes(names: &[&str]) -> [bool; 256] {
    let mut starts = [false; 256];
    for name in names {
        if let Some(&byte) = name.as_bytes().first() {
            starts[byte as usize] = true;
        }
    }
    starts
}

/// The reference pass over lines `range`: widens, per label, the lowest and highest line that
/// references it in `touched`, and adds the hard intervals (conditional branches, numeric locals)
/// the range opens to `hard`.
#[allow(clippy::too_many_arguments)]
fn scan_references(
    lines: &Lines<'_>,
    range: std::ops::Range<usize>,
    ids: &FastMap<&str, usize>,
    spans: &[LabelSpan],
    numeric: &FastMap<&str, Vec<usize>>,
    starts_a_label: &[bool; 256],
    touched: &mut FastMap<u32, (u32, u32)>,
    hard: &mut Vec<(usize, usize)>,
    address_taken: &mut Vec<u32>,
) {
    for index in range {
        let raw = lines.get(index);
        let line = ascii_trim(raw);
        if line.is_empty() {
            continue;
        }
        let is_definition = line.strip_suffix(':').is_some_and(is_label_name);
        if !is_definition
            && !line.starts_with(".globl ")
            && !line.starts_with(".private_extern ")
            && !line.starts_with(".comm ")
        {
            // Asked only once a token names a known label: most of the 44 M lines of a Symfony
            // build name none, and two substring searches per line were a visible share of this
            // pass. `@GOTPAGE` contains `PAGE` after its `@`, so one `@PAGE`/`@GOTPAGE` test is
            // a search for `PAGE` right after an `@`.
            let mut takes_address: Option<bool> = None;
            visit_identifiers(raw, |token| {
                if !starts_a_label[token.as_bytes()[0] as usize] {
                    return;
                }
                if let Some(&id) = ids.get(token) {
                    if *takes_address
                        .get_or_insert_with(|| line.contains("@PAGE") || line.contains("@GOTPAGE"))
                    {
                        address_taken.push(id as u32);
                    }
                    let line = index as u32;
                    touched
                        .entry(id as u32)
                        .and_modify(|(lo, hi)| {
                            *lo = (*lo).min(line);
                            *hi = (*hi).max(line);
                        })
                        .or_insert((line, line));
                }
            });
        }
        // HAZARD 1 — `as` refuses a conditional branch to an external symbol
        // ("conditional branch requires assembler-local label"). Branch and target
        // therefore have to stay in the same slice; the pair is a hard interval and
        // no cut may fall inside it. Rewriting `b.cond t` as `b.!cond skip; b t` is
        // the alternative and is deliberately not taken: keeping the pair together
        // costs nothing, because every such pair is intra-function, and rewriting
        // would change instruction counts and therefore later branch distances.
        if let Some(target) = conditional_branch_target(line) {
            if let Some(&id) = ids.get(target) {
                let def = spans[id].def;
                hard.push((index.min(def), index.max(def)));
            }
        }
        // A numeric local (`1:`) is resolved by direction, so `1f`/`1b` reaches
        // the nearest definition and can never leave the object it was written in.
        for (position, direction) in numeric_references(raw) {
            let Some(definitions) = numeric.get(position) else {
                continue;
            };
            if direction == b'f' {
                if let Some(next) = nearest_definition(definitions, index, true, || {}) {
                    hard.push((index, next));
                }
            } else if let Some(previous) = nearest_definition(definitions, index, false, || {}) {
                hard.push((previous, index));
            }
        }
    }
}

/// The line of the definition a numeric reference on line `index` resolves to: the first
/// definition at or after it for `1f`, the last at or before it for `1b`. `definitions` ascends.
///
/// A numeric label such as `1:` is defined hundreds of thousands of times in a large file, so a
/// linear `find` per reference made the reference pass quadratic: the Symfony `--web` scan ran
/// at 45 MB/s over its first blocks and 3 MB/s over its last, and the pass cost 100 CPU-seconds
/// instead of 10. The nearest definition is found by bisection; `probe` is called once per
/// comparison so a test can bound the work.
fn nearest_definition(
    definitions: &[usize],
    index: usize,
    forward: bool,
    mut probe: impl FnMut(),
) -> Option<usize> {
    if forward {
        let at = definitions.partition_point(|&line| {
            probe();
            line < index
        });
        definitions.get(at).copied()
    } else {
        let at = definitions.partition_point(|&line| {
            probe();
            line <= index
        });
        at.checked_sub(1).map(|at| definitions[at])
    }
}

/// Records a first definition of `name` at `index`.
fn define<'a>(
    name: &'a str,
    index: usize,
    names: &mut Vec<&'a str>,
    ids: &mut FastMap<&'a str, usize>,
    spans: &mut Vec<LabelSpan>,
) {
    if name.is_empty() || ids.contains_key(name) {
        return;
    }
    ids.insert(name, names.len());
    names.push(name);
    spans.push(LabelSpan {
        def: index,
        lo: index,
        hi: index,
    });
}

/// Returns the line index at which the file-global visibility footer starts.
fn footer_start(lines: &Lines<'_>) -> usize {
    (0..lines.len())
        .rev()
        .find(|&index| lines.get(index).trim() == VISIBILITY_FOOTER_MARKER)
        .unwrap_or(lines.len())
}

/// Returns the symbols named by a visibility footer, in footer order.
fn footer_names<'a>(footer: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    footer
        .filter_map(|line| {
            let line = ascii_trim(line);
            line.strip_prefix(".private_extern ")
                .or_else(|| line.strip_prefix(".hidden "))
                .map(str::trim)
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// Returns whether a name is a plain assembly label identifier.
fn is_label_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'.'))
}

/// Returns whether a directive changes the current section.
fn is_section_directive(line: &str) -> bool {
    line == ".text"
        || line == ".data"
        || line == ".bss"
        || line == ".const"
        || line == ".cstring"
        || line.starts_with(".section")
}

/// Calls `visit` with every identifier token on one line, skipping quoted strings.
///
/// Quote handling matches `codegen_support::emit::localize_internal_labels`
/// (src/codegen_support/emit.rs:374-420): a quoted run is passed over verbatim
/// and an unmatched quote is bounded to its physical line, so a user string
/// constant can neither be read as a symbol nor hide the rest of the file.
fn visit_identifiers<'a>(line: &'a str, mut visit: impl FnMut(&'a str)) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' {
            index += 1;
            let mut escaped = false;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if byte == b'\n' || (!escaped && byte == b'"') {
                    break;
                }
                escaped = !escaped && byte == b'\\';
            }
        } else if is_identifier_start(byte) {
            let start = index;
            while index < bytes.len() && is_identifier_byte(bytes[index]) {
                index += 1;
            }
            visit(&line[start..index]);
        } else {
            index += 1;
        }
    }
}

/// Returns whether a byte may begin an identifier token.
fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$' | b'.')
}

/// Returns whether a byte may continue an identifier token.
fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'.')
}

/// Returns the symbolic target of an AArch64 conditional branch, if the line is one.
///
/// Mirrors the shapes `codegen::aarch64_relax::parse_conditional_branch`
/// (src/codegen/aarch64_relax.rs:74-126) recognises, because those are exactly
/// the instructions whose target `as` requires to be assembler-local.
fn conditional_branch_target(line: &str) -> Option<&str> {
    let (mnemonic, operands) = line.split_once(char::is_whitespace)?;
    let conditional =
        mnemonic.starts_with("b.") || matches!(mnemonic, "cbz" | "cbnz" | "tbz" | "tbnz");
    if !conditional {
        return None;
    }
    let target = operands.rsplit(',').next()?.trim();
    is_label_name(target).then_some(target)
}

/// Returns `(position, b'b' | b'f')` for every numeric local reference on a line.
fn numeric_references(line: &str) -> Vec<(&str, u8)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() || (index > 0 && is_identifier_byte(bytes[index - 1])) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        let direction = match bytes.get(index).copied() {
            Some(b'b') => b'b',
            Some(b'f') => b'f',
            _ => continue,
        };
        if bytes.get(index + 1).copied().is_some_and(is_identifier_byte) {
            continue;
        }
        out.push((&line[start..index], direction));
        index += 1;
    }
    out
}

/// Returns every line index a slice may legally start at, ascending.
///
/// Two conditions, both conservative:
/// * the line opens a top-level non-temporary label, and control cannot fall
///   through into it — the previous meaningful line is an unconditional transfer
///   (`ret`, `b`, `br`, `brk`) or data. Two slices become two objects that `ld`
///   concatenates with section padding between them, so a fall-through across a
///   cut would run into the padding;
/// * no hard interval (a conditional branch pair, a numeric local's `1b`/`1f`
///   reach) spans the cut.
///
/// The `.align`/`.globl`/comment lines that introduce the label are pulled into
/// the following slice with it, so the label keeps its alignment and declaration.
///
/// Returns the line indices and, parallel to them, the name of the label each one
/// opens. The name is what the stable cut policy hashes, so it has to be carried
/// out of here rather than recovered from the line later.
fn boundary_candidates<'a>(
    lines: &Lines<'a>,
    local_prefix: &str,
    hard: &[(usize, usize)],
) -> (Vec<usize>, Vec<&'a str>) {
    let total = lines.len();
    let chunks = index_chunks(lines.offset(total));
    let per = total.div_ceil(chunks).max(1);
    // The decision for a line reads only earlier lines and `hard`, so ranges are independent;
    // their results are joined in order and de-duplicated across the seams exactly as the single
    // pass de-duplicates against the previous candidate.
    let parts: Vec<(Vec<usize>, Vec<&'a str>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..chunks)
            .map(|chunk| {
                let range = (chunk * per).min(total)..((chunk + 1) * per).min(total);
                scope.spawn(move || candidates_in(lines, local_prefix, hard, range))
            })
            .collect();
        workers.into_iter().map(|w| w.join().expect("candidate scan panicked")).collect()
    });
    let mut candidates: Vec<usize> = Vec::new();
    let mut candidate_names: Vec<&'a str> = Vec::new();
    for (starts, names) in parts {
        for (start, name) in starts.into_iter().zip(names) {
            if candidates.last() != Some(&start) {
                candidates.push(start);
                candidate_names.push(name);
            }
        }
    }
    (candidates, candidate_names)
}

/// The candidates opened by the labels on lines `range`.
fn candidates_in<'a>(
    lines: &Lines<'a>,
    local_prefix: &str,
    hard: &[(usize, usize)],
    range: std::ops::Range<usize>,
) -> (Vec<usize>, Vec<&'a str>) {
    let mut candidates: Vec<usize> = Vec::new();
    let mut candidate_names: Vec<&'a str> = Vec::new();
    for index in range {
        let raw = lines.get(index);
        if raw.starts_with(' ') || raw.starts_with('\t') {
            continue;
        }
        let Some(name) = ascii_trim(raw).strip_suffix(':') else {
            continue;
        };
        if !is_label_name(name)
            || name.starts_with(local_prefix)
            || name.bytes().all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        if !cannot_fall_through(lines, index) {
            continue;
        }
        let mut start = index;
        while start > 0 && introduces_a_label(lines[start - 1].trim()) {
            start -= 1;
        }
        if start == 0 || inside_hard_interval(hard, start) {
            continue;
        }
        if candidates.last() != Some(&start) {
            candidates.push(start);
            candidate_names.push(name);
        }
    }
    (candidates, candidate_names)
}

/// Returns whether control cannot reach the label defined at `index` by falling through.
fn cannot_fall_through(lines: &Lines<'_>, index: usize) -> bool {
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        let line = ascii_trim(&lines[cursor]);
        if line.is_empty() || introduces_a_label(line) {
            continue;
        }
        let mnemonic = line.split_whitespace().next().unwrap_or("");
        return matches!(mnemonic, "ret" | "b" | "br" | "brk")
            || line.starts_with(".quad")
            || line.starts_with(".long")
            || line.starts_with(".short")
            || line.starts_with(".byte")
            || line.starts_with(".ascii")
            || line.starts_with(".space")
            || line.starts_with(".comm")
            || is_section_directive(line);
    }
    true
}

/// Returns whether a line only introduces the label that follows it.
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

/// Returns whether `line` falls strictly inside a merged hard interval.
fn inside_hard_interval(hard: &[(usize, usize)], line: usize) -> bool {
    match hard.binary_search_by(|probe| probe.0.cmp(&line)) {
        Ok(_) | Err(0) => false,
        Err(position) => {
            let (lo, hi) = hard[position - 1];
            lo < line && line <= hi
        }
    }
}

/// Which policy decides where the body is cut.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum CutPolicy {
    /// Cut as near an equal byte split as the legal boundaries allow.
    ///
    /// Every cut is a function of the body's TOTAL size, so any edit moves every
    /// boundary. Best balance, worst cacheability.
    Balanced,
    /// Cut where a boundary's own symbol name hashes into a residue class.
    ///
    /// No boundary decision reads the file's size or any other symbol, so adding,
    /// removing or resizing one function moves no boundary but its own: the edit
    /// dirties the slice it lands in and leaves every other slice's bytes alone.
    /// This is content-defined chunking, the same reason rsync and borg cut where
    /// they do, and it is what makes the slice cache useful across an edit rather
    /// than only across an unchanged rebuild.
    Stable,
}

/// Returns the cut policy this build uses.
///
/// `ELEPHC_ASM_CUTS=stable` opts in. The default stays `Balanced`, which is the
/// policy the split's measurements were taken with; the stable policy changes where
/// the cuts fall, and therefore how many symbols cross one, so it does not become
/// the default until that is measured on a machine quiet enough to measure it.
pub(super) fn cut_policy() -> CutPolicy {
    cut_policy_from_env(std::env::var("ELEPHC_ASM_CUTS").ok().as_deref())
}

/// Maps an `ELEPHC_ASM_CUTS` value to a policy.
///
/// Only `stable` selects the content-defined policy. Anything else, including a
/// typo, keeps today's balanced cuts, so an unrecognised value can never silently
/// change what the assembler is handed.
fn cut_policy_from_env(value: Option<&str>) -> CutPolicy {
    match value {
        Some("stable") => CutPolicy::Stable,
        _ => CutPolicy::Balanced,
    }
}

/// FNV-1a over a symbol name, so a boundary decision depends on that name alone.
fn name_hash(name: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in name.as_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Picks cuts whose positions do not depend on the size of the rest of the file.
///
/// A candidate becomes a cut when the hash of ITS OWN name falls in the residue
/// class, so an edit moves no boundary but its own. Two things keep that from
/// degenerating:
///
/// * the stride is a POWER OF TWO near `candidates / jobs`, not the exact ratio.
///   An exact ratio would be a function of the candidate count, so adding one
///   symbol would change the stride and therefore every boundary — reintroducing
///   precisely the instability this policy exists to remove. Rounding to a power of
///   two means the candidate count has to change by nearly a factor of two before
///   any boundary moves.
/// * a SIZE CAP closes the one hole a hash-based rule leaves: a run of candidates
///   that all hash the wrong way would produce one enormous slice and destroy the
///   parallelism. A cap-forced cut is not stable — it depends on where the previous
///   cut fell — but it fires only on that pathological run, so it bounds the damage
///   instead of spreading it.
fn choose_cuts_stable(analysis: &Analysis<'_>, jobs: usize) -> Vec<usize> {
    let total = analysis.total();
    if total == 0 || analysis.candidates.is_empty() || jobs <= 1 {
        return Vec::new();
    }
    let stride = stable_stride(analysis.candidates.len(), jobs);
    // No slice may exceed twice the ideal share, so the largest slice bounds the
    // speedup at half of what an even split would give rather than at nothing.
    let cap = (total / jobs).saturating_mul(2).max(1);
    let mut cuts: Vec<usize> = Vec::new();
    let mut slice_start = 0usize;
    for (position, &candidate) in analysis.candidates.iter().enumerate() {
        if cuts.last().is_some_and(|&last| candidate <= last) {
            continue;
        }
        let chosen = name_hash(analysis.candidate_names[position]) % stride == 0;
        let oversized = analysis.offset(candidate).saturating_sub(slice_start) >= cap;
        if !chosen && !oversized {
            continue;
        }
        cuts.push(candidate);
        slice_start = analysis.offset(candidate);
    }
    cuts
}

/// Returns the residue-class stride for `candidates` boundaries and `jobs` slices.
///
/// A power of two so that a small change in the candidate count leaves it alone;
/// at least 1, and never so large that a typical file yields no cut at all.
fn stable_stride(candidates: usize, jobs: usize) -> u64 {
    let ideal = (candidates / jobs).max(1);
    // The power of two nearest `ideal` from below, which keeps the expected slice
    // count at or above `jobs` rather than under it.
    let stride = 1u64 << (usize::BITS - 1 - ideal.leading_zeros()).min(31);
    stride.max(1)
}

/// Picks up to `jobs - 1` cut lines, each as close as the legal boundaries allow
/// to an equal split of the body's bytes.
fn choose_cuts(analysis: &Analysis<'_>, jobs: usize) -> Vec<usize> {
    let total = analysis.total();
    if total == 0 || analysis.candidates.is_empty() {
        return Vec::new();
    }
    let mut cuts: Vec<usize> = Vec::new();
    for part in 1..jobs {
        let ideal = total * part / jobs;
        let mut best: Option<usize> = None;
        for &candidate in &analysis.candidates {
            if cuts.last().is_some_and(|&last| candidate <= last) {
                continue;
            }
            let distance = analysis.offset(candidate).abs_diff(ideal);
            let keep = best.is_some_and(|current| {
                analysis.offset(current).abs_diff(ideal) <= distance
            });
            if !keep {
                best = Some(candidate);
            }
        }
        if let Some(candidate) = best {
            cuts.push(candidate);
        }
    }
    cuts
}

/// Returns the section directive in effect at `line`.
fn section_in_effect<'a>(sections: &[(usize, &'a str)], line: usize) -> &'a str {
    match sections.binary_search_by(|probe| probe.0.cmp(&line)) {
        Ok(position) => sections[position].1,
        Err(0) => ".text",
        Err(position) => sections[position - 1].1,
    }
}

/// [`push_renamed`] for compact slices: a promoted temporary takes its promoted name, any other
/// temporary this file defines becomes `<prefix>z<index>`. The emitter only ever mints temporaries
/// as `<prefix>_...`, so a `<prefix>z` name cannot meet one of its own.
fn push_compacted(
    out: &mut String,
    line: &str,
    promoted: &FastMap<&str, String>,
    local_prefix: &str,
    ids: &FastMap<&str, usize>,
) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' {
            let start = index;
            index += 1;
            let mut escaped = false;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if byte == b'\n' || (!escaped && byte == b'"') {
                    break;
                }
                escaped = !escaped && byte == b'\\';
            }
            out.push_str(&line[start..index]);
        } else if is_identifier_start(byte) {
            let start = index;
            while index < bytes.len() && is_identifier_byte(bytes[index]) {
                index += 1;
            }
            let token = &line[start..index];
            if !token.starts_with(local_prefix) {
                out.push_str(token);
            } else if let Some(renamed) = promoted.get(token) {
                out.push_str(renamed);
            } else if let Some(&id) = ids.get(token) {
                use std::fmt::Write as _;
                let _ = write!(out, "{local_prefix}z{id:x}");
            } else {
                out.push_str(token);
            }
        } else {
            let start = index;
            while index < bytes.len() && bytes[index] != b'"' && !is_identifier_start(bytes[index]) {
                index += 1;
            }
            out.push_str(&line[start..index]);
        }
    }
}

/// Copies one line, replacing every whole-token occurrence of a promoted name.
///
/// Quoted runs are copied verbatim for the same reason `localize_internal_labels`
/// copies them: a label name that also appears inside a user string constant must
/// not be rewritten there.
fn push_renamed(
    out: &mut String,
    line: &str,
    promoted: &FastMap<&str, String>,
    local_prefix: &str,
) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' {
            let start = index;
            index += 1;
            let mut escaped = false;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if byte == b'\n' || (!escaped && byte == b'"') {
                    break;
                }
                escaped = !escaped && byte == b'\\';
            }
            out.push_str(&line[start..index]);
        } else if is_identifier_start(byte) {
            let start = index;
            while index < bytes.len() && is_identifier_byte(bytes[index]) {
                index += 1;
            }
            let token = &line[start..index];
            // Only local labels are ever promoted, so no other token needs a lookup.
            let renamed = if token.starts_with(local_prefix) { promoted.get(token) } else { None };
            match renamed {
                Some(renamed) => out.push_str(renamed),
                None => out.push_str(token),
            }
        } else {
            let start = index;
            while index < bytes.len() && bytes[index] != b'"' && !is_identifier_start(bytes[index]) {
                index += 1;
            }
            out.push_str(&line[start..index]);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the contiguous assembly splitter.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - `split_bench` times the split and the slice rendering over a real listing:
    //!   `ELEPHC_SPLIT_BENCH=<file.s> cargo test --release --lib split_bench -- --ignored --nocapture`.
    //! - The fixture reproduces the four shapes the real emitter produces that a
    //!   split can break: a conditional branch to an intra-function label, a plain
    //!   top-level local referenced from another function, an `L`-prefixed function
    //!   whose address a `.data` record takes, and a data tail behind a `.data`
    //!   directive that only the file prologue establishes.

    use super::*;

    /// The Mach-O assembler-local prefix, the one the splitter is written for.
    const MACOS_LOCAL: &str = "L";

    // TEMPORARY PROBE (remove before commit): real cut analysis over ELEPHC_PROBE_ASM.
    #[test]
    #[ignore]
    fn probe_region_cuts() {
        let path = std::env::var("ELEPHC_PROBE_ASM").unwrap();
        let source = std::fs::read_to_string(path).unwrap();
        let analysis = analyze(&source, MACOS_LOCAL);
        let hard_cov: usize = 0;
        eprintln!("lines {} candidates {} hard_cov {}", analysis.lines.len(), analysis.candidates.len(), hard_cov);
        let mut gaps: Vec<(usize, usize)> = Vec::new();
        let mut prev = 0usize;
        for &c in analysis.candidates.iter().chain(std::iter::once(&analysis.lines.len())) {
            gaps.push((c - prev, prev));
            prev = c;
        }
        gaps.sort_unstable_by(|a, b| b.cmp(a));
        for (gap, at) in gaps.iter().take(5) {
            eprintln!("gap {gap} lines from {at}: {}", analysis.lines.get(*at).trim());
        }
        let cuts = choose_cuts(&analysis, 18);
        eprintln!("cuts {:?}", cuts);
    }

    /// Returns every identifier token on a line, for the scanner tests.
    fn identifiers(line: &str) -> Vec<&str> {
        let mut out = Vec::new();
        visit_identifiers(line, |token| out.push(token));
        out
    }

    /// A hand-written fixture shaped like real emitter output.
    ///
    /// Every shape here is one a split can break, and each is taken from the real
    /// 8.8 MB assembly this module was checked against:
    /// * `_second` branches conditionally to `L_second_retry`, so the two may never
    ///   land in different slices;
    /// * `_second_tail` is a plain, non-`.globl`, non-footer label INSIDE another
    ///   atom that `_fourth` branches to — the shape that has to be published;
    /// * `L_thunk` is an assembler temporary whose only reference is a `.quad` in
    ///   `.data` — the shape that forces a rename, because `.globl` on an
    ///   `L`-prefixed name is an assembler error;
    /// * the `.data` tail exercises the section replay and the eight-byte alignment.
    ///
    /// The visibility footer names exactly the `.globl` symbols outside the export
    /// allowlist, which is what `append_visibility_directives` produces
    /// (src/codegen_support/visibility.rs:65-78) — `_main` is the only export.
    fn fixture() -> String {
        let mut asm = String::new();
        asm.push_str(".globl _main\n_main:\n    bl _first\n    ret\n");
        asm.push_str(".align 2\n.globl _first\n_first:\n    mov x0, #1\n    ret\n");
        asm.push_str(".align 2\n.globl _second\n_second:\n    cmp x0, #0\n");
        asm.push_str("    b.eq L_second_retry\n    ret\n");
        asm.push_str("L_second_retry:\n    mov x0, #2\n_second_tail:\n    ret\n");
        asm.push_str(".align 2\n.globl _third\n_third:\n    bl _first\n    bl _second\n    ret\n");
        asm.push_str(".align 2\nL_thunk:\n    mov x0, #3\n    ret\n");
        asm.push_str(".align 2\n.globl _fourth\n_fourth:\n    bl _third\n    b _second_tail\n");
        asm.push_str(".data\n.p2align 3\n.globl _data_0\n_data_0:\n");
        asm.push_str("    .quad L_thunk\n    .quad _first\n");
        asm.push_str(".globl _data_1\n_data_1:\n    .quad 0x0000000000000001\n");
        asm.push('\n');
        asm.push_str(VISIBILITY_FOOTER_MARKER);
        asm.push('\n');
        for name in ["_first", "_second", "_third", "_fourth", "_data_0", "_data_1"] {
            asm.push_str(".private_extern ");
            asm.push_str(name);
            asm.push('\n');
        }
        asm
    }

    /// One job must return the input byte for byte: that is the whole identity gate.
    #[test]
    fn one_job_returns_the_input_unchanged() {
        let source = fixture();
        for jobs in [0, 1] {
            let outcome = split_assembly(&source, jobs, MACOS_LOCAL);
            assert_eq!(outcome.slices.len(), 1);
            assert_eq!(outcome.slices[0], source);
            assert!(outcome.promoted.is_empty());
            assert!(!outcome.is_split());
        }
    }

    /// See the module note: times `plan_split` and the slice rendering over a real listing.
    #[test]
    #[ignore]
    fn split_bench() {
        let Ok(path) = std::env::var("ELEPHC_SPLIT_BENCH") else {
            return;
        };
        let text = std::fs::read_to_string(path).expect("listing");
        for _ in 0..2 {
            let started = std::time::Instant::now();
            let plan = plan_split(&text, 6, "L", std::env::var_os("ELEPHC_SPLIT_BENCH_RAW").is_none()).expect("a legal cut");
            let planned = started.elapsed();
            let mut bytes = 0usize;
            for part in 0..plan.slices() {
                plan.render_slice(part, &mut |chunk| {
                    bytes += chunk.len();
                    Ok(())
                })
                .expect("render");
            }
            eprintln!("plan {:.2}s, render {:.2}s, {} slices, {} bytes", planned.as_secs_f64(),
                (started.elapsed() - planned).as_secs_f64(), plan.slices(), bytes);
        }
    }

    /// Concatenating the sliced line ranges must reproduce the body of the input.
    ///
    /// Everything a slice adds is a prologue (section state, alignment) or an
    /// epilogue (its own visibility footer); nothing in between may be dropped,
    /// duplicated, or reordered, because a `__data` record's meaning depends on
    /// its neighbours.
    #[test]
    fn slices_cover_the_body_in_order_without_gaps() {
        let source = fixture();
        let analysis = analyze(&source, MACOS_LOCAL);
        let cuts = choose_cuts(&analysis, 4);
        assert!(!cuts.is_empty(), "the fixture must offer at least one legal cut");
        let mut rebuilt = String::new();
        for part in 0..=cuts.len() {
            let start = if part == 0 { 0 } else { cuts[part - 1] };
            let end = cuts.get(part).copied().unwrap_or(analysis.lines.len());
            for line in analysis.lines.range(start..end) {
                rebuilt.push_str(line);
            }
        }
        let all = Lines::new(&source);
        let body: String = all.range(0..footer_start(&all)).collect();
        assert_eq!(rebuilt, body);
    }

    /// No conditional branch and no numeric local reach may cross a slice
    /// boundary: `as` rejects the first and silently rebinds the second.
    #[test]
    fn no_hard_interval_crosses_a_slice() {
        let source = fixture();
        let analysis = analyze(&source, MACOS_LOCAL);
        let cuts = choose_cuts(&analysis, 4);
        let mut checked = 0usize;
        for (index, raw) in analysis.lines.iter().enumerate() {
            let Some(target) = conditional_branch_target(ascii_trim(raw)) else {
                continue;
            };
            let Some(&id) = analysis.ids.get(target) else {
                continue;
            };
            let def = analysis.spans[id].def;
            let (lo, hi) = (index.min(def), index.max(def));
            checked += 1;
            assert!(
                !cuts.iter().any(|&cut| lo < cut && cut <= hi),
                "conditional branch on line {index} to {target} (line {def}) crosses {cuts:?}"
            );
        }
        assert!(checked > 0, "the fixture must contain a conditional branch");
    }

    /// A local referenced from another slice must be published, and an assembler
    /// temporary must be renamed everywhere it appears rather than published
    /// under its temporary name.
    #[test]
    fn cross_slice_locals_are_published_and_temporaries_renamed() {
        let source = fixture();
        let outcome = split_assembly(&source, 4, MACOS_LOCAL);
        assert!(outcome.is_split(), "the fixture must split at four jobs");
        let joined = outcome.slices.concat();

        // `L_thunk` is reached only from the `.data` tail, so any cut between the
        // two forces the rename. `.globl L_thunk` is not an alternative: the
        // assembler rejects it ("non-local symbol required").
        let renamed = format!("{PROMOTED_PREFIX}L_thunk");
        assert!(
            outcome.promoted.iter().any(|name| name == &renamed),
            "the cross-section temporary was not promoted: {:?}",
            outcome.promoted
        );
        assert!(
            !joined.contains("\nL_thunk:"),
            "a promoted temporary kept its temporary definition:\n{joined}"
        );
        assert!(
            joined.contains(&format!(".quad {renamed}")),
            "a promoted temporary kept a temporary reference:\n{joined}"
        );
        assert!(
            joined.contains(&format!(".private_extern {renamed}")),
            "a promoted temporary was not published:\n{joined}"
        );
        // `_second_tail` is a plain local inside another atom that `_fourth`
        // branches to, so it must be published from the slice that defines it.
        assert!(
            outcome.published >= 1,
            "a plain local crossing a slice was not published"
        );
        assert!(
            outcome
                .slices
                .iter()
                .any(|slice| slice.contains(".private_extern _second_tail\n")),
            "the cross-slice plain local was not published:\n{joined}"
        );

        let analysis = analyze(&source, MACOS_LOCAL);
        let cuts = choose_cuts(&analysis, 4);
        let slice_of = |line: usize| cuts.partition_point(|&cut| cut <= line);
        for (id, name) in analysis.names.iter().enumerate() {
            let span = analysis.spans[id];
            if slice_of(span.lo) == slice_of(span.hi) || name.starts_with(MACOS_LOCAL) {
                continue;
            }
            assert!(
                outcome.slices[slice_of(span.def)].contains(&format!(".private_extern {name}\n"))
                    || *name == "_main",
                "{name} crosses a slice but is not published from slice {}",
                slice_of(span.def)
            );
        }
    }

    /// Every slice after the first must restate its section and its alignment.
    #[test]
    fn every_later_slice_restates_section_and_alignment() {
        let source = fixture();
        let outcome = split_assembly(&source, 4, MACOS_LOCAL);
        assert!(outcome.is_split());
        for (index, slice) in outcome.slices.iter().enumerate().skip(1) {
            let mut lines = slice.lines();
            let section = lines.next().unwrap_or_default();
            assert!(
                is_section_directive(section),
                "slice {index} does not open with a section directive:\n{slice}"
            );
            let alignment = lines.next().unwrap_or_default();
            assert!(
                alignment.starts_with(".p2align") || alignment.starts_with(".align"),
                "slice {index} does not restate alignment:\n{slice}"
            );
        }
        // A slice that opens in `.data` must restate eight-byte alignment, or a
        // `.quad` record whose own `.p2align` stayed behind lands misaligned.
        for slice in outcome.slices.iter().skip(1) {
            if slice.starts_with(".data\n") {
                assert!(
                    slice.starts_with(".data\n.p2align 3\n") || slice.starts_with(".data\n.align"),
                    "a data slice did not restate eight-byte alignment:\n{slice}"
                );
            }
        }
    }

    /// The file-global footer must be partitioned, never copied: a
    /// `.private_extern` naming a symbol the slice does not define is a private
    /// external reference with no definition in that object.
    #[test]
    fn the_visibility_footer_is_partitioned_not_copied() {
        let source = fixture();
        let outcome = split_assembly(&source, 4, MACOS_LOCAL);
        assert!(outcome.is_split());
        let analysis = analyze(&source, MACOS_LOCAL);
        let cuts = choose_cuts(&analysis, 4);
        let slice_of = |line: usize| cuts.partition_point(|&cut| cut <= line);
        let mut seen = 0usize;
        for (index, slice) in outcome.slices.iter().enumerate() {
            for line in slice.lines() {
                let Some(name) = line.trim().strip_prefix(".private_extern ") else {
                    continue;
                };
                let name = name.trim();
                let original = name.strip_prefix(PROMOTED_PREFIX).unwrap_or(name);
                let id = *analysis
                    .ids
                    .get(original)
                    .unwrap_or_else(|| panic!("slice {index} published unknown symbol {name}"));
                assert_eq!(
                    slice_of(analysis.spans[id].def),
                    index,
                    "slice {index} published {name}, which it does not define"
                );
                seen += 1;
            }
        }
        assert!(seen >= analysis.footer.len(), "the footer lost entries: {seen}");
        assert!(
            !outcome
                .slices
                .iter()
                .any(|slice| slice.contains(".private_extern _main")),
            "the entry point must not be published private"
        );
    }

    /// A cut may only land where control cannot fall through into the next slice.
    #[test]
    fn cuts_land_only_where_control_cannot_fall_through() {
        let source = fixture();
        let analysis = analyze(&source, MACOS_LOCAL);
        assert!(!analysis.candidates.is_empty());
        for &candidate in &analysis.candidates {
            let mut cursor = candidate;
            while cursor > 0 {
                cursor -= 1;
                let line = analysis.lines[cursor].trim();
                if line.is_empty() || introduces_a_label(line) {
                    continue;
                }
                let mnemonic = line.split_whitespace().next().unwrap_or("");
                assert!(
                    matches!(mnemonic, "ret" | "b" | "br" | "brk") || line.starts_with('.'),
                    "candidate {candidate} follows a fall-through line: {line}"
                );
                break;
            }
        }
    }

    /// The tokenizer must not read a symbol out of a quoted assembly string.
    #[test]
    fn quoted_strings_hide_their_contents_from_the_scanner() {
        let line = "    .ascii \"_first b.eq L_second_retry\"\n";
        assert_eq!(identifiers(line), vec![".ascii"]);
        let mut promoted: FastMap<&str, String> = FastMap::default();
        promoted.insert("L_second_retry", format!("{PROMOTED_PREFIX}L_second_retry"));
        let mut out = String::new();
        push_renamed(&mut out, line, &promoted, "L");
        assert_eq!(out, line, "a quoted string must survive a rename verbatim");
    }

    /// Renaming is whole-token: a longer identifier that merely starts with a
    /// promoted name must not be rewritten.
    #[test]
    fn renaming_is_whole_token() {
        let mut promoted: FastMap<&str, String> = FastMap::default();
        promoted.insert("L_thunk", format!("{PROMOTED_PREFIX}L_thunk"));
        let mut out = String::new();
        push_renamed(&mut out, "    .quad L_thunk_2\n", &promoted, "L");
        assert_eq!(out, "    .quad L_thunk_2\n");
        out.clear();
        push_renamed(&mut out, "    .quad L_thunk@PAGE\n", &promoted, "L");
        assert_eq!(out, format!("    .quad {PROMOTED_PREFIX}L_thunk@PAGE\n"));
    }

    /// A numeric local's `1f`/`1b` reach binds it to its definition inside one slice.
    #[test]
    fn numeric_local_references_are_recognised() {
        assert_eq!(numeric_references("    b.eq 1f\n"), vec![("1", b'f')]);
        assert_eq!(numeric_references("    b 12b\n"), vec![("12", b'b')]);
        assert!(numeric_references("    mov x0, #0x1f\n").is_empty());
        assert!(numeric_references("    ldr x0, [sp, #16]\n").is_empty());
    }

    /// Non-macOS targets and library artifacts keep today's single-`as` path.
    #[test]
    fn only_macos_aarch64_executables_are_split() {
        let macos = Target::new(Platform::MacOS, Arch::AArch64);
        assert!(supports_split(macos, Emit::Executable));
        assert!(!supports_split(macos, Emit::Cdylib));
        assert!(!supports_split(macos, Emit::Staticlib));
        assert!(!supports_split(
            Target::new(Platform::Linux, Arch::AArch64),
            Emit::Executable
        ));
        assert!(!supports_split(
            Target::new(Platform::Linux, Arch::X86_64),
            Emit::Executable
        ));
        assert!(!supports_split(
            Target::new(Platform::MacOS, Arch::X86_64),
            Emit::Executable
        ));
    }

    /// Builds an assembly of `count` top-level functions, one of which can be resized.
    ///
    /// Every function ends in `ret`, so every following label is a legal boundary --
    /// which is what makes this fixture able to say anything about cut POLICY rather
    /// than about which boundaries are legal.
    fn many_functions(count: usize, fat: Option<usize>) -> String {
        let mut asm = String::new();
        asm.push_str(".globl _main\n_main:\n    ret\n");
        for index in 0..count {
            asm.push_str(&format!(".align 2\n.globl _fn{index}\n_fn{index}:\n"));
            let body = if fat == Some(index) { 40 } else { 4 };
            for step in 0..body {
                asm.push_str(&format!("    mov x0, #{step}\n"));
            }
            asm.push_str("    ret\n");
        }
        asm
    }

    /// Returns the NAME of the symbol each cut opens, under one policy.
    ///
    /// Names rather than line indices: an edit shifts every later line, so comparing
    /// indices would report movement that is only renumbering.
    fn cut_names(source: &str, jobs: usize, policy: CutPolicy) -> Vec<String> {
        let analysis = analyze(source, MACOS_LOCAL);
        let cuts = match policy {
            CutPolicy::Balanced => choose_cuts(&analysis, jobs),
            CutPolicy::Stable => choose_cuts_stable(&analysis, jobs),
        };
        cuts.iter()
            .map(|cut| {
                let position = analysis
                    .candidates
                    .iter()
                    .position(|candidate| candidate == cut)
                    .expect("every cut is a candidate");
                analysis.candidate_names[position].to_string()
            })
            .collect()
    }

    /// THE PROPERTY THE WHOLE POLICY EXISTS FOR: resizing one function must leave
    /// every other boundary exactly where it was.
    ///
    /// That is what lets the slice cache survive an edit — every slice but the one
    /// holding the edit renders the same bytes and so keeps its key.
    #[test]
    fn a_stable_cut_set_does_not_move_when_one_function_changes_size() {
        let before = cut_names(&many_functions(64, None), 8, CutPolicy::Stable);
        let after = cut_names(&many_functions(64, Some(30)), 8, CutPolicy::Stable);
        assert!(!before.is_empty(), "the fixture must produce cuts to compare");
        assert_eq!(
            before, after,
            "resizing _fn30 must move no boundary but the ones around it"
        );
    }

    /// Adding a function must not move the boundaries of the functions around it.
    #[test]
    fn a_stable_cut_set_survives_an_added_function() {
        let before = cut_names(&many_functions(64, None), 8, CutPolicy::Stable);
        let after = cut_names(&many_functions(65, None), 8, CutPolicy::Stable);
        let kept = before.iter().filter(|name| after.contains(name)).count();
        assert!(
            kept * 10 >= before.len() * 9,
            "adding one function should keep almost every boundary, kept {kept} of {}",
            before.len()
        );
    }

    /// The same edit under the BALANCED policy moves boundaries it never touched.
    ///
    /// This is the contrast that says why the stable policy exists: it is not that
    /// the balanced policy is wrong, it is that every one of its cuts is a function
    /// of the file's total size, so one edit invalidates every cached slice.
    #[test]
    fn the_balanced_policy_moves_boundaries_the_edit_never_touched() {
        let before = cut_names(&many_functions(64, None), 8, CutPolicy::Balanced);
        let after = cut_names(&many_functions(64, Some(30)), 8, CutPolicy::Balanced);
        assert_ne!(
            before, after,
            "if this ever becomes equal the balanced policy has changed and the \
             stable policy may no longer be needed"
        );
    }

    /// No slice may exceed the cap, whatever the names happen to hash to.
    #[test]
    fn no_stable_slice_exceeds_the_size_cap() {
        let source = many_functions(64, Some(10));
        let analysis = analyze(&source, MACOS_LOCAL);
        let jobs = 8;
        let cuts = choose_cuts_stable(&analysis, jobs);
        let total = analysis.total();
        let cap = (total / jobs).saturating_mul(2).max(1);
        let mut start = 0usize;
        for &cut in &cuts {
            let size = analysis.offset(cut) - start;
            assert!(size <= cap, "slice of {size} bytes exceeds the cap of {cap}");
            start = analysis.offset(cut);
        }
    }

    /// The stride must be a power of two, so a small edit cannot change it.
    ///
    /// An exact `candidates / jobs` ratio would be a function of the candidate
    /// count, so adding one symbol would change the stride and therefore EVERY
    /// boundary -- exactly the instability this policy removes.
    #[test]
    fn the_stride_is_a_power_of_two_and_ignores_a_small_change_in_the_count() {
        for candidates in [64usize, 65, 70, 100, 4096] {
            let stride = stable_stride(candidates, 8);
            assert!(stride.is_power_of_two(), "stride {stride} is not a power of two");
        }
        // A handful of added symbols must not move the stride.
        assert_eq!(stable_stride(64, 8), stable_stride(70, 8));
        assert_eq!(stable_stride(1000, 8), stable_stride(1010, 8));
        // THE KNOWN LIMIT, pinned rather than hidden: the stride is constant only
        // BETWEEN powers of two. A program that grows across one -- here from 1000
        // to 1100 candidates, straddling 128 * 8 -- does change stride, and that one
        // build misses on every slice. Rounding to a power of two makes that rare
        // instead of impossible; only a stride fixed independently of the program
        // could make it impossible, and that would stop the slice count tracking
        // the program's size at all.
        assert_ne!(stable_stride(1000, 8), stable_stride(1100, 8));
        // A pathological count still yields a usable stride rather than zero.
        assert!(stable_stride(0, 8) >= 1);
        assert!(stable_stride(1, 8) >= 1);
    }

    /// The slice count must stay in a usable band, not collapse to one or explode.
    #[test]
    fn the_stride_bounds_the_slice_count() {
        let source = many_functions(256, None);
        let analysis = analyze(&source, MACOS_LOCAL);
        let jobs = 8;
        let cuts = choose_cuts_stable(&analysis, jobs);
        let slices = cuts.len() + 1;
        assert!(
            slices >= 2 && slices <= jobs * 4,
            "{slices} slices for {jobs} jobs is outside the usable band"
        );
    }

    /// Only `stable` opts in; a typo keeps today's cuts.
    #[test]
    fn only_stable_selects_the_content_defined_policy() {
        assert_eq!(cut_policy_from_env(None), CutPolicy::Balanced);
        assert_eq!(cut_policy_from_env(Some("stable")), CutPolicy::Stable);
        assert_eq!(cut_policy_from_env(Some("balanced")), CutPolicy::Balanced);
        assert_eq!(cut_policy_from_env(Some("Stable")), CutPolicy::Balanced);
        assert_eq!(cut_policy_from_env(Some("1")), CutPolicy::Balanced);
        assert_eq!(cut_policy_from_env(Some("")), CutPolicy::Balanced);
    }

    /// A boundary's name must be the label it opens, or the hash means nothing.
    #[test]
    fn every_candidate_carries_the_name_it_opens() {
        let source = many_functions(8, None);
        let analysis = analyze(&source, MACOS_LOCAL);
        assert_eq!(analysis.candidates.len(), analysis.candidate_names.len());
        for (position, &candidate) in analysis.candidates.iter().enumerate() {
            let name = analysis.candidate_names[position];
            let opens = analysis
                .lines
                .range(candidate..analysis.lines.len())
                .any(|line| line.trim() == format!("{name}:"));
            assert!(opens, "candidate at line {candidate} does not open {name}");
        }
    }

    /// An address-taken temporary defined deep in a slice too large for a relocation addend is
    /// promoted to a real symbol; a branch-only temporary in the same slice is not.
    ///
    /// ARM64_RELOC_ADDEND holds 24 bits, so `adrp x1, L_x@PAGE` to a temporary more than 8 MB into
    /// its section failed to assemble ("addend too big for relocation") -- the Symfony demo's
    /// preload build, whose 1.7 GB slice no cut can divide.
    #[test]
    fn address_taken_temporaries_in_an_oversized_slice_are_promoted() {
        let mut asm = String::new();
        asm.push_str(".globl _main\n_main:\n    ret\n");
        asm.push_str(".align 2\n.globl _big\n_big:\n    cmp x0, #0\n    b.eq L_big_end\n");
        for step in 0..500_000 {
            asm.push_str(&format!("    mov x0, #{}\n", step % 4096));
        }
        asm.push_str("L_loop:\n    adrp x1, L_helper@PAGE\n    add x1, x1, L_helper@PAGEOFF\n");
        asm.push_str("    b L_loop\nL_big_end:\n    ret\n");
        asm.push_str(".align 2\nL_helper:\n    mov x0, #7\n    ret\n");
        for index in 0..8 {
            asm.push_str(&format!(".align 2\n.globl _fn{index}\n_fn{index}:\n    ret\n"));
        }
        let outcome = split_assembly(&asm, 4, MACOS_LOCAL);
        assert!(outcome.is_split(), "the fixture must split");
        assert!(
            outcome.promoted.iter().any(|name| name == &format!("{PROMOTED_PREFIX}L_helper")),
            "the address-taken temporary was not promoted: {:?}",
            outcome.promoted
        );
        assert!(
            !outcome.promoted.iter().any(|name| name.ends_with("L_loop")),
            "a branch-only temporary was promoted: {:?}",
            outcome.promoted
        );
    }
    /// The in-tree assembler fed a slice's borrowed parts builds, byte for byte, the object it
    /// builds from the rendered slice -- promoted temporaries renamed in definitions, branches,
    /// `.quad` records and the footer alike.
    #[test]
    fn assembler_parts_build_the_object_the_rendered_slice_builds() {
        use super::super::inhouse::{assemble, assemble_parts, Names};
        // `_fourth` also branches to `L_thunk`, so a promoted temporary is the target of an
        // instruction fixup, not only of a `.quad`.
        let source = fixture().replace("    b _second_tail\n", "    bl L_thunk\n    b _second_tail\n");
        let plan = plan_split(&source, 4, MACOS_LOCAL, false).expect("the fixture must split");
        assert!(!plan.promoted().is_empty(), "the fixture must promote a temporary");
        let names = Names::promoted(plan.promoted(), plan.local_prefix());
        for part in 0..plan.slices() {
            let mut rendered = String::new();
            plan.render_slice(part, &mut |chunk| {
                rendered.push_str(chunk);
                Ok(())
            })
            .expect("render");
            let (prologue, body, footer) = plan.assembler_parts(part);
            let direct = assemble_parts(&[prologue.as_str(), body, footer.as_str()], names)
                .unwrap_or_else(|error| panic!("slice {part}: {error:?}"));
            let expected = assemble(&rendered).unwrap_or_else(|error| panic!("slice {part}: {error:?}"));
            assert!(direct == expected, "slice {part} differs from its rendered object:\n{rendered}");
        }
    }

    /// Compact slices drop comments and indentation and rename every assembler temporary to the
    /// same short name in every slice, while promoted temporaries keep their exported names.
    #[test]
    fn compact_slices_rename_temporaries_consistently_and_drop_comments() {
        let mut source = String::from("    ; a comment line\n");
        source.push_str(&fixture());
        let mut plan = plan_split(&source, 4, MACOS_LOCAL, false).expect("the fixture splits");
        plan.compact(";");
        let mut slices = Vec::new();
        for part in 0..plan.slices() {
            let mut out = String::new();
            plan.render_slice(part, &mut |text| {
                out.push_str(text);
                Ok(())
            })
            .expect("render");
            slices.push(out);
        }
        let all = slices.concat();
        assert!(!all.contains("; a comment"), "{all}");
        assert!(!all.contains("L_second_retry") || plan.promoted.contains_key("L_second_retry"), "{all}");
        assert!(all.lines().all(|line| !line.starts_with("    ")), "{all}");
        // Every temporary still has exactly one definition, and every reference names a definition.
        let defined: Vec<&str> = all
            .lines()
            .filter_map(|line| line.strip_suffix(':'))
            .filter(|name| name.starts_with('L'))
            .collect();
        let unique: std::collections::HashSet<&str> = defined.iter().copied().collect();
        assert_eq!(defined.len(), unique.len(), "{all}");
        for line in all.lines() {
            for token in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                if token.starts_with("Lz") {
                    assert!(unique.contains(token), "dangling {token} in {all}");
                }
            }
        }
    }

    /// Resolving a numeric reference is a bisection: the comparisons per lookup stay logarithmic
    /// in the number of definitions of that label (a linear scan made the whole pass quadratic),
    /// and the answer is the same as a linear scan's.
    #[test]
    fn numeric_reference_lookup_is_logarithmic_and_agrees_with_a_scan() {
        let definitions: Vec<usize> = (0..50_000).map(|n| n * 3 + 1).collect();
        let lookups: Vec<usize> = (0..2_000).map(|n| n * 71).collect();
        let mut probes = 0usize;
        for &index in &lookups {
            let forward = nearest_definition(&definitions, index, true, || probes += 1);
            assert_eq!(forward, definitions.iter().copied().find(|&line| line >= index));
            let backward = nearest_definition(&definitions, index, false, || probes += 1);
            assert_eq!(backward, definitions.iter().rev().copied().find(|&line| line <= index));
        }
        // ceil(log2(50_000)) = 16 comparisons per lookup, 2 lookups per index.
        let bound = lookups.len() * 2 * 17;
        assert!(probes <= bound, "{probes} comparisons for {} lookups", lookups.len() * 2);
        assert_eq!(nearest_definition(&[], 5, true, || {}), None);
        assert_eq!(nearest_definition(&[10], 5, false, || {}), None);
        assert_eq!(nearest_definition(&[10], 10, false, || {}), Some(10));
    }

    /// A body full of `1:`/`1f` pairs keeps each pair inside one slice and still splits.
    #[test]
    fn numeric_local_pairs_stay_in_one_slice_when_there_are_many() {
        let mut source = String::from(".text\n");
        for n in 0..400 {
            source.push_str(&format!(
                ".globl _fn_{n}\n_fn_{n}:\n    cbnz x0, 1f\n    mov x0, #{n}\n1:\n    ret\n"
            ));
        }
        let outcome = split_assembly(&source, 4, MACOS_LOCAL);
        assert!(outcome.is_split());
        for slice in &outcome.slices {
            let branches = slice.matches("cbnz x0, 1f").count();
            let targets = slice.lines().filter(|line| line.trim() == "1:").count();
            assert_eq!(branches, targets, "a `1f` was separated from its `1:`");
        }
    }

    /// The parallel line index and candidate scan produce exactly what one pass over the whole
    /// range does, seams included, and a source past the threshold really is split in chunks.
    #[test]
    fn parallel_index_and_candidates_equal_a_single_pass() {
        let mut source = String::from(".text\n");
        for n in 0..300 {
            source.push_str(&format!(
                ".globl _fn_{n}\n_fn_{n}:\n    cbnz x0, 1f\n    mov x0, #{n}\n1:\n    ret\n"
            ));
        }
        assert!(source.len() >= PARALLEL_INDEX_BYTES);
        if worker_threads() > 1 {
            assert!(index_chunks(source.len()) > 1);
            assert_eq!(index_chunks(PARALLEL_INDEX_BYTES - 1), 1);
        }
        let lines = Lines::new(&source);
        let naive: Vec<usize> = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        assert_eq!(lines.len(), naive.len() - usize::from(source.ends_with('\n')));
        for index in 0..lines.len() {
            assert_eq!(lines.offset(index), naive[index]);
        }
        let (parallel, parallel_names) = boundary_candidates(&lines, MACOS_LOCAL, &[]);
        let (single, single_names) = candidates_in(&lines, MACOS_LOCAL, &[], 0..lines.len());
        assert_eq!(parallel, single);
        assert_eq!(parallel_names, single_names);
        assert!(parallel.len() > 100);
    }
}
