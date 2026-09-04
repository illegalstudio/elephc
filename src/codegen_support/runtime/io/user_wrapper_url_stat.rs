//! Purpose:
//! Emits `__rt_user_wrapper_url_stat`, the path-based stat dispatcher for
//! userspace stream wrappers. Given a `scheme://...` path it scans the
//! registered-wrapper table, instantiates the matching class, calls its
//! `url_stat($path, $flags)` method (vtable slot 9), and returns the boxed
//! Mixed stat array. Backs `file_exists()`/`is_file()`/`filesize()` on
//! `scheme://` URLs.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The file_exists/is_file/filesize builtin emitters call it before their
//!   normal filesystem path and branch on the `_url_stat_matched` out-flag.
//!
//! Key details:
//! - `_url_stat_matched` is set to 1 only when the path's scheme matches a
//!   registered wrapper, distinguishing "not a wrapper URL → fall back to the
//!   real filesystem" from "the wrapper reported the path absent → false".
//! - The scheme scan / slot match mirrors the inlined logic in
//!   `__rt_fopen`. The throwaway wrapper instance is freed with
//!   `__rt_decref_any` once `url_stat` returns; the boxed array is normalized
//!   by the shared `__rt_box_wrapper_stat_result`.
//! - `__rt_new_by_name` takes the class name in x1/x2 (AArch64) or rax/rdx
//!   (x86_64), NOT the SysV argument registers. The method call uses the
//!   regular elephc method ABI (`$this`, then a string pair, then the int flag).

use super::MIN_WRAPPER_SCHEME_LEN;
use crate::codegen_support::runtime::data::US_CACHE_PATH_CAP;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Byte offset of the url_stat method pointer in the per-class user-wrapper
/// vtable (slot 9 of `USER_WRAPPER_VTABLE_SLOTS`, 8 bytes per slot).
const VTABLE_URL_STAT_OFFSET: usize = 9 * 8;

/// Emits `__rt_user_wrapper_url_stat(path_ptr, path_len, flags)`.
///
/// On a registered scheme match it sets `_url_stat_matched = 1` and returns the
/// wrapper's `url_stat()` result boxed as a Mixed cell (an associative stat
/// array, or `false` when the class/method is missing or the wrapper reports
/// the path absent). On no match it sets `_url_stat_matched = 0` and returns 0
/// so the caller falls back to the real filesystem. Dispatches by target.
pub fn emit_user_wrapper_url_stat(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_url_stat_linux_x86_64(emitter);
        return;
    }
    emit_user_wrapper_url_stat_aarch64(emitter);
}

/// Emits `__rt_clear_stat_cache`: empties both stat slots, releasing what they held.
///
/// This is the ONLY thing that empties them. MEASURED on `php -n` 8.5.6: `unlink()`, `rename()`,
/// `touch()`, `chmod()`, `mkdir()` and a write through `fopen()` all leave the cached answer
/// standing, and `clearstatcache()` clears it whatever its arguments say — a targeted path does
/// not spare an unrelated one.
pub fn emit_clear_stat_cache(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: clear_stat_cache ---");
    emitter.label_global("__rt_clear_stat_cache");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");
            emitter.instruction("stp x29, x30, [sp, #0]");
            emitter.instruction("mov x29, sp");
            for (len_sym, box_sym, label) in [
                ("_us_cache_stat_len", "_us_cache_stat_box", "__rt_csc_stat_done"),
                ("_us_cache_lstat_len", "_us_cache_lstat_box", "__rt_csc_lstat_done"),
            ] {
                abi::emit_symbol_address(emitter, "x9", len_sym);
                emitter.instruction("str xzr, [x9]");                           // the slot answers for nothing
                abi::emit_symbol_address(emitter, "x9", box_sym);
                emitter.instruction("ldr x0, [x9]");
                emitter.instruction(&format!("cbz x0, {}", label));
                emitter.instruction("str xzr, [x9]");                           // cleared BEFORE the release, so nothing can see a freed box
                emitter.instruction("bl __rt_decref_any");                      // the reference the slot itself held
                emitter.label(label);
            }
            emitter.instruction("ldp x29, x30, [sp, #0]");
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            for (len_sym, box_sym, label) in [
                ("_us_cache_stat_len", "_us_cache_stat_box", "__rt_csc_stat_done_x86"),
                ("_us_cache_lstat_len", "_us_cache_lstat_box", "__rt_csc_lstat_done_x86"),
            ] {
                abi::emit_symbol_address(emitter, "r10", len_sym);
                emitter.instruction("mov QWORD PTR [r10], 0");                  // the slot answers for nothing
                abi::emit_symbol_address(emitter, "r10", box_sym);
                emitter.instruction("mov rax, QWORD PTR [r10]");
                emitter.instruction("test rax, rax");
                emitter.instruction(&format!("jz {}", label));
                emitter.instruction("mov QWORD PTR [r10], 0");                  // cleared BEFORE the release
                emitter.instruction("call __rt_decref_any");                    // the reference the slot itself held
                emitter.label(label);
            }
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Picks the stat-cache slot a query belongs to, leaving its three symbols in x13/x14/x15.
///
/// The slot is three INDEPENDENT `.comm` symbols — length, path buffer, box — and nothing orders
/// them in memory, so each is addressed by name rather than by an offset from the first. Both the
/// lookup and the fill call this, with their own label names, because the fill runs after calls
/// that clobber every scratch register.
fn emit_select_stat_slot(emitter: &mut Emitter, link_label: &str, chosen_label: &str) {
    emitter.instruction("ldr x9, [sp, #32]");                                   // the flags this query asked with
    emitter.instruction("and x9, x9, #1");                                      // bit 0 is php's STREAM_URL_STAT_LINK
    emitter.instruction(&format!("cbnz x9, {}", link_label));                   // is_link()/lstat() keep their own slot
    abi::emit_symbol_address(emitter, "x13", "_us_cache_stat_len");
    abi::emit_symbol_address(emitter, "x14", "_us_cache_stat_path");
    abi::emit_symbol_address(emitter, "x15", "_us_cache_stat_box");
    emitter.instruction(&format!("b {}", chosen_label));
    emitter.label(link_label);
    abi::emit_symbol_address(emitter, "x13", "_us_cache_lstat_len");
    abi::emit_symbol_address(emitter, "x14", "_us_cache_lstat_path");
    abi::emit_symbol_address(emitter, "x15", "_us_cache_lstat_box");
    emitter.label(chosen_label);
}

/// AArch64 implementation of `__rt_user_wrapper_url_stat`.
///
/// Inputs: x0 = path pointer, x1 = path length, x2 = `url_stat` flags.
/// Output: x0 = boxed Mixed result (valid when `_url_stat_matched` is 1).
fn emit_user_wrapper_url_stat_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat ---");
    emitter.label_global("__rt_user_wrapper_url_stat");

    // Frame: 64 bytes. [sp,#0..16] x29/x30, [sp,#16] path ptr, [sp,#24] path
    //   len, [sp,#32] flags, [sp,#48] obj, [sp,#56] boxed result.
    emitter.instruction("sub sp, sp, #64");                                     // helper frame for the path-stat dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the path pointer across the helper calls
    emitter.instruction("str x1, [sp, #24]");                                   // save the path length across the helper calls
    emitter.instruction("str x2, [sp, #32]");                                   // save the url_stat flags across the helper calls

    // -- php's stat cache: one slot for LINK queries, one for the rest --
    //
    // MEASURED on `php -n` 8.5.6: `filesize()`, `file_exists()`, `is_dir()`, `is_file()` and
    // `filemtime()` on one path call `url_stat()` ONCE between them. `is_link()`/`lstat()` do not
    // share that slot — `filesize()` then `is_link()` calls it twice. Nothing but
    // `clearstatcache()` empties either: not `unlink()`, `rename()`, `touch()`, `chmod()`, or a
    // write through `fopen()` — all measured, all leave the cached answer standing.
    //
    // A cached path was a wrapper path when it went in, so a hit skips the scheme scan entirely.
    emit_select_stat_slot(emitter, "__rt_uus_cache_link", "__rt_uus_cache_chosen");
    emitter.instruction("ldr x9, [x13]");                                       // the length it currently answers for
    emitter.instruction("cbz x9, __rt_uus_scan_start");                         // empty slot
    emitter.instruction("cmp x9, x1");                                          // same length as the path asked about?
    emitter.instruction("b.ne __rt_uus_evict");
    emitter.instruction("mov x10, #0");                                         // compare the bytes themselves
    emitter.label("__rt_uus_cache_cmp");
    emitter.instruction("cmp x10, x9");
    emitter.instruction("b.hs __rt_uus_cache_hit");                             // every byte matched
    emitter.instruction("ldrb w11, [x0, x10]");
    emitter.instruction("ldrb w12, [x14, x10]");
    emitter.instruction("cmp w11, w12");
    emitter.instruction("b.ne __rt_uus_evict");                                 // a different path replaces this one
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("b __rt_uus_cache_cmp");

    // A miss is just a miss. Emptying the slot belongs to the moment ANOTHER stat takes it, which
    // is not here: a wrapper path that resolves fills the slot itself, one that answers FALSE puts
    // nothing there, and a plain path is handled at the no-match exit below.
    emitter.label("__rt_uus_evict");
    emitter.instruction("b __rt_uus_scan_start");

    emitter.label("__rt_uus_cache_hit");
    abi::emit_symbol_address(emitter, "x10", "_url_stat_matched");
    emitter.instruction("mov w9, #1");
    emitter.instruction("strb w9, [x10]");                                      // a cached path matched a wrapper when it went in
    emitter.instruction("ldr x0, [x15]");                                       // the box the slot owns
    emitter.instruction("str x0, [sp, #56]");
    emitter.instruction("bl __rt_incref");                                      // every caller releases what it gets its own
    emitter.instruction("ldr x0, [sp, #56]");
    emitter.instruction("b __rt_uus_ret");

    emitter.label("__rt_uus_scan_start");
    emitter.instruction("ldr x0, [sp, #16]");                                   // the probe above walked the path pointer
    emitter.instruction("ldr x1, [sp, #24]");

    // -- scan the path for the "://" scheme separator (x0=ptr, x1=len) --
    emitter.instruction(&format!("mov x9, #{}", MIN_WRAPPER_SCHEME_LEN));       // scheme scan index: a one-letter scheme is never a wrapper
    emitter.label("__rt_uus_scan");
    emitter.instruction("add x10, x9, #3");                                     // need three bytes for the "://" marker
    emitter.instruction("cmp x10, x1");                                         // do enough bytes remain in the path?
    emitter.instruction("b.gt __rt_uus_nomatch");                               // no scheme separator → not a wrapper URL
    emitter.instruction("ldrb w11, [x0, x9]");                                  // load the candidate ':' byte
    emitter.instruction("cmp w11, #58");                                        // is it ':'?
    emitter.instruction("b.ne __rt_uus_scan_next");                             // not the scheme marker
    emitter.instruction("add x12, x9, #1");                                     // index of the first '/'
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the candidate first '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_uus_scan_next");                             // not the scheme marker
    emitter.instruction("add x12, x9, #2");                                     // index of the second '/'
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the candidate second '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_uus_scan_next");                             // not the scheme marker
    emitter.instruction("b __rt_uus_check");                                    // "://" found at index x9 — x9 is the scheme length
    emitter.label("__rt_uus_scan_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_uus_scan");                                     // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (x9=scheme len) --
    emitter.label("__rt_uus_check");
    super::emit_load_table_base(emitter, "x10");
    emitter.instruction("mov x11, #0");                                         // wrapper slot index
    emitter.label("__rt_uus_slot");
    super::emit_load_table_cap(emitter, "x12");
    emitter.instruction("cmp x11, x12");                                        // checked every allocated wrapper slot?
    emitter.instruction("b.ge __rt_uus_nomatch");                               // no registered wrapper matched the scheme
    emitter.instruction("add x12, x10, x11, lsl #5");                           // slot base = table + index * 32
    emitter.instruction("ldr x13, [x12]");                                      // stored protocol pointer
    emitter.instruction("cbz x13, __rt_uus_slot_next");                         // empty slot — skip it
    emitter.instruction("ldr x14, [x12, #8]");                                  // stored protocol length
    emitter.instruction("cmp x14, x9");                                         // does the stored length match the scheme length?
    emitter.instruction("b.ne __rt_uus_slot_next");                             // length mismatch — try the next slot
    emitter.instruction("mov x15, #0");                                         // byte compare index
    emitter.label("__rt_uus_bytes");
    emitter.instruction("cmp x15, x9");                                         // compared every protocol byte?
    emitter.instruction("b.ge __rt_uus_match");                                 // full match — dispatch into the wrapper class
    emitter.instruction("ldrb w16, [x13, x15]");                                // stored protocol byte
    emitter.instruction("ldrb w17, [x0, x15]");                                 // path scheme byte
    emitter.instruction("cmp w16, w17");                                        // do the bytes match?
    emitter.instruction("b.ne __rt_uus_slot_next");                             // protocol byte differs — try the next slot
    emitter.instruction("add x15, x15, #1");                                    // advance the compare index
    emitter.instruction("b __rt_uus_bytes");                                    // continue comparing bytes
    emitter.label("__rt_uus_slot_next");
    emitter.instruction("add x11, x11, #1");                                    // advance the slot index
    emitter.instruction("b __rt_uus_slot");                                     // continue scanning slots

    // -- matched scheme: x12 = registry slot base --
    emitter.label("__rt_uus_match");
    abi::emit_symbol_address(emitter, "x10", "_url_stat_matched");
    emitter.instruction("mov w9, #1");                                          // record that a registered wrapper scheme matched
    emitter.instruction("strb w9, [x10]");                                      // set _url_stat_matched = 1 (do not fall back to the filesystem)
    emitter.instruction("ldr x1, [x12, #16]");                                  // wrapper class name pointer from the registry slot
    emitter.instruction("ldr x2, [x12, #24]");                                  // wrapper class name length from the registry slot
    emitter.instruction("bl __rt_new_by_name");                                 // instantiate the wrapper class → x0 = obj, or 0 when unknown
    emitter.instruction("bl __rt_user_wrapper_construct");                      // php constructs before it asks
    emitter.instruction("cbz x0, __rt_uus_false");                              // unknown class → boxed false
    emitter.instruction("str x0, [sp, #48]");                                   // save the throwaway wrapper instance
    // php assigns `$context` to this instance too, so a class that declares no such property is
    // deprecated here exactly as it is for `fopen()` — MEASURED, once per instantiation.
    emitter.instruction("bl __rt_wrapper_context_notice");
    emitter.instruction("ldr x0, [sp, #48]");                                   // the notice clobbers nothing it needs back

    // -- look up url_stat in the per-class user-wrapper vtable (slot 9) --
    emitter.instruction("ldr x9, [x0]");                                        // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("ldr x11, [x10, #{}]", VTABLE_URL_STAT_OFFSET)); // load the url_stat method pointer (slot 9)
    emitter.instruction("cbz x11, __rt_uus_false_obj");                         // class did not implement url_stat → boxed false

    // -- call url_stat($this, $path, $flags) → x0 = raw return --
    emitter.instruction("ldr x0, [sp, #48]");                                   // $this = wrapper object
    emitter.instruction("ldr x1, [sp, #16]");                                   // path ptr → string-arg pair
    emitter.instruction("ldr x2, [sp, #24]");                                   // path len → string-arg pair
    emitter.instruction("ldr x3, [sp, #32]");                                   // url_stat flags
    emitter.instruction("blr x11");                                             // invoke url_stat on the throwaway wrapper object
    emitter.instruction("bl __rt_box_wrapper_stat_result");                     // normalize the type-erased return into a boxed Mixed
    emitter.instruction("str x0, [sp, #56]");                                   // save the boxed result across the wrapper-instance release
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the throwaway wrapper object
    emitter.instruction("bl __rt_decref_any");                                  // free the throwaway wrapper instance
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the boxed result for return

    // -- fill the slot this query belongs to --
    // A wrapper that reports the path ABSENT is not cached: measured, php asks again every time.
    emitter.instruction("ldr x9, [x0]");                                        // the boxed runtime tag
    emitter.instruction("cmp x9, #3");                                          // php false: the path is not there
    emitter.instruction("b.eq __rt_uus_ret");
    emitter.instruction("ldr x1, [sp, #24]");                                   // the path length
    emitter.instruction(&format!("cmp x1, #{}", US_CACHE_PATH_CAP));
    emitter.instruction("b.hi __rt_uus_ret");                                   // too long for the slot: correct, only slower
    emit_select_stat_slot(emitter, "__rt_uus_fill_link", "__rt_uus_fill_chosen");
    emitter.instruction("str xzr, [x13]");                                      // the slot answers for nothing while it is rebuilt
    emitter.instruction("ldr x9, [x15]");                                       // whatever it answered with before
    emitter.instruction("cbz x9, __rt_uus_cache_fill");
    emitter.instruction("mov x0, x9");
    emitter.instruction("bl __rt_decref_any");                                  // the slot's own reference goes with it
    emitter.instruction("ldr x0, [sp, #56]");                                   // the new box, saved across that release
    emitter.label("__rt_uus_cache_fill");
    emitter.instruction("bl __rt_incref");                                      // the slot holds one reference of its own
    emitter.instruction("ldr x0, [sp, #56]");
    emit_select_stat_slot(emitter, "__rt_uus_fill2_link", "__rt_uus_fill2_chosen");
    emitter.instruction("str x0, [x15]");                                       // the box the slot now answers with
    emitter.instruction("ldr x1, [sp, #16]");                                   // copy the path in: the caller's may be freed
    emitter.instruction("ldr x2, [sp, #24]");
    emitter.instruction("mov x10, #0");
    emitter.label("__rt_uus_cache_copy");
    emitter.instruction("cmp x10, x2");
    emitter.instruction("b.hs __rt_uus_cache_copied");
    emitter.instruction("ldrb w11, [x1, x10]");
    emitter.instruction("strb w11, [x14, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("b __rt_uus_cache_copy");
    emitter.label("__rt_uus_cache_copied");
    emitter.instruction("str x2, [x13]");                                       // published LAST: a length is what makes the slot live

    // -- a LINK query that found something that is NOT a link fills the ordinary slot too --
    //
    // MEASURED: `lstat()` then `stat()` on the same path calls `url_stat` ONCE, but `is_link()`
    // then `filesize()` on a REAL symlink calls it TWICE. php can answer an ordinary stat from an
    // lstat result exactly when the thing is not a link. Filling both slots here reproduces that
    // from the fill side alone, so the lookup stays one probe.
    emitter.instruction("ldr x9, [sp, #32]");                                   // the flags this query asked with
    emitter.instruction("and x9, x9, #1");
    emitter.instruction("cbz x9, __rt_uus_cache_done");                         // an ordinary query already filled its own slot
    emitter.instruction("ldr x0, [sp, #56]");                                   // the boxed stat array, borrowed
    abi::emit_symbol_address(emitter, "x1", "_stat_key_mode");
    emitter.instruction("mov x2, #4");                                          // strlen("mode")
    emitter.instruction("bl __rt_uusf_read");                                   // x0 = mode, x1 = was it there and an int
    emitter.instruction("cbz x1, __rt_uus_cache_done");                         // no mode: cannot tell, so do not share
    // Hex, not a leading zero: an assembler is free to read `0170000` as decimal, and S_IFMT
    // silently becoming 120000 would make every entry look like a non-link.
    emitter.instruction("and x0, x0, #0xF000");                                 // S_IFMT
    emitter.instruction("mov x9, #0xA000");                                     // S_IFLNK
    emitter.instruction("cmp x0, x9");
    emitter.instruction("b.eq __rt_uus_cache_done");                            // a real link: the ordinary slot must ask again
    emitter.label("__rt_uus_fill_both");
    emitter.instruction("ldr x0, [sp, #56]");
    emitter.instruction("bl __rt_incref");                                      // the second slot holds a reference of its own
    abi::emit_symbol_address(emitter, "x13", "_us_cache_stat_len");
    emitter.instruction("str xzr, [x13]");                                      // it answers for nothing while it is rebuilt
    abi::emit_symbol_address(emitter, "x15", "_us_cache_stat_box");
    emitter.instruction("ldr x9, [x15]");                                       // whatever it answered with before
    emitter.instruction("cbz x9, __rt_uus_both_fill");
    emitter.instruction("mov x0, x9");
    emitter.instruction("bl __rt_decref_any");
    emitter.label("__rt_uus_both_fill");
    emitter.instruction("ldr x0, [sp, #56]");
    abi::emit_symbol_address(emitter, "x13", "_us_cache_stat_len");
    abi::emit_symbol_address(emitter, "x14", "_us_cache_stat_path");
    abi::emit_symbol_address(emitter, "x15", "_us_cache_stat_box");
    emitter.instruction("str x0, [x15]");
    emitter.instruction("ldr x1, [sp, #16]");
    emitter.instruction("ldr x2, [sp, #24]");
    emitter.instruction("mov x10, #0");
    emitter.label("__rt_uus_both_copy");
    emitter.instruction("cmp x10, x2");
    emitter.instruction("b.hs __rt_uus_both_copied");
    emitter.instruction("ldrb w11, [x1, x10]");
    emitter.instruction("strb w11, [x14, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("b __rt_uus_both_copy");
    emitter.label("__rt_uus_both_copied");
    emitter.instruction("str x2, [x13]");                                       // published LAST, as above

    // Every exit from the fill comes through here: the mode read above left the MODE in x0, and
    // the common return path hands back whatever x0 holds.
    emitter.label("__rt_uus_cache_done");
    emitter.instruction("ldr x0, [sp, #56]");                                   // the caller's own reference
    emitter.instruction("b __rt_uus_ret");                                      // share the common return path

    // -- the class does not implement url_stat: warn the way php does, then box false --
    // The caller's name was published by the lowering; every stat builtin reaches this one helper.
    emitter.label("__rt_uus_false_obj");
    emitter.instruction("ldr x0, [sp, #48]");                                   // the wrapper object
    emitter.instruction("ldr x0, [x0]");                                        // class_id stored at its head
    abi::emit_symbol_address(emitter, "x9", "_uwmh_head");
    emitter.instruction("ldp x1, x2, [x9]");                                    // the caller's half
    abi::emit_symbol_address(emitter, "x9", "_uwmh_tail");
    emitter.instruction("ldp x3, x4, [x9]");                                    // the method's half
    emitter.instruction("bl __rt_wrapper_missing_hook_warning");
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the throwaway wrapper object
    emitter.instruction("bl __rt_decref_any");                                  // free it before falling through to boxed false
    emitter.label("__rt_uus_false");
    emitter.instruction("mov x0, #0");                                          // null sentinel → boxed false (scheme matched, stat unavailable)
    emitter.instruction("bl __rt_box_wrapper_stat_result");                     // produce boxed false; _url_stat_matched stays 1
    emitter.instruction("b __rt_uus_ret");                                      // share the common return path

    emitter.label("__rt_uus_nomatch");

    // -- a PLAIN path takes php's one slot, unless it is a query that never fills it --
    //
    // php holds ONE entry for the whole process: MEASURED, `filesize()` on a real file makes the
    // next wrapper query ask again. `file_exists()` and the access predicates do NOT — they answer
    // from `access(2)` and put nothing in the slot, so they empty nothing either. `_us_gentle`
    // says which kind this is; see `emit_publish_missing_hook_message`.
    //
    // A plain stat that FAILS also leaves php's slot alone, and this cannot see that — the
    // filesystem call happens in the caller. So a failing plain stat still evicts here: one extra
    // question, never a stale answer.
    abi::emit_symbol_address(emitter, "x9", "_us_gentle");
    emitter.instruction("ldr x9, [x9]");
    emitter.instruction("cbnz x9, __rt_uus_plain_kept");                        // it fills nothing, so it empties nothing
    emit_select_stat_slot(emitter, "__rt_uus_plain_link", "__rt_uus_plain_chosen");
    emitter.label("__rt_uus_plain_evict");
    emitter.instruction("str xzr, [x13]");                                      // the slot answers for nothing
    emitter.instruction("ldr x0, [x15]");
    emitter.instruction("cbz x0, __rt_uus_plain_kept");
    emitter.instruction("str xzr, [x15]");                                      // cleared BEFORE the release
    emitter.instruction("bl __rt_decref_any");                                  // the reference the slot held
    emitter.label("__rt_uus_plain_kept");

    abi::emit_symbol_address(emitter, "x10", "_url_stat_matched");
    emitter.instruction("strb wzr, [x10]");                                     // _url_stat_matched = 0 — caller falls back to the real filesystem
    emitter.instruction("mov x0, #0");                                          // return 0; the caller ignores it when the flag is 0

    emitter.label("__rt_uus_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed Mixed result (or 0 on no match)
}

/// x86_64 twin of [`emit_select_stat_slot`]; the three symbols land in r13/r14/r15.
fn emit_select_stat_slot_x86(emitter: &mut Emitter, link_label: &str, chosen_label: &str) {
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // the flags this query asked with
    emitter.instruction("and r9, 1");                                           // bit 0 is php's STREAM_URL_STAT_LINK
    emitter.instruction(&format!("jnz {}", link_label));                        // is_link()/lstat() keep their own slot
    abi::emit_symbol_address(emitter, "r13", "_us_cache_stat_len");
    abi::emit_symbol_address(emitter, "r14", "_us_cache_stat_path");
    abi::emit_symbol_address(emitter, "r15", "_us_cache_stat_box");
    emitter.instruction(&format!("jmp {}", chosen_label));
    emitter.label(link_label);
    abi::emit_symbol_address(emitter, "r13", "_us_cache_lstat_len");
    abi::emit_symbol_address(emitter, "r14", "_us_cache_lstat_path");
    abi::emit_symbol_address(emitter, "r15", "_us_cache_lstat_box");
    emitter.label(chosen_label);
}

/// x86_64 implementation of `__rt_user_wrapper_url_stat`.
///
/// Inputs: rdi = path pointer, rsi = path length, rdx = `url_stat` flags.
/// Output: rax = boxed Mixed result (valid when `_url_stat_matched` is 1).
fn emit_user_wrapper_url_stat_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat ---");
    emitter.label_global("__rt_user_wrapper_url_stat");

    // Frame: [rbp-8] path ptr, [rbp-16] path len, [rbp-24] flags, [rbp-32] obj,
    //   [rbp-40] boxed result. push rbp then sub rsp,64 keeps rsp 16-aligned.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // spill slots for path/flags/obj/result
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the path pointer across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the path length across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the url_stat flags across the helper calls

    // -- php's stat cache; see the AArch64 twin for the whole measured rule --
    emit_select_stat_slot_x86(emitter, "__rt_uus_cache_link_x86", "__rt_uus_cache_chosen_x86");
    emitter.instruction("mov r9, QWORD PTR [r13]");                             // the length it currently answers for
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_uus_scan_start_x86");                          // empty slot
    emitter.instruction("cmp r9, rsi");                                         // same length as the path asked about?
    emitter.instruction("jne __rt_uus_evict_x86");
    emitter.instruction("xor r10, r10");                                        // compare the bytes themselves
    emitter.label("__rt_uus_cache_cmp_x86");
    emitter.instruction("cmp r10, r9");
    emitter.instruction("jae __rt_uus_cache_hit_x86");                          // every byte matched
    emitter.instruction("movzx r11d, BYTE PTR [rdi + r10]");
    emitter.instruction("movzx r12d, BYTE PTR [r14 + r10]");
    emitter.instruction("cmp r11b, r12b");
    emitter.instruction("jne __rt_uus_evict_x86");                              // a different path replaces this one
    emitter.instruction("inc r10");
    emitter.instruction("jmp __rt_uus_cache_cmp_x86");

    // See the AArch64 twin: a miss is just a miss.
    emitter.label("__rt_uus_evict_x86");
    emitter.instruction("jmp __rt_uus_scan_start_x86");

    emitter.label("__rt_uus_cache_hit_x86");
    abi::emit_symbol_address(emitter, "r10", "_url_stat_matched");
    emitter.instruction("mov BYTE PTR [r10], 1");                               // a cached path matched a wrapper when it went in
    emitter.instruction("mov rax, QWORD PTR [r15]");                            // the box the slot owns
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");
    emitter.instruction("call __rt_incref");                                    // every caller releases what it gets its own
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
    emitter.instruction("jmp __rt_uus_ret_x86");

    emitter.label("__rt_uus_scan_start_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the probe above walked these
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("mov rax, rdi");                                        // path pointer → scan base register
    emitter.instruction("mov rdx, rsi");                                        // path length → scan bound register

    // -- scan the path for the "://" scheme separator (rax=ptr, rdx=len) --
    emitter.instruction(&format!("mov r9d, {}", MIN_WRAPPER_SCHEME_LEN));       // scheme scan index: a one-letter scheme is never a wrapper
    emitter.label("__rt_uus_scan_x86");
    emitter.instruction("lea r10, [r9 + 3]");                                   // need three bytes for the "://" marker
    emitter.instruction("cmp r10, rdx");                                        // do enough bytes remain in the path?
    emitter.instruction("jg __rt_uus_nomatch_x86");                             // no scheme separator → not a wrapper URL
    emitter.instruction("movzx r11d, BYTE PTR [rax + r9]");                     // load the candidate ':' byte
    emitter.instruction("cmp r11b, 58");                                        // is it ':'?
    emitter.instruction("jne __rt_uus_next_x86");                               // not the scheme marker
    emitter.instruction("lea r12, [r9 + 1]");                                   // index of the first '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate first '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_uus_next_x86");                               // not the scheme marker
    emitter.instruction("lea r12, [r9 + 2]");                                   // index of the second '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate second '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_uus_next_x86");                               // not the scheme marker
    emitter.instruction("jmp __rt_uus_check_x86");                              // "://" found at r9 — r9 is the scheme length
    emitter.label("__rt_uus_next_x86");
    emitter.instruction("inc r9");                                              // advance the scan index
    emitter.instruction("jmp __rt_uus_scan_x86");                               // keep scanning for the scheme marker

    // -- match the scheme against the registered-wrapper table (r9=scheme len) --
    emitter.label("__rt_uus_check_x86");
    super::emit_load_table_base(emitter, "r10");                 // wrapper table base
    emitter.instruction("xor r11, r11");                                        // wrapper slot index
    emitter.label("__rt_uus_slot_x86");
    super::emit_load_table_cap(emitter, "r12");
    emitter.instruction("cmp r11, r12");                                         // checked every allocated wrapper slot?
    emitter.instruction("jge __rt_uus_nomatch_x86");                            // no registered wrapper matched the scheme
    emitter.instruction("mov r12, r11");                                        // copy the slot index for scaling
    emitter.instruction("shl r12, 5");                                          // slot offset = index * 32
    emitter.instruction("add r12, r10");                                        // slot base = table + offset
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // stored protocol pointer
    emitter.instruction("test r13, r13");                                       // is this slot empty?
    emitter.instruction("jz __rt_uus_slot_next_x86");                           // skip empty slots
    emitter.instruction("mov r14, QWORD PTR [r12 + 8]");                        // stored protocol length
    emitter.instruction("cmp r14, r9");                                         // does the stored length match the scheme length?
    emitter.instruction("jne __rt_uus_slot_next_x86");                          // length mismatch — try the next slot
    emitter.instruction("xor r15, r15");                                        // byte compare index
    emitter.label("__rt_uus_bytes_x86");
    emitter.instruction("cmp r15, r9");                                         // compared every protocol byte?
    emitter.instruction("jge __rt_uus_match_x86");                              // full match — dispatch into the wrapper class
    emitter.instruction("movzx ecx, BYTE PTR [r13 + r15]");                     // stored protocol byte
    emitter.instruction("movzx r8d, BYTE PTR [rax + r15]");                     // path scheme byte
    emitter.instruction("cmp cl, r8b");                                         // do the bytes match?
    emitter.instruction("jne __rt_uus_slot_next_x86");                          // protocol byte differs — try the next slot
    emitter.instruction("inc r15");                                             // advance the compare index
    emitter.instruction("jmp __rt_uus_bytes_x86");                              // continue comparing bytes
    emitter.label("__rt_uus_slot_next_x86");
    emitter.instruction("inc r11");                                             // advance the slot index
    emitter.instruction("jmp __rt_uus_slot_x86");                               // continue scanning slots

    // -- matched scheme: r12 = registry slot base --
    emitter.label("__rt_uus_match_x86");
    abi::emit_symbol_address(emitter, "r10", "_url_stat_matched");              // out-flag address
    emitter.instruction("mov BYTE PTR [r10], 1");                               // set _url_stat_matched = 1 (do not fall back to the filesystem)
    emitter.instruction("mov rax, QWORD PTR [r12 + 16]");                       // wrapper class name pointer from the registry slot
    emitter.instruction("mov rdx, QWORD PTR [r12 + 24]");                       // wrapper class name length (new_by_name reads rax/rdx)
    emitter.instruction("call __rt_new_by_name");                               // instantiate the wrapper class → rax = obj, or 0 when unknown
    emitter.instruction("call __rt_user_wrapper_construct");                    // php constructs before it asks
    emitter.instruction("test rax, rax");                                       // unknown class?
    emitter.instruction("jz __rt_uus_false_x86");                               // unknown class → boxed false
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the throwaway wrapper instance
    // php assigns `$context` to this instance too, so a class that declares no such property is
    // deprecated here exactly as it is for `fopen()` — MEASURED, once per instantiation.
    emitter.instruction("mov rdi, rax");
    emitter.instruction("call __rt_wrapper_context_notice");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the instance the lookup below reads

    // -- look up url_stat in the per-class user-wrapper vtable (slot 9) --
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_vtable_ptrs");      // base of the per-class user-wrapper vtable pointer table
    emitter.instruction("mov r10, QWORD PTR [r10 + r9 * 8]");                   // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {}]", VTABLE_URL_STAT_OFFSET)); // load the url_stat method pointer (slot 9)
    emitter.instruction("test r11, r11");                                       // class did not implement url_stat?
    emitter.instruction("jz __rt_uus_false_obj_x86");                           // no url_stat → boxed false

    // -- call url_stat($this, $path, $flags) → rax = raw return --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // $this = wrapper object
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // path ptr → string-arg pair
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // path len → string-arg pair
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // url_stat flags
    emitter.instruction("call r11");                                            // invoke url_stat on the throwaway wrapper object
    emitter.instruction("call __rt_box_wrapper_stat_result");                   // normalize the type-erased return into a boxed Mixed
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the boxed result across the wrapper-instance release
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the throwaway wrapper object
    emitter.instruction("call __rt_decref_any");                                // free the throwaway wrapper instance
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed result for return

    // -- fill the slot this query belongs to; see the AArch64 twin --
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // the boxed runtime tag
    emitter.instruction("cmp r9, 3");                                           // php false: the path is not there
    emitter.instruction("je __rt_uus_ret_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // the path length
    emitter.instruction(&format!("cmp rsi, {}", US_CACHE_PATH_CAP));
    emitter.instruction("ja __rt_uus_ret_x86");                                 // too long for the slot
    emit_select_stat_slot_x86(emitter, "__rt_uus_fill_link_x86", "__rt_uus_fill_chosen_x86");
    emitter.instruction("mov QWORD PTR [r13], 0");                              // answers for nothing while it is rebuilt
    emitter.instruction("mov r9, QWORD PTR [r15]");                             // whatever it answered with before
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_uus_cache_fill_x86");
    emitter.instruction("mov rax, r9");
    emitter.instruction("call __rt_decref_any");                                // the slot's own reference goes with it
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
    emitter.label("__rt_uus_cache_fill_x86");
    emitter.instruction("call __rt_incref");                                    // the slot holds one reference of its own
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
    emit_select_stat_slot_x86(emitter, "__rt_uus_fill2_link_x86", "__rt_uus_fill2_chosen_x86");
    emitter.instruction("mov QWORD PTR [r15], rax");                            // the box the slot now answers with
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // copy the path in
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");
    emitter.instruction("xor r10, r10");
    emitter.label("__rt_uus_cache_copy_x86");
    emitter.instruction("cmp r10, rdx");
    emitter.instruction("jae __rt_uus_cache_copied_x86");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r10]");
    emitter.instruction("mov BYTE PTR [r14 + r10], r11b");
    emitter.instruction("inc r10");
    emitter.instruction("jmp __rt_uus_cache_copy_x86");
    emitter.label("__rt_uus_cache_copied_x86");
    emitter.instruction("mov QWORD PTR [r13], rdx");                            // published LAST

    // -- a LINK query that found a NON-link fills the ordinary slot too; see the AArch64 twin --
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");
    emitter.instruction("and r9, 1");
    emitter.instruction("jz __rt_uus_cache_done_x86");                          // an ordinary query already filled its own
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // the reader takes the array in rdi
    abi::emit_symbol_address(emitter, "rax", "_stat_key_mode");                 // and the key in rax/rdx
    emitter.instruction("mov rdx, 4");                                          // strlen("mode")
    emitter.instruction("call __rt_uusf_read_x86");                             // rax = mode, rdx = present-and-an-int
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_uus_cache_done_x86");                          // no mode: cannot tell, so do not share
    emitter.instruction("and rax, 0xF000");                                     // S_IFMT, in hex for the reason the twin gives
    emitter.instruction("cmp rax, 0xA000");                                     // S_IFLNK
    emitter.instruction("je __rt_uus_cache_done_x86");                          // a real link: the ordinary slot must ask again
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
    emitter.instruction("call __rt_incref");                                    // the second slot holds a reference of its own
    abi::emit_symbol_address(emitter, "r13", "_us_cache_stat_len");
    emitter.instruction("mov QWORD PTR [r13], 0");
    abi::emit_symbol_address(emitter, "r15", "_us_cache_stat_box");
    emitter.instruction("mov r9, QWORD PTR [r15]");
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_uus_both_fill_x86");
    emitter.instruction("mov rax, r9");
    emitter.instruction("call __rt_decref_any");
    emitter.label("__rt_uus_both_fill_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
    abi::emit_symbol_address(emitter, "r13", "_us_cache_stat_len");
    abi::emit_symbol_address(emitter, "r14", "_us_cache_stat_path");
    abi::emit_symbol_address(emitter, "r15", "_us_cache_stat_box");
    emitter.instruction("mov QWORD PTR [r15], rax");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");
    emitter.instruction("xor r10, r10");
    emitter.label("__rt_uus_both_copy_x86");
    emitter.instruction("cmp r10, rdx");
    emitter.instruction("jae __rt_uus_both_copied_x86");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r10]");
    emitter.instruction("mov BYTE PTR [r14 + r10], r11b");
    emitter.instruction("inc r10");
    emitter.instruction("jmp __rt_uus_both_copy_x86");
    emitter.label("__rt_uus_both_copied_x86");
    emitter.instruction("mov QWORD PTR [r13], rdx");                            // published LAST

    emitter.label("__rt_uus_cache_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the caller's own reference
    emitter.instruction("jmp __rt_uus_ret_x86");                                // share the common return path

    // -- the class does not implement url_stat: warn the way php does, then box false --
    emitter.label("__rt_uus_false_obj_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // the wrapper object
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // class_id stored at its head
    abi::emit_symbol_address(emitter, "r10", "_uwmh_head");
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // the caller's half
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");
    abi::emit_symbol_address(emitter, "r10", "_uwmh_tail");
    emitter.instruction("mov rcx, QWORD PTR [r10]");                            // the method's half
    emitter.instruction("mov r8, QWORD PTR [r10 + 8]");
    emitter.instruction("call __rt_wrapper_missing_hook_warning");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the throwaway wrapper object
    emitter.instruction("call __rt_decref_any");                                // free it before falling through to boxed false
    emitter.label("__rt_uus_false_x86");
    emitter.instruction("xor eax, eax");                                        // null sentinel → boxed false (scheme matched, stat unavailable)
    emitter.instruction("call __rt_box_wrapper_stat_result");                   // produce boxed false; _url_stat_matched stays 1
    emitter.instruction("jmp __rt_uus_ret_x86");                                // share the common return path

    emitter.label("__rt_uus_nomatch_x86");

    // See the AArch64 twin: a plain path takes php's one slot unless it never fills it.
    abi::emit_symbol_address(emitter, "r9", "_us_gentle");
    emitter.instruction("mov r9, QWORD PTR [r9]");
    emitter.instruction("test r9, r9");
    emitter.instruction("jnz __rt_uus_plain_kept_x86");                         // it fills nothing, so it empties nothing
    emit_select_stat_slot_x86(emitter, "__rt_uus_plain_link_x86", "__rt_uus_plain_chosen_x86");
    emitter.instruction("mov QWORD PTR [r13], 0");                              // the slot answers for nothing
    emitter.instruction("mov rax, QWORD PTR [r15]");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_uus_plain_kept_x86");
    emitter.instruction("mov QWORD PTR [r15], 0");                              // cleared BEFORE the release
    emitter.instruction("call __rt_decref_any");                                // the reference the slot held
    emitter.label("__rt_uus_plain_kept_x86");

    abi::emit_symbol_address(emitter, "r10", "_url_stat_matched");              // out-flag address
    emitter.instruction("mov BYTE PTR [r10], 0");                               // _url_stat_matched = 0 — caller falls back to the real filesystem
    emitter.instruction("xor eax, eax");                                        // return 0; the caller ignores it when the flag is 0

    emitter.label("__rt_uus_ret_x86");
    emitter.instruction("add rsp, 64");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed result (or 0 on no match)
}

/// Emits `__rt_user_wrapper_url_stat_field(path_ptr, path_len, field_sel, flags)`.
///
/// Calls `__rt_user_wrapper_url_stat` (which sets `_url_stat_matched`) and reads
/// the stat array it returns. The selector picks what comes back:
///
/// | sel | key(s) read        | result                                    |
/// |-----|--------------------|-------------------------------------------|
/// | 0   | `size`             | the integer field, or `-1`                |
/// | 1   | `mode`             | the integer field, or `-1`                |
/// | 2   | `mtime`            | the integer field, or `-1`                |
/// | 3   | `mode`+`uid`+`gid` | `is_readable()` as 0/1, or 0              |
/// | 4   | `mode`+`uid`+`gid` | `is_writable()` as 0/1, or 0              |
/// | 5   | `mode`+`uid`+`gid` | `is_executable()` as 0/1, or 0            |
///
/// Backs the whole stat family on `scheme://` URLs; the caller reads
/// `_url_stat_matched` to choose between this result and the real-filesystem
/// fallback. The boolean selectors report a plain `false` rather than the `-1`
/// sentinel, because their callers store the answer straight into a PHP bool
/// where `-1` would read as true.
///
/// The three boolean selectors read three keys from ONE `url_stat` call: PHP
/// calls a wrapper's `url_stat()` once per predicate, and reading the fields
/// through separate calls would make a wrapper with side effects observe a
/// second one.
///
/// Integer selectors return the field in `x0`/`rax` plus a success flag in `x1`/`rdx`; the flag is
/// clear whenever the payload is the `-1` sentinel. `-1` alone could not distinguish "absent" from
/// a real field value for callers that must box PHP `false`, and it is a value a wrapper is free to
/// report. `is_file()` reads the payload register only, so the flag is inert for it. Reuses the
/// boxed-Mixed reader (`__rt_mixed_array_get`) with a `__rt_hash_normalize_key`-normalized string
/// key, then releases both the field box and the stat-array box.
pub fn emit_user_wrapper_url_stat_field(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_url_stat_field_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_field ---");
    emitter.label_global("__rt_user_wrapper_url_stat_field");

    // Frame: 80 bytes. [sp,#0..16] x29/x30, [sp,#16] field_sel, [sp,#24] stat
    //   Mixed, [sp,#32] primary field, [sp,#40] found flag, [sp,#48] uid,
    //   [sp,#56] gid.
    emitter.instruction("sub sp, sp, #80");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the field selector (see the table above)
    emitter.instruction("mov x2, x3");                                          // url_stat flags chosen by the calling builtin, not a fixed 0
    emitter.instruction("bl __rt_user_wrapper_url_stat");                       // x0 = boxed Mixed stat array (sets _url_stat_matched)
    emitter.instruction("cbz x0, __rt_uusf_fail");                              // scheme not matched / null → sentinel (caller ignores when unmatched)
    emitter.instruction("ldr x9, [x0]");                                        // boxed Mixed runtime tag
    emitter.instruction("cmp x9, #3");                                          // wrapper reported the path absent (boxed false)?
    emitter.instruction("b.eq __rt_uusf_fail_box");                             // → release the false box and return the sentinel
    emitter.instruction("str x0, [sp, #24]");                                   // save the stat-array Mixed across the key lookups
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the field selector
    emitter.instruction("cmp x10, #3");                                         // selectors 3..5 are the permission predicates
    emitter.instruction("b.ge __rt_uusf_access");                               // → read mode, uid and gid together

    // -- single integer field: select the stat-array key string --
    emitter.instruction("cmp x10, #1");                                         // selector 1 = 'mode'
    emitter.instruction("b.eq __rt_uusf_mode");                                 // → load the mode key
    emitter.instruction("cmp x10, #2");                                         // selector 2 = 'mtime'
    emitter.instruction("b.eq __rt_uusf_mtime");                                // → load the mtime key
    abi::emit_symbol_address(emitter, "x1", "_stat_key_size");
    emitter.instruction("mov x2, #4");                                          // strlen("size")
    emitter.instruction("b __rt_uusf_havekey");                                 // proceed with the size key
    emitter.label("__rt_uusf_mode");
    abi::emit_symbol_address(emitter, "x1", "_stat_key_mode");
    emitter.instruction("mov x2, #4");                                          // strlen("mode")
    emitter.instruction("b __rt_uusf_havekey");                                 // proceed with the mode key
    emitter.label("__rt_uusf_mtime");
    abi::emit_symbol_address(emitter, "x1", "_stat_key_mtime");
    emitter.instruction("mov x2, #5");                                          // strlen("mtime")
    emitter.label("__rt_uusf_havekey");
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed → reader receiver
    emitter.instruction("bl __rt_uusf_read");                                   // x0 = integer field, x1 = 1 when present and integral
    emitter.instruction("str x0, [sp, #32]");                                   // stash the field across the array release
    emitter.instruction("str x1, [sp, #40]");                                   // stash whether it was readable at all
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed stat array
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the found flag
    emitter.instruction("cbz x1, __rt_uusf_fail");                              // missing/non-int field → sentinel
    emitter.instruction("ldr x0, [sp, #32]");                                   // load the integer result
    emitter.instruction("mov x1, #1");                                          // success flag for callers that box int|false
    emitter.instruction("b __rt_uusf_ret");                                     // return it

    // -- permission predicate: mode, uid and gid from the same stat array --
    emitter.label("__rt_uusf_access");
    abi::emit_symbol_address(emitter, "x1", "_stat_key_mode");
    emitter.instruction("mov x2, #4");                                          // strlen("mode")
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed → reader receiver
    emitter.instruction("bl __rt_uusf_read");                                   // x0 = mode, x1 = whether it was present
    emitter.instruction("str x0, [sp, #32]");                                   // stash the mode
    emitter.instruction("str x1, [sp, #40]");                                   // stash whether the mode was readable at all
    abi::emit_symbol_address(emitter, "x1", "_stat_key_uid");
    emitter.instruction("mov x2, #3");                                          // strlen("uid")
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed → reader receiver
    emitter.instruction("bl __rt_uusf_read");                                   // x0 = uid (0 when the wrapper omitted it, as PHP zero-fills)
    emitter.instruction("str x0, [sp, #48]");                                   // stash the reported owner uid
    abi::emit_symbol_address(emitter, "x1", "_stat_key_gid");
    emitter.instruction("mov x2, #3");                                          // strlen("gid")
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed → reader receiver
    emitter.instruction("bl __rt_uusf_read");                                   // x0 = gid (0 when the wrapper omitted it)
    emitter.instruction("str x0, [sp, #56]");                                   // stash the reported owning gid
    emitter.instruction("ldr x0, [sp, #24]");                                   // stat-array Mixed
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed stat array
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload whether the mode was present
    emitter.instruction("cbz x9, __rt_uusf_fail");                              // no integer 'mode' → the predicate is false
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the selector to pick the permission bit
    emitter.instruction("mov x10, #4");                                         // selector 3 (is_readable) wants the read bit
    emitter.instruction("mov x11, #2");                                         // selector 4 (is_writable) wants the write bit
    emitter.instruction("cmp x9, #4");                                          // is this the is_writable predicate?
    emitter.instruction("csel x10, x11, x10, eq");                              // pick the write bit when it is
    emitter.instruction("mov x11, #1");                                         // selector 5 (is_executable) wants the execute bit
    emitter.instruction("cmp x9, #5");                                          // is this the is_executable predicate?
    emitter.instruction("csel x10, x11, x10, eq");                              // pick the execute bit when it is
    emitter.instruction("ldr x0, [sp, #32]");                                   // mode
    emitter.instruction("ldr x1, [sp, #48]");                                   // reported owner uid
    emitter.instruction("ldr x2, [sp, #56]");                                   // reported owning gid
    emitter.instruction("mov x3, x10");                                         // the permission bit this predicate asks about
    emitter.instruction("bl __rt_stat_mode_access");                            // apply PHP's triad-selection rule
    emitter.instruction("b __rt_uusf_ret");                                     // return the boolean

    emitter.label("__rt_uusf_fail_box");
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed-false stat result (x0)
    emitter.label("__rt_uusf_fail");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the selector to choose the sentinel
    emitter.instruction("mov x0, #-1");                                         // integer selectors report -1
    emitter.instruction("mov x10, #0");                                         // boolean selectors report false
    emitter.instruction("cmp x9, #3");                                          // selectors 3..5 are the permission predicates
    emitter.instruction("csel x0, x10, x0, ge");                                // a -1 stored into a PHP bool would read as true
    emitter.instruction("mov x1, #0");                                          // failure flag: `filesize()` boxes PHP false rather than -1

    emitter.label("__rt_uusf_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the integer field, the boolean, or the sentinel

    // -- internal: read one integer field out of a borrowed stat array --
    // Inputs: x0 = stat-array Mixed (borrowed), x1/x2 = key pointer/length.
    // Outputs: x0 = the integer (0 when absent or not an int), x1 = 1 when it
    // was present AND an integer. Factored out because the permission selectors
    // read three keys from one array, and open-coding the read three times is
    // how the release of the value box drifts from the release of the array.
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_field (field reader) ---");
    emitter.label_global("__rt_uusf_read");
    emitter.instruction("sub sp, sp, #48");                                     // reader frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the reader frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the borrowed stat array across the key normalization
    emitter.instruction("bl __rt_hash_normalize_key");                          // normalize the string key → key_lo/key_hi in x1/x2
    emitter.instruction("ldr x0, [sp, #16]");                                   // stat-array Mixed → reader receiver
    emitter.instruction("mov x3, xzr");                                         // optional stat fields are probed without PHP undefined-key warnings
    emitter.instruction("bl __rt_mixed_array_get");                             // x0 = boxed Mixed value at the key (Mixed null on miss)
    emitter.instruction("mov x10, x0");                                         // keep the value box for release
    emitter.instruction("ldr x9, [x0]");                                        // value runtime tag
    emitter.instruction("ldr x11, [x0, #8]");                                   // integer payload (only meaningful for tag 0)
    emitter.instruction("str x9, [sp, #24]");                                   // stash the tag across the release
    emitter.instruction("str x11, [sp, #32]");                                  // stash the payload across the release
    emitter.instruction("mov x0, x10");                                         // value box
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed field value
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the tag
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the payload
    emitter.instruction("cmp x9, #0");                                          // was the field an integer?
    emitter.instruction("csel x0, x0, xzr, eq");                                // a non-integer field reads as 0
    emitter.instruction("cset x1, eq");                                         // and reports "absent" to the caller
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the reader frame
    emitter.instruction("ret");                                                 // return the field and its presence flag
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper url stat field.
fn emit_user_wrapper_url_stat_field_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_field ---");
    emitter.label_global("__rt_user_wrapper_url_stat_field");

    // Frame: [rbp-8] field_sel, [rbp-16] stat Mixed, [rbp-24] primary field,
    //   [rbp-32] found flag, [rbp-40] uid, [rbp-48] gid.
    // push rbp then sub rsp,64 keeps rsp 16-aligned for the helper calls.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // spill slots for the selector, the array and the read fields
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // save the field selector (see the table on the AArch64 emitter)
    emitter.instruction("mov rdx, rcx");                                        // url_stat flags chosen by the calling builtin, not a fixed 0
    emitter.instruction("call __rt_user_wrapper_url_stat");                     // rax = boxed Mixed stat array (sets _url_stat_matched)
    emitter.instruction("test rax, rax");                                       // scheme not matched / null?
    emitter.instruction("jz __rt_uusf_fail_x86");                               // → sentinel (caller ignores when unmatched)
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // boxed Mixed runtime tag
    emitter.instruction("cmp r9, 3");                                           // wrapper reported the path absent (boxed false)?
    emitter.instruction("je __rt_uusf_fail_box_x86");                           // → release the false box and return the sentinel
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the stat-array Mixed across the key lookups
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the field selector
    emitter.instruction("cmp r10, 3");                                          // selectors 3..5 are the permission predicates
    emitter.instruction("jge __rt_uusf_access_x86");                            // → read mode, uid and gid together

    // -- single integer field: select the stat-array key string --
    emitter.instruction("cmp r10, 1");                                          // selector 1 = 'mode'
    emitter.instruction("je __rt_uusf_mode_x86");                               // → load the mode key
    emitter.instruction("cmp r10, 2");                                          // selector 2 = 'mtime'
    emitter.instruction("je __rt_uusf_mtime_x86");                              // → load the mtime key
    abi::emit_symbol_address(emitter, "rax", "_stat_key_size");                 // size key pointer (new_by_name-style rax/rdx string ABI)
    emitter.instruction("mov rdx, 4");                                          // strlen("size")
    emitter.instruction("jmp __rt_uusf_havekey_x86");                           // proceed with the size key
    emitter.label("__rt_uusf_mode_x86");
    abi::emit_symbol_address(emitter, "rax", "_stat_key_mode");                 // mode key pointer
    emitter.instruction("mov rdx, 4");                                          // strlen("mode")
    emitter.instruction("jmp __rt_uusf_havekey_x86");                           // proceed with the mode key
    emitter.label("__rt_uusf_mtime_x86");
    abi::emit_symbol_address(emitter, "rax", "_stat_key_mtime");                // mtime key pointer
    emitter.instruction("mov rdx, 5");                                          // strlen("mtime")
    emitter.label("__rt_uusf_havekey_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // stat-array Mixed → reader receiver
    emitter.instruction("call __rt_uusf_read_x86");                             // rax = integer field, rdx = 1 when present and integral
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // stash the field across the array release
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // stash whether it was readable at all
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // stat-array Mixed
    emitter.instruction("call __rt_decref_any");                                // release the boxed stat array
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the found flag
    emitter.instruction("test rdx, rdx");                                       // was the field present and an integer?
    emitter.instruction("jz __rt_uusf_fail_x86");                               // missing/non-int field → sentinel
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the integer result
    emitter.instruction("mov rdx, 1");                                          // success flag for callers that box int|false
    emitter.instruction("jmp __rt_uusf_ret_x86");                               // return it

    // -- permission predicate: mode, uid and gid from the same stat array --
    emitter.label("__rt_uusf_access_x86");
    abi::emit_symbol_address(emitter, "rax", "_stat_key_mode");                 // mode key pointer
    emitter.instruction("mov rdx, 4");                                          // strlen("mode")
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // stat-array Mixed → reader receiver
    emitter.instruction("call __rt_uusf_read_x86");                             // rax = mode, rdx = whether it was present
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // stash the mode
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // stash whether the mode was readable at all
    abi::emit_symbol_address(emitter, "rax", "_stat_key_uid");                  // uid key pointer
    emitter.instruction("mov rdx, 3");                                          // strlen("uid")
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // stat-array Mixed → reader receiver
    emitter.instruction("call __rt_uusf_read_x86");                             // rax = uid (0 when the wrapper omitted it, as PHP zero-fills)
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // stash the reported owner uid
    abi::emit_symbol_address(emitter, "rax", "_stat_key_gid");                  // gid key pointer
    emitter.instruction("mov rdx, 3");                                          // strlen("gid")
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // stat-array Mixed → reader receiver
    emitter.instruction("call __rt_uusf_read_x86");                             // rax = gid (0 when the wrapper omitted it)
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // stash the reported owning gid
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // stat-array Mixed
    emitter.instruction("call __rt_decref_any");                                // release the boxed stat array
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload whether the mode was present
    emitter.instruction("test r9, r9");                                         // no integer 'mode'?
    emitter.instruction("jz __rt_uusf_fail_x86");                               // → the predicate is false
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the selector to pick the permission bit
    emitter.instruction("mov ecx, 4");                                          // selector 3 (is_readable) wants the read bit
    emitter.instruction("mov r8d, 2");                                          // selector 4 (is_writable) wants the write bit
    emitter.instruction("cmp r10, 4");                                          // is this the is_writable predicate?
    emitter.instruction("cmove ecx, r8d");                                      // pick the write bit when it is
    emitter.instruction("mov r8d, 1");                                          // selector 5 (is_executable) wants the execute bit
    emitter.instruction("cmp r10, 5");                                          // is this the is_executable predicate?
    emitter.instruction("cmove ecx, r8d");                                      // pick the execute bit when it is
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // mode
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reported owner uid
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reported owning gid
    emitter.instruction("call __rt_stat_mode_access");                          // apply PHP's triad-selection rule
    emitter.instruction("jmp __rt_uusf_ret_x86");                               // return the boolean

    emitter.label("__rt_uusf_fail_box_x86");
    emitter.instruction("call __rt_decref_any");                                // release the boxed-false stat result (rax)
    emitter.label("__rt_uusf_fail_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the selector to choose the sentinel
    emitter.instruction("mov rax, -1");                                         // integer selectors report -1
    emitter.instruction("xor edx, edx");                                        // boolean selectors report false
    emitter.instruction("cmp r9, 3");                                           // selectors 3..5 are the permission predicates
    emitter.instruction("cmovge rax, rdx");                                     // a -1 stored into a PHP bool would read as true
    emitter.instruction("mov rdx, 0");                                          // failure flag: `filesize()` boxes PHP false rather than -1

    emitter.label("__rt_uusf_ret_x86");
    emitter.instruction("add rsp, 64");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the integer field, the boolean, or the sentinel

    // -- internal: read one integer field out of a borrowed stat array --
    // Inputs: rdi = stat-array Mixed (borrowed), rax/rdx = key pointer/length.
    // Outputs: rax = the integer (0 when absent or not an int), rdx = 1 when it
    // was present AND an integer.
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_url_stat_field (field reader) ---");
    emitter.label_global("__rt_uusf_read_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the reader frame pointer
    emitter.instruction("sub rsp, 48");                                         // spill slots for the array, the tag and the payload
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the borrowed stat array across the key normalization
    emitter.instruction("call __rt_hash_normalize_key");                        // normalize the string key → key_lo in rax, key_hi in rdx
    emitter.instruction("mov rsi, rax");                                        // key_lo → SysV second arg for the reader
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // stat-array Mixed → reader receiver
    emitter.instruction("xor ecx, ecx");                                        // optional stat fields are probed without PHP undefined-key warnings
    emitter.instruction("call __rt_mixed_array_get");                           // rax = boxed Mixed value at the key (Mixed null on miss)
    emitter.instruction("mov r10, rax");                                        // keep the value box for release
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // value runtime tag
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // integer payload (only meaningful for tag 0)
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // stash the tag across the release
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // stash the payload across the release
    emitter.instruction("mov rax, r10");                                        // value box
    emitter.instruction("call __rt_decref_any");                                // release the boxed field value
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the tag
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the payload
    emitter.instruction("xor edx, edx");                                        // zero source for the non-integer case
    emitter.instruction("test r9, r9");                                         // was the field an integer (tag 0)?
    emitter.instruction("cmovne rax, rdx");                                     // a non-integer field reads as 0
    emitter.instruction("sete dl");                                             // and reports "absent" to the caller
    emitter.instruction("movzx edx, dl");                                       // widen the presence flag
    emitter.instruction("add rsp, 48");                                         // release the reader frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the field and its presence flag
}
