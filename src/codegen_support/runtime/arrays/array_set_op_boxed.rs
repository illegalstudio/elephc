//! Purpose:
//! Emits `__rt_array_set_op_boxed`, the by-value `array_diff()` / `array_intersect()` /
//! `array_unique()` scan over any PHP array layout.
//!
//! Called from:
//! - The typed ArrayDiff and ArrayIntersect backends, and ArrayUnique for boxed elements.
//!
//! Key details:
//! - php compares these builtins' elements by their STRING rendering (`(string) $a ===
//!   (string) $b`), whatever their type, so every element is rendered with
//!   `__rt_mixed_cast_string` and looked up in a hash keyed by that rendering. A raw-slot compare
//!   matched pointers, not values, which is why boxed elements used to be refused.
//! - The result is a fresh hash under the SOURCE's keys, as php's are: `array_diff([1, 2, 3],
//!   [2])` is `[0 => 1, 2 => 3]`, not the reindexed list the old indexed helper built.
//! - Both inputs are borrowed 24-byte cells (tag, low word, high word), so a packed array, a hash
//!   and a boxed `array` are all read through `__rt_array_iter_next`. A kept string is duplicated
//!   and a kept heap value retained, as `__rt_array_to_hash` gives its result its own values.
//! - Each rendering is released before the next one: a string rendering is a persisted copy that
//!   `__rt_heap_free` releases, a number is formatted into `_concat_buf`, whose offset is put back.
//!   `__rt_hash_set` persists the key it inserts, so nothing keeps a rendering alive.
//! - Rendering can run user code: an object's `__toString`, or an error handler for the "Array to
//!   string conversion" warning. So the scan is split like `__rt_array_flip_boxed`: the boundary
//!   retains both arrays (a handler may unset the variable holding one), runs the body through
//!   `__rt_cleanup_invoke`, and on a throw releases the partial result and the rendering set the
//!   body published in the shared context before rethrowing.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, runtime::exceptions};

/// Mode argument: keep source elements whose rendering is absent from the other array.
pub const MODE_DIFF: i64 = 0;
/// Mode argument: keep source elements whose rendering is present in the other array.
pub const MODE_INTERSECT: i64 = 1;
/// Mode argument: keep the first source element of each rendering; the other array is unused.
pub const MODE_UNIQUE: i64 = 2;

const NAME: &str = "__rt_array_set_op_boxed";
const BODY: &str = "__rt_array_set_op_boxed_body";

// Boundary frame, as offsets below the frame pointer. The context the body receives is the
// address of RESULT; the following words sit at increasing addresses, so `context + 8` is SET.
const FRAME: usize = 64;
const PENDING: usize = 8;
const MODE: usize = 16;
const OTHER: usize = 24;
const SRC: usize = 32;
const SET: usize = 40;
const RESULT: usize = 48;
const CTX_RESULT: usize = 0;
const CTX_SET: usize = 8;
const CTX_SRC: usize = 16;
const CTX_OTHER: usize = 24;
const CTX_MODE: usize = 32;

// Body frame. The element cell is three ascending words starting at CELL: tag, low, high.
const BODY_FRAME: usize = 112;
const CONTEXT: usize = 8;
const CURSOR: usize = 16;
const CELL_HIGH: usize = 32;
const CELL_LOW: usize = 40;
const CELL: usize = 48;
const KEY_LO: usize = 56;
const KEY_HI: usize = 64;
const RENDER_PTR: usize = 72;
const RENDER_LEN: usize = 80;
const CONCAT: usize = 88;
const FOUND: usize = 96;

/// Borrows source/other cells in ABI args 0/1 and the mode in arg 2.
///
/// Returns the owned result hash, or 0 when either operand is not an array, with the rejected
/// operand's one-based position in the second integer result register (aarch64 x1, x86_64 rdx).
/// The caller raises the TypeError; nothing has been allocated by then. A throw out of a
/// rendering propagates after the partial result is released.
pub fn emit_array_set_op_boxed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg = |emitter: &Emitter, index: usize| abi::int_arg_reg_name(emitter.target, index);
    let label = |suffix: &str| format!("{NAME}_{suffix}");
    emitter.blank();
    emitter.label_global(NAME);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, arg(emitter, 0), SRC);
    abi::store_at_offset(emitter, arg(emitter, 1), OTHER);
    abi::store_at_offset(emitter, arg(emitter, 2), MODE);

    // -- validate and unbox both operands before allocating anything --
    emit_unbox_array_slot(emitter, SRC, &label("invalid_first"));
    abi::load_at_offset(emitter, result, MODE);
    emit_branch_if_result_equals(emitter, MODE_UNIQUE, &label("unique_operand"));
    emit_unbox_array_slot(emitter, OTHER, &label("invalid_second"));
    abi::emit_jump(emitter, &label("operands_ready"));
    emitter.label(&label("unique_operand"));
    abi::emit_store_zero_to_local_slot(emitter, OTHER);
    emitter.label(&label("operands_ready"));

    // -- keep both arrays alive while user code runs, then scan under a cleanup boundary --
    abi::load_at_offset(emitter, result, SRC);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, result, OTHER);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_store_zero_to_local_slot(emitter, RESULT);
    abi::emit_store_zero_to_local_slot(emitter, SET);
    abi::emit_store_zero_to_local_slot(emitter, PENDING);
    abi::emit_frame_slot_address(emitter, result, RESULT);
    exceptions::emit_guarded_cleanup_call(emitter, BODY, result, PENDING);

    // -- the set only ever held rendering keys and integers, so its release runs no user code --
    abi::load_at_offset(emitter, result, SET);
    abi::emit_call_label(emitter, "__rt_decref_hash");
    abi::load_at_offset(emitter, result, SRC);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, PENDING);
    abi::load_at_offset(emitter, result, OTHER);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, PENDING);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_nonzero(emitter, &label("throw"));
    abi::load_at_offset(emitter, result, RESULT);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);

    // -- a rendering threw: drop the partial result, then rethrow the chained exception --
    let low = match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    };
    emitter.label(&label("throw"));
    abi::load_at_offset(emitter, result, RESULT);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, PENDING);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_load_symbol_to_reg(emitter, low, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");

    // -- a rejected operand: zero, with its one-based position in the second result word --
    let position = match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdx",
    };
    emitter.label(&label("invalid_first"));
    abi::emit_load_int_immediate(emitter, position, 1);
    abi::emit_jump(emitter, &label("invalid"));
    emitter.label(&label("invalid_second"));
    abi::emit_load_int_immediate(emitter, position, 2);
    emitter.label(&label("invalid"));
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emit_body(emitter);
}

/// Fills the context's result, publishing every new result and set pointer before the next
/// rendering can run user code.
fn emit_body(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg = |emitter: &Emitter, index: usize| abi::int_arg_reg_name(emitter.target, index);
    let label = |suffix: &str| format!("{BODY}_{suffix}");
    emitter.blank();
    emitter.label_global(BODY);
    abi::emit_frame_prologue(emitter, BODY_FRAME);
    abi::store_at_offset(emitter, result, CONTEXT);

    // -- the rendering set: string keys, integer payloads --
    abi::emit_load_int_immediate(emitter, arg(emitter, 0), 8);
    abi::emit_load_int_immediate(emitter, arg(emitter, 1), 0);
    abi::emit_call_label(emitter, "__rt_hash_new");
    publish(emitter, CTX_SET);

    // -- diff/intersect: render every element of the other array into the set --
    load_context_word(emitter, result, CTX_MODE);
    emit_branch_if_result_equals(emitter, MODE_UNIQUE, &label("set_ready"));
    abi::emit_store_zero_to_local_slot(emitter, CURSOR);
    emitter.label(&label("fill_loop"));
    emit_iter_next(emitter, CTX_OTHER, &label("set_ready"));
    emit_render_cell(emitter);
    emit_insert_rendering_into_set(emitter);
    emit_release_rendering(emitter);
    abi::emit_jump(emitter, &label("fill_loop"));
    emitter.label(&label("set_ready"));

    // -- the result: the source's capacity and value layout, its keys preserved --
    emit_new_result_like_source(emitter);
    publish(emitter, CTX_RESULT);
    abi::emit_store_zero_to_local_slot(emitter, CURSOR);

    emitter.label(&label("loop"));
    emit_iter_next(emitter, CTX_SRC, &label("done"));
    emit_render_cell(emitter);
    load_context_word(emitter, arg(emitter, 0), CTX_SET);
    abi::load_at_offset(emitter, arg(emitter, 1), RENDER_PTR);
    abi::load_at_offset(emitter, arg(emitter, 2), RENDER_LEN);
    abi::emit_call_label(emitter, "__rt_hash_get");
    abi::store_at_offset(emitter, result, FOUND);
    // unique: the first element of each rendering joins the set, so later ones are found
    load_context_word(emitter, result, CTX_MODE);
    emit_branch_if_result_not_equals(emitter, MODE_UNIQUE, &label("rendering_used"));
    abi::load_at_offset(emitter, result, FOUND);
    emit_branch_if_result_not_equals(emitter, 0, &label("rendering_used"));
    emit_insert_rendering_into_set(emitter);
    emitter.label(&label("rendering_used"));
    emit_release_rendering(emitter);

    // -- keep: intersect keeps found elements, diff and unique keep the others --
    load_context_word(emitter, result, CTX_MODE);
    emit_branch_if_result_equals(emitter, MODE_INTERSECT, &label("keep_if_found"));
    abi::load_at_offset(emitter, result, FOUND);
    emit_branch_if_result_not_equals(emitter, 0, &label("loop"));
    abi::emit_jump(emitter, &label("keep"));
    emitter.label(&label("keep_if_found"));
    abi::load_at_offset(emitter, result, FOUND);
    abi::emit_branch_if_int_result_zero(emitter, &label("loop"));
    emitter.label(&label("keep"));
    emit_retain_cell_payload(emitter, &label("retained"));
    emitter.label(&label("retained"));
    load_context_word(emitter, arg(emitter, 0), CTX_RESULT);
    abi::load_at_offset(emitter, arg(emitter, 1), KEY_LO);
    abi::load_at_offset(emitter, arg(emitter, 2), KEY_HI);
    abi::load_at_offset(emitter, arg(emitter, 3), CELL_LOW);
    abi::load_at_offset(emitter, arg(emitter, 4), CELL_HIGH);
    abi::load_at_offset(emitter, arg(emitter, 5), CELL);
    abi::emit_call_label(emitter, "__rt_hash_set");
    publish(emitter, CTX_RESULT);
    abi::emit_jump(emitter, &label("loop"));

    emitter.label(&label("done"));
    abi::emit_frame_restore(emitter, BODY_FRAME);
    abi::emit_return(emitter);
}

/// Loads one word of the boundary's context into `dest`.
fn load_context_word(emitter: &mut Emitter, dest: &str, field: usize) {
    abi::load_at_offset(emitter, dest, CONTEXT);
    abi::emit_load_from_address(emitter, dest, dest, field);
}

/// Stores the integer result register into one word of the boundary's context.
fn publish(emitter: &mut Emitter, field: usize) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, scratch, CONTEXT);
    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), scratch, field);
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

/// Advances the iterator over the array in context word `field`, branching to `done` when
/// exhausted, and spills the cursor, the key and the element cell.
fn emit_iter_next(emitter: &mut Emitter, field: usize, done: &str) {
    let result = abi::int_result_reg(emitter);
    load_context_word(emitter, abi::int_arg_reg_name(emitter.target, 0), field);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), CURSOR);
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    let (key_lo, key_hi, tag, lo, hi) = match emitter.target.arch {
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
    let scratch = match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r11",
    };
    for (reg, slot) in [
        (result, CURSOR),
        (key_lo, KEY_LO),
        (key_hi, KEY_HI),
        (tag, CELL),
        (lo, CELL_LOW),
        (hi, CELL_HIGH),
    ] {
        abi::store_at_offset_scratch(emitter, reg, slot, scratch);
    }
}

/// Renders the element cell as php's `(string)` would, remembering the scratch offset first.
fn emit_render_cell(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let (scratch_addr, scratch_value) = scratch_regs(emitter);
    abi::emit_symbol_address(emitter, scratch_addr, "_concat_off");
    load_from(emitter, scratch_value, scratch_addr);
    abi::store_at_offset(emitter, scratch_value, CONCAT);
    abi::emit_frame_slot_address(emitter, result, CELL);
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");
    let (ptr, len) = abi::string_result_regs(emitter);
    abi::store_at_offset(emitter, ptr, RENDER_PTR);
    abi::store_at_offset(emitter, len, RENDER_LEN);
}

/// Inserts the current rendering into the set as a key, with an integer payload.
fn emit_insert_rendering_into_set(emitter: &mut Emitter) {
    let arg = |emitter: &Emitter, index: usize| abi::int_arg_reg_name(emitter.target, index);
    load_context_word(emitter, arg(emitter, 0), CTX_SET);
    abi::load_at_offset(emitter, arg(emitter, 1), RENDER_PTR);
    abi::load_at_offset(emitter, arg(emitter, 2), RENDER_LEN);
    abi::emit_load_int_immediate(emitter, arg(emitter, 3), 1);
    abi::emit_load_int_immediate(emitter, arg(emitter, 4), 0);
    abi::emit_load_int_immediate(emitter, arg(emitter, 5), 0);
    abi::emit_call_label(emitter, "__rt_hash_set");
    publish(emitter, CTX_SET);
}

/// Frees a persisted rendering (`__rt_heap_free` ignores scratch) and rewinds `_concat_buf`.
fn emit_release_rendering(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, result, RENDER_PTR);
    abi::emit_call_label(emitter, "__rt_heap_free");
    let (scratch_addr, scratch_value) = scratch_regs(emitter);
    abi::emit_symbol_address(emitter, scratch_addr, "_concat_off");
    abi::load_at_offset(emitter, scratch_value, CONCAT);
    store_to(emitter, scratch_value, scratch_addr);
}

/// Allocates the result hash with the source's element count and value layout.
fn emit_new_result_like_source(emitter: &mut Emitter) {
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let ready = format!("{BODY}_layout_ready");
    match emitter.target.arch {
        Arch::AArch64 => {
            load_context_word(emitter, "x9", CTX_SRC);
            emitter.instruction("ldr x10, [x9, #-8]");                          // read the source's heap kind word
            emitter.instruction("and x11, x10, #255");                          // isolate the container kind (2 packed, 3 hash)
            emitter.instruction("ubfx x12, x10, #8, #7");                       // a packed array keeps its value tag in the kind word
            emitter.instruction("cmp x11, #3");                                 // a hash keeps it in its header instead
            emitter.instruction(&format!("b.ne {ready}"));                      // packed: the kind-word tag stands
            emitter.instruction("ldr x12, [x9, #16]");                          // hash: read the header's value type
            emitter.label(&ready);
            emitter.instruction("cmp x12, #11");                                // inline tagged scalars are a packed storage marker, not a value tag
            emitter.instruction("mov x13, #7");                                 // their entries keep per-slot tags, as a Mixed hash does
            emitter.instruction("csel x12, x13, x12, eq");                      // only the tagged-scalar marker is replaced
            emitter.instruction("ldr x13, [x9]");                               // both layouts begin with their live count
            emitter.instruction("mov x14, #8");                                 // never ask for a zero-capacity table
            emitter.instruction("cmp x13, x14");                                // choose the larger capacity hint
            emitter.instruction(&format!("csel {arg0}, x13, x14, ge"));         // size the result for every source element
            emitter.instruction(&format!("mov {arg1}, x12"));                   // the result keeps the source's value layout
        }
        Arch::X86_64 => {
            load_context_word(emitter, "r11", CTX_SRC);
            emitter.instruction("mov r10, QWORD PTR [r11 - 8]");                // read the source's heap kind word
            emitter.instruction("mov r9, r10");                                 // copy the kind word before extracting the value tag
            emitter.instruction("shr r9, 8");                                   // a packed array keeps its value tag above the kind byte
            emitter.instruction("and r9, 127");                                 // isolate the seven-bit value tag
            emitter.instruction("and r10, 255");                                // isolate the container kind (2 packed, 3 hash)
            emitter.instruction("cmp r10, 3");                                  // a hash keeps the value type in its header
            emitter.instruction(&format!("jne {ready}"));                       // packed: the kind-word tag stands
            emitter.instruction("mov r9, QWORD PTR [r11 + 16]");                // hash: read the header's value type
            emitter.label(&ready);
            emitter.instruction("mov r8, 7");                                   // tagged-scalar entries keep per-slot tags, as a Mixed hash does
            emitter.instruction("cmp r9, 11");                                  // inline tagged scalars are a packed storage marker, not a value tag
            emitter.instruction("cmove r9, r8");                                // only the tagged-scalar marker is replaced
            emitter.instruction("mov r8, QWORD PTR [r11]");                     // both layouts begin with their live count
            emitter.instruction("mov rax, 8");                                  // never ask for a zero-capacity table
            emitter.instruction("cmp r8, rax");                                 // choose the larger capacity hint
            emitter.instruction("cmovge rax, r8");                              // size the result for every source element
            emitter.instruction(&format!("mov {arg0}, rax"));                   // pass the capacity hint
            emitter.instruction(&format!("mov {arg1}, r9"));                    // the result keeps the source's value layout
        }
    }
    abi::emit_call_label(emitter, "__rt_hash_new");
}

/// Gives the result its own copy of a kept element. A string is DUPLICATED with
/// `__rt_str_persist`, as `__rt_array_to_hash` does: an array owns its string bytes exclusively
/// and frees them on overwrite, so sharing them would leave the result reading freed bytes. The
/// heap-backed tags 4..=7, callables (10) and hash reference cells (11) are reference counted and
/// retained; `__rt_incref` skips static storage. Scalars, null and resources need nothing.
fn emit_retain_cell_payload(emitter: &mut Emitter, retained: &str) {
    let result = abi::int_result_reg(emitter);
    let retain = format!("{retained}_retain");
    let persist = format!("{retained}_persist");
    abi::load_at_offset(emitter, abi::secondary_scratch_reg(emitter), CELL);
    let tag = abi::secondary_scratch_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {tag}, #1"));                     // a string payload is owned by its array
            emitter.instruction(&format!("b.eq {persist}"));                    // duplicate the borrowed string bytes
            emitter.instruction(&format!("cmp {tag}, #4"));                     // tags below 4 are scalar payloads
            emitter.instruction(&format!("b.lt {retained}"));                   // scalars need no retain
            emitter.instruction(&format!("cmp {tag}, #7"));                     // arrays, hashes, objects and boxes are heap-backed
            emitter.instruction(&format!("b.le {retain}"));                     // retain the borrowed container, object or box
            emitter.instruction(&format!("cmp {tag}, #10"));                    // callables and reference cells are heap-backed too
            emitter.instruction(&format!("b.lt {retained}"));                   // null and resources need no retain
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {tag}, 1"));                      // a string payload is owned by its array
            emitter.instruction(&format!("je {persist}"));                      // duplicate the borrowed string bytes
            emitter.instruction(&format!("cmp {tag}, 4"));                      // tags below 4 are scalar payloads
            emitter.instruction(&format!("jl {retained}"));                     // scalars need no retain
            emitter.instruction(&format!("cmp {tag}, 7"));                      // arrays, hashes, objects and boxes are heap-backed
            emitter.instruction(&format!("jle {retain}"));                      // retain the borrowed container, object or box
            emitter.instruction(&format!("cmp {tag}, 10"));                     // callables and reference cells are heap-backed too
            emitter.instruction(&format!("jl {retained}"));                     // null and resources need no retain
        }
    }
    emitter.label(&retain);
    abi::load_at_offset(emitter, result, CELL_LOW);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_jump(emitter, retained);
    emitter.label(&persist);
    let (ptr, len) = abi::string_result_regs(emitter);
    abi::load_at_offset(emitter, ptr, CELL_LOW);
    abi::load_at_offset(emitter, len, CELL_HIGH);
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::store_at_offset_scratch(emitter, ptr, CELL_LOW, scratch_regs(emitter).0);
    abi::store_at_offset_scratch(emitter, len, CELL_HIGH, scratch_regs(emitter).0);
}

/// Branches when the integer result register equals `value`.
fn emit_branch_if_result_equals(emitter: &mut Emitter, value: i64, target: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{value}"));                  // compare the loaded word with the constant
            emitter.instruction(&format!("b.eq {target}"));                     // take the branch on equality
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {value}"));                  // compare the loaded word with the constant
            emitter.instruction(&format!("je {target}"));                       // take the branch on equality
        }
    }
}

/// Branches when the integer result register differs from `value`.
fn emit_branch_if_result_not_equals(emitter: &mut Emitter, value: i64, target: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{value}"));                  // compare the loaded word with the constant
            emitter.instruction(&format!("b.ne {target}"));                     // take the branch on inequality
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {value}"));                  // compare the loaded word with the constant
            emitter.instruction(&format!("jne {target}"));                      // take the branch on inequality
        }
    }
}

/// Two scratch registers no helper argument occupies while the scratch offset moves.
fn scratch_regs(emitter: &Emitter) -> (&'static str, &'static str) {
    match emitter.target.arch {
        Arch::AArch64 => ("x10", "x11"),
        Arch::X86_64 => ("r11", "r10"),
    }
}

/// Loads one word from the address in `addr` into `dest`.
fn load_from(emitter: &mut Emitter, dest: &str, addr: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("ldr {dest}, [{addr}]")), // read the word at the address
        Arch::X86_64 => emitter.instruction(&format!("mov {dest}, QWORD PTR [{addr}]")), // read the word at the address
    }
}

/// Stores one word from `src` to the address in `addr`.
fn store_to(emitter: &mut Emitter, src: &str, addr: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("str {src}, [{addr}]")), // write the word to the address
        Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [{addr}], {src}")), // write the word to the address
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The scan renders through the shared cast, indexes renderings in a hash, never allocates a
    /// Mixed box for an element, and runs its body under a cleanup boundary on every target.
    #[test]
    fn set_op_scan_renders_and_indexes_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_set_op_boxed(&mut emitter);
            let asm = emitter.output();
            let (boundary, body) = asm.split_once(&format!("{BODY}:")).unwrap();
            assert!(boundary.contains("__rt_cleanup_invoke"), "{target}");
            assert!(boundary.contains("__rt_throw_current"), "{target}");
            assert!(
                boundary.find("__rt_incref").unwrap() < boundary.find("__rt_cleanup_invoke").unwrap(),
                "{target}: retain the operands before user code can run"
            );
            for helper in [
                "__rt_array_iter_next",
                "__rt_mixed_cast_string",
                "__rt_hash_get",
                "__rt_hash_set",
                "__rt_heap_free",
            ] {
                assert!(body.contains(helper), "{target}: {helper}");
            }
            assert!(!asm.contains("__rt_mixed_from_value"), "{target}");
        }
    }
}
