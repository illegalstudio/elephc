//! Purpose:
//! Emits the `__rt_preg_split`, `__rt_preg_strip` runtime helper assembly for preg split.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - Regex helpers preserve PHP PCRE-flavored inputs for PCRE2 and must preserve match array construction.
//! - Dynamic split flags force boxed Mixed result slots so offset-capture arrays never conflict with string-slot layout.

use crate::codegen_support::{emit::Emitter, platform::Arch};

const PREG_SPLIT_NO_EMPTY: i64 = 1;
const PREG_SPLIT_DELIM_CAPTURE: i64 = 2;
const PREG_SPLIT_OFFSET_CAPTURE: i64 = 4;
const PREG_SPLIT_FORCE_MIXED_RESULT: i64 = 1 << 30;

/// Emits the `__rt_preg_split` runtime helper.
///
/// Dispatches to the x86_64 Linux implementation or runs the generic ARM64 path.
/// The helper accepts a PHP PCRE-flavored pattern, subject, limit, and preg_split
/// flags. It strips slash delimiters via `__rt_preg_strip`, materializes the
/// PCRE pattern via `__rt_pcre_to_posix`, compiles through the opaque shim, then
/// executes it repeatedly to extract pre-match segments and optional captured
/// delimiters. Offset capture returns boxed rows shaped as `[string, offset]`.
///
/// ARM64 input: x1=pattern ptr, x2=pattern len, x3=subject ptr, x4=subject len,
/// x5=limit, x6=flags. ARM64 output: x0=array pointer.
pub(crate) fn emit_preg_split(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_split_linux_x86_64(emitter);
        return;
    }

    let handle_off = 0;
    let regmatches_ptr_off = handle_off + 8;
    let nmatch_off = regmatches_ptr_off + 8;
    let pattern_ptr_off = nmatch_off + 8;
    let pattern_len_off = pattern_ptr_off + 8;
    let subject_ptr_off = pattern_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let limit_off = subject_len_off + 8;
    let preg_flags_off = limit_off + 8;
    let regex_flags_off = preg_flags_off + 8;
    let pattern_cstr_off = regex_flags_off + 8;
    let array_ptr_off = pattern_cstr_off + 8;
    let subject_cstr_off = array_ptr_off + 8;
    let current_cstr_off = subject_cstr_off + 8;
    let current_elephc_off = current_cstr_off + 8;
    let split_count_off = current_elephc_off + 8;
    let piece_ptr_off = split_count_off + 8;
    let piece_len_off = piece_ptr_off + 8;
    let piece_offset_off = piece_len_off + 8;
    let pair_ptr_off = piece_offset_off + 8;
    let mixed_ptr_off = pair_ptr_off + 8;
    let capture_idx_off = mixed_ptr_off + 8;
    let stack_size = (capture_idx_off + 32 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_split ---");
    emitter.label_global("__rt_preg_split");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_split stack frame
    emitter.instruction(&format!("str x29, [sp, #{}]", save_off));              // save frame pointer in the large preg_split frame
    emitter.instruction(&format!("str x30, [sp, #{}]", save_off + 8));          // save return address in the large preg_split frame
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // set new frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // preserve the source regex pattern pointer
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // preserve the source regex pattern length
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // preserve the elephc subject pointer
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // preserve the elephc subject length
    emitter.instruction(&format!("str x5, [sp, #{}]", limit_off));              // preserve the PHP split limit
    emitter.instruction(&format!("str x6, [sp, #{}]", preg_flags_off));         // preserve the PHP preg_split flags

    // -- strip delimiters --
    emitter.instruction("bl __rt_preg_strip");                                  // strip slash delimiters and return regex flags in x3
    emitter.instruction(&format!("str x3, [sp, #{}]", regex_flags_off));        // save regex compilation flags

    // -- materialize the PCRE pattern as a C string --
    emitter.instruction("bl __rt_pcre_to_posix");                               // materialize PCRE pattern as a C string
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off));       // save null-terminated PCRE pattern

    // -- prepare locale state for regex helpers --
    super::emit_prepare_regex_locale(emitter);

    // -- compile regex --
    emitter.instruction(&format!("add x0, sp, #{}", handle_off));               // pass opaque-handle output storage
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off));       // pass null-terminated PCRE pattern
    emitter.instruction(&format!("ldr x2, [sp, #{}]", regex_flags_off));        // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("add x3, sp, #{}", nmatch_off));               // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile");                                    // compile without exposing PCRE2-owned layouts
    emitter.instruction("cbnz x0, __rt_preg_split_fail");                       // return an empty result array when compilation fails

    // -- allocate a reusable fixed offset-pair buffer --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", nmatch_off));             // load match-slot count returned by the shim
    emitter.instruction("lsr x10, x9, #60");                                    // reject slot counts whose 16-byte size would overflow
    emitter.instruction("cbnz x10, __rt_preg_split_malloc_fail");               // free the handle instead of allocating a wrapped size
    emitter.instruction("lsl x0, x9, #4");                                      // allocate one 16-byte signed-64-bit pair per slot
    emitter.bl_c("malloc");                                                     // allocate the fixed offset-pair vector
    emitter.instruction("cbz x0, __rt_preg_split_malloc_fail");                 // allocation failure frees the handle and returns an empty array
    emitter.instruction(&format!("str x0, [sp, #{}]", regmatches_ptr_off));     // save dynamic offset-pair buffer pointer

    // -- create result array with the required runtime element layout --
    emit_preg_split_alloc_result_arm64(emitter, "main", preg_flags_off, array_ptr_off);

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // load elephc subject pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // load elephc subject length
    emitter.instruction("bl __rt_cstr2");                                       // materialize a null-terminated subject copy
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string

    // -- initialize positions --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_cstr_off));       // load the subject C-string start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_cstr_off));       // initialize the C-string cursor
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_ptr_off));        // load the elephc subject start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_elephc_off));     // initialize the elephc payload cursor
    emitter.instruction(&format!("str xzr, [sp, #{}]", split_count_off));       // initialize the processed separator count

    // -- split loop --
    emitter.label("__rt_preg_split_loop");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", limit_off));              // reload PHP split limit
    emitter.instruction("cmp x9, #0");                                          // non-positive limits mean unlimited splitting
    emitter.instruction("b.le __rt_preg_split_limit_ok");                       // skip the split-count check for unlimited splitting
    emitter.instruction("sub x9, x9, #1");                                      // compute max separators to process for the requested limit
    emitter.instruction(&format!("ldr x10, [sp, #{}]", split_count_off));       // reload processed separator count
    emitter.instruction("cmp x10, x9");                                         // has the positive split limit already been reached?
    emitter.instruction("b.ge __rt_preg_split_last");                           // emit the unsplit remainder as the final element
    emitter.label("__rt_preg_split_limit_ok");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_cstr_off));       // load current C-string cursor
    emitter.instruction("ldrb w9, [x1]");                                       // inspect current subject byte
    emitter.instruction("cbz w9, __rt_preg_split_last");                        // end of string means only the trailing segment remains

    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // pass compiled opaque handle
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_cstr_off));       // pass current C-string cursor
    emitter.instruction(&format!("ldr x2, [sp, #{}]", nmatch_off));             // request one regmatch slot for every compiled capture group
    emitter.instruction(&format!("ldr x3, [sp, #{}]", regmatches_ptr_off));     // pass dynamic fixed offset-pair buffer
    emitter.instruction("mov x4, #0");                                          // use default execution flags
    emitter.bl_c("elephc_pcre2_v1_exec");                                       // execute and initialize every requested offset pair
    emitter.instruction("cbnz x0, __rt_preg_split_last");                       // no more matches means the trailing segment remains

    // -- add segment before match to array --
    emitter.instruction(&format!("ldr x14, [sp, #{}]", regmatches_ptr_off));    // load dynamic full-match pair for the pre-match extent
    emitter.instruction("ldr x9, [x14]");                                       // load full-match signed-64-bit start
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_elephc_off));     // load pre-match segment start
    emitter.instruction("mov x2, x9");                                          // use rm_so as the pre-match segment length
    emitter.instruction(&format!("ldr x3, [sp, #{}]", current_elephc_off));     // reload segment start for offset calculation
    emitter.instruction(&format!("ldr x10, [sp, #{}]", subject_ptr_off));       // load original subject start
    emitter.instruction("sub x3, x3, x10");                                     // compute absolute byte offset of the segment
    emit_preg_split_push_piece_arm64(
        emitter,
        "segment",
        preg_flags_off,
        array_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );

    emit_preg_split_capture_loop_arm64(
        emitter,
        regmatches_ptr_off,
        nmatch_off,
        preg_flags_off,
        subject_ptr_off,
        current_elephc_off,
        array_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
        capture_idx_off,
    );

    // -- count this separator and advance past match --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", split_count_off));        // reload processed separator count
    emitter.instruction("add x9, x9, #1");                                      // account for the separator just processed
    emitter.instruction(&format!("str x9, [sp, #{}]", split_count_off));        // save updated separator count
    emitter.instruction(&format!("ldr x14, [sp, #{}]", regmatches_ptr_off));    // load dynamic full-match pair for cursor advancement
    emitter.instruction("ldr x9, [x14, #8]");                                   // load full-match signed-64-bit end
    emitter.instruction("cmp x9, #0");                                          // detect zero-length separators
    emitter.instruction("b.gt __rt_preg_split_advance_ok");                     // trust rm_eo when the separator consumed bytes
    emitter.instruction("mov x9, #1");                                          // force progress for zero-length matches
    emitter.label("__rt_preg_split_advance_ok");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_cstr_off));      // reload current C-string cursor
    emitter.instruction("add x10, x10, x9");                                    // advance C-string cursor past separator
    emitter.instruction(&format!("str x10, [sp, #{}]", current_cstr_off));      // save advanced C-string cursor
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_elephc_off));    // reload current elephc payload cursor
    emitter.instruction("add x10, x10, x9");                                    // advance elephc cursor by the same byte distance
    emitter.instruction(&format!("str x10, [sp, #{}]", current_elephc_off));    // save advanced elephc cursor
    emitter.instruction("b __rt_preg_split_loop");                              // continue splitting the remaining subject

    // -- add last segment --
    emitter.label("__rt_preg_split_last");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_elephc_off));     // load trailing segment start
    emitter.instruction(&format!("ldr x10, [sp, #{}]", subject_ptr_off));       // load original subject start
    emitter.instruction(&format!("ldr x11, [sp, #{}]", subject_len_off));       // load original subject length
    emitter.instruction("add x11, x10, x11");                                   // compute end address of original subject
    emitter.instruction("sub x2, x11, x1");                                     // compute trailing segment length
    emitter.instruction("sub x3, x1, x10");                                     // compute trailing segment byte offset
    emit_preg_split_push_piece_arm64(
        emitter,
        "last",
        preg_flags_off,
        array_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );

    // -- free regex and return --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regmatches_ptr_off));     // reload dynamic capture buffer for cleanup
    emitter.bl_c("free");                                                       // release the reusable offset-pair vector
    emitter.instruction(&format!("ldr x0, [sp, #{}]", array_ptr_off));          // reload result array pointer
    emitter.instruction("b __rt_preg_split_ret");                               // return through common epilogue

    // -- failure: return empty array with the same layout the successful path would use --
    emitter.label("__rt_preg_split_fail");
    emit_preg_split_alloc_result_arm64(emitter, "fail", preg_flags_off, array_ptr_off);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", array_ptr_off));          // reload empty result array pointer
    emitter.instruction("b __rt_preg_split_ret");                               // return through common epilogue

    emitter.label("__rt_preg_split_malloc_fail");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // reload opaque handle after allocation failure
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release regex resources before returning an empty array
    emit_preg_split_alloc_result_arm64(emitter, "malloc_fail", preg_flags_off, array_ptr_off);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", array_ptr_off));          // reload empty result array pointer after allocation failure

    emitter.label("__rt_preg_split_ret");
    emitter.instruction(&format!("ldr x29, [sp, #{}]", save_off));              // restore frame pointer from the large preg_split frame
    emitter.instruction(&format!("ldr x30, [sp, #{}]", save_off + 8));          // restore return address from the large preg_split frame
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate preg_split stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the ARM64 result-array allocation path for preg_split.
fn emit_preg_split_alloc_result_arm64(
    emitter: &mut Emitter,
    suffix: &str,
    preg_flags_off: usize,
    array_ptr_off: usize,
) {
    let mixed = format!("__rt_preg_split_alloc_mixed_{suffix}");
    let done = format!("__rt_preg_split_alloc_done_{suffix}");

    emitter.instruction(&format!("ldr x9, [sp, #{}]", preg_flags_off));         // reload preg_split flags before choosing result element layout
    emitter.instruction(&format!("tst x9, #{}", PREG_SPLIT_OFFSET_CAPTURE));    // offset capture requires boxed Mixed rows
    emitter.instruction(&format!("b.ne {mixed}"));                              // allocate Mixed slots for offset-capture results
    emitter.instruction("mov x10, #1");                                         // prepare the internal force-Mixed bit
    emitter.instruction("lsl x10, x10, #30");                                   // materialize PREG_SPLIT_FORCE_MIXED_RESULT
    emitter.instruction("tst x9, x10");                                         // dynamic flags force Mixed slots even without offset capture
    emitter.instruction(&format!("b.ne {mixed}"));                              // allocate Mixed slots for dynamic flag calls
    emitter.instruction("mov x0, #8");                                          // initial string-result capacity
    emitter.instruction("mov x1, #16");                                         // string result slots store ptr/len pairs
    emitter.instruction("bl __rt_array_new");                                   // allocate string result array
    emitter.instruction(&format!("str x0, [sp, #{}]", array_ptr_off));          // save result array pointer
    emitter.instruction(&format!("b {done}"));                                  // skip Mixed metadata stamping
    emitter.label(&mixed);
    emitter.instruction("mov x0, #8");                                          // initial Mixed-result capacity
    emitter.instruction("mov x1, #8");                                          // Mixed result slots store boxed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate Mixed result array
    emit_stamp_indexed_array_mixed_arm64(emitter, "x0");
    emitter.instruction(&format!("str x0, [sp, #{}]", array_ptr_off));          // save result array pointer
    emitter.label(&done);
}

/// Emits ARM64 code that appends one split piece using the currently saved flags.
#[allow(clippy::too_many_arguments)]
fn emit_preg_split_push_piece_arm64(
    emitter: &mut Emitter,
    suffix: &str,
    preg_flags_off: usize,
    array_ptr_off: usize,
    piece_ptr_off: usize,
    piece_len_off: usize,
    piece_offset_off: usize,
    pair_ptr_off: usize,
    mixed_ptr_off: usize,
) {
    let keep = format!("__rt_preg_split_push_keep_{suffix}");
    let boxed = format!("__rt_preg_split_push_boxed_{suffix}");
    let offset = format!("__rt_preg_split_push_offset_{suffix}");
    let done = format!("__rt_preg_split_push_done_{suffix}");

    emitter.instruction(&format!("str x1, [sp, #{}]", piece_ptr_off));          // save split piece pointer across append helpers
    emitter.instruction(&format!("str x2, [sp, #{}]", piece_len_off));          // save split piece length across append helpers
    emitter.instruction(&format!("str x3, [sp, #{}]", piece_offset_off));       // save split piece absolute offset across append helpers
    emitter.instruction(&format!("ldr x9, [sp, #{}]", preg_flags_off));         // reload preg_split flags for no-empty filtering
    emitter.instruction(&format!("tst x9, #{}", PREG_SPLIT_NO_EMPTY));          // is PREG_SPLIT_NO_EMPTY enabled?
    emitter.instruction(&format!("b.eq {keep}"));                               // keep empty strings when no-empty filtering is disabled
    emitter.instruction(&format!("cbz x2, {done}"));                            // skip this piece when no-empty filtering removes it
    emitter.label(&keep);
    emitter.instruction(&format!("ldr x9, [sp, #{}]", preg_flags_off));         // reload preg_split flags for result-shape selection
    emitter.instruction(&format!("tst x9, #{}", PREG_SPLIT_OFFSET_CAPTURE));    // does this piece need an offset-capture row?
    emitter.instruction(&format!("b.ne {offset}"));                             // build [string, offset] when offset capture is enabled
    emitter.instruction("mov x10, #1");                                         // prepare the internal force-Mixed bit
    emitter.instruction("lsl x10, x10, #30");                                   // materialize PREG_SPLIT_FORCE_MIXED_RESULT
    emitter.instruction("tst x9, x10");                                         // do dynamic flags require boxed string pieces?
    emitter.instruction(&format!("b.ne {boxed}"));                              // box plain strings for Mixed-layout result arrays
    emitter.instruction(&format!("ldr x0, [sp, #{}]", array_ptr_off));          // reload string result array pointer
    emitter.instruction(&format!("ldr x1, [sp, #{}]", piece_ptr_off));          // reload split piece pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", piece_len_off));          // reload split piece length
    emitter.instruction("bl __rt_array_push_str");                              // append a plain string piece
    emitter.instruction(&format!("str x0, [sp, #{}]", array_ptr_off));          // save possibly-grown result array pointer
    emitter.instruction(&format!("b {done}"));                                  // finish this append

    emitter.label(&boxed);
    emit_box_saved_piece_string_arm64(emitter, piece_ptr_off, piece_len_off, mixed_ptr_off);
    emit_push_saved_mixed_piece_arm64(emitter, array_ptr_off, mixed_ptr_off);
    emitter.instruction(&format!("b {done}"));                                  // finish boxed-string append

    emitter.label(&offset);
    emit_build_offset_capture_row_arm64(
        emitter,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );
    emit_push_saved_mixed_piece_arm64(emitter, array_ptr_off, mixed_ptr_off);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", pair_ptr_off));           // reload temporary offset-capture row array
    emitter.instruction("bl __rt_decref_array");                                // drop the helper's owner now that the boxed row retained it
    emitter.label(&done);
}

/// Emits ARM64 code that boxes a saved string piece as Mixed.
fn emit_box_saved_piece_string_arm64(
    emitter: &mut Emitter,
    piece_ptr_off: usize,
    piece_len_off: usize,
    mixed_ptr_off: usize,
) {
    emitter.instruction("mov x0, #1");                                          // runtime value tag 1 = string
    emitter.instruction(&format!("ldr x1, [sp, #{}]", piece_ptr_off));          // load string payload pointer for boxing
    emitter.instruction(&format!("ldr x2, [sp, #{}]", piece_len_off));          // load string payload length for boxing
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the string piece
    emitter.instruction(&format!("str x0, [sp, #{}]", mixed_ptr_off));          // save boxed string Mixed pointer
}

/// Emits ARM64 code that appends a saved Mixed pointer to the result array.
fn emit_push_saved_mixed_piece_arm64(
    emitter: &mut Emitter,
    array_ptr_off: usize,
    mixed_ptr_off: usize,
) {
    emitter.instruction(&format!("ldr x0, [sp, #{}]", array_ptr_off));          // reload Mixed result array pointer
    emitter.instruction(&format!("ldr x1, [sp, #{}]", mixed_ptr_off));          // reload boxed Mixed piece pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append and retain the boxed Mixed piece
    emitter.instruction(&format!("str x0, [sp, #{}]", array_ptr_off));          // save possibly-grown result array pointer
    emitter.instruction(&format!("ldr x0, [sp, #{}]", mixed_ptr_off));          // reload helper-owned boxed Mixed piece
    emitter.instruction("bl __rt_decref_mixed");                                // drop helper ownership after the array retained the Mixed cell
}

/// Emits ARM64 code that builds a boxed `[string, offset]` row for offset capture.
fn emit_build_offset_capture_row_arm64(
    emitter: &mut Emitter,
    piece_ptr_off: usize,
    piece_len_off: usize,
    piece_offset_off: usize,
    pair_ptr_off: usize,
    mixed_ptr_off: usize,
) {
    emitter.instruction("mov x0, #2");                                          // capacity for [string, offset]
    emitter.instruction("mov x1, #8");                                          // row stores boxed Mixed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate offset-capture row
    emit_stamp_indexed_array_mixed_arm64(emitter, "x0");
    emitter.instruction(&format!("str x0, [sp, #{}]", pair_ptr_off));           // save row array pointer
    emit_box_saved_piece_string_arm64(emitter, piece_ptr_off, piece_len_off, mixed_ptr_off);
    emitter.instruction(&format!("ldr x9, [sp, #{}]", pair_ptr_off));           // reload row array pointer for string cell store
    emitter.instruction(&format!("ldr x10, [sp, #{}]", mixed_ptr_off));         // reload boxed string cell
    emitter.instruction("str x10, [x9, #24]");                                  // store row[0] = boxed string
    emitter.instruction("mov x11, #1");                                         // row length after storing the string cell
    emitter.instruction("str x11, [x9]");                                       // publish row length 1
    emitter.instruction("mov x0, #0");                                          // runtime value tag 0 = integer
    emitter.instruction(&format!("ldr x1, [sp, #{}]", piece_offset_off));       // load absolute byte offset for boxing
    emitter.instruction("mov x2, xzr");                                         // integer payload has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the integer offset
    emitter.instruction(&format!("ldr x9, [sp, #{}]", pair_ptr_off));           // reload row array pointer for offset cell store
    emitter.instruction("str x0, [x9, #32]");                                   // store row[1] = boxed offset
    emitter.instruction("mov x11, #2");                                         // row length after storing both cells
    emitter.instruction("str x11, [x9]");                                       // publish row length 2
    emitter.instruction("mov x0, #4");                                          // runtime value tag 4 = indexed array
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pair_ptr_off));           // load row array pointer for boxing
    emitter.instruction("mov x2, xzr");                                         // indexed-array payload has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the row array as Mixed
    emitter.instruction(&format!("str x0, [sp, #{}]", mixed_ptr_off));          // save boxed row Mixed pointer
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

/// Emits the ARM64 capture-loop block for delimiter-capture split flags.
#[allow(clippy::too_many_arguments)]
fn emit_preg_split_capture_loop_arm64(
    emitter: &mut Emitter,
    regmatches_ptr_off: usize,
    nmatch_off: usize,
    preg_flags_off: usize,
    subject_ptr_off: usize,
    current_elephc_off: usize,
    array_ptr_off: usize,
    piece_ptr_off: usize,
    piece_len_off: usize,
    piece_offset_off: usize,
    pair_ptr_off: usize,
    mixed_ptr_off: usize,
    capture_idx_off: usize,
) {
    emitter.instruction(&format!("ldr x9, [sp, #{}]", preg_flags_off));         // reload preg_split flags before capture appends
    emitter.instruction(&format!("tst x9, #{}", PREG_SPLIT_DELIM_CAPTURE));     // is PREG_SPLIT_DELIM_CAPTURE enabled?
    emitter.instruction("b.eq __rt_preg_split_captures_done");                  // skip delimiter captures when the flag is absent
    emitter.instruction("mov x9, #1");                                          // start at capture group 1, after the full match
    emitter.instruction(&format!("str x9, [sp, #{}]", capture_idx_off));        // save current capture-group index
    emitter.label("__rt_preg_split_capture_loop");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", capture_idx_off));        // reload current capture-group index
    emitter.instruction(&format!("ldr x10, [sp, #{}]", nmatch_off));            // reload dynamic regmatch slot count
    emitter.instruction("cmp x9, x10");                                         // stop after all compiled capture groups
    emitter.instruction("b.ge __rt_preg_split_captures_done");                  // finish capture processing
    emitter.instruction(&format!("ldr x11, [sp, #{}]", regmatches_ptr_off));    // load dynamic offset-pair buffer base
    emitter.instruction("lsl x10, x9, #4");                                     // scale capture index by the fixed 16-byte pair stride
    emitter.instruction("add x11, x11, x10");                                   // address this capture group's offset pair
    emitter.instruction("ldr x12, [x11]");                                      // load signed-64-bit capture start
    emitter.instruction("cmp x12, #0");                                         // unmatched captures have negative rm_so
    emitter.instruction("b.lt __rt_preg_split_capture_next");                   // skip unmatched capture groups
    emitter.instruction("ldr x13, [x11, #8]");                                  // load signed-64-bit capture end
    emitter.instruction("sub x2, x13, x12");                                    // compute captured delimiter length
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_elephc_off));     // load current elephc cursor
    emitter.instruction("add x1, x1, x12");                                     // compute captured delimiter pointer
    emitter.instruction(&format!("ldr x3, [sp, #{}]", current_elephc_off));     // reload current elephc cursor for offset calculation
    emitter.instruction(&format!("ldr x10, [sp, #{}]", subject_ptr_off));       // load original subject start
    emitter.instruction("sub x3, x3, x10");                                     // compute current cursor absolute offset
    emitter.instruction("add x3, x3, x12");                                     // add capture-local rm_so to get capture offset
    emit_preg_split_push_piece_arm64(
        emitter,
        "capture",
        preg_flags_off,
        array_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );
    emitter.label("__rt_preg_split_capture_next");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", capture_idx_off));        // reload capture-group index
    emitter.instruction("add x9, x9, #1");                                      // advance to next capture group
    emitter.instruction(&format!("str x9, [sp, #{}]", capture_idx_off));        // save next capture-group index
    emitter.instruction("b __rt_preg_split_capture_loop");                      // continue capture processing
    emitter.label("__rt_preg_split_captures_done");
}

/// Emits the x86_64 Linux-specific `__rt_preg_split` runtime helper.
///
/// Uses the System V AMD64 ABI: pattern ptr/len in rdi/rsi, subject ptr/len in
/// rdx/rcx, limit in r8, flags in r9. The helper strips delimiters, translates
/// PCRE pattern as a C string, compiles through the opaque shim, then iterates it to
/// collect pre-match segments, optional delimiter captures, and optional
/// offset-capture rows. Zero-length matches advance by one byte to avoid
/// infinite loops. On failure returns a small empty array with the requested
/// result layout.
fn emit_preg_split_linux_x86_64(emitter: &mut Emitter) {
    let handle_off = 0;
    let regmatches_ptr_off = handle_off + 8;
    let nmatch_off = regmatches_ptr_off + 8;
    let subject_ptr_off = nmatch_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let limit_off = subject_len_off + 8;
    let preg_flags_off = limit_off + 8;
    let regex_flags_off = preg_flags_off + 8;
    let pattern_cstr_off = regex_flags_off + 8;
    let array_ptr_off = pattern_cstr_off + 8;
    let subject_cstr_off = array_ptr_off + 8;
    let current_cstr_off = subject_cstr_off + 8;
    let current_elephc_off = current_cstr_off + 8;
    let split_count_off = current_elephc_off + 8;
    let piece_ptr_off = split_count_off + 8;
    let piece_len_off = piece_ptr_off + 8;
    let piece_offset_off = piece_len_off + 8;
    let pair_ptr_off = piece_offset_off + 8;
    let mixed_ptr_off = pair_ptr_off + 8;
    let capture_idx_off = mixed_ptr_off + 8;
    let stack_size = (capture_idx_off + 16 + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: preg_split ---");
    emitter.label_global("__rt_preg_split");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving regex-split scratch storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for regex object and split bookkeeping
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve aligned local storage for the handle, offset pairs, and split spills
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", subject_ptr_off)); // preserve the elephc subject pointer across helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", subject_len_off)); // preserve the elephc subject length across helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r8", limit_off));   // preserve the PHP split limit
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", preg_flags_off)); // preserve the PHP preg_split flags
    emitter.instruction("mov rax, rdi");                                        // move pattern pointer into preg-strip input register
    emitter.instruction("mov rdx, rsi");                                        // move pattern length into preg-strip input register
    emitter.instruction("call __rt_preg_strip");                                // strip slash delimiters and gather supported regex flags
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", regex_flags_off)); // preserve delimiter-strip regex flags
    emitter.instruction("call __rt_pcre_to_posix");                             // materialize PCRE pattern as a C string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // preserve null-terminated PCRE pattern
    super::emit_prepare_regex_locale(emitter);
    emitter.instruction(&format!("lea rdi, [rsp + {}]", handle_off));           // pass opaque-handle output storage
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass null-terminated PCRE pattern to PCRE2
    emitter.instruction(&format!("mov edx, DWORD PTR [rsp + {}]", regex_flags_off)); // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("lea rcx, [rsp + {}]", nmatch_off));           // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile");                                    // compile without exposing PCRE2-owned layouts
    emitter.instruction("test eax, eax");                                       // did regex compilation succeed?
    emitter.instruction("jnz __rt_preg_split_fail_linux_x86_64");               // return an empty result array on compilation failure

    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", nmatch_off)); // load match-slot count returned by the shim
    emitter.instruction("mov r10, rdi");                                        // copy slot count for allocation-overflow validation
    emitter.instruction("shr r10, 60");                                         // detect a wrapped 16-byte pair-vector size
    emitter.instruction("jnz __rt_preg_split_malloc_fail_linux_x86_64");        // free the handle instead of allocating a wrapped size
    emitter.instruction("shl rdi, 4");                                          // allocate one 16-byte signed-64-bit pair per slot
    emitter.bl_c("malloc");                                                     // allocate the fixed offset-pair vector
    emitter.instruction("test rax, rax");                                       // did malloc return a capture buffer?
    emitter.instruction("jz __rt_preg_split_malloc_fail_linux_x86_64");         // allocation failure frees the opaque handle and returns an empty array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", regmatches_ptr_off)); // save dynamic offset-pair buffer pointer

    emit_preg_split_alloc_result_x86_64(emitter, "main", preg_flags_off, array_ptr_off);
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload elephc subject pointer before C-string conversion
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload elephc subject length before C-string conversion
    emitter.instruction("call __rt_cstr2");                                     // materialize a null-terminated subject copy
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // save subject C string pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", current_cstr_off)); // initialize C-string cursor
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload original elephc subject pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", current_elephc_off)); // initialize elephc payload cursor
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", split_count_off)); // initialize processed separator count

    emitter.label("__rt_preg_split_loop_linux_x86_64");
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", limit_off));   // reload PHP split limit
    emitter.instruction("cmp r9, 0");                                           // non-positive limits mean unlimited splitting
    emitter.instruction("jle __rt_preg_split_limit_ok_linux_x86_64");           // skip the split-count check for unlimited splitting
    emitter.instruction("sub r9, 1");                                           // compute max separators to process for the requested limit
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", split_count_off)); // reload processed separator count
    emitter.instruction("cmp r10, r9");                                         // has the positive split limit already been reached?
    emitter.instruction("jge __rt_preg_split_last_linux_x86_64");               // emit the unsplit remainder as the final element
    emitter.label("__rt_preg_split_limit_ok_linux_x86_64");
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_cstr_off)); // reload current C-string cursor
    emitter.instruction("movzx r9d, BYTE PTR [rsi]");                           // inspect current subject byte
    emitter.instruction("test r9d, r9d");                                       // is the current byte the trailing null terminator?
    emitter.instruction("jz __rt_preg_split_last_linux_x86_64");                // emit the final segment at end of string
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // pass compiled opaque handle
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_cstr_off)); // pass current C-string cursor to regexec
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", nmatch_off)); // request one regmatch slot for every compiled capture group
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // pass dynamic fixed offset-pair buffer
    emitter.instruction("xor r8d, r8d");                                        // use default execution flags
    emitter.bl_c("elephc_pcre2_v1_exec");                                       // execute and initialize every requested offset pair
    emitter.instruction("test eax, eax");                                       // did regexec find another separator?
    emitter.instruction("jnz __rt_preg_split_last_linux_x86_64");               // no more matches means the trailing segment remains

    emitter.instruction(&format!("mov r12, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic full-match pair for the pre-match extent
    emitter.instruction("mov r9, QWORD PTR [r12]");                             // load full-match signed-64-bit start
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_elephc_off)); // load pre-match segment start
    emitter.instruction("mov rdx, r9");                                         // use rm_so as the pre-match segment length
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", current_elephc_off)); // reload current elephc cursor for offset calculation
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", subject_ptr_off)); // load original subject start
    emitter.instruction("sub rcx, r10");                                        // compute absolute byte offset of the segment
    emit_preg_split_push_piece_x86_64(
        emitter,
        "segment",
        preg_flags_off,
        array_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );

    emit_preg_split_capture_loop_x86_64(
        emitter,
        regmatches_ptr_off,
        nmatch_off,
        preg_flags_off,
        subject_ptr_off,
        current_elephc_off,
        array_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
        capture_idx_off,
    );

    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", split_count_off)); // reload processed separator count
    emitter.instruction("add r9, 1");                                           // account for the separator just processed
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", split_count_off)); // save updated separator count
    emitter.instruction(&format!("mov r12, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic full-match pair for cursor advancement
    emitter.instruction("mov r9, QWORD PTR [r12 + 8]");                         // load full-match signed-64-bit end
    emitter.instruction("cmp r9, 0");                                           // detect zero-length separators
    emitter.instruction("jg __rt_preg_split_advance_ok_linux_x86_64");          // trust rm_eo when the separator consumed bytes
    emitter.instruction("mov r9, 1");                                           // force progress for zero-length matches
    emitter.label("__rt_preg_split_advance_ok_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_cstr_off)); // reload current C-string cursor
    emitter.instruction("add r10, r9");                                         // advance C-string cursor past separator
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", current_cstr_off)); // save advanced C-string cursor
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_elephc_off)); // reload current elephc payload cursor
    emitter.instruction("add r10, r9");                                         // advance elephc cursor by the same byte distance
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", current_elephc_off)); // save advanced elephc cursor
    emitter.instruction("jmp __rt_preg_split_loop_linux_x86_64");               // continue splitting the remaining subject

    emitter.label("__rt_preg_split_last_linux_x86_64");
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_elephc_off)); // load trailing segment start
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", subject_ptr_off)); // load original subject start
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", subject_len_off)); // load original subject length
    emitter.instruction("add r11, r10");                                        // compute end address of original subject
    emitter.instruction("mov rdx, r11");                                        // seed trailing length from subject end
    emitter.instruction("sub rdx, rsi");                                        // compute trailing segment length
    emitter.instruction("mov rcx, rsi");                                        // copy trailing segment start for offset calculation
    emitter.instruction("sub rcx, r10");                                        // compute trailing segment byte offset
    emit_preg_split_push_piece_x86_64(
        emitter,
        "last",
        preg_flags_off,
        array_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // reload dynamic capture buffer for cleanup
    emitter.bl_c("free");                                                       // release the reusable offset-pair vector
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", array_ptr_off)); // return final result array pointer
    emitter.instruction("jmp __rt_preg_split_ret_linux_x86_64");                // share common epilogue

    emitter.label("__rt_preg_split_fail_linux_x86_64");
    emit_preg_split_alloc_result_x86_64(emitter, "fail", preg_flags_off, array_ptr_off);
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", array_ptr_off)); // return empty result array pointer
    emitter.instruction("jmp __rt_preg_split_ret_linux_x86_64");                // return through common epilogue

    emitter.label("__rt_preg_split_malloc_fail_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // reload opaque handle after allocation failure
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources before returning empty
    emit_preg_split_alloc_result_x86_64(emitter, "malloc_fail", preg_flags_off, array_ptr_off);
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", array_ptr_off)); // return empty result array pointer after allocation failure

    emitter.label("__rt_preg_split_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release opaque-handle, offset-pair, and split spill storage
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return preg_split result in rax
}

/// Emits x86_64 result-array allocation for preg_split.
fn emit_preg_split_alloc_result_x86_64(
    emitter: &mut Emitter,
    suffix: &str,
    preg_flags_off: usize,
    array_ptr_off: usize,
) {
    let mixed = format!("__rt_preg_split_alloc_mixed_{suffix}_linux_x86_64");
    let done = format!("__rt_preg_split_alloc_done_{suffix}_linux_x86_64");

    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", preg_flags_off)); // reload preg_split flags before choosing result element layout
    emitter.instruction(&format!("test r9, {}", PREG_SPLIT_OFFSET_CAPTURE));    // offset capture requires boxed Mixed rows
    emitter.instruction(&format!("jnz {mixed}"));                               // allocate Mixed slots for offset-capture results
    emitter.instruction(&format!("mov r10, {}", PREG_SPLIT_FORCE_MIXED_RESULT)); // materialize the internal force-Mixed bit
    emitter.instruction("test r9, r10");                                        // dynamic flags force Mixed slots even without offset capture
    emitter.instruction(&format!("jnz {mixed}"));                               // allocate Mixed slots for dynamic flag calls
    emitter.instruction("mov edi, 8");                                          // initial string-result capacity
    emitter.instruction("mov esi, 16");                                         // string result slots store ptr/len pairs
    emitter.instruction("call __rt_array_new");                                 // allocate string result array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", array_ptr_off)); // save result array pointer
    emitter.instruction(&format!("jmp {done}"));                                // skip Mixed metadata stamping
    emitter.label(&mixed);
    emitter.instruction("mov edi, 8");                                          // initial Mixed-result capacity
    emitter.instruction("mov esi, 8");                                          // Mixed result slots store boxed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate Mixed result array
    emit_stamp_indexed_array_mixed_x86_64(emitter, "rax");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", array_ptr_off)); // save result array pointer
    emitter.label(&done);
}

/// Emits x86_64 code that appends one split piece using the currently saved flags.
#[allow(clippy::too_many_arguments)]
fn emit_preg_split_push_piece_x86_64(
    emitter: &mut Emitter,
    suffix: &str,
    preg_flags_off: usize,
    array_ptr_off: usize,
    piece_ptr_off: usize,
    piece_len_off: usize,
    piece_offset_off: usize,
    pair_ptr_off: usize,
    mixed_ptr_off: usize,
) {
    let keep = format!("__rt_preg_split_push_keep_{suffix}_linux_x86_64");
    let boxed = format!("__rt_preg_split_push_boxed_{suffix}_linux_x86_64");
    let offset = format!("__rt_preg_split_push_offset_{suffix}_linux_x86_64");
    let done = format!("__rt_preg_split_push_done_{suffix}_linux_x86_64");

    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rsi", piece_ptr_off)); // save split piece pointer across append helpers
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", piece_len_off)); // save split piece length across append helpers
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", piece_offset_off)); // save split piece absolute offset across append helpers
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", preg_flags_off)); // reload preg_split flags for no-empty filtering
    emitter.instruction(&format!("test r9, {}", PREG_SPLIT_NO_EMPTY));          // is PREG_SPLIT_NO_EMPTY enabled?
    emitter.instruction(&format!("jz {keep}"));                                 // keep empty strings when no-empty filtering is disabled
    emitter.instruction("test rdx, rdx");                                       // is this split piece empty?
    emitter.instruction(&format!("jz {done}"));                                 // skip empty pieces when no-empty filtering removes them
    emitter.label(&keep);
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", preg_flags_off)); // reload preg_split flags for result-shape selection
    emitter.instruction(&format!("test r9, {}", PREG_SPLIT_OFFSET_CAPTURE));    // does this piece need an offset-capture row?
    emitter.instruction(&format!("jnz {offset}"));                              // build [string, offset] when offset capture is enabled
    emitter.instruction(&format!("mov r10, {}", PREG_SPLIT_FORCE_MIXED_RESULT)); // materialize the internal force-Mixed bit
    emitter.instruction("test r9, r10");                                        // do dynamic flags require boxed string pieces?
    emitter.instruction(&format!("jnz {boxed}"));                               // box plain strings for Mixed-layout result arrays
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", array_ptr_off)); // reload string result array pointer
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", piece_ptr_off)); // reload split piece pointer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", piece_len_off)); // reload split piece length
    emitter.instruction("call __rt_array_push_str");                            // append a plain string piece
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", array_ptr_off)); // save possibly-grown result array pointer
    emitter.instruction(&format!("jmp {done}"));                                // finish this append

    emitter.label(&boxed);
    emit_box_saved_piece_string_x86_64(emitter, piece_ptr_off, piece_len_off, mixed_ptr_off);
    emit_push_saved_mixed_piece_x86_64(emitter, array_ptr_off, mixed_ptr_off);
    emitter.instruction(&format!("jmp {done}"));                                // finish boxed-string append

    emitter.label(&offset);
    emit_build_offset_capture_row_x86_64(
        emitter,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );
    emit_push_saved_mixed_piece_x86_64(emitter, array_ptr_off, mixed_ptr_off);
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", pair_ptr_off)); // reload temporary offset-capture row array
    emitter.instruction("call __rt_decref_array");                              // drop the helper's owner now that the boxed row retained it
    emitter.label(&done);
}

/// Emits x86_64 code that boxes a saved string piece as Mixed.
fn emit_box_saved_piece_string_x86_64(
    emitter: &mut Emitter,
    piece_ptr_off: usize,
    piece_len_off: usize,
    mixed_ptr_off: usize,
) {
    emitter.instruction("mov rax, 1");                                          // runtime value tag 1 = string
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", piece_ptr_off)); // load string payload pointer for boxing
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", piece_len_off)); // load string payload length for boxing
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the string piece
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", mixed_ptr_off)); // save boxed string Mixed pointer
}

/// Emits x86_64 code that appends a saved Mixed pointer to the result array.
fn emit_push_saved_mixed_piece_x86_64(
    emitter: &mut Emitter,
    array_ptr_off: usize,
    mixed_ptr_off: usize,
) {
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", array_ptr_off)); // reload Mixed result array pointer
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", mixed_ptr_off)); // reload boxed Mixed piece pointer
    emitter.instruction("call __rt_array_push_refcounted");                     // append and retain the boxed Mixed piece
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", array_ptr_off)); // save possibly-grown result array pointer
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", mixed_ptr_off)); // reload helper-owned boxed Mixed piece
    emitter.instruction("call __rt_decref_mixed");                              // drop helper ownership after the array retained the Mixed cell
}

/// Emits x86_64 code that builds a boxed `[string, offset]` row for offset capture.
fn emit_build_offset_capture_row_x86_64(
    emitter: &mut Emitter,
    piece_ptr_off: usize,
    piece_len_off: usize,
    piece_offset_off: usize,
    pair_ptr_off: usize,
    mixed_ptr_off: usize,
) {
    emitter.instruction("mov edi, 2");                                          // capacity for [string, offset]
    emitter.instruction("mov esi, 8");                                          // row stores boxed Mixed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate offset-capture row
    emit_stamp_indexed_array_mixed_x86_64(emitter, "rax");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pair_ptr_off)); // save row array pointer
    emit_box_saved_piece_string_x86_64(emitter, piece_ptr_off, piece_len_off, mixed_ptr_off);
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", pair_ptr_off)); // reload row array pointer for string cell store
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", mixed_ptr_off)); // reload boxed string cell
    emitter.instruction("mov QWORD PTR [r9 + 24], r10");                        // store row[0] = boxed string
    emitter.instruction("mov QWORD PTR [r9], 1");                               // publish row length 1
    emitter.instruction("xor eax, eax");                                        // runtime value tag 0 = integer
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", piece_offset_off)); // load absolute byte offset for boxing
    emitter.instruction("xor esi, esi");                                        // integer payload has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the integer offset
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", pair_ptr_off)); // reload row array pointer for offset cell store
    emitter.instruction("mov QWORD PTR [r9 + 32], rax");                        // store row[1] = boxed offset
    emitter.instruction("mov QWORD PTR [r9], 2");                               // publish row length 2
    emitter.instruction("mov rax, 4");                                          // runtime value tag 4 = indexed array
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", pair_ptr_off)); // load row array pointer for boxing
    emitter.instruction("xor esi, esi");                                        // indexed-array payload has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the row array as Mixed
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", mixed_ptr_off)); // save boxed row Mixed pointer
}

/// Emits x86_64 code that stamps an indexed array as boxed-Mixed slots.
fn emit_stamp_indexed_array_mixed_x86_64(emitter: &mut Emitter, array_reg: &str) {
    emitter.instruction(&format!("mov r10, QWORD PTR [{} - 8]", array_reg));    // load indexed-array packed kind word
    emitter.instruction(&format!("mov r8, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(0x80ff))); // preserve heap magic, indexed kind, and COW flag
    emitter.instruction("and r10, r8");                                         // clear stale value_type bits
    emitter.instruction("or r10, 0x700");                                       // stamp runtime value_type 7 = boxed Mixed
    emitter.instruction(&format!("mov QWORD PTR [{} - 8], r10", array_reg));    // store boxed-Mixed indexed-array metadata
}

/// Emits the x86_64 capture-loop block for delimiter-capture split flags.
#[allow(clippy::too_many_arguments)]
fn emit_preg_split_capture_loop_x86_64(
    emitter: &mut Emitter,
    regmatches_ptr_off: usize,
    nmatch_off: usize,
    preg_flags_off: usize,
    subject_ptr_off: usize,
    current_elephc_off: usize,
    array_ptr_off: usize,
    piece_ptr_off: usize,
    piece_len_off: usize,
    piece_offset_off: usize,
    pair_ptr_off: usize,
    mixed_ptr_off: usize,
    capture_idx_off: usize,
) {
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", preg_flags_off)); // reload preg_split flags before capture appends
    emitter.instruction(&format!("test r9, {}", PREG_SPLIT_DELIM_CAPTURE));     // is PREG_SPLIT_DELIM_CAPTURE enabled?
    emitter.instruction("jz __rt_preg_split_captures_done_linux_x86_64");       // skip delimiter captures when the flag is absent
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 1", capture_idx_off)); // start at capture group 1, after the full match
    emitter.label("__rt_preg_split_capture_loop_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", capture_idx_off)); // reload current capture-group index
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", nmatch_off));  // reload dynamic regmatch slot count
    emitter.instruction("cmp r10, r9");                                         // stop after all compiled capture groups
    emitter.instruction("jge __rt_preg_split_captures_done_linux_x86_64");      // finish capture processing
    emitter.instruction("shl r10, 4");                                          // scale capture index by the fixed 16-byte pair stride
    emitter.instruction(&format!("mov r12, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic offset-pair buffer base
    emitter.instruction("add r10, r12");                                        // address this capture group's offset pair
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load signed-64-bit capture start
    emitter.instruction("cmp r11, 0");                                          // unmatched captures have negative rm_so
    emitter.instruction("jl __rt_preg_split_capture_next_linux_x86_64");        // skip unmatched capture groups
    emitter.instruction("mov r9, QWORD PTR [r10 + 8]");                         // load signed-64-bit capture end
    emitter.instruction("mov rdx, r9");                                         // seed captured delimiter end offset
    emitter.instruction("sub rdx, r11");                                        // compute captured delimiter length
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_elephc_off)); // load current elephc cursor
    emitter.instruction("add rsi, r11");                                        // compute captured delimiter pointer
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", current_elephc_off)); // reload current elephc cursor
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", subject_ptr_off)); // load original subject start
    emitter.instruction("sub rcx, r10");                                        // compute current cursor absolute offset
    emitter.instruction("add rcx, r11");                                        // add capture-local rm_so to get capture offset
    emit_preg_split_push_piece_x86_64(
        emitter,
        "capture",
        preg_flags_off,
        array_ptr_off,
        piece_ptr_off,
        piece_len_off,
        piece_offset_off,
        pair_ptr_off,
        mixed_ptr_off,
    );
    emitter.label("__rt_preg_split_capture_next_linux_x86_64");
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", capture_idx_off)); // reload capture-group index
    emitter.instruction("add r9, 1");                                           // advance to next capture group
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", capture_idx_off)); // save next capture-group index
    emitter.instruction("jmp __rt_preg_split_capture_loop_linux_x86_64");       // continue capture processing
    emitter.label("__rt_preg_split_captures_done_linux_x86_64");
}
