//! Purpose:
//! Emits the `__rt_preg_replace_callback` runtime helper for regex callbacks.
//! Builds match arrays, invokes the callback, and appends returned strings.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - Callback strings may use `_concat_buf`; the helper delays copying each
//!   unmatched prefix until after callback results have been persisted, and
//!   backs up already-emitted output because callback prologues reset `_concat_off`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the target's in-band null string pair into the given pointer/length registers.
fn emit_preg_replace_callback_null(emitter: &mut Emitter, ptr_reg: &str, len_reg: &str) {
    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        ptr_reg,
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("mov {}, #0", len_reg)),  // null has no string bytes
        Arch::X86_64 => emitter.instruction(&format!("xor {}, {}", len_reg, len_reg)), // null has no string bytes
    }
}

/// __rt_preg_replace_callback: replace regex matches with a callback result.
/// Input:  x1=pattern ptr, x2=pattern len, x3=callback ptr, x4=callback env ptr,
///         x5=subject ptr, x6=subject len
/// Output: x1=result ptr, x2=result len
pub(crate) fn emit_preg_replace_callback(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_replace_callback_linux_x86_64(emitter);
        return;
    }

    let handle_off = 0;
    let regmatches_ptr_off = handle_off + 8;
    let nmatch_off = regmatches_ptr_off + 8;
    let pattern_ptr_off = nmatch_off + 8;
    let pattern_len_off = pattern_ptr_off + 8;
    let callback_ptr_off = pattern_len_off + 8;
    let callback_env_off = callback_ptr_off + 8;
    let subject_ptr_off = callback_env_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let output_start_off = subject_cstr_off + 8;
    let output_write_off = output_start_off + 8;
    let current_pos_off = output_write_off + 8;
    let matches_array_off = current_pos_off + 8;
    let group_idx_off = matches_array_off + 8;
    let prefix_len_off = group_idx_off + 8;
    let max_group_off = prefix_len_off + 8;
    let output_backup_ptr_off = max_group_off + 8;
    let output_backup_len_off = output_backup_ptr_off + 8;
    let callback_result_ptr_off = output_backup_len_off + 8;
    let callback_result_len_off = callback_result_ptr_off + 8;
    let output_cap_off = callback_result_len_off + 8;
    let saved_x19_off = output_cap_off + 8;
    let saved_x20_off = saved_x19_off + 8;
    let saved_x21_off = saved_x20_off + 8;
    let stack_size = (saved_x21_off + 96 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_replace_callback ---");
    emitter.label_global("__rt_preg_replace_callback");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_replace_callback stack frame
    emitter.instruction(&format!("add x9, sp, #{}", save_off));                 // compute save-slot address beyond ARM64 pair-store immediate range
    emitter.instruction("stp x29, x30, [x9]");                                  // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // establish the runtime helper frame pointer
    emitter.instruction(&format!("str x19, [sp, #{}]", saved_x19_off));         // preserve the caller's output base register
    emitter.instruction(&format!("str x20, [sp, #{}]", saved_x20_off));         // preserve the caller's output write register
    emitter.instruction(&format!("str x21, [sp, #{}]", saved_x21_off));         // preserve the caller's output capacity register

    // -- save all inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // save pattern pointer for delimiter stripping
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // save pattern length for delimiter stripping
    emitter.instruction(&format!("str x3, [sp, #{}]", callback_ptr_off));       // save callback entry point across libc calls
    emitter.instruction(&format!("str x4, [sp, #{}]", callback_env_off));       // save optional callback capture environment
    emitter.instruction(&format!("str x5, [sp, #{}]", subject_ptr_off));        // save subject pointer for fallback and C-string conversion
    emitter.instruction(&format!("str x6, [sp, #{}]", subject_len_off));        // save subject length for fallback and C-string conversion

    // -- strip delimiters and compile PCRE regex --
    emitter.instruction("bl __rt_preg_strip");                                  // strip slash delimiters and expose supported regex flags
    emitter.instruction("tbnz x3, #30, __rt_preg_replace_callback_fail");       // the removed /e modifier is invalid in PHP 8
    emitter.instruction(&format!("str x3, [sp, #{}]", flags_off));              // save regex flags from the stripped pattern
    emitter.instruction("bl __rt_pcre_to_posix");                               // materialize PCRE pattern as a C string
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off));       // save null-terminated PCRE pattern
    emitter.instruction(&format!("add x0, sp, #{}", handle_off));               // pass opaque-handle output storage
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off));       // pass null-terminated PCRE pattern to regcomp
    emitter.instruction(&format!("ldr x2, [sp, #{}]", flags_off));              // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("add x3, sp, #{}", nmatch_off));               // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile");                                    // compile without exposing PCRE2-owned layouts
    emitter.instruction("cbnz x0, __rt_preg_replace_callback_fail");            // return the original subject when regex compilation fails

    // -- allocate a reusable fixed offset-pair buffer --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", nmatch_off));             // load match-slot count returned by the shim
    emitter.instruction("lsr x10, x9, #60");                                    // reject slot counts whose 16-byte size would overflow
    emitter.instruction("cbnz x10, __rt_preg_replace_callback_malloc_fail");    // free the handle instead of allocating a wrapped size
    emitter.instruction("lsl x0, x9, #4");                                      // allocate one 16-byte signed-64-bit pair per slot
    emitter.bl_c("malloc");                                                     // allocate the fixed offset-pair vector
    emitter.instruction("cbz x0, __rt_preg_replace_callback_malloc_fail");      // allocation failure frees the handle and returns the original subject
    emitter.instruction(&format!("str x0, [sp, #{}]", regmatches_ptr_off));     // save dynamic offset-pair buffer pointer

    // -- materialize a private subject copy for repeated regexec calls --
    // The callback may run another regex that grows the shared `__rt_cstr2`
    // buffer. Keeping the subject in a private owned block means the outer
    // cursor stays valid across any nested conversion.
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // reload the subject length
    emitter.instruction("adds x0, x2, #1");                                     // reserve room for the payload plus its NUL terminator
    emitter.instruction("b.cc __rt_preg_replace_callback_subject_len_ok");      // a representable length continues to allocation
    emitter.instruction("b __rt_alloc_overflow");                               // a wrapped length can never be terminated
    emitter.label("__rt_preg_replace_callback_subject_len_ok");
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate a private subject copy callbacks cannot replace
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned string payload
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the private copy so the epilogue can release it
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save the private null-terminated subject pointer
    emitter.instruction("mov x9, x0");                                          // destination cursor
    emitter.instruction(&format!("ldr x10, [sp, #{}]", subject_ptr_off));       // source byte pointer
    emitter.instruction(&format!("ldr x11, [sp, #{}]", subject_len_off));       // source byte count
    emitter.instruction("mov x12, #0");                                         // copy index
    emitter.label("__rt_preg_replace_callback_subject_copy");
    emitter.instruction("cmp x12, x11");                                        // have all subject bytes been copied?
    emitter.instruction("b.ge __rt_preg_replace_callback_subject_copy_done");   // terminate the private copy
    emitter.instruction("ldrb w13, [x10, x12]");                                // load one subject byte
    emitter.instruction("strb w13, [x9, x12]");                                 // store it into the private copy
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_preg_replace_callback_subject_copy");           // continue copying subject bytes
    emitter.label("__rt_preg_replace_callback_subject_copy_done");
    emitter.instruction("strb wzr, [x9, x11]");                                 // terminate the private subject copy

    // -- allocate a dedicated output buffer that callback scratch cannot clobber --
    emitter.instruction("mov x19, #4096");                                      // initial output capacity
    emitter.instruction("mov x0, x19");                                         // request the initial output block
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate owned output storage
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned string payload
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the output block so growth can release it
    emitter.instruction("mov x20, x0");                                         // output write cursor
    emitter.instruction("mov x19, x0");                                         // output base pointer
    emitter.instruction("mov x21, #4096");                                      // output capacity in bytes
    emitter.instruction(&format!("str x19, [sp, #{}]", output_start_off));      // save final output start pointer
    emitter.instruction(&format!("str x20, [sp, #{}]", output_write_off));      // initialize final output write pointer
    emitter.instruction(&format!("str x21, [sp, #{}]", output_cap_off));        // initialize output capacity
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_cstr_off));       // load subject C-string start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_pos_off));        // initialize current regex search cursor

    // -- replacement loop --
    emitter.label("__rt_preg_replace_callback_loop");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_pos_off));        // load current subject cursor
    emitter.instruction("ldrb w9, [x1]");                                       // read the current subject byte
    emitter.instruction("cbz w9, __rt_preg_replace_callback_empty_subject");    // finish when the cursor reaches the null terminator
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // pass compiled opaque handle
    emitter.instruction(&format!("ldr x2, [sp, #{}]", nmatch_off));             // request one regmatch slot for every compiled capture group
    emitter.instruction(&format!("ldr x3, [sp, #{}]", regmatches_ptr_off));     // pass dynamic fixed offset-pair buffer
    emitter.instruction("mov x4, #0");                                          // use default execution flags
    emitter.bl_c("elephc_pcre2_v1_exec");                                       // execute and initialize every requested offset pair
    emitter.instruction("cbnz x0, __rt_preg_replace_callback_tail");            // copy the remaining subject when there are no more matches

    // -- remember unmatched prefix length before this match --
    emitter.instruction(&format!("ldr x14, [sp, #{}]", regmatches_ptr_off));    // load dynamic offset-pair buffer base
    emitter.instruction("ldr x9, [x14]");                                       // load full-match signed-64-bit start
    emitter.instruction(&format!("str x9, [sp, #{}]", prefix_len_off));         // save prefix byte count until callback scratch work is done

    // -- find highest populated capture so trailing unmatched captures are omitted --
    emitter.instruction(&format!("ldr x12, [sp, #{}]", nmatch_off));            // reload dynamic regmatch count
    emitter.instruction("sub x12, x12, #1");                                    // start scanning from the last compiled capture slot
    emitter.label("__rt_preg_replace_callback_scan");
    emitter.instruction("lsl x14, x12, #4");                                    // scale capture index by the fixed 16-byte pair stride
    emitter.instruction(&format!("ldr x15, [sp, #{}]", regmatches_ptr_off));    // load dynamic offset-pair buffer base
    emitter.instruction("add x14, x15, x14");                                   // compute address of this offset pair
    emitter.instruction("ldr x13, [x14]");                                      // load signed-64-bit capture start
    emitter.instruction("cmp x13, #0");                                         // check whether this capture participated
    emitter.instruction("b.ge __rt_preg_replace_callback_scan_found");          // use this as the last emitted capture
    emitter.instruction("cbz x12, __rt_preg_replace_callback_scan_found");      // keep at least the full-match slot
    emitter.instruction("sub x12, x12, #1");                                    // move to the previous capture slot
    emitter.instruction("b __rt_preg_replace_callback_scan");                   // continue searching for the highest populated capture
    emitter.label("__rt_preg_replace_callback_scan_found");
    emitter.instruction(&format!("str x12, [sp, #{}]", max_group_off));         // save highest capture index to materialize

    // -- build callback matches array from capture slots --
    emitter.label("__rt_preg_replace_callback_matches");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", nmatch_off));             // allocate enough slots for every compiled capture
    emitter.instruction("mov x1, #16");                                         // string array slots store pointer and length pairs
    emitter.instruction("bl __rt_array_new");                                   // allocate indexed string matches array
    emitter.instruction(&format!("str x0, [sp, #{}]", matches_array_off));      // save matches array pointer across pushes
    emitter.instruction("mov x12, #0");                                         // start with $matches[0]
    emitter.instruction(&format!("str x12, [sp, #{}]", group_idx_off));         // save current capture index
    emitter.label("__rt_preg_replace_callback_group_loop");
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload current capture index
    emitter.instruction(&format!("ldr x13, [sp, #{}]", max_group_off));         // reload highest capture index
    emitter.instruction("cmp x12, x13");                                        // have all required captures been materialized?
    emitter.instruction("b.gt __rt_preg_replace_callback_call");                // invoke callback after the highest populated capture
    emitter.instruction("lsl x14, x12, #4");                                    // scale capture index by the fixed 16-byte pair stride
    emitter.instruction(&format!("ldr x17, [sp, #{}]", regmatches_ptr_off));    // load dynamic offset-pair buffer base
    emitter.instruction("add x14, x17, x14");                                   // compute address of this offset pair
    emitter.instruction("ldr x15, [x14]");                                      // load signed-64-bit capture start
    emitter.instruction("ldr x16, [x14, #8]");                                  // load signed-64-bit capture end
    emitter.instruction("cmp x15, #0");                                         // check whether this capture was populated
    emitter.instruction("b.lt __rt_preg_replace_callback_empty_capture");       // emit an empty string for interior unmatched captures
    emitter.instruction("sub x2, x16, x15");                                    // compute capture byte length
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_pos_off));        // reload current subject cursor
    emitter.instruction("add x1, x1, x15");                                     // compute capture byte pointer
    emitter.instruction("b __rt_preg_replace_callback_push_capture");           // append matched capture text
    emitter.label("__rt_preg_replace_callback_empty_capture");
    emitter.instruction("mov x1, #0");                                          // empty unmatched capture has a null pointer
    emitter.instruction("mov x2, #0");                                          // empty unmatched capture has zero length
    emitter.label("__rt_preg_replace_callback_push_capture");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", matches_array_off));      // reload matches array pointer
    emitter.instruction("bl __rt_array_push_str");                              // append owned capture string to matches array
    emitter.instruction(&format!("str x0, [sp, #{}]", matches_array_off));      // save possibly grown matches array pointer
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload current capture index after helper call
    emitter.instruction("add x12, x12, #1");                                    // advance to next capture slot
    emitter.instruction(&format!("str x12, [sp, #{}]", group_idx_off));         // save next capture index
    emitter.instruction("b __rt_preg_replace_callback_group_loop");             // continue materializing capture strings

    // -- invoke callback and append its string result --
    emitter.label("__rt_preg_replace_callback_call");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", matches_array_off));      // pass matches array as the callback's first argument
    emitter.instruction(&format!("ldr x9, [sp, #{}]", callback_env_off));       // load optional callback capture environment
    emitter.instruction("cbz x9, __rt_preg_replace_callback_direct");           // omit env argument for direct callbacks
    emitter.instruction("mov x1, x9");                                          // pass capture environment after visible callback args
    emitter.label("__rt_preg_replace_callback_direct");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", callback_ptr_off));      // reload callback entry point
    emitter.instruction("blr x10");                                             // call callback and receive its owned replacement string in x1/x2
    emitter.instruction(&format!("str x1, [sp, #{}]", callback_result_ptr_off)); // save the owned callback result pointer across prefix copying
    emitter.instruction(&format!("str x2, [sp, #{}]", callback_result_len_off)); // save persisted callback result length across prefix copying

    // -- copy unmatched prefix into the dedicated output buffer --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", prefix_len_off));         // reload unmatched prefix byte count
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // reload current subject cursor for prefix copy
    emitter.instruction(&format!("add x0, sp, #{}", output_start_off));         // guard address of the output base
    emitter.instruction(&format!("add x1, sp, #{}", output_write_off));         // guard address of the output write cursor
    emitter.instruction(&format!("add x2, sp, #{}", output_cap_off));           // guard address of the output capacity
    emitter.instruction("mov x3, x9");                                          // prefix bytes that will be appended
    emitter.instruction("bl __rt_preg_cb_ensure");                              // grow the owned output block if the prefix does not fit
    emitter.instruction(&format!("ldr x9, [sp, #{}]", prefix_len_off));         // reload the prefix byte count after the guard
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // reload the subject cursor after the guard
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // reload the output write cursor
    emitter.instruction("mov x12, #0");                                         // initialize prefix copy index
    emitter.label("__rt_preg_replace_callback_pre");
    emitter.instruction("cmp x12, x9");                                         // compare prefix copy index with rm_so
    emitter.instruction("b.ge __rt_preg_replace_callback_copy_repl_start");     // switch to callback result once the prefix is copied
    emitter.instruction("ldrb w13, [x10, x12]");                                // load next unmatched prefix byte
    emitter.instruction("strb w13, [x11]");                                     // append unmatched prefix byte to output
    emitter.instruction("add x11, x11, #1");                                    // advance output write pointer
    emitter.instruction("add x12, x12, #1");                                    // advance prefix copy index
    emitter.instruction("b __rt_preg_replace_callback_pre");                    // keep copying unmatched prefix bytes

    // -- append callback string result --
    emitter.label("__rt_preg_replace_callback_copy_repl_start");
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // publish the prefix end before growing for the callback result
    emitter.instruction(&format!("ldr x3, [sp, #{}]", callback_result_len_off)); // callback result bytes that will be appended
    emitter.instruction(&format!("add x0, sp, #{}", output_start_off));         // guard address of the output base
    emitter.instruction(&format!("add x1, sp, #{}", output_write_off));         // guard address of the output write cursor
    emitter.instruction(&format!("add x2, sp, #{}", output_cap_off));           // guard address of the output capacity
    emitter.instruction("bl __rt_preg_cb_ensure");                              // grow the owned output block if the result does not fit
    emitter.instruction(&format!("ldr x1, [sp, #{}]", callback_result_ptr_off)); // reload persisted callback result pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", callback_result_len_off)); // reload persisted callback result length
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // reload the output write cursor after the guard
    emitter.instruction("mov x12, #0");                                         // initialize callback-result copy index
    emitter.label("__rt_preg_replace_callback_copy_repl");
    emitter.instruction("cmp x12, x2");                                         // compare copied bytes against callback result length
    emitter.instruction("b.ge __rt_preg_replace_callback_advance");             // advance regex cursor when callback result is fully copied
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load next callback result byte
    emitter.instruction("strb w13, [x11]");                                     // append callback result byte to output
    emitter.instruction("add x11, x11, #1");                                    // advance output write pointer
    emitter.instruction("add x12, x12, #1");                                    // advance callback-result copy index
    emitter.instruction("b __rt_preg_replace_callback_copy_repl");              // continue copying callback result bytes

    // -- advance past this match --
    emitter.label("__rt_preg_replace_callback_advance");
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // save output write pointer after callback copy
    emitter.instruction(&format!("ldr x0, [sp, #{}]", callback_result_ptr_off)); // reload the persisted callback string
    emitter.instruction("bl __rt_heap_free_safe");                              // release the callback result after copying it
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // reload the output write pointer after the frees
    emitter.instruction(&format!("ldr x14, [sp, #{}]", regmatches_ptr_off));    // load dynamic full-match pair before advancing cursor
    emitter.instruction("ldr x9, [x14, #8]");                                   // load full-match signed-64-bit end
    emitter.instruction("cmp x9, #0");                                          // detect zero-length regex matches
    emitter.instruction("b.gt __rt_preg_replace_callback_advance_ok");          // use native rm_eo when the match consumed bytes
    emitter.instruction("mov x9, #1");                                          // force progress for zero-length matches
    emitter.label("__rt_preg_replace_callback_advance_ok");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // reload current subject cursor
    emitter.instruction("add x10, x10, x9");                                    // move cursor past the current match
    emitter.instruction(&format!("str x10, [sp, #{}]", current_pos_off));       // save next regex search cursor
    emitter.instruction("b __rt_preg_replace_callback_loop");                   // continue replacing further matches

    // -- copy trailing subject after the last match --
    emitter.label("__rt_preg_replace_callback_tail");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // reload current subject cursor for tail copy
    emitter.instruction("mov x9, #0");                                          // measure the trailing subject length first
    emitter.label("__rt_preg_replace_callback_tail_measure");
    emitter.instruction("ldrb w13, [x10, x9]");                                 // load the next trailing byte
    emitter.instruction("cbz w13, __rt_preg_replace_callback_tail_measured");   // stop at the subject's null terminator
    emitter.instruction("add x9, x9, #1");                                      // advance the tail length
    emitter.instruction("b __rt_preg_replace_callback_tail_measure");           // continue measuring the tail
    emitter.label("__rt_preg_replace_callback_tail_measured");
    emitter.instruction(&format!("str x9, [sp, #{}]", prefix_len_off));         // preserve the measured tail length across the guard
    emitter.instruction(&format!("add x0, sp, #{}", output_start_off));         // guard address of the output base
    emitter.instruction(&format!("add x1, sp, #{}", output_write_off));         // guard address of the output write cursor
    emitter.instruction(&format!("add x2, sp, #{}", output_cap_off));           // guard address of the output capacity
    emitter.instruction("mov x3, x9");                                          // trailing bytes that will be appended
    emitter.instruction("bl __rt_preg_cb_ensure");                              // grow the owned output block if the tail does not fit
    emitter.instruction(&format!("ldr x9, [sp, #{}]", prefix_len_off));         // reload the measured tail length after the guard
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // reload the subject cursor after the guard
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // reload the output write cursor
    emitter.instruction("mov x12, #0");                                         // tail copy index
    emitter.label("__rt_preg_replace_callback_tail_loop");
    emitter.instruction("cmp x12, x9");                                         // have all tail bytes been copied?
    emitter.instruction("b.ge __rt_preg_replace_callback_done");                // finish once the trailing subject is emitted
    emitter.instruction("ldrb w13, [x10, x12]");                                // load one trailing byte
    emitter.instruction("strb w13, [x11]");                                     // append it to the output
    emitter.instruction("add x11, x11, #1");                                    // advance the output write cursor
    emitter.instruction("add x12, x12, #1");                                    // advance the tail copy index
    emitter.instruction("b __rt_preg_replace_callback_tail_loop");              // continue copying tail bytes

    // -- free regex and return final output slice --
    emitter.label("__rt_preg_replace_callback_empty_subject");
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // load the initialized empty-subject output cursor
    emitter.label("__rt_preg_replace_callback_done");
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // save final output pointer
    emitter.instruction(&format!("ldr x0, [sp, #{}]", subject_cstr_off));       // reload the private subject copy for release
    emitter.instruction("bl __rt_heap_free_safe");                              // release the subject copy now that no cursor points into it
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regmatches_ptr_off));     // reload dynamic capture buffer for cleanup
    emitter.bl_c("free");                                                       // release the reusable offset-pair vector
    emitter.instruction(&format!("ldr x1, [sp, #{}]", output_start_off));       // return output start pointer
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // reload output end pointer
    emitter.instruction("sub x2, x11, x1");                                     // compute output byte length
    emitter.instruction("b __rt_preg_replace_callback_ret");                    // share common epilogue

    // -- failure: PHP returns null for an invalid pattern --
    emitter.label("__rt_preg_replace_callback_fail");
    emit_preg_replace_callback_null(emitter, "x1", "x2");
    emitter.instruction("b __rt_preg_replace_callback_ret");                    // return through the common epilogue

    emitter.label("__rt_preg_replace_callback_malloc_fail");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // reload opaque handle after capture-buffer allocation failed
    emitter.bl_c("elephc_pcre2_v1_free");                                       // free compiled regex resources before returning null
    emit_preg_replace_callback_null(emitter, "x1", "x2");

    emitter.label("__rt_preg_replace_callback_ret");
    emitter.instruction(&format!("ldr x19, [sp, #{}]", saved_x19_off));         // restore the caller's output base register
    emitter.instruction(&format!("ldr x20, [sp, #{}]", saved_x20_off));         // restore the caller's output write register
    emitter.instruction(&format!("ldr x21, [sp, #{}]", saved_x21_off));         // restore the caller's output capacity register
    emitter.instruction(&format!("add x9, sp, #{}", save_off));                 // compute save-slot address beyond ARM64 pair-load immediate range
    emitter.instruction("ldp x29, x30, [x9]");                                  // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // release preg_replace_callback stack frame
    emitter.instruction("ret");                                                 // return to generated code
}

/// Emits the x86_64 implementation of `__rt_preg_replace_callback`.
///
/// Mirrors the AArch64 contract with System V registers. The accumulated result lives in a
/// dedicated growable heap buffer (`r12` base, `r13` write cursor, `r14` capacity), so a callback
/// may freely use the shared scratch arena without forcing a copy of the output per match.
fn emit_preg_replace_callback_linux_x86_64(emitter: &mut Emitter) {
    let handle_off = 0;
    let regmatches_ptr_off = handle_off + 8;
    let nmatch_off = regmatches_ptr_off + 8;
    let pattern_ptr_off = nmatch_off + 8;
    let pattern_len_off = pattern_ptr_off + 8;
    let callback_ptr_off = pattern_len_off + 8;
    let callback_env_off = callback_ptr_off + 8;
    let subject_ptr_off = callback_env_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let output_start_off = subject_cstr_off + 8;
    let output_write_off = output_start_off + 8;
    let current_pos_off = output_write_off + 8;
    let matches_array_off = current_pos_off + 8;
    let group_idx_off = matches_array_off + 8;
    let prefix_len_off = group_idx_off + 8;
    let max_group_off = prefix_len_off + 8;
    let output_backup_ptr_off = max_group_off + 8;
    let output_backup_len_off = output_backup_ptr_off + 8;
    let callback_result_ptr_off = output_backup_len_off + 8;
    let callback_result_len_off = callback_result_ptr_off + 8;
    let output_cap_off = callback_result_len_off + 8;
    let saved_r12_off = output_cap_off + 8;
    let saved_r13_off = saved_r12_off + 8;
    let saved_r14_off = saved_r13_off + 8;
    let stack_size = (saved_r14_off + 16 + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: preg_replace_callback ---");
    emitter.label_global("__rt_preg_replace_callback");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving regex callback scratch storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for regex callback spill slots
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve aligned local storage for the handle, offset pairs, and callback state
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r12", saved_r12_off)); // preserve the caller's output base register
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r13", saved_r13_off)); // preserve the caller's output write register
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r14", saved_r14_off)); // preserve the caller's output capacity register

    // -- save all inputs --
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdi", pattern_ptr_off)); // preserve pattern pointer across regex helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rsi", pattern_len_off)); // preserve pattern length across regex helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", callback_ptr_off)); // preserve callback entry point across regex helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", callback_env_off)); // preserve optional callback capture environment
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r8", subject_ptr_off)); // preserve subject pointer for fallback and C-string conversion
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", subject_len_off)); // preserve subject length for fallback and C-string conversion

    // -- strip delimiters and compile PCRE regex --
    emitter.instruction("mov rax, rdi");                                        // move pattern pointer into the delimiter-strip helper input register
    emitter.instruction("mov rdx, rsi");                                        // move pattern length into the delimiter-strip helper input register
    emitter.instruction("call __rt_preg_strip");                                // strip slash delimiters and gather supported regex flags
    emitter.instruction("bt rcx, 30");                                          // did the pattern use the removed /e modifier?
    emitter.instruction("jc __rt_preg_replace_callback_fail_linux_x86_64");     // PHP 8 rejects /e as an unknown modifier
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", flags_off));  // preserve delimiter-strip flags for regcomp
    emitter.instruction("call __rt_pcre_to_posix");                             // materialize PCRE pattern as a C string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // preserve null-terminated PCRE pattern across compilation
    emitter.instruction(&format!("lea rdi, [rsp + {}]", handle_off));           // pass opaque-handle output storage
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass null-terminated PCRE pattern to regcomp
    emitter.instruction(&format!("mov edx, DWORD PTR [rsp + {}]", flags_off));  // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("lea rcx, [rsp + {}]", nmatch_off));           // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile");                                    // compile without exposing PCRE2-owned layouts
    emitter.instruction("test eax, eax");                                       // did regex compilation succeed?
    emitter.instruction("jnz __rt_preg_replace_callback_fail_linux_x86_64");    // return original subject when regex compilation fails

    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", nmatch_off)); // load match-slot count returned by the shim
    emitter.instruction("mov r10, rdi");                                        // copy slot count for allocation-overflow validation
    emitter.instruction("shr r10, 60");                                         // detect a wrapped 16-byte pair-vector size
    emitter.instruction("jnz __rt_preg_replace_callback_malloc_fail_linux_x86_64"); // free the handle instead of allocating a wrapped size
    emitter.instruction("shl rdi, 4");                                          // allocate one 16-byte signed-64-bit pair per slot
    emitter.bl_c("malloc");                                                     // allocate the fixed offset-pair vector
    emitter.instruction("test rax, rax");                                       // did malloc return a capture buffer?
    emitter.instruction("jz __rt_preg_replace_callback_malloc_fail_linux_x86_64"); // allocation failure frees the opaque handle and returns the subject
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", regmatches_ptr_off)); // save dynamic offset-pair buffer pointer

    // -- materialize a private subject copy for repeated regexec calls --
    // The callback may run another regex that grows the shared `__rt_cstr2`
    // buffer. Keeping the subject in a private owned block means the outer
    // cursor stays valid across any nested conversion.
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload the subject length
    emitter.instruction("mov rax, rdx");                                        // start the private subject size from the byte length
    emitter.instruction("add rax, 1");                                          // reserve room for the payload plus its NUL terminator
    emitter.instruction("jnc __rt_preg_replace_callback_subject_len_ok_linux_x86_64"); // a representable length continues to allocation
    emitter.instruction("jmp __rt_alloc_overflow");                             // a wrapped length can never be terminated
    emitter.label("__rt_preg_replace_callback_subject_len_ok_linux_x86_64");
    emitter.instruction("call __rt_heap_alloc");                                // allocate a private subject copy callbacks cannot replace
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the private copy so the epilogue can release it
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // save the private null-terminated subject pointer
    emitter.instruction("mov rdi, rax");                                        // destination cursor
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", subject_ptr_off)); // source byte pointer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // source byte count
    emitter.instruction("xor ecx, ecx");                                        // copy index
    emitter.label("__rt_preg_replace_callback_subject_copy_linux_x86_64");
    emitter.instruction("cmp rcx, rdx");                                        // have all subject bytes been copied?
    emitter.instruction("jge __rt_preg_replace_callback_subject_copy_done_linux_x86_64"); // terminate the private copy
    emitter.instruction("mov r8b, BYTE PTR [rsi + rcx]");                       // load one subject byte
    emitter.instruction("mov BYTE PTR [rdi + rcx], r8b");                       // store it into the private copy
    emitter.instruction("add rcx, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_preg_replace_callback_subject_copy_linux_x86_64"); // continue copying subject bytes
    emitter.label("__rt_preg_replace_callback_subject_copy_done_linux_x86_64");
    emitter.instruction("mov BYTE PTR [rdi + rdx], 0");                         // terminate the private subject copy

    // -- allocate a dedicated output buffer that callback scratch cannot clobber --
    emitter.instruction("mov rax, 4096");                                       // initial output capacity
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned output storage
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the output block so growth can release it
    emitter.instruction("mov r12, rax");                                        // output base pointer
    emitter.instruction("mov r13, rax");                                        // output write cursor
    emitter.instruction("mov r14, 4096");                                       // output capacity in bytes
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r12", output_start_off)); // save final output start pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r13", output_write_off)); // initialize final output write pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r14", output_cap_off)); // initialize output capacity
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_cstr_off)); // load subject C-string start
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", current_pos_off)); // initialize current regex search cursor

    // -- replacement loop --
    emitter.label("__rt_preg_replace_callback_loop_linux_x86_64");
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_pos_off)); // reload current subject cursor for regexec
    emitter.instruction("movzx r9d, BYTE PTR [rsi]");                           // read the current subject byte
    emitter.instruction("test r9d, r9d");                                       // check whether the cursor reached the null terminator
    emitter.instruction("jz __rt_preg_replace_callback_empty_subject_linux_x86_64"); // finish when the full subject has been consumed
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // pass compiled opaque handle
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", nmatch_off)); // request one regmatch slot for every compiled capture group
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // pass dynamic fixed offset-pair buffer
    emitter.instruction("xor r8d, r8d");                                        // use default execution flags
    emitter.bl_c("elephc_pcre2_v1_exec");                                       // execute and initialize every requested offset pair
    emitter.instruction("test eax, eax");                                       // did regexec find another match?
    emitter.instruction("jnz __rt_preg_replace_callback_tail_linux_x86_64");    // copy the remaining subject once no more matches exist

    // -- remember unmatched prefix length before this match --
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic offset-pair buffer base
    emitter.instruction("mov r9, QWORD PTR [r10]");                             // load full-match signed-64-bit start
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", prefix_len_off)); // save prefix byte count until callback scratch work is done

    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", nmatch_off));  // reload dynamic regmatch count
    emitter.instruction("sub r9, 1");                                           // start scanning from the last compiled capture slot
    emitter.label("__rt_preg_replace_callback_scan_linux_x86_64");
    emitter.instruction("mov r10, r9");                                         // copy capture index before scaling
    emitter.instruction("shl r10, 4");                                          // scale capture index by the fixed 16-byte pair stride
    emitter.instruction(&format!("mov r12, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic offset-pair buffer base
    emitter.instruction("add r10, r12");                                        // compute address of this offset pair
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load signed-64-bit capture start
    emitter.instruction("cmp r11, 0");                                          // check whether this capture participated
    emitter.instruction("jge __rt_preg_replace_callback_scan_found_linux_x86_64"); // use this as the highest emitted capture
    emitter.instruction("test r9, r9");                                         // have we reached the full-match slot?
    emitter.instruction("jz __rt_preg_replace_callback_scan_found_linux_x86_64"); // keep at least the full match after successful regexec
    emitter.instruction("sub r9, 1");                                           // move to the previous capture slot
    emitter.instruction("jmp __rt_preg_replace_callback_scan_linux_x86_64");    // continue searching for the highest populated capture
    emitter.label("__rt_preg_replace_callback_scan_found_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", max_group_off)); // save highest capture index to materialize

    // -- build callback matches array from capture slots --
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", nmatch_off)); // allocate enough slots for every compiled capture
    emitter.instruction("mov rsi, 16");                                         // string array slots store pointer and length pairs
    emitter.instruction("call __rt_array_new");                                 // allocate indexed string matches array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", matches_array_off)); // save matches array pointer across pushes
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", group_idx_off)); // start with $matches[0]
    emitter.label("__rt_preg_replace_callback_group_loop_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", group_idx_off)); // reload current capture index
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", max_group_off)); // reload highest capture index
    emitter.instruction("cmp r10, r9");                                         // have all required captures been materialized?
    emitter.instruction("jg __rt_preg_replace_callback_call_linux_x86_64");     // invoke callback after highest populated capture
    emitter.instruction("shl r10, 4");                                          // scale capture index by the fixed 16-byte pair stride
    emitter.instruction(&format!("mov r12, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic offset-pair buffer base
    emitter.instruction("add r10, r12");                                        // compute address of this offset pair
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load signed-64-bit capture start
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load signed-64-bit capture end
    emitter.instruction("cmp r11, 0");                                          // check whether this capture was populated
    emitter.instruction("jl __rt_preg_replace_callback_empty_capture_linux_x86_64"); // emit an empty string for interior unmatched captures
    emitter.instruction("sub rcx, r11");                                        // compute capture byte length
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_pos_off)); // reload current subject cursor
    emitter.instruction("add rsi, r11");                                        // compute capture byte pointer
    emitter.instruction("jmp __rt_preg_replace_callback_push_capture_linux_x86_64"); // append matched capture text
    emitter.label("__rt_preg_replace_callback_empty_capture_linux_x86_64");
    emitter.instruction("xor esi, esi");                                        // empty unmatched capture has a null pointer
    emitter.instruction("xor edx, edx");                                        // empty unmatched capture has zero length
    emitter.label("__rt_preg_replace_callback_push_capture_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", matches_array_off)); // reload matches array pointer
    emitter.instruction("test r11, r11");                                       // was the current capture matched?
    emitter.instruction("cmovge rdx, rcx");                                     // pass capture byte length for matched captures
    emitter.instruction("call __rt_array_push_str");                            // append owned capture string to matches array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", matches_array_off)); // save possibly grown matches array pointer
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", group_idx_off)); // reload current capture index after helper call
    emitter.instruction("add r10, 1");                                          // advance to next capture slot
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", group_idx_off)); // save next capture index
    emitter.instruction("jmp __rt_preg_replace_callback_group_loop_linux_x86_64"); // continue materializing capture strings

    // -- invoke callback and append its string result --
    emitter.label("__rt_preg_replace_callback_call_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", matches_array_off)); // pass matches array as the callback's first argument
    emitter.instruction(&format!("cmp QWORD PTR [rsp + {}], 0", callback_env_off)); // check whether callback has a capture environment
    emitter.instruction("je __rt_preg_replace_callback_direct_linux_x86_64");   // omit env argument for direct callbacks
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", callback_env_off)); // pass capture environment after visible callback args
    emitter.label("__rt_preg_replace_callback_direct_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", callback_ptr_off)); // reload callback entry point
    emitter.instruction("call r10");                                            // call callback and receive its owned replacement string in rax/rdx
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", callback_result_ptr_off)); // save the owned callback result pointer across prefix copying
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", callback_result_len_off)); // save persisted callback result length across prefix copying

    // -- copy unmatched prefix into the dedicated output buffer --
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", prefix_len_off)); // reload unmatched prefix byte count
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload current subject cursor for prefix copy
    emitter.instruction(&format!("lea rdi, [rsp + {}]", output_start_off));     // guard address of the output base
    emitter.instruction(&format!("lea rsi, [rsp + {}]", output_write_off));     // guard address of the output write cursor
    emitter.instruction(&format!("lea rdx, [rsp + {}]", output_cap_off));       // guard address of the output capacity
    emitter.instruction("mov rcx, r9");                                         // prefix bytes that will be appended
    emitter.instruction("call __rt_preg_cb_ensure");                            // grow the owned output block if the prefix does not fit
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", prefix_len_off)); // reload the prefix byte count after the guard
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload the subject cursor after the guard
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", output_write_off)); // reload the output write cursor
    emitter.instruction("xor ecx, ecx");                                        // initialize prefix copy index
    emitter.label("__rt_preg_replace_callback_pre_linux_x86_64");
    emitter.instruction("cmp rcx, r9");                                         // compare prefix copy index with rm_so
    emitter.instruction("jge __rt_preg_replace_callback_copy_repl_start_linux_x86_64"); // switch to callback result once prefix is copied
    emitter.instruction("mov r8b, BYTE PTR [r10 + rcx]");                       // load next unmatched prefix byte
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // append unmatched prefix byte to output
    emitter.instruction("add r11, 1");                                          // advance output write pointer
    emitter.instruction("add rcx, 1");                                          // advance prefix copy index
    emitter.instruction("jmp __rt_preg_replace_callback_pre_linux_x86_64");     // keep copying unmatched prefix bytes

    // -- append callback string result --
    emitter.label("__rt_preg_replace_callback_copy_repl_start_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", output_write_off)); // publish the prefix end before growing for the callback result
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", callback_result_len_off)); // callback result bytes that will be appended
    emitter.instruction(&format!("lea rdi, [rsp + {}]", output_start_off));     // guard address of the output base
    emitter.instruction(&format!("lea rsi, [rsp + {}]", output_write_off));     // guard address of the output write cursor
    emitter.instruction(&format!("lea rdx, [rsp + {}]", output_cap_off));       // guard address of the output capacity
    emitter.instruction("call __rt_preg_cb_ensure");                            // grow the owned output block if the result does not fit
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", callback_result_ptr_off)); // reload persisted callback result pointer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", callback_result_len_off)); // reload persisted callback result length
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", output_write_off)); // reload the output write cursor after the guard
    emitter.instruction("xor ecx, ecx");                                        // initialize callback-result copy index
    emitter.label("__rt_preg_replace_callback_copy_repl_linux_x86_64");
    emitter.instruction("cmp rcx, rdx");                                        // compare copied bytes against callback result length
    emitter.instruction("jge __rt_preg_replace_callback_advance_linux_x86_64"); // advance regex cursor when callback result is fully copied
    emitter.instruction("mov r8b, BYTE PTR [rax + rcx]");                       // load next callback result byte
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // append callback result byte to output
    emitter.instruction("add r11, 1");                                          // advance output write pointer
    emitter.instruction("add rcx, 1");                                          // advance callback-result copy index
    emitter.instruction("jmp __rt_preg_replace_callback_copy_repl_linux_x86_64"); // continue copying callback result bytes

    // -- advance past this match --
    emitter.label("__rt_preg_replace_callback_advance_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", output_write_off)); // save output write pointer after callback copy
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", callback_result_ptr_off)); // reload the persisted callback string
    emitter.instruction("call __rt_heap_free_safe");                            // release the callback result after copying it
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", output_write_off)); // reload the output write pointer after the frees
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic full-match pair before advancing cursor
    emitter.instruction("mov r9, QWORD PTR [r10 + 8]");                         // load full-match signed-64-bit end
    emitter.instruction("cmp r9, 0");                                           // detect zero-length regex matches
    emitter.instruction("jg __rt_preg_replace_callback_advance_ok_linux_x86_64"); // use native rm_eo when the match consumed bytes
    emitter.instruction("mov r9, 1");                                           // force progress for zero-length matches
    emitter.label("__rt_preg_replace_callback_advance_ok_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload current subject cursor
    emitter.instruction("add r10, r9");                                         // move cursor past the current match
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", current_pos_off)); // save next regex search cursor
    emitter.instruction("jmp __rt_preg_replace_callback_loop_linux_x86_64");    // continue replacing further matches

    // -- copy trailing subject after the last match --
    emitter.label("__rt_preg_replace_callback_tail_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload current subject cursor for tail copy
    emitter.instruction("xor r9d, r9d");                                        // measure the trailing subject length first
    emitter.label("__rt_preg_replace_callback_tail_measure_linux_x86_64");
    emitter.instruction("cmp BYTE PTR [r10 + r9], 0");                          // is this the subject's null terminator?
    emitter.instruction("je __rt_preg_replace_callback_tail_measured_linux_x86_64"); // stop measuring at the terminator
    emitter.instruction("add r9, 1");                                           // count one more trailing byte
    emitter.instruction("jmp __rt_preg_replace_callback_tail_measure_linux_x86_64"); // continue measuring the tail
    emitter.label("__rt_preg_replace_callback_tail_measured_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", prefix_len_off)); // preserve the measured tail length across the guard
    emitter.instruction(&format!("lea rdi, [rsp + {}]", output_start_off));     // guard address of the output base
    emitter.instruction(&format!("lea rsi, [rsp + {}]", output_write_off));     // guard address of the output write cursor
    emitter.instruction(&format!("lea rdx, [rsp + {}]", output_cap_off));       // guard address of the output capacity
    emitter.instruction("mov rcx, r9");                                         // trailing bytes that will be appended
    emitter.instruction("call __rt_preg_cb_ensure");                            // grow the owned output block if the tail does not fit
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", prefix_len_off)); // reload the measured tail length after the guard
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload the subject cursor after the guard
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", output_write_off)); // reload the output write cursor
    emitter.instruction("xor ecx, ecx");                                        // tail copy index
    emitter.label("__rt_preg_replace_callback_tail_loop_linux_x86_64");
    emitter.instruction("cmp rcx, r9");                                         // have all tail bytes been copied?
    emitter.instruction("jge __rt_preg_replace_callback_done_linux_x86_64");    // finish once the trailing subject is emitted
    emitter.instruction("mov r8b, BYTE PTR [r10 + rcx]");                       // load one trailing byte
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // append it to the output
    emitter.instruction("add r11, 1");                                          // advance the output write cursor
    emitter.instruction("add rcx, 1");                                          // advance the tail copy index
    emitter.instruction("jmp __rt_preg_replace_callback_tail_loop_linux_x86_64"); // continue copying tail bytes

    // -- free regex and return final output slice --
    emitter.label("__rt_preg_replace_callback_empty_subject_linux_x86_64");
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", output_write_off)); // load the initialized empty-subject output cursor
    emitter.label("__rt_preg_replace_callback_done_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", output_write_off)); // save final output pointer
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_cstr_off)); // reload the private subject copy into the runtime free ABI register
    emitter.instruction("call __rt_heap_free_safe");                            // release the subject copy now that no cursor points into it
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // reload dynamic capture buffer for cleanup
    emitter.bl_c("free");                                                       // release the reusable offset-pair vector
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", output_start_off)); // return output start pointer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", output_write_off)); // reload output end pointer
    emitter.instruction("sub rdx, rax");                                        // compute output byte length
    emitter.instruction("jmp __rt_preg_replace_callback_ret_linux_x86_64");     // share common epilogue

    // -- failure: PHP returns null for an invalid pattern --
    emitter.label("__rt_preg_replace_callback_fail_linux_x86_64");
    emit_preg_replace_callback_null(emitter, "rax", "rdx");
    emitter.instruction("jmp __rt_preg_replace_callback_ret_linux_x86_64");     // return through the common epilogue

    emitter.label("__rt_preg_replace_callback_malloc_fail_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // reload opaque handle after capture-buffer allocation failed
    emitter.bl_c("elephc_pcre2_v1_free");                                       // free compiled regex resources before returning null
    emit_preg_replace_callback_null(emitter, "rax", "rdx");

    emitter.label("__rt_preg_replace_callback_ret_linux_x86_64");
    emitter.instruction(&format!("mov r12, QWORD PTR [rsp + {}]", saved_r12_off)); // restore the caller's output base register
    emitter.instruction(&format!("mov r13, QWORD PTR [rsp + {}]", saved_r13_off)); // restore the caller's output write register
    emitter.instruction(&format!("mov r14, QWORD PTR [rsp + {}]", saved_r14_off)); // restore the caller's output capacity register
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release preg_replace_callback stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code
}
