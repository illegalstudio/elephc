//! Purpose:
//! Owns the assembly text builder and target-aware syntax helpers used by all emitters.
//! Centralizes labels, directives, relocation forms, comments, and raw text output.
//!
//! Called from:
//! - `crate::codegen` and all shared `crate::codegen_support::*` emitters.
//!
//! Key details:
//! - Instruction comments are emitted by callers; this module preserves target syntax and output ordering.

// The house hasher: `internal_labels` takes one insert per emitted internal label and is then
// probed by `localize_internal_labels` for EVERY identifier token in the finished listing --
// 1.4 GB of it on the Symfony build, where a sample caught `sip::Hasher::write`. It is only
// inserted into, taken and probed, never iterated, so the hasher cannot change any output.
use crate::fast_hash::FastSet as HashSet;
use std::fmt::Write;

use super::platform::{Arch, Platform, Target};

/// Replaces every occurrence of `label` in `text` that names `label` itself or a label spelled
/// from it (`label` followed by `_` and a non-digit, or by a non-identifier byte). An occurrence
/// followed by a digit, or by `_` and a digit, is another counter-numbered symbol that merely
/// starts with the same text and is left alone.
fn replace_label_tokens(text: &str, label: &str, with: &str) -> String {
    let positions = label_token_positions(text, label);
    let mut out = String::with_capacity(text.len());
    for_each_label_piece(text, label.len(), &positions, with, |piece| out.push_str(piece));
    out
}

/// Whether `ELEPHC_INVOKER_MEMO_VERIFY` asks every memoized repeat to be lowered and compared.
/// `text` without the keyed helper regions nested in it (`marker_open` … `marker_close`).
fn without_keyed_regions(text: &str, marker_open: &str, marker_close: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for line in text.lines() {
        if line.contains(marker_open) {
            depth += 1;
            continue;
        }
        if depth > 0 {
            if line.contains(marker_close) {
                depth -= 1;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

thread_local! {
    /// Under `ELEPHC_INVOKER_MEMO_VERIFY`, the label-blanked text of each memoized body, to show
    /// where a repeat differs.
    static MEMO_VERIFY_TEXTS: std::cell::RefCell<std::collections::HashMap<String, String>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn memo_verify() -> bool {
    static VERIFY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *VERIFY.get_or_init(|| std::env::var_os("ELEPHC_INVOKER_MEMO_VERIFY").is_some())
}

/// Where `replace_label_tokens` replaces `label` in `text`, found with one `memmem` pass: a
/// shared body is scanned for its label twice (to hash it, then to emit it), and the `str`
/// searcher this replaced was a top codegen frame on Symfony.
fn label_token_positions(text: &str, label: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut positions = Vec::new();
    let mut copied = 0;
    for at in memchr::memmem::find_iter(bytes, label.as_bytes()) {
        if at < copied {
            continue;
        }
        let after = at + label.len();
        let keep = match bytes.get(after) {
            Some(b) if b.is_ascii_digit() => true,
            Some(b'_') => bytes.get(after + 1).is_some_and(u8::is_ascii_digit),
            _ => false,
        };
        if keep {
            continue;
        }
        positions.push(at);
        copied = after;
    }
    positions
}

/// Hands `text` to `piece` with the label at each of `positions` replaced by `with`, in order,
/// so a caller can hash or copy the replaced text without building it first.
fn for_each_label_piece(
    text: &str,
    label_len: usize,
    positions: &[usize],
    with: &str,
    mut piece: impl FnMut(&str),
) {
    let mut copied = 0;
    for &at in positions {
        piece(&text[copied..at]);
        piece(with);
        copied = at + label_len;
    }
    piece(&text[copied..]);
}

/// Assembly emitter.
pub struct Emitter {
    buf: String,
    pub target: Target,
    pub platform: Platform,
    /// When `true`, the `emit_*_symbol_*` helpers in `codegen::abi::symbols`
    /// route global-symbol references through the GOT (`@GOTPCREL` on x86_64,
    /// `:got:` + `:got_lo12:` on AArch64) instead of using direct PC-relative
    /// addressing. Required for shared-library output, where the loader cannot
    /// resolve cross-object `R_X86_64_PC32` relocations at dlopen time.
    pub pic_data_refs: bool,
    /// Capacity of the shared `_concat_buf` result arena in bytes. The default
    /// matches the historical 64 KiB scratch buffer; runtime generation raises
    /// it to the program's configured heap budget so string producers can use
    /// the same memory ceiling as PHP instead of an artificial 64 KiB cap.
    pub concat_capacity: usize,
    /// When `true`, user frames publish exceptional cleanup activations and
    /// process-fatal runtime paths may unwind through the cdylib host boundary.
    /// This is deliberately independent from position-independent addressing:
    /// PIC is a relocation choice, while boundary recovery is an ABI contract.
    pub cdylib_boundary: bool,
    /// Routes final output to a malloc-only producer callback, never to the host stdout fd.
    pub phpext_output: bool,
    /// When `true`, macOS runtime emission is prepared for per-symbol dead
    /// stripping: `label()` records each internal label name in
    /// `internal_labels` so the final assembly can rename them to Mach-O
    /// assembler-local (`L`-prefixed) labels. Under the `.subsections_via_symbols`
    /// footer that keeps each `__rt_*` helper a single atom (local labels never
    /// start an atom) while remaining valid conditional-branch targets on every
    /// toolchain, so the linker's `-dead_strip` drops whole unreferenced helpers.
    /// Only set for the macOS executable runtime object; Linux uses per-section
    /// `--gc-sections` and cdylibs never dead-strip.
    pub dead_strip: bool,
    /// The platform-mangled symbol of the hosted-extension shutdown routine, when this module
    /// hosts PHP extensions. Every process-exit sequence calls it after flushing output, which
    /// is where php-src runs request shutdown (RSHUTDOWN) and then module shutdown (MSHUTDOWN).
    pub exit_hook: Option<String>,
    /// AArch64 only: while a PHP function body is being emitted, `Some(k)` means `x28` holds
    /// `x29 - k` (the frame's lowest address), so a local slot `offset` bytes below `x29` is
    /// `[x28, #(k - offset)]`. `ldur`/`stur` reach only 255 bytes below `x29`; past that a slot
    /// access was an address computation plus the access, two or three instructions, and that
    /// pair was 15% of the compiled Symfony methods. `None` everywhere else: runtime routines,
    /// and the cleanup callbacks that borrow another frame's `x29`.
    pub frame_slot_base: Option<usize>,
    /// Names of internal (`label()`) labels recorded while `dead_strip` is set,
    /// used by `localize_internal_labels` to rewrite them `L`-prefixed.
    internal_labels: HashSet<String>,
    /// Whole functions emitted while another was being written, appended after every other
    /// line of text by [`Self::output`]. See [`Self::begin_out_of_line`].
    out_of_line: String,
    /// Name of the last `.text.<name>` section opened, so a caller that emits a
    /// global entry *inside* a function body can put the body back where it was.
    /// Only ELF splits text per symbol; Mach-O keeps one flat `__text`, so this
    /// stays `None` there and reopening is a no-op.
    current_text_section: Option<String>,
    /// Where completed text goes once the buffer grows past `SPILL_THRESHOLD`, when enabled.
    spill: Option<Spill>,
    /// Content-named bodies this emitter already holds (see [`Self::emit_content_shared`]).
    content_bodies: HashSet<String>,
    /// Where in `buf` each `content_bodies` entry was written, so `rollback_to` forgets a shared
    /// body together with its text. A parallel codegen worker rolls a deferred body back; keeping
    /// the entry made the next body of the chunk branch to a definition that was gone, and the
    /// Symfony app failed to link on `_eir_cbody_*` symbols.
    content_body_log: Vec<(usize, String)>,
    /// Log entries below this index belong to a buffer set aside by an open out-of-line scope,
    /// whose offsets index THAT buffer: a rollback in the scope's own buffer must not pop them.
    content_body_log_floor: usize,
    /// Shared body name per memo key (see [`Self::emit_content_shared_memo`]): a later request
    /// with the same key jumps to that body without lowering it again, while the body is still
    /// in `content_bodies`.
    content_memo: crate::fast_hash::FastMap<u64, String>,
    /// Keys of the shared bodies written inside out-of-line scopes, with the length of
    /// `out_of_line` before the scope's text was appended. `end_out_of_line` moves a scope's
    /// log entries here (their buffer offsets mean nothing once the scope is closed) so that
    /// [`Self::rollback_out_of_line_to`] can forget the keys together with the text.
    out_of_line_keys: Vec<(usize, String)>,
}

/// The caller's text, held aside while an out-of-line function is written.
///
/// Returned by [`Emitter::begin_out_of_line`] and consumed by [`Emitter::end_out_of_line`]. It
/// owns everything the caller had emitted so far, so dropping it without closing the scope would
/// lose that text rather than misplace it — hence `must_use`.
#[must_use = "an out-of-line function must be closed with `end_out_of_line`, or its caller's text is lost"]
pub struct OutOfLine {
    caller: String,
    /// The log floor in force before this scope opened.
    log_floor_before: usize,
    /// How many log entries existed when the scope opened; later ones were written in its buffer.
    log_len_at_begin: usize,
}

/// Buffer size past which `spill_completed_text` moves the text to disk.
const SPILL_THRESHOLD: usize = 32 << 20;

/// The on-disk prefix of an emitter's output.
struct Spill {
    path: std::path::PathBuf,
    writer: std::io::BufWriter<std::fs::File>,
    bytes: usize,
    error: Option<std::io::Error>,
}

/// Text an emitter moved to disk, in order, ahead of what it still holds in memory.
#[derive(Debug)]
pub struct SpilledText {
    /// The file holding the spilled prefix.
    pub path: std::path::PathBuf,
    /// Its length in bytes.
    pub len: usize,
}

/// Whether the assembly is only going to be assembled, never read: set by the pipeline for an
/// ordinary build, left off by `--emit-asm`, `--debug-info` and every in-process caller.
static COMPACT_ASM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Turns compact assembly on or off for the emissions that follow.
///
/// Compact assembly has no comment lines and names intra-function labels `L_<body hash>_<n>`
/// instead of spelling the function and block in them (`L_eir_Symfony_Component_Mime_M_..._9`):
/// on the Symfony `--web` build those names were 23% of the text and the comments 6%, all of it
/// held in memory by codegen, written, mapped, split and lexed by `as`. The object differs only
/// in assembler-local names, which never reach the symbol table.
pub fn set_compact_asm(on: bool) {
    COMPACT_ASM.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether compact assembly is on (see [`set_compact_asm`]).
pub fn compact_asm() -> bool {
    COMPACT_ASM.load(std::sync::atomic::Ordering::Relaxed)
}

impl Emitter {
    /// Creates an emitter for the specified target platform.
    pub fn new(target: Target) -> Self {
        Self {
            buf: String::with_capacity(4096),
            target,
            platform: target.platform,
            pic_data_refs: false,
            concat_capacity: 65_536,
            cdylib_boundary: false,
            phpext_output: false,
            dead_strip: false,
            exit_hook: None,
            frame_slot_base: None,
            internal_labels: HashSet::default(),
            out_of_line: String::new(),
            current_text_section: None,
            spill: None,
            content_bodies: HashSet::default(),
            content_body_log: Vec::new(),
            content_body_log_floor: 0,
            out_of_line_keys: Vec::new(),
            content_memo: Default::default(),
        }
    }

    /// Returns a new emitter configured for position-independent data
    /// references. Used by `--emit cdylib` so global symbol accesses survive
    /// dynamic loading as a shared object.
    pub fn new_pic(target: Target) -> Self {
        let mut emitter = Self::new(target);
        emitter.pic_data_refs = true;
        emitter
    }

    /// Returns an emitter configured for the recoverable cdylib ABI.
    pub fn new_cdylib(target: Target) -> Self {
        let mut emitter = Self::new_pic(target);
        emitter.cdylib_boundary = true;
        emitter
    }

    /// Returns a non-PIC emitter configured for the recoverable library ABI.
    ///
    /// Static archives are relocated once by the consuming application's linker,
    /// so they keep direct PC-relative data references while publishing the same
    /// fatal-recovery boundary as a dynamically loaded library.
    pub fn new_staticlib(target: Target) -> Self {
        let mut emitter = Self::new(target);
        emitter.cdylib_boundary = true;
        emitter
    }

    /// The length of the buffered text, to measure what one piece of lowering emitted.
    pub fn text_mark(&self) -> usize {
        self.buf.len()
    }

    /// The instruction lines emitted since `mark` (labels, comments and directives excluded).
    ///
    /// Returns 0 when the buffer was spilled to disk in between, which only happens between
    /// functions.
    pub fn instructions_since(&self, mark: usize) -> usize {
        let Some(text) = self.buf.get(mark..) else {
            return 0;
        };
        text.lines()
            .filter(|line| {
                line.starts_with("    ")
                    && !line.trim_start().is_empty()
                    && !line.trim_start().starts_with(['.', ';', '@', '#'])
            })
            .count()
    }

    /// Emits a single assembly instruction with standard indentation.
    pub fn instruction(&mut self, instr: &str) {
        // Plain appends: this runs for every one of the ~37 M instructions of a Symfony build,
        // and the formatting machinery `writeln!` goes through buys nothing for a fixed shape.
        self.buf.push_str("    ");
        self.buf.push_str(instr);
        self.buf.push('\n');
    }

    /// Emits a local label (name:).
    /// Under macOS per-symbol dead stripping (`dead_strip`), a named identifier
    /// label is recorded so the final assembly can rename it to a Mach-O
    /// assembler-local (`L`-prefixed) label: those never start an atom under
    /// `.subsections_via_symbols` (keeping each helper one strippable unit) yet
    /// stay valid conditional-branch targets on every toolchain — unlike
    /// `.alt_entry`, which older assemblers reject as "external" for conditional
    /// branches. Numeric (`1:`/`2:`) and already-`L` labels are assembler-local
    /// already, so they are left untouched.
    pub fn label(&mut self, name: &str) {
        if self.dead_strip
            && self.platform == Platform::MacOS
            && !name.starts_with('L')
            && !name.bytes().all(|b| b.is_ascii_digit())
        {
            self.internal_labels.insert(name.to_string());
        }
        let _ = writeln!(self.buf, "{}:", name);
    }

    /// Takes ownership of the recorded internal-label names, clearing the set.
    /// Called once after runtime emission to drive `localize_internal_labels`.
    pub fn take_internal_labels(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.internal_labels)
    }

    /// Emits a label for an internal helper that is reached from *another* helper
    /// via an unconditional `b`/`bl` (never a conditional branch). Under macOS
    /// dead stripping it is marked `.alt_entry`: it stays inside its defining
    /// helper's atom (so that helper is not split and its own conditional
    /// branches remain intra-atom) yet remains a real symbol, so the cross-helper
    /// `b`/`bl` keeps the atom alive under `-dead_strip`. Unlike `label()` it is
    /// NOT recorded for `L`-localization, so the bare name still resolves. Only
    /// valid for `b`/`bl` targets — older assemblers reject conditional branches
    /// to `.alt_entry` labels.
    pub fn label_shared(&mut self, name: &str) {
        if self.dead_strip && self.platform == Platform::MacOS {
            let _ = writeln!(self.buf, ".alt_entry {}", name);
        }
        let _ = writeln!(self.buf, "{}:", name);
    }

    /// Emit a label that is visible across object files (for two-object linking).
    /// On Linux, places each global symbol in its own `.text.<name>` section so
    /// that `--gc-sections` can eliminate unreachable helpers at link time.
    pub fn label_global(&mut self, name: &str) {
        // An assembler-local name cannot be `.globl` ("non-local symbol required"). Catch
        // it here with the offending label instead of an assembler error thousands of
        // lines into the stream — entry points must mint via `next_global_label()`.
        debug_assert!(
            !name.starts_with(self.platform.local_label_prefix()),
            "label_global on assembler-local name {name}"
        );
        if self.platform == Platform::Linux {
            let _ = writeln!(self.buf, ".section .text.{},\"ax\",@progbits", name);
            let _ = writeln!(self.buf, ".globl {}", name);
            let _ = writeln!(self.buf, ".type {}, %function", name);
            let _ = writeln!(self.buf, "{}:", name);
            self.current_text_section = Some(name.to_string());
        } else {
            let _ = writeln!(self.buf, ".globl {}", name);
            let _ = writeln!(self.buf, "{}:", name);
        }
    }

    /// The section a later [`Self::reopen_text_section`] should return to.
    pub fn current_text_section(&self) -> Option<String> {
        self.current_text_section.clone()
    }

    /// Reopens a section previously captured with [`Self::current_text_section`].
    ///
    /// Needed because [`Self::label_global`] opens a fresh `.text.<name>` on ELF: code
    /// emitted after an inline global thunk would otherwise continue inside *that*
    /// thunk's section, leaving the enclosing function's tail in another function's section.
    pub fn reopen_text_section(&mut self, section: Option<String>) {
        if self.platform != Platform::Linux {
            return;
        }
        let Some(name) = section else { return };
        if self.current_text_section.as_deref() == Some(name.as_str()) {
            return;
        }
        let _ = writeln!(self.buf, ".section .text.{},\"ax\",@progbits", name);
        self.current_text_section = Some(name);
    }

    /// Starts writing a whole function that is needed WHILE another one is being written.
    ///
    /// Descriptor invokers and PHP-ABI wrappers are discovered mid-body — the first callable that
    /// needs one is where it gets emitted — and they used to be spliced into the caller's text at
    /// that point, behind a jump over them. That puts every later block of the caller as far away
    /// as the thunk is long, and a conditional branch only reaches ±1MB on AArch64: in a program
    /// using `eval`, the callable-argument normalizer carried 464 thunks between its `cmp`/`b.eq`
    /// dispatch and its own arms, ~283k instructions, and `as` refused it with "fixup value out of
    /// range". Every eval program failed to assemble on macos-aarch64.
    ///
    /// Linux never showed it, and that is the same mechanism rather than luck: `label_global` gives
    /// each symbol its own `.text.<name>` section there, so the thunk's bytes never sat between the
    /// caller's labels — but the caller's TAIL then continued in the thunk's section, which is what
    /// broke `--debug-info`'s function extents on every PDO program and needed a section reopen.
    ///
    /// Writing the thunk out of line answers both. Its text goes to a side buffer and lands after
    /// all other text in [`Self::output`], so the caller stays contiguous in its own section on
    /// every platform and nothing has to be put back. Scopes nest: a thunk that needs a thunk
    /// sets its own caller aside the same way.
    pub fn begin_out_of_line(&mut self) -> OutOfLine {
        let log_len_at_begin = self.content_body_log.len();
        let log_floor_before = std::mem::replace(&mut self.content_body_log_floor, log_len_at_begin);
        OutOfLine {
            caller: std::mem::take(&mut self.buf),
            log_floor_before,
            log_len_at_begin,
        }
    }

    /// Finishes an out-of-line function and resumes the caller exactly where it stopped.
    ///
    /// Each body is filed behind its own plain `.text`. A body that opens its own ELF section
    /// through `label_global` moves on from there; one that only has a local entry label would
    /// otherwise land inside whatever section the PREVIOUS out-of-line body left open — the very
    /// "tail filed under another function" mistake this whole mechanism exists to avoid.
    pub fn end_out_of_line(&mut self, scope: OutOfLine) {
        let body = std::mem::replace(&mut self.buf, scope.caller);
        let out_of_line_before = self.out_of_line.len();
        for (_, key) in self.content_body_log.drain(scope.log_len_at_begin..) {
            self.out_of_line_keys.push((out_of_line_before, key));
        }
        self.push_out_of_line(&body);
        // The shared bodies written in the scope live in `out_of_line`, which a rollback of the
        // caller's text keeps: their keys stay in the set but leave the rollback log, whose
        // offsets index a buffer that no longer exists (see `out_of_line_keys`).
        self.content_body_log_floor = scope.log_floor_before;
    }

    /// Files a body taken with [`Self::end_detached`] after all other text, as
    /// [`Self::end_out_of_line`] would have.
    pub fn push_out_of_line(&mut self, body: &str) {
        self.out_of_line.push_str(".text\n");
        self.out_of_line.push_str(body);
    }

    /// Finishes a detached scope opened by [`Self::begin_out_of_line`] and returns its text.
    ///
    /// Unlike [`Self::end_out_of_line`], the body is handed back to the caller instead of being
    /// filed after all other output, so a function emitter can place cold paths at the end of the
    /// SAME function (keeping its branches intra-function and its section intact).
    pub fn end_detached(&mut self, scope: OutOfLine) -> String {
        let text = std::mem::replace(&mut self.buf, scope.caller);
        // The text returns to the caller's buffer, ahead of any later checkpoint: re-anchor the
        // scope's entries at the current end so a rollback past it forgets them with the text.
        let end = self.buf.len();
        for entry in self.content_body_log.iter_mut().skip(scope.log_len_at_begin) {
            entry.0 = end;
        }
        self.content_body_log_floor = scope.log_floor_before;
        text
    }

    /// Returns the byte length of the text written to the current buffer so far.
    ///
    /// Used as a start marker for [`Self::rewrite_text_from`]; the offset stays valid across
    /// nested out-of-line scopes because each one restores the caller's buffer intact.
    pub fn text_len(&self) -> usize {
        self.buf.len()
    }

    /// Replaces the text written since `start` with `rewrite(text)` when it returns `Some`.
    ///
    /// Late machine peepholes run on one finished function region through this hook; `None`
    /// leaves the region untouched without copying it.
    pub fn rewrite_text_from(&mut self, start: usize, rewrite: impl FnOnce(&str) -> Option<String>) {
        if let Some(rewritten) = rewrite(&self.buf[start..]) {
            self.buf.truncate(start);
            self.buf.push_str(&rewritten);
        }
    }

    /// Appends already formatted assembly text verbatim.
    pub fn append_text(&mut self, text: &str) {
        self.buf.push_str(text);
    }

    /// Emits a line comment using the target's comment prefix.
    pub fn comment(&mut self, text: &str) {
        // `@helper` / `@endhelper` markers are not commentary: the shared-helper merge reads them.
        // (`@src`, `@block` and `@fn` are read only by `--debug-info` and the source map, which
        // keep the readable form.)
        if compact_asm() && !text.starts_with("@helper") && !text.starts_with("@endhelper") {
            return;
        }
        let _ = writeln!(
            self.buf,
            "    {} {}",
            self.target.line_comment_prefix(),
            text
        );
    }

    /// Emits a blank line for visual separation.
    pub fn blank(&mut self) {
        self.buf.push('\n');
    }

    /// Emits raw text directly to the output buffer without formatting.
    pub fn raw(&mut self, text: &str) {
        // `raw` is the escape hatch every hand-written directive goes through, so a section
        // change can arrive here rather than via `label_global`. Track it, or
        // `reopen_text_section` would restore a section that is no longer the caller's.
        let line = text.trim_start();
        if let Some(rest) = line.strip_prefix(".section .text.") {
            self.current_text_section =
                Some(rest.split(&[',', ' '][..]).next().unwrap_or_default().to_string());
        } else if line.starts_with(".section") || line == ".text" || line.starts_with(".data") {
            self.current_text_section = None;
        }
        self.buf.push_str(text);
        self.buf.push('\n');
    }

    /// Emits the .text section prelude, including Intel syntax switch for x86_64.
    pub fn emit_text_prelude(&mut self) {
        if self.target.arch == Arch::X86_64 {
            self.raw(".intel_syntax noprefix");
        }
        self.raw(".text");
    }

    /// Returns an EMPTY emitter carrying this one's target and ABI flags.
    ///
    /// A body emitted on a worker has to be lowered against the same contract as one emitted
    /// here — PIC data references, the cdylib boundary and dead-strip labelling all change what
    /// the ABI helpers emit — so the flags travel and only the buffer starts over.
    pub fn fresh(&self) -> Self {
        Self {
            buf: String::with_capacity(4096),
            target: self.target,
            platform: self.platform,
            pic_data_refs: self.pic_data_refs,
            concat_capacity: self.concat_capacity,
            cdylib_boundary: self.cdylib_boundary,
            phpext_output: self.phpext_output,
            dead_strip: self.dead_strip,
            exit_hook: self.exit_hook.clone(),
            frame_slot_base: None,
            internal_labels: HashSet::default(),
            out_of_line: String::new(),
            current_text_section: None,
            spill: None,
            content_bodies: HashSet::default(),
            content_body_log: Vec::new(),
            content_body_log_floor: 0,
            out_of_line_keys: Vec::new(),
            content_memo: Default::default(),
        }
    }

    /// Emits a global entry `label` whose body is shared with every identical body.
    ///
    /// `emit` writes a complete body defining `label` (with `label_global`) and any local labels
    /// spelled from it. Generated entries such as callable invokers are keyed by more than what
    /// their code depends on, so thousands of differently named copies can carry one body: on the
    /// Symfony demo, about 7 700 of 8 900 invokers repeated one of a few hundred bodies. The body
    /// is emitted once under a name derived from its text with `label` blanked out, and `label`
    /// becomes a one-instruction jump to it.
    ///
    /// The shared body is bracketed as a keyed helper, so the parallel merge keeps one copy across
    /// workers, and this emitter remembers it so later bodies here only add their jump. A
    /// reference to `label` from outside the text (a data word, a descriptor) still resolves: the
    /// jump keeps that name. Dead-strip runtime emission records internal labels by name and is
    /// emitted directly.
    pub fn emit_content_shared(
        &mut self,
        label: &str,
        marker_open: &str,
        marker_close: &str,
        emit: impl FnOnce(&mut Self),
    ) {
        self.emit_content_shared_with_entry(label, marker_open, marker_close, |_| {}, emit);
    }

    /// Emits a keyed helper region once per emitter (and, through the helper markers, once per
    /// module after the parallel merge).
    ///
    /// Like a shared invoker body, the region is forgotten again by [`Self::rollback_to`] when
    /// the text around it is rolled back, so a caller emits it OUTSIDE any body it may roll back.
    pub fn emit_keyed_once(
        &mut self,
        key: &str,
        marker_open: &str,
        marker_close: &str,
        emit: impl FnOnce(&mut Self),
    ) {
        if !self.content_bodies.insert(key.to_string()) {
            return;
        }
        self.content_body_log.push((self.buf.len(), key.to_string()));
        self.comment(&format!("{}{}", marker_open, key));
        // A keyed body is emitted in the middle of whatever function requested it, but it runs in
        // its caller's frame with no x28 of its own: `main` keeps x28 as its frame base, and a
        // shared routine that inherited it (`_eir_invoker_enter`) wrote its boundary record
        // through main's x28 instead of the invoker's x29, beyond main's frame.
        let frame_slot_base = self.frame_slot_base.take();
        emit(self);
        self.frame_slot_base = frame_slot_base;
        self.comment(marker_close);
        if self.platform == Platform::Linux {
            self.current_text_section = Some(key.to_string());
        }
    }

    /// Records shared bodies another emitter (a parallel worker) already produced, by key.
    ///
    /// Their text was merged into this module's output without passing through this emitter, so
    /// `emit_content_shared` and `emit_keyed_once` would emit them again on the next request.
    pub fn mark_shared_bodies_emitted<'a>(&mut self, keys: impl IntoIterator<Item = &'a String>) {
        for key in keys {
            if key.starts_with("_eir_") {
                self.content_bodies.insert(key.clone());
            }
        }
    }

    /// Whether [`Self::emit_content_shared`] currently shares bodies (and so jumps from a stub).
    pub fn content_sharing_active(&self) -> bool {
        !(self.dead_strip || std::env::var_os("ELEPHC_NO_SHARED_BODIES").is_some())
    }

    /// [`Self::emit_content_shared`] with code that runs in the per-label STUB, before the jump.
    ///
    /// `entry` lets a body that is identical across signatures still receive something signature
    /// specific (a pointer to its parameter-name table) in a scratch register. It runs only when
    /// bodies are shared; when they are not, the body itself must do the same work, which it
    /// learns from [`Self::content_sharing_active`].
    pub fn emit_content_shared_with_entry(
        &mut self,
        label: &str,
        marker_open: &str,
        marker_close: &str,
        entry: impl FnOnce(&mut Self),
        emit: impl FnOnce(&mut Self),
    ) {
        self.emit_content_shared_memo(label, None, marker_open, marker_close, entry, emit);
    }

    /// [`Self::emit_content_shared_with_entry`] that skips lowering when `memo` was seen before.
    ///
    /// `ELEPHC_INVOKER_MEMO_VERIFY` is read once per process.
    ///
    /// `memo` is a hash of every input that decides the body's text except `label` (for an
    /// invoker: its signature with the parameter names left out, which reach a shared body
    /// through the stub's name table). On the Symfony demo 79 474 invokers were lowered for
    /// 8 956 distinct bodies: every repeat was emitted, hashed and rolled back only to find the
    /// body already there. A repeat now writes its stub and jumps straight to the body.
    /// `ELEPHC_INVOKER_MEMO_VERIFY=1` lowers every repeat anyway and panics if the body differs.
    pub fn emit_content_shared_memo(
        &mut self,
        label: &str,
        memo: Option<u64>,
        marker_open: &str,
        marker_close: &str,
        entry: impl FnOnce(&mut Self),
        emit: impl FnOnce(&mut Self),
    ) {
        let remembered = memo
            .filter(|_| self.content_sharing_active())
            .and_then(|key| self.content_memo.get(&key))
            .filter(|shared| self.content_bodies.contains(*shared))
            .cloned();
        if let Some(shared) = &remembered {
            if !memo_verify() {
                self.label_global(label);
                entry(self);
                match self.target.arch {
                    Arch::AArch64 => self.instruction(&format!("b {}", shared)),
                    Arch::X86_64 => self.instruction(&format!("jmp {}", shared)),
                }
                return;
            }
        }
        // `ELEPHC_NO_SHARED_BODIES=1` emits every body under its own name, for size and
        // behaviour comparisons.
        // A shared body never inherits the requesting function's frame-base register (see
        // `emit_keyed_once`).
        let frame_slot_base = self.frame_slot_base.take();
        if !self.content_sharing_active() {
            emit(self);
            self.frame_slot_base = frame_slot_base;
            return;
        }
        let start = self.checkpoint();
        let section_before = self.current_text_section.clone();
        emit(self);
        self.frame_slot_base = frame_slot_base;
        let body = self.text_from(start).to_string();
        self.rollback_to(start);
        self.current_text_section = section_before;
        let positions = label_token_positions(&body, label);
        // Hashed piece by piece: the same bytes as hashing the replaced `String`, whose `Hash`
        // is its bytes then 0xff, so the shared names are unchanged.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for_each_label_piece(&body, label.len(), &positions, "@", |piece| {
            std::hash::Hasher::write(&mut hasher, piece.as_bytes())
        });
        std::hash::Hasher::write_u8(&mut hasher, 0xff);
        let shared = format!(
            "_eir_cbody_{:016x}_{:x}",
            std::hash::Hasher::finish(&hasher),
            body.len()
        );
        if let Some(expected) = &remembered {
            // The first lowering also carried the keyed helpers it was the first to need (they
            // stay defined inside its body); a repeat finds them emitted and leaves them out. So
            // the bodies are compared without their nested keyed regions.
            let blanked = without_keyed_regions(&replace_label_tokens(&body, label, "@"), marker_open, marker_close);
            let first = MEMO_VERIFY_TEXTS.with(|texts| texts.borrow().get(expected).cloned());
            if let Some(first) = first.filter(|first| *first != blanked) {
                let difference = first
                    .lines()
                    .zip(blanked.lines())
                    .find(|(a, b)| a != b)
                    .map_or("(one body is a prefix of the other)".to_string(), |(a, b)| {
                        format!("first: {a}\n  this: {b}")
                    });
                panic!(
                    "ELEPHC_INVOKER_MEMO_VERIFY: `{label}` lowered to another body than its memo \
                     key's:\n  {difference}"
                );
            }
        } else if memo.is_some() && memo_verify() {
            let blanked = without_keyed_regions(&replace_label_tokens(&body, label, "@"), marker_open, marker_close);
            MEMO_VERIFY_TEXTS.with(|texts| texts.borrow_mut().insert(shared.clone(), blanked));
        }
        if let Some(key) = memo {
            self.content_memo.insert(key, shared.clone());
        }
        self.label_global(label);
        entry(self);
        match self.target.arch {
            Arch::AArch64 => self.instruction(&format!("b {}", shared)),
            Arch::X86_64 => self.instruction(&format!("jmp {}", shared)),
        }
        if self.content_bodies.insert(shared.clone()) {
            self.content_body_log.push((self.buf.len(), shared.clone()));
            self.comment(&format!("{}{}", marker_open, shared));
            let buf = &mut self.buf;
            for_each_label_piece(&body, label.len(), &positions, &shared, |piece| buf.push_str(piece));
            self.comment(marker_close);
            if self.platform == Platform::Linux {
                self.current_text_section = Some(shared);
            }
        }
    }

    /// How many bodies [`Self::emit_content_shared_memo`] remembers (for counter tests).
    pub fn content_memo_len(&self) -> usize {
        self.content_memo.len()
    }

    /// Returns the output emitted since `checkpoint`.
    pub fn text_from(&self, checkpoint: usize) -> &str {
        &self.buf[checkpoint.min(self.buf.len())..]
    }

    /// The text emitted so far followed by the out-of-line functions it requested.
    ///
    /// Rollbacks deliberately keep out-of-line functions, like the keyed-helper cache they are
    /// registered in. A codegen worker hands this back instead of [`Self::text_from`], or its thunks would
    /// never reach the merged output.
    pub fn text_with_out_of_line(&self) -> String {
        let mut text = String::with_capacity(self.buf.len() + self.out_of_line.len());
        text.push_str(&self.buf);
        text.push_str(&self.out_of_line);
        text
    }

    /// Byte length of the out-of-line text written so far; a start marker for
    /// [`Self::out_of_line_from`] and [`Self::replace_out_of_line_from`].
    pub fn out_of_line_len(&self) -> usize {
        self.out_of_line.len()
    }

    /// The out-of-line text written since `start`.
    pub fn out_of_line_from(&self, start: usize) -> &str {
        &self.out_of_line[start.min(self.out_of_line.len())..]
    }

    /// Discards the out-of-line functions written since `start`, and forgets the shared bodies
    /// they held so a later request writes them again.
    ///
    /// A parallel worker rolls a deferred body back; the functions that body requested must go
    /// with it, because the serial pass emits the body again and spells the same global symbols.
    pub fn rollback_out_of_line_to(&mut self, start: usize) {
        self.out_of_line.truncate(start);
        while self
            .out_of_line_keys
            .last()
            .is_some_and(|(offset, _)| *offset >= start)
        {
            if let Some((_, key)) = self.out_of_line_keys.pop() {
                self.content_bodies.remove(&key);
            }
        }
    }

    /// Replaces the out-of-line text written since `start`.
    pub fn replace_out_of_line_from(&mut self, start: usize, text: &str) {
        self.out_of_line.truncate(start);
        self.out_of_line.push_str(text);
    }

    /// Appends a block of already-emitted assembly verbatim.
    ///
    /// Unlike `raw`, this does not track a section change: the block carries its own directives
    /// and the caller reopens a known section after the last one. Scanning a multi-line block to
    /// re-derive that would cost a second pass over every body's text.
    pub fn append_block(&mut self, text: &str) {
        self.buf.push_str(text);
    }

    /// Returns the accumulated assembly output as a String, out-of-line functions last.
    ///
    /// An emitter that spilled reads its prefix back, so this always returns the whole text; the
    /// user-program path calls `finish` instead to keep the prefix on disk.
    pub fn output(self) -> String {
        match self.finish() {
            Ok((None, text)) => text,
            Ok((Some(spilled), text)) => {
                let mut whole = std::fs::read_to_string(&spilled.path)
                    .expect("spilled assembly is readable");
                let _ = std::fs::remove_file(&spilled.path);
                whole.push_str(&text);
                whole
            }
            Err(error) => panic!("could not spill assembly to disk: {error}"),
        }
    }

    /// Sends completed text to `path` whenever `spill_completed_text` finds the buffer large.
    ///
    /// The user program's text is 1.5 GB on the Symfony `--web` build. Held in one growing
    /// buffer it was the largest live allocation of the whole compile, and on macOS a freed
    /// allocation never leaves the process footprint, so its high-water mark set the build's
    /// peak. Spilled in 32 MB steps it never occupies more than one step of memory.
    pub fn enable_spill(&mut self, path: &std::path::Path) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        self.spill = Some(Spill {
            path: path.to_path_buf(),
            writer: std::io::BufWriter::with_capacity(1 << 20, file),
            bytes: 0,
            error: None,
        });
        Ok(())
    }

    /// Moves the buffered text to the spill file when spilling is on and the buffer is large.
    ///
    /// Callers invoke it only where no checkpoint is held — between function bodies — because a
    /// spill empties the buffer that checkpoints index into. The buffer must also end on a line
    /// boundary, so the spilled prefix is always whole lines.
    pub fn spill_completed_text(&mut self) {
        let Some(spill) = self.spill.as_mut() else {
            return;
        };
        if self.buf.len() < SPILL_THRESHOLD || spill.error.is_some() || !self.buf.ends_with('\n') {
            return;
        }
        use std::io::Write as _;
        match spill.writer.write_all(self.buf.as_bytes()) {
            Ok(()) => {
                spill.bytes += self.buf.len();
                self.buf.clear();
                // No checkpoint survives a spill, so no rollback can reach these bodies.
                self.content_body_log.clear();
            }
            Err(error) => spill.error = Some(error),
        }
    }

    /// Returns the spilled prefix (if any) and the text still in memory, in that order.
    pub fn finish(mut self) -> std::io::Result<(Option<SpilledText>, String)> {
        // Out-of-line functions land after every other line of text; each already starts from a
        // plain `.text` (see [`Self::end_out_of_line`]).
        let out_of_line = std::mem::take(&mut self.out_of_line);
        self.buf.push_str(&out_of_line);
        let Some(spill) = self.spill else {
            return Ok((None, self.buf));
        };
        if let Some(error) = spill.error {
            return Err(error);
        }
        if spill.bytes == 0 {
            drop(spill.writer);
            let _ = std::fs::remove_file(&spill.path);
            return Ok((None, self.buf));
        }
        let file = spill.writer.into_inner().map_err(|error| error.into_error())?;
        file.sync_data().ok();
        drop(file);
        Ok((
            Some(SpilledText {
                path: spill.path,
                len: spill.bytes,
            }),
            self.buf,
        ))
    }

    /// Records the current end of the output buffer for a possible rollback.
    pub fn checkpoint(&self) -> usize {
        self.buf.len()
    }

    /// The text emitted since `checkpoint` (empty when the checkpoint is past the buffer end).
    /// Text only spills to disk between whole bodies, so a checkpoint taken inside a body stays
    /// an offset into this buffer.
    pub fn text_since(&self, checkpoint: usize) -> &str {
        self.buf.get(checkpoint..).unwrap_or("")
    }

    /// Discards output emitted after a previously recorded checkpoint.
    pub fn rollback_to(&mut self, checkpoint: usize) {
        if checkpoint <= self.buf.len() {
            self.buf.truncate(checkpoint);
        }
        while self.content_body_log.len() > self.content_body_log_floor
            && self
                .content_body_log
                .last()
                .is_some_and(|(offset, _)| *offset >= checkpoint)
        {
            if let Some((_, name)) = self.content_body_log.pop() {
                self.content_bodies.remove(&name);
            }
        }
    }

    // ── Platform-aware relocation helpers ─────────────────────────────

    /// Emit `adrp reg, sym@PAGE` (macOS) or `adrp reg, sym` (Linux).
    pub fn adrp(&mut self, reg: &str, sym: &str) {
        self.target
            .ensure_aarch64_backend("adrp relocation emission");
        match self.platform {
            Platform::MacOS => self.instruction(&format!("adrp {}, {}@PAGE", reg, sym)),
            Platform::Linux => self.instruction(&format!("adrp {}, {}", reg, sym)),
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    /// Emit `add dst, src, sym@PAGEOFF` (macOS) or `add dst, src, :lo12:sym` (Linux).
    pub fn add_lo12(&mut self, dst: &str, src: &str, sym: &str) {
        self.target
            .ensure_aarch64_backend("lo12 relocation emission");
        match self.platform {
            Platform::MacOS => self.instruction(&format!("add {}, {}, {}@PAGEOFF", dst, src, sym)),
            Platform::Linux => self.instruction(&format!("add {}, {}, :lo12:{}", dst, src, sym)),
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    /// Emit `ldr reg, [base, sym@PAGEOFF]` (macOS) or `ldr reg, [base, :lo12:sym]` (Linux).
    pub fn ldr_lo12(&mut self, reg: &str, base: &str, sym: &str) {
        self.target.ensure_aarch64_backend("lo12 load emission");
        match self.platform {
            Platform::MacOS => {
                self.instruction(&format!("ldr {}, [{}, {}@PAGEOFF]", reg, base, sym))
            }
            Platform::Linux => self.instruction(&format!("ldr {}, [{}, :lo12:{}]", reg, base, sym)),
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    /// Emit `adrp reg, sym@GOTPAGE` (macOS) or `adrp reg, :got:sym` (Linux).
    pub fn adrp_got(&mut self, reg: &str, sym: &str) {
        self.target
            .ensure_aarch64_backend("GOT page relocation emission");
        match self.platform {
            Platform::MacOS => self.instruction(&format!("adrp {}, {}@GOTPAGE", reg, sym)),
            Platform::Linux => self.instruction(&format!("adrp {}, :got:{}", reg, sym)),
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    /// Emit `ldr reg, [base, sym@GOTPAGEOFF]` (macOS) or `ldr reg, [base, :got_lo12:sym]` (Linux).
    pub fn ldr_got_lo12(&mut self, reg: &str, base: &str, sym: &str) {
        self.target.ensure_aarch64_backend("GOT lo12 load emission");
        match self.platform {
            Platform::MacOS => {
                self.instruction(&format!("ldr {}, [{}, {}@GOTPAGEOFF]", reg, base, sym))
            }
            Platform::Linux => {
                self.instruction(&format!("ldr {}, [{}, :got_lo12:{}]", reg, base, sym))
            }
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    // ── Platform-aware syscall helper ─────────────────────────────────

    /// Emit a complete syscall sequence: sets the syscall register and traps.
    /// On macOS: `mov x16, #N` + `svc #0x80`.
    /// On Linux: optional AT_FDCWD arg shift + `mov x8, #M` + `svc #0`.
    pub fn syscall(&mut self, macos_num: u32) {
        self.target.ensure_aarch64_backend("syscall emission");
        match self.platform {
            Platform::MacOS => {
                self.instruction(&format!("mov x16, #{}", macos_num));
                self.instruction("svc #0x80");
            }
            Platform::Linux => {
                let target = self.target;
                target.emit_linux_syscall(self, macos_num);
            }
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    // ── Platform-aware C symbol call ─────────────────────────────────

    /// Emit `bl _func` (macOS) or `bl func` (Linux) for C library calls.
    pub fn bl_c(&mut self, func: &str) {
        match (self.platform, self.target.arch) {
            (Platform::MacOS, Arch::AArch64) => self.instruction(&format!("bl _{}", func)),
            (Platform::Linux, Arch::AArch64) => self.instruction(&format!("bl {}", func)),
            (Platform::Linux, Arch::X86_64) => self.instruction(&format!("call {}", func)),
            (Platform::MacOS, Arch::X86_64) => {
                panic!("C symbol calls are not implemented yet for target macos-x86_64");
            }
            (Platform::Windows, _) => panic!("Windows target is not yet supported (see issue #379)"),
        }
    }

    // ── Platform-aware entry point ───────────────────────────────────

    /// Returns the program entry point symbol: `_main` (macOS) or `main` (Linux).
    pub fn entry_symbol(&self) -> &'static str {
        match self.target.arch {
            Arch::AArch64 => match self.platform {
                Platform::MacOS => "_main",
                Platform::Linux => "main",
                Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
            },
            Arch::X86_64 => "main",
        }
    }

    /// Emit the program entry point label: `_main` (macOS) or `main` (Linux).
    pub fn entry_label(&mut self) {
        let symbol = self.entry_symbol();
        self.label_global(symbol);
    }
}

/// Rewrites every whole-token occurrence of an internal label name to its
/// Mach-O assembler-local (`L`-prefixed) form, covering both the `name:`
/// definition and every branch/reference to it. Used by the macOS dead-strip
/// path: under `.subsections_via_symbols`, conditional branches may only target
/// assembler-local labels, and `L`-prefixed labels also do not start a new atom,
/// so each `__rt_*` helper stays a single dead-strippable unit. Matching is
/// whole-token (identifier runs of `[A-Za-z0-9_$]`), so a name is never rewritten
/// inside a longer identifier. Quoted assembly strings are copied verbatim within
/// their physical line, so user string constants cannot be changed when user
/// metadata references one of the localized labels and an unmatched quote cannot
/// suppress localization on later lines.
pub fn localize_internal_labels(asm: &str, internal: &HashSet<String>) -> String {
    if internal.is_empty() {
        return asm.to_string();
    }
    let bytes = asm.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';
    let mut out = String::with_capacity(asm.len() + internal.len());
    // The output differs from the input ONLY where an `L` is inserted, so everything between
    // two insertions is copied in one go. Pushing each quoted run, each token and each run of
    // punctuation separately meant tens of millions of `push_str` calls to rebuild 1.4 GB of
    // text that was already in memory.
    let mut verbatim_from = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i += 1;
            let mut escaped = false;
            while i < bytes.len() {
                let byte = bytes[i];
                i += 1;
                if byte == b'\n' {
                    break;
                } else if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    break;
                }
            }
        } else if is_ident(bytes[i]) {
            let start = i;
            while i < bytes.len() && is_ident(bytes[i]) {
                i += 1;
            }
            if internal.contains(&asm[start..i]) {
                // Flush everything before the token, insert the prefix, and leave the token
                // itself at the head of the next verbatim stretch.
                out.push_str(&asm[verbatim_from..start]);
                out.push('L');
                verbatim_from = start;
            }
        } else {
            while i < bytes.len() && !is_ident(bytes[i]) && bytes[i] != b'"' {
                i += 1;
            }
        }
    }
    out.push_str(&asm[verbatim_from..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The label is replaced where it names itself or a label spelled from it, never inside a
    /// counter-numbered sibling, and hashing the pieces gives the hash of the replaced string,
    /// so shared body names did not change when the hashing stopped building that string.
    #[test]
    fn label_pieces_replace_and_hash_like_the_replaced_string() {
        let body = "_f_7:\n    b _f_7_end\n    bl _f_71\n    bl _f_7_3\n_f_7_end:\n    ret _f_7";
        let replaced = replace_label_tokens(body, "_f_7", "@");
        assert_eq!(replaced, "@:\n    b @_end\n    bl _f_71\n    bl _f_7_3\n@_end:\n    ret @");
        let mut whole = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&replaced, &mut whole);
        let mut pieces = std::collections::hash_map::DefaultHasher::new();
        let positions = label_token_positions(body, "_f_7");
        for_each_label_piece(body, 4, &positions, "@", |piece| {
            std::hash::Hasher::write(&mut pieces, piece.as_bytes())
        });
        std::hash::Hasher::write_u8(&mut pieces, 0xff);
        assert_eq!(std::hash::Hasher::finish(&whole), std::hash::Hasher::finish(&pieces));
    }

    /// Verifies comment prefix is platform aware.
    #[test]
    fn test_comment_prefix_is_platform_aware() {
        let mut mac = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        mac.comment("-- block --");
        assert_eq!(mac.output(), "    ; -- block --\n");

        let mut linux = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        linux.comment("-- block --");
        assert_eq!(linux.output(), "    // -- block --\n");

        let mut linux_x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        linux_x86.comment("-- block --");
        assert_eq!(linux_x86.output(), "    # -- block --\n");
    }

    /// Verifies text prelude switches x86 to intel syntax.
    #[test]
    fn test_text_prelude_switches_x86_to_intel_syntax() {
        let mut mac = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        mac.emit_text_prelude();
        assert_eq!(mac.output(), ".text\n");

        let mut linux_x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        linux_x86.emit_text_prelude();
        assert_eq!(linux_x86.output(), ".intel_syntax noprefix\n.text\n");
    }

    /// Verifies internal symbol references are localized without rewriting quoted user bytes.
    #[test]
    fn test_localize_internal_labels_preserves_assembly_strings() {
        let internal: HashSet<String> = ["_eir_branch_1".to_string()].into_iter().collect();
        let asm = "    b _eir_branch_1\n_eir_branch_1:\n    .ascii \"_eir_branch_1\"\n";
        assert_eq!(
            localize_internal_labels(asm, &internal),
            "    b L_eir_branch_1\nL_eir_branch_1:\n    .ascii \"_eir_branch_1\"\n"
        );
    }

    /// Verifies an unmatched quote in one assembly line cannot hide labels on later lines.
    #[test]
    fn test_localize_internal_labels_bounds_unmatched_quotes_to_one_line() {
        let internal: HashSet<String> = ["_eir_branch_1".to_string()].into_iter().collect();
        let asm = "    ; unmatched \" in comment\n    b _eir_branch_1\n_eir_branch_1:\n";
        assert_eq!(
            localize_internal_labels(asm, &internal),
            "    ; unmatched \" in comment\n    b L_eir_branch_1\nL_eir_branch_1:\n"
        );
    }


    /// Emits `_php_outer`, whose body is split by an out-of-line `_thunk`, and returns the text.
    ///
    /// `HEAD` and `TAIL` are the caller's instructions on either side of the point where the
    /// thunk was needed; `THUNK` is the thunk's body.
    fn caller_with_out_of_line_thunk(platform: Platform) -> String {
        let mut e = Emitter::new(Target::new(platform, Arch::AArch64));
        e.label_global("_php_outer");
        e.instruction("mov x0, #1 // HEAD");
        let scope = e.begin_out_of_line();
        e.label_global("_thunk");
        e.instruction("mov x0, #2 // THUNK");
        e.end_out_of_line(scope);
        e.instruction("mov x0, #3 // TAIL");
        e.output()
    }

    /// A thunk needed mid-body must not sit between its caller's instructions.
    ///
    /// Spliced inline behind a jump, every thunk pushed the caller's later labels further from its
    /// earlier branches, until the normalizer's `b.eq` to its own arm crossed 464 thunks (~1.1MB)
    /// and `as` refused the program: "fixup value out of range". On Mach-O nothing splits `__text`,
    /// so this is the platform where that distance was real.
    #[test]
    fn test_an_out_of_line_thunk_leaves_its_caller_contiguous_on_mach_o() {
        let out = caller_with_out_of_line_thunk(Platform::MacOS);
        let head = out.find("HEAD").expect("caller head");
        let tail = out.find("TAIL").expect("caller tail");
        let thunk = out.find("THUNK").expect("thunk body");
        assert!(head < tail && tail < thunk, "the thunk must follow its whole caller:\n{out}");
        assert!(!out[head..tail].contains("_thunk"), "the thunk was spliced into its caller:\n{out}");
        let text = out.rfind(".text").expect("out-of-line text is reopened");
        assert!(tail < text && text < thunk, "the thunk must follow a `.text`:\n{out}");
    }

    /// On ELF the caller keeps its own section without anything having to reopen it.
    ///
    /// Inline, `label_global` opened `.text._thunk` mid-caller, the caller's tail — epilogue and
    /// `ret` included — continued in the thunk's section, and `--debug-info`'s
    /// `Lelephc_fend_N - <fn>` extents stopped assembling, which is how every PDO program failed to
    /// build on Linux. That needed a section reopen after each thunk. Out of line, the caller's
    /// section is opened once and its tail is simply never displaced.
    #[test]
    fn test_an_out_of_line_thunk_needs_no_section_reopen_on_elf() {
        let out = caller_with_out_of_line_thunk(Platform::Linux);
        assert_eq!(
            out.matches(".section .text._php_outer").count(),
            1,
            "the caller's section must be opened exactly once:\n{out}"
        );
        let head = out.find("HEAD").expect("caller head");
        let tail = out.find("TAIL").expect("caller tail");
        assert!(!out[head..tail].contains(".section"), "the caller left its section:\n{out}");
        let thunk_section = out.find(".section .text._thunk").expect("the thunk keeps its own section");
        assert!(tail < thunk_section, "the thunk must follow its caller:\n{out}");
    }

    /// A thunk that needs a thunk sets its own caller aside the same way, and each body stays whole.
    #[test]
    fn test_out_of_line_scopes_nest_without_splitting_either_body() {
        let mut e = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        e.label_global("_php_outer");
        e.instruction("mov x0, #1 // CALLER_HEAD");
        let outer = e.begin_out_of_line();
        e.label_global("_outer_thunk");
        e.instruction("mov x0, #2 // OUTER_HEAD");
        let inner = e.begin_out_of_line();
        e.label_global("_inner_thunk");
        e.instruction("mov x0, #3 // INNER");
        e.end_out_of_line(inner);
        e.instruction("mov x0, #4 // OUTER_TAIL");
        e.end_out_of_line(outer);
        e.instruction("mov x0, #5 // CALLER_TAIL");
        let out = e.output();

        let at = |needle: &str| out.find(needle).unwrap_or_else(|| panic!("missing {needle}:\n{out}"));
        assert!(at("CALLER_HEAD") < at("CALLER_TAIL"), "{out}");
        assert!(at("CALLER_TAIL") < at("INNER") && at("CALLER_TAIL") < at("OUTER_HEAD"), "{out}");
        assert!(
            !out[at("OUTER_HEAD")..at("OUTER_TAIL")].contains("INNER"),
            "the inner thunk was spliced into the outer one:\n{out}"
        );
    }

    /// An emitter that never goes out of line produces exactly what it did before.
    ///
    /// Runtime objects are emitted this way and never defer anything, so their text must not grow
    /// a trailing `.text` — that would change every runtime object for no reason.
    #[test]
    fn test_an_emitter_without_out_of_line_functions_is_unchanged() {
        let mut e = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        e.label_global("_php_outer");
        e.instruction("ret");
        assert_eq!(e.output(), ".globl _php_outer\n_php_outer:\n    ret\n");
    }
}
