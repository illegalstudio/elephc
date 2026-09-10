//! Purpose:
//! Emits the `__rt_array_ptr_seek`, `__rt_array_ptr_key` and `__rt_array_ptr_value`
//! runtime helpers backing PHP's internal array pointer family
//! (`key`, `current`, `next`, `prev`, `reset`, `end`).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The cursor is a LOGICAL ORDINAL into the container's iteration order, never a
//!   physical bucket index, so every read is bounds-checked against the live element
//!   count and a stale cursor can only produce `false`/`null`, never an out-of-bounds
//!   read. `-1` is the single canonical "invalid" cursor, matching PHP's one-way
//!   past-the-end state (`prev()` off the front and `next()` off the back both land in
//!   the same unrecoverable position; only `reset`/`end` restore a valid cursor).
//! - All three helpers are LEAF routines: each starts with an inline normalization loop
//!   that unwraps boxed Mixed cells, and every exit is a tail jump. Nothing here builds a
//!   stack frame, so nothing can clobber the caller's return address.
//! - After normalization the live element count is at header word 0 for BOTH indexed
//!   arrays (kind 2) and hashes (kind 3), so the bounds check is one load either way.
//! - Value boxing is delegated to already-audited ownership paths: indexed storage tail
//!   calls `__rt_array_get_mixed_key` (the ordinary `$a[$i]` read path, which understands
//!   every indexed `value_type`), hash storage tail calls `__rt_mixed_from_value` (which
//!   retains containers and persists strings).
//! - Hash ordinals are resolved by walking the insertion-order `next` chain, so hash
//!   reads are `O(cursor)`; indexed reads are `O(1)`.
//! - The seek modes consumed by `__rt_array_ptr_seek` are `0` reset, `1` end, `2` next and
//!   `3` prev. `crate::builtins::semantics::ArrayPointerOp::seek_mode` is the single source
//!   of truth for that mapping and must stay in step with the dispatch below.

use crate::codegen_support::runtime::arrays::hash_layout;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the AArch64 inline normalization prologue shared by all three helpers.
///
/// Unwraps boxed Mixed cells until a bare container remains, then leaves the container
/// pointer in `x0`, its live element count in `x11`, and its heap kind in `x12`, or
/// branches to `invalid` when the input is not a live array or hash. Only `x9`-`x12` are
/// touched, so the caller's `x1`/`x2` argument registers survive.
fn emit_normalize_aarch64(emitter: &mut Emitter, prefix: &str, invalid: &str) {
    emitter.label(&format!("{}_norm", prefix));
    emitter.instruction(&format!("cbz x0, {}", invalid));                       // a null container is never positionable
    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        "x9",
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    emitter.instruction("cmp x0, x9");                                          // does the container carry the in-band null sentinel?
    emitter.instruction(&format!("b.eq {}", invalid));                          // sentinel-null containers are never positionable
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the uniform heap-kind header word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low-byte heap kind
    emitter.instruction("cmp x9, #5");                                          // is the container a boxed mixed cell?
    emitter.instruction(&format!("b.eq {}_unbox", prefix));                     // mixed cells are unwrapped before anything else
    emitter.instruction("cmp x9, #2");                                          // is the container an indexed array?
    emitter.instruction(&format!("b.eq {}_live", prefix));                      // indexed arrays keep their count at header word 0
    emitter.instruction("cmp x9, #3");                                          // is the container an associative hash?
    emitter.instruction(&format!("b.eq {}_live", prefix));                      // hashes also keep their count at header word 0
    emitter.instruction(&format!("b {}", invalid));                             // any other kind is not iterable here
    emitter.label(&format!("{}_unbox", prefix));
    emitter.instruction("ldr x10, [x0]");                                       // load the boxed mixed value tag
    emitter.instruction("cmp x10, #4");                                         // does the cell box an indexed array?
    emitter.instruction(&format!("b.eq {}_unwrap", prefix));                    // unwrap indexed array payloads
    emitter.instruction("cmp x10, #5");                                         // does the cell box an associative array?
    emitter.instruction(&format!("b.ne {}", invalid));                          // non-array mixed payloads are never positionable
    emitter.label(&format!("{}_unwrap", prefix));
    emitter.instruction("ldr x0, [x0, #8]");                                    // unbox the container pointer from mixed[8]
    emitter.instruction(&format!("b {}_norm", prefix));                         // re-normalize in case the payload nests another cell
    emitter.label(&format!("{}_live", prefix));
    emitter.instruction("mov x12, x9");                                         // x12 = heap kind (2 = indexed, 3 = hash)
    emitter.instruction("ldr x11, [x0, #0]");                                   // x11 = live element count from header word 0
}

/// Emits the x86_64 inline normalization prologue shared by all three helpers.
///
/// Unwraps boxed Mixed cells until a bare container remains, then leaves the container
/// pointer in `rdi` and its live element count in `r11`, or branches to `invalid`. Only
/// `rax`, `r10` and `r11` are touched, so the caller's `rsi`/`rdx` argument registers
/// survive; callers that need the heap kind reload the header byte after the check.
fn emit_normalize_x86_64(emitter: &mut Emitter, prefix: &str, invalid: &str) {
    emitter.label(&format!("{}_norm", prefix));
    emitter.instruction("test rdi, rdi");                                       // is the container pointer null?
    emitter.instruction(&format!("je {}", invalid));                            // a null container is never positionable
    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        "r10",
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    emitter.instruction("cmp rdi, r10");                                        // does the container carry the in-band null sentinel?
    emitter.instruction(&format!("je {}", invalid));                            // sentinel-null containers are never positionable
    emitter.instruction("movzx eax, BYTE PTR [rdi - 8]");                       // load the low-byte heap kind from the uniform header
    emitter.instruction("cmp eax, 5");                                          // is the container a boxed mixed cell?
    emitter.instruction(&format!("je {}_unbox", prefix));                       // mixed cells are unwrapped before anything else
    emitter.instruction("cmp eax, 2");                                          // is the container an indexed array?
    emitter.instruction(&format!("je {}_live", prefix));                        // indexed arrays keep their count at header word 0
    emitter.instruction("cmp eax, 3");                                          // is the container an associative hash?
    emitter.instruction(&format!("je {}_live", prefix));                        // hashes also keep their count at header word 0
    emitter.instruction(&format!("jmp {}", invalid));                           // any other kind is not iterable here
    emitter.label(&format!("{}_unbox", prefix));
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed mixed value tag
    emitter.instruction("cmp r10, 4");                                          // does the cell box an indexed array?
    emitter.instruction(&format!("je {}_unwrap", prefix));                      // unwrap indexed array payloads
    emitter.instruction("cmp r10, 5");                                          // does the cell box an associative array?
    emitter.instruction(&format!("jne {}", invalid));                           // non-array mixed payloads are never positionable
    emitter.label(&format!("{}_unwrap", prefix));
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // unbox the container pointer from mixed[8]
    emitter.instruction(&format!("jmp {}_norm", prefix));                       // re-normalize in case the payload nests another cell
    emitter.label(&format!("{}_live", prefix));
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // r11 = live element count from header word 0
}

/// Emits the AArch64 inline hash ordinal walk.
///
/// Follows the insertion-order `next` chain from the header head slot `x1` times and
/// leaves the selected entry's address in `x10`. The cursor has already been bounds
/// checked against the live count, so a chain that runs out early means the table is
/// inconsistent; that branches to `invalid` instead of reading past the entries.
fn emit_hash_walk_aarch64(emitter: &mut Emitter, prefix: &str, invalid: &str) {
    emitter.instruction("ldr x9, [x0, #24]");                                   // x9 = insertion-order head slot index
    emitter.label(&format!("{}_walk", prefix));
    emitter.instruction("cmn x9, #1");                                          // has the insertion-order chain run out?
    emitter.instruction(&format!("b.eq {}", invalid));                          // an exhausted chain has no entry at this ordinal
    emitter.instruction("mov x10, #64");                                        // x10 = hash entry stride in bytes
    hash_layout::emit_entry_address(emitter, "x10", "x0", "x9");
    emitter.instruction(&format!("cbz x1, {}_walk_done", prefix));              // ordinal 0 selects the current entry
    emitter.instruction("sub x1, x1, #1");                                      // consume one step of the requested ordinal
    emitter.instruction("ldr x9, [x10, #56]");                                  // x9 = next slot index from the insertion-order chain
    emitter.instruction(&format!("b {}_walk", prefix));                         // keep walking towards the requested ordinal
    emitter.label(&format!("{}_walk_done", prefix));
}

/// Emits the x86_64 inline hash ordinal walk.
///
/// Follows the insertion-order `next` chain from the header head slot `rsi` times and
/// leaves the selected entry's address in `r10`. Branches to `invalid` if the chain runs
/// out before the bounds-checked ordinal is reached.
fn emit_hash_walk_x86_64(emitter: &mut Emitter, prefix: &str, invalid: &str) {
    emitter.instruction("mov rax, QWORD PTR [rdi + 24]");                       // rax = insertion-order head slot index
    emitter.label(&format!("{}_walk", prefix));
    emitter.instruction("cmp rax, -1");                                         // has the insertion-order chain run out?
    emitter.instruction(&format!("je {}", invalid));                            // an exhausted chain has no entry at this ordinal
    hash_layout::emit_entry_address(emitter, "r10", "rdi", "rax");
    emitter.instruction("test rsi, rsi");                                       // is the requested ordinal exhausted?
    emitter.instruction(&format!("je {}_walk_done", prefix));                   // ordinal 0 selects the current entry
    emitter.instruction("sub rsi, 1");                                          // consume one step of the requested ordinal
    emitter.instruction("mov rax, QWORD PTR [r10 + 56]");                       // rax = next slot index from the insertion-order chain
    emitter.instruction(&format!("jmp {}_walk", prefix));                       // keep walking towards the requested ordinal
    emitter.label(&format!("{}_walk_done", prefix));
}

/// array_ptr_seek: compute the next internal-pointer cursor for one PHP seek operation.
///
/// The cursor is a logical ordinal; `-1` is the single canonical invalid position. PHP
/// only ever leaves the invalid position through `reset`/`end`, so `next`/`prev` short out
/// to `-1` whenever the incoming cursor is already out of range — which is what makes
/// `end($a); next($a); prev($a)` report `false` three times instead of walking back in.
///
/// Input:  x0 = container pointer, x1 = current cursor, x2 = seek mode
///         (0 = reset, 1 = end, 2 = next, 3 = prev)
/// Output: x0 = new cursor, or `-1` when the container has no element at that position
pub fn emit_array_ptr_seek(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_ptr_seek_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_ptr_seek ---");
    emitter.label_global("__rt_array_ptr_seek");
    emit_normalize_aarch64(emitter, "__rt_aptr_seek", "__rt_aptr_seek_invalid");
    emitter.instruction("cbz x11, __rt_aptr_seek_invalid");                     // an empty container has no valid position at all
    emitter.instruction("cmp x2, #0");                                          // is this a reset (mode 0)?
    emitter.instruction("b.eq __rt_aptr_seek_first");                           // reset rewinds to the first ordinal
    emitter.instruction("cmp x2, #1");                                          // is this an end (mode 1)?
    emitter.instruction("b.eq __rt_aptr_seek_last");                            // end jumps to the final ordinal
    emitter.instruction("cmp x1, #0");                                          // is the incoming cursor before the first ordinal?
    emitter.instruction("b.lt __rt_aptr_seek_invalid");                         // an already-invalid cursor stays invalid
    emitter.instruction("cmp x1, x11");                                         // is the incoming cursor past the last ordinal?
    emitter.instruction("b.ge __rt_aptr_seek_invalid");                         // a stale past-the-end cursor stays invalid
    emitter.instruction("cmp x2, #2");                                          // is this a next (mode 2)?
    emitter.instruction("b.eq __rt_aptr_seek_forward");                         // forward steps add one ordinal
    emitter.instruction("cbz x1, __rt_aptr_seek_invalid");                      // stepping back off the front lands on the invalid cursor
    emitter.instruction("sub x0, x1, #1");                                      // x0 = previous ordinal
    emitter.instruction("ret");                                                 // return the rewound cursor
    emitter.label("__rt_aptr_seek_forward");
    emitter.instruction("add x0, x1, #1");                                      // x0 = next ordinal
    emitter.instruction("cmp x0, x11");                                         // has the cursor stepped past the last ordinal?
    emitter.instruction("b.ge __rt_aptr_seek_invalid");                         // stepping past the end lands on the invalid cursor
    emitter.instruction("ret");                                                 // return the advanced cursor
    emitter.label("__rt_aptr_seek_first");
    emitter.instruction("mov x0, #0");                                          // reset selects the first ordinal
    emitter.instruction("ret");                                                 // return the rewound cursor
    emitter.label("__rt_aptr_seek_last");
    emitter.instruction("sub x0, x11, #1");                                     // end selects the final ordinal
    emitter.instruction("ret");                                                 // return the advanced cursor
    emitter.label("__rt_aptr_seek_invalid");
    emitter.instruction("mov x0, #-1");                                         // -1 is the canonical invalid cursor
    emitter.instruction("ret");                                                 // return the invalid cursor
}

/// x86_64 Linux implementation of `__rt_array_ptr_seek`.
/// Input:  rdi = container pointer, rsi = current cursor, rdx = seek mode
/// Output: rax = new cursor, or `-1` for the invalid position
fn emit_array_ptr_seek_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_ptr_seek ---");
    emitter.label_global("__rt_array_ptr_seek");
    emit_normalize_x86_64(emitter, "__rt_aptr_seek", "__rt_aptr_seek_invalid");
    emitter.instruction("test r11, r11");                                       // is the container empty?
    emitter.instruction("je __rt_aptr_seek_invalid");                           // an empty container has no valid position at all
    emitter.instruction("cmp rdx, 0");                                          // is this a reset (mode 0)?
    emitter.instruction("je __rt_aptr_seek_first");                             // reset rewinds to the first ordinal
    emitter.instruction("cmp rdx, 1");                                          // is this an end (mode 1)?
    emitter.instruction("je __rt_aptr_seek_last");                              // end jumps to the final ordinal
    emitter.instruction("cmp rsi, 0");                                          // is the incoming cursor before the first ordinal?
    emitter.instruction("jl __rt_aptr_seek_invalid");                           // an already-invalid cursor stays invalid
    emitter.instruction("cmp rsi, r11");                                        // is the incoming cursor past the last ordinal?
    emitter.instruction("jge __rt_aptr_seek_invalid");                          // a stale past-the-end cursor stays invalid
    emitter.instruction("cmp rdx, 2");                                          // is this a next (mode 2)?
    emitter.instruction("je __rt_aptr_seek_forward");                           // forward steps add one ordinal
    emitter.instruction("test rsi, rsi");                                       // is the cursor already on the first ordinal?
    emitter.instruction("je __rt_aptr_seek_invalid");                           // stepping back off the front lands on the invalid cursor
    emitter.instruction("lea rax, [rsi - 1]");                                  // rax = previous ordinal
    emitter.instruction("ret");                                                 // return the rewound cursor
    emitter.label("__rt_aptr_seek_forward");
    emitter.instruction("lea rax, [rsi + 1]");                                  // rax = next ordinal
    emitter.instruction("cmp rax, r11");                                        // has the cursor stepped past the last ordinal?
    emitter.instruction("jge __rt_aptr_seek_invalid");                          // stepping past the end lands on the invalid cursor
    emitter.instruction("ret");                                                 // return the advanced cursor
    emitter.label("__rt_aptr_seek_first");
    emitter.instruction("xor eax, eax");                                        // reset selects the first ordinal
    emitter.instruction("ret");                                                 // return the rewound cursor
    emitter.label("__rt_aptr_seek_last");
    emitter.instruction("lea rax, [r11 - 1]");                                  // end selects the final ordinal
    emitter.instruction("ret");                                                 // return the advanced cursor
    emitter.label("__rt_aptr_seek_invalid");
    emitter.instruction("mov rax, -1");                                         // -1 is the canonical invalid cursor
    emitter.instruction("ret");                                                 // return the invalid cursor
}

/// array_ptr_key: box the key at a logical cursor as a Mixed cell (`key()`).
///
/// Out-of-range cursors box canonical null, which is exactly what PHP's `key()` returns
/// once the internal pointer has run off either end. Indexed keys are the ordinal itself
/// because elephc's indexed storage is dense; hash keys come from the ordinal's entry.
///
/// Input:  x0 = container pointer, x1 = cursor ordinal
/// Output: x0 = boxed Mixed key, or boxed null when the cursor is invalid
pub fn emit_array_ptr_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_ptr_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_ptr_key ---");
    emitter.label_global("__rt_array_ptr_key");
    emit_normalize_aarch64(emitter, "__rt_aptr_key", "__rt_aptr_key_null");
    emitter.instruction("cmp x1, #0");                                          // is the cursor before the first ordinal?
    emitter.instruction("b.lt __rt_aptr_key_null");                             // invalid cursors have no key
    emitter.instruction("cmp x1, x11");                                         // is the cursor past the last ordinal?
    emitter.instruction("b.ge __rt_aptr_key_null");                             // invalid cursors have no key
    emitter.instruction("cmp x12, #3");                                         // is the container an associative hash?
    emitter.instruction("b.eq __rt_aptr_key_hash");                             // hashes read the key out of the ordinal's entry
    emitter.instruction("mov x0, #0");                                          // dense indexed keys are the ordinal: tag 0 (integer)
    emitter.instruction("mov x2, #0");                                          // value_hi unused for integers
    emitter.instruction("b __rt_mixed_from_value");                             // box the integer key and return it to the caller
    emitter.label("__rt_aptr_key_hash");
    emit_hash_walk_aarch64(emitter, "__rt_aptr_key", "__rt_aptr_key_null");
    emitter.instruction("ldr x9, [x10, #16]");                                  // x9 = key_len (-1 marks an integer key)
    emitter.instruction("ldr x13, [x10, #8]");                                  // x13 = key payload (integer value or string pointer)
    emitter.instruction("cmn x9, #1");                                          // is the entry keyed by an integer?
    emitter.instruction("b.eq __rt_aptr_key_int");                              // integer keys box with tag 0
    emitter.instruction("mov x0, #1");                                          // value_tag = 1 (string)
    emitter.instruction("mov x1, x13");                                         // value_lo = key string pointer
    emitter.instruction("mov x2, x9");                                          // value_hi = key string length
    emitter.instruction("b __rt_mixed_from_value");                             // box (and persist) the string key and return it
    emitter.label("__rt_aptr_key_int");
    emitter.instruction("mov x0, #0");                                          // value_tag = 0 (integer)
    emitter.instruction("mov x1, x13");                                         // value_lo = integer key
    emitter.instruction("mov x2, #0");                                          // value_hi unused for integers
    emitter.instruction("b __rt_mixed_from_value");                             // box the integer key and return it to the caller
    emitter.label("__rt_aptr_key_null");
    emitter.instruction("mov x0, #8");                                          // value_tag = 8 (null)
    emitter.instruction("mov x1, #0");                                          // canonical null has no low payload word
    emitter.instruction("mov x2, #0");                                          // value_hi unused
    emitter.instruction("b __rt_mixed_from_value");                             // box canonical null and return it to the caller
}

/// x86_64 Linux implementation of `__rt_array_ptr_key`.
/// Input:  rdi = container pointer, rsi = cursor ordinal
/// Output: rax = boxed Mixed key, or boxed null when the cursor is invalid
fn emit_array_ptr_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_ptr_key ---");
    emitter.label_global("__rt_array_ptr_key");
    emit_normalize_x86_64(emitter, "__rt_aptr_key", "__rt_aptr_key_null");
    emitter.instruction("cmp rsi, 0");                                          // is the cursor before the first ordinal?
    emitter.instruction("jl __rt_aptr_key_null");                               // invalid cursors have no key
    emitter.instruction("cmp rsi, r11");                                        // is the cursor past the last ordinal?
    emitter.instruction("jge __rt_aptr_key_null");                              // invalid cursors have no key
    emitter.instruction("movzx eax, BYTE PTR [rdi - 8]");                       // reload the low-byte heap kind after normalization
    emitter.instruction("cmp eax, 3");                                          // is the container an associative hash?
    emitter.instruction("je __rt_aptr_key_hash");                               // hashes read the key out of the ordinal's entry
    emitter.instruction("mov rdi, rsi");                                        // dense indexed keys are the ordinal itself
    emitter.instruction("xor esi, esi");                                        // value_hi unused for integers
    emitter.instruction("mov rax, 0");                                          // value_tag = 0 (integer)
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the integer key and return it to the caller
    emitter.label("__rt_aptr_key_hash");
    emit_hash_walk_x86_64(emitter, "__rt_aptr_key", "__rt_aptr_key_null");
    emitter.instruction("mov r8, QWORD PTR [r10 + 16]");                        // r8 = key_len (-1 marks an integer key)
    emitter.instruction("mov r9, QWORD PTR [r10 + 8]");                         // r9 = key payload (integer value or string pointer)
    emitter.instruction("cmp r8, -1");                                          // is the entry keyed by an integer?
    emitter.instruction("je __rt_aptr_key_int");                                // integer keys box with tag 0
    emitter.instruction("mov rdi, r9");                                         // value_lo = key string pointer
    emitter.instruction("mov rsi, r8");                                         // value_hi = key string length
    emitter.instruction("mov rax, 1");                                          // value_tag = 1 (string)
    emitter.instruction("jmp __rt_mixed_from_value");                           // box (and persist) the string key and return it
    emitter.label("__rt_aptr_key_int");
    emitter.instruction("mov rdi, r9");                                         // value_lo = integer key
    emitter.instruction("xor esi, esi");                                        // value_hi unused for integers
    emitter.instruction("mov rax, 0");                                          // value_tag = 0 (integer)
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the integer key and return it to the caller
    emitter.label("__rt_aptr_key_null");
    emitter.instruction("xor edi, edi");                                        // canonical null has no low payload word
    emitter.instruction("xor esi, esi");                                        // value_hi unused
    emitter.instruction("mov rax, 8");                                          // value_tag = 8 (null)
    emitter.instruction("jmp __rt_mixed_from_value");                           // box canonical null and return it to the caller
}

/// array_ptr_value: box the value at a logical cursor as a Mixed cell.
///
/// This backs `current()` and the value half of `next`/`prev`/`reset`/`end`. Out-of-range
/// cursors box `false`, matching PHP's return value once the pointer is off the end — and
/// PHP has the same `false`-vs-`false` ambiguity for an element that really holds `false`.
///
/// Input:  x0 = container pointer, x1 = cursor ordinal
/// Output: x0 = boxed Mixed value, or boxed `false` when the cursor is invalid
pub fn emit_array_ptr_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_ptr_value_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_ptr_value ---");
    emitter.label_global("__rt_array_ptr_value");
    emit_normalize_aarch64(emitter, "__rt_aptr_val", "__rt_aptr_val_false");
    emitter.instruction("cmp x1, #0");                                          // is the cursor before the first ordinal?
    emitter.instruction("b.lt __rt_aptr_val_false");                            // invalid cursors have no value
    emitter.instruction("cmp x1, x11");                                         // is the cursor past the last ordinal?
    emitter.instruction("b.ge __rt_aptr_val_false");                            // invalid cursors have no value
    emitter.instruction("cmp x12, #3");                                         // is the container an associative hash?
    emitter.instruction("b.eq __rt_aptr_val_hash");                             // hashes box the entry payload directly
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer indexed key
    emitter.instruction("mov x3, #0");                                          // never warn: the cursor was already bounds-checked
    emitter.instruction("b __rt_array_get_mixed_key");                          // reuse the ordinary indexed read path and return its box
    emitter.label("__rt_aptr_val_hash");
    emit_hash_walk_aarch64(emitter, "__rt_aptr_val", "__rt_aptr_val_false");
    emitter.instruction("ldr x9, [x10, #24]");                                  // x9 = value_lo from the hash entry
    emitter.instruction("ldr x13, [x10, #32]");                                 // x13 = value_hi from the hash entry
    emitter.instruction("ldr x14, [x10, #40]");                                 // x14 = value_tag from the hash entry
    emitter.instruction("mov x0, x14");                                         // value_tag = the entry's runtime tag
    emitter.instruction("mov x1, x9");                                          // value_lo = the entry's low payload word
    emitter.instruction("mov x2, x13");                                         // value_hi = the entry's high payload word
    emitter.instruction("b __rt_mixed_from_value");                             // retain/persist the payload and return the box
    emitter.label("__rt_aptr_val_false");
    emitter.instruction("mov x0, #3");                                          // value_tag = 3 (bool)
    emitter.instruction("mov x1, #0");                                          // value_lo = 0 (false)
    emitter.instruction("mov x2, #0");                                          // value_hi unused
    emitter.instruction("b __rt_mixed_from_value");                             // box PHP false and return it to the caller
}

/// x86_64 Linux implementation of `__rt_array_ptr_value`.
/// Input:  rdi = container pointer, rsi = cursor ordinal
/// Output: rax = boxed Mixed value, or boxed `false` when the cursor is invalid
fn emit_array_ptr_value_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_ptr_value ---");
    emitter.label_global("__rt_array_ptr_value");
    emit_normalize_x86_64(emitter, "__rt_aptr_val", "__rt_aptr_val_false");
    emitter.instruction("cmp rsi, 0");                                          // is the cursor before the first ordinal?
    emitter.instruction("jl __rt_aptr_val_false");                              // invalid cursors have no value
    emitter.instruction("cmp rsi, r11");                                        // is the cursor past the last ordinal?
    emitter.instruction("jge __rt_aptr_val_false");                             // invalid cursors have no value
    emitter.instruction("movzx eax, BYTE PTR [rdi - 8]");                       // reload the low-byte heap kind after normalization
    emitter.instruction("cmp eax, 3");                                          // is the container an associative hash?
    emitter.instruction("je __rt_aptr_val_hash");                               // hashes box the entry payload directly
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer indexed key
    emitter.instruction("xor ecx, ecx");                                        // never warn: the cursor was already bounds-checked
    emitter.instruction("jmp __rt_array_get_mixed_key");                        // reuse the ordinary indexed read path and return its box
    emitter.label("__rt_aptr_val_hash");
    emit_hash_walk_x86_64(emitter, "__rt_aptr_val", "__rt_aptr_val_false");
    emitter.instruction("mov r8, QWORD PTR [r10 + 24]");                        // r8 = value_lo from the hash entry
    emitter.instruction("mov r9, QWORD PTR [r10 + 32]");                        // r9 = value_hi from the hash entry
    emitter.instruction("mov rax, QWORD PTR [r10 + 40]");                       // rax = value_tag from the hash entry
    emitter.instruction("mov rdi, r8");                                         // value_lo = the entry's low payload word
    emitter.instruction("mov rsi, r9");                                         // value_hi = the entry's high payload word
    emitter.instruction("jmp __rt_mixed_from_value");                           // retain/persist the payload and return the box
    emitter.label("__rt_aptr_val_false");
    emitter.instruction("xor edi, edi");                                        // value_lo = 0 (false)
    emitter.instruction("xor esi, esi");                                        // value_hi unused
    emitter.instruction("mov rax, 3");                                          // value_tag = 3 (bool)
    emitter.instruction("jmp __rt_mixed_from_value");                           // box PHP false and return it to the caller
}
