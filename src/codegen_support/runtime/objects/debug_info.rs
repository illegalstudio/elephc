//! Purpose:
//! Emits the run-time `__debugInfo()` support of the object dump walkers:
//! `__rt_obj_debuginfo_fetch` calls a class's `__debugInfo()` through the dense
//! `_class_debuginfo_ptrs` table and classifies the result, while
//! `__rt_vd_debuginfo_object` / `__rt_pr_debuginfo_object` render the returned
//! array in place of the object's properties for `var_dump` and `print_r`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::objects`.
//! - The tag-6 branch of `__rt_var_dump_value` (`runtime::io::var_dump_walk`)
//!   calls `__rt_vd_debuginfo_object` after pushing the object on the `_vd_seen` stack.
//! - `__rt_print_r_object` (`runtime::objects::print_r_object`) calls
//!   `__rt_pr_debuginfo_object` at the same point of its walk.
//!
//! Key details:
//! - WHICH CLASSES. `_class_debuginfo_ptrs[class_id]` is non-zero only for user
//!   classes whose `__debugInfo()` is NOT a property projection the compiler already
//!   folds into the static descriptor rows (see `data::user`). Every other class
//!   keeps the descriptor walk, so classes without `__debugInfo()` pay one table
//!   probe and nothing else.
//! - RESULT SHAPES. The method may hand back a concrete indexed array (heap kind 2),
//!   a hash (kind 3) or a boxed `Mixed` cell (kind 5) wrapping one of those or null.
//!   PHP treats a null return as an empty array after the deprecation
//!   `Returning null from C::__debugInfo() is deprecated, return an empty array instead`
//!   (written through `__rt_diag_warning`, so handlers and `error_reporting` apply), and any
//!   other non-array as the uncatchable fatal `__debuginfo() must return an array`, which
//!   exits with status 255 right here (text on stdout after draining output buffers, like the
//!   uncaught-exception report), BEFORE the object header is written (var_dump) or after the
//!   `C Object` line (print_r), like PHP.
//! - EXCEPTIONS. An exception escaping `__debugInfo()` is NOT catchable in PHP 8.5: the engine
//!   prints `Warning: Uncaught <Class>: <message> in F:L` and then the same `__debuginfo() must
//!   return an array` fatal, exit 255, running no `catch`. The call runs with `_debuginfo_guard`
//!   set to `_exc_handler_top | 1`; `__rt_throw_current` compares it with the current handler top
//!   and, when no newer handler (a `try` inside the method) stands in between, jumps to
//!   `__rt_report_debuginfo_exception` (`exceptions::uncaught_report`) instead of unwinding. So
//!   no dump state needs restoring: the process never resumes. `finally` blocks inside the method
//!   still run first, as in PHP, because their handler is newer than the guard.
//! - OWNERSHIP. The method returns an owned value. The fetch helper keeps that
//!   owner and hands back a borrowed container pointer (the boxed payload when the
//!   result was a `Mixed` cell); the renderers release the owner once the walk ends.
//! - RECURSION. The callers have already pushed the object on `_vd_seen`, so an
//!   object whose `__debugInfo()` contains itself renders `*RECURSION*` for the inner
//!   occurrence exactly like a plain property cycle (php-src guards the object first).
//! - Fetch protocol: input AArch64 x0 / x86_64 rdi = object, x1 / rsi = the `_debuginfo_pad`
//!   flag of this dump (1 = var_dump, 0 = print_r), installed for the call and restored after
//!   it so nested dumps do not leak theirs. Output: result register
//!   = owner (0 when nothing to release), arg1 register = borrowed container,
//!   arg2 register = tag (0 = class has no run-time `__debugInfo()`, 1 = null/empty
//!   result, 4 = indexed array, 5 = hash).

use crate::codegen_support::runtime::data::{DEBUGINFO_NULL_PREFIX, DEBUGINFO_NULL_SUFFIX};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Frame slots shared by the three helpers (byte offsets below the frame pointer).
const FRAME: usize = 64;
const OBJ: usize = 8;
const OWNER: usize = 16;
const CONT: usize = 24;
const TAG: usize = 32;
const BASE: usize = 40;
/// Fetch-helper slot that keeps the indirect `__debugInfo` entry across the activation setup.
const ENTRY: usize = 48;
/// Fetch-helper slot that keeps the caller's `_debuginfo_guard` while the call runs.
const PREV_GUARD: usize = 56;
/// Fetch-helper slot that keeps the caller's `_print_r_mode` while `__debugInfo()` runs.
const SAVE_MODE: usize = 64;
/// Fetch-helper slot that keeps the caller's `_print_r_off` while `__debugInfo()` runs.
const SAVE_LEN: usize = 72;
/// Fetch-helper slot that keeps the heap copy of the caller's print_r capture bytes.
const SAVE_PTR: usize = 80;
/// Fetch-helper slot that first holds this dump's `_debuginfo_pad` flag, then the caller's value
/// while `__debugInfo()` runs (a nested dump inside the method rewrites the global).
const PREV_PAD: usize = 88;
/// Fetch-helper slot that keeps the caller's `_debuginfo_call_line` while `__debugInfo()` runs.
const PREV_LINE: usize = 96;
/// Frame size of the fetch helper: the shared slots, `ENTRY`, `PREV_GUARD`, `PREV_PAD`,
/// `PREV_LINE` and the capture save slots.
const FETCH_FRAME: usize = 112;

/// Tag reported when the class has no run-time `__debugInfo()`.
const TAG_NONE: usize = 0;
/// Tag reported when `__debugInfo()` returned null (an empty array in PHP).
const TAG_EMPTY: usize = 1;

/// Branches to `label` when the integer result register equals `value`.
fn branch_if_result_eq(emitter: &mut Emitter, value: usize, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{value}"));                  // compare the classified tag or heap kind
            emitter.instruction(&format!("b.eq {label}"));                      // dispatch to the matching local label
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {value}"));                  // compare the classified tag or heap kind
            emitter.instruction(&format!("je {label}"));                        // dispatch to the matching local label
        }
    }
}

/// Stores an immediate into a frame slot through the secondary scratch register.
fn store_imm_slot(emitter: &mut Emitter, value: i64, offset: usize) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, value);
    abi::store_at_offset(emitter, scratch, offset);
}

/// `__rt_obj_debuginfo_fetch`: call `__debugInfo()` and classify its result.
///
/// Input: AArch64 x0 / x86_64 rdi = object pointer (non-null), x1 / rsi = this dump's
/// `_debuginfo_pad` flag. Output: result
/// register = owner to release (or 0), arg1 register = borrowed container pointer,
/// arg2 register = tag (see the module preamble). Emits PHP's deprecation for a null
/// result and exits with the `__debuginfo() must return an array` fatal for any other
/// non-array result.
pub(crate) fn emit_obj_debuginfo_fetch(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let cont_reg = abi::int_arg_reg_name(emitter.target, 1);
    let tag_reg = abi::int_arg_reg_name(emitter.target, 2);
    let scratch = abi::secondary_scratch_reg(emitter);

    emitter.blank();
    emitter.comment("--- runtime: obj_debuginfo_fetch ---");
    emitter.label_global("__rt_obj_debuginfo_fetch");
    abi::emit_frame_prologue(emitter, FETCH_FRAME);
    abi::store_at_offset(emitter, arg0, OBJ);
    abi::store_at_offset(emitter, cont_reg, PREV_PAD);                          // park this dump's pad flag until the guard push swaps it in
    store_imm_slot(emitter, 0, OWNER);
    store_imm_slot(emitter, 0, CONT);
    store_imm_slot(emitter, TAG_NONE as i64, TAG);

    // -- resolve the class's __debugInfo entry; an unknown id or a zero entry means "none" --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0]");                                // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("ldr x10, [x10]");                              // load the number of registered class ids
            emitter.instruction("cmp x9, x10");                                 // is the class id within the metadata tables?
            emitter.instruction("b.hs __rt_obj_debuginfo_none");                // incomplete or stale ids have no __debugInfo
            abi::emit_symbol_address(emitter, "x10", "_class_debuginfo_ptrs");  // resolve the per-class __debugInfo entry table
            emitter.instruction("ldr x10, [x10, x9, lsl #3]");                  // x10 = this class's __debugInfo symbol (or 0)
            emitter.instruction("cbz x10, __rt_obj_debuginfo_none");            // the class dumps through its descriptor rows
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, QWORD PTR [rdi]");                     // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "r10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("cmp r9, QWORD PTR [r10]");                     // is the class id within the metadata tables?
            emitter.instruction("jae __rt_obj_debuginfo_none");                 // incomplete or stale ids have no __debugInfo
            abi::emit_symbol_address(emitter, "r10", "_class_debuginfo_ptrs");  // resolve the per-class __debugInfo entry table
            emitter.instruction("mov r10, QWORD PTR [r10 + r9 * 8]");           // r10 = this class's __debugInfo symbol (or 0)
            emitter.instruction("test r10, r10");                               // does the class define a run-time __debugInfo?
            emitter.instruction("jz __rt_obj_debuginfo_none");                  // the class dumps through its descriptor rows
        }
    }
    let entry_reg = nested_entry_reg(emitter);
    abi::store_at_offset(emitter, entry_reg, ENTRY);
    emit_guard_push(emitter);
    emit_print_r_capture_save(emitter);
    abi::load_at_offset(emitter, arg0, OBJ);
    abi::load_at_offset(emitter, entry_reg, ENTRY);
    abi::emit_call_reg(emitter, entry_reg);                                     // invoke __debugInfo() with $this borrowed in the first argument
    abi::store_at_offset(emitter, result, OWNER);
    // -- a method that returns nothing leaves no value in the result register: read PHP's null --
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x9", OBJ);
            emitter.instruction("ldr x9, [x9]");                                // reload the runtime class id from the borrowed receiver
            abi::emit_symbol_address(emitter, "x10", "_class_debuginfo_void");  // resolve the returns-nothing flags
            emitter.instruction("ldrb w10, [x10, x9]");                         // w10 = 1 when this class's __debugInfo() returns nothing
            emitter.instruction("cbz w10, __rt_obj_debuginfo_classify");        // a value-returning method keeps its result
            emitter.instruction("mov x0, #0");                                  // no value: PHP sees null
            abi::store_at_offset(emitter, "x0", OWNER);
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r9", OBJ);
            emitter.instruction("mov r9, QWORD PTR [r9]");                      // reload the runtime class id from the borrowed receiver
            abi::emit_symbol_address(emitter, "r10", "_class_debuginfo_void");  // resolve the returns-nothing flags
            emitter.instruction("movzx r10d, BYTE PTR [r10 + r9]");             // r10 = 1 when this class's __debugInfo() returns nothing
            emitter.instruction("test r10d, r10d");                             // does the method return a value?
            emitter.instruction("jz __rt_obj_debuginfo_classify");              // a value-returning method keeps its result
            emitter.instruction("xor eax, eax");                                // no value: PHP sees null
            abi::store_at_offset(emitter, "rax", OWNER);
        }
    }
    emitter.label("__rt_obj_debuginfo_classify");

    // -- classify the owned result: null, concrete array, hash or boxed Mixed --
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, result, scratch, "__rt_obj_debuginfo_null_raw",
    );
    abi::emit_call_label(emitter, "__rt_heap_kind");
    branch_if_result_eq(emitter, 5, "__rt_obj_debuginfo_boxed");
    branch_if_result_eq(emitter, 2, "__rt_obj_debuginfo_indexed");
    branch_if_result_eq(emitter, 3, "__rt_obj_debuginfo_hash");
    abi::emit_jump(emitter, "__rt_obj_debuginfo_invalid");

    emitter.label("__rt_obj_debuginfo_indexed");
    abi::load_at_offset(emitter, result, OWNER);
    abi::store_at_offset(emitter, result, CONT);
    store_imm_slot(emitter, 4, TAG);
    abi::emit_jump(emitter, "__rt_obj_debuginfo_done");

    emitter.label("__rt_obj_debuginfo_hash");
    abi::load_at_offset(emitter, result, OWNER);
    abi::store_at_offset(emitter, result, CONT);
    store_imm_slot(emitter, 5, TAG);
    abi::emit_jump(emitter, "__rt_obj_debuginfo_done");

    emitter.label("__rt_obj_debuginfo_boxed");
    abi::load_at_offset(emitter, result, OWNER);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // result = inner tag, payload in the unbox low register
    let low = match emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
    abi::store_at_offset(emitter, low, CONT);
    branch_if_result_eq(emitter, 4, "__rt_obj_debuginfo_boxed_indexed");
    branch_if_result_eq(emitter, 5, "__rt_obj_debuginfo_boxed_hash");
    branch_if_result_eq(emitter, 8, "__rt_obj_debuginfo_null_boxed");
    abi::emit_jump(emitter, "__rt_obj_debuginfo_invalid");

    emitter.label("__rt_obj_debuginfo_boxed_indexed");
    store_imm_slot(emitter, 4, TAG);
    abi::emit_jump(emitter, "__rt_obj_debuginfo_boxed_check");
    emitter.label("__rt_obj_debuginfo_boxed_hash");
    store_imm_slot(emitter, 5, TAG);
    emitter.label("__rt_obj_debuginfo_boxed_check");
    abi::load_at_offset(emitter, result, CONT);
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, result, scratch, "__rt_obj_debuginfo_null_boxed",
    );
    abi::emit_jump(emitter, "__rt_obj_debuginfo_done");

    // -- null results are an empty array; a raw null owns nothing, a boxed one still owns its cell --
    emitter.label("__rt_obj_debuginfo_null_raw");
    store_imm_slot(emitter, 0, OWNER);
    emitter.label("__rt_obj_debuginfo_null_boxed");
    store_imm_slot(emitter, 0, CONT);
    store_imm_slot(emitter, TAG_EMPTY as i64, TAG);
    emit_null_deprecation(emitter);

    // -- normal return: the call finished, so hand the guard back to the enclosing call --
    emitter.label("__rt_obj_debuginfo_done");
    emit_print_r_capture_restore(emitter);
    emit_guard_pop(emitter);
    emitter.label("__rt_obj_debuginfo_none");
    abi::load_at_offset(emitter, tag_reg, TAG);
    abi::load_at_offset(emitter, cont_reg, CONT);
    abi::load_at_offset(emitter, result, OWNER);
    abi::emit_frame_restore(emitter, FETCH_FRAME);
    abi::emit_return(emitter);

    // -- a non-array result: PHP's uncatchable `__debuginfo() must return an array` fatal --
    emitter.label("__rt_obj_debuginfo_invalid");
    emit_fatal_exit(emitter);

}

/// Arms `_debuginfo_guard` for the `__debugInfo()` call.
///
/// The guard is `_exc_handler_top | 1` (never zero), so `__rt_throw_current` can tell an
/// exception that escapes the method from one caught by a `try` inside it. The caller's guard
/// (a dump nested in another `__debugInfo()`) is kept in `PREV_GUARD`. The same nesting rule
/// applies to `_debuginfo_pad`: this dump's flag (parked in `PREV_PAD`) is swapped in and the
/// caller's value kept in `PREV_PAD`, so an enclosing `__debugInfo()` that throws or returns a
/// non-array after a nested dump still reports with its own var_dump indent; `_debuginfo_call_line`
/// is kept in `PREV_LINE` for the same reason (the fatal names the outer dump call).
fn emit_guard_push(emitter: &mut Emitter) {
    let scratch = abi::secondary_scratch_reg(emitter);
    let result = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, "_debuginfo_pad", 0);
    abi::load_at_offset(emitter, scratch, PREV_PAD);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_debuginfo_pad", 0);
    abi::store_at_offset(emitter, result, PREV_PAD);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_debuginfo_guard", 0);
    abi::store_at_offset(emitter, scratch, PREV_GUARD);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_exc_handler_top", 0);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("orr {scratch}, {scratch}, #1")), // tag the top so an empty handler stack is still non-zero
        Arch::X86_64 => emitter.instruction(&format!("or {scratch}, 1")),       // tag the top so an empty handler stack is still non-zero
    }
    abi::emit_store_reg_to_symbol(emitter, scratch, "_debuginfo_guard", 0);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_debuginfo_call_line", 0);
    abi::store_at_offset(emitter, scratch, PREV_LINE);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_php_diagnostic_line", 0);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_debuginfo_call_line", 0);   // the dump call's line: the user method's own calls overwrite the diagnostic line before a fatal can name it
}

/// Moves an in-flight print_r capture out of the way while `__debugInfo()` runs.
///
/// A deferred echo `print_r` (mode 2) or a return-mode `print_r` (mode 1) that reaches this helper
/// has its text so far in the shared capture buffer. User code run by the method must not see that
/// capture: an `echo` goes straight to the output like in PHP, and a nested `print_r($x, true)` may
/// reuse the buffer from offset 0. So the bytes are copied to a heap block, the capture is switched
/// off, and `emit_print_r_capture_restore` puts everything back after the call.
fn emit_print_r_capture_save(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let a0 = abi::int_arg_reg_name(emitter.target, 0);
    let a1 = abi::int_arg_reg_name(emitter.target, 1);
    let a2 = abi::int_arg_reg_name(emitter.target, 2);
    let malloc = emitter.target.extern_symbol("malloc");
    let memcpy = emitter.target.extern_symbol("memcpy");
    store_imm_slot(emitter, 0, SAVE_MODE);
    store_imm_slot(emitter, 0, SAVE_LEN);
    store_imm_slot(emitter, 0, SAVE_PTR);
    abi::emit_load_symbol_to_reg(emitter, result, "_print_r_mode", 0);
    abi::store_at_offset(emitter, result, SAVE_MODE);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_obj_debuginfo_pr_saved");
    abi::emit_load_symbol_to_reg(emitter, result, "_print_r_off", 0);
    abi::store_at_offset(emitter, result, SAVE_LEN);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_obj_debuginfo_pr_reset");
    abi::emit_reg_move(emitter, a0, result);
    abi::emit_call_label(emitter, &malloc);
    // A failed park must not memcpy through NULL: report the shared allocation fatal instead.
    // The fatal is reached through an unconditional jump, which stays in range across atoms.
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_obj_debuginfo_pr_alloc_ok");
    abi::emit_jump(emitter, "__rt_alloc_overflow");
    emitter.label("__rt_obj_debuginfo_pr_alloc_ok");
    abi::store_at_offset(emitter, result, SAVE_PTR);
    abi::emit_call_label(emitter, "__rt_pr_base");
    abi::emit_reg_move(emitter, a1, result);
    abi::load_at_offset(emitter, a0, SAVE_PTR);
    abi::load_at_offset(emitter, a2, SAVE_LEN);
    abi::emit_call_label(emitter, &memcpy);
    emitter.label("__rt_obj_debuginfo_pr_reset");
    abi::emit_store_zero_to_symbol(emitter, "_print_r_off", 0);
    abi::emit_store_zero_to_symbol(emitter, "_print_r_mode", 0);
    emitter.label("__rt_obj_debuginfo_pr_saved");
}

/// Puts back the print_r capture parked by `emit_print_r_capture_save` after a normal return.
fn emit_print_r_capture_restore(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let a0 = abi::int_arg_reg_name(emitter.target, 0);
    let a1 = abi::int_arg_reg_name(emitter.target, 1);
    let a2 = abi::int_arg_reg_name(emitter.target, 2);
    let free = emitter.target.extern_symbol("free");
    let memcpy = emitter.target.extern_symbol("memcpy");
    abi::load_at_offset(emitter, result, SAVE_MODE);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_obj_debuginfo_pr_done");
    abi::load_at_offset(emitter, result, SAVE_PTR);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_obj_debuginfo_pr_nocopy");
    abi::load_at_offset(emitter, a0, SAVE_LEN);
    abi::emit_call_label(emitter, "__rt_pr_reserve");
    abi::emit_call_label(emitter, "__rt_pr_base");
    abi::emit_reg_move(emitter, a0, result);
    abi::load_at_offset(emitter, a1, SAVE_PTR);
    abi::load_at_offset(emitter, a2, SAVE_LEN);
    abi::emit_call_label(emitter, &memcpy);
    abi::load_at_offset(emitter, a0, SAVE_PTR);
    abi::emit_call_label(emitter, &free);
    emitter.label("__rt_obj_debuginfo_pr_nocopy");
    abi::load_at_offset(emitter, result, SAVE_LEN);
    abi::emit_store_reg_to_symbol(emitter, result, "_print_r_off", 0);
    abi::load_at_offset(emitter, result, SAVE_MODE);
    abi::emit_store_reg_to_symbol(emitter, result, "_print_r_mode", 0);
    emitter.label("__rt_obj_debuginfo_pr_done");
}

/// Restores the caller's `_debuginfo_guard`, `_debuginfo_pad` and `_debuginfo_call_line` after a
/// normal return from `__debugInfo()`.
fn emit_guard_pop(emitter: &mut Emitter) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, scratch, PREV_GUARD);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_debuginfo_guard", 0);
    abi::load_at_offset(emitter, scratch, PREV_PAD);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_debuginfo_pad", 0);
    abi::load_at_offset(emitter, scratch, PREV_LINE);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_debuginfo_call_line", 0);
}

/// Emits PHP's `Returning null from C::__debugInfo() is deprecated` diagnostic.
///
/// The class name comes from the dense `_class_name_entries` table (the one
/// `get_class()` reads); an out-of-range id prints an empty name.
fn emit_null_deprecation(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", "_debuginfo_null_prefix");
            emitter.instruction(&format!("mov x2, #{}", DEBUGINFO_NULL_PREFIX.len())); // prefix byte length
            abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
            abi::load_at_offset(emitter, "x11", OBJ);
            emitter.instruction("ldr x11, [x11]");                              // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "x10", "_class_name_count");      // resolve the class-name table extent
            emitter.instruction("ldr x10, [x10]");                              // load the number of named class ids
            emitter.instruction("cmp x11, x10");                                // is the class id within the name table?
            emitter.instruction("b.hs __rt_obj_debuginfo_anon");                // an unknown class id prints no name
            abi::emit_symbol_address(emitter, "x12", "_class_name_entries");    // resolve the class-name entry table
            emitter.instruction("add x12, x12, x11, lsl #4");                   // each entry is a 16-byte (ptr, len) pair
            emitter.instruction("ldr x1, [x12]");                               // load the class-name pointer
            emitter.instruction("ldr x2, [x12, #8]");                           // load the class-name length
            emitter.instruction("b __rt_obj_debuginfo_named");                  // emit the resolved name
            emitter.label("__rt_obj_debuginfo_anon");
            abi::emit_symbol_address(emitter, "x1", "_class_name_missing");     // fall back to the empty class-name slot
            emitter.instruction("mov x2, #0");                                  // a zero-length fragment adds nothing
            emitter.label("__rt_obj_debuginfo_named");
            abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
            abi::emit_symbol_address(emitter, "x1", "_debuginfo_null_suffix");
            emitter.instruction(&format!("mov x2, #{}", DEBUGINFO_NULL_SUFFIX.len())); // suffix byte length
            abi::emit_call_label(emitter, "__rt_diag_warning");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rdi", "_debuginfo_null_prefix");
            emitter.instruction(&format!("mov esi, {}", DEBUGINFO_NULL_PREFIX.len())); // prefix byte length
            abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
            abi::load_at_offset(emitter, "r11", OBJ);
            emitter.instruction("mov r11, QWORD PTR [r11]");                    // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "r10", "_class_name_count");      // resolve the class-name table extent
            emitter.instruction("cmp r11, QWORD PTR [r10]");                    // is the class id within the name table?
            emitter.instruction("jae __rt_obj_debuginfo_anon");                 // an unknown class id prints no name
            abi::emit_symbol_address(emitter, "r10", "_class_name_entries");    // resolve the class-name entry table
            emitter.instruction("shl r11, 4");                                  // each entry is a 16-byte (ptr, len) pair
            emitter.instruction("add r10, r11");                                // advance to this class's entry
            emitter.instruction("mov rdi, QWORD PTR [r10]");                    // load the class-name pointer
            emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                // load the class-name length
            emitter.instruction("jmp __rt_obj_debuginfo_named");                // emit the resolved name
            emitter.label("__rt_obj_debuginfo_anon");
            abi::emit_symbol_address(emitter, "rdi", "_class_name_missing");    // fall back to the empty class-name slot
            emitter.instruction("xor esi, esi");                                // a zero-length fragment adds nothing
            emitter.label("__rt_obj_debuginfo_named");
            abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
            abi::emit_symbol_address(emitter, "rdi", "_debuginfo_null_suffix");
            emitter.instruction(&format!("mov esi, {}", DEBUGINFO_NULL_SUFFIX.len())); // suffix byte length
            abi::emit_call_label(emitter, "__rt_diag_warning");
        }
    }
}

/// Writes PHP's `__debuginfo() must return an array` fatal and exits with status 255 (never returns).
///
/// The indent a nested `var_dump` value has already earned is written first; the shared
/// `__rt_debuginfo_fatal` then drains the output buffers and prints the text with its location.
fn emit_fatal_exit(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(emitter, "__rt_debuginfo_pre_report");
            emitter.instruction("b __rt_debuginfo_fatal");                      // flush, print the fatal with its location, exit 255
        }
        Arch::X86_64 => {
            emitter.instruction("and rsp, -16");                                // realign the stack for the C-ABI flush; this path never returns
            abi::emit_call_label(emitter, "__rt_debuginfo_pre_report");
            emitter.instruction("jmp __rt_debuginfo_fatal");                    // flush, print the fatal with its location, exit 255
        }
    }
}

/// Register that holds the indirect `__debugInfo` entry address across the call.
fn nested_entry_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    }
}

/// Shared prologue of the two renderers: stores the arguments, runs the fetch
/// helper and spills its owner/container/tag triple; branches to `none_label`
/// (with the result register zeroed) when the class has no run-time `__debugInfo()`.
fn emit_render_prologue(emitter: &mut Emitter, with_base: bool, none_label: &str) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let cont_reg = arg1;
    let tag_reg = abi::int_arg_reg_name(emitter.target, 2);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, arg0, OBJ);
    if with_base {
        abi::store_at_offset(emitter, arg1, BASE);
    }
    abi::emit_load_int_immediate(emitter, arg1, i64::from(!with_base));         // the fetch installs this dump's `_debuginfo_pad` flag
    abi::load_at_offset(emitter, arg0, OBJ);
    abi::emit_call_label(emitter, "__rt_obj_debuginfo_fetch");
    abi::store_at_offset(emitter, result, OWNER);
    abi::store_at_offset(emitter, cont_reg, CONT);
    abi::store_at_offset(emitter, tag_reg, TAG);
    abi::emit_reg_move(emitter, result, tag_reg);
    abi::emit_branch_if_int_result_zero(emitter, none_label);
}

/// Releases the owner kept by the fetch helper and returns 1 ("handled").
fn emit_render_epilogue(emitter: &mut Emitter, none_label: &str) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, result, OWNER);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emitter.label(none_label);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Tells the hash walker about to run that its string keys are an object's property keys, which
/// var_dump and print_r show unmangled, as php-src does for a `__debugInfo()` result.
fn emit_mark_property_keys(emitter: &mut Emitter) {
    let (address, one) = match emitter.target.arch {
        Arch::AArch64 => ("x9", "x10"),
        Arch::X86_64 => ("r10", "r11"),
    };
    abi::emit_symbol_address(emitter, address, "_dump_unmangle");
    abi::emit_load_int_immediate(emitter, one, 1);
    abi::emit_store_to_address(emitter, one, address, 0);
}

/// `__rt_vd_debuginfo_object`: render an object through its run-time `__debugInfo()`.
///
/// Input: AArch64 x0 / x86_64 rdi = object pointer, already on the `_vd_seen` stack.
/// Output: result register = 1 when the whole `object(C)#N (n) { ... }` block was
/// written, 0 when the class has no run-time `__debugInfo()` (nothing written).
pub(crate) fn emit_vd_debuginfo_object(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);

    emitter.blank();
    emitter.comment("--- runtime: vd_debuginfo_object ---");
    emitter.label_global("__rt_vd_debuginfo_object");
    emit_render_prologue(emitter, false, "__rt_vd_debuginfo_none");

    // -- header: the entry count of the returned array replaces the property count --
    // The tag test uses the result register, which aliases the first argument on AArch64,
    // so the object pointer is loaded only after the branch.
    abi::load_at_offset(emitter, result, TAG);
    branch_if_result_eq(emitter, TAG_EMPTY, "__rt_vd_debuginfo_empty");
    abi::load_at_offset(emitter, arg1, CONT);
    abi::emit_load_from_address(emitter, arg1, arg1, 0);                        // entry count at offset 0 of an indexed array or hash
    abi::emit_jump(emitter, "__rt_vd_debuginfo_header");
    emitter.label("__rt_vd_debuginfo_empty");
    abi::emit_load_int_immediate(emitter, arg1, 0);
    emitter.label("__rt_vd_debuginfo_header");
    abi::load_at_offset(emitter, arg0, OBJ);
    abi::emit_call_label(emitter, "__rt_var_dump_open_object_n");
    abi::emit_call_label(emitter, "__rt_vd_indent_push");

    // -- body: the array's entries at one level deeper, then the closing brace --
    abi::load_at_offset(emitter, result, TAG);
    branch_if_result_eq(emitter, 4, "__rt_vd_debuginfo_indexed");
    branch_if_result_eq(emitter, 5, "__rt_vd_debuginfo_hash");
    abi::emit_jump(emitter, "__rt_vd_debuginfo_close");
    emitter.label("__rt_vd_debuginfo_indexed");
    abi::load_at_offset(emitter, arg0, CONT);
    abi::emit_call_label(emitter, "__rt_var_dump_indexed");
    abi::emit_jump(emitter, "__rt_vd_debuginfo_close");
    emitter.label("__rt_vd_debuginfo_hash");
    emit_mark_property_keys(emitter);
    abi::load_at_offset(emitter, arg0, CONT);
    abi::emit_call_label(emitter, "__rt_var_dump_hash");
    emitter.label("__rt_vd_debuginfo_close");
    abi::emit_call_label(emitter, "__rt_vd_indent_pop");
    abi::emit_call_label(emitter, "__rt_var_dump_close_container");
    emit_render_epilogue(emitter, "__rt_vd_debuginfo_none");
}

/// `__rt_pr_debuginfo_object`: render an object body through its run-time `__debugInfo()`.
///
/// Input: AArch64 x0 / x86_64 rdi = object pointer (already on the `_vd_seen`
/// stack, `C Object\n` header written), arg1 = paren base indent. Output: result
/// register = 1 when the `<base>(\n ... <base>)\n` body was written, 0 when the
/// class has no run-time `__debugInfo()` (nothing written).
pub(crate) fn emit_pr_debuginfo_object(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);

    emitter.blank();
    emitter.comment("--- runtime: pr_debuginfo_object ---");
    emitter.label_global("__rt_pr_debuginfo_object");
    emit_render_prologue(emitter, true, "__rt_pr_debuginfo_none");

    abi::load_at_offset(emitter, result, TAG);
    branch_if_result_eq(emitter, 4, "__rt_pr_debuginfo_indexed");
    branch_if_result_eq(emitter, 5, "__rt_pr_debuginfo_hash");
    // -- a null result is an empty array: just the parentheses --
    abi::load_at_offset(emitter, arg0, BASE);
    abi::emit_call_label(emitter, "__rt_print_r_open");
    abi::load_at_offset(emitter, arg0, BASE);
    abi::emit_call_label(emitter, "__rt_print_r_close");
    abi::emit_jump(emitter, "__rt_pr_debuginfo_release");
    emitter.label("__rt_pr_debuginfo_indexed");
    abi::load_at_offset(emitter, arg0, CONT);
    abi::load_at_offset(emitter, arg1, BASE);
    abi::emit_call_label(emitter, "__rt_print_r_indexed");
    abi::emit_jump(emitter, "__rt_pr_debuginfo_release");
    emitter.label("__rt_pr_debuginfo_hash");
    emit_mark_property_keys(emitter);
    abi::load_at_offset(emitter, arg0, CONT);
    abi::load_at_offset(emitter, arg1, BASE);
    abi::emit_call_label(emitter, "__rt_print_r_hash");
    emitter.label("__rt_pr_debuginfo_release");
    emit_render_epilogue(emitter, "__rt_pr_debuginfo_none");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target emits the fetch helper and both renderers with the same contract:
    /// table probe, throw guard, indirect call, heap-kind classification, a fatal exit on a
    /// bad result, the null deprecation and a release of the owned return after the walk.
    #[test]
    fn debug_info_helpers_emit_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_obj_debuginfo_fetch(&mut emitter);
            emit_vd_debuginfo_object(&mut emitter);
            emit_pr_debuginfo_object(&mut emitter);
            let asm = emitter.output();
            let (fetch, renderers) = asm.split_once("__rt_vd_debuginfo_object:").unwrap();
            assert!(fetch.contains("_class_debuginfo_ptrs"), "{name}");
            assert!(fetch.find("_class_debuginfo_ptrs").unwrap() < fetch.find("__rt_heap_kind").unwrap(), "{name}");
            assert!(fetch.contains("__rt_mixed_unbox"), "{name}");
            assert!(fetch.contains("__rt_debuginfo_fatal"), "{name}");
            assert!(!fetch.contains("__rt_throw_current"), "{name}");
            assert!(fetch.contains("_debuginfo_null_prefix") && fetch.contains("_debuginfo_null_suffix"), "{name}");
            assert!(fetch.contains("_debuginfo_guard") && fetch.contains("_exc_handler_top"), "{name}");
            assert!(!fetch.contains("_exc_call_frame_top"), "{name}");
            assert!(fetch.contains("_print_r_mode") && fetch.contains("__rt_pr_base"), "{name}");
            assert!(renderers.contains("__rt_var_dump_open_object_n"), "{name}");
            assert!(renderers.contains("__rt_print_r_hash"), "{name}");
            assert_eq!(renderers.matches("__rt_decref_any").count(), 2, "{name}");
        }
    }

    /// The print_r capture park checks malloc's result on every target: a NULL block reaches the
    /// allocation fatal before any store or memcpy uses it.
    #[test]
    fn print_r_capture_save_null_checks_malloc() {
        for name in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_print_r_capture_save(&mut emitter);
            let asm = emitter.output();
            let malloc = asm.find("malloc").unwrap_or_else(|| panic!("{name}: no malloc"));
            let fatal = asm.find("__rt_alloc_overflow").unwrap_or_else(|| panic!("{name}: no null check"));
            let ok = asm.find("__rt_obj_debuginfo_pr_alloc_ok:").unwrap_or_else(|| panic!("{name}: no ok label"));
            let copy = asm.find("memcpy").unwrap_or_else(|| panic!("{name}: no memcpy"));
            assert!(malloc < fatal && fatal < ok && ok < copy, "{name}: {asm}");
        }
    }
}
