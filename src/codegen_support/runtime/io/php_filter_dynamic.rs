//! Purpose:
//! Emits the run-time half of `php://filter/...`: parsing a URL whose bytes are only known at
//! run time, attaching the filter it names once the resource behind it is open, and saying what
//! php says about the open — the failed-open line, and the names it could not resolve.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The dynamic `fopen()` lowering, which parses first and attaches after boxing.
//! - The path readers and `file_put_contents()`, which share the parse and the reports.
//!
//! Key details:
//! - Two things the parse publishes exist only for the DIAGNOSTICS: the URL itself, because the
//!   caller opens the RESOURCE and php names the whole URL when that fails, and the spans of the
//!   names that resolved to nothing, because php warns twice for each and skipping one in silence
//!   turns a typo in a filter name into an unfiltered read.
//! - Every diagnostic names the CALLING function, which is why the reporters take it as a string
//!   pair instead of each route composing its own copy.
//! - A filter URL is "open THIS, then filter it". The parse therefore hands the caller the
//!   RESOURCE and stops: the open path stays exactly the one a plain path takes, instead of the
//!   opener being re-implemented inside a helper that would also have to recurse for a
//!   `resource=php://temp` and carry the fopen mode down with it.
//! - Attaching needs nothing new. The literal path already goes through `__rt_filter_create` and
//!   `__rt_stream_filter_link`, two runtime helpers with plain arguments; the only difference
//!   here is that the id and direction are run-time values rather than immediates.
//! - An unrecognised name is SKIPPED and its neighbours still apply, and a URL whose every name is
//!   unrecognised publishes direction 0, which opens the resource unfiltered. Both readings match
//!   what the literal path does with the same URL, and what `php -n` 8.5.6 was measured doing.
//! - The parse publishes a LIST. `read=string.toupper|string.rot13` names two filters and php runs
//!   the bytes through both; a one-slot hand-off answered the first filter's result and said
//!   nothing, which is the worst shape a wrong answer can take.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

use crate::codegen_support::runtime::data::{
    FGC_FILTER_FAIL_TAIL, PF_WARN_CREATE_END, PF_WARN_CREATE_MID, PF_WARN_HEAD,
    PF_WARN_LOCATE_END, PF_WARN_LOCATE_MID, PF_WARN_OPEN_MID,
};
use crate::codegen_support::runtime::resources::layout::{
    STREAM_READ_FILTER_HEAD_OFFSET, STREAM_WRITE_FILTER_HEAD_OFFSET,
};

/// Filters a single run-time `php://filter` URL can hand to the attach.
///
/// php-src imposes no limit, so this is a bound elephc adds; it exists because the hand-off is a
/// fixed BSS array rather than an allocation. Names past it are dropped. Nothing in the wild
/// chains anywhere near this many — the literal path, which is what real code takes, has no bound
/// at all — but the number is stated here rather than left to be discovered from the assembly.
pub(crate) const PHP_FILTER_PENDING_MAX: usize = 32;

/// Filtered opens that can be IN FLIGHT at once.
///
/// An open nests when the resource is a user wrapper, because `stream_open` is PHP and may open
/// something itself. That inner open runs the parse, which republishes the single-slot hand-off,
/// so the outer open cannot read its own URL back out of it — and, worse, could not tell whether
/// it had opened a suppression scope, which would leak one and silence every later warning in
/// the program. Each open therefore SAVES what it needs on the way in and reads its own frame on
/// the way out. Past this depth an open behaves as an unfiltered one: it neither suppresses nor
/// composes, which is quiet rather than wrong.
pub(crate) const PHP_FILTER_OPEN_DEPTH_MAX: usize = 8;

/// Words one parked hand-off occupies, rounded so the depth indexes it with a SHIFT.
///
/// [`PENDING_STATE`] is what actually has to fit; the assertion below is what keeps the two
/// honest when a slot is added to the hand-off.
pub(crate) const PHP_FILTER_PENDING_FRAME_SLOTS: usize = 128;

/// `log2` of the frame stride in BYTES, so `depth << SHIFT` reaches a frame in one instruction.
const PHP_FILTER_PENDING_FRAME_SHIFT: u32 = PHP_FILTER_PENDING_FRAME_SLOTS.trailing_zeros() + 3;

/// Everything `__rt_php_filter_parse` publishes that must survive the OPEN it is published for.
///
/// The parse hands its results to the attach and the reports through fixed globals, and the open
/// that sits between them can run PHP — a user wrapper's `stream_open` — which can `fopen()`
/// something else and re-enter the parse. One global set therefore answers the INNER open's
/// question to the OUTER open's caller: the outer chain vanished (`abc` where php answers `ABC`)
/// and the outer URL's unresolved names went unreported with it.
///
/// `_php_filter_res_ptr`/`_len` are deliberately absent: the caller reads them into registers
/// before it opens anything, so no nested parse can reach them. `_php_filter_open_dirs` IS here —
/// it is published before the parse but read by the report AFTER the open, and a nested open
/// publishes its own.
const PENDING_STATE: &[(&str, usize)] = &[
    ("_php_filter_pending_count", 1),
    ("_php_filter_pending_mode", 1),
    ("_php_filter_unknown_count", 1),
    ("_php_filter_url_ptr", 1),
    ("_php_filter_url_len", 1),
    ("_php_filter_url_dir", 1),
    ("_php_filter_open_dirs", 1),
    ("_php_filter_pending_ids", PHP_FILTER_PENDING_MAX),
    ("_php_filter_unknown_ptr", PHP_FILTER_PENDING_MAX),
    ("_php_filter_unknown_len", PHP_FILTER_PENDING_MAX),
];

/// Words one parked hand-off actually needs.
const PENDING_STATE_SLOTS: usize = {
    let mut total = 0;
    let mut index = 0;
    while index < PENDING_STATE.len() {
        total += PENDING_STATE[index].1;
        index += 1;
    }
    total
};

const _: () = assert!(
    PENDING_STATE_SLOTS <= PHP_FILTER_PENDING_FRAME_SLOTS,
    "a parked php://filter hand-off must fit the frame the depth indexes"
);

/// Emits `__rt_pf_match`, `__rt_php_filter_parse` and `__rt_php_filter_attach_pending`.
pub fn emit_php_filter_dynamic(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_prefix_match_aarch64(emitter);
            emit_filter_parse_aarch64(emitter);
            emit_filter_attach_aarch64(emitter);
            emit_mode_dirs_aarch64(emitter);
            emit_suppress_begin_aarch64(emitter);
            emit_pending_save_aarch64(emitter);
            emit_pending_restore_aarch64(emitter);
            emit_open_failed_aarch64(emitter);
            emit_unknown_report_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_prefix_match_x86_64(emitter);
            emit_filter_parse_x86_64(emitter);
            emit_filter_attach_x86_64(emitter);
            emit_mode_dirs_x86_64(emitter);
            emit_suppress_begin_x86_64(emitter);
            emit_pending_save_x86_64(emitter);
            emit_pending_restore_x86_64(emitter);
            emit_open_failed_x86_64(emitter);
            emit_unknown_report_x86_64(emitter);
        }
    }
}

/// `__rt_pf_match(x0 = haystack, x1 = length, x2 = needle, x3 = needle length) -> x0 = 0/1`.
fn emit_prefix_match_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: does a byte range start with a needle ---");
    emitter.label_global("__rt_pf_match");
    emitter.instruction("cmp x1, x3");                                          // enough bytes to hold the needle?
    emitter.instruction("b.lt __rt_pfm_no");                                    // too short to start with it
    emitter.instruction("mov x9, #0");                                          // comparison index
    emitter.label("__rt_pfm_byte");
    emitter.instruction("cmp x9, x3");                                          // compared the whole needle?
    emitter.instruction("b.hs __rt_pfm_yes");                                   // every byte agreed
    emitter.instruction("ldrb w10, [x0, x9]");                                  // one haystack byte
    emitter.instruction("ldrb w11, [x2, x9]");                                  // the corresponding needle byte
    emitter.instruction("cmp w10, w11");                                        // do they agree?
    emitter.instruction("b.ne __rt_pfm_no");                                    // a mismatch ends it
    emitter.instruction("add x9, x9, #1");                                      // advance the comparison index
    emitter.instruction("b __rt_pfm_byte");                                     // keep comparing
    emitter.label("__rt_pfm_yes");
    emitter.instruction("mov x0, #1");                                          // the range starts with the needle
    emitter.instruction("ret");
    emitter.label("__rt_pfm_no");
    emitter.instruction("mov x0, #0");                                          // it does not
    emitter.instruction("ret");
}

/// `__rt_php_filter_parse(x0 = path, x1 = length) -> x0 = 1 when a filter URL was parsed`.
fn emit_filter_parse_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: parse a run-time php://filter URL ---");
    emitter.label_global("__rt_php_filter_parse");
    // Frame: [0]=cursor [8]=remaining [16]=direction [24]=scan index / name length
    //        [32]=segment start [40]=filters resolved [48]=separator offset [56]=segment pointer
    //        [64]=segment length [72]=unresolved names, saved pair at [80].
    emitter.instruction("sub sp, sp, #96");                                     // reserve the parse frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the path
    emitter.instruction("str x1, [sp, #8]");                                    // preserve its length
    abi::emit_symbol_address(emitter, "x2", "_pf_n_prefix");
    emitter.instruction("mov x3, #13");                                         // "php://filter/"
    emitter.instruction("bl __rt_pf_match");                                    // is this a filter URL at all?
    emitter.instruction("cbz x0, __rt_pfp_no");                                 // no: leave the path alone
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the path
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the length
    // Publish the WHOLE URL before the cursor walks off it: the caller opens the RESOURCE, and
    // php names the URL — not the resource — when that open fails.
    abi::emit_symbol_address(emitter, "x12", "_php_filter_url_ptr");
    emitter.instruction("str x0, [x12]");                                       // the URL the program actually wrote
    abi::emit_symbol_address(emitter, "x12", "_php_filter_url_len");
    emitter.instruction("str x1, [x12]");                                       // and its length
    emitter.instruction("add x0, x0, #13");                                     // step past the scheme
    emitter.instruction("sub x1, x1, #13");                                     // and shorten the remaining count
    emitter.instruction("str x0, [sp, #0]");                                    // the cursor now sits on the direction
    emitter.instruction("str x1, [sp, #8]");

    emitter.instruction("mov x9, #3");                                          // no prefix means both directions
    emitter.instruction("str x9, [sp, #16]");
    abi::emit_symbol_address(emitter, "x2", "_pf_n_read");
    emitter.instruction("mov x3, #5");                                          // "read="
    emitter.instruction("bl __rt_pf_match");
    emitter.instruction("cbz x0, __rt_pfp_try_write");                          // not a read-only URL
    emitter.instruction("mov x9, #1");                                          // read direction
    emitter.instruction("str x9, [sp, #16]");
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("ldr x1, [sp, #8]");
    emitter.instruction("add x0, x0, #5");                                      // step past "read="
    emitter.instruction("sub x1, x1, #5");
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction("str x1, [sp, #8]");
    emitter.instruction("b __rt_pfp_find_resource");

    emitter.label("__rt_pfp_try_write");
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("ldr x1, [sp, #8]");
    abi::emit_symbol_address(emitter, "x2", "_pf_n_write");
    emitter.instruction("mov x3, #6");                                          // "write="
    emitter.instruction("bl __rt_pf_match");
    emitter.instruction("cbz x0, __rt_pfp_find_resource");                      // neither prefix: both directions
    emitter.instruction("mov x9, #2");                                          // write direction
    emitter.instruction("str x9, [sp, #16]");
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("ldr x1, [sp, #8]");
    emitter.instruction("add x0, x0, #6");                                      // step past "write="
    emitter.instruction("sub x1, x1, #6");
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction("str x1, [sp, #8]");

    // -- scan for "/resource=", which separates the filter name from what it wraps --
    emitter.label("__rt_pfp_find_resource");
    emitter.instruction("mov x9, #0");                                          // scan index
    emitter.instruction("str x9, [sp, #24]");
    emitter.label("__rt_pfp_scan");
    emitter.instruction("ldr x9, [sp, #24]");                                   // the scan index
    emitter.instruction("ldr x1, [sp, #8]");                                    // bytes remaining after the direction
    emitter.instruction("add x10, x9, #10");                                    // does "/resource=" still fit here?
    emitter.instruction("cmp x10, x1");
    emitter.instruction("b.gt __rt_pfp_no_resource");                           // ran out: the URL names no resource
    emitter.instruction("ldr x0, [sp, #0]");                                    // the filter-name cursor
    emitter.instruction("add x0, x0, x9");                                      // the candidate separator position
    emitter.instruction("sub x1, x1, x9");                                      // bytes left from there
    abi::emit_symbol_address(emitter, "x2", "_pf_n_resource");
    emitter.instruction("mov x3, #10");                                         // "/resource="
    emitter.instruction("bl __rt_pf_match");
    emitter.instruction("cbnz x0, __rt_pfp_found");                             // the separator starts here
    emitter.instruction("ldr x9, [sp, #24]");
    emitter.instruction("add x9, x9, #1");                                      // keep scanning
    emitter.instruction("str x9, [sp, #24]");
    emitter.instruction("b __rt_pfp_scan");

    emitter.label("__rt_pfp_found");
    emitter.instruction("ldr x9, [sp, #24]");                                   // the separator offset IS the name length
    emitter.instruction("ldr x0, [sp, #0]");                                    // the filter name starts at the cursor
    emitter.instruction("ldr x1, [sp, #8]");                                    // bytes after the direction
    emitter.instruction("add x10, x0, x9");                                     // the separator
    emitter.instruction("add x10, x10, #10");                                   // the resource begins after it
    emitter.instruction("sub x11, x1, x9");                                     // bytes from the separator on
    emitter.instruction("sub x11, x11, #10");                                   // minus the separator itself
    emitter.instruction("cmp x11, #1");                                         // an empty resource names nothing
    emitter.instruction("b.lt __rt_pfp_no_resource");                           // php throws for it, and the caller does the throwing
    abi::emit_symbol_address(emitter, "x12", "_php_filter_res_ptr");
    emitter.instruction("str x10, [x12]");                                      // publish the resource pointer
    abi::emit_symbol_address(emitter, "x12", "_php_filter_res_len");
    emitter.instruction("str x11, [x12]");                                      // and its length
    // A resource that is itself a filter URL is what php-src refuses too.
    emitter.instruction("mov x0, x10");
    emitter.instruction("mov x1, x11");
    abi::emit_symbol_address(emitter, "x2", "_pf_n_prefix");
    emitter.instruction("mov x3, #12");                                         // "php://filter" without the slash
    emitter.instruction("bl __rt_pf_match");
    emitter.instruction("cbnz x0, __rt_pfp_no");                                // nested filters are not supported

    // -- resolve EVERY name in the `|` chain, in order, the way the literal path does --
    emitter.instruction("str xzr, [sp, #32]");                                  // the current segment's start offset
    emitter.instruction("str xzr, [sp, #40]");                                  // filters resolved so far
    emitter.instruction("str xzr, [sp, #72]");                                  // names that resolved to nothing

    emitter.label("__rt_pfp_seg");
    emitter.instruction("ldr x9, [sp, #24]");                                   // the full name length
    emitter.instruction("ldr x10, [sp, #32]");                                  // where this segment starts
    emitter.instruction("cmp x10, x9");
    emitter.instruction("b.hs __rt_pfp_publish");                               // past the last segment
    // Measure this segment: it ends at the next '|', or at the end of the name.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the name
    emitter.instruction("mov x11, x10");                                        // scan index, from the segment start
    emitter.label("__rt_pfp_pipe");
    emitter.instruction("cmp x11, x9");                                         // reached the end of the name?
    emitter.instruction("b.hs __rt_pfp_seg_end");                               // no further pipe: the segment runs to it
    emitter.instruction("ldrb w12, [x0, x11]");
    emitter.instruction("cmp w12, #124");                                       // ASCII '|'
    emitter.instruction("b.eq __rt_pfp_seg_end");                               // this one closes the segment
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_pfp_pipe");

    emitter.label("__rt_pfp_seg_end");
    emitter.instruction("str x11, [sp, #48]");                                  // remember where the separator sits
    emitter.instruction("sub x1, x11, x10");                                    // this segment's length
    emitter.instruction("cbz x1, __rt_pfp_seg_next");                           // an empty segment names nothing
    emitter.instruction("add x0, x0, x10");                                     // the segment's first byte
    emitter.instruction("str x0, [sp, #56]");                                   // remember the span: the id lookup destroys both
    emitter.instruction("str x1, [sp, #64]");
    emitter.instruction("bl __rt_builtin_filter_id");                           // x0 = the built-in id, or 0
    emitter.instruction("cbnz x0, __rt_pfp_seg_known");                         // it named a built-in filter
    // An unrecognised name is SKIPPED — its neighbours still apply — but php does not skip it in
    // SILENCE: it warns twice for every creation that fails. The span is recorded here because
    // nothing downstream can recover it; the run-time parse used to drop it on the floor, which
    // turned a typo in a filter name into a silently unfiltered read.
    emitter.instruction("ldr x11, [sp, #72]");                                  // names recorded so far
    emitter.instruction(&format!("cmp x11, #{PHP_FILTER_PENDING_MAX}"));
    emitter.instruction("b.hs __rt_pfp_seg_next");                              // the report array is full
    abi::emit_symbol_address(emitter, "x12", "_php_filter_unknown_ptr");
    emitter.instruction("ldr x13, [sp, #56]");
    emitter.instruction("str x13, [x12, x11, lsl #3]");                         // where the name starts
    abi::emit_symbol_address(emitter, "x12", "_php_filter_unknown_len");
    emitter.instruction("ldr x13, [sp, #64]");
    emitter.instruction("str x13, [x12, x11, lsl #3]");                         // and how long it is
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("str x11, [sp, #72]");
    emitter.instruction("b __rt_pfp_seg_next");

    emitter.label("__rt_pfp_seg_known");
    emitter.instruction("ldr x11, [sp, #40]");                                  // filters resolved so far
    emitter.instruction(&format!("cmp x11, #{PHP_FILTER_PENDING_MAX}"));
    emitter.instruction("b.hs __rt_pfp_seg_next");                              // the hand-off array is full
    abi::emit_symbol_address(emitter, "x12", "_php_filter_pending_ids");
    emitter.instruction("str x0, [x12, x11, lsl #3]");                          // append this filter to the list
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("str x11, [sp, #40]");

    emitter.label("__rt_pfp_seg_next");
    emitter.instruction("ldr x11, [sp, #48]");                                  // the separator this segment ended on
    emitter.instruction("add x11, x11, #1");                                    // the next segment starts after it
    emitter.instruction("str x11, [sp, #32]");
    emitter.instruction("b __rt_pfp_seg");

    emitter.label("__rt_pfp_publish");
    emitter.instruction("ldr x11, [sp, #40]");                                  // how many filters resolved
    abi::emit_symbol_address(emitter, "x12", "_php_filter_pending_count");
    emitter.instruction("str x11, [x12]");                                      // publish the count
    emitter.instruction("ldr x9, [sp, #16]");                                   // the requested direction
    // The URL's OWN direction, kept apart from the pending mode below: that one is zeroed when
    // nothing resolved, and the warning count depends on which prefix the URL spelled.
    abi::emit_symbol_address(emitter, "x12", "_php_filter_url_dir");
    emitter.instruction("str x9, [x12]");
    emitter.instruction("ldr x10, [sp, #72]");                                  // names that resolved to nothing
    abi::emit_symbol_address(emitter, "x12", "_php_filter_unknown_count");
    emitter.instruction("str x10, [x12]");                                      // publish them for the report
    emitter.instruction("cmp x11, #0");                                         // did ANY name resolve?
    emitter.instruction("csel x9, xzr, x9, eq");                                // a chain of unknowns attaches nothing
    abi::emit_symbol_address(emitter, "x12", "_php_filter_pending_mode");
    emitter.instruction("str x9, [x12]");                                       // publish the direction
    emitter.instruction("mov x0, #1");                                          // the caller should open the resource
    emitter.instruction("ldp x29, x30, [sp, #80]");
    emitter.instruction("add sp, sp, #96");
    emitter.instruction("ret");

    emitter.label("__rt_pfp_no");
    emit_clear_parse_state_aarch64(emitter);
    emitter.instruction("mov x0, #0");                                          // the path is not a usable filter URL
    emitter.instruction("ldp x29, x30, [sp, #80]");
    emitter.instruction("add sp, sp, #96");
    emitter.instruction("ret");

    // A filter URL that names NO resource — missing or empty — is verdict 2: php answers it
    // with `Error: No URL resource specified`, and the throw lives at the call sites, which
    // have the lowering machinery a throwable needs.
    emitter.label("__rt_pfp_no_resource");
    emit_clear_parse_state_aarch64(emitter);
    emitter.instruction("mov x0, #2");                                          // the caller must throw php's Error
    emitter.instruction("ldp x29, x30, [sp, #80]");
    emitter.instruction("add sp, sp, #96");
    emitter.instruction("ret");
}

/// Clears every slot the parse publishes, for the two verdicts that publish nothing.
///
/// The URL pointer doubles as the "a filter URL was parsed" flag, so leaving it set on a
/// declined parse would make the NEXT plain open name a URL it never saw.
fn emit_clear_parse_state_aarch64(emitter: &mut Emitter) {
    for symbol in [
        "_php_filter_pending_count",
        "_php_filter_pending_mode",
        "_php_filter_unknown_count",
        "_php_filter_url_ptr",
        "_php_filter_url_len",
        "_php_filter_url_dir",
    ] {
        abi::emit_symbol_address(emitter, "x12", symbol);
        emitter.instruction("str xzr, [x12]");                                  // nothing is pending
    }
}

/// `__rt_php_filter_attach_pending(x0 = boxed fopen result)`; returns it unchanged.
fn emit_filter_attach_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: attach the filter a php://filter URL named ---");
    emitter.label_global("__rt_php_filter_attach_pending");
    // Frame: [0]=boxed result [8]=stream handle [16]=filter handle [24]=direction
    //        [32]=list index [40]=filters published, saved pair at [48].
    emitter.instruction("sub sp, sp, #64");                                     // reserve the attach frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the boxed result
    abi::emit_symbol_address(emitter, "x9", "_php_filter_pending_mode");
    emitter.instruction("ldr x10, [x9]");                                       // the direction the URL asked for
    emitter.instruction("str xzr, [x9]");                                       // clear it: exactly one open consumes it
    emitter.instruction("str x10, [sp, #24]");
    abi::emit_symbol_address(emitter, "x9", "_php_filter_pending_count");
    emitter.instruction("ldr x11, [x9]");                                       // how many filters the URL named
    emitter.instruction("str xzr, [x9]");                                       // cleared for the same reason
    emitter.instruction("str x11, [sp, #40]");
    emitter.instruction("cbz x10, __rt_pfa_done");                              // no direction: nothing to attach
    emitter.instruction("cbz x11, __rt_pfa_done");                              // no filter: the resource opened plain
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("ldr x9, [x0]");                                        // the boxed tag
    emitter.instruction("cmp x9, #9");                                          // did the open produce a resource?
    emitter.instruction("b.ne __rt_pfa_done");                                  // a false result carries no stream
    emitter.instruction("ldr x9, [x0, #8]");                                    // the opaque stream handle
    emitter.instruction("str x9, [sp, #8]");

    // Attach in list order: php runs the bytes through the filters the way the URL spelled them,
    // and `__rt_stream_filter_link` appends at the tail, so creating in order builds that chain.
    emitter.instruction("str xzr, [sp, #32]");                                  // the first filter in the list
    emitter.label("__rt_pfa_next");
    emitter.instruction("ldr x9, [sp, #32]");                                   // which filter this pass attaches
    emitter.instruction("ldr x10, [sp, #40]");                                  // how many there are
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.hs __rt_pfa_done");                                  // the whole chain is attached
    abi::emit_symbol_address(emitter, "x11", "_php_filter_pending_ids");
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // the built-in filter id
    emitter.instruction("add x9, x9, #1");                                      // advance before the calls clobber it
    emitter.instruction("str x9, [sp, #32]");
    emitter.instruction("mov x1, #0");                                          // built-ins carry no user-filter object
    emitter.instruction("ldr x2, [sp, #24]");                                   // direction bits from the URL
    emitter.instruction("mov x3, #0");                                          // built-ins retain no params value
    abi::emit_call_label(emitter, "__rt_filter_create");
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the filter handle
    emitter.instruction("ldr x10, [sp, #24]");
    emitter.instruction("tst x10, #1");                                         // does it filter reads?
    emitter.instruction("b.eq __rt_pfa_write");
    emitter.instruction("ldr x0, [sp, #8]");                                    // stream handle
    emitter.instruction("ldr x1, [sp, #16]");                                   // filter handle
    emitter.instruction(&format!("mov x2, #{STREAM_READ_FILTER_HEAD_OFFSET}"));
    emitter.instruction("mov x3, #0");                                          // append at the chain tail
    abi::emit_call_label(emitter, "__rt_stream_filter_link");
    emitter.label("__rt_pfa_write");
    emitter.instruction("ldr x10, [sp, #24]");
    emitter.instruction("tst x10, #2");                                         // does it filter writes?
    emitter.instruction("b.eq __rt_pfa_next");
    emitter.instruction("ldr x0, [sp, #8]");                                    // stream handle
    emitter.instruction("ldr x1, [sp, #16]");                                   // filter handle
    emitter.instruction(&format!("mov x2, #{STREAM_WRITE_FILTER_HEAD_OFFSET}"));
    emitter.instruction("mov x3, #0");                                          // append at the chain tail
    abi::emit_call_label(emitter, "__rt_stream_filter_link");
    emitter.instruction("b __rt_pfa_next");                                     // on to the next filter in the chain

    emitter.label("__rt_pfa_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // hand the boxed result straight back
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// `__rt_php_filter_mode_dirs(x0 = mode pointer, x1 = mode length)`.
///
/// Publishes the directions the OPEN MODE selects into `_php_filter_open_dirs`: bit 0 read,
/// bit 1 write. php-src searches the WHOLE mode string with `strchr`, so `rb` reads, `a`
/// writes, `r+` does both and `x` selects NEITHER — which is why a prefix-less filter chain
/// opened `r+` warns twice per unknown name and the same chain opened `x` warns not at all.
/// Measured on `php -n` 8.5.6; a compile-time-literal mode would answer only half the calls,
/// because `fopen($url, $mode)` reaches here with both assembled at run time.
fn emit_mode_dirs_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: directions an fopen mode string selects ---");
    emitter.label_global("__rt_php_filter_mode_dirs");
    emitter.instruction("mov x9, #0");                                          // the directions found so far
    emitter.instruction("mov x10, #0");                                         // scan index
    emitter.label("__rt_pfmd_byte");
    emitter.instruction("cmp x10, x1");
    emitter.instruction("b.hs __rt_pfmd_done");                                 // the whole mode was read
    emitter.instruction("ldrb w11, [x0, x10]");
    emitter.instruction("cmp w11, #114");                                       // 'r'
    emitter.instruction("b.eq __rt_pfmd_read");
    emitter.instruction("cmp w11, #43");                                        // '+' names both
    emitter.instruction("b.eq __rt_pfmd_both");
    emitter.instruction("cmp w11, #119");                                       // 'w'
    emitter.instruction("b.eq __rt_pfmd_write");
    emitter.instruction("cmp w11, #97");                                        // 'a'
    emitter.instruction("b.eq __rt_pfmd_write");
    emitter.instruction("b __rt_pfmd_next");                                    // 'x', 'c', 'b', 't': neither
    emitter.label("__rt_pfmd_read");
    emitter.instruction("orr x9, x9, #1");
    emitter.instruction("b __rt_pfmd_next");
    emitter.label("__rt_pfmd_write");
    emitter.instruction("orr x9, x9, #2");
    emitter.instruction("b __rt_pfmd_next");
    emitter.label("__rt_pfmd_both");
    emitter.instruction("orr x9, x9, #3");
    emitter.label("__rt_pfmd_next");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("b __rt_pfmd_byte");
    emitter.label("__rt_pfmd_done");
    abi::emit_symbol_address(emitter, "x12", "_php_filter_open_dirs");
    emitter.instruction("str x9, [x12]");
    emitter.instruction("ret");
}

/// `__rt_php_filter_suppress_begin()` — silences the INNER opener, but only for a filter URL.
///
/// php-src's `php_stream_url_wrap_php` returns NULL the moment the inner resource fails to
/// open, BEFORE a single filter is created, and the generic caller composes one fixed line from
/// the URL it was handed. The inner opener would instead name itself and the bare resource with
/// its own errno, so it is silenced through `_php_filter_suppression`. Gating on the published
/// URL keeps a PLAIN open loud: it is the pairing partner of the pop in
/// `__rt_php_filter_open_failed`, which reads the same flag.
///
/// The counter is the filter machinery's OWN, not the one `@` raises. php-src silences the inner
/// open by dropping `REPORT_ERRORS` from the flags it passes down, which reaches that open's own
/// diagnostics and nothing else; PHP running underneath — a user wrapper's `stream_open` — warns
/// normally. A shared counter cannot say that, because standing it down for the wrapper would
/// also hand back a depth an enclosing `@` was holding. Two counters, and
/// `__rt_fopen`'s wrapper dispatch stands only this one down.
fn emit_suppress_begin_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: suppress the inner opener of a php://filter URL ---");
    emitter.label_global("__rt_php_filter_suppress_begin");
    // Save this open's URL in ITS OWN frame before anything nested can republish the hand-off,
    // and let a saved null stand for "not a filter URL" — which is also the record of whether a
    // suppression scope was opened, so the pop can never disagree with the push.
    abi::emit_symbol_address(emitter, "x9", "_php_filter_open_depth");
    emitter.instruction("ldr x10, [x9]");                                       // opens already in flight
    emitter.instruction(&format!("cmp x10, #{PHP_FILTER_OPEN_DEPTH_MAX}"));
    emitter.instruction("b.hs __rt_pfsb_too_deep");                             // past the bound: behave as unfiltered
    abi::emit_symbol_address(emitter, "x11", "_php_filter_url_ptr");
    emitter.instruction("ldr x12, [x11]");                                      // the URL this open must name
    abi::emit_symbol_address(emitter, "x13", "_php_filter_open_url_ptr");
    emitter.instruction("str x12, [x13, x10, lsl #3]");
    abi::emit_symbol_address(emitter, "x11", "_php_filter_url_len");
    emitter.instruction("ldr x14, [x11]");
    abi::emit_symbol_address(emitter, "x13", "_php_filter_open_url_len");
    emitter.instruction("str x14, [x13, x10, lsl #3]");
    emitter.instruction("b __rt_pfsb_saved");
    emitter.label("__rt_pfsb_too_deep");
    emitter.instruction("mov x12, #0");                                         // nothing saved, so nothing suppressed
    emitter.label("__rt_pfsb_saved");
    emitter.instruction("add x10, x10, #1");                                    // this open is now in flight
    emitter.instruction("str x10, [x9]");
    emitter.instruction("cbz x12, __rt_pfsb_done");                             // a plain path warns in its own words
    emitter.instruction("sub sp, sp, #16");
    emitter.instruction("str x30, [sp, #0]");                                   // the call below takes the link register
    emitter.instruction("bl __rt_diag_push_filter_suppression");
    emitter.instruction("ldr x30, [sp, #0]");
    emitter.instruction("add sp, sp, #16");
    emitter.label("__rt_pfsb_done");
    emitter.instruction("ret");
}

/// `__rt_php_filter_pending_save()` — parks the whole hand-off until this open is finished.
///
/// Called after the parse and before the opener, and paired with
/// [`emit_pending_restore_aarch64`] at the three places that consume a parse: the dynamic
/// `fopen()` exits, the path readers' filter route, and `file_put_contents`'s.
///
/// The opener can run PHP. A user wrapper's `stream_open` is a PHP method, and a method that
/// `fopen()`s anything re-enters the parse, which publishes over every slot in [`PENDING_STATE`].
/// The outer open then attached the INNER URL's chain — usually nothing, since the inner parse's
/// consumer has already cleared it — and reported the inner URL's unresolved names as its own.
///
/// Touches x9-x14 only: `fopen()` still holds the filename in x1/x2 and the mode in x3/x4 here,
/// and the restore runs with the boxed result live in x0.
fn emit_pending_save_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: park the php://filter hand-off for the length of one open ---");
    emitter.label_global("__rt_php_filter_pending_save");
    abi::emit_symbol_address(emitter, "x9", "_php_filter_pending_depth");
    emitter.instruction("ldr x10, [x9]");                                       // hand-offs already parked
    emitter.instruction("add x11, x10, #1");                                    // this one is now in flight
    emitter.instruction("str x11, [x9]");                                       // counted even when it cannot be parked
    emitter.instruction(&format!("cmp x10, #{PHP_FILTER_OPEN_DEPTH_MAX}"));
    emitter.instruction("b.hs __rt_pfpsv_done");                                // past the bound: nothing is parked
    abi::emit_symbol_address(emitter, "x12", "_php_filter_pending_stack");
    emitter.instruction(&format!(
        "add x12, x12, x10, lsl #{PHP_FILTER_PENDING_FRAME_SHIFT}"
    ));                                                                         // this open's own frame
    for (index, (symbol, words)) in PENDING_STATE.iter().enumerate() {
        abi::emit_symbol_address(emitter, "x11", symbol);
        if *words == 1 {
            emitter.instruction("ldr x14, [x11]");
            emitter.instruction("str x14, [x12]");                              // park the published value
            emitter.instruction("add x12, x12, #8");
            continue;
        }
        emitter.instruction("mov x13, #0");                                     // walk the published list
        emitter.label(&format!("__rt_pfpsv_w{index}"));
        emitter.instruction(&format!("cmp x13, #{words}"));
        emitter.instruction(&format!("b.hs __rt_pfpsv_w{index}_end"));
        emitter.instruction("ldr x14, [x11, x13, lsl #3]");
        emitter.instruction("str x14, [x12, x13, lsl #3]");
        emitter.instruction("add x13, x13, #1");
        emitter.instruction(&format!("b __rt_pfpsv_w{index}"));
        emitter.label(&format!("__rt_pfpsv_w{index}_end"));
        emitter.instruction(&format!("add x12, x12, #{}", words * 8));
    }
    emitter.label("__rt_pfpsv_done");
    emitter.instruction("ret");
}

/// `__rt_php_filter_pending_restore()` — republishes the hand-off this open parked.
///
/// Runs BEFORE the failed-open line, the attach and the unresolved-name report, because those
/// three read the globals and two of them clear what they consume: restoring after the failed-open
/// line would resurrect the names php drops for an open that never reached the filters.
///
/// Preserves x0, which carries the boxed open result across all three.
fn emit_pending_restore_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: republish the php://filter hand-off this open parked ---");
    emitter.label_global("__rt_php_filter_pending_restore");
    abi::emit_symbol_address(emitter, "x9", "_php_filter_pending_depth");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("cbz x10, __rt_pfprs_done");                            // nothing parked to republish
    emitter.instruction("sub x10, x10, #1");
    emitter.instruction("str x10, [x9]");
    emitter.instruction(&format!("cmp x10, #{PHP_FILTER_OPEN_DEPTH_MAX}"));
    emitter.instruction("b.hs __rt_pfprs_done");                                // past the bound: nothing was parked
    abi::emit_symbol_address(emitter, "x12", "_php_filter_pending_stack");
    emitter.instruction(&format!(
        "add x12, x12, x10, lsl #{PHP_FILTER_PENDING_FRAME_SHIFT}"
    ));                                                                         // this open's own frame
    for (index, (symbol, words)) in PENDING_STATE.iter().enumerate() {
        abi::emit_symbol_address(emitter, "x11", symbol);
        if *words == 1 {
            emitter.instruction("ldr x14, [x12]");
            emitter.instruction("str x14, [x11]");                              // republish the parked value
            emitter.instruction("add x12, x12, #8");
            continue;
        }
        emitter.instruction("mov x13, #0");
        emitter.label(&format!("__rt_pfprs_w{index}"));
        emitter.instruction(&format!("cmp x13, #{words}"));
        emitter.instruction(&format!("b.hs __rt_pfprs_w{index}_end"));
        emitter.instruction("ldr x14, [x12, x13, lsl #3]");
        emitter.instruction("str x14, [x11, x13, lsl #3]");
        emitter.instruction("add x13, x13, #1");
        emitter.instruction(&format!("b __rt_pfprs_w{index}"));
        emitter.label(&format!("__rt_pfprs_w{index}_end"));
        emitter.instruction(&format!("add x12, x12, #{}", words * 8));
    }
    emitter.label("__rt_pfprs_done");
    emitter.instruction("ret");
}

/// `__rt_php_filter_open_failed(x0 = boxed result, x1 = callee pointer, x2 = callee length) -> x0`.
///
/// Ends the suppression `__rt_php_filter_suppress_begin` opened and, when the open failed,
/// composes php's own line: `<callee>(<WHOLE URL>): Failed to open stream: operation failed`.
/// The URL is what php names — the swap replaced the caller's filename with the RESOURCE, so
/// the inner opener could only ever have named a path the program never wrote.
///
/// A failed open never reaches the filters, so it also drops the unresolved names: php prints
/// the failed-open line ALONE for `php://filter/read=no.such/resource=missing.txt`.
fn emit_open_failed_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: a failed php://filter open names the URL ---");
    emitter.label_global("__rt_php_filter_open_failed");
    // Frame: [0]=boxed result [8]=callee pointer [16]=callee length [24]=URL pointer
    //        [32]=URL length, saved pair at [48].
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");
    emitter.instruction("add x29, sp, #48");
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction("str x1, [sp, #8]");
    emitter.instruction("str x2, [sp, #16]");
    // Pop THIS open's frame. Reading the global hand-off instead would read whatever a nested
    // open republished, and could pop a suppression this open never pushed.
    abi::emit_symbol_address(emitter, "x9", "_php_filter_open_depth");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("cbz x10, __rt_pfof_done");                             // no open in flight to close
    emitter.instruction("sub x10, x10, #1");
    emitter.instruction("str x10, [x9]");
    emitter.instruction(&format!("cmp x10, #{PHP_FILTER_OPEN_DEPTH_MAX}"));
    emitter.instruction("b.hs __rt_pfof_done");                                 // past the bound: nothing was saved
    abi::emit_symbol_address(emitter, "x11", "_php_filter_open_url_ptr");
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // the URL this open saved
    abi::emit_symbol_address(emitter, "x11", "_php_filter_open_url_len");
    emitter.instruction("ldr x13, [x11, x10, lsl #3]");
    emitter.instruction("cbz x12, __rt_pfof_done");                             // not a filter URL: nothing was suppressed
    emitter.instruction("str x12, [sp, #24]");
    emitter.instruction("str x13, [sp, #32]");
    emitter.instruction("bl __rt_diag_pop_filter_suppression");
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("ldr x9, [x0]");                                        // the boxed open result tag
    emitter.instruction("cmp x9, #9");                                          // a resource has nothing to warn about
    emitter.instruction("b.eq __rt_pfof_done");
    emit_warning_fragment_aarch64(emitter, "_pf_w_head", PF_WARN_HEAD.len());
    emitter.instruction("ldr x1, [sp, #8]");                                    // the CALLING function's name
    emitter.instruction("ldr x2, [sp, #16]");
    emitter.instruction("bl __rt_diag_warning");
    emit_warning_fragment_aarch64(emitter, "_pf_w_open_mid", PF_WARN_OPEN_MID.len());
    emitter.instruction("ldr x1, [sp, #24]");                                   // the WHOLE URL, not the resource
    emitter.instruction("ldr x2, [sp, #32]");
    emitter.instruction("bl __rt_diag_warning");
    emit_warning_fragment_aarch64(emitter, "_fgc_filter_fail_tail", FGC_FILTER_FAIL_TAIL.len());
    abi::emit_symbol_address(emitter, "x9", "_php_filter_unknown_count");
    emitter.instruction("str xzr, [x9]");                                       // a failed open never reaches the filters
    emitter.label("__rt_pfof_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // hand the boxed result straight back
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// `__rt_php_filter_unknown_report(x0 = boxed result, x1 = callee pointer, x2 = callee length) -> x0`.
///
/// Warns twice for every name the URL spelled that named no filter, and STILL keeps the stream:
/// php-src prints one line from `php_stream_filter_create` (main/streams/filter.c) and the next
/// from `php_stream_apply_filter_list`, and neither cancels the open.
///
/// The count is not one pair per name. php walks the list once per DIRECTION it applies, so a
/// prefix-less chain opened `r+` warns twice per name and the same chain opened `x` warns not at
/// all, while an explicit `read=`/`write=` list is applied exactly once whatever the mode.
///
/// Clears the whole hand-off — including the URL flag — because exactly one open consumes it.
fn emit_unknown_report_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: warn for every php://filter name that named no filter ---");
    emitter.label_global("__rt_php_filter_unknown_report");
    // Frame: [0]=boxed result [8]=callee pointer [16]=callee length [24]=name count
    //        [32]=name index [40]=attempts per name [48]=attempt index, saved pair at [64].
    emitter.instruction("sub sp, sp, #80");
    emitter.instruction("stp x29, x30, [sp, #64]");
    emitter.instruction("add x29, sp, #64");
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction("str x1, [sp, #8]");
    emitter.instruction("str x2, [sp, #16]");
    abi::emit_symbol_address(emitter, "x9", "_php_filter_unknown_count");
    emitter.instruction("ldr x10, [x9]");                                       // how many names resolved to nothing
    emitter.instruction("str xzr, [x9]");                                       // exactly one open consumes them
    emitter.instruction("str x10, [sp, #24]");
    abi::emit_symbol_address(emitter, "x9", "_php_filter_url_ptr");
    emitter.instruction("str xzr, [x9]");                                       // and one open consumes the URL flag
    emitter.instruction("cbz x10, __rt_pfur_done");                             // every name resolved
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("ldr x9, [x0]");                                        // the boxed open result tag
    emitter.instruction("cmp x9, #9");
    emitter.instruction("b.ne __rt_pfur_done");                                 // a failed open never reaches the filters

    // -- how many times php walks this list --
    abi::emit_symbol_address(emitter, "x9", "_php_filter_url_dir");
    emitter.instruction("ldr x9, [x9]");
    emitter.instruction("cmp x9, #3");                                          // did the URL spell a direction itself?
    emitter.instruction("b.ne __rt_pfur_once");                                 // an explicit list is applied exactly once
    abi::emit_symbol_address(emitter, "x10", "_php_filter_open_dirs");
    emitter.instruction("ldr x10, [x10]");
    emitter.instruction("and x11, x10, #1");                                    // the read pass
    emitter.instruction("lsr x12, x10, #1");
    emitter.instruction("and x12, x12, #1");                                    // the write pass
    emitter.instruction("add x11, x11, x12");
    emitter.instruction("b __rt_pfur_attempts_ready");
    emitter.label("__rt_pfur_once");
    emitter.instruction("mov x11, #1");
    emitter.label("__rt_pfur_attempts_ready");
    emitter.instruction("str x11, [sp, #40]");
    emitter.instruction("str xzr, [sp, #32]");

    emitter.label("__rt_pfur_name");
    emitter.instruction("ldr x9, [sp, #32]");
    emitter.instruction("ldr x10, [sp, #24]");
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.hs __rt_pfur_done");                                 // every name reported
    emitter.instruction("str xzr, [sp, #48]");
    emitter.label("__rt_pfur_attempt");
    emitter.instruction("ldr x9, [sp, #48]");
    emitter.instruction("ldr x10, [sp, #40]");
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.hs __rt_pfur_name_next");
    // -- `Warning: <callee>(): Unable to locate filter "<name>"` --
    emit_warning_fragment_aarch64(emitter, "_pf_w_head", PF_WARN_HEAD.len());
    emit_callee_fragment_aarch64(emitter);
    emit_warning_fragment_aarch64(emitter, "_pf_w_locate_mid", PF_WARN_LOCATE_MID.len());
    emit_unknown_name_fragment_aarch64(emitter);
    emit_warning_fragment_aarch64(emitter, "_pf_w_locate_end", PF_WARN_LOCATE_END.len());
    // -- `Warning: <callee>(): Unable to create filter (<name>)` --
    emit_warning_fragment_aarch64(emitter, "_pf_w_head", PF_WARN_HEAD.len());
    emit_callee_fragment_aarch64(emitter);
    emit_warning_fragment_aarch64(emitter, "_pf_w_create_mid", PF_WARN_CREATE_MID.len());
    emit_unknown_name_fragment_aarch64(emitter);
    emit_warning_fragment_aarch64(emitter, "_pf_w_create_end", PF_WARN_CREATE_END.len());
    emitter.instruction("ldr x9, [sp, #48]");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [sp, #48]");
    emitter.instruction("b __rt_pfur_attempt");

    emitter.label("__rt_pfur_name_next");
    emitter.instruction("ldr x9, [sp, #32]");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [sp, #32]");
    emitter.instruction("b __rt_pfur_name");

    emitter.label("__rt_pfur_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // hand the boxed result straight back
    emitter.instruction("ldp x29, x30, [sp, #64]");
    emitter.instruction("add sp, sp, #80");
    emitter.instruction("ret");
}

/// Writes one interned fragment through `__rt_diag_warning`, so `@` silences it like any warning.
fn emit_warning_fragment_aarch64(emitter: &mut Emitter, symbol: &str, len: usize) {
    abi::emit_symbol_address(emitter, "x1", symbol);
    emitter.instruction(&format!("mov x2, #{len}"));
    emitter.instruction("bl __rt_diag_warning");                                // clobbers x0/x9/x10: everything is reloaded
}

/// Writes the calling function's name, reloaded because the warning call destroys the pair.
fn emit_callee_fragment_aarch64(emitter: &mut Emitter) {
    emitter.instruction("ldr x1, [sp, #8]");
    emitter.instruction("ldr x2, [sp, #16]");
    emitter.instruction("bl __rt_diag_warning");
}

/// Writes the span of the name currently being reported.
fn emit_unknown_name_fragment_aarch64(emitter: &mut Emitter) {
    emitter.instruction("ldr x9, [sp, #32]");                                   // which name this pass reports
    abi::emit_symbol_address(emitter, "x10", "_php_filter_unknown_ptr");
    emitter.instruction("ldr x1, [x10, x9, lsl #3]");
    abi::emit_symbol_address(emitter, "x10", "_php_filter_unknown_len");
    emitter.instruction("ldr x2, [x10, x9, lsl #3]");
    emitter.instruction("bl __rt_diag_warning");
}

/// x86_64 form of [`emit_prefix_match_aarch64`].
///
/// `__rt_pf_match(rdi = haystack, rsi = length, rdx = needle, rcx = needle length) -> rax = 0/1`.
fn emit_prefix_match_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: does a byte range start with a needle ---");
    emitter.label_global("__rt_pf_match");
    emitter.instruction("cmp rsi, rcx");                                        // enough bytes to hold the needle?
    emitter.instruction("jl __rt_pfm_no_x");                                    // too short to start with it
    emitter.instruction("xor r9, r9");                                          // comparison index
    emitter.label("__rt_pfm_byte_x");
    emitter.instruction("cmp r9, rcx");                                         // compared the whole needle?
    emitter.instruction("jae __rt_pfm_yes_x");                                  // every byte agreed
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // one haystack byte
    emitter.instruction("movzx r10d, BYTE PTR [rdx + r9]");                     // the corresponding needle byte
    emitter.instruction("cmp al, r10b");                                        // do they agree?
    emitter.instruction("jne __rt_pfm_no_x");                                   // a mismatch ends it
    emitter.instruction("add r9, 1");                                           // advance the comparison index
    emitter.instruction("jmp __rt_pfm_byte_x");                                 // keep comparing
    emitter.label("__rt_pfm_yes_x");
    emitter.instruction("mov rax, 1");                                          // the range starts with the needle
    emitter.instruction("ret");
    emitter.label("__rt_pfm_no_x");
    emitter.instruction("xor eax, eax");                                        // it does not
    emitter.instruction("ret");
}

/// x86_64 form of [`emit_filter_parse_aarch64`].
///
/// `__rt_php_filter_parse(rdi = path, rsi = length) -> rax = 1 when a filter URL was parsed`.
fn emit_filter_parse_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: parse a run-time php://filter URL ---");
    emitter.label_global("__rt_php_filter_parse");
    // Frame: [rbp-8]=cursor [rbp-16]=remaining [rbp-24]=direction [rbp-32]=scan index / name length
    //        [rbp-40]=segment start [rbp-48]=filters resolved [rbp-56]=separator offset
    //        [rbp-64]=segment pointer [rbp-72]=segment length [rbp-80]=unresolved names
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the parse frame
    emitter.instruction("sub rsp, 96");                                         // reserve the spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the path
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve its length
    abi::emit_symbol_address(emitter, "rdx", "_pf_n_prefix");
    emitter.instruction("mov rcx, 13");                                         // "php://filter/"
    emitter.instruction("call __rt_pf_match");                                  // is this a filter URL at all?
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_pfp_no_x");                                    // no: leave the path alone
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    // See the AArch64 counterpart: the caller opens the RESOURCE, php names the URL.
    abi::emit_symbol_address(emitter, "r8", "_php_filter_url_ptr");
    emitter.instruction("mov QWORD PTR [r8], rdi");                             // the URL the program actually wrote
    abi::emit_symbol_address(emitter, "r8", "_php_filter_url_len");
    emitter.instruction("mov QWORD PTR [r8], rsi");                             // and its length
    emitter.instruction("add rdi, 13");                                         // step past the scheme
    emitter.instruction("sub rsi, 13");                                         // and shorten the remaining count
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the cursor now sits on the direction
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");

    emitter.instruction("mov QWORD PTR [rbp - 24], 3");                         // no prefix means both directions
    abi::emit_symbol_address(emitter, "rdx", "_pf_n_read");
    emitter.instruction("mov rcx, 5");                                          // "read="
    emitter.instruction("call __rt_pf_match");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_pfp_try_write_x");                             // not a read-only URL
    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // read direction
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("add rdi, 5");                                          // step past "read="
    emitter.instruction("sub rsi, 5");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");
    emitter.instruction("jmp __rt_pfp_find_resource_x");

    emitter.label("__rt_pfp_try_write_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    abi::emit_symbol_address(emitter, "rdx", "_pf_n_write");
    emitter.instruction("mov rcx, 6");                                          // "write="
    emitter.instruction("call __rt_pf_match");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_pfp_find_resource_x");                         // neither prefix: both directions
    emitter.instruction("mov QWORD PTR [rbp - 24], 2");                         // write direction
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("add rdi, 6");                                          // step past "write="
    emitter.instruction("sub rsi, 6");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");

    emitter.label("__rt_pfp_find_resource_x");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // scan index
    emitter.label("__rt_pfp_scan_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // the scan index
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // bytes remaining after the direction
    emitter.instruction("lea r10, [r9 + 10]");                                  // does "/resource=" still fit here?
    emitter.instruction("cmp r10, rsi");
    emitter.instruction("jg __rt_pfp_no_resource_x");                           // ran out: the URL names no resource
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the filter-name cursor
    emitter.instruction("add rdi, r9");                                         // the candidate separator position
    emitter.instruction("sub rsi, r9");                                         // bytes left from there
    abi::emit_symbol_address(emitter, "rdx", "_pf_n_resource");
    emitter.instruction("mov rcx, 10");                                         // "/resource="
    emitter.instruction("call __rt_pf_match");
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_pfp_found_x");                                // the separator starts here
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");
    emitter.instruction("add r9, 1");                                           // keep scanning
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");
    emitter.instruction("jmp __rt_pfp_scan_x");

    emitter.label("__rt_pfp_found_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // the separator offset IS the name length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the filter name starts at the cursor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // bytes after the direction
    emitter.instruction("lea r10, [rdi + r9]");                                 // the separator
    emitter.instruction("add r10, 10");                                         // the resource begins after it
    emitter.instruction("mov r11, rsi");
    emitter.instruction("sub r11, r9");                                         // bytes from the separator on
    emitter.instruction("sub r11, 10");                                         // minus the separator itself
    emitter.instruction("cmp r11, 1");                                          // an empty resource names nothing
    emitter.instruction("jl __rt_pfp_no_resource_x");                           // php throws for it, and the caller does the throwing
    abi::emit_symbol_address(emitter, "r8", "_php_filter_res_ptr");
    emitter.instruction("mov QWORD PTR [r8], r10");                             // publish the resource pointer
    abi::emit_symbol_address(emitter, "r8", "_php_filter_res_len");
    emitter.instruction("mov QWORD PTR [r8], r11");                             // and its length
    emitter.instruction("mov rdi, r10");
    emitter.instruction("mov rsi, r11");
    abi::emit_symbol_address(emitter, "rdx", "_pf_n_prefix");
    emitter.instruction("mov rcx, 12");                                         // "php://filter" without the slash
    emitter.instruction("call __rt_pf_match");
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_pfp_no_x");                                   // nested filters are not supported

    // -- resolve EVERY name in the `|` chain, in order, the way the literal path does --
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // the current segment's start offset
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // filters resolved so far
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // names that resolved to nothing

    emitter.label("__rt_pfp_seg_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // the full name length
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // where this segment starts
    emitter.instruction("cmp r10, r9");
    emitter.instruction("jae __rt_pfp_publish_x");                              // past the last segment
    // Measure this segment: it ends at the next '|', or at the end of the name.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the name
    emitter.instruction("mov r11, r10");                                        // scan index, from the segment start
    emitter.label("__rt_pfp_pipe_x");
    emitter.instruction("cmp r11, r9");                                         // reached the end of the name?
    emitter.instruction("jae __rt_pfp_seg_end_x");                              // no further pipe: the segment runs to it
    emitter.instruction("movzx eax, BYTE PTR [rdi + r11]");
    emitter.instruction("cmp eax, 124");                                        // ASCII '|'
    emitter.instruction("je __rt_pfp_seg_end_x");                               // this one closes the segment
    emitter.instruction("add r11, 1");
    emitter.instruction("jmp __rt_pfp_pipe_x");

    emitter.label("__rt_pfp_seg_end_x");
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // remember where the separator sits
    emitter.instruction("mov rsi, r11");
    emitter.instruction("sub rsi, r10");                                        // this segment's length
    emitter.instruction("jz __rt_pfp_seg_next_x");                              // an empty segment names nothing
    emitter.instruction("add rdi, r10");                                        // the segment's first byte
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // remember the span: the id lookup destroys both
    emitter.instruction("mov QWORD PTR [rbp - 72], rsi");
    emitter.instruction("call __rt_builtin_filter_id");                         // rax = the built-in id, or 0
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_pfp_seg_known_x");                            // it named a built-in filter
    // See the AArch64 counterpart: skipped is not the same as silent.
    emitter.instruction("mov r11, QWORD PTR [rbp - 80]");                       // names recorded so far
    emitter.instruction(&format!("cmp r11, {PHP_FILTER_PENDING_MAX}"));
    emitter.instruction("jae __rt_pfp_seg_next_x");                             // the report array is full
    abi::emit_symbol_address(emitter, "r8", "_php_filter_unknown_ptr");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");
    emitter.instruction("mov QWORD PTR [r8 + r11 * 8], rax");                   // where the name starts
    abi::emit_symbol_address(emitter, "r8", "_php_filter_unknown_len");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");
    emitter.instruction("mov QWORD PTR [r8 + r11 * 8], rax");                   // and how long it is
    emitter.instruction("add r11, 1");
    emitter.instruction("mov QWORD PTR [rbp - 80], r11");
    emitter.instruction("jmp __rt_pfp_seg_next_x");

    emitter.label("__rt_pfp_seg_known_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // filters resolved so far
    emitter.instruction(&format!("cmp r11, {PHP_FILTER_PENDING_MAX}"));
    emitter.instruction("jae __rt_pfp_seg_next_x");                             // the hand-off array is full
    abi::emit_symbol_address(emitter, "r8", "_php_filter_pending_ids");
    emitter.instruction("mov QWORD PTR [r8 + r11 * 8], rax");                   // append this filter to the list
    emitter.instruction("add r11, 1");
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");

    emitter.label("__rt_pfp_seg_next_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // the separator this segment ended on
    emitter.instruction("add r11, 1");                                          // the next segment starts after it
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");
    emitter.instruction("jmp __rt_pfp_seg_x");

    emitter.label("__rt_pfp_publish_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // how many filters resolved
    abi::emit_symbol_address(emitter, "r8", "_php_filter_pending_count");
    emitter.instruction("mov QWORD PTR [r8], r11");                             // publish the count
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // the requested direction
    // See the AArch64 counterpart: the URL's own direction survives the zeroing below.
    abi::emit_symbol_address(emitter, "r8", "_php_filter_url_dir");
    emitter.instruction("mov QWORD PTR [r8], r9");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // names that resolved to nothing
    abi::emit_symbol_address(emitter, "r8", "_php_filter_unknown_count");
    emitter.instruction("mov QWORD PTR [r8], rax");                             // publish them for the report
    emitter.instruction("xor r10, r10");
    emitter.instruction("test r11, r11");                                       // did ANY name resolve?
    emitter.instruction("cmove r9, r10");                                       // a chain of unknowns attaches nothing
    abi::emit_symbol_address(emitter, "r8", "_php_filter_pending_mode");
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish the direction
    emitter.instruction("mov rax, 1");                                          // the caller should open the resource
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");

    emitter.label("__rt_pfp_no_x");
    emit_clear_parse_state_x86_64(emitter);
    emitter.instruction("xor eax, eax");                                        // the path is not a usable filter URL
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");

    // See the AArch64 counterpart: verdict 2 asks the caller to throw php's Error.
    emitter.label("__rt_pfp_no_resource_x");
    emit_clear_parse_state_x86_64(emitter);
    emitter.instruction("mov eax, 2");                                          // the caller must throw php's Error
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// x86_64 form of [`emit_clear_parse_state_aarch64`].
fn emit_clear_parse_state_x86_64(emitter: &mut Emitter) {
    for symbol in [
        "_php_filter_pending_count",
        "_php_filter_pending_mode",
        "_php_filter_unknown_count",
        "_php_filter_url_ptr",
        "_php_filter_url_len",
        "_php_filter_url_dir",
    ] {
        abi::emit_symbol_address(emitter, "r8", symbol);
        emitter.instruction("mov QWORD PTR [r8], 0");                           // nothing is pending
    }
}

/// x86_64 form of [`emit_filter_attach_aarch64`].
///
/// `__rt_php_filter_attach_pending(rax = boxed fopen result)`; returns it unchanged in rax.
fn emit_filter_attach_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: attach the filter a php://filter URL named ---");
    emitter.label_global("__rt_php_filter_attach_pending");
    // Frame: [rbp-8]=boxed result [rbp-16]=stream handle [rbp-24]=filter handle [rbp-32]=direction
    //        [rbp-40]=list index [rbp-48]=filters published
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the attach frame
    emitter.instruction("sub rsp, 48");                                         // reserve the spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the boxed result
    abi::emit_symbol_address(emitter, "r9", "_php_filter_pending_mode");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // the direction the URL asked for
    emitter.instruction("mov QWORD PTR [r9], 0");                               // clear it: exactly one open consumes it
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");
    abi::emit_symbol_address(emitter, "r9", "_php_filter_pending_count");
    emitter.instruction("mov r11, QWORD PTR [r9]");                             // how many filters the URL named
    emitter.instruction("mov QWORD PTR [r9], 0");                               // cleared for the same reason
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_pfa_done_x");                                  // no direction: nothing to attach
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_pfa_done_x");                                  // no filter: the resource opened plain
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    emitter.instruction("cmp QWORD PTR [rax], 9");                              // did the open produce a resource?
    emitter.instruction("jne __rt_pfa_done_x");                                 // a false result carries no stream
    emitter.instruction("mov r9, QWORD PTR [rax + 8]");                         // the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");

    // Attach in list order: php runs the bytes through the filters the way the URL spelled them,
    // and `__rt_stream_filter_link` appends at the tail, so creating in order builds that chain.
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // the first filter in the list
    emitter.label("__rt_pfa_next_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // which filter this pass attaches
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // how many there are
    emitter.instruction("cmp r9, r10");
    emitter.instruction("jae __rt_pfa_done_x");                                 // the whole chain is attached
    abi::emit_symbol_address(emitter, "r11", "_php_filter_pending_ids");
    emitter.instruction("mov rdi, QWORD PTR [r11 + r9 * 8]");                   // the built-in filter id
    emitter.instruction("add r9, 1");                                           // advance before the calls clobber it
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");
    emitter.instruction("xor esi, esi");                                        // built-ins carry no user-filter object
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // direction bits from the URL
    emitter.instruction("xor ecx, ecx");                                        // built-ins retain no params value
    abi::emit_call_label(emitter, "__rt_filter_create");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the filter handle
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");
    emitter.instruction("test r10, 1");                                         // does it filter reads?
    emitter.instruction("jz __rt_pfa_write_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // stream handle
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // filter handle
    emitter.instruction(&format!("mov rdx, {STREAM_READ_FILTER_HEAD_OFFSET}"));
    emitter.instruction("xor ecx, ecx");                                        // append at the chain tail
    abi::emit_call_label(emitter, "__rt_stream_filter_link");
    emitter.label("__rt_pfa_write_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");
    emitter.instruction("test r10, 2");                                         // does it filter writes?
    emitter.instruction("jz __rt_pfa_next_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // stream handle
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // filter handle
    emitter.instruction(&format!("mov rdx, {STREAM_WRITE_FILTER_HEAD_OFFSET}"));
    emitter.instruction("xor ecx, ecx");                                        // append at the chain tail
    abi::emit_call_label(emitter, "__rt_stream_filter_link");
    emitter.instruction("jmp __rt_pfa_next_x");                                 // on to the next filter in the chain

    emitter.label("__rt_pfa_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // hand the boxed result straight back
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// x86_64 form of [`emit_mode_dirs_aarch64`].
///
/// `__rt_php_filter_mode_dirs(rdi = mode pointer, rsi = mode length)`.
fn emit_mode_dirs_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: directions an fopen mode string selects ---");
    emitter.label_global("__rt_php_filter_mode_dirs");
    emitter.instruction("xor r9, r9");                                          // the directions found so far
    emitter.instruction("xor r10, r10");                                        // scan index
    emitter.label("__rt_pfmd_byte_x");
    emitter.instruction("cmp r10, rsi");
    emitter.instruction("jae __rt_pfmd_done_x");                                // the whole mode was read
    emitter.instruction("movzx eax, BYTE PTR [rdi + r10]");
    emitter.instruction("cmp al, 114");                                         // 'r'
    emitter.instruction("je __rt_pfmd_read_x");
    emitter.instruction("cmp al, 43");                                          // '+' names both
    emitter.instruction("je __rt_pfmd_both_x");
    emitter.instruction("cmp al, 119");                                         // 'w'
    emitter.instruction("je __rt_pfmd_write_x");
    emitter.instruction("cmp al, 97");                                          // 'a'
    emitter.instruction("je __rt_pfmd_write_x");
    emitter.instruction("jmp __rt_pfmd_next_x");                                // 'x', 'c', 'b', 't': neither
    emitter.label("__rt_pfmd_read_x");
    emitter.instruction("or r9, 1");
    emitter.instruction("jmp __rt_pfmd_next_x");
    emitter.label("__rt_pfmd_write_x");
    emitter.instruction("or r9, 2");
    emitter.instruction("jmp __rt_pfmd_next_x");
    emitter.label("__rt_pfmd_both_x");
    emitter.instruction("or r9, 3");
    emitter.label("__rt_pfmd_next_x");
    emitter.instruction("add r10, 1");
    emitter.instruction("jmp __rt_pfmd_byte_x");
    emitter.label("__rt_pfmd_done_x");
    abi::emit_symbol_address(emitter, "r8", "_php_filter_open_dirs");
    emitter.instruction("mov QWORD PTR [r8], r9");
    emitter.instruction("ret");
}

/// x86_64 form of [`emit_suppress_begin_aarch64`].
fn emit_suppress_begin_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: suppress the inner opener of a php://filter URL ---");
    emitter.label_global("__rt_php_filter_suppress_begin");
    // See the AArch64 counterpart. `rax`/`rdx` carry the filename and `rdi`/`rsi` the mode at
    // this point, so only r8-r11 are touched here.
    abi::emit_symbol_address(emitter, "r8", "_php_filter_open_depth");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // opens already in flight
    emitter.instruction(&format!("cmp r9, {PHP_FILTER_OPEN_DEPTH_MAX}"));
    emitter.instruction("jae __rt_pfsb_too_deep_x");                            // past the bound: behave as unfiltered
    abi::emit_symbol_address(emitter, "r10", "_php_filter_url_ptr");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // the URL this open must name
    abi::emit_symbol_address(emitter, "r10", "_php_filter_open_url_ptr");
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], r11");
    abi::emit_symbol_address(emitter, "r10", "_php_filter_url_len");
    emitter.instruction("mov r10, QWORD PTR [r10]");
    abi::emit_symbol_address(emitter, "r8", "_php_filter_open_url_len");
    emitter.instruction("mov QWORD PTR [r8 + r9 * 8], r10");
    abi::emit_symbol_address(emitter, "r8", "_php_filter_open_depth");
    emitter.instruction("jmp __rt_pfsb_saved_x");
    emitter.label("__rt_pfsb_too_deep_x");
    emitter.instruction("xor r11, r11");                                        // nothing saved, so nothing suppressed
    emitter.label("__rt_pfsb_saved_x");
    emitter.instruction("add r9, 1");                                           // this open is now in flight
    emitter.instruction("mov QWORD PTR [r8], r9");
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_pfsb_done_x");                                 // a plain path warns in its own words
    emitter.instruction("call __rt_diag_push_filter_suppression");
    emitter.label("__rt_pfsb_done_x");
    emitter.instruction("ret");
}

/// x86_64 form of [`emit_pending_save_aarch64`].
///
/// `__rt_php_filter_pending_save()`. Touches r8-r11 only: `rax`/`rdx` carry the filename and
/// `rdi`/`rsi` the mode at the call site, exactly as for the suppression above.
fn emit_pending_save_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: park the php://filter hand-off for the length of one open ---");
    emitter.label_global("__rt_php_filter_pending_save");
    abi::emit_symbol_address(emitter, "r8", "_php_filter_pending_depth");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // hand-offs already parked
    emitter.instruction("lea r10, [r9 + 1]");                                   // this one is now in flight
    emitter.instruction("mov QWORD PTR [r8], r10");                             // counted even when it cannot be parked
    emitter.instruction(&format!("cmp r9, {PHP_FILTER_OPEN_DEPTH_MAX}"));
    emitter.instruction("jae __rt_pfpsv_done_x");                               // past the bound: nothing is parked
    emitter.instruction(&format!("shl r9, {PHP_FILTER_PENDING_FRAME_SHIFT}"));
    abi::emit_symbol_address(emitter, "r10", "_php_filter_pending_stack");
    emitter.instruction("add r10, r9");                                         // this open's own frame
    for (index, (symbol, words)) in PENDING_STATE.iter().enumerate() {
        abi::emit_symbol_address(emitter, "r8", symbol);
        if *words == 1 {
            emitter.instruction("mov r9, QWORD PTR [r8]");
            emitter.instruction("mov QWORD PTR [r10], r9");                     // park the published value
            emitter.instruction("add r10, 8");
            continue;
        }
        emitter.instruction("xor r11, r11");                                    // walk the published list
        emitter.label(&format!("__rt_pfpsv_w{index}_x"));
        emitter.instruction(&format!("cmp r11, {words}"));
        emitter.instruction(&format!("jae __rt_pfpsv_w{index}_end_x"));
        emitter.instruction("mov r9, QWORD PTR [r8 + r11 * 8]");
        emitter.instruction("mov QWORD PTR [r10 + r11 * 8], r9");
        emitter.instruction("add r11, 1");
        emitter.instruction(&format!("jmp __rt_pfpsv_w{index}_x"));
        emitter.label(&format!("__rt_pfpsv_w{index}_end_x"));
        emitter.instruction(&format!("add r10, {}", words * 8));
    }
    emitter.label("__rt_pfpsv_done_x");
    emitter.instruction("ret");
}

/// x86_64 form of [`emit_pending_restore_aarch64`].
///
/// `__rt_php_filter_pending_restore()`; preserves `rax`, which carries the boxed open result.
fn emit_pending_restore_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: republish the php://filter hand-off this open parked ---");
    emitter.label_global("__rt_php_filter_pending_restore");
    abi::emit_symbol_address(emitter, "r8", "_php_filter_pending_depth");
    emitter.instruction("mov r9, QWORD PTR [r8]");
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_pfprs_done_x");                                // nothing parked to republish
    emitter.instruction("sub r9, 1");
    emitter.instruction("mov QWORD PTR [r8], r9");
    emitter.instruction(&format!("cmp r9, {PHP_FILTER_OPEN_DEPTH_MAX}"));
    emitter.instruction("jae __rt_pfprs_done_x");                               // past the bound: nothing was parked
    emitter.instruction(&format!("shl r9, {PHP_FILTER_PENDING_FRAME_SHIFT}"));
    abi::emit_symbol_address(emitter, "r10", "_php_filter_pending_stack");
    emitter.instruction("add r10, r9");                                         // this open's own frame
    for (index, (symbol, words)) in PENDING_STATE.iter().enumerate() {
        abi::emit_symbol_address(emitter, "r8", symbol);
        if *words == 1 {
            emitter.instruction("mov r9, QWORD PTR [r10]");
            emitter.instruction("mov QWORD PTR [r8], r9");                      // republish the parked value
            emitter.instruction("add r10, 8");
            continue;
        }
        emitter.instruction("xor r11, r11");
        emitter.label(&format!("__rt_pfprs_w{index}_x"));
        emitter.instruction(&format!("cmp r11, {words}"));
        emitter.instruction(&format!("jae __rt_pfprs_w{index}_end_x"));
        emitter.instruction("mov r9, QWORD PTR [r10 + r11 * 8]");
        emitter.instruction("mov QWORD PTR [r8 + r11 * 8], r9");
        emitter.instruction("add r11, 1");
        emitter.instruction(&format!("jmp __rt_pfprs_w{index}_x"));
        emitter.label(&format!("__rt_pfprs_w{index}_end_x"));
        emitter.instruction(&format!("add r10, {}", words * 8));
    }
    emitter.label("__rt_pfprs_done_x");
    emitter.instruction("ret");
}

/// x86_64 form of [`emit_open_failed_aarch64`].
///
/// `__rt_php_filter_open_failed(rax = boxed result, rdi = callee pointer, rsi = callee length)`;
/// answers the boxed result unchanged in `rax`.
fn emit_open_failed_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: a failed php://filter open names the URL ---");
    emitter.label_global("__rt_php_filter_open_failed");
    // Frame: [rbp-8]=boxed result [rbp-16]=callee pointer [rbp-24]=callee length
    //        [rbp-32]=URL pointer [rbp-40]=URL length.
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 48");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");
    // See the AArch64 counterpart: pop THIS open's frame, never the global hand-off.
    abi::emit_symbol_address(emitter, "r8", "_php_filter_open_depth");
    emitter.instruction("mov r9, QWORD PTR [r8]");
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_pfof_done_x");                                 // no open in flight to close
    emitter.instruction("sub r9, 1");
    emitter.instruction("mov QWORD PTR [r8], r9");
    emitter.instruction(&format!("cmp r9, {PHP_FILTER_OPEN_DEPTH_MAX}"));
    emitter.instruction("jae __rt_pfof_done_x");                                // past the bound: nothing was saved
    abi::emit_symbol_address(emitter, "r10", "_php_filter_open_url_ptr");
    emitter.instruction("mov r11, QWORD PTR [r10 + r9 * 8]");                   // the URL this open saved
    abi::emit_symbol_address(emitter, "r10", "_php_filter_open_url_len");
    emitter.instruction("mov r10, QWORD PTR [r10 + r9 * 8]");
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_pfof_done_x");                                 // not a filter URL: nothing was suppressed
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");
    emitter.instruction("call __rt_diag_pop_filter_suppression");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    emitter.instruction("cmp QWORD PTR [rax], 9");                              // a resource has nothing to warn about
    emitter.instruction("je __rt_pfof_done_x");
    emit_warning_fragment_x86_64(emitter, "_pf_w_head", PF_WARN_HEAD.len());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the CALLING function's name
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");
    emitter.instruction("call __rt_diag_warning");
    emit_warning_fragment_x86_64(emitter, "_pf_w_open_mid", PF_WARN_OPEN_MID.len());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // the WHOLE URL, not the resource
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");
    emitter.instruction("call __rt_diag_warning");
    emit_warning_fragment_x86_64(emitter, "_fgc_filter_fail_tail", FGC_FILTER_FAIL_TAIL.len());
    abi::emit_symbol_address(emitter, "r9", "_php_filter_unknown_count");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // a failed open never reaches the filters
    emitter.label("__rt_pfof_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // hand the boxed result straight back
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// x86_64 form of [`emit_unknown_report_aarch64`].
///
/// `__rt_php_filter_unknown_report(rax = boxed result, rdi = callee pointer, rsi = callee length)`;
/// answers the boxed result unchanged in `rax`.
fn emit_unknown_report_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: warn for every php://filter name that named no filter ---");
    emitter.label_global("__rt_php_filter_unknown_report");
    // Frame: [rbp-8]=boxed result [rbp-16]=callee pointer [rbp-24]=callee length
    //        [rbp-32]=name count [rbp-40]=name index [rbp-48]=attempts [rbp-56]=attempt index.
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 64");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");
    abi::emit_symbol_address(emitter, "r9", "_php_filter_unknown_count");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // how many names resolved to nothing
    emitter.instruction("mov QWORD PTR [r9], 0");                               // exactly one open consumes them
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");
    abi::emit_symbol_address(emitter, "r9", "_php_filter_url_ptr");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // and one open consumes the URL flag
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_pfur_done_x");                                 // every name resolved
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    emitter.instruction("cmp QWORD PTR [rax], 9");
    emitter.instruction("jne __rt_pfur_done_x");                                // a failed open never reaches the filters

    // -- how many times php walks this list --
    abi::emit_symbol_address(emitter, "r9", "_php_filter_url_dir");
    emitter.instruction("mov r9, QWORD PTR [r9]");
    emitter.instruction("cmp r9, 3");                                           // did the URL spell a direction itself?
    emitter.instruction("jne __rt_pfur_once_x");                                // an explicit list is applied exactly once
    abi::emit_symbol_address(emitter, "r10", "_php_filter_open_dirs");
    emitter.instruction("mov r10, QWORD PTR [r10]");
    emitter.instruction("mov r11, r10");
    emitter.instruction("and r11, 1");                                          // the read pass
    emitter.instruction("shr r10, 1");
    emitter.instruction("and r10, 1");                                          // the write pass
    emitter.instruction("add r11, r10");
    emitter.instruction("jmp __rt_pfur_attempts_ready_x");
    emitter.label("__rt_pfur_once_x");
    emitter.instruction("mov r11, 1");
    emitter.label("__rt_pfur_attempts_ready_x");
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");

    emitter.label("__rt_pfur_name_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");
    emitter.instruction("cmp r9, r10");
    emitter.instruction("jae __rt_pfur_done_x");                                // every name reported
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");
    emitter.label("__rt_pfur_attempt_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");
    emitter.instruction("cmp r9, r10");
    emitter.instruction("jae __rt_pfur_name_next_x");
    // -- `Warning: <callee>(): Unable to locate filter "<name>"` --
    emit_warning_fragment_x86_64(emitter, "_pf_w_head", PF_WARN_HEAD.len());
    emit_callee_fragment_x86_64(emitter);
    emit_warning_fragment_x86_64(emitter, "_pf_w_locate_mid", PF_WARN_LOCATE_MID.len());
    emit_unknown_name_fragment_x86_64(emitter);
    emit_warning_fragment_x86_64(emitter, "_pf_w_locate_end", PF_WARN_LOCATE_END.len());
    // -- `Warning: <callee>(): Unable to create filter (<name>)` --
    emit_warning_fragment_x86_64(emitter, "_pf_w_head", PF_WARN_HEAD.len());
    emit_callee_fragment_x86_64(emitter);
    emit_warning_fragment_x86_64(emitter, "_pf_w_create_mid", PF_WARN_CREATE_MID.len());
    emit_unknown_name_fragment_x86_64(emitter);
    emit_warning_fragment_x86_64(emitter, "_pf_w_create_end", PF_WARN_CREATE_END.len());
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");
    emitter.instruction("add r9, 1");
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");
    emitter.instruction("jmp __rt_pfur_attempt_x");

    emitter.label("__rt_pfur_name_next_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");
    emitter.instruction("add r9, 1");
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");
    emitter.instruction("jmp __rt_pfur_name_x");

    emitter.label("__rt_pfur_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // hand the boxed result straight back
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// x86_64 form of [`emit_warning_fragment_aarch64`].
fn emit_warning_fragment_x86_64(emitter: &mut Emitter, symbol: &str, len: usize) {
    abi::emit_symbol_address(emitter, "rdi", symbol);
    emitter.instruction(&format!("mov rsi, {len}"));
    emitter.instruction("call __rt_diag_warning");                              // clobbers rax/rdx/r10: all reloaded
}

/// x86_64 form of [`emit_callee_fragment_aarch64`].
fn emit_callee_fragment_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");
    emitter.instruction("call __rt_diag_warning");
}

/// x86_64 form of [`emit_unknown_name_fragment_aarch64`].
fn emit_unknown_name_fragment_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // which name this pass reports
    abi::emit_symbol_address(emitter, "r10", "_php_filter_unknown_ptr");
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9 * 8]");
    abi::emit_symbol_address(emitter, "r10", "_php_filter_unknown_len");
    emitter.instruction("mov rsi, QWORD PTR [r10 + r9 * 8]");
    emitter.instruction("call __rt_diag_warning");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Collects the `_php_filter_*` symbols one helper's emitted body touches, in order.
    fn filter_symbols_in(asm: &str, helper: &str) -> Vec<String> {
        let at = asm
            .find(&format!("{helper}:"))
            .unwrap_or_else(|| panic!("{helper} must be emitted"));
        let end = asm[at..]
            .find("--- runtime:")
            .map_or(asm.len(), |offset| at + offset);
        let mut found: Vec<String> = Vec::new();
        let body = &asm[at..end];
        let mut cursor = 0usize;
        while let Some(offset) = body[cursor..].find("_php_filter_") {
            let start = cursor + offset;
            let tail = &body[start..];
            let len = tail
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(tail.len());
            cursor = start + len;
            // `__rt_php_filter_parse` CONTAINS `_php_filter_parse`. Only a match that starts the
            // identifier is a data symbol; anything glued to a preceding word is a helper name.
            let glued = body[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            if glued {
                continue;
            }
            let symbol = tail[..len].to_string();
            if !found.contains(&symbol) {
                found.push(symbol);
            }
        }
        found
    }

    /// Every slot the PARSE publishes must be parked, or the nesting fix has a silent hole.
    ///
    /// The hand-off is a set of fixed globals, and a slot added to it later is invisible to the
    /// save/restore pair: nothing fails to compile, nothing crashes, and the outer open simply
    /// answers with the inner open's value for that one slot. That is exactly the shape the whole
    /// defect had — `abc` where php answers `ABC` — so the table is checked against what the
    /// parse actually writes rather than trusted to stay in step by hand.
    ///
    /// Only the resource pair is exempt: the caller reads it into registers before it opens
    /// anything, so no nested parse can reach it.
    #[test]
    fn test_every_slot_the_parse_publishes_is_parked() {
        const EXEMPT: &[&str] = &["_php_filter_res_ptr", "_php_filter_res_len"];
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_php_filter_dynamic(&mut emitter);
            let asm = emitter.output();
            for symbol in filter_symbols_in(&asm, "__rt_php_filter_parse") {
                assert!(
                    EXEMPT.contains(&symbol.as_str())
                        || PENDING_STATE.iter().any(|(parked, _)| *parked == symbol),
                    "{arch:?}: the parse publishes {symbol}, which no open parks across its resource's open"
                );
            }
        }
    }

    /// The save and the restore must walk the SAME symbols, in the same order, on both arches.
    ///
    /// They are two hand-written loops over one frame layout: a symbol parked at one offset and
    /// republished from another silently swaps two pieces of the hand-off — a filter count read
    /// back as a URL pointer, say — which is a wrong answer, not a crash.
    #[test]
    fn test_the_parked_frame_is_saved_and_restored_symbol_for_symbol() {
        let expected: Vec<String> = PENDING_STATE
            .iter()
            .map(|(symbol, _)| (*symbol).to_string())
            .collect();
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_php_filter_dynamic(&mut emitter);
            let asm = emitter.output();
            let saved = filter_symbols_in(&asm, "__rt_php_filter_pending_save");
            let restored = filter_symbols_in(&asm, "__rt_php_filter_pending_restore");
            let bookkeeping = ["_php_filter_pending_depth", "_php_filter_pending_stack"]
                .map(str::to_string)
                .to_vec();
            assert_eq!(
                saved,
                [bookkeeping, expected.clone()].concat(),
                "{arch:?}: the save must park the whole hand-off, in table order"
            );
            assert_eq!(
                saved, restored,
                "{arch:?}: the restore must walk the frame the save wrote, slot for slot"
            );
        }
    }

    /// The attach loop must spill its index BEFORE the calls that consume the filter.
    ///
    /// `r9` and `x9` are caller-saved on both ABIs, so an index left in a register across
    /// `__rt_filter_create` and `__rt_stream_filter_link` comes back as whatever those helpers
    /// left behind. The loop would then re-attach one filter forever or walk off the list — and
    /// only for a chain of two or more, which is precisely the case this change introduced.
    #[test]
    fn test_the_attach_loop_spills_its_index_before_the_calls() {
        for (arch, label, spill) in [
            (Arch::AArch64, "__rt_pfa_next:", "str x9, [sp, #32]"),
            (
                Arch::X86_64,
                "__rt_pfa_next_x:",
                "mov QWORD PTR [rbp - 40], r9",
            ),
        ] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_php_filter_dynamic(&mut emitter);
            let asm = emitter.output();
            let at = asm
                .find(label)
                .unwrap_or_else(|| panic!("{arch:?}: the attach loop must be labelled"));
            let spilled = asm[at..]
                .find(spill)
                .unwrap_or_else(|| panic!("{arch:?}: the loop index must be spilled"));
            let called = asm[at..]
                .find("__rt_filter_create")
                .unwrap_or_else(|| panic!("{arch:?}: the loop must create a filter"));
            assert!(
                spilled < called,
                "{arch:?}: the index must reach the frame before the first call clobbers it"
            );
        }
    }

    /// Both arches must issue the SAME diagnostic calls, in the same order, per helper.
    ///
    /// x86_64 cannot be executed on the machine this was written on, and the two emitters are
    /// hand-written twins, so a fragment dropped from one of them is invisible until CI. The
    /// warning fragments are the part most easily lost: a missing one is not a crash, it is a
    /// warning that silently reads `Warning: (): Unable to locate filter` — still a line, still
    /// green in any test that only asserts "something warned".
    #[test]
    fn test_both_arches_emit_the_same_diagnostic_calls_per_helper() {
        let mut a64 = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        emit_php_filter_dynamic(&mut a64);
        let a64 = a64.output();
        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_php_filter_dynamic(&mut x86);
        let x86 = x86.output();
        // `open_failed` composes five fragments and pops the suppression it was paired with;
        // `unknown_report` composes five per line and prints two lines.
        for (helper, warnings) in [
            ("__rt_php_filter_open_failed", 5),
            ("__rt_php_filter_unknown_report", 10),
        ] {
            for (arch, asm, call) in [("aarch64", &a64, "bl "), ("x86_64", &x86, "call ")] {
                let at = asm
                    .find(&format!("{helper}:"))
                    .unwrap_or_else(|| panic!("{arch}: {helper} must be emitted"));
                let end = asm[at..]
                    .find("--- runtime:")
                    .map_or(asm.len(), |offset| at + offset);
                let body = &asm[at..end];
                assert_eq!(
                    body.matches(&format!("{call}__rt_diag_warning")).count(),
                    warnings,
                    "{arch}: {helper} must compose every fragment php words the line with"
                );
            }
        }
        // The suppression is a PAIR across two helpers: one pushes only for a filter URL, the
        // other pops under the same flag. Losing either leaks a suppression depth, which
        // silences every later warning in the program.
        //
        // The pair names the FILTER counter, never `@`'s: the two are separate so a filtered
        // open can stand its scope down for the user wrapper's `stream_open`, which is PHP and
        // whose warnings php prints. Reverting either call to `__rt_diag_{push,pop}_suppression`
        // would swallow that PHP again, silently, so the spelling is asserted here.
        assert!(
            a64.contains("bl __rt_diag_push_filter_suppression")
                && a64.contains("bl __rt_diag_pop_filter_suppression"),
            "aarch64: the filter open must both open and close its OWN suppression scope"
        );
        assert!(
            x86.contains("call __rt_diag_push_filter_suppression")
                && x86.contains("call __rt_diag_pop_filter_suppression"),
            "x86_64: the filter open must both open and close its OWN suppression scope"
        );
        assert!(
            !a64.contains("bl __rt_diag_push_suppression")
                && !x86.contains("call __rt_diag_push_suppression"),
            "the filter machinery must not raise the depth `@` owns"
        );
    }

    /// The parse must record the names it SKIPPED, on both arches.
    ///
    /// It published only the ids it had resolved, so an unrecognised name left no trace at all
    /// and the report had nothing to warn about — the silence this whole channel exists to end.
    #[test]
    fn test_the_parse_records_the_names_it_could_not_resolve() {
        for (arch, store) in [
            (Arch::AArch64, "str x13, [x12, x11, lsl #3]"),
            (Arch::X86_64, "mov QWORD PTR [r8 + r11 * 8], rax"),
        ] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_php_filter_dynamic(&mut emitter);
            let asm = emitter.output();
            for symbol in ["_php_filter_unknown_ptr", "_php_filter_unknown_len"] {
                assert!(
                    asm.contains(symbol),
                    "{arch:?}: the parse must publish {symbol}"
                );
            }
            assert!(
                asm.matches(store).count() >= 2,
                "{arch:?}: both halves of the skipped name's span must be appended"
            );
        }
    }

    /// Each emitter's frame must reach the deepest slot it writes.
    ///
    /// Publishing a list needed two more parse slots and two more attach slots than the single-id
    /// hand-off did. A slot past the reservation is a write below `rsp` on x86_64 and into the
    /// saved `x30` on AArch64 — the second is a corrupted return address, which reproduces as a
    /// crash nowhere near this file.
    #[test]
    fn test_every_frame_slot_written_is_inside_its_reservation() {
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_php_filter_dynamic(&mut emitter);
            let asm = emitter.output();
            match arch {
                Arch::AArch64 => {
                    // The parse writes [sp, #72]; its saved pair must sit above that.
                    assert!(
                        asm.contains("sub sp, sp, #96") && asm.contains("stp x29, x30, [sp, #80]"),
                        "the parse frame must clear the unresolved-name slots before the linkage"
                    );
                    assert!(
                        asm.contains("sub sp, sp, #64") && asm.contains("stp x29, x30, [sp, #48]"),
                        "the attach frame must clear the count slot before the linkage"
                    );
                    // The report writes [sp, #48]; its saved pair must sit above that.
                    assert!(
                        asm.contains("sub sp, sp, #80") && asm.contains("stp x29, x30, [sp, #64]"),
                        "the report frame must clear the attempt slot before the linkage"
                    );
                }
                Arch::X86_64 => {
                    // The parse writes [rbp - 80]; anything past the reservation is below rsp.
                    assert!(
                        asm.contains("sub rsp, 96"),
                        "the parse frame must reserve the unresolved-name slots"
                    );
                    assert!(
                        asm.contains("mov QWORD PTR [rbp - 56], r11"),
                        "the parse must record the separator it stopped on"
                    );
                    assert!(
                        asm.contains("mov QWORD PTR [rbp - 80], r11"),
                        "the parse must record how many names resolved to nothing"
                    );
                    // The report writes [rbp - 56].
                    assert!(
                        asm.contains("sub rsp, 64"),
                        "the report frame must reserve the attempt slot"
                    );
                }
            }
        }
    }
}
