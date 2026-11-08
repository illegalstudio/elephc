//! Purpose:
//! Emits the search-window pieces of `__rt_preg_match_all_capture`: the PHP `$offset`
//! start position, PHP's trimming of trailing unmatched groups in a SET_ORDER row, and
//! the private single-match mode `preg_match($pattern, $subject, $m, $flags, $offset)`
//! lowers to.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system::preg_match_all_capture`.
//!
//! Key details:
//! - A negative offset counts from the end of the subject and clamps at `0`; an offset
//!   past the end is PHP's internal error. It returns the `-2` sentinel: PHP `false`, but
//!   unlike a compile failure (`-1`) PHP still writes the empty array to `$matches`.
//! - Without `PREG_UNMATCHED_AS_NULL`, a SET_ORDER row (and therefore the single
//!   `preg_match()` row) stops at the highest participating group, like php-src.
//! - `PREG_CAPTURE_FIRST_ONLY` is private to elephc: the helper stops after the first
//!   match and returns that SET_ORDER row itself instead of a list of rows.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Private helper flag: stop after one match and return its row (the `preg_match()` form).
pub(super) const PREG_CAPTURE_FIRST_ONLY: i64 = 1 << 29;
const PREG_UNMATCHED_AS_NULL: i64 = 512;

/// Stack slots the window helpers read and write inside the capture frame.
pub(super) struct CaptureWindowSlots {
    pub(super) handle_off: usize,
    pub(super) regmatches_ptr_off: usize,
    pub(super) nmatch_off: usize,
    pub(super) preg_flags_off: usize,
    pub(super) subject_len_off: usize,
    pub(super) subject_cstr_off: usize,
    pub(super) current_cstr_off: usize,
    pub(super) start_offset_off: usize,
    pub(super) row_limit_off: usize,
}

/// Normalizes the PHP `$offset` and starts the search cursor there.
///
/// Runs after the subject length is known and before any capture matrix is allocated,
/// so an out-of-range offset only has to release the compiled handle and pair vector.
pub(super) fn emit_start_offset(emitter: &mut Emitter, slots: &CaptureWindowSlots) {
    match emitter.target.arch {
        Arch::AArch64 => emit_start_offset_arm64(emitter, slots),
        Arch::X86_64 => emit_start_offset_x86_64(emitter, slots),
    }
}

/// Emits the ARM64 `$offset` normalization and the out-of-range failure exit.
fn emit_start_offset_arm64(emitter: &mut Emitter, s: &CaptureWindowSlots) {
    // -- resolve a negative offset from the end and reject one past the end --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", s.start_offset_off));     // reload the PHP start offset
    emitter.instruction(&format!("ldr x10, [sp, #{}]", s.subject_len_off));     // reload the subject length
    emitter.instruction("cmp x9, #0");                                          // does the offset count from the end?
    emitter.instruction("b.ge __rt_pma_cap_offset_forward");                    // a non-negative offset is used as given
    emitter.instruction("add x9, x9, x10");                                     // a negative offset counts back from the end
    emitter.instruction("cmp x9, #0");                                          // did it reach before the subject start?
    emitter.instruction("b.ge __rt_pma_cap_offset_ready");                      // keep an in-range resolved offset
    emitter.instruction("mov x9, #0");                                          // PHP clamps a too-negative offset to the start
    emitter.instruction("b __rt_pma_cap_offset_ready");                         // publish the clamped offset
    emitter.label("__rt_pma_cap_offset_forward");
    emitter.instruction("cmp x9, x10");                                         // is the offset past the subject end?
    emitter.instruction("b.gt __rt_pma_cap_offset_bad");                        // PHP reports an internal error and returns false
    emitter.label("__rt_pma_cap_offset_ready");
    emitter.instruction(&format!("str x9, [sp, #{}]", s.start_offset_off));     // save the resolved start offset
    emitter.instruction(&format!("ldr x10, [sp, #{}]", s.subject_cstr_off));    // reload the subject C-string base
    emitter.instruction("add x10, x10, x9");                                    // first search position = base + offset
    emitter.instruction(&format!("str x10, [sp, #{}]", s.current_cstr_off));    // start the search cursor at the offset
    emitter.label("__rt_pma_cap_offset_done");
}

/// Emits the x86_64 `$offset` normalization and the out-of-range failure exit.
fn emit_start_offset_x86_64(emitter: &mut Emitter, s: &CaptureWindowSlots) {
    // -- resolve a negative offset from the end and reject one past the end --
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", s.start_offset_off)); // reload the PHP start offset
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", s.subject_len_off)); // reload the subject length
    emitter.instruction("cmp r9, 0");                                           // does the offset count from the end?
    emitter.instruction("jge __rt_pma_cap_offset_forward_linux_x86_64");        // a non-negative offset is used as given
    emitter.instruction("add r9, r10");                                         // a negative offset counts back from the end
    emitter.instruction("jns __rt_pma_cap_offset_ready_linux_x86_64");          // keep an in-range resolved offset
    emitter.instruction("xor r9d, r9d");                                        // PHP clamps a too-negative offset to the start
    emitter.instruction("jmp __rt_pma_cap_offset_ready_linux_x86_64");          // publish the clamped offset
    emitter.label("__rt_pma_cap_offset_forward_linux_x86_64");
    emitter.instruction("cmp r9, r10");                                         // is the offset past the subject end?
    emitter.instruction("jg __rt_pma_cap_offset_bad_linux_x86_64");             // PHP reports an internal error and returns false
    emitter.label("__rt_pma_cap_offset_ready_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", s.start_offset_off)); // save the resolved start offset
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", s.subject_cstr_off)); // reload the subject C-string base
    emitter.instruction("add r10, r9");                                         // first search position = base + offset
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", s.current_cstr_off)); // start the search cursor at the offset
    emitter.label("__rt_pma_cap_offset_done_linux_x86_64");
}

/// Emits the out-of-range offset exit: release the regex, return `-2` and an empty array.
///
/// Placed out of line by the capture helper; it ends by jumping to the helper's epilogue.
pub(super) fn emit_bad_offset_exit(emitter: &mut Emitter, s: &CaptureWindowSlots) {
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- out-of-range offset: release the regex and return false with [] --
            emitter.label("__rt_pma_cap_offset_bad");
            emitter.instruction(&format!("ldr x0, [sp, #{}]", s.handle_off));   // reload the compiled opaque handle
            emitter.bl_c("elephc_pcre2_v1_free");                               // release compiled regex resources
            emitter.instruction(&format!("ldr x0, [sp, #{}]", s.regmatches_ptr_off)); // reload the offset-pair buffer
            emitter.bl_c("free");                                               // release the pair vector
            emitter.instruction("mov x0, #0");                                  // empty-array capacity
            emitter.instruction("mov x1, #8");                                  // Mixed slots store boxed pointers
            emitter.instruction("bl __rt_array_new");                           // allocate the empty $matches array
            super::preg_match_all_capture::emit_stamp_indexed_array_mixed_arm64(emitter, "x0");
            emitter.instruction("mov x1, x0");                                  // return the empty array in x1
            emitter.instruction("mov x0, #-2");                                 // bad-offset sentinel: PHP false, [] written
            emitter.instruction("b __rt_pma_cap_ret");                          // share the common epilogue
        }
        Arch::X86_64 => {
            // -- out-of-range offset: release the regex and return false with [] --
            emitter.label("__rt_pma_cap_offset_bad_linux_x86_64");
            emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", s.handle_off)); // reload the compiled opaque handle
            emitter.bl_c("elephc_pcre2_v1_free");                               // release compiled regex resources
            emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", s.regmatches_ptr_off)); // reload the offset-pair buffer
            emitter.bl_c("free");                                               // release the pair vector
            emitter.instruction("xor edi, edi");                                // empty-array capacity
            emitter.instruction("mov esi, 8");                                  // Mixed slots store boxed pointers
            emitter.instruction("call __rt_array_new");                         // allocate the empty $matches array
            super::preg_match_all_capture::emit_stamp_indexed_array_mixed_x86_64(emitter, "rax");
            emitter.instruction("mov rdx, rax");                                // return the empty array in rdx
            emitter.instruction("mov rax, -2");                                 // bad-offset sentinel: PHP false, [] written
            emitter.instruction("jmp __rt_pma_cap_ret_linux_x86_64");           // share the common epilogue
        }
    }
}

/// Stores how many groups the current SET_ORDER row exposes.
///
/// `PREG_UNMATCHED_AS_NULL` keeps every compiled group; otherwise the row ends at the
/// highest group that participated, and always keeps the full match at index 0.
pub(super) fn emit_set_row_limit(emitter: &mut Emitter, slots: &CaptureWindowSlots) {
    match emitter.target.arch {
        Arch::AArch64 => emit_set_row_limit_arm64(emitter, slots),
        Arch::X86_64 => emit_set_row_limit_x86_64(emitter, slots),
    }
}

/// Emits the ARM64 scan for the highest participating group of this match.
fn emit_set_row_limit_arm64(emitter: &mut Emitter, s: &CaptureWindowSlots) {
    // -- find the exposed group count for this SET_ORDER row --
    emitter.instruction(&format!("ldr x13, [sp, #{}]", s.nmatch_off));          // default limit is every compiled group
    emitter.instruction(&format!("ldr x9, [sp, #{}]", s.preg_flags_off));       // reload PHP flags
    emitter.instruction(&format!("tst x9, #{}", PREG_UNMATCHED_AS_NULL));       // UNMATCHED_AS_NULL keeps trailing groups as null
    emitter.instruction("b.ne __rt_pma_cap_row_limit_done");                    // keep the full compiled width
    emitter.instruction(&format!("ldr x14, [sp, #{}]", s.regmatches_ptr_off));  // load the offset-pair buffer
    emitter.label("__rt_pma_cap_row_limit_scan");
    emitter.instruction("cmp x13, #1");                                         // the full match is always exposed
    emitter.instruction("b.le __rt_pma_cap_row_limit_done");                    // stop at the full-match slot
    emitter.instruction("sub x12, x13, #1");                                    // index of the last candidate group
    emitter.instruction("lsl x12, x12, #4");                                    // scale it by the 16-byte pair stride
    emitter.instruction("ldr x15, [x14, x12]");                                 // load that group's start offset
    emitter.instruction("cmp x15, #0");                                         // did the group participate?
    emitter.instruction("b.ge __rt_pma_cap_row_limit_done");                    // it ends the exposed row
    emitter.instruction("sub x13, x13, #1");                                    // drop the trailing unmatched group
    emitter.instruction("b __rt_pma_cap_row_limit_scan");                       // keep scanning backwards
    emitter.label("__rt_pma_cap_row_limit_done");
    emitter.instruction(&format!("str x13, [sp, #{}]", s.row_limit_off));       // publish the exposed group count
}

/// Emits the x86_64 scan for the highest participating group of this match.
fn emit_set_row_limit_x86_64(emitter: &mut Emitter, s: &CaptureWindowSlots) {
    // -- find the exposed group count for this SET_ORDER row --
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", s.nmatch_off)); // default limit is every compiled group
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", s.preg_flags_off)); // reload PHP flags
    emitter.instruction(&format!("test r10, {}", PREG_UNMATCHED_AS_NULL));      // UNMATCHED_AS_NULL keeps trailing groups as null
    emitter.instruction("jnz __rt_pma_cap_row_limit_done_linux_x86_64");        // keep the full compiled width
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", s.regmatches_ptr_off)); // load the offset-pair buffer
    emitter.label("__rt_pma_cap_row_limit_scan_linux_x86_64");
    emitter.instruction("cmp r9, 1");                                           // the full match is always exposed
    emitter.instruction("jle __rt_pma_cap_row_limit_done_linux_x86_64");        // stop at the full-match slot
    emitter.instruction("mov r11, r9");                                         // copy the current limit
    emitter.instruction("sub r11, 1");                                          // index of the last candidate group
    emitter.instruction("shl r11, 4");                                          // scale it by the 16-byte pair stride
    emitter.instruction("cmp QWORD PTR [r10 + r11], 0");                        // did the group participate?
    emitter.instruction("jge __rt_pma_cap_row_limit_done_linux_x86_64");        // it ends the exposed row
    emitter.instruction("sub r9, 1");                                           // drop the trailing unmatched group
    emitter.instruction("jmp __rt_pma_cap_row_limit_scan_linux_x86_64");        // keep scanning backwards
    emitter.label("__rt_pma_cap_row_limit_done_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", s.row_limit_off)); // publish the exposed group count
}

/// In single-match mode, makes the finished row the result and leaves the search loop.
///
/// Emitted right after the row is complete, before it would be boxed into the outer
/// list. Ordinary SET_ORDER calls fall through to that boxing unchanged.
pub(super) fn emit_first_only_row_result(
    emitter: &mut Emitter,
    preg_flags_off: usize,
    outer_off: usize,
    row_off: usize,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- preg_match(): the single row replaces the empty outer list --
            emitter.instruction(&format!("ldr x9, [sp, #{}]", preg_flags_off)); // reload the helper flags
            emitter.instruction(&format!("tst x9, #{}", PREG_CAPTURE_FIRST_ONLY)); // is this the single-match form?
            emitter.instruction("b.eq __rt_pma_cap_first_only_skip");           // ordinary SET_ORDER boxes the row into the list
            emitter.instruction(&format!("ldr x0, [sp, #{}]", outer_off));      // reload the still-empty outer list
            emitter.instruction("bl __rt_decref_array");                        // release the list the row replaces
            emitter.instruction(&format!("ldr x9, [sp, #{}]", row_off));        // reload the owned row
            emitter.instruction(&format!("str x9, [sp, #{}]", outer_off));      // return the row itself as $matches
            emitter.instruction("b __rt_pma_cap_done");                         // stop after the first match
            emitter.label("__rt_pma_cap_first_only_skip");
        }
        Arch::X86_64 => {
            // -- preg_match(): the single row replaces the empty outer list --
            emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", preg_flags_off)); // reload the helper flags
            emitter.instruction(&format!("test r9, {}", PREG_CAPTURE_FIRST_ONLY)); // is this the single-match form?
            emitter.instruction("jz __rt_pma_cap_first_only_skip_linux_x86_64"); // ordinary SET_ORDER boxes the row into the list
            emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", outer_off)); // reload the still-empty outer list
            emitter.instruction("call __rt_decref_array");                      // release the list the row replaces
            emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", row_off)); // reload the owned row
            emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", outer_off)); // return the row itself as $matches
            emitter.instruction("jmp __rt_pma_cap_done_linux_x86_64");          // stop after the first match
            emitter.label("__rt_pma_cap_first_only_skip_linux_x86_64");
        }
    }
}
