//! Purpose:
//! Emits `__rt_hash_ksort`, `__rt_hash_krsort`, `__rt_hash_asort`, `__rt_hash_arsort`,
//! `__rt_hash_natsort` and `__rt_hash_natcasesort`, the runtime sorters behind PHP's
//! order-preserving associative-array sorts, plus the shared `__rt_hash_sort_links`
//! engine, its `__rt_hash_sort_triple` operand reader, and the `__rt_hash_natcmp` /
//! `__rt_hash_natcasecmp` adapters that give the engine php's natural order.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - A hash table's iteration order lives ONLY in the insertion-order doubly-linked list
//!   (`header[24]` = head, `header[32]` = tail, entry `+48` = prev, entry `+56` = next).
//!   Sorting therefore relinks that chain and never moves a bucket, so open-addressing
//!   probe positions, key/value association, and every refcount stay exactly as they were:
//!   these helpers acquire, persist and release nothing.
//! - The algorithm is a linked-list insertion sort that scans the already-sorted suffix
//!   backwards from its tail. That makes it stable — PHP 8 sorts are stable, and ties are
//!   observable for keys such as `'01'` and `' 1'`, which PHP compares equal — and linear
//!   on input that is already ordered.
//! - Ordering is a parameter, not a branch: each entry point hands the engine the address
//!   of its comparator, so `ksort`/`asort` follow PHP 8's own comparison table through
//!   `__rt_php_compare` (`10 < 'Banana'`, `'0.5' < 2`, …) while `natsort`/`natcasesort`
//!   follow php's `strnatcmp_ex`. Nothing else differs between them, which is what makes
//!   `natsort` key-preserving here: php's
//!   `zend_array_sort(..., php_array_natural_compare, 0)` passes `renumber = 0`, exactly
//!   like `asort`, so only the iteration order moves. Selecting per entry point also keeps
//!   the linker able to drop the natural comparators from a program that never natsorts.
//! - Callers must split shared tables with `__rt_hash_ensure_unique` first: these helpers
//!   mutate the table they are handed.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::NULL_SENTINEL;

/// Mode word selecting an ascending key sort (`ksort`).
pub(super) const MODE_KEY_ASCENDING: i64 = 0;

/// Mode word selecting a descending key sort (`krsort`).
pub(super) const MODE_KEY_DESCENDING: i64 = 1;

/// Mode word selecting an ascending value sort (`asort`).
pub(super) const MODE_VALUE_ASCENDING: i64 = 2;

/// Mode word selecting a descending value sort (`arsort`).
pub(super) const MODE_VALUE_DESCENDING: i64 = 3;

/// The comparator behind every non-natural sort: PHP 8's own comparison table.
const CMP_PHP: &str = "__rt_php_compare";

/// The comparator behind `ksort`/`krsort`: php's exact SORT_REGULAR ordering for KEYS.
///
/// Not `__rt_php_compare`: that is php's ordering table for VALUES, and it calls two numeric
/// strings equal once they round to the same binary64. php orders integer-spelled keys exactly,
/// so `"+9007199254740992"` sorts before `"+9007199254740993"` where the value table sees one
/// number twice and a stable sort leaves them in insertion order.
const CMP_KEY: &str = "__rt_hash_keycmp";

/// The comparator behind `natsort`: php's `strnatcmp_ex` over two string payloads.
const CMP_NATURAL: &str = "__rt_hash_natcmp";

/// The comparator behind `natcasesort`: the same, folding case.
const CMP_NATURAL_CASE: &str = "__rt_hash_natcasecmp";

/// Emits every hash link-order sort helper for the active target.
///
/// Publishes the six PHP-facing entry points (`__rt_hash_ksort`, `__rt_hash_krsort`,
/// `__rt_hash_asort`, `__rt_hash_arsort`, `__rt_hash_natsort`, `__rt_hash_natcasesort`),
/// the shared `__rt_hash_sort_links` engine, the `__rt_hash_sort_triple` operand reader
/// and the two natural-order comparator adapters. Every entry point takes the hash-table
/// pointer in the first integer argument register and returns nothing; the table is
/// mutated in place by relinking its insertion-order chain.
pub fn emit_hash_sort(emitter: &mut Emitter) {
    // `__rt_key_compare_regular` and its siblings are emitted here rather than from an emitter
    // list because the eval bridge CALLS them: `__elephc_eval_value_regular_key_compare` branches
    // straight to `__rt_key_compare_regular`, so a binary that omits them fails to LINK — a
    // failure no type-check can show, which is why the merge nearly shipped without it.
    super::hash_key_compare::emit_hash_key_compare(emitter);
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_sort_entry_points_x86_64(emitter);
        emit_hash_sort_links_x86_64(emitter);
        emit_hash_sort_triple_x86_64(emitter);
        emit_hash_keycmp_x86_64(emitter);
        emit_hash_natcmp_x86_64(emitter, "__rt_hash_natcmp", "__rt_natcmp");
        emit_hash_natcmp_x86_64(emitter, "__rt_hash_natcasecmp", "__rt_natcasecmp");
        return;
    }
    emit_hash_sort_entry_points_aarch64(emitter);
    emit_hash_sort_links_aarch64(emitter);
    emit_hash_sort_triple_aarch64(emitter);
    emit_hash_keycmp_aarch64(emitter);
    emit_hash_natcmp_aarch64(emitter, "__rt_hash_natcmp", "__rt_natcmp");
    emit_hash_natcmp_aarch64(emitter, "__rt_hash_natcasecmp", "__rt_natcasecmp");
}

/// Emits the AArch64 adapter bridging the sort engine's triple ABI to
/// `__rt_key_compare_regular`'s `(low, high, low, high)` key ABI.
///
/// In: `x0`/`x3` = the two runtime tags, `x1`/`x4` = low payload words, `x2`/`x5` = high payload
/// words — exactly what the engine already staged. A string key carries its pointer in the low
/// word and its length in the high word; an integer key carries its value in the low word.
///
/// The high word is REBUILT for an integer key rather than forwarded: `__rt_hash_sort_triple`
/// zeroes it, because a triple's high word is a payload and an integer has none, while
/// `__rt_key_compare_regular` recognises an integer key by an all-ones high word. Forwarding the
/// zero would present every integer key as a zero-length string.
fn emit_hash_keycmp_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_hash_keycmp (triple ABI -> __rt_key_compare_regular) ---");
    emitter.label_global("__rt_hash_keycmp");

    emitter.instruction("cmp x0, #1");                                          // runtime tag 1 = string on the left key
    emitter.instruction("b.eq __rt_hash_keycmp_left_str");
    emitter.instruction("mov x2, #-1");                                         // an integer key is marked by the all-ones high word
    emitter.label("__rt_hash_keycmp_left_str");
    emitter.instruction("cmp x3, #1");                                          // runtime tag 1 = string on the right key
    emitter.instruction("b.eq __rt_hash_keycmp_right_str");
    emitter.instruction("mov x5, #-1");                                         // the same sentinel for an integer right key
    emitter.label("__rt_hash_keycmp_right_str");
    emitter.instruction("mov x0, x1");                                          // left low word becomes the first key argument
    emitter.instruction("mov x1, x2");                                          // left high word or sentinel becomes the second
    emitter.instruction("mov x2, x4");                                          // right low word becomes the third
    emitter.instruction("mov x3, x5");                                          // right high word or sentinel becomes the fourth
    emitter.instruction("b __rt_key_compare_regular");                          // tail-branch: the -1/0/1 result is already ours to return
}

/// Emits the x86_64 System V form of [`emit_hash_keycmp_aarch64`].
///
/// In: `rdi`/`rcx` = the two runtime tags, `rsi`/`r8` = low payload words, `rdx`/`r9` = high
/// payload words. `__rt_key_compare_regular` wants `rdi`/`rsi` = the left key pair and
/// `rdx`/`rcx` = the right one, so `rcx` is read as the right TAG before it is overwritten with
/// the right high word — the one ordering constraint in the shuffle.
fn emit_hash_keycmp_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_hash_keycmp (triple ABI -> __rt_key_compare_regular) ---");
    emitter.label_global("__rt_hash_keycmp");

    emitter.instruction("cmp rdi, 1");                                          // runtime tag 1 = string on the left key
    emitter.instruction("je __rt_hash_keycmp_left_str_x86");
    emitter.instruction("mov rdx, -1");                                         // an integer key is marked by the all-ones high word
    emitter.label("__rt_hash_keycmp_left_str_x86");
    emitter.instruction("cmp rcx, 1");                                          // runtime tag 1 = string on the right key
    emitter.instruction("je __rt_hash_keycmp_right_str_x86");
    emitter.instruction("mov r9, -1");                                          // the same sentinel for an integer right key
    emitter.label("__rt_hash_keycmp_right_str_x86");
    emitter.instruction("mov rdi, rsi");                                        // left low word becomes the first key argument
    emitter.instruction("mov rsi, rdx");                                        // left high word or sentinel becomes the second
    emitter.instruction("mov rdx, r8");                                         // right low word becomes the third
    emitter.instruction("mov rcx, r9");                                         // right high word or sentinel becomes the fourth
    emitter.instruction("jmp __rt_key_compare_regular");                        // tail-jump: the -1/0/1 result is already ours to return
}

/// Emits the AArch64 natural-order adapter bridging `__rt_php_compare`'s triple ABI to
/// `__rt_natcmp`'s `(ptr, len, ptr, len)` ABI.
///
/// In: `x0`/`x3` = the two runtime tags, `x1`/`x4` = low payload words, `x2`/`x5` = high
/// payload words — exactly what the sort engine already staged for `__rt_php_compare`.
/// A string operand carries its pointer in the low word and its length in the high word,
/// so runtime tag 1 on BOTH sides is what makes the reinterpretation legal.
///
/// php's `natsort` compares through `zval_get_tmp_string()`, so it orders every value as a
/// string. This backend only routes string-valued hashes here, which is why the non-string
/// path is a guard rather than a conversion: it exists so a tag that is not a string can
/// never be dereferenced as a pointer, not to define an ordering the lowering can reach.
fn emit_hash_natcmp_aarch64(emitter: &mut Emitter, label: &str, target: &str) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} (triple ABI -> {}) ---", label, target));
    emitter.label_global(label);

    emitter.instruction("cmp x0, #1");                                          // runtime tag 1 = string on the left operand
    emitter.instruction(&format!("b.ne {}_fallback", label));
    emitter.instruction("cmp x3, #1");                                          // runtime tag 1 = string on the right operand
    emitter.instruction(&format!("b.ne {}_fallback", label));
    emitter.instruction("mov x3, x4");                                          // the right operand's pointer becomes natcmp's third argument
    emitter.instruction("mov x4, x5");                                          // the right operand's length becomes natcmp's fourth argument
    emitter.instruction(&format!("b {}", target));                              // tail-branch: x1/x2 already hold the left pointer and length

    emitter.label(&format!("{}_fallback", label));
    emitter.instruction("b __rt_php_compare");                                  // a non-string operand keeps PHP 8's ordering table
}

/// Emits the x86_64 System V form of [`emit_hash_natcmp_aarch64`].
///
/// In: `rdi`/`rcx` = the two runtime tags, `rsi`/`r8` = low payload words, `rdx`/`r9` =
/// high payload words. `__rt_natcmp` wants `rdi` = a ptr, `rsi` = a len, `rdx` = b ptr,
/// `rcx` = b len, so the four moves run in an order that never overwrites a word still
/// needed by a later one.
fn emit_hash_natcmp_x86_64(emitter: &mut Emitter, label: &str, target: &str) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} (triple ABI -> {}) ---", label, target));
    emitter.label_global(label);

    emitter.instruction("cmp rdi, 1");                                          // runtime tag 1 = string on the left operand
    emitter.instruction(&format!("jne {}_fallback", label));
    emitter.instruction("cmp rcx, 1");                                          // runtime tag 1 = string on the right operand
    emitter.instruction(&format!("jne {}_fallback", label));
    emitter.instruction("mov rdi, rsi");                                        // left pointer into natcmp's first argument
    emitter.instruction("mov rsi, rdx");                                        // left length into natcmp's second argument
    emitter.instruction("mov rdx, r8");                                         // right pointer into natcmp's third argument
    emitter.instruction("mov rcx, r9");                                         // right length into natcmp's fourth argument
    emitter.instruction(&format!("jmp {}", target));                            // tail-jump so the comparison returns to the sort engine

    emitter.label(&format!("{}_fallback", label));
    emitter.instruction("jmp __rt_php_compare");                                // a non-string operand keeps PHP 8's ordering table
}

/// Emits the six AArch64 entry stubs that select a mode plus a comparator and enter the
/// shared engine.
///
/// Each stub loads its mode word into `x1` and its comparator's address into `x2`, then
/// tail-branches to `__rt_hash_sort_links`, so the engine's stack frame and return address
/// belong to the original caller.
fn emit_hash_sort_entry_points_aarch64(emitter: &mut Emitter) {
    for (label, mode, comparator, description) in hash_sort_entry_points() {
        emitter.blank();
        emitter.comment(&format!("--- runtime: {} ({}) ---", label, description));
        emitter.label_global(label);
        emitter.instruction(&format!("mov x1, #{}", mode));                     // select the key/value and ascending/descending sort mode
        abi::emit_symbol_address(emitter, "x2", comparator);                    // select this sort's ordering function
        emitter.instruction("b __rt_hash_sort_links");                          // enter the shared insertion-order relinking engine
    }
}

/// Emits the six x86_64 entry stubs that select a mode plus a comparator and enter the
/// shared engine.
///
/// Each stub loads its mode word into `rsi` and its comparator's address into `rdx`, then
/// tail-jumps to `__rt_hash_sort_links`, mirroring the AArch64 stubs one-for-one.
fn emit_hash_sort_entry_points_x86_64(emitter: &mut Emitter) {
    for (label, mode, comparator, description) in hash_sort_entry_points() {
        emitter.blank();
        emitter.comment(&format!("--- runtime: {} ({}) ---", label, description));
        emitter.label_global(label);
        emitter.instruction(&format!("mov esi, {}", mode));                     // select the key/value and ascending/descending sort mode
        abi::emit_symbol_address(emitter, "rdx", comparator);                   // select this sort's ordering function
        emitter.instruction("jmp __rt_hash_sort_links");                        // enter the shared insertion-order relinking engine
    }
}

/// Returns the PHP-facing hash sort entry points with their mode words, comparators and
/// descriptions.
///
/// The comparator is chosen HERE, per entry point, rather than branched on inside the shared
/// engine. That keeps each sort's dependency on its own atom: a program that only calls
/// `ksort()` never references `__rt_hash_natcmp`, so macOS `-dead_strip` and Linux
/// `--gc-sections` still drop php's ~1.4 KB natural comparator pair from the binary.
fn hash_sort_entry_points() -> [(&'static str, i64, &'static str, &'static str); 6] {
    [
        ("__rt_hash_ksort", MODE_KEY_ASCENDING, CMP_KEY, "sort a hash by key ascending"),
        ("__rt_hash_krsort", MODE_KEY_DESCENDING, CMP_KEY, "sort a hash by key descending"),
        ("__rt_hash_asort", MODE_VALUE_ASCENDING, CMP_PHP, "sort a hash by value ascending"),
        ("__rt_hash_arsort", MODE_VALUE_DESCENDING, CMP_PHP, "sort a hash by value descending"),
        (
            "__rt_hash_natsort",
            MODE_VALUE_ASCENDING,
            CMP_NATURAL,
            "sort a hash by value in natural order",
        ),
        (
            "__rt_hash_natcasesort",
            MODE_VALUE_ASCENDING,
            CMP_NATURAL_CASE,
            "sort a hash by value in case-insensitive natural order",
        ),
    ]
}

/// Emits the AArch64 `__rt_hash_sort_links` engine.
///
/// Input `x0` = hash-table pointer, `x1` = mode word (bit 0 = descending, bit 1 = sort by
/// value), `x2` = comparator address. The routine detaches entries from the insertion-order
/// chain one at a time and reinserts each into a growing sorted chain, scanning that chain
/// backwards from its tail so equal operands keep their original relative order. Null
/// pointers, the in-band null-container sentinel, and tables with fewer than two live
/// entries return untouched.
///
/// Frame (112 bytes): `[sp,#0]` table, `[sp,#8]` entries base, `[sp,#16]` mode,
/// `[sp,#24]` sorted head, `[sp,#32]` sorted tail, `[sp,#40]` current slot,
/// `[sp,#48]` next source slot, `[sp,#56]` backward scan cursor,
/// `[sp,#64..#80]` the current entry's comparison triple, `[sp,#88]` comparator,
/// `[sp,#96]` saved `x29`/`x30`.
fn emit_hash_sort_links_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_sort_links ---");
    emitter.label_global("__rt_hash_sort_links");

    // -- reject containers that carry no sortable insertion-order chain --
    emitter.instruction("cbz x0, __rt_hsort_ret");                              // null tables from missed reads have nothing to reorder
    abi::emit_load_int_immediate(emitter, "x9", NULL_SENTINEL);
    emitter.instruction("cmp x0, x9");                                          // does the table carry the in-band null-container sentinel?
    emitter.instruction("b.eq __rt_hsort_ret");                                 // sentinel-null tables have no header to relink
    emitter.instruction("ldr x9, [x0]");                                        // x9 = live entry count from the hash header
    emitter.instruction("cmp x9, #2");                                          // does the table hold at least two entries?
    emitter.instruction("b.ge __rt_hsort_begin");                               // only multi-entry tables can change order

    emitter.label("__rt_hsort_ret");
    emitter.instruction("ret");                                                 // return with the table left exactly as it was

    // -- establish the sort frame and seed an empty destination chain --
    emitter.label("__rt_hsort_begin");
    emitter.instruction("sub sp, sp, #112");                                    // allocate the link-sort frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the link-sort frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the hash-table pointer for the final header update
    emitter.instruction("add x9, x0, #40");                                     // compute the entries region base past the 40-byte header
    emitter.instruction("str x9, [sp, #8]");                                    // save the entries base used by every slot address computation
    emitter.instruction("str x1, [sp, #16]");                                   // save the key/value and direction mode word
    emitter.instruction("str x2, [sp, #88]");                                   // save this sort's comparator for the whole run
    emitter.instruction("mov x9, #-1");                                         // the destination chain starts empty
    emitter.instruction("str x9, [sp, #24]");                                   // sorted head = none
    emitter.instruction("str x9, [sp, #32]");                                   // sorted tail = none
    emitter.instruction("ldr x9, [x0, #24]");                                   // x9 = current insertion-order head slot
    emitter.instruction("str x9, [sp, #40]");                                   // start consuming the source chain from its head

    // -- outer loop: detach the next source entry and read its comparison triple --
    emitter.label("__rt_hsort_outer");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the slot being placed
    emitter.instruction("cmn x9, #1");                                          // has the source chain been fully consumed?
    emitter.instruction("b.eq __rt_hsort_finish");                              // publish the sorted chain once no source entry remains
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the entries region base
    emitter.instruction("add x11, x10, x9, lsl #6");                            // x11 = address of the 64-byte entry being placed
    emitter.instruction("ldr x12, [x11, #56]");                                 // read the source successor before the entry is relinked
    emitter.instruction("str x12, [sp, #48]");                                  // remember where the source walk resumes
    emitter.instruction("mov x0, x11");                                         // pass the entry address to the operand reader
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the mode so the reader picks the key or the value
    emitter.instruction("bl __rt_hash_sort_triple");                            // materialize the entry's PHP comparison triple
    emitter.instruction("str x0, [sp, #64]");                                   // cache the placed entry's runtime tag
    emitter.instruction("str x1, [sp, #72]");                                   // cache the placed entry's low payload word
    emitter.instruction("str x2, [sp, #80]");                                   // cache the placed entry's high payload word
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the sorted chain's tail
    emitter.instruction("str x9, [sp, #56]");                                   // start the backward scan at that tail

    // -- inner loop: walk the sorted chain backwards to the stable insertion point --
    emitter.label("__rt_hsort_scan");
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the backward scan cursor
    emitter.instruction("cmn x9, #1");                                          // has the scan run off the front of the sorted chain?
    emitter.instruction("b.eq __rt_hsort_insert");                              // the entry belongs at the head of the sorted chain
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the entries region base
    emitter.instruction("add x0, x10, x9, lsl #6");                             // x0 = address of the sorted entry under the cursor
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the mode so the reader picks the key or the value
    emitter.instruction("bl __rt_hash_sort_triple");                            // materialize the scanned entry's comparison triple
    emitter.instruction("ldr x3, [sp, #64]");                                   // pass the placed entry's tag as the right operand
    emitter.instruction("ldr x4, [sp, #72]");                                   // pass the placed entry's low payload word
    emitter.instruction("ldr x5, [sp, #80]");                                   // pass the placed entry's high payload word
    emitter.instruction("ldr x9, [sp, #88]");                                   // reload this sort's comparator
    emitter.instruction("blr x9");                                              // apply the selected ordering to scanned versus placed
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the mode word to pick the direction test
    emitter.instruction("tbnz x9, #0, __rt_hsort_scan_desc");                   // descending sorts invert the stop condition
    emitter.instruction("cmp x0, #0");                                          // does the scanned entry already sort at or before the placed one?
    emitter.instruction("b.le __rt_hsort_insert");                              // stopping on equality keeps ties in their original order
    emitter.instruction("b __rt_hsort_scan_prev");                              // otherwise keep walking towards the chain head

    emitter.label("__rt_hsort_scan_desc");
    emitter.instruction("cmp x0, #0");                                          // does the scanned entry already sort at or before the placed one?
    emitter.instruction("b.ge __rt_hsort_insert");                              // stopping on equality keeps ties in their original order

    emitter.label("__rt_hsort_scan_prev");
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the backward scan cursor
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the entries region base
    emitter.instruction("add x11, x10, x9, lsl #6");                            // x11 = address of the scanned entry
    emitter.instruction("ldr x12, [x11, #48]");                                 // follow the sorted chain's predecessor link
    emitter.instruction("str x12, [sp, #56]");                                  // advance the backward scan cursor
    emitter.instruction("b __rt_hsort_scan");                                   // keep scanning for the stable insertion point

    // -- splice the placed entry into the sorted chain --
    emitter.label("__rt_hsort_insert");
    emitter.instruction("ldr x9, [sp, #56]");                                   // x9 = predecessor slot, or -1 for a head insertion
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the entries region base
    emitter.instruction("ldr x11, [sp, #40]");                                  // x11 = the slot being placed
    emitter.instruction("add x12, x10, x11, lsl #6");                           // x12 = address of the entry being placed
    emitter.instruction("cmn x9, #1");                                          // is there a predecessor to splice after?
    emitter.instruction("b.ne __rt_hsort_insert_after");                        // splice after the located predecessor

    emitter.instruction("mov x13, #-1");                                        // a head insertion has no predecessor
    emitter.instruction("str x13, [x12, #48]");                                 // placed entry prev = none
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the current sorted head
    emitter.instruction("str x13, [x12, #56]");                                 // placed entry next = the old sorted head
    emitter.instruction("cmn x13, #1");                                         // was the sorted chain still empty?
    emitter.instruction("b.eq __rt_hsort_insert_first");                        // the first placed entry is also the sorted tail
    emitter.instruction("add x14, x10, x13, lsl #6");                           // x14 = address of the old sorted head
    emitter.instruction("str x11, [x14, #48]");                                 // old sorted head prev = the placed entry
    emitter.instruction("b __rt_hsort_insert_head");                            // publish the new sorted head

    emitter.label("__rt_hsort_insert_first");
    emitter.instruction("str x11, [sp, #32]");                                  // sorted tail = the first placed entry

    emitter.label("__rt_hsort_insert_head");
    emitter.instruction("str x11, [sp, #24]");                                  // sorted head = the placed entry
    emitter.instruction("b __rt_hsort_advance");                                // continue with the next source entry

    emitter.label("__rt_hsort_insert_after");
    emitter.instruction("add x13, x10, x9, lsl #6");                            // x13 = address of the predecessor entry
    emitter.instruction("ldr x14, [x13, #56]");                                 // x14 = the predecessor's current successor
    emitter.instruction("str x9, [x12, #48]");                                  // placed entry prev = the predecessor
    emitter.instruction("str x14, [x12, #56]");                                 // placed entry next = the predecessor's old successor
    emitter.instruction("str x11, [x13, #56]");                                 // predecessor next = the placed entry
    emitter.instruction("cmn x14, #1");                                         // was the predecessor the sorted tail?
    emitter.instruction("b.eq __rt_hsort_insert_tail");                         // then the placed entry becomes the new tail
    emitter.instruction("add x15, x10, x14, lsl #6");                           // x15 = address of the displaced successor
    emitter.instruction("str x11, [x15, #48]");                                 // displaced successor prev = the placed entry
    emitter.instruction("b __rt_hsort_advance");                                // continue with the next source entry

    emitter.label("__rt_hsort_insert_tail");
    emitter.instruction("str x11, [sp, #32]");                                  // sorted tail = the placed entry

    emitter.label("__rt_hsort_advance");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the remembered source successor
    emitter.instruction("str x9, [sp, #40]");                                   // resume the source walk from that entry
    emitter.instruction("b __rt_hsort_outer");                                  // place the next source entry

    // -- publish the sorted chain through the hash header --
    emitter.label("__rt_hsort_finish");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash-table pointer
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the sorted chain head
    emitter.instruction("str x9, [x0, #24]");                                   // header[24]: publish the new iteration-order head
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the sorted chain tail
    emitter.instruction("str x9, [x0, #32]");                                   // header[32]: publish the new iteration-order tail
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the link-sort frame
    emitter.instruction("ret");                                                 // return with the table reordered in place
}

/// Emits the AArch64 `__rt_hash_sort_triple` operand reader.
///
/// Input `x0` = hash entry address, `x1` = mode word; output is the `__rt_php_compare`
/// triple `x0` = runtime tag, `x1` = low payload word, `x2` = high payload word. Key mode
/// turns the normalized key encoding (`key_len == -1` marks an integer key) into tag 0 or
/// tag 1; value mode reads the entry payload and peels boxed Mixed cells (tag 7) through a
/// tail branch into `__rt_mixed_unbox`. String payloads stay borrowed from the table.
fn emit_hash_sort_triple_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_sort_triple ---");
    emitter.label_global("__rt_hash_sort_triple");

    emitter.instruction("tbnz x1, #1, __rt_hsort_triple_value");                // mode bit 1 selects the entry value instead of its key
    emitter.instruction("ldr x2, [x0, #16]");                                   // x2 = stored key length, or -1 for a normalized integer key
    emitter.instruction("ldr x1, [x0, #8]");                                    // x1 = stored key pointer, or the integer key payload
    emitter.instruction("cmn x2, #1");                                          // is this a normalized integer key?
    emitter.instruction("b.ne __rt_hsort_triple_key_str");                      // string keys keep their pointer and length
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("mov x2, #0");                                          // integer operands carry no high payload word
    emitter.instruction("ret");                                                 // return the integer key triple

    emitter.label("__rt_hsort_triple_key_str");
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("ret");                                                 // return the borrowed string key triple

    emitter.label("__rt_hsort_triple_value");
    emitter.instruction("ldr x3, [x0, #40]");                                   // x3 = the entry's per-entry runtime value tag
    emitter.instruction("ldr x1, [x0, #24]");                                   // x1 = the entry's low payload word
    emitter.instruction("ldr x2, [x0, #32]");                                   // x2 = the entry's high payload word
    emitter.instruction("cmp x3, #7");                                          // does the entry hold a boxed Mixed cell?
    emitter.instruction("b.eq __rt_hsort_triple_value_boxed");                  // boxed cells must be peeled before comparing
    emitter.instruction("mov x0, x3");                                          // unboxed entries already carry a concrete tag
    emitter.instruction("ret");                                                 // return the borrowed value triple

    emitter.label("__rt_hsort_triple_value_boxed");
    emitter.instruction("mov x0, x1");                                          // pass the borrowed Mixed cell to the unboxing helper
    emitter.instruction("b __rt_mixed_unbox");                                  // tail-branch so the peeled triple returns to our caller
}

/// Emits the x86_64 System V `__rt_hash_sort_links` engine.
///
/// Input `rdi` = hash-table pointer, `rsi` = mode word (bit 0 = descending, bit 1 = sort
/// by value), `rdx` = comparator address. Semantics are identical to the AArch64 engine,
/// including the stable backward scan and the untouched-on-empty early exits.
///
/// Frame (96 bytes below `rbp`): `[rbp-8]` table, `[rbp-16]` entries base, `[rbp-24]` mode,
/// `[rbp-32]` sorted head, `[rbp-40]` sorted tail, `[rbp-48]` current slot,
/// `[rbp-56]` next source slot, `[rbp-64]` backward scan cursor,
/// `[rbp-72..-88]` the current entry's comparison triple, `[rbp-96]` comparator.
fn emit_hash_sort_links_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_sort_links ---");
    emitter.label_global("__rt_hash_sort_links");

    // -- reject containers that carry no sortable insertion-order chain --
    emitter.instruction("test rdi, rdi");                                       // null tables from missed reads have nothing to reorder
    emitter.instruction("jz __rt_hsort_ret");                                   // return before dereferencing a null table header
    abi::emit_load_int_immediate(emitter, "r10", NULL_SENTINEL);
    emitter.instruction("cmp rdi, r10");                                        // does the table carry the in-band null-container sentinel?
    emitter.instruction("je __rt_hsort_ret");                                   // sentinel-null tables have no header to relink
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // r10 = live entry count from the hash header
    emitter.instruction("cmp r10, 2");                                          // does the table hold at least two entries?
    emitter.instruction("jge __rt_hsort_begin");                                // only multi-entry tables can change order

    emitter.label("__rt_hsort_ret");
    emitter.instruction("ret");                                                 // return with the table left exactly as it was

    // -- establish the sort frame and seed an empty destination chain --
    emitter.label("__rt_hsort_begin");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the link-sort frame pointer
    emitter.instruction("sub rsp, 96");                                         // allocate the aligned link-sort frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash-table pointer for the final header update
    emitter.instruction("lea r10, [rdi + 40]");                                 // compute the entries region base past the 40-byte header
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the entries base used by every slot address computation
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the key/value and direction mode word
    emitter.instruction("mov QWORD PTR [rbp - 96], rdx");                       // save this sort's comparator for the whole run
    emitter.instruction("mov QWORD PTR [rbp - 32], -1");                        // sorted head = none
    emitter.instruction("mov QWORD PTR [rbp - 40], -1");                        // sorted tail = none
    emitter.instruction("mov r10, QWORD PTR [rdi + 24]");                       // r10 = current insertion-order head slot
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // start consuming the source chain from its head

    // -- outer loop: detach the next source entry and read its comparison triple --
    emitter.label("__rt_hsort_outer");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the slot being placed
    emitter.instruction("cmp r10, -1");                                         // has the source chain been fully consumed?
    emitter.instruction("je __rt_hsort_finish");                                // publish the sorted chain once no source entry remains
    emitter.instruction("mov r11, r10");                                        // copy the slot index before scaling it
    emitter.instruction("shl r11, 6");                                          // convert the slot index into a 64-byte entry offset
    emitter.instruction("add r11, QWORD PTR [rbp - 16]");                       // r11 = address of the entry being placed
    emitter.instruction("mov rax, QWORD PTR [r11 + 56]");                       // read the source successor before the entry is relinked
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // remember where the source walk resumes
    emitter.instruction("mov rdi, r11");                                        // pass the entry address to the operand reader
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the mode so the reader picks the key or the value
    emitter.instruction("call __rt_hash_sort_triple");                          // materialize the entry's PHP comparison triple
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // cache the placed entry's runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 80], rdi");                       // cache the placed entry's low payload word
    emitter.instruction("mov QWORD PTR [rbp - 88], rdx");                       // cache the placed entry's high payload word
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the sorted chain's tail
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // start the backward scan at that tail

    // -- inner loop: walk the sorted chain backwards to the stable insertion point --
    emitter.label("__rt_hsort_scan");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // reload the backward scan cursor
    emitter.instruction("cmp r10, -1");                                         // has the scan run off the front of the sorted chain?
    emitter.instruction("je __rt_hsort_insert");                                // the entry belongs at the head of the sorted chain
    emitter.instruction("mov r11, r10");                                        // copy the cursor slot index before scaling it
    emitter.instruction("shl r11, 6");                                          // convert the slot index into a 64-byte entry offset
    emitter.instruction("add r11, QWORD PTR [rbp - 16]");                       // r11 = address of the sorted entry under the cursor
    emitter.instruction("mov rdi, r11");                                        // pass the entry address to the operand reader
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the mode so the reader picks the key or the value
    emitter.instruction("call __rt_hash_sort_triple");                          // materialize the scanned entry's comparison triple
    emitter.instruction("mov rsi, rdi");                                        // move the scanned low payload word into the left-operand slot
    emitter.instruction("mov rdi, rax");                                        // move the scanned runtime tag into the left-operand slot
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // pass the placed entry's tag as the right operand
    emitter.instruction("mov r8, QWORD PTR [rbp - 80]");                        // pass the placed entry's low payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 88]");                        // pass the placed entry's high payload word
    emitter.instruction("call QWORD PTR [rbp - 96]");                           // apply the selected ordering to scanned versus placed
    emitter.instruction("test QWORD PTR [rbp - 24], 1");                        // reload the mode word to pick the direction test
    emitter.instruction("jnz __rt_hsort_scan_desc");                            // descending sorts invert the stop condition
    emitter.instruction("cmp rax, 0");                                          // does the scanned entry already sort at or before the placed one?
    emitter.instruction("jle __rt_hsort_insert");                               // stopping on equality keeps ties in their original order
    emitter.instruction("jmp __rt_hsort_scan_prev");                            // otherwise keep walking towards the chain head

    emitter.label("__rt_hsort_scan_desc");
    emitter.instruction("cmp rax, 0");                                          // does the scanned entry already sort at or before the placed one?
    emitter.instruction("jge __rt_hsort_insert");                               // stopping on equality keeps ties in their original order

    emitter.label("__rt_hsort_scan_prev");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // reload the backward scan cursor
    emitter.instruction("shl r10, 6");                                          // convert the slot index into a 64-byte entry offset
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // r10 = address of the scanned entry
    emitter.instruction("mov r11, QWORD PTR [r10 + 48]");                       // follow the sorted chain's predecessor link
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // advance the backward scan cursor
    emitter.instruction("jmp __rt_hsort_scan");                                 // keep scanning for the stable insertion point

    // -- splice the placed entry into the sorted chain --
    emitter.label("__rt_hsort_insert");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // r10 = predecessor slot, or -1 for a head insertion
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // r11 = the slot being placed
    emitter.instruction("mov rax, r11");                                        // copy the placed slot index before scaling it
    emitter.instruction("shl rax, 6");                                          // convert the slot index into a 64-byte entry offset
    emitter.instruction("add rax, QWORD PTR [rbp - 16]");                       // rax = address of the entry being placed
    emitter.instruction("cmp r10, -1");                                         // is there a predecessor to splice after?
    emitter.instruction("jne __rt_hsort_insert_after");                         // splice after the located predecessor

    emitter.instruction("mov QWORD PTR [rax + 48], -1");                        // placed entry prev = none
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the current sorted head
    emitter.instruction("mov QWORD PTR [rax + 56], rcx");                       // placed entry next = the old sorted head
    emitter.instruction("cmp rcx, -1");                                         // was the sorted chain still empty?
    emitter.instruction("je __rt_hsort_insert_first");                          // the first placed entry is also the sorted tail
    emitter.instruction("shl rcx, 6");                                          // convert the old head slot index into an entry offset
    emitter.instruction("add rcx, QWORD PTR [rbp - 16]");                       // rcx = address of the old sorted head
    emitter.instruction("mov QWORD PTR [rcx + 48], r11");                       // old sorted head prev = the placed entry
    emitter.instruction("jmp __rt_hsort_insert_head");                          // publish the new sorted head

    emitter.label("__rt_hsort_insert_first");
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // sorted tail = the first placed entry

    emitter.label("__rt_hsort_insert_head");
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // sorted head = the placed entry
    emitter.instruction("jmp __rt_hsort_advance");                              // continue with the next source entry

    emitter.label("__rt_hsort_insert_after");
    emitter.instruction("mov rcx, r10");                                        // copy the predecessor slot index before scaling it
    emitter.instruction("shl rcx, 6");                                          // convert the slot index into a 64-byte entry offset
    emitter.instruction("add rcx, QWORD PTR [rbp - 16]");                       // rcx = address of the predecessor entry
    emitter.instruction("mov rdx, QWORD PTR [rcx + 56]");                       // rdx = the predecessor's current successor
    emitter.instruction("mov QWORD PTR [rax + 48], r10");                       // placed entry prev = the predecessor
    emitter.instruction("mov QWORD PTR [rax + 56], rdx");                       // placed entry next = the predecessor's old successor
    emitter.instruction("mov QWORD PTR [rcx + 56], r11");                       // predecessor next = the placed entry
    emitter.instruction("cmp rdx, -1");                                         // was the predecessor the sorted tail?
    emitter.instruction("je __rt_hsort_insert_tail");                           // then the placed entry becomes the new tail
    emitter.instruction("shl rdx, 6");                                          // convert the displaced successor index into an entry offset
    emitter.instruction("add rdx, QWORD PTR [rbp - 16]");                       // rdx = address of the displaced successor
    emitter.instruction("mov QWORD PTR [rdx + 48], r11");                       // displaced successor prev = the placed entry
    emitter.instruction("jmp __rt_hsort_advance");                              // continue with the next source entry

    emitter.label("__rt_hsort_insert_tail");
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // sorted tail = the placed entry

    emitter.label("__rt_hsort_advance");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the remembered source successor
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // resume the source walk from that entry
    emitter.instruction("jmp __rt_hsort_outer");                                // place the next source entry

    // -- publish the sorted chain through the hash header --
    emitter.label("__rt_hsort_finish");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the sorted chain head
    emitter.instruction("mov QWORD PTR [rdi + 24], r10");                       // header[24]: publish the new iteration-order head
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the sorted chain tail
    emitter.instruction("mov QWORD PTR [rdi + 32], r10");                       // header[32]: publish the new iteration-order tail
    emitter.instruction("mov rsp, rbp");                                        // release the link-sort frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with the table reordered in place
}

/// Emits the x86_64 System V `__rt_hash_sort_triple` operand reader.
///
/// Input `rdi` = hash entry address, `rsi` = mode word; output `rax` = runtime tag,
/// `rdi` = low payload word, `rdx` = high payload word — the same register triple
/// `__rt_mixed_unbox` returns, so boxed values are peeled with a tail jump. String
/// payloads stay borrowed from the table.
fn emit_hash_sort_triple_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_sort_triple ---");
    emitter.label_global("__rt_hash_sort_triple");

    emitter.instruction("test rsi, 2");                                         // mode bit 1 selects the entry value instead of its key
    emitter.instruction("jnz __rt_hsort_triple_value");                         // value sorts read the payload instead of the key
    emitter.instruction("mov rdx, QWORD PTR [rdi + 16]");                       // rdx = stored key length, or -1 for a normalized integer key
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // r10 = stored key pointer, or the integer key payload
    emitter.instruction("cmp rdx, -1");                                         // is this a normalized integer key?
    emitter.instruction("jne __rt_hsort_triple_key_str");                       // string keys keep their pointer and length
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    emitter.instruction("mov rdi, r10");                                        // publish the integer key payload as the low word
    emitter.instruction("xor edx, edx");                                        // integer operands carry no high payload word
    emitter.instruction("ret");                                                 // return the integer key triple

    emitter.label("__rt_hsort_triple_key_str");
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("mov rdi, r10");                                        // publish the borrowed key pointer as the low word
    emitter.instruction("ret");                                                 // return the borrowed string key triple

    emitter.label("__rt_hsort_triple_value");
    emitter.instruction("mov r11, QWORD PTR [rdi + 40]");                       // r11 = the entry's per-entry runtime value tag
    emitter.instruction("mov r10, QWORD PTR [rdi + 24]");                       // r10 = the entry's low payload word
    emitter.instruction("mov rdx, QWORD PTR [rdi + 32]");                       // rdx = the entry's high payload word
    emitter.instruction("cmp r11, 7");                                          // does the entry hold a boxed Mixed cell?
    emitter.instruction("je __rt_hsort_triple_value_boxed");                    // boxed cells must be peeled before comparing
    emitter.instruction("mov rax, r11");                                        // unboxed entries already carry a concrete tag
    emitter.instruction("mov rdi, r10");                                        // publish the borrowed payload as the low word
    emitter.instruction("ret");                                                 // return the borrowed value triple

    emitter.label("__rt_hsort_triple_value_boxed");
    emitter.instruction("mov rax, r10");                                        // pass the borrowed Mixed cell to the unboxing helper
    emitter.instruction("jmp __rt_mixed_unbox");                                // tail-jump so the peeled triple returns to our caller
}
