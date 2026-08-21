//! Purpose:
//! Emits the PHP RESOURCE-id registry: `__rt_resource_id_mint` and
//! `__rt_resource_id_of`. Together they give every PHP resource the small,
//! creation-ordered integer that `var_dump()` prints as `resource(N) of type (T)`,
//! that `get_resource_id()` returns, that `(int)$res` casts to, and that
//! `"$res"` renders as `Resource id #N`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - `__rt_mixed_from_value`'s tag-9 arm (bind-if-absent, so EVERY boxed resource
//!   gets an id in box order even from creation sites this module never sees) — with
//!   TWO deliberate exclusions, resource kinds 2 and 5. Both are the raw incremental-hash
//!   context that PHP 8 hands back from `hash_init()` as a `HashContext` OBJECT (see
//!   `crate::hash_prelude`), and PHP counts such a context in the OBJECT handle space,
//!   not this one. Binding an id to it would shift every later `fopen()` by one and make
//!   `resource(N)` disagree with PHP for programs that merely hash. Kind 2 is the HOST
//!   context, so `__rt_hash_init` / `__rt_hash_copy` correspondingly no longer mint at
//!   creation. Kind 5 is the same context reached from inside `eval()`, boxed by
//!   `__elephc_eval_value_hash_context`; without it, `eval('hash_init("md5"); ...')`
//!   burned a shared-counter id and every host resource created afterwards reported one
//!   too high, even though the eval interpreter's OTHER resources must keep consuming
//!   ids (see the shared-registry note below).
//! - The descriptor-boxing helpers in `crate::codegen::lower_inst::builtins::io`
//!   (mint-and-overwrite at the moment the underlying descriptor is created).
//! - Every display site: `emit_var_dump_resource`, the two
//!   `emit_resource_display_id_to_int` helpers, and `__rt_resource_to_string`.
//!
//! Key details:
//! - WHY THIS EXISTS AT ALL. Before it, a resource's displayed id was its native
//!   payload plus one. That works for a file descriptor (a small integer) and is
//!   catastrophic for anything else: an elephc-crypto `HashContext` handle is a
//!   malloc'd address, so `var_dump(hash_init('md5'))` printed
//!   `resource(4318862849) of type (stream)` — a raw heap address, leaked into
//!   program stdout, and different on every run because of ASLR. No program could
//!   produce stable output and no test could assert one.
//! - IT IS A DIFFERENT NUMBERING SPACE FROM OBJECT HANDLES. php-src keeps
//!   `zend_resource.handle` in the resource list and `zend_object.handle` in the
//!   object store; the two counters are unrelated and `resource(5)` may coexist
//!   with `object(C)#5`. This registry therefore has its own cursor and shares
//!   nothing with `runtime::objects::handles` beyond the side-table technique.
//! - IDS ARE MINTED FROM A COUNTER, NOT DERIVED FROM THE KEY. That is what makes
//!   output stable across runs: the id a resource receives depends only on how
//!   many resources the program created before it, never on an address. ASLR can
//!   move `_heap_buf` and the C allocator's arena freely without moving a printed
//!   id.
//! - A NEGATIVE PAYLOAD IS A CLOSED HANDLE THAT CARRIES ITS OWN ID. `fclose`,
//!   `pclose` and `closedir` stamp `-id` into the Mixed box's low payload word
//!   (`apply_resource_release_sentinel` in `crate::codegen::lower_inst::builtins::io`)
//!   so scope cleanup cannot close a descriptor twice. php-src does not disturb
//!   `zend_resource.handle` on close, so under 8.5.6 `fclose($r); echo "$r";` still
//!   prints `Resource id #5` and `get_resource_id($r)` still answers 5. Answering
//!   `-payload` here reproduces that and, just as importantly, does NOT mint: the
//!   bare `-1` sentinel this replaced missed the table on every display, minted a
//!   fresh id, and shifted the next `fopen()` by one. No real payload can be
//!   negative — descriptors are small positives, `DIR*`/`FILE*`/HashContext handles
//!   are userspace addresses, and `EVAL_RESOURCE_PAYLOAD_BASE` is `1 << 62`.
//! - IDS ARE NEVER REUSED, unlike object handles. php-src's `zend_resource`
//!   handles come from a monotonically increasing list index, so closing a stream
//!   and opening another gives the second one the NEXT id, not the first one's.
//!   `_resource_id_next` therefore only ever counts up; there is no free stack.
//! - THE TABLE IS AN OPEN-ADDRESSED HASH, not the direct-mapped granule table the
//!   object pool uses. Object handles are keyed by a heap-block address, which
//!   divides exactly into `_heap_buf` granules. Resource payloads have no such
//!   structure: a descriptor is 3, a `DIR*` and a HashContext handle come from the
//!   C allocator outside elephc's heap entirely. Hashing is the only mapping that
//!   covers all three.
//! - SLOT 0 OF THE VALUE ARRAY MEANS EMPTY. Ids start at 5, so a zero value word
//!   is unambiguous and the zero-initialized `.comm` needs no runtime setup pass.
//! - PAYLOADS 0, 1 AND 2 ARE NEVER STORED. They are the standard streams, whose
//!   ids PHP fixes at 1, 2 and 3 (`get_resource_id(STDIN)` is 1 under 8.5.6), and
//!   which elephc materializes as constants rather than through any creation site.
//!   Both entry points answer `payload + 1` for them directly. No live descriptor
//!   for a user resource can be 0..2 while the standard streams are open, so the
//!   shortcut cannot shadow a real entry.
//! - THIS REGISTRY IS SHARED WITH `eval()`, DELIBERATELY. PHP keeps one resource-id
//!   counter per request and `eval()`'d code draws from it like any other code:
//!   under 8.5.6, `$h = fopen(...); eval('$a = fopen(...); $b = fopen(...);');
//!   $i = fopen(...)` reports 5 / 6, 7 / 8. The eval interpreter's resources reach
//!   this table through `__elephc_eval_value_resource` -> `__rt_mixed_from_value`,
//!   so they interleave with the host program's in true creation order.
//!   What is NOT shared is the KEY space: an eval payload is an index into
//!   `elephc_magician::stream_resources::EvalStreamResources`, a dense counter that
//!   would otherwise collide with descriptor numbers. `EVAL_RESOURCE_PAYLOAD_BASE`
//!   (`1 << 62`) lifts every eval payload clear of both the standard-stream
//!   shortcut above and every descriptor and allocator handle a host resource can
//!   present. Before that base existed, eval's first three resources took the
//!   shortcut and reported 1, 2, 3 while consuming nothing from `_resource_id_next`,
//!   and eval key `n` aliased host descriptor `n`.
//! - MINT OVERWRITES, LOOKUP BINDS. `__rt_resource_id_mint` always takes a fresh
//!   id, which is what makes `$a = fopen(); fclose($a); $b = fopen();` give `$b`
//!   the next id even though the kernel handed back the same descriptor number.
//!   `__rt_resource_id_of` mints only on a miss, which is what makes `$b = $a`
//!   share one id and what guarantees that no display path can EVER fall through
//!   to printing a raw payload.
//! - CAPACITY IS FIXED AND FAILURE IS DEGRADED, NOT WRONG. With the table full,
//!   `mint` evicts the home slot; a later lookup of the evicted payload misses and
//!   mints a fresh id. The printed value is then higher than PHP's, but it is still
//!   small, still deterministic for a given program, and still never an address.
//!   In practice the table does not fill: an entry is keyed by payload, and
//!   descriptor numbers and freed allocator blocks are both heavily reused, so
//!   repeated open/close cycles rewrite one slot instead of consuming new ones.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Number of slots in `_resource_id_keys` / `_resource_id_vals`.
///
/// Must stay a power of two: both helpers reduce the hash with a single `and`
/// against `SLOTS - 1` rather than a division.
pub(crate) const RESOURCE_ID_TABLE_SLOTS: usize = 1024;

/// Highest native payload answered directly as `payload + 1` without consulting
/// the table: the three standard stream descriptors.
const STD_STREAM_MAX_PAYLOAD: u64 = 2;

/// Emits both resource-id entry points for the active target.
pub(crate) fn emit_resource_ids(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_resource_id_mint_aarch64(emitter);
            emit_resource_id_of_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_resource_id_mint_x86_64(emitter);
            emit_resource_id_of_x86_64(emitter);
        }
    }
}

/// Emits the AArch64 `__rt_resource_id_mint`.
///
/// Input: `x0` = native resource payload of a resource being CREATED.
/// Output: none — every register including `x0` is preserved, so the call can be
/// dropped straight in front of a boxing sequence without disturbing it. Contains
/// no nested `bl`, so it returns with `ret` and never clobbers `x30`.
fn emit_resource_id_mint_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_id_mint (bind a fresh id to a new resource) ---");
    emitter.label_global("__rt_resource_id_mint");

    emitter.instruction("stp x9, x10, [sp, #-48]!");                            // preserve the hash and probe scratch pair
    emitter.instruction("stp x11, x12, [sp, #16]");                             // preserve the table-address scratch pair
    emitter.instruction("stp x13, x14, [sp, #32]");                             // preserve the cursor and slot-index scratch pair

    emitter.instruction(&format!("cmp x0, #{}", STD_STREAM_MAX_PAYLOAD));       // is this one of the standard stream descriptors?
    emitter.instruction("b.ls __rt_resource_id_mint_done");                     // STDIN/STDOUT/STDERR keep their fixed 1/2/3 ids

    emit_resource_id_hash_aarch64(emitter, "x9", "x0");
    emitter.instruction("mov x14, x9");                                         // x14 = current probe slot, starting at the home slot
    emitter.instruction("mov x10, #0");                                         // x10 = number of slots probed so far
    abi::emit_symbol_address(emitter, "x11", "_resource_id_keys");
    abi::emit_symbol_address(emitter, "x12", "_resource_id_vals");

    emitter.label("__rt_resource_id_mint_probe");
    emitter.instruction(&format!("cmp x10, #{}", RESOURCE_ID_TABLE_SLOTS));     // has every slot been examined?
    emitter.instruction("b.eq __rt_resource_id_mint_evict");                    // a full table reuses the home slot
    emitter.instruction("ldr x13, [x12, x14, lsl #3]");                         // load the id stored in the probed slot
    emitter.instruction("cbz x13, __rt_resource_id_mint_store");                // an empty slot takes the new binding
    emitter.instruction("ldr x13, [x11, x14, lsl #3]");                         // load the payload stored in the probed slot
    emitter.instruction("cmp x13, x0");                                         // does this slot already describe this payload?
    emitter.instruction("b.eq __rt_resource_id_mint_store");                    // rebind the same payload to its new incarnation
    emitter.instruction("add x14, x14, #1");                                    // advance to the next slot
    emitter.instruction(&format!("and x14, x14, #{}", RESOURCE_ID_TABLE_SLOTS - 1)); // wrap the probe around the table
    emitter.instruction("add x10, x10, #1");                                    // count the probed slot
    emitter.instruction("b __rt_resource_id_mint_probe");                       // keep probing

    emitter.label("__rt_resource_id_mint_evict");
    emitter.instruction("mov x14, x9");                                         // fall back to the home slot when the table is full

    emitter.label("__rt_resource_id_mint_store");
    abi::emit_symbol_address(emitter, "x13", "_resource_id_next");
    emitter.instruction("ldr x9, [x13]");                                       // x9 = the next never-used resource id
    emitter.instruction("str x0, [x11, x14, lsl #3]");                          // bind the slot to this native payload
    emitter.instruction("str x9, [x12, x14, lsl #3]");                          // store the freshly minted id for that payload
    emitter.instruction("add x9, x9, #1");                                      // advance the never-used id cursor
    emitter.instruction("str x9, [x13]");                                       // publish the advanced cursor

    emitter.label("__rt_resource_id_mint_done");
    emitter.instruction("ldp x13, x14, [sp, #32]");                             // restore the cursor and slot-index scratch pair
    emitter.instruction("ldp x11, x12, [sp, #16]");                             // restore the table-address scratch pair
    emitter.instruction("ldp x9, x10, [sp], #48");                              // restore the hash and probe scratch pair
    emitter.instruction("ret");                                                 // return with x0 and every scratch register untouched
}

/// Emits the AArch64 `__rt_resource_id_of`.
///
/// Input: `x0` = opaque registry handle or legacy native payload.
/// Output: `x0` = the PHP resource id. Registry-owned resources carry the id in
/// their authoritative slot; only legacy non-stream payloads fall back to the
/// compatibility map below.
fn emit_resource_id_of_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_id_of (display id for a resource payload) ---");
    emitter.label_global("__rt_resource_id_of");

    emitter.instruction("stp x0, x30, [sp, #-16]!");                            // preserve the candidate handle and return address across registry lookup
    emitter.instruction("bl __rt_resource_id_of_registry");                     // ask the authoritative dynamic registry first
    emitter.instruction("cbz x0, __rt_resource_id_of_legacy");                  // zero means this is a legacy non-registry payload
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller return address on the registry hit path
    emitter.instruction("add sp, sp, #16");                                     // release the saved candidate handle
    emitter.instruction("ret");                                                 // return the PHP id stored in the registry slot
    emitter.label("__rt_resource_id_of_legacy");
    emitter.instruction("ldp x0, x30, [sp], #16");                              // restore the legacy payload and caller return address

    emitter.instruction("stp x9, x10, [sp, #-48]!");                            // preserve the hash and probe scratch pair
    emitter.instruction("stp x11, x12, [sp, #16]");                             // preserve the table-address scratch pair
    emitter.instruction("stp x13, x14, [sp, #32]");                             // preserve the cursor and slot-index scratch pair

    emitter.instruction("tbnz x0, #63, __rt_resource_id_of_closed");            // a negative payload is a closed handle carrying its own id
    emitter.instruction(&format!("cmp x0, #{}", STD_STREAM_MAX_PAYLOAD));       // is this one of the standard stream descriptors?
    emitter.instruction("b.hi __rt_resource_id_of_lookup");                     // ordinary resources consult the table
    emitter.instruction("add x0, x0, #1");                                      // STDIN/STDOUT/STDERR render as PHP's fixed 1/2/3
    emitter.instruction("b __rt_resource_id_of_done");                          // done without touching the table

    emitter.label("__rt_resource_id_of_closed");
    emitter.instruction("neg x0, x0");                                          // recover the id an explicit close preserved in the sentinel
    emitter.instruction("b __rt_resource_id_of_done");                          // closed handles never consult or mint into the table

    emitter.label("__rt_resource_id_of_lookup");
    emit_resource_id_hash_aarch64(emitter, "x9", "x0");
    emitter.instruction("mov x14, x9");                                         // x14 = current probe slot, starting at the home slot
    emitter.instruction("mov x10, #0");                                         // x10 = number of slots probed so far
    abi::emit_symbol_address(emitter, "x11", "_resource_id_keys");
    abi::emit_symbol_address(emitter, "x12", "_resource_id_vals");

    emitter.label("__rt_resource_id_of_probe");
    emitter.instruction(&format!("cmp x10, #{}", RESOURCE_ID_TABLE_SLOTS));     // has every slot been examined?
    emitter.instruction("b.eq __rt_resource_id_of_bind");                       // a full table with no match binds over the home slot
    emitter.instruction("ldr x13, [x12, x14, lsl #3]");                         // load the id stored in the probed slot
    emitter.instruction("cbz x13, __rt_resource_id_of_bind");                   // an empty slot ends the probe: this payload is unbound
    emitter.instruction("ldr x13, [x11, x14, lsl #3]");                         // load the payload stored in the probed slot
    emitter.instruction("cmp x13, x0");                                         // does this slot describe the payload being displayed?
    emitter.instruction("b.eq __rt_resource_id_of_hit");                        // yes — report its bound id
    emitter.instruction("add x14, x14, #1");                                    // advance to the next slot
    emitter.instruction(&format!("and x14, x14, #{}", RESOURCE_ID_TABLE_SLOTS - 1)); // wrap the probe around the table
    emitter.instruction("add x10, x10, #1");                                    // count the probed slot
    emitter.instruction("b __rt_resource_id_of_probe");                         // keep probing

    emitter.label("__rt_resource_id_of_hit");
    emitter.instruction("ldr x0, [x12, x14, lsl #3]");                          // report the id bound to this payload
    emitter.instruction("b __rt_resource_id_of_done");                          // return the bound id

    emitter.label("__rt_resource_id_of_bind");
    emitter.instruction(&format!("cmp x10, #{}", RESOURCE_ID_TABLE_SLOTS));     // did the probe stop because the table is full?
    emitter.instruction("b.ne __rt_resource_id_of_bind_slot");                  // no — bind into the empty slot the probe found
    emitter.instruction("mov x14, x9");                                         // yes — reuse the home slot rather than fail
    emitter.label("__rt_resource_id_of_bind_slot");
    abi::emit_symbol_address(emitter, "x13", "_resource_id_next");
    emitter.instruction("ldr x9, [x13]");                                       // x9 = the next never-used resource id
    emitter.instruction("str x0, [x11, x14, lsl #3]");                          // bind the slot to this native payload
    emitter.instruction("str x9, [x12, x14, lsl #3]");                          // store the freshly minted id for that payload
    emitter.instruction("mov x0, x9");                                          // report the freshly minted id
    emitter.instruction("add x9, x9, #1");                                      // advance the never-used id cursor
    emitter.instruction("str x9, [x13]");                                       // publish the advanced cursor

    emitter.label("__rt_resource_id_of_done");
    emitter.instruction("ldp x13, x14, [sp, #32]");                             // restore the cursor and slot-index scratch pair
    emitter.instruction("ldp x11, x12, [sp, #16]");                             // restore the table-address scratch pair
    emitter.instruction("ldp x9, x10, [sp], #48");                              // restore the hash and probe scratch pair
    emitter.instruction("ret");                                                 // return the resource id in x0
}

/// Emits the AArch64 payload hash into `dest` from `src`, leaving `src` untouched.
///
/// Mixes the payload down rather than masking it directly because the two payload
/// families cluster in opposite directions: descriptors are tiny consecutive
/// integers that only occupy the low bits, while allocator handles are 16-byte
/// aligned so their low four bits are always zero. Folding bits 4.. and 13.. back
/// down spreads both across the table instead of piling either into one slot.
fn emit_resource_id_hash_aarch64(emitter: &mut Emitter, dest: &str, src: &str) {
    emitter.instruction(&format!("eor {}, {}, {}, lsr #4", dest, src, src));    // fold out the allocator alignment zeros
    emitter.instruction(&format!("eor {}, {}, {}, lsr #13", dest, dest, dest)); // spread the high bits back into the index
    emitter.instruction(&format!("and {}, {}, #{}", dest, dest, RESOURCE_ID_TABLE_SLOTS - 1)); // reduce to a table slot
}

/// Emits the x86_64 `__rt_resource_id_mint`.
///
/// Input: `rax` = native resource payload of a resource being CREATED. Every
/// register including `rax` is preserved. Scratch is caller-saved only.
fn emit_resource_id_mint_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_id_mint (bind a fresh id to a new resource) ---");
    emitter.label_global("__rt_resource_id_mint");

    emitter.instruction("push rcx");                                            // preserve the hash scratch
    emitter.instruction("push rdx");                                            // preserve the probe-counter scratch
    emitter.instruction("push rsi");                                            // preserve the slot-index scratch
    emitter.instruction("push r8");                                             // preserve the key-table address scratch
    emitter.instruction("push r9");                                             // preserve the value-table address scratch
    emitter.instruction("push r10");                                            // preserve the loaded-word scratch
    emitter.instruction("push r11");                                            // preserve the id-cursor scratch

    emitter.instruction(&format!("cmp rax, {}", STD_STREAM_MAX_PAYLOAD));       // is this one of the standard stream descriptors?
    emitter.instruction("jbe __rt_resource_id_mint_done_x86");                  // STDIN/STDOUT/STDERR keep their fixed 1/2/3 ids

    emit_resource_id_hash_x86_64(emitter, "rcx", "rax");
    emitter.instruction("mov rsi, rcx");                                        // rsi = current probe slot, starting at the home slot
    emitter.instruction("xor edx, edx");                                        // rdx = number of slots probed so far
    abi::emit_symbol_address(emitter, "r8", "_resource_id_keys");
    abi::emit_symbol_address(emitter, "r9", "_resource_id_vals");

    emitter.label("__rt_resource_id_mint_probe_x86");
    emitter.instruction(&format!("cmp rdx, {}", RESOURCE_ID_TABLE_SLOTS));      // has every slot been examined?
    emitter.instruction("je __rt_resource_id_mint_evict_x86");                  // a full table reuses the home slot
    emitter.instruction("mov r10, QWORD PTR [r9 + rsi * 8]");                   // load the id stored in the probed slot
    emitter.instruction("test r10, r10");                                       // is the probed slot empty?
    emitter.instruction("jz __rt_resource_id_mint_store_x86");                  // an empty slot takes the new binding
    emitter.instruction("mov r10, QWORD PTR [r8 + rsi * 8]");                   // load the payload stored in the probed slot
    emitter.instruction("cmp r10, rax");                                        // does this slot already describe this payload?
    emitter.instruction("je __rt_resource_id_mint_store_x86");                  // rebind the same payload to its new incarnation
    emitter.instruction("add rsi, 1");                                          // advance to the next slot
    emitter.instruction(&format!("and rsi, {}", RESOURCE_ID_TABLE_SLOTS - 1));  // wrap the probe around the table
    emitter.instruction("add rdx, 1");                                          // count the probed slot
    emitter.instruction("jmp __rt_resource_id_mint_probe_x86");                 // keep probing

    emitter.label("__rt_resource_id_mint_evict_x86");
    emitter.instruction("mov rsi, rcx");                                        // fall back to the home slot when the table is full

    emitter.label("__rt_resource_id_mint_store_x86");
    abi::emit_symbol_address(emitter, "r10", "_resource_id_next");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // r11 = the next never-used resource id
    emitter.instruction("mov QWORD PTR [r8 + rsi * 8], rax");                   // bind the slot to this native payload
    emitter.instruction("mov QWORD PTR [r9 + rsi * 8], r11");                   // store the freshly minted id for that payload
    emitter.instruction("add r11, 1");                                          // advance the never-used id cursor
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the advanced cursor

    emitter.label("__rt_resource_id_mint_done_x86");
    emitter.instruction("pop r11");                                             // restore the id-cursor scratch
    emitter.instruction("pop r10");                                             // restore the loaded-word scratch
    emitter.instruction("pop r9");                                              // restore the value-table address scratch
    emitter.instruction("pop r8");                                              // restore the key-table address scratch
    emitter.instruction("pop rsi");                                             // restore the slot-index scratch
    emitter.instruction("pop rdx");                                             // restore the probe-counter scratch
    emitter.instruction("pop rcx");                                             // restore the hash scratch
    emitter.instruction("ret");                                                 // return with rax and every scratch register untouched
}

/// Emits the x86_64 `__rt_resource_id_of`.
///
/// Input: `rax` = opaque registry handle or legacy native payload.
/// Output: `rax` = the PHP resource id, consulting the authoritative dynamic
/// registry before the legacy compatibility map.
fn emit_resource_id_of_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_id_of (display id for a resource payload) ---");
    emitter.label_global("__rt_resource_id_of");

    emitter.instruction("push rax");                                            // preserve the candidate handle and align the stack for the helper call
    emitter.instruction("mov rdi, rax");                                        // pass the candidate opaque handle in the SysV argument register
    emitter.instruction("call __rt_resource_id_of_registry");                   // ask the authoritative dynamic registry first
    emitter.instruction("test rax, rax");                                       // did the registry recognize this handle generation?
    emitter.instruction("jz __rt_resource_id_of_legacy_x86");                   // zero means this is a legacy non-registry payload
    emitter.instruction("add rsp, 8");                                          // discard the saved candidate after a registry hit
    emitter.instruction("ret");                                                 // return the PHP id stored in the registry slot
    emitter.label("__rt_resource_id_of_legacy_x86");
    emitter.instruction("pop rax");                                             // restore the legacy payload for compatibility lookup

    emitter.instruction("push rcx");                                            // preserve the hash scratch
    emitter.instruction("push rdx");                                            // preserve the probe-counter scratch
    emitter.instruction("push rsi");                                            // preserve the slot-index scratch
    emitter.instruction("push r8");                                             // preserve the key-table address scratch
    emitter.instruction("push r9");                                             // preserve the value-table address scratch
    emitter.instruction("push r10");                                            // preserve the loaded-word scratch
    emitter.instruction("push r11");                                            // preserve the id-cursor scratch

    emitter.instruction("test rax, rax");                                       // a negative payload is a closed handle carrying its own id
    emitter.instruction("js __rt_resource_id_of_closed_x86");                   // recover it directly instead of probing the table
    emitter.instruction(&format!("cmp rax, {}", STD_STREAM_MAX_PAYLOAD));       // is this one of the standard stream descriptors?
    emitter.instruction("ja __rt_resource_id_of_lookup_x86");                   // ordinary resources consult the table
    emitter.instruction("add rax, 1");                                          // STDIN/STDOUT/STDERR render as PHP's fixed 1/2/3
    emitter.instruction("jmp __rt_resource_id_of_done_x86");                    // done without touching the table

    emitter.label("__rt_resource_id_of_closed_x86");
    emitter.instruction("neg rax");                                             // recover the id an explicit close preserved in the sentinel
    emitter.instruction("jmp __rt_resource_id_of_done_x86");                    // closed handles never consult or mint into the table

    emitter.label("__rt_resource_id_of_lookup_x86");
    emit_resource_id_hash_x86_64(emitter, "rcx", "rax");
    emitter.instruction("mov rsi, rcx");                                        // rsi = current probe slot, starting at the home slot
    emitter.instruction("xor edx, edx");                                        // rdx = number of slots probed so far
    abi::emit_symbol_address(emitter, "r8", "_resource_id_keys");
    abi::emit_symbol_address(emitter, "r9", "_resource_id_vals");

    emitter.label("__rt_resource_id_of_probe_x86");
    emitter.instruction(&format!("cmp rdx, {}", RESOURCE_ID_TABLE_SLOTS));      // has every slot been examined?
    emitter.instruction("je __rt_resource_id_of_bind_x86");                     // a full table with no match binds over the home slot
    emitter.instruction("mov r10, QWORD PTR [r9 + rsi * 8]");                   // load the id stored in the probed slot
    emitter.instruction("test r10, r10");                                       // is the probed slot empty?
    emitter.instruction("jz __rt_resource_id_of_bind_x86");                     // an empty slot ends the probe: this payload is unbound
    emitter.instruction("mov r10, QWORD PTR [r8 + rsi * 8]");                   // load the payload stored in the probed slot
    emitter.instruction("cmp r10, rax");                                        // does this slot describe the payload being displayed?
    emitter.instruction("je __rt_resource_id_of_hit_x86");                      // yes — report its bound id
    emitter.instruction("add rsi, 1");                                          // advance to the next slot
    emitter.instruction(&format!("and rsi, {}", RESOURCE_ID_TABLE_SLOTS - 1));  // wrap the probe around the table
    emitter.instruction("add rdx, 1");                                          // count the probed slot
    emitter.instruction("jmp __rt_resource_id_of_probe_x86");                   // keep probing

    emitter.label("__rt_resource_id_of_hit_x86");
    emitter.instruction("mov rax, QWORD PTR [r9 + rsi * 8]");                   // report the id bound to this payload
    emitter.instruction("jmp __rt_resource_id_of_done_x86");                    // return the bound id

    emitter.label("__rt_resource_id_of_bind_x86");
    emitter.instruction(&format!("cmp rdx, {}", RESOURCE_ID_TABLE_SLOTS));      // did the probe stop because the table is full?
    emitter.instruction("jne __rt_resource_id_of_bind_slot_x86");               // no — bind into the empty slot the probe found
    emitter.instruction("mov rsi, rcx");                                        // yes — reuse the home slot rather than fail
    emitter.label("__rt_resource_id_of_bind_slot_x86");
    abi::emit_symbol_address(emitter, "r10", "_resource_id_next");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // r11 = the next never-used resource id
    emitter.instruction("mov QWORD PTR [r8 + rsi * 8], rax");                   // bind the slot to this native payload
    emitter.instruction("mov QWORD PTR [r9 + rsi * 8], r11");                   // store the freshly minted id for that payload
    emitter.instruction("mov rax, r11");                                        // report the freshly minted id
    emitter.instruction("add r11, 1");                                          // advance the never-used id cursor
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the advanced cursor

    emitter.label("__rt_resource_id_of_done_x86");
    emitter.instruction("pop r11");                                             // restore the id-cursor scratch
    emitter.instruction("pop r10");                                             // restore the loaded-word scratch
    emitter.instruction("pop r9");                                              // restore the value-table address scratch
    emitter.instruction("pop r8");                                              // restore the key-table address scratch
    emitter.instruction("pop rsi");                                             // restore the slot-index scratch
    emitter.instruction("pop rdx");                                             // restore the probe-counter scratch
    emitter.instruction("pop rcx");                                             // restore the hash scratch
    emitter.instruction("ret");                                                 // return the resource id in rax
}

/// Emits the x86_64 payload hash into `dest` from `src`, leaving `src` untouched.
/// Same mix as the AArch64 form; see `emit_resource_id_hash_aarch64`.
fn emit_resource_id_hash_x86_64(emitter: &mut Emitter, dest: &str, src: &str) {
    emitter.instruction(&format!("mov {}, {}", dest, src));                     // copy the payload before folding it
    emitter.instruction(&format!("shr {}, 4", dest));                           // fold out the allocator alignment zeros
    emitter.instruction(&format!("xor {}, {}", dest, src));                     // mix the folded bits back into the payload
    emitter.instruction(&format!("mov r10, {}", dest));                         // copy again for the second fold
    emitter.instruction("shr r10, 13");                                         // spread the high bits back down
    emitter.instruction(&format!("xor {}, r10", dest));                         // mix the second fold in
    emitter.instruction(&format!("and {}, {}", dest, RESOURCE_ID_TABLE_SLOTS - 1)); // reduce to a table slot
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// The slot count must stay a power of two: both helpers reduce the hash with a
    /// single `and` against `SLOTS - 1`, which is only a modulo for powers of two.
    #[test]
    fn table_slot_count_is_a_power_of_two() {
        assert!(RESOURCE_ID_TABLE_SLOTS.is_power_of_two());
        assert!(RESOURCE_ID_TABLE_SLOTS >= 64);
    }

    /// Verifies both architectures emit both entry points.
    #[test]
    fn emits_both_entry_points_on_both_targets() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_ids(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_resource_id_mint:"), "{target:?}");
            assert!(asm.contains("__rt_resource_id_of:"), "{target:?}");
        }
    }

    /// Pins the AArch64 closed-handle arm of `__rt_resource_id_of`: a payload with bit
    /// 63 set is a `-id` sentinel stamped by `fclose`/`pclose`/`closedir`, and must be
    /// negated back into the id it carries.
    ///
    /// php-src leaves `zend_resource.handle` alone on close, so under 8.5.6
    /// `fclose($r); echo "$r";` still prints `Resource id #5` and
    /// `get_resource_id($r)` still answers 5. The bare `-1` sentinel this replaced had
    /// no such arm: every display of a closed handle missed the table, minted, printed
    /// `Resource id #6` and shifted the next `fopen()` to 7.
    #[test]
    fn aarch64_reports_the_id_a_closed_handle_carries() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_resource_ids(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("tbnz x0, #63, __rt_resource_id_of_closed"), "{asm}");
        assert!(asm.contains("__rt_resource_id_of_closed:\n"), "{asm}");
        assert!(asm.contains("neg x0, x0"), "{asm}");
    }

    /// Pins the same closed-handle arm on x86_64, so the two targets cannot drift.
    #[test]
    fn x86_64_reports_the_id_a_closed_handle_carries() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_resource_ids(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("js __rt_resource_id_of_closed_x86"), "{asm}");
        assert!(asm.contains("__rt_resource_id_of_closed_x86:\n"), "{asm}");
        assert!(asm.contains("neg rax"), "{asm}");
    }

    /// The closed-handle arm must reach the epilogue WITHOUT probing or minting: it may
    /// not read `_resource_id_next`, or a program that merely displays a closed handle
    /// would still steal the id the next `fopen()` is owed.
    #[test]
    fn the_closed_handle_arm_never_mints_on_either_target() {
        for (target, label, end) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "__rt_resource_id_of_closed:\n",
                "__rt_resource_id_of_lookup:\n",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "__rt_resource_id_of_closed_x86:\n",
                "__rt_resource_id_of_lookup_x86:\n",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_ids(&mut emitter);
            let asm = emitter.output();
            let arm = asm
                .split(label)
                .nth(1)
                .unwrap_or_else(|| panic!("missing closed arm for {target:?}:\n{asm}"));
            let arm = arm.split(end).next().expect("closed arm precedes the lookup");
            assert!(
                !arm.contains("_resource_id_next"),
                "closed handles must not mint a fresh id ({target:?}):\n{arm}"
            );
            assert!(
                !arm.contains("_resource_id_keys") && !arm.contains("_resource_id_vals"),
                "closed handles must not probe the table ({target:?}):\n{arm}"
            );
        }
    }

    /// These stopped being leaf helpers when `__rt_resource_id_of` began asking the
    /// authoritative registry first, so the property that actually protects the call
    /// sites (including `__rt_mixed_from_value`, mid-allocation) is no longer "contains
    /// no `bl`" but "never loses `x30` across the one it does contain".
    ///
    /// The save is the `stp x0, x30` that also preserves the candidate handle, and BOTH
    /// paths out of the lookup must undo it: the registry hit reloads `x30` and drops the
    /// slot, the legacy miss pops the pair back.
    #[test]
    fn aarch64_helpers_preserve_the_link_register_across_the_registry_lookup() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_resource_ids(&mut emitter);
        let asm = emitter.output();
        assert_eq!(
            asm.matches("    bl ").count(),
            1,
            "only the registry lookup may be called:\n{asm}"
        );
        let (before, after) = asm
            .split_once("    bl __rt_resource_id_of_registry\n")
            .expect("the registry lookup must be emitted");
        assert!(
            before.contains("    stp x0, x30, [sp, #-16]!\n"),
            "the link register must be saved BEFORE the lookup:\n{asm}"
        );
        assert!(
            after.contains("    ldr x30, [sp, #8]\n"),
            "the registry-hit path must restore the link register:\n{asm}"
        );
        assert!(
            after.contains("    ldp x0, x30, [sp], #16\n"),
            "the legacy path must restore the link register:\n{asm}"
        );
    }
}
