//! Purpose:
//! Emits `__rt_array_pad_boxed` and `__rt_array_chunk_boxed`, `array_pad()` and `array_chunk()`
//! over any array layout and element type.
//!
//! Called from:
//! - The typed ArrayPad and ArrayChunk backends, for the shapes their typed helpers cannot read
//!   (string or boxed elements, a pad value of another type, associative or boxed operands).
//!
//! Key details:
//! - `array_pad()` follows php-src: when `|$length|` does not exceed the count the result is a
//!   copy under the source's own keys; otherwise integer keys are renumbered from zero in order,
//!   string keys stand, and the padding takes the next integer keys, after the elements for a
//!   positive length and before them for a negative one.
//! - `array_chunk()` builds a list of hashes; each chunk keeps the source keys when asked to and
//!   numbers from zero otherwise.
//! - The source is read through `__rt_array_iter_next`, so a packed array, a hash and a boxed
//!   `array` are all accepted. Every stored value is a copy the result owns: a string is
//!   duplicated with `__rt_str_persist`, a heap value retained. Neither builder renders or
//!   compares a value, so no user code runs inside them.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const PAD: &str = "__rt_array_pad_boxed";
const CHUNK: &str = "__rt_array_chunk_boxed";

// `__rt_array_pad_boxed` frame, as offsets below the frame pointer. Cells are three ascending
// words starting at their tag slot.
const PAD_FRAME: usize = 160;
const PAD_SRC: usize = 8;
const PAD_LENGTH: usize = 16;
const PAD_COUNT: usize = 24;
const PAD_RESULT: usize = 32;
const PAD_CURSOR: usize = 40;
const PAD_NEXT_INT: usize = 48;
const PAD_PADS: usize = 56;
const PAD_PRESERVE: usize = 64;
const PAD_VALUE_HIGH: usize = 72;
const PAD_VALUE_LOW: usize = 80;
const PAD_VALUE: usize = 88;
const PAD_ELEM_HIGH: usize = 96;
const PAD_ELEM_LOW: usize = 104;
const PAD_ELEM: usize = 112;
// A key pair keeps its high word one slot below its low word, at `key_lo - 8`.
const PAD_KEY_HI: usize = 120;
const PAD_KEY_LO: usize = 128;
const PAD_REMAINING: usize = 136;

// `__rt_array_chunk_boxed` frame.
const CHUNK_FRAME: usize = 144;
const CHUNK_SRC: usize = 8;
const CHUNK_SIZE: usize = 16;
const CHUNK_PRESERVE: usize = 24;
const CHUNK_RESULT: usize = 32;
const CHUNK_CURRENT: usize = 40;
const CHUNK_FILLED: usize = 48;
const CHUNK_INDEX: usize = 56;
const CHUNK_NEXT_INT: usize = 64;
const CHUNK_CURSOR: usize = 72;
// The element cell's low and high words sit at 88 and 80, below its tag.
const CHUNK_ELEM: usize = 96;
const CHUNK_KEY_HI: usize = 104;
const CHUNK_KEY_LO: usize = 112;

/// Borrows the array cell in ABI arg 0, the signed length in arg 1 and the pad value cell in
/// arg 2; returns an owned hash, or 0 when the first operand is not an array. The caller bounds
/// the length's magnitude first.
pub fn emit_array_pad_boxed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg = |emitter: &Emitter, index: usize| abi::int_arg_reg_name(emitter.target, index);
    let label = |suffix: &str| format!("{PAD}_{suffix}");
    let (low, high) = unbox_payload_regs(emitter);
    emitter.blank();
    emitter.label_global(PAD);
    abi::emit_frame_prologue(emitter, PAD_FRAME);
    abi::store_at_offset(emitter, arg(emitter, 0), PAD_SRC);
    abi::store_at_offset(emitter, arg(emitter, 1), PAD_LENGTH);
    abi::store_at_offset(emitter, arg(emitter, 2), PAD_VALUE);
    emit_unbox_array_slot(emitter, PAD_SRC, &label("invalid"));
    abi::load_at_offset(emitter, result, PAD_VALUE);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::store_at_offset(emitter, result, PAD_VALUE);
    abi::store_at_offset(emitter, low, PAD_VALUE_LOW);
    abi::store_at_offset(emitter, high, PAD_VALUE_HIGH);

    // -- how many pads, and whether the source keys stand unchanged --
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x9", PAD_SRC);
            emitter.instruction("ldr x10, [x9]");                               // both layouts begin with their live count
            abi::load_at_offset(emitter, "x11", PAD_LENGTH);
            emitter.instruction("cmp x11, #0");                                 // the sign picks the padding side
            emitter.instruction("cneg x12, x11, lt");                           // the caller already bounded the magnitude
            emitter.instruction("subs x12, x12, x10");                          // pads = |length| - count
            emitter.instruction("csel x12, x12, xzr, gt");                      // no padding when the array is long enough
            emitter.instruction("cset x13, le");                                // and then the source keys stand as they are
            abi::store_at_offset_scratch(emitter, "x10", PAD_COUNT, "x14");
            abi::store_at_offset_scratch(emitter, "x12", PAD_PADS, "x14");
            abi::store_at_offset_scratch(emitter, "x13", PAD_PRESERVE, "x14");
            emitter.instruction("add x0, x10, x12");                            // the result holds every element and every pad
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r9", PAD_SRC);
            emitter.instruction("mov r10, QWORD PTR [r9]");                     // both layouts begin with their live count
            abi::load_at_offset(emitter, "r11", PAD_LENGTH);
            emitter.instruction("mov r8, r11");                                 // copy the signed length before taking its magnitude
            emitter.instruction("neg r8");                                      // the caller already bounded the magnitude
            emitter.instruction("cmovl r8, r11");                               // keep the original when it was already positive
            emitter.instruction("sub r8, r10");                                 // pads = |length| - count
            emitter.instruction("mov rcx, 0");                                  // no padding when the array is long enough
            emitter.instruction("cmovle r8, rcx");                              // clamp a non-positive difference to zero pads
            emitter.instruction("setle cl");                                    // and then the source keys stand as they are
            abi::store_at_offset(emitter, "r10", PAD_COUNT);
            abi::store_at_offset(emitter, "r8", PAD_PADS);
            abi::store_at_offset(emitter, "rcx", PAD_PRESERVE);
            emitter.instruction("lea rax, [r10 + r8]");                         // the result holds every element and every pad
        }
    }
    emit_new_mixed_hash(emitter, result);
    abi::store_at_offset(emitter, result, PAD_RESULT);
    abi::emit_store_zero_to_local_slot(emitter, PAD_NEXT_INT);

    // -- a negative length pads in front --
    abi::load_at_offset(emitter, result, PAD_LENGTH);
    emit_branch_if_result_not_negative(emitter, &label("elements"));
    emit_pads(emitter, &label("front"));
    emitter.label(&label("elements"));
    abi::emit_store_zero_to_local_slot(emitter, PAD_CURSOR);
    emitter.label(&label("loop"));
    emit_iter_next(emitter, PAD_SRC, PAD_CURSOR, PAD_ELEM, PAD_KEY_LO, &label("tail"));
    abi::load_at_offset(emitter, result, PAD_PRESERVE);
    abi::emit_branch_if_int_result_nonzero(emitter, &label("key_ready"));
    emit_renumber_int_key(emitter, PAD_KEY_LO, PAD_NEXT_INT, &label("key_ready"));
    emitter.label(&label("key_ready"));
    emit_own_cell(emitter, PAD_ELEM, &label("elem_owned"));
    emitter.label(&label("elem_owned"));
    emit_insert(emitter, PAD_RESULT, PAD_KEY_LO, PAD_ELEM);
    abi::emit_jump(emitter, &label("loop"));

    // -- a positive length pads behind --
    emitter.label(&label("tail"));
    abi::load_at_offset(emitter, result, PAD_LENGTH);
    emit_branch_if_result_negative(emitter, &label("done"));
    emit_pads(emitter, &label("back"));
    emitter.label(&label("done"));
    abi::load_at_offset(emitter, result, PAD_RESULT);
    abi::emit_frame_restore(emitter, PAD_FRAME);
    abi::emit_return(emitter);

    emitter.label(&label("invalid"));
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, PAD_FRAME);
    abi::emit_return(emitter);
}

/// Inserts PAD_PADS copies of the pad value under the next integer keys.
fn emit_pads(emitter: &mut Emitter, prefix: &str) {
    let result = abi::int_result_reg(emitter);
    let scratch = scratch_reg(emitter);
    abi::load_at_offset(emitter, result, PAD_PADS);
    abi::store_at_offset(emitter, result, PAD_REMAINING);
    emitter.label(&format!("{prefix}_loop"));
    abi::load_at_offset(emitter, result, PAD_REMAINING);
    abi::emit_branch_if_int_result_zero(emitter, &format!("{prefix}_done"));
    emit_add_immediate(emitter, result, -1);
    abi::store_at_offset(emitter, result, PAD_REMAINING);
    abi::load_at_offset(emitter, result, PAD_NEXT_INT);
    abi::store_at_offset_scratch(emitter, result, PAD_KEY_LO, scratch);
    emit_add_immediate(emitter, result, 1);
    abi::store_at_offset_scratch(emitter, result, PAD_NEXT_INT, scratch);
    abi::emit_load_int_immediate(emitter, result, -1);
    abi::store_at_offset_scratch(emitter, result, PAD_KEY_HI, scratch);
    for (from, to) in [(PAD_VALUE, PAD_ELEM), (PAD_VALUE_LOW, PAD_ELEM_LOW), (PAD_VALUE_HIGH, PAD_ELEM_HIGH)] {
        abi::load_at_offset(emitter, result, from);
        abi::store_at_offset_scratch(emitter, result, to, scratch);
    }
    emit_own_cell(emitter, PAD_ELEM, &format!("{prefix}_owned"));
    emitter.label(&format!("{prefix}_owned"));
    emit_insert(emitter, PAD_RESULT, PAD_KEY_LO, PAD_ELEM);
    abi::emit_jump(emitter, &format!("{prefix}_loop"));
    emitter.label(&format!("{prefix}_done"));
}

/// Borrows the array cell in ABI arg 0, the positive chunk size in arg 1 and the preserve-keys
/// flag in arg 2; returns an owned hash of hashes, or 0 when the operand is not an array. The
/// caller rejects a size below one first.
pub fn emit_array_chunk_boxed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg = |emitter: &Emitter, index: usize| abi::int_arg_reg_name(emitter.target, index);
    let label = |suffix: &str| format!("{CHUNK}_{suffix}");
    let scratch = scratch_reg(emitter);
    emitter.blank();
    emitter.label_global(CHUNK);
    abi::emit_frame_prologue(emitter, CHUNK_FRAME);
    abi::store_at_offset(emitter, arg(emitter, 0), CHUNK_SRC);
    abi::store_at_offset(emitter, arg(emitter, 1), CHUNK_SIZE);
    abi::store_at_offset(emitter, arg(emitter, 2), CHUNK_PRESERVE);
    emit_unbox_array_slot(emitter, CHUNK_SRC, &label("invalid"));
    abi::emit_load_int_immediate(emitter, result, 8);
    emit_new_mixed_hash(emitter, result);
    abi::store_at_offset(emitter, result, CHUNK_RESULT);
    abi::emit_store_zero_to_local_slot(emitter, CHUNK_CURRENT);
    abi::emit_store_zero_to_local_slot(emitter, CHUNK_INDEX);
    abi::emit_store_zero_to_local_slot(emitter, CHUNK_CURSOR);

    emitter.label(&label("loop"));
    emit_iter_next(emitter, CHUNK_SRC, CHUNK_CURSOR, CHUNK_ELEM, CHUNK_KEY_LO, &label("tail"));
    abi::load_at_offset(emitter, result, CHUNK_CURRENT);
    abi::emit_branch_if_int_result_nonzero(emitter, &label("chunk_open"));
    // -- open a chunk sized for min(size, source count), never below the table minimum --
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x9", CHUNK_SRC);
            emitter.instruction("ldr x9, [x9]");                                // the source count bounds every chunk
            abi::load_at_offset(emitter, "x10", CHUNK_SIZE);
            emitter.instruction("cmp x10, x9");                                 // a chunk never holds more than the source
            emitter.instruction("csel x0, x10, x9, lt");                        // capacity = min(size, count)
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r9", CHUNK_SRC);
            emitter.instruction("mov r9, QWORD PTR [r9]");                      // the source count bounds every chunk
            abi::load_at_offset(emitter, "rax", CHUNK_SIZE);
            emitter.instruction("cmp rax, r9");                                 // a chunk never holds more than the source
            emitter.instruction("cmovge rax, r9");                              // capacity = min(size, count)
        }
    }
    emit_new_mixed_hash(emitter, result);
    abi::store_at_offset(emitter, result, CHUNK_CURRENT);
    abi::emit_store_zero_to_local_slot(emitter, CHUNK_FILLED);
    abi::emit_store_zero_to_local_slot(emitter, CHUNK_NEXT_INT);
    emitter.label(&label("chunk_open"));
    abi::load_at_offset(emitter, result, CHUNK_PRESERVE);
    abi::emit_branch_if_int_result_nonzero(emitter, &label("key_ready"));
    // without preserve_keys every chunk is a fresh list, string keys included
    abi::load_at_offset(emitter, result, CHUNK_NEXT_INT);
    abi::store_at_offset_scratch(emitter, result, CHUNK_KEY_LO, scratch);
    emit_add_immediate(emitter, result, 1);
    abi::store_at_offset_scratch(emitter, result, CHUNK_NEXT_INT, scratch);
    abi::emit_load_int_immediate(emitter, result, -1);
    abi::store_at_offset_scratch(emitter, result, CHUNK_KEY_HI, scratch);
    emitter.label(&label("key_ready"));
    emit_own_cell(emitter, CHUNK_ELEM, &label("elem_owned"));
    emitter.label(&label("elem_owned"));
    emit_insert(emitter, CHUNK_CURRENT, CHUNK_KEY_LO, CHUNK_ELEM);
    abi::load_at_offset(emitter, result, CHUNK_FILLED);
    emit_add_immediate(emitter, result, 1);
    abi::store_at_offset(emitter, result, CHUNK_FILLED);
    abi::load_at_offset(emitter, scratch, CHUNK_SIZE);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, {scratch}"));                 // has the open chunk reached the requested size?
            emitter.instruction(&format!("b.lt {}", label("loop")));            // keep filling it
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {scratch}"));                // has the open chunk reached the requested size?
            emitter.instruction(&format!("jl {}", label("loop")));              // keep filling it
        }
    }
    emit_flush_chunk(emitter);
    abi::emit_jump(emitter, &label("loop"));

    // -- a partial last chunk still belongs to the result --
    emitter.label(&label("tail"));
    abi::load_at_offset(emitter, result, CHUNK_CURRENT);
    abi::emit_branch_if_int_result_zero(emitter, &label("done"));
    emit_flush_chunk(emitter);
    emitter.label(&label("done"));
    abi::load_at_offset(emitter, result, CHUNK_RESULT);
    abi::emit_frame_restore(emitter, CHUNK_FRAME);
    abi::emit_return(emitter);

    emitter.label(&label("invalid"));
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, CHUNK_FRAME);
    abi::emit_return(emitter);
}

/// Moves the open chunk into the result under the next list index; the result owns it now.
fn emit_flush_chunk(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg = |emitter: &Emitter, index: usize| abi::int_arg_reg_name(emitter.target, index);
    abi::load_at_offset(emitter, arg(emitter, 0), CHUNK_RESULT);
    abi::load_at_offset(emitter, arg(emitter, 1), CHUNK_INDEX);
    abi::emit_load_int_immediate(emitter, arg(emitter, 2), -1);
    abi::load_at_offset(emitter, arg(emitter, 3), CHUNK_CURRENT);
    abi::emit_load_int_immediate(emitter, arg(emitter, 4), 0);
    abi::emit_load_int_immediate(emitter, arg(emitter, 5), 5);
    abi::emit_call_label(emitter, "__rt_hash_set");
    abi::store_at_offset(emitter, result, CHUNK_RESULT);
    abi::load_at_offset(emitter, result, CHUNK_INDEX);
    emit_add_immediate(emitter, result, 1);
    abi::store_at_offset(emitter, result, CHUNK_INDEX);
    abi::emit_store_zero_to_local_slot(emitter, CHUNK_CURRENT);
}

/// Allocates a hash with the capacity in the result register (at least 8) and a Mixed value
/// summary: entries carry their own runtime tags.
fn emit_new_mixed_hash(emitter: &mut Emitter, capacity: &str) {
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x9, #8");                                  // never ask for a zero-capacity table
            emitter.instruction(&format!("cmp {capacity}, x9"));                // choose the larger capacity hint
            emitter.instruction(&format!("csel {arg0}, {capacity}, x9, ge"));   // size the table for its entries
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, 8");                                   // never ask for a zero-capacity table
            emitter.instruction(&format!("cmp {capacity}, r9"));                // choose the larger capacity hint
            emitter.instruction(&format!("cmovl {capacity}, r9"));              // size the table for its entries
            emitter.instruction(&format!("mov {arg0}, {capacity}"));            // pass the capacity hint
        }
    }
    abi::emit_load_int_immediate(emitter, arg1, 7);
    abi::emit_call_label(emitter, "__rt_hash_new");
}

/// Replaces an integer key (high word -1) with the next renumbered index; string keys stand.
fn emit_renumber_int_key(emitter: &mut Emitter, key_lo: usize, next_int: usize, done: &str) {
    let result = abi::int_result_reg(emitter);
    let scratch = scratch_reg(emitter);
    abi::load_at_offset(emitter, result, key_lo - 8);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmn x0, #1");                                  // minus one marks an integer key
            emitter.instruction(&format!("b.ne {done}"));                       // string keys keep their name
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, -1");                                 // minus one marks an integer key
            emitter.instruction(&format!("jne {done}"));                        // string keys keep their name
        }
    }
    abi::load_at_offset(emitter, result, next_int);
    abi::store_at_offset_scratch(emitter, result, key_lo, scratch);
    emit_add_immediate(emitter, result, 1);
    abi::store_at_offset_scratch(emitter, result, next_int, scratch);
}

/// Inserts the owned cell under the key pair into the hash in `table`, keeping its new pointer.
fn emit_insert(emitter: &mut Emitter, table: usize, key_lo: usize, cell: usize) {
    let result = abi::int_result_reg(emitter);
    let arg = |emitter: &Emitter, index: usize| abi::int_arg_reg_name(emitter.target, index);
    abi::load_at_offset(emitter, arg(emitter, 0), table);
    abi::load_at_offset(emitter, arg(emitter, 1), key_lo);
    abi::load_at_offset(emitter, arg(emitter, 2), key_lo - 8);
    abi::load_at_offset(emitter, arg(emitter, 3), cell - 8);
    abi::load_at_offset(emitter, arg(emitter, 4), cell - 16);
    abi::load_at_offset(emitter, arg(emitter, 5), cell);
    abi::emit_call_label(emitter, "__rt_hash_set");
    abi::store_at_offset(emitter, result, table);
}

/// Unboxes the borrowed cell whose pointer is in `slot`, rejects a non-array, and replaces the
/// slot with the array payload pointer.
fn emit_unbox_array_slot(emitter: &mut Emitter, slot: usize, invalid: &str) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, result, slot);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub x9, x0, #4");                              // fold valid packed/hash tags into zero and one
            emitter.instruction("cmp x9, #1");                                  // only PHP arrays may be traversed
            emitter.instruction(&format!("b.hi {invalid}"));                    // reject scalar, object and null payloads before dereference
            abi::store_at_offset_scratch(emitter, "x1", slot, "x10");
        }
        Arch::X86_64 => {
            emitter.instruction("lea r10, [rax - 4]");                          // fold valid packed/hash tags into zero and one
            emitter.instruction("cmp r10, 1");                                  // only PHP arrays may be traversed
            emitter.instruction(&format!("ja {invalid}"));                      // reject scalar, object and null payloads before dereference
            abi::store_at_offset(emitter, "rdi", slot);
        }
    }
}

/// Advances the iterator over the array in `src` with the cursor in `cursor`, branching to `done`
/// when exhausted; spills the key pair at `key_lo` / `key_lo - 8` and the element cell at `cell`.
fn emit_iter_next(
    emitter: &mut Emitter,
    src: usize,
    cursor: usize,
    cell: usize,
    key_lo: usize,
    done: &str,
) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), src);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), cursor);
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    let (klo, khi, tag, lo, hi) = match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmn x0, #1");                                  // minus one signals iterator exhaustion
            emitter.instruction(&format!("b.eq {done}"));                       // the whole array has been visited
            ("x1", "x2", "x3", "x4", "x5")
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, -1");                                 // minus one signals iterator exhaustion
            emitter.instruction(&format!("je {done}"));                         // the whole array has been visited
            ("rcx", "rdx", "r8", "r9", "r10")
        }
    };
    let scratch = scratch_reg(emitter);
    for (reg, slot) in [
        (result, cursor),
        (klo, key_lo),
        (khi, key_lo - 8),
        (tag, cell),
        (lo, cell - 8),
        (hi, cell - 16),
    ] {
        abi::store_at_offset_scratch(emitter, reg, slot, scratch);
    }
}

/// Gives the cell at `cell` a copy its new owner holds: a string is duplicated with
/// `__rt_str_persist`, the heap-backed tags 4..=7, callables (10) and reference cells (11) are
/// retained. Falls through to `owned`.
fn emit_own_cell(emitter: &mut Emitter, cell: usize, owned: &str) {
    let result = abi::int_result_reg(emitter);
    let retain = format!("{owned}_retain");
    let persist = format!("{owned}_persist");
    let tag = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, tag, cell);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {tag}, #1"));                     // a string payload is owned by its array
            emitter.instruction(&format!("b.eq {persist}"));                    // strings are copied, not shared
            emitter.instruction(&format!("cmp {tag}, #4"));                     // tags below 4 are scalar payloads
            emitter.instruction(&format!("b.lt {owned}"));                      // scalars own nothing
            emitter.instruction(&format!("cmp {tag}, #7"));                     // arrays, hashes, objects and boxes are heap-backed
            emitter.instruction(&format!("b.le {retain}"));                     // these carry a reference count
            emitter.instruction(&format!("cmp {tag}, #10"));                    // callables and reference cells are heap-backed too
            emitter.instruction(&format!("b.lt {owned}"));                      // null and resources own nothing
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {tag}, 1"));                      // a string payload is owned by its array
            emitter.instruction(&format!("je {persist}"));                      // strings are copied, not shared
            emitter.instruction(&format!("cmp {tag}, 4"));                      // tags below 4 are scalar payloads
            emitter.instruction(&format!("jl {owned}"));                        // scalars own nothing
            emitter.instruction(&format!("cmp {tag}, 7"));                      // arrays, hashes, objects and boxes are heap-backed
            emitter.instruction(&format!("jle {retain}"));                      // these carry a reference count
            emitter.instruction(&format!("cmp {tag}, 10"));                     // callables and reference cells are heap-backed too
            emitter.instruction(&format!("jl {owned}"));                        // null and resources own nothing
        }
    }
    emitter.label(&retain);
    abi::load_at_offset(emitter, result, cell - 8);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_jump(emitter, owned);
    emitter.label(&persist);
    let (ptr, len) = abi::string_result_regs(emitter);
    abi::load_at_offset(emitter, ptr, cell - 8);
    abi::load_at_offset(emitter, len, cell - 16);
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::store_at_offset_scratch(emitter, ptr, cell - 8, scratch_reg(emitter));
    abi::store_at_offset_scratch(emitter, len, cell - 16, scratch_reg(emitter));
}

/// Adds a small signed immediate to `reg` in place.
fn emit_add_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    match emitter.target.arch {
        Arch::AArch64 if value >= 0 => emitter.instruction(&format!("add {reg}, {reg}, #{value}")), // advance the counter
        Arch::AArch64 => emitter.instruction(&format!("sub {reg}, {reg}, #{}", -value)),           // step the counter back
        Arch::X86_64 => emitter.instruction(&format!("add {reg}, {value}")),                       // adjust the counter in place
    }
}

/// Branches when the signed integer result register is negative.
fn emit_branch_if_result_negative(emitter: &mut Emitter, target: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // inspect the sign of the loaded word
            emitter.instruction(&format!("b.lt {target}"));                     // take the branch for a negative value
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // inspect the sign of the loaded word
            emitter.instruction(&format!("js {target}"));                       // take the branch for a negative value
        }
    }
}

/// Branches when the signed integer result register is zero or positive.
fn emit_branch_if_result_not_negative(emitter: &mut Emitter, target: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // inspect the sign of the loaded word
            emitter.instruction(&format!("b.ge {target}"));                     // take the branch for a non-negative value
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // inspect the sign of the loaded word
            emitter.instruction(&format!("jns {target}"));                      // take the branch for a non-negative value
        }
    }
}

/// The payload registers `__rt_mixed_unbox` returns beside the tag.
fn unbox_payload_regs(emitter: &Emitter) -> (&'static str, &'static str) {
    match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rdx"),
    }
}

/// A scratch register for frame-slot stores, distinct from every value being stored.
fn scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r11",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Both builders read any layout through the logical iterator and copy what they keep, on
    /// every target.
    #[test]
    fn pad_and_chunk_builders_iterate_and_own_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_pad_boxed(&mut emitter);
            emit_array_chunk_boxed(&mut emitter);
            let asm = emitter.output();
            let (pad, chunk) = asm.split_once(&format!("{CHUNK}:")).unwrap();
            for body in [pad, chunk] {
                for helper in ["__rt_array_iter_next", "__rt_hash_set", "__rt_str_persist", "__rt_incref"] {
                    assert!(body.contains(helper), "{target}: {helper}");
                }
            }
            assert!(pad.contains("__rt_mixed_unbox"), "{target}: the pad value is unboxed once");
        }
    }
}
