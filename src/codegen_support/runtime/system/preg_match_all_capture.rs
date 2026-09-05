//! Purpose:
//! Emits `__rt_preg_match_all_capture`, the PCRE2 helper that fills PHP's
//! `preg_match_all($pattern, $subject, &$matches, $flags)` capture matrix.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system::preg_match_all::emit_preg_match_all()`
//!
//! Key details:
//! - ABI matches the count-only helper plus flags: ARM64 `x1`/`x2` pattern,
//!   `x3`/`x4` subject, `x5` flags; x86_64 `rdi`/`rsi` pattern, `rdx`/`rcx`
//!   subject, `r8` flags. Count returns in `x0`/`rax`, the matches array in
//!   `x1`/`rdx`.
//! - Default / `PREG_PATTERN_ORDER` fills `$matches[group][match]`.
//!   `PREG_SET_ORDER` fills `$matches[match][group]`. `PREG_OFFSET_CAPTURE`
//!   boxes each cell as `[value, offset]`. `PREG_UNMATCHED_AS_NULL` uses Mixed
//!   null (offset `-1` with offset capture) instead of an empty string.
//! - Numbered groups only. Unsupported extra flag bits, or both order bits at
//!   once, return count `0` and `[]`. A successful compile with zero matches
//!   still materializes `nmatch` empty PATTERN_ORDER rows.

use crate::codegen_support::{emit::Emitter, platform::Arch};

const PREG_PATTERN_ORDER: i64 = 1;
const PREG_SET_ORDER: i64 = 2;
const PREG_OFFSET_CAPTURE: i64 = 256;
const PREG_UNMATCHED_AS_NULL: i64 = 512;
const PREG_MATCH_ALL_SUPPORTED_FLAGS: i64 =
    PREG_PATTERN_ORDER | PREG_SET_ORDER | PREG_OFFSET_CAPTURE | PREG_UNMATCHED_AS_NULL;

/// Emits `__rt_preg_match_all_capture` for the active target.
pub(crate) fn emit_preg_match_all_capture(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_match_all_capture_linux_x86_64(emitter);
        return;
    }
    emit_preg_match_all_capture_arm64(emitter);
}

/// Emits ARM64 `__rt_preg_match_all_capture`.
fn emit_preg_match_all_capture_arm64(emitter: &mut Emitter) {
    let handle_off = 0;
    let regmatches_ptr_off = handle_off + 8;
    let nmatch_off = regmatches_ptr_off + 8;
    let preg_flags_off = nmatch_off + 8;
    let regex_flags_off = preg_flags_off + 8;
    let pattern_cstr_off = regex_flags_off + 8;
    let subject_ptr_off = pattern_cstr_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let subject_cstr_off = subject_len_off + 8;
    let current_cstr_off = subject_cstr_off + 8;
    let match_count_off = current_cstr_off + 8;
    let group_rows_off = match_count_off + 8;
    let outer_off = group_rows_off + 8;
    let group_idx_off = outer_off + 8;
    let piece_ptr_off = group_idx_off + 8;
    let piece_len_off = piece_ptr_off + 8;
    let piece_offset_off = piece_len_off + 8;
    let pair_ptr_off = piece_offset_off + 8;
    let mixed_ptr_off = pair_ptr_off + 8;
    let row_off = mixed_ptr_off + 8;
    let stack_size = (row_off + 48 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_match_all_capture ---");
    emitter.label_global("__rt_preg_match_all_capture");

    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_match_all capture stack frame
    emitter.instruction(&format!("str x29, [sp, #{}]", save_off));              // save frame pointer in the large capture frame
    emitter.instruction(&format!("str x30, [sp, #{}]", save_off + 8));          // save return address in the large capture frame
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // set new frame pointer

    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // preserve the elephc subject pointer
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // preserve the elephc subject length
    emitter.instruction(&format!("str x5, [sp, #{}]", preg_flags_off));         // preserve PHP preg_match_all flags
    emitter.instruction(&format!("str xzr, [sp, #{}]", group_rows_off));        // no group-row table until PATTERN_ORDER setup
    emitter.instruction(&format!("str xzr, [sp, #{}]", regmatches_ptr_off));    // no offset-pair buffer until compile succeeds
    emitter.instruction(&format!("str xzr, [sp, #{}]", match_count_off));       // match count starts at zero

    emit_preg_match_all_capture_validate_flags_arm64(emitter, preg_flags_off);

    emitter.instruction("bl __rt_preg_strip");                                  // strip slash delimiters and return regex flags in x3
    emitter.instruction(&format!("str x3, [sp, #{}]", regex_flags_off));        // save regex compilation flags
    emitter.instruction("bl __rt_pcre_to_posix");                               // materialize PCRE pattern as a C string
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off));       // save null-terminated PCRE pattern
    super::emit_prepare_regex_locale(emitter);
    emitter.instruction(&format!("add x0, sp, #{}", handle_off));               // pass opaque-handle output storage
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off));       // pass null-terminated PCRE pattern
    emitter.instruction(&format!("ldr x2, [sp, #{}]", regex_flags_off));        // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("add x3, sp, #{}", nmatch_off));               // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile");                                    // compile without exposing PCRE2-owned layouts
    emitter.instruction("cbnz x0, __rt_pma_cap_empty");                         // compile failure returns count 0 and []

    emitter.instruction(&format!("ldr x9, [sp, #{}]", nmatch_off));             // load match-slot count returned by the shim
    emitter.instruction("cmp x9, #1");                                          // a compiled pattern always has a full-match slot
    emitter.instruction("b.ge __rt_pma_cap_nmatch_ok");                         // keep the compiled slot count when it is at least 1
    emitter.instruction("mov x9, #1");                                          // force at least the full-match row
    emitter.instruction(&format!("str x9, [sp, #{}]", nmatch_off));             // publish the normalized slot count
    emitter.label("__rt_pma_cap_nmatch_ok");
    emitter.instruction("lsr x10, x9, #60");                                    // reject slot counts whose 16-byte size would overflow
    emitter.instruction("cbnz x10, __rt_pma_cap_malloc_fail");                  // free the handle instead of allocating a wrapped size
    emitter.instruction("lsl x0, x9, #4");                                      // allocate one 16-byte signed-64-bit pair per slot
    emitter.bl_c("malloc");                                                     // allocate the reusable offset-pair vector
    emitter.instruction("cbz x0, __rt_pma_cap_malloc_fail");                    // allocation failure frees the handle and returns []
    emitter.instruction(&format!("str x0, [sp, #{}]", regmatches_ptr_off));     // save dynamic offset-pair buffer pointer

    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // load elephc subject pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // load elephc subject length
    emitter.instruction("bl __rt_cstr2");                                       // materialize a null-terminated subject copy
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string
    emitter.instruction(&format!("str x0, [sp, #{}]", current_cstr_off));       // start the search cursor at the subject beginning

    emit_preg_match_all_capture_init_matrix_arm64(
        emitter,
        preg_flags_off,
        nmatch_off,
        group_rows_off,
        outer_off,
        group_idx_off,
    );

    emitter.label("__rt_pma_cap_loop");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_cstr_off));        // reload the current subject C-string cursor
    emitter.instruction("ldrb w9, [x1]");                                       // inspect the byte at the current cursor
    emitter.instruction("cbz w9, __rt_pma_cap_done");                           // the trailing NUL ends the search
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // pass compiled opaque handle
    emitter.instruction(&format!("ldr x2, [sp, #{}]", nmatch_off));             // request one pair for every compiled capture
    emitter.instruction(&format!("ldr x3, [sp, #{}]", regmatches_ptr_off));     // pass the reusable offset-pair buffer
    emitter.instruction("mov x4, #0");                                          // use default execution flags
    emitter.bl_c("elephc_pcre2_v1_exec");                                       // execute and initialize every requested offset pair
    emitter.instruction("cbnz x0, __rt_pma_cap_done");                          // stop when the shim reports no further match

    emitter.instruction(&format!("ldr x9, [sp, #{}]", match_count_off));        // reload the running match count
    emitter.instruction("add x9, x9, #1");                                      // count this non-overlapping match
    emitter.instruction(&format!("str x9, [sp, #{}]", match_count_off));        // save the updated match count

    emit_preg_match_all_capture_append_match_arm64(
        emitter,
        preg_flags_off,
        nmatch_off,
        regmatches_ptr_off,
        current_cstr_off,
        subject_cstr_off,
        subject_ptr_off,
        group_rows_off,
        outer_off,
        group_idx_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
        row_off,
    );

    emitter.instruction(&format!("ldr x14, [sp, #{}]", regmatches_ptr_off));    // load the full-match pair base
    emitter.instruction("ldr x11, [x14, #8]");                                  // load signed-64-bit full-match end
    emitter.instruction("cmp x11, #0");                                         // detect a zero-length match
    emitter.instruction("b.gt __rt_pma_cap_adv");                               // use rm_eo when the match consumed bytes
    emitter.instruction("mov x11, #1");                                          // force zero-length matches to advance one byte
    emitter.label("__rt_pma_cap_adv");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_cstr_off));      // reload the current C-string cursor
    emitter.instruction("add x10, x10, x11");                                   // advance past this match
    emitter.instruction(&format!("str x10, [sp, #{}]", current_cstr_off));      // save the advanced cursor
    emitter.instruction("b __rt_pma_cap_loop");                                 // continue searching

    emitter.label("__rt_pma_cap_done");
    emit_preg_match_all_capture_finish_pattern_order_arm64(
        emitter,
        preg_flags_off,
        nmatch_off,
        group_rows_off,
        outer_off,
        group_idx_off,
        mixed_ptr_off,
    );
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regmatches_ptr_off));     // reload the offset-pair buffer
    emitter.bl_c("free");                                                       // release the reusable pair vector
    emitter.instruction(&format!("ldr x0, [sp, #{}]", match_count_off));        // return the match count in x0
    emitter.instruction(&format!("ldr x1, [sp, #{}]", outer_off));              // return the matches array in x1
    emitter.instruction("b __rt_pma_cap_ret");                                  // share the common epilogue

    emitter.label("__rt_pma_cap_malloc_fail");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // reload the opaque handle after allocation failure
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regmatches_ptr_off));     // reload any allocated offset-pair buffer
    emitter.instruction("cbz x0, __rt_pma_cap_empty");                          // skip free when compile never allocated pairs
    emitter.bl_c("free");                                                       // release the pair vector before returning []

    emitter.label("__rt_pma_cap_empty");
    emitter.instruction("mov x0, #0");                                          // empty-array capacity
    emitter.instruction("mov x1, #8");                                          // Mixed slots store boxed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate an empty Mixed matches array
    emit_stamp_indexed_array_mixed_arm64(emitter, "x0");
    emitter.instruction("mov x1, x0");                                          // return the empty array in x1
    emitter.instruction("mov x0, #0");                                          // return count 0

    emitter.label("__rt_pma_cap_ret");
    emitter.instruction(&format!("ldr x29, [sp, #{}]", save_off));              // restore frame pointer
    emitter.instruction(&format!("ldr x30, [sp, #{}]", save_off + 8));          // restore return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate the capture stack frame
    emitter.instruction("ret");                                                 // return count in x0 and matches in x1
}

/// Rejects unsupported ARM64 flag combinations before compile.
fn emit_preg_match_all_capture_validate_flags_arm64(
    emitter: &mut Emitter,
    preg_flags_off: usize,
) {
    emitter.instruction(&format!("ldr x9, [sp, #{}]", preg_flags_off));         // reload PHP preg_match_all flags
    emitter.instruction(&format!("mov x10, #{}", PREG_PATTERN_ORDER));          // start the supported-flag mask
    emitter.instruction(&format!("orr x10, x10, #{}", PREG_SET_ORDER));         // include SET_ORDER
    emitter.instruction(&format!("mov x11, #{}", PREG_OFFSET_CAPTURE));         // materialize OFFSET_CAPTURE
    emitter.instruction("orr x10, x10, x11");                                   // include OFFSET_CAPTURE
    emitter.instruction(&format!("mov x11, #{}", PREG_UNMATCHED_AS_NULL));      // materialize UNMATCHED_AS_NULL
    emitter.instruction("orr x10, x10, x11");                                   // include UNMATCHED_AS_NULL
    emitter.instruction("mvn x11, x10");                                        // invert the supported mask
    emitter.instruction("tst x9, x11");                                         // any unsupported extra bit?
    emitter.instruction("b.ne __rt_pma_cap_empty");                             // unsupported flags return count 0 and []
    emitter.instruction(&format!(
        "and x11, x9, #{}",
        PREG_PATTERN_ORDER | PREG_SET_ORDER
    )); // isolate the order bits
    emitter.instruction(&format!(
        "cmp x11, #{}",
        PREG_PATTERN_ORDER | PREG_SET_ORDER
    )); // both order bits together are invalid
    emitter.instruction("b.eq __rt_pma_cap_empty");                             // conflicting order flags return count 0 and []
}

/// Allocates the ARM64 SET_ORDER outer array or PATTERN_ORDER group-row table.
fn emit_preg_match_all_capture_init_matrix_arm64(
    emitter: &mut Emitter,
    preg_flags_off: usize,
    nmatch_off: usize,
    group_rows_off: usize,
    outer_off: usize,
    group_idx_off: usize,
) {
    emitter.instruction(&format!("ldr x9, [sp, #{}]", preg_flags_off));         // reload PHP flags before choosing matrix order
    emitter.instruction(&format!("tst x9, #{}", PREG_SET_ORDER));               // SET_ORDER grows one row per match
    emitter.instruction("b.ne __rt_pma_cap_init_set");                          // allocate an empty SET_ORDER outer array
    emitter.instruction(&format!("ldr x0, [sp, #{}]", nmatch_off));             // allocate one group-row pointer per compiled slot
    emitter.instruction("lsl x0, x0, #3");                                      // group-row table is nmatch 8-byte pointers
    emitter.bl_c("malloc");                                                     // allocate the PATTERN_ORDER group-row table
    emitter.instruction("cbz x0, __rt_pma_cap_malloc_fail");                    // table allocation failure shares handle cleanup
    emitter.instruction(&format!("str x0, [sp, #{}]", group_rows_off));         // save the group-row table pointer
    emitter.instruction(&format!("str xzr, [sp, #{}]", outer_off));             // PATTERN_ORDER outer is built after the search
    emitter.instruction(&format!("str xzr, [sp, #{}]", group_idx_off));         // start filling group row 0
    emitter.label("__rt_pma_cap_init_po_loop");
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload the current group index
    emitter.instruction(&format!("ldr x13, [sp, #{}]", nmatch_off));            // reload compiled slot count
    emitter.instruction("cmp x12, x13");                                        // have all group rows been allocated?
    emitter.instruction("b.ge __rt_pma_cap_init_done");                         // PATTERN_ORDER skeleton is ready
    emitter.instruction("mov x0, #8");                                          // each group row starts with room for several matches
    emitter.instruction("mov x1, #8");                                          // Mixed slots store boxed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate one empty group row
    emit_stamp_indexed_array_mixed_arm64(emitter, "x0");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", group_rows_off));         // reload the group-row table
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload the current group index
    emitter.instruction("lsl x10, x12, #3");                                    // scale the group index to a pointer offset
    emitter.instruction("str x0, [x9, x10]");                                   // store this group row pointer
    emitter.instruction("add x12, x12, #1");                                    // advance to the next group index
    emitter.instruction(&format!("str x12, [sp, #{}]", group_idx_off));         // save the next group index
    emitter.instruction("b __rt_pma_cap_init_po_loop");                         // continue allocating group rows
    emitter.label("__rt_pma_cap_init_set");
    emitter.instruction("mov x0, #8");                                          // SET_ORDER outer starts with room for several matches
    emitter.instruction("mov x1, #8");                                          // Mixed slots store boxed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate an empty SET_ORDER outer array
    emit_stamp_indexed_array_mixed_arm64(emitter, "x0");
    emitter.instruction(&format!("str x0, [sp, #{}]", outer_off));              // save the SET_ORDER outer array
    emitter.label("__rt_pma_cap_init_done");
}

/// Appends one ARM64 match into the live PATTERN_ORDER or SET_ORDER matrix.
#[allow(clippy::too_many_arguments)]
fn emit_preg_match_all_capture_append_match_arm64(
    emitter: &mut Emitter,
    preg_flags_off: usize,
    nmatch_off: usize,
    regmatches_ptr_off: usize,
    current_cstr_off: usize,
    subject_cstr_off: usize,
    subject_ptr_off: usize,
    group_rows_off: usize,
    outer_off: usize,
    group_idx_off: usize,
    piece_ptr_off: usize,
    piece_len_off: usize,
    piece_offset_off: usize,
    pair_ptr_off: usize,
    mixed_ptr_off: usize,
    row_off: usize,
) {
    emitter.instruction(&format!("ldr x9, [sp, #{}]", preg_flags_off));         // reload PHP flags before choosing the append shape
    emitter.instruction(&format!("tst x9, #{}", PREG_SET_ORDER));               // SET_ORDER builds one row for this match
    emitter.instruction("b.ne __rt_pma_cap_append_set");                        // build a SET_ORDER row
    emitter.instruction(&format!("str xzr, [sp, #{}]", group_idx_off));         // start at group 0 for PATTERN_ORDER
    emitter.label("__rt_pma_cap_append_po_loop");
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload the current group index
    emitter.instruction(&format!("ldr x13, [sp, #{}]", nmatch_off));            // reload compiled slot count
    emitter.instruction("cmp x12, x13");                                        // have all groups for this match been stored?
    emitter.instruction("b.ge __rt_pma_cap_append_done");                       // PATTERN_ORDER append for this match is complete
    emit_preg_match_all_capture_box_cell_arm64(
        emitter,
        "po",
        preg_flags_off,
        regmatches_ptr_off,
        group_idx_off,
        current_cstr_off,
        subject_cstr_off,
        subject_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );
    emitter.instruction(&format!("ldr x9, [sp, #{}]", group_rows_off));         // reload the group-row table
    emitter.instruction(&format!("ldr x10, [sp, #{}]", group_idx_off));         // reload the current group index
    emitter.instruction("lsl x11, x10, #3");                                    // scale the group index to a pointer offset
    emitter.instruction("ldr x0, [x9, x11]");                                   // load this group's growing row array
    emitter.instruction(&format!("ldr x1, [sp, #{}]", mixed_ptr_off));          // load the boxed capture cell
    emitter.instruction("bl __rt_array_push_refcounted");                       // append and retain the boxed cell
    emitter.instruction(&format!("ldr x9, [sp, #{}]", group_rows_off));         // reload the group-row table after the helper
    emitter.instruction(&format!("ldr x10, [sp, #{}]", group_idx_off));         // reload the current group index
    emitter.instruction("lsl x11, x10, #3");                                    // scale the group index to a pointer offset
    emitter.instruction("str x0, [x9, x11]");                                   // store the possibly-grown group row
    emitter.instruction(&format!("ldr x0, [sp, #{}]", mixed_ptr_off));          // reload helper-owned boxed cell
    emitter.instruction("bl __rt_decref_mixed");                                // drop helper ownership after the row retained it
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload the group index
    emitter.instruction("add x12, x12, #1");                                    // advance to the next group
    emitter.instruction(&format!("str x12, [sp, #{}]", group_idx_off));         // save the next group index
    emitter.instruction("b __rt_pma_cap_append_po_loop");                       // continue filling PATTERN_ORDER groups
    emitter.label("__rt_pma_cap_append_set");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", nmatch_off));             // SET_ORDER row capacity matches compiled slots
    emitter.instruction("mov x1, #8");                                          // Mixed slots store boxed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate this match's SET_ORDER row
    emit_stamp_indexed_array_mixed_arm64(emitter, "x0");
    emitter.instruction(&format!("str x0, [sp, #{}]", row_off));                // save the SET_ORDER row pointer
    emitter.instruction(&format!("str xzr, [sp, #{}]", group_idx_off));         // start at group 0
    emitter.label("__rt_pma_cap_append_set_loop");
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload the current group index
    emitter.instruction(&format!("ldr x13, [sp, #{}]", nmatch_off));            // reload compiled slot count
    emitter.instruction("cmp x12, x13");                                        // have all groups for this match been stored?
    emitter.instruction("b.ge __rt_pma_cap_append_set_row");                    // box the completed SET_ORDER row
    emit_preg_match_all_capture_box_cell_arm64(
        emitter,
        "set",
        preg_flags_off,
        regmatches_ptr_off,
        group_idx_off,
        current_cstr_off,
        subject_cstr_off,
        subject_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );
    emitter.instruction(&format!("ldr x0, [sp, #{}]", row_off));                // reload the SET_ORDER row
    emitter.instruction(&format!("ldr x1, [sp, #{}]", mixed_ptr_off));          // load the boxed capture cell
    emitter.instruction("bl __rt_array_push_refcounted");                       // append and retain the boxed cell
    emitter.instruction(&format!("str x0, [sp, #{}]", row_off));                // save the possibly-grown SET_ORDER row
    emitter.instruction(&format!("ldr x0, [sp, #{}]", mixed_ptr_off));          // reload helper-owned boxed cell
    emitter.instruction("bl __rt_decref_mixed");                                // drop helper ownership after the row retained it
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload the group index
    emitter.instruction("add x12, x12, #1");                                    // advance to the next group
    emitter.instruction(&format!("str x12, [sp, #{}]", group_idx_off));         // save the next group index
    emitter.instruction("b __rt_pma_cap_append_set_loop");                      // continue filling the SET_ORDER row
    emitter.label("__rt_pma_cap_append_set_row");
    emitter.instruction("mov x0, #4");                                          // runtime value tag 4 = indexed array
    emitter.instruction(&format!("ldr x1, [sp, #{}]", row_off));                // load the SET_ORDER row for boxing
    emitter.instruction("mov x2, xzr");                                         // indexed-array payload has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the SET_ORDER row as Mixed
    emitter.instruction(&format!("str x0, [sp, #{}]", mixed_ptr_off));          // save the boxed row Mixed pointer
    emitter.instruction(&format!("ldr x0, [sp, #{}]", row_off));                // reload the helper-owned row array
    emitter.instruction("bl __rt_decref_array");                                // drop helper ownership after Mixed retain
    emitter.instruction(&format!("ldr x0, [sp, #{}]", outer_off));              // reload the SET_ORDER outer array
    emitter.instruction(&format!("ldr x1, [sp, #{}]", mixed_ptr_off));          // load the boxed row
    emitter.instruction("bl __rt_array_push_refcounted");                       // append and retain the boxed row
    emitter.instruction(&format!("str x0, [sp, #{}]", outer_off));              // save the possibly-grown outer array
    emitter.instruction(&format!("ldr x0, [sp, #{}]", mixed_ptr_off));          // reload helper-owned boxed row
    emitter.instruction("bl __rt_decref_mixed");                                // drop helper ownership after the outer retained it
    emitter.label("__rt_pma_cap_append_done");
}

/// Boxes one ARM64 capture cell, including optional offset-capture wrapping.
#[allow(clippy::too_many_arguments)]
fn emit_preg_match_all_capture_box_cell_arm64(
    emitter: &mut Emitter,
    suffix: &str,
    preg_flags_off: usize,
    regmatches_ptr_off: usize,
    group_idx_off: usize,
    current_cstr_off: usize,
    subject_cstr_off: usize,
    subject_ptr_off: usize,
    piece_ptr_off: usize,
    piece_len_off: usize,
    piece_offset_off: usize,
    pair_ptr_off: usize,
    mixed_ptr_off: usize,
) {
    let unmatched = format!("__rt_pma_cap_box_unmatched_{suffix}");
    let empty = format!("__rt_pma_cap_box_empty_{suffix}");
    let maybe_offset = format!("__rt_pma_cap_box_maybe_offset_{suffix}");
    let done = format!("__rt_pma_cap_box_done_{suffix}");
    emitter.instruction(&format!("ldr x14, [sp, #{}]", regmatches_ptr_off));    // load the offset-pair buffer
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // load the current group index
    emitter.instruction("lsl x15, x12, #4");                                    // scale the group index by the 16-byte pair stride
    emitter.instruction("add x14, x14, x15");                                   // compute this group's offset pair
    emitter.instruction("ldr x15, [x14]");                                      // load signed-64-bit capture start
    emitter.instruction("ldr x16, [x14, #8]");                                  // load signed-64-bit capture end
    emitter.instruction("cmp x15, #0");                                         // unmatched captures report a negative start
    emitter.instruction(&format!("b.lt {unmatched}"));                          // emit PHP's unmatched cell
    emitter.instruction(&format!("ldr x9, [sp, #{}]", current_cstr_off));       // reload the current C-string cursor
    emitter.instruction(&format!("ldr x10, [sp, #{}]", subject_cstr_off));      // reload the original subject C string
    emitter.instruction("sub x9, x9, x10");                                     // bytes already consumed before this exec
    emitter.instruction("add x9, x9, x15");                                     // absolute subject offset of this capture
    emitter.instruction(&format!("str x9, [sp, #{}]", piece_offset_off));       // save the absolute byte offset
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // reload the original elephc subject payload
    emitter.instruction("add x1, x1, x9");                                      // capture pointer = subject + absolute offset
    emitter.instruction("sub x2, x16, x15");                                    // capture length = rm_eo - rm_so
    emitter.instruction(&format!("str x1, [sp, #{}]", piece_ptr_off));          // save the capture string pointer
    emitter.instruction(&format!("str x2, [sp, #{}]", piece_len_off));          // save the capture string length
    emitter.instruction("mov x0, #1");                                          // runtime value tag 1 = string
    emitter.instruction(&format!("ldr x1, [sp, #{}]", piece_ptr_off));          // load the capture string pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", piece_len_off));          // load the capture string length
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the capture string
    emitter.instruction(&format!("str x0, [sp, #{}]", mixed_ptr_off));          // save the boxed string cell
    emitter.instruction(&format!("b {maybe_offset}"));                          // wrap with an offset row when requested
    emitter.label(&unmatched);
    emitter.instruction("mov x9, #-1");                                         // unmatched offset-capture uses -1
    emitter.instruction(&format!("str x9, [sp, #{}]", piece_offset_off));       // save the unmatched offset
    emitter.instruction(&format!("ldr x9, [sp, #{}]", preg_flags_off));         // reload PHP flags
    emitter.instruction(&format!("tst x9, #{}", PREG_UNMATCHED_AS_NULL));       // should unmatched cells be Mixed null?
    emitter.instruction(&format!("b.eq {empty}"));                              // default unmatched cell is an empty string
    emitter.instruction("mov x0, #8");                                          // runtime value tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null payload is zero
    emitter.instruction("mov x2, #0");                                          // null payload has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box Mixed null
    emitter.instruction(&format!("str x0, [sp, #{}]", mixed_ptr_off));          // save the boxed null cell
    emitter.instruction(&format!("b {maybe_offset}"));                          // wrap with an offset row when requested
    emitter.label(&empty);
    emitter.instruction("mov x0, #1");                                          // runtime value tag 1 = string
    emitter.instruction("mov x1, #0");                                          // empty unmatched capture has a null pointer
    emitter.instruction("mov x2, #0");                                          // empty unmatched capture has zero length
    emitter.instruction("bl __rt_mixed_from_value");                            // box the empty string
    emitter.instruction(&format!("str x0, [sp, #{}]", mixed_ptr_off));          // save the boxed empty-string cell
    emitter.label(&maybe_offset);
    emitter.instruction(&format!("ldr x9, [sp, #{}]", preg_flags_off));         // reload PHP flags
    emitter.instruction(&format!("tst x9, #{}", PREG_OFFSET_CAPTURE));          // OFFSET_CAPTURE boxes [value, offset]
    emitter.instruction(&format!("b.eq {done}"));                               // leave a bare value cell when the flag is off
    emitter.instruction("mov x0, #2");                                          // capacity for [value, offset]
    emitter.instruction("mov x1, #8");                                          // row stores boxed Mixed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate the offset-capture row
    emit_stamp_indexed_array_mixed_arm64(emitter, "x0");
    emitter.instruction(&format!("str x0, [sp, #{}]", pair_ptr_off));           // save the offset-capture row pointer
    emitter.instruction(&format!("ldr x9, [sp, #{}]", pair_ptr_off));           // reload the row array
    emitter.instruction(&format!("ldr x10, [sp, #{}]", mixed_ptr_off));         // reload the boxed value cell
    emitter.instruction("str x10, [x9, #24]");                                  // store row[0] = boxed value
    emitter.instruction("mov x11, #1");                                         // row length after storing the value cell
    emitter.instruction("str x11, [x9]");                                       // publish row length 1
    emitter.instruction("mov x0, #0");                                          // runtime value tag 0 = integer
    emitter.instruction(&format!("ldr x1, [sp, #{}]", piece_offset_off));       // load the absolute or unmatched offset
    emitter.instruction("mov x2, xzr");                                         // integer payload has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the integer offset
    emitter.instruction(&format!("ldr x9, [sp, #{}]", pair_ptr_off));           // reload the row array
    emitter.instruction("str x0, [x9, #32]");                                   // store row[1] = boxed offset
    emitter.instruction("mov x11, #2");                                         // row length after storing both cells
    emitter.instruction("str x11, [x9]");                                       // publish row length 2
    emitter.instruction("mov x0, #4");                                          // runtime value tag 4 = indexed array
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pair_ptr_off));           // load the row array for boxing
    emitter.instruction("mov x2, xzr");                                         // indexed-array payload has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the offset-capture row as Mixed
    emitter.instruction(&format!("str x0, [sp, #{}]", mixed_ptr_off));          // save the boxed [value, offset] cell
    emitter.instruction(&format!("ldr x0, [sp, #{}]", pair_ptr_off));           // reload the helper-owned row array
    emitter.instruction("bl __rt_decref_array");                                // drop helper ownership after Mixed retain
    emitter.label(&done);
}

/// Boxes PATTERN_ORDER group rows into the ARM64 outer array after the search.
fn emit_preg_match_all_capture_finish_pattern_order_arm64(
    emitter: &mut Emitter,
    preg_flags_off: usize,
    nmatch_off: usize,
    group_rows_off: usize,
    outer_off: usize,
    group_idx_off: usize,
    mixed_ptr_off: usize,
) {
    emitter.instruction(&format!("ldr x9, [sp, #{}]", preg_flags_off));         // reload PHP flags
    emitter.instruction(&format!("tst x9, #{}", PREG_SET_ORDER));               // SET_ORDER already built the outer array
    emitter.instruction("b.ne __rt_pma_cap_finish_skip");                       // leave the SET_ORDER outer unchanged
    emitter.instruction(&format!("ldr x9, [sp, #{}]", group_rows_off));         // reload the group-row table
    emitter.instruction("cbz x9, __rt_pma_cap_finish_skip");                    // missing table means the empty-result path
    emitter.instruction(&format!("ldr x0, [sp, #{}]", nmatch_off));             // outer capacity matches compiled slots
    emitter.instruction("mov x1, #8");                                          // Mixed slots store boxed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate the PATTERN_ORDER outer array
    emit_stamp_indexed_array_mixed_arm64(emitter, "x0");
    emitter.instruction(&format!("str x0, [sp, #{}]", outer_off));              // replace the placeholder outer array
    emitter.instruction(&format!("str xzr, [sp, #{}]", group_idx_off));         // start boxing group 0
    emitter.label("__rt_pma_cap_finish_loop");
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload the current group index
    emitter.instruction(&format!("ldr x13, [sp, #{}]", nmatch_off));            // reload compiled slot count
    emitter.instruction("cmp x12, x13");                                        // have all group rows been boxed?
    emitter.instruction("b.ge __rt_pma_cap_finish_free");                       // free the group-row table after boxing
    emitter.instruction(&format!("ldr x9, [sp, #{}]", group_rows_off));         // reload the group-row table
    emitter.instruction("lsl x10, x12, #3");                                    // scale the group index to a pointer offset
    emitter.instruction("ldr x1, [x9, x10]");                                   // load this group row array
    emitter.instruction("mov x0, #4");                                          // runtime value tag 4 = indexed array
    emitter.instruction("mov x2, xzr");                                         // indexed-array payload has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the group row as Mixed
    emitter.instruction(&format!("str x0, [sp, #{}]", mixed_ptr_off));          // save the boxed group-row cell
    emitter.instruction(&format!("ldr x9, [sp, #{}]", group_rows_off));         // reload the group-row table
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload the current group index
    emitter.instruction("lsl x10, x12, #3");                                    // scale the group index to a pointer offset
    emitter.instruction("ldr x0, [x9, x10]");                                   // reload the helper-owned group row
    emitter.instruction("bl __rt_decref_array");                                // drop helper ownership after Mixed retain
    emitter.instruction(&format!("ldr x0, [sp, #{}]", outer_off));              // reload the PATTERN_ORDER outer array
    emitter.instruction(&format!("ldr x1, [sp, #{}]", mixed_ptr_off));          // load the boxed group row
    emitter.instruction("bl __rt_array_push_refcounted");                       // append and retain the boxed group row
    emitter.instruction(&format!("str x0, [sp, #{}]", outer_off));              // save the possibly-grown outer array
    emitter.instruction(&format!("ldr x0, [sp, #{}]", mixed_ptr_off));          // reload helper-owned boxed group row
    emitter.instruction("bl __rt_decref_mixed");                                // drop helper ownership after the outer retained it
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload the group index
    emitter.instruction("add x12, x12, #1");                                    // advance to the next group
    emitter.instruction(&format!("str x12, [sp, #{}]", group_idx_off));         // save the next group index
    emitter.instruction("b __rt_pma_cap_finish_loop");                          // continue boxing group rows
    emitter.label("__rt_pma_cap_finish_free");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", group_rows_off));         // reload the group-row table
    emitter.bl_c("free");                                                       // release the PATTERN_ORDER pointer table
    emitter.instruction(&format!("str xzr, [sp, #{}]", group_rows_off));        // forget the freed table pointer
    emitter.label("__rt_pma_cap_finish_skip");
}

/// Emits ARM64 code that stamps an indexed array as boxed-Mixed slots.
fn emit_stamp_indexed_array_mixed_arm64(emitter: &mut Emitter, array_reg: &str) {
    emitter.instruction(&format!("ldr x10, [{array_reg}, #-8]"));               // load indexed-array packed kind word
    emitter.instruction("mov x11, #0x80ff");                                    // preserve indexed-array kind and persistent COW flag
    emitter.instruction("and x10, x10, x11");                                   // clear stale value_type bits
    emitter.instruction("mov x11, #7");                                         // runtime value_type 7 = boxed Mixed
    emitter.instruction("lsl x11, x11, #8");                                    // move Mixed tag into packed kind word
    emitter.instruction("orr x10, x10, x11");                                   // combine stable metadata with Mixed tag
    emitter.instruction(&format!("str x10, [{array_reg}, #-8]"));               // store boxed-Mixed indexed-array metadata
}

/// Emits x86_64 `__rt_preg_match_all_capture`.
fn emit_preg_match_all_capture_linux_x86_64(emitter: &mut Emitter) {
    let handle_off = 0;
    let regmatches_ptr_off = handle_off + 8;
    let nmatch_off = regmatches_ptr_off + 8;
    let preg_flags_off = nmatch_off + 8;
    let regex_flags_off = preg_flags_off + 8;
    let pattern_cstr_off = regex_flags_off + 8;
    let subject_ptr_off = pattern_cstr_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let subject_cstr_off = subject_len_off + 8;
    let current_cstr_off = subject_cstr_off + 8;
    let match_count_off = current_cstr_off + 8;
    let group_rows_off = match_count_off + 8;
    let outer_off = group_rows_off + 8;
    let group_idx_off = outer_off + 8;
    let piece_ptr_off = group_idx_off + 8;
    let piece_len_off = piece_ptr_off + 8;
    let piece_offset_off = piece_len_off + 8;
    let pair_ptr_off = piece_offset_off + 8;
    let mixed_ptr_off = pair_ptr_off + 8;
    let row_off = mixed_ptr_off + 8;
    let stack_size = (row_off + 32 + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: preg_match_all_capture ---");
    emitter.label_global("__rt_preg_match_all_capture");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve capture helper local storage
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", subject_ptr_off)); // preserve the elephc subject pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", subject_len_off)); // preserve the elephc subject length
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r8", preg_flags_off)); // preserve PHP preg_match_all flags
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", group_rows_off)); // no group-row table until PATTERN_ORDER setup
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", regmatches_ptr_off)); // no offset-pair buffer until compile succeeds
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", match_count_off)); // match count starts at zero
    emitter.instruction("mov rax, rdi");                                        // move the pattern pointer into preg-strip input
    emitter.instruction("mov rdx, rsi");                                        // move the pattern length into preg-strip input

    emit_preg_match_all_capture_validate_flags_linux_x86_64(emitter, preg_flags_off);

    emitter.instruction("call __rt_preg_strip");                                // strip slash delimiters and collect regex flags
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", regex_flags_off)); // save stripped regex flags
    emitter.instruction("call __rt_pcre_to_posix");                             // materialize PCRE pattern as a C string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // save null-terminated PCRE pattern
    super::emit_prepare_regex_locale(emitter);
    emitter.instruction(&format!("lea rdi, [rsp + {}]", handle_off));           // pass opaque-handle output storage
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass null-terminated PCRE pattern
    emitter.instruction(&format!("mov edx, DWORD PTR [rsp + {}]", regex_flags_off)); // pass PCRE2 POSIX compile flags
    emitter.instruction(&format!("lea rcx, [rsp + {}]", nmatch_off));           // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile");                                    // compile without exposing PCRE2-owned layouts
    emitter.instruction("test eax, eax");                                       // did regex compilation succeed?
    emitter.instruction("jnz __rt_pma_cap_empty_linux_x86_64");                 // compile failure returns count 0 and []

    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", nmatch_off));  // load match-slot count returned by the shim
    emitter.instruction("cmp r9, 1");                                           // a compiled pattern always has a full-match slot
    emitter.instruction("jge __rt_pma_cap_nmatch_ok_linux_x86_64");             // keep the compiled slot count when it is at least 1
    emitter.instruction("mov r9, 1");                                           // force at least the full-match row
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", nmatch_off));  // publish the normalized slot count
    emitter.label("__rt_pma_cap_nmatch_ok_linux_x86_64");
    emitter.instruction("mov r10, r9");                                         // copy slot count for overflow validation
    emitter.instruction("shr r10, 60");                                         // detect a wrapped 16-byte pair-vector size
    emitter.instruction("jnz __rt_pma_cap_malloc_fail_linux_x86_64");           // free the handle instead of allocating a wrapped size
    emitter.instruction("mov rdi, r9");                                         // load slot count for the pair-vector allocation
    emitter.instruction("shl rdi, 4");                                          // allocate one 16-byte signed-64-bit pair per slot
    emitter.bl_c("malloc");                                                     // allocate the reusable offset-pair vector
    emitter.instruction("test rax, rax");                                       // did malloc return a pair buffer?
    emitter.instruction("jz __rt_pma_cap_malloc_fail_linux_x86_64");            // allocation failure frees the handle and returns []
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", regmatches_ptr_off)); // save dynamic offset-pair buffer pointer

    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload the elephc subject pointer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload the elephc subject length
    emitter.instruction("call __rt_cstr2");                                     // materialize a null-terminated subject copy
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // save subject C string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", current_cstr_off)); // start the search cursor at the subject beginning

    emit_preg_match_all_capture_init_matrix_linux_x86_64(
        emitter,
        preg_flags_off,
        nmatch_off,
        group_rows_off,
        outer_off,
        group_idx_off,
    );

    emitter.label("__rt_pma_cap_loop_linux_x86_64");
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_cstr_off)); // reload the current subject C-string cursor
    emitter.instruction("movzx r9d, BYTE PTR [rsi]");                           // inspect the byte at the current cursor
    emitter.instruction("test r9d, r9d");                                       // the trailing NUL ends the search
    emitter.instruction("jz __rt_pma_cap_done_linux_x86_64");                   // stop when the subject has been consumed
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // pass compiled opaque handle
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", nmatch_off)); // request one pair for every compiled capture
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // pass the reusable offset-pair buffer
    emitter.instruction("xor r8d, r8d");                                        // use default execution flags
    emitter.bl_c("elephc_pcre2_v1_exec");                                       // execute and initialize every requested offset pair
    emitter.instruction("test eax, eax");                                       // did the shim find another match?
    emitter.instruction("jnz __rt_pma_cap_done_linux_x86_64");                  // stop when the shim reports no further match

    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", match_count_off)); // reload the running match count
    emitter.instruction("add r9, 1");                                           // count this non-overlapping match
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", match_count_off)); // save the updated match count

    emit_preg_match_all_capture_append_match_linux_x86_64(
        emitter,
        preg_flags_off,
        nmatch_off,
        regmatches_ptr_off,
        current_cstr_off,
        subject_cstr_off,
        subject_ptr_off,
        group_rows_off,
        outer_off,
        group_idx_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
        row_off,
    );

    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load the full-match pair base
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load signed-64-bit full-match end
    emitter.instruction("cmp r11, 0");                                          // detect a zero-length match
    emitter.instruction("jg __rt_pma_cap_adv_linux_x86_64");                    // use rm_eo when the match consumed bytes
    emitter.instruction("mov r11, 1");                                          // force zero-length matches to advance one byte
    emitter.label("__rt_pma_cap_adv_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_cstr_off)); // reload the current C-string cursor
    emitter.instruction("add r10, r11");                                        // advance past this match
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", current_cstr_off)); // save the advanced cursor
    emitter.instruction("jmp __rt_pma_cap_loop_linux_x86_64");                  // continue searching

    emitter.label("__rt_pma_cap_done_linux_x86_64");
    emit_preg_match_all_capture_finish_pattern_order_linux_x86_64(
        emitter,
        preg_flags_off,
        nmatch_off,
        group_rows_off,
        outer_off,
        group_idx_off,
        mixed_ptr_off,
    );
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // reload the offset-pair buffer
    emitter.bl_c("free");                                                       // release the reusable pair vector
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", match_count_off)); // return the match count in rax
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", outer_off));  // return the matches array in rdx
    emitter.instruction("jmp __rt_pma_cap_ret_linux_x86_64");                   // share the common epilogue

    emitter.label("__rt_pma_cap_malloc_fail_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // reload the opaque handle after allocation failure
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // reload any allocated offset-pair buffer
    emitter.instruction("test rdi, rdi");                                       // skip free when compile never allocated pairs
    emitter.instruction("jz __rt_pma_cap_empty_linux_x86_64");                  // share the empty-result path
    emitter.bl_c("free");                                                       // release the pair vector before returning []

    emitter.label("__rt_pma_cap_empty_linux_x86_64");
    emitter.instruction("xor edi, edi");                                        // empty-array capacity
    emitter.instruction("mov esi, 8");                                          // Mixed slots store boxed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate an empty Mixed matches array
    emit_stamp_indexed_array_mixed_x86_64(emitter, "rax");
    emitter.instruction("mov rdx, rax");                                        // return the empty array in rdx
    emitter.instruction("xor eax, eax");                                        // return count 0

    emitter.label("__rt_pma_cap_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release capture helper local storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return count in rax and matches in rdx
}

/// Rejects unsupported x86_64 flag combinations before compile.
fn emit_preg_match_all_capture_validate_flags_linux_x86_64(
    emitter: &mut Emitter,
    preg_flags_off: usize,
) {
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", preg_flags_off)); // reload PHP preg_match_all flags
    emitter.instruction(&format!("mov r10, {}", PREG_MATCH_ALL_SUPPORTED_FLAGS)); // load the supported-flag mask
    emitter.instruction("mov r11, r10");                                        // copy the supported mask
    emitter.instruction("not r11");                                             // invert the supported mask
    emitter.instruction("test r9, r11");                                        // any unsupported extra bit?
    emitter.instruction("jnz __rt_pma_cap_empty_linux_x86_64");                 // unsupported flags return count 0 and []
    emitter.instruction("mov r11, r9");                                         // copy flags before isolating order bits
    emitter.instruction(&format!("and r11, {}", PREG_PATTERN_ORDER | PREG_SET_ORDER)); // isolate the order bits
    emitter.instruction(&format!("cmp r11, {}", PREG_PATTERN_ORDER | PREG_SET_ORDER)); // both order bits together are invalid
    emitter.instruction("je __rt_pma_cap_empty_linux_x86_64");                  // conflicting order flags return count 0 and []
}

/// Allocates the x86_64 SET_ORDER outer array or PATTERN_ORDER group-row table.
fn emit_preg_match_all_capture_init_matrix_linux_x86_64(
    emitter: &mut Emitter,
    preg_flags_off: usize,
    nmatch_off: usize,
    group_rows_off: usize,
    outer_off: usize,
    group_idx_off: usize,
) {
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", preg_flags_off)); // reload PHP flags before choosing matrix order
    emitter.instruction(&format!("test r9, {}", PREG_SET_ORDER));               // SET_ORDER grows one row per match
    emitter.instruction("jnz __rt_pma_cap_init_set_linux_x86_64");              // allocate an empty SET_ORDER outer array
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", nmatch_off)); // allocate one group-row pointer per compiled slot
    emitter.instruction("shl rdi, 3");                                          // group-row table is nmatch 8-byte pointers
    emitter.bl_c("malloc");                                                     // allocate the PATTERN_ORDER group-row table
    emitter.instruction("test rax, rax");                                       // did malloc return a table?
    emitter.instruction("jz __rt_pma_cap_malloc_fail_linux_x86_64");            // table allocation failure shares handle cleanup
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", group_rows_off)); // save the group-row table pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", outer_off));    // PATTERN_ORDER outer is built after the search
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", group_idx_off)); // start filling group row 0
    emitter.label("__rt_pma_cap_init_po_loop_linux_x86_64");
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload the current group index
    emitter.instruction(&format!("cmp r9, QWORD PTR [rsp + {}]", nmatch_off));  // have all group rows been allocated?
    emitter.instruction("jge __rt_pma_cap_init_done_linux_x86_64");             // PATTERN_ORDER skeleton is ready
    emitter.instruction("mov edi, 8");                                          // each group row starts with room for several matches
    emitter.instruction("mov esi, 8");                                          // Mixed slots store boxed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate one empty group row
    emit_stamp_indexed_array_mixed_x86_64(emitter, "rax");
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_rows_off)); // reload the group-row table
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", group_idx_off)); // reload the current group index
    emitter.instruction("mov QWORD PTR [r9 + r10 * 8], rax");                   // store this group row pointer
    emitter.instruction("add r10, 1");                                          // advance to the next group index
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", group_idx_off)); // save the next group index
    emitter.instruction("jmp __rt_pma_cap_init_po_loop_linux_x86_64");          // continue allocating group rows
    emitter.label("__rt_pma_cap_init_set_linux_x86_64");
    emitter.instruction("mov edi, 8");                                          // SET_ORDER outer starts with room for several matches
    emitter.instruction("mov esi, 8");                                          // Mixed slots store boxed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate an empty SET_ORDER outer array
    emit_stamp_indexed_array_mixed_x86_64(emitter, "rax");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", outer_off));  // save the SET_ORDER outer array
    emitter.label("__rt_pma_cap_init_done_linux_x86_64");
}

/// Appends one x86_64 match into the live PATTERN_ORDER or SET_ORDER matrix.
#[allow(clippy::too_many_arguments)]
fn emit_preg_match_all_capture_append_match_linux_x86_64(
    emitter: &mut Emitter,
    preg_flags_off: usize,
    nmatch_off: usize,
    regmatches_ptr_off: usize,
    current_cstr_off: usize,
    subject_cstr_off: usize,
    subject_ptr_off: usize,
    group_rows_off: usize,
    outer_off: usize,
    group_idx_off: usize,
    piece_ptr_off: usize,
    piece_len_off: usize,
    piece_offset_off: usize,
    pair_ptr_off: usize,
    mixed_ptr_off: usize,
    row_off: usize,
) {
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", preg_flags_off)); // reload PHP flags before choosing the append shape
    emitter.instruction(&format!("test r9, {}", PREG_SET_ORDER));               // SET_ORDER builds one row for this match
    emitter.instruction("jnz __rt_pma_cap_append_set_linux_x86_64");            // build a SET_ORDER row
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", group_idx_off)); // start at group 0 for PATTERN_ORDER
    emitter.label("__rt_pma_cap_append_po_loop_linux_x86_64");
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload the current group index
    emitter.instruction(&format!("cmp r9, QWORD PTR [rsp + {}]", nmatch_off));  // have all groups for this match been stored?
    emitter.instruction("jge __rt_pma_cap_append_done_linux_x86_64");           // PATTERN_ORDER append for this match is complete
    emit_preg_match_all_capture_box_cell_linux_x86_64(
        emitter,
        "po",
        preg_flags_off,
        regmatches_ptr_off,
        group_idx_off,
        current_cstr_off,
        subject_cstr_off,
        subject_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_rows_off)); // reload the group-row table
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", group_idx_off)); // reload the current group index
    emitter.instruction("mov rdi, QWORD PTR [r9 + r10 * 8]");                   // load this group's growing row array
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", mixed_ptr_off)); // load the boxed capture cell
    emitter.instruction("call __rt_array_push_refcounted");                     // append and retain the boxed cell
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_rows_off)); // reload the group-row table after the helper
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", group_idx_off)); // reload the current group index
    emitter.instruction("mov QWORD PTR [r9 + r10 * 8], rax");                   // store the possibly-grown group row
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", mixed_ptr_off)); // reload helper-owned boxed cell
    emitter.instruction("call __rt_decref_mixed");                              // drop helper ownership after the row retained it
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload the group index
    emitter.instruction("add r9, 1");                                           // advance to the next group
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", group_idx_off)); // save the next group index
    emitter.instruction("jmp __rt_pma_cap_append_po_loop_linux_x86_64");        // continue filling PATTERN_ORDER groups
    emitter.label("__rt_pma_cap_append_set_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", nmatch_off)); // SET_ORDER row capacity matches compiled slots
    emitter.instruction("mov esi, 8");                                          // Mixed slots store boxed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate this match's SET_ORDER row
    emit_stamp_indexed_array_mixed_x86_64(emitter, "rax");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", row_off));    // save the SET_ORDER row pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", group_idx_off)); // start at group 0
    emitter.label("__rt_pma_cap_append_set_loop_linux_x86_64");
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload the current group index
    emitter.instruction(&format!("cmp r9, QWORD PTR [rsp + {}]", nmatch_off));  // have all groups for this match been stored?
    emitter.instruction("jge __rt_pma_cap_append_set_row_linux_x86_64");        // box the completed SET_ORDER row
    emit_preg_match_all_capture_box_cell_linux_x86_64(
        emitter,
        "set",
        preg_flags_off,
        regmatches_ptr_off,
        group_idx_off,
        current_cstr_off,
        subject_cstr_off,
        subject_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", row_off));    // reload the SET_ORDER row
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", mixed_ptr_off)); // load the boxed capture cell
    emitter.instruction("call __rt_array_push_refcounted");                     // append and retain the boxed cell
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", row_off));    // save the possibly-grown SET_ORDER row
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", mixed_ptr_off)); // reload helper-owned boxed cell
    emitter.instruction("call __rt_decref_mixed");                              // drop helper ownership after the row retained it
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload the group index
    emitter.instruction("add r9, 1");                                           // advance to the next group
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", group_idx_off)); // save the next group index
    emitter.instruction("jmp __rt_pma_cap_append_set_loop_linux_x86_64");       // continue filling the SET_ORDER row
    emitter.label("__rt_pma_cap_append_set_row_linux_x86_64");
    emitter.instruction("mov rax, 4");                                          // runtime value tag 4 = indexed array
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", row_off));    // load the SET_ORDER row for boxing
    emitter.instruction("xor esi, esi");                                        // indexed-array payload has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the SET_ORDER row as Mixed
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", mixed_ptr_off)); // save the boxed row Mixed pointer
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", row_off));    // reload the helper-owned row array into the x86 decref input
    emitter.instruction("call __rt_decref_array");                              // drop helper ownership after Mixed retain
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", outer_off));  // reload the SET_ORDER outer array
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", mixed_ptr_off)); // load the boxed row
    emitter.instruction("call __rt_array_push_refcounted");                     // append and retain the boxed row
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", outer_off));  // save the possibly-grown outer array
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", mixed_ptr_off)); // reload helper-owned boxed row
    emitter.instruction("call __rt_decref_mixed");                              // drop helper ownership after the outer retained it
    emitter.label("__rt_pma_cap_append_done_linux_x86_64");
}

/// Boxes one x86_64 capture cell, including optional offset-capture wrapping.
#[allow(clippy::too_many_arguments)]
fn emit_preg_match_all_capture_box_cell_linux_x86_64(
    emitter: &mut Emitter,
    suffix: &str,
    preg_flags_off: usize,
    regmatches_ptr_off: usize,
    group_idx_off: usize,
    current_cstr_off: usize,
    subject_cstr_off: usize,
    subject_ptr_off: usize,
    piece_ptr_off: usize,
    piece_len_off: usize,
    piece_offset_off: usize,
    pair_ptr_off: usize,
    mixed_ptr_off: usize,
) {
    let unmatched = format!("__rt_pma_cap_box_unmatched_{suffix}_linux_x86_64");
    let empty = format!("__rt_pma_cap_box_empty_{suffix}_linux_x86_64");
    let maybe_offset = format!("__rt_pma_cap_box_maybe_offset_{suffix}_linux_x86_64");
    let done = format!("__rt_pma_cap_box_done_{suffix}_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load the offset-pair buffer
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", group_idx_off)); // load the current group index
    emitter.instruction("shl r11, 4");                                          // scale the group index by the 16-byte pair stride
    emitter.instruction("add r10, r11");                                        // compute this group's offset pair
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load signed-64-bit capture start
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load signed-64-bit capture end
    emitter.instruction("cmp r11, 0");                                          // unmatched captures report a negative start
    emitter.instruction(&format!("jl {unmatched}"));                            // emit PHP's unmatched cell
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", current_cstr_off)); // reload the current C-string cursor
    emitter.instruction(&format!("sub r9, QWORD PTR [rsp + {}]", subject_cstr_off)); // bytes already consumed before this exec
    emitter.instruction("add r9, r11");                                         // absolute subject offset of this capture
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", piece_offset_off)); // save the absolute byte offset
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload the original elephc subject payload
    emitter.instruction("add rsi, r9");                                         // capture pointer = subject + absolute offset
    emitter.instruction("mov rdx, rcx");                                        // copy capture end before subtracting start
    emitter.instruction("sub rdx, r11");                                        // capture length = rm_eo - rm_so
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rsi", piece_ptr_off)); // save the capture string pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", piece_len_off)); // save the capture string length
    emitter.instruction("mov rax, 1");                                          // runtime value tag 1 = string
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", piece_ptr_off)); // load the capture string pointer
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", piece_len_off)); // load the capture string length
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the capture string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", mixed_ptr_off)); // save the boxed string cell
    emitter.instruction(&format!("jmp {maybe_offset}"));                        // wrap with an offset row when requested
    emitter.label(&unmatched);
    emitter.instruction("mov r9, -1");                                          // unmatched offset-capture uses -1
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", piece_offset_off)); // save the unmatched offset
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", preg_flags_off)); // reload PHP flags
    emitter.instruction(&format!("test r9, {}", PREG_UNMATCHED_AS_NULL));        // should unmatched cells be Mixed null?
    emitter.instruction(&format!("jz {empty}"));                                // default unmatched cell is an empty string
    emitter.instruction("mov rax, 8");                                          // runtime value tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null payload is zero
    emitter.instruction("xor esi, esi");                                        // null payload has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // box Mixed null
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", mixed_ptr_off)); // save the boxed null cell
    emitter.instruction(&format!("jmp {maybe_offset}"));                        // wrap with an offset row when requested
    emitter.label(&empty);
    emitter.instruction("mov rax, 1");                                          // runtime value tag 1 = string
    emitter.instruction("xor edi, edi");                                        // empty unmatched capture has a null pointer
    emitter.instruction("xor esi, esi");                                        // empty unmatched capture has zero length
    emitter.instruction("call __rt_mixed_from_value");                          // box the empty string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", mixed_ptr_off)); // save the boxed empty-string cell
    emitter.label(&maybe_offset);
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", preg_flags_off)); // reload PHP flags
    emitter.instruction(&format!("test r9, {}", PREG_OFFSET_CAPTURE));          // OFFSET_CAPTURE boxes [value, offset]
    emitter.instruction(&format!("jz {done}"));                                 // leave a bare value cell when the flag is off
    emitter.instruction("mov edi, 2");                                          // capacity for [value, offset]
    emitter.instruction("mov esi, 8");                                          // row stores boxed Mixed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate the offset-capture row
    emit_stamp_indexed_array_mixed_x86_64(emitter, "rax");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pair_ptr_off)); // save the offset-capture row pointer
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", pair_ptr_off)); // reload the row array
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", mixed_ptr_off)); // reload the boxed value cell
    emitter.instruction("mov QWORD PTR [r9 + 24], r10");                        // store row[0] = boxed value
    emitter.instruction("mov QWORD PTR [r9], 1");                               // publish row length 1
    emitter.instruction("xor eax, eax");                                        // runtime value tag 0 = integer
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", piece_offset_off)); // load the absolute or unmatched offset
    emitter.instruction("xor esi, esi");                                        // integer payload has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the integer offset
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", pair_ptr_off)); // reload the row array
    emitter.instruction("mov QWORD PTR [r9 + 32], rax");                        // store row[1] = boxed offset
    emitter.instruction("mov QWORD PTR [r9], 2");                               // publish row length 2
    emitter.instruction("mov rax, 4");                                          // runtime value tag 4 = indexed array
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", pair_ptr_off)); // load the row array for boxing
    emitter.instruction("xor esi, esi");                                        // indexed-array payload has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the offset-capture row as Mixed
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", mixed_ptr_off)); // save the boxed [value, offset] cell
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", pair_ptr_off)); // reload the helper-owned row array into the x86 decref input
    emitter.instruction("call __rt_decref_array");                              // drop helper ownership after Mixed retain
    emitter.label(&done);
}

/// Boxes PATTERN_ORDER group rows into the x86_64 outer array after the search.
fn emit_preg_match_all_capture_finish_pattern_order_linux_x86_64(
    emitter: &mut Emitter,
    preg_flags_off: usize,
    nmatch_off: usize,
    group_rows_off: usize,
    outer_off: usize,
    group_idx_off: usize,
    mixed_ptr_off: usize,
) {
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", preg_flags_off)); // reload PHP flags
    emitter.instruction(&format!("test r9, {}", PREG_SET_ORDER));               // SET_ORDER already built the outer array
    emitter.instruction("jnz __rt_pma_cap_finish_skip_linux_x86_64");           // leave the SET_ORDER outer unchanged
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_rows_off)); // reload the group-row table
    emitter.instruction("test r9, r9");                                         // missing table means the empty-result path
    emitter.instruction("jz __rt_pma_cap_finish_skip_linux_x86_64");            // skip boxing when there is no table
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", nmatch_off)); // outer capacity matches compiled slots
    emitter.instruction("mov esi, 8");                                          // Mixed slots store boxed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate the PATTERN_ORDER outer array
    emit_stamp_indexed_array_mixed_x86_64(emitter, "rax");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", outer_off));  // replace the placeholder outer array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", group_idx_off)); // start boxing group 0
    emitter.label("__rt_pma_cap_finish_loop_linux_x86_64");
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload the current group index
    emitter.instruction(&format!("cmp r9, QWORD PTR [rsp + {}]", nmatch_off));  // have all group rows been boxed?
    emitter.instruction("jge __rt_pma_cap_finish_free_linux_x86_64");           // free the group-row table after boxing
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", group_rows_off)); // reload the group-row table
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9 * 8]");                   // load this group row array
    emitter.instruction("mov rax, 4");                                          // runtime value tag 4 = indexed array
    emitter.instruction("xor esi, esi");                                        // indexed-array payload has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the group row as Mixed
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", mixed_ptr_off)); // save the boxed group-row cell
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload the current group index
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", group_rows_off)); // reload the group-row table
    emitter.instruction("mov rax, QWORD PTR [r10 + r9 * 8]");                   // reload the helper-owned group row into the x86 decref input
    emitter.instruction("call __rt_decref_array");                              // drop helper ownership after Mixed retain
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", outer_off));  // reload the PATTERN_ORDER outer array
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", mixed_ptr_off)); // load the boxed group row
    emitter.instruction("call __rt_array_push_refcounted");                     // append and retain the boxed group row
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", outer_off));  // save the possibly-grown outer array
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", mixed_ptr_off)); // reload helper-owned boxed group row
    emitter.instruction("call __rt_decref_mixed");                              // drop helper ownership after the outer retained it
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload the group index
    emitter.instruction("add r9, 1");                                           // advance to the next group
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", group_idx_off)); // save the next group index
    emitter.instruction("jmp __rt_pma_cap_finish_loop_linux_x86_64");           // continue boxing group rows
    emitter.label("__rt_pma_cap_finish_free_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", group_rows_off)); // reload the group-row table
    emitter.bl_c("free");                                                       // release the PATTERN_ORDER pointer table
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", group_rows_off)); // forget the freed table pointer
    emitter.label("__rt_pma_cap_finish_skip_linux_x86_64");
}

/// Emits x86_64 code that stamps an indexed array as boxed-Mixed slots.
fn emit_stamp_indexed_array_mixed_x86_64(emitter: &mut Emitter, array_reg: &str) {
    emitter.instruction(&format!("mov r10, QWORD PTR [{} - 8]", array_reg));    // load indexed-array packed kind word
    emitter.instruction(&format!(
        "mov r8, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(0x80ff)
    )); // preserve heap magic, indexed kind, and COW flag
    emitter.instruction("and r10, r8");                                         // clear stale value_type bits
    emitter.instruction("or r10, 0x700");                                       // stamp runtime value_type 7 = boxed Mixed
    emitter.instruction(&format!("mov QWORD PTR [{} - 8], r10", array_reg));    // store boxed-Mixed indexed-array metadata
}
