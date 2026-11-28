//! Purpose:
//! Emits the `__rt_preg_match`, `__rt_preg_strip` runtime helper assembly for preg match.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - Regex helpers preserve PHP PCRE-flavored inputs for PCRE2 and must preserve match array construction.

use elephc_memoir::buffers::native_words;
use elephc_memoir::buffers::offset_pairs;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// __rt_preg_match: check if a PCRE regex matches a subject string.
/// Input:  x1=pattern ptr, x2=pattern len, x3=subject ptr, x4=subject len
/// Output: x0=1 if a match is found, 0 if not, -1 when the pattern is invalid or uses `/e`
pub(crate) fn emit_preg_match(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_match_linux_x86_64(emitter);
        emit_preg_match_capture_linux_x86_64(emitter);
        return;
    }

    let handle_off = 0;
    let match_slot_count_off = handle_off + 8;
    let match_pair_off = match_slot_count_off + 8;
    let pattern_ptr_off = match_pair_off + 16;
    let pattern_len_off = pattern_ptr_off + 8;
    let subject_ptr_off = pattern_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_body_ptr_off = flags_off + 8;
    let pattern_body_len_off = pattern_body_ptr_off + 8;
    let subject_cstr_off = pattern_body_len_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let stack_size = (regexec_result_off + 40 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_match ---");
    emitter.label_global("__rt_preg_match");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_match stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // set new frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // save pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // save pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // save subject ptr
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // save subject len

    // -- strip delimiters from pattern --
    emitter.instruction("bl __rt_preg_strip");                                  // → x1=stripped, x2=len, x3=flags
    emitter.instruction("tbnz x3, #30, __rt_preg_match_fail");                  // PHP rejects this pattern before compiling it
    emitter.instruction(&format!("str x3, [sp, #{}]", flags_off));              // save flags

    // -- keep the stripped pattern body and its explicit length --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_body_ptr_off));   // save the stripped pattern pointer
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_body_len_off));   // save the stripped pattern length

    // -- prepare locale state for regex helpers --
    super::emit_prepare_regex_locale(emitter);

    // -- compile regex through the opaque Elephc shim with an explicit length --
    emitter.instruction(&format!("add x0, sp, #{}", handle_off));               // pass opaque-handle output storage
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_body_ptr_off));   // pass the pattern body pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", pattern_body_len_off));   // pass the authoritative pattern length
    emitter.instruction(&format!("ldr x3, [sp, #{}]", flags_off));              // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("add x4, sp, #{}", match_slot_count_off));     // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile_len");                                // compile by length so embedded NULs are preserved
    emitter.instruction("cbnz x0, __rt_preg_match_fail");                       // compile failed → PHP false

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // load subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // load subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string

    // -- execute regex through the opaque Elephc shim --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // pass compiled opaque handle
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_cstr_off));       // pass null-terminated subject
    emitter.instruction("mov x2, #1");                                          // request only the full-match pair
    emitter.instruction(&format!("add x3, sp, #{}", match_pair_off));           // receive one fixed signed-64-bit offset pair
    emitter.instruction(&format!("str xzr, [sp, #{}]", match_pair_off));        // start the REG_STARTEND range at byte zero
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_len_off));        // load the explicit subject length
    emitter.instruction(&format!("str x9, [sp, #{}]", match_pair_off + 8));     // bound the match at the final subject byte, NULs included
    emitter.instruction("mov x4, #128");                                        // REG_STARTEND keeps every subject byte in range
    super::preg_error_state::emit_exec_call(emitter);                           // execute without exposing PCRE2-owned layouts
    emitter.instruction(&format!("str x0, [sp, #{}]", regexec_result_off));     // save regexec result

    // -- free compiled regex --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources

    // -- return result --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regexec_result_off));     // reload regexec result
    emitter.instruction("cbnz x0, __rt_preg_match_no");                         // non-zero = no match
    emitter.instruction("mov x0, #1");                                          // matched → return 1
    emitter.instruction("b __rt_preg_match_ret");                               // return

    emitter.label("__rt_preg_match_no");
    emitter.instruction("mov x0, #0");                                          // no match → return 0
    emitter.instruction("b __rt_preg_match_ret");                               // skip the failure sentinel

    emitter.label("__rt_preg_match_fail");
    emitter.instruction("mov x0, #-1");                                         // failure sentinel: PHP false
    emitter.instruction("b __rt_preg_match_ret");                               // return the sentinel

    emitter.label("__rt_preg_match_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    emit_preg_match_capture_arm64(emitter);
}

/// Emits ARM64 `__rt_preg_match_capture`, returning the match flag and `$matches` array.
fn emit_preg_match_capture_arm64(emitter: &mut Emitter) {
    let handle_off = 0;
    let regmatches_ptr_off = handle_off + 8;
    let nmatch_off = regmatches_ptr_off + 8;
    let pattern_ptr_off = nmatch_off + 8;
    let pattern_len_off = pattern_ptr_off + 8;
    let subject_ptr_off = pattern_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_body_ptr_off = flags_off + 8;
    let pattern_body_len_off = pattern_body_ptr_off + 8;
    let subject_cstr_off = pattern_body_len_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let matches_array_off = regexec_result_off + 8;
    let group_idx_off = matches_array_off + 8;
    let max_group_off = group_idx_off + 8;
    let stack_size = (max_group_off + 96 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_match_capture ---");
    emitter.label_global("__rt_preg_match_capture");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_match capture stack frame
    emitter.instruction(&format!("add x9, sp, #{}", save_off));                 // compute save-slot address beyond ARM64 pair-store range
    emitter.instruction("stp x29, x30, [x9]");                                  // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // establish the preg_match capture frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // save pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // save pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // save subject ptr
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // save subject len

    // -- strip delimiters and compile PCRE regex --
    emitter.instruction("bl __rt_preg_strip");                                  // strip delimiters and expose regex flags
    emitter.instruction("tbnz x3, #30, __rt_preg_match_capture_compile_fail");  // PHP rejects this pattern before compiling it
    emitter.instruction(&format!("str x3, [sp, #{}]", flags_off));              // save stripped regex flags
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_body_ptr_off));   // save the stripped pattern pointer
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_body_len_off));   // save the stripped pattern length
    super::emit_prepare_regex_locale(emitter);
    emitter.instruction(&format!("add x0, sp, #{}", handle_off));               // pass opaque-handle output storage
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_body_ptr_off));   // pass the pattern body pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", pattern_body_len_off));   // pass the authoritative pattern length
    emitter.instruction(&format!("ldr x3, [sp, #{}]", flags_off));              // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("add x4, sp, #{}", nmatch_off));               // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile_len");                                // compile by length so embedded NULs are preserved
    emitter.instruction("cbnz x0, __rt_preg_match_capture_compile_fail");       // compile failure returns PHP false plus an empty matches array

    // -- allocate fixed Elephc offset pairs for every compiled capture --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", nmatch_off));             // load match-slot count returned by the shim
    offset_pairs::emit_allocate(emitter, "x9", regmatches_ptr_off, "__rt_preg_match_capture_malloc_fail");

    // -- null-terminate subject and run regex --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // reload subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // reload subject len
    emitter.instruction("bl __rt_cstr2");                                       // materialize null-terminated subject copy
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // pass compiled opaque handle
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_cstr_off));       // pass subject C string to regexec
    emitter.instruction(&format!("ldr x2, [sp, #{}]", nmatch_off));             // request one regmatch slot for every capture group
    emitter.instruction(&format!("ldr x3, [sp, #{}]", regmatches_ptr_off));     // pass dynamic fixed offset-pair buffer
    emitter.instruction(&format!("ldr x9, [sp, #{}]", regmatches_ptr_off));     // load the capture pair vector
    emitter.instruction("str xzr, [x9]");                                       // start the REG_STARTEND range at byte zero
    emitter.instruction(&format!("ldr x10, [sp, #{}]", subject_len_off));       // load the explicit subject length
    emitter.instruction("str x10, [x9, #8]");                                   // bound the match at the final subject byte, NULs included
    emitter.instruction("mov x4, #128");                                        // REG_STARTEND keeps every subject byte in range
    super::preg_error_state::emit_exec_call(emitter);                           // execute and initialize every requested offset pair
    emitter.instruction(&format!("str x0, [sp, #{}]", regexec_result_off));     // save regexec status across cleanup
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regexec_result_off));     // reload regexec status
    emitter.instruction("cbnz x0, __rt_preg_match_capture_no_match");           // no match frees capture storage and returns an empty array

    // -- find highest populated capture so trailing unmatched groups are omitted --
    emitter.instruction(&format!("ldr x12, [sp, #{}]", nmatch_off));            // reload the dynamic regmatch count
    emitter.instruction("sub x12, x12, #1");                                    // start scanning from the last compiled capture slot
    emitter.label("__rt_preg_match_capture_scan");
    emitter.instruction("lsl x14, x12, #4");                                    // scale capture index by the fixed 16-byte pair stride
    emitter.instruction(&format!("ldr x15, [sp, #{}]", regmatches_ptr_off));    // load dynamic offset-pair buffer base
    emitter.instruction("add x14, x15, x14");                                   // compute address of this offset pair
    emitter.instruction("ldr x13, [x14]");                                      // load signed-64-bit capture start
    emitter.instruction("cmp x13, #0");                                         // check whether this capture participated
    emitter.instruction("b.ge __rt_preg_match_capture_scan_found");             // use this as the last emitted capture
    emitter.instruction("cbz x12, __rt_preg_match_capture_scan_found");         // keep at least the full match slot after a successful regexec
    emitter.instruction("sub x12, x12, #1");                                    // move to the previous capture slot
    emitter.instruction("b __rt_preg_match_capture_scan");                      // continue searching for the highest populated capture
    emitter.label("__rt_preg_match_capture_scan_found");
    emitter.instruction(&format!("str x12, [sp, #{}]", max_group_off));         // save highest capture index to materialize

    // -- allocate and fill matches array --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", nmatch_off));             // allocate enough slots for every compiled capture
    emitter.instruction("mov x1, #16");                                         // string arrays use pointer/length payload slots
    emitter.instruction("bl __rt_array_new");                                   // allocate indexed string matches array
    emitter.instruction(&format!("str x0, [sp, #{}]", matches_array_off));      // save matches array pointer across pushes
    emitter.instruction(&format!("str xzr, [sp, #{}]", group_idx_off));         // start with capture index zero
    emitter.label("__rt_preg_match_capture_group_loop");
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload current capture index
    emitter.instruction(&format!("ldr x13, [sp, #{}]", max_group_off));         // reload highest capture index
    emitter.instruction("cmp x12, x13");                                        // have all required captures been materialized?
    emitter.instruction("b.gt __rt_preg_match_capture_success");                // finish after the highest populated capture
    emitter.instruction("lsl x14, x12, #4");                                    // scale capture index by the fixed 16-byte pair stride
    emitter.instruction(&format!("ldr x17, [sp, #{}]", regmatches_ptr_off));    // load dynamic offset-pair buffer base
    emitter.instruction("add x14, x17, x14");                                   // compute address of this offset pair
    emitter.instruction("ldr x15, [x14]");                                      // load signed-64-bit capture start
    emitter.instruction("ldr x16, [x14, #8]");                                  // load signed-64-bit capture end
    emitter.instruction("cmp x15, #0");                                         // detect unmatched captures before the highest populated slot
    emitter.instruction("b.lt __rt_preg_match_capture_empty_string");           // emit PHP's empty string for an interior unmatched capture
    emitter.instruction("sub x2, x16, x15");                                    // capture length = rm_eo - rm_so
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_cstr_off));       // reload subject C string base
    emitter.instruction("add x1, x1, x15");                                     // compute capture string pointer
    emitter.instruction("b __rt_preg_match_capture_push");                      // append this capture string
    emitter.label("__rt_preg_match_capture_empty_string");
    emitter.instruction("mov x1, #0");                                          // empty unmatched capture has a null pointer
    emitter.instruction("mov x2, #0");                                          // empty unmatched capture has zero length
    emitter.label("__rt_preg_match_capture_push");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", matches_array_off));      // reload matches array pointer
    emitter.instruction("bl __rt_array_push_str");                              // append capture string to matches array
    emitter.instruction(&format!("str x0, [sp, #{}]", matches_array_off));      // save possibly-grown matches array pointer
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload capture index after helper calls
    emitter.instruction("add x12, x12, #1");                                    // advance to next capture index
    emitter.instruction(&format!("str x12, [sp, #{}]", group_idx_off));         // save next capture index
    emitter.instruction("b __rt_preg_match_capture_group_loop");                // continue materializing captures

    emitter.label("__rt_preg_match_capture_success");
    native_words::emit_release_stack_vector(emitter, regmatches_ptr_off, native_words::RetireSlot::Keep);
    emitter.instruction("mov x0, #1");                                          // report that preg_match found a match
    emitter.instruction(&format!("ldr x1, [sp, #{}]", matches_array_off));      // return the matches array pointer in x1
    emitter.instruction("b __rt_preg_match_capture_ret");                       // share helper epilogue

    emitter.label("__rt_preg_match_capture_no_match");
    native_words::emit_release_stack_vector(emitter, regmatches_ptr_off, native_words::RetireSlot::Keep);
    emitter.instruction("b __rt_preg_match_capture_empty");                     // allocate and return the empty matches array

    emitter.label("__rt_preg_match_capture_malloc_fail");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // reload opaque handle after capture-buffer allocation failed
    emitter.bl_c("elephc_pcre2_v1_free");                                       // free compiled regex resources before returning no match

    emitter.label("__rt_preg_match_capture_compile_fail");
    emitter.instruction("mov x0, #0");                                          // empty array capacity for the failed call
    emitter.instruction("mov x1, #16");                                         // empty matches array still has string slot metadata
    emitter.instruction("bl __rt_array_new");                                   // allocate empty indexed string matches array
    emitter.instruction("mov x1, x0");                                          // return empty matches array in x1
    emitter.instruction("mov x0, #-1");                                         // failure sentinel: PHP false
    emitter.instruction("b __rt_preg_match_capture_ret");                       // return the false sentinel and empty matches
    emitter.label("__rt_preg_match_capture_empty");
    emitter.instruction("mov x0, #0");                                          // empty array capacity for the no-match path
    emitter.instruction("mov x1, #16");                                         // empty matches array still has string slot metadata
    emitter.instruction("bl __rt_array_new");                                   // allocate empty indexed string matches array
    emitter.instruction("mov x1, x0");                                          // return empty matches array in x1
    emitter.instruction("mov x0, #0");                                          // report no match

    emitter.label("__rt_preg_match_capture_ret");
    emitter.instruction(&format!("add x9, sp, #{}", save_off));                 // compute save-slot address for epilogue restore
    emitter.instruction("ldp x29, x30, [x9]");                                  // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate preg_match capture stack frame
    emitter.instruction("ret");                                                 // return match flag in x0 and matches array in x1
}

/// Emits the x86_64 Linux variant of `__rt_preg_match`.
/// Called from `emit_preg_match` when `target.arch == Arch::X86_64`.
/// Uses System V AMD64 ABI: pattern in rdi/rsi, subject in rdx/rcx, result in eax.
/// Compiles and executes through the versioned opaque PCRE2 shim, then frees its handle.
fn emit_preg_match_linux_x86_64(emitter: &mut Emitter) {
    let handle_off = 0;
    let match_slot_count_off = handle_off + 8;
    let match_pair_off = match_slot_count_off + 8;
    let subject_ptr_off = match_pair_off + 16;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_body_ptr_off = flags_off + 8;
    let pattern_body_len_off = pattern_body_ptr_off + 8;
    let subject_cstr_off = pattern_body_len_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let stack_size = (regexec_result_off + 16 + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: preg_match ---");
    emitter.label_global("__rt_preg_match");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving regex compilation scratch storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the regex object, regmatch buffer, and subject spill slots
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve aligned local storage for the opaque handle, pair, and spill fields
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", subject_ptr_off)); // preserve the elephc subject pointer across delimiter stripping and regex compilation helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", subject_len_off)); // preserve the elephc subject length across delimiter stripping and regex compilation helper calls
    emitter.instruction("mov rax, rdi");                                        // move the elephc pattern pointer into the preg-strip helper input register
    emitter.instruction("mov rdx, rsi");                                        // move the elephc pattern length into the preg-strip helper input register
    emitter.instruction("call __rt_preg_strip");                                // strip delimiters and collect supported regex flags from the elephc pattern payload
    emitter.instruction("bt rcx, 30");                                          // did delimiter parsing reject the pattern?
    emitter.instruction("jc __rt_preg_match_fail_linux_x86_64");                // PHP rejects this pattern before compiling it
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", flags_off));  // preserve the regex flag word returned by the delimiter-strip helper for the later regcomp() call
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_body_ptr_off)); // preserve the stripped pattern pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", pattern_body_len_off)); // preserve the stripped pattern length
    super::emit_prepare_regex_locale(emitter);
    emitter.instruction(&format!("lea rdi, [rsp + {}]", handle_off));           // pass opaque-handle output storage
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_body_ptr_off)); // pass the pattern body pointer as the second compile argument
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", pattern_body_len_off)); // pass the authoritative pattern length as the third compile argument
    emitter.instruction(&format!("mov ecx, DWORD PTR [rsp + {}]", flags_off));  // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("lea r8, [rsp + {}]", match_slot_count_off));  // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile_len");                                // compile by length so embedded NULs are preserved
    emitter.instruction("test eax, eax");                                       // did regcomp() succeed and produce a compiled regex object?
    emitter.instruction("jnz __rt_preg_match_fail_linux_x86_64");               // failed regex compilation maps to PHP false
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload the elephc subject pointer before null-terminating it in the secondary scratch buffer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload the elephc subject length before null-terminating it in the secondary scratch buffer
    emitter.instruction("call __rt_cstr2");                                     // materialize a null-terminated C version of the subject string for PCRE2 regex execution
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // preserve the subject C string pointer for the regexec() call and later cleanup path
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // pass compiled opaque handle
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", subject_cstr_off)); // pass the null-terminated subject C string as the second regexec() argument
    emitter.instruction("mov edx, 1");                                          // request only the full-match pair
    emitter.instruction(&format!("lea rcx, [rsp + {}]", match_pair_off));       // receive one fixed signed-64-bit offset pair
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", match_pair_off)); // start the REG_STARTEND range at byte zero
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", subject_len_off)); // load the explicit subject length
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", match_pair_off + 8)); // bound the match at the final subject byte, NULs included
    emitter.instruction("mov r8d, 128");                                        // REG_STARTEND keeps every subject byte in range
    super::preg_error_state::emit_exec_call(emitter);                           // execute without exposing PCRE2-owned layouts
    emitter.instruction(&format!("mov DWORD PTR [rsp + {}], eax", regexec_result_off)); // preserve the regexec() result code across the mandatory regfree() cleanup call
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("mov eax, DWORD PTR [rsp + {}]", regexec_result_off)); // reload the saved regexec() status code after regfree() clobbered caller-saved registers
    emitter.instruction("test eax, eax");                                       // interpret a zero regexec() result as a successful regex match
    emitter.instruction("jnz __rt_preg_match_no_linux_x86_64");                 // return zero when PCRE2 regex execution reports no match
    emitter.instruction("mov eax, 1");                                          // return one when PCRE2 regex execution reports a successful match
    emitter.instruction("jmp __rt_preg_match_ret_linux_x86_64");                // share the common epilogue after materializing the successful match result

    emitter.label("__rt_preg_match_no_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // return zero when the subject does not match
    emitter.instruction("jmp __rt_preg_match_ret_linux_x86_64");                // skip the failure sentinel

    emitter.label("__rt_preg_match_fail_linux_x86_64");
    emitter.instruction("mov rax, -1");                                         // failure sentinel: PHP false
    emitter.instruction("jmp __rt_preg_match_ret_linux_x86_64");                // return the sentinel

    emitter.label("__rt_preg_match_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release the opaque-handle, pair, and subject spill storage before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the regex helper completes
    emitter.instruction("ret");                                                 // return the preg_match() integer result in the x86_64 integer result register
}

/// Emits x86_64 `__rt_preg_match_capture`, returning the match flag and `$matches` array.
fn emit_preg_match_capture_linux_x86_64(emitter: &mut Emitter) {
    let handle_off = 0;
    let regmatches_ptr_off = handle_off + 8;
    let nmatch_off = regmatches_ptr_off + 8;
    let subject_ptr_off = nmatch_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_body_ptr_off = flags_off + 8;
    let pattern_body_len_off = pattern_body_ptr_off + 8;
    let subject_cstr_off = pattern_body_len_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let matches_array_off = regexec_result_off + 8;
    let group_idx_off = matches_array_off + 8;
    let max_group_off = group_idx_off + 8;
    let stack_size = (max_group_off + 32 + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: preg_match_capture ---");
    emitter.label_global("__rt_preg_match_capture");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving capture helper storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for regex and capture spill slots
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve local storage for the opaque handle, pair buffer, and matches state
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", subject_ptr_off)); // preserve the elephc subject pointer across pattern helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", subject_len_off)); // preserve the elephc subject length across pattern helper calls
    emitter.instruction("mov rax, rdi");                                        // move pattern pointer into preg-strip helper input register
    emitter.instruction("mov rdx, rsi");                                        // move pattern length into preg-strip helper input register
    emitter.instruction("call __rt_preg_strip");                                // strip delimiters and collect supported regex flags
    emitter.instruction("bt rcx, 30");                                          // did delimiter parsing reject the pattern?
    emitter.instruction("jc __rt_preg_match_capture_compile_fail_linux_x86_64"); // PHP rejects this pattern before compiling it
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", flags_off));  // save stripped regex flags for regcomp
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_body_ptr_off)); // save the stripped pattern pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", pattern_body_len_off)); // save the stripped pattern length
    super::emit_prepare_regex_locale(emitter);
    emitter.instruction(&format!("lea rdi, [rsp + {}]", handle_off));           // pass opaque-handle output storage
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_body_ptr_off)); // pass the pattern body pointer to regcomp
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", pattern_body_len_off)); // pass the authoritative pattern length to regcomp
    emitter.instruction(&format!("mov ecx, DWORD PTR [rsp + {}]", flags_off));  // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("lea r8, [rsp + {}]", nmatch_off));            // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile_len");                                // compile by length so embedded NULs are preserved
    emitter.instruction("test eax, eax");                                       // did regex compilation succeed?
    emitter.instruction("jnz __rt_preg_match_capture_compile_fail_linux_x86_64"); // compile failure returns PHP false plus an empty matches array
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", nmatch_off)); // load match-slot count returned by the shim
    offset_pairs::emit_allocate(emitter, "rdi", regmatches_ptr_off, "__rt_preg_match_capture_malloc_fail_linux_x86_64");
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload subject pointer before C-string conversion
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload subject length before C-string conversion
    emitter.instruction("call __rt_cstr2");                                     // materialize null-terminated subject for regexec
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // save subject C string across regexec and pushes
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // pass compiled opaque handle
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", subject_cstr_off)); // pass subject C string to regexec
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", nmatch_off)); // request one regmatch slot for every capture group
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // pass dynamic fixed offset-pair buffer
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load the capture pair vector
    emitter.instruction("mov QWORD PTR [r10], 0");                              // start the REG_STARTEND range at byte zero
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", subject_len_off)); // load the explicit subject length
    emitter.instruction("mov QWORD PTR [r10 + 8], r11");                        // bound the match at the final subject byte, NULs included
    emitter.instruction("mov r8d, 128");                                        // REG_STARTEND keeps every subject byte in range
    super::preg_error_state::emit_exec_call(emitter);                           // execute and initialize every requested offset pair
    emitter.instruction(&format!("mov DWORD PTR [rsp + {}], eax", regexec_result_off)); // save regexec status across regfree
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("mov eax, DWORD PTR [rsp + {}]", regexec_result_off)); // reload saved regexec status
    emitter.instruction("test eax, eax");                                       // was there a successful regex match?
    emitter.instruction("jnz __rt_preg_match_capture_no_match_linux_x86_64");   // no match frees capture storage and returns an empty array

    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", nmatch_off));  // reload the dynamic regmatch count
    emitter.instruction("sub r9, 1");                                           // start scanning from the last compiled capture slot
    emitter.label("__rt_preg_match_capture_scan_linux_x86_64");
    emitter.instruction("mov r10, r9");                                         // copy capture index before scaling
    emitter.instruction("shl r10, 4");                                          // scale capture index by the fixed 16-byte pair stride
    emitter.instruction(&format!("mov r12, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic offset-pair buffer base
    emitter.instruction("add r10, r12");                                        // compute address of this offset pair
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load signed-64-bit capture start
    emitter.instruction("cmp r11, 0");                                          // check whether this capture participated
    emitter.instruction("jge __rt_preg_match_capture_scan_found_linux_x86_64"); // use this as the highest emitted capture
    emitter.instruction("test r9, r9");                                         // have we reached the full-match slot?
    emitter.instruction("jz __rt_preg_match_capture_scan_found_linux_x86_64");  // keep at least the full match after successful regexec
    emitter.instruction("sub r9, 1");                                           // move to the previous capture slot
    emitter.instruction("jmp __rt_preg_match_capture_scan_linux_x86_64");       // continue searching for the highest populated capture
    emitter.label("__rt_preg_match_capture_scan_found_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", max_group_off)); // save highest capture index to materialize

    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", nmatch_off)); // allocate enough slots for every compiled capture
    emitter.instruction("mov rsi, 16");                                         // string arrays use pointer/length payload slots
    emitter.instruction("call __rt_array_new");                                 // allocate indexed string matches array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", matches_array_off)); // save matches array pointer across pushes
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", group_idx_off)); // start with capture index zero
    emitter.label("__rt_preg_match_capture_group_loop_linux_x86_64");
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload current capture index
    emitter.instruction(&format!("mov r8, QWORD PTR [rsp + {}]", max_group_off)); // reload highest capture index
    emitter.instruction("cmp r9, r8");                                          // have all required captures been materialized?
    emitter.instruction("jg __rt_preg_match_capture_success_linux_x86_64");     // finish after the highest populated capture
    emitter.instruction("mov r10, r9");                                         // copy capture index before scaling
    emitter.instruction("shl r10, 4");                                          // scale capture index by the fixed 16-byte pair stride
    emitter.instruction(&format!("mov r12, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic offset-pair buffer base
    emitter.instruction("add r10, r12");                                        // compute address of this offset pair
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load signed-64-bit capture start
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load signed-64-bit capture end
    emitter.instruction("cmp r11, 0");                                          // detect unmatched captures before the highest populated slot
    emitter.instruction("jl __rt_preg_match_capture_empty_string_linux_x86_64"); // emit PHP's empty string for an interior unmatched capture
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", subject_cstr_off)); // reload subject C string base
    emitter.instruction("add rsi, r11");                                        // compute capture string pointer
    emitter.instruction("mov rdx, rcx");                                        // copy capture end offset before subtracting start
    emitter.instruction("sub rdx, r11");                                        // capture length = rm_eo - rm_so
    emitter.instruction("jmp __rt_preg_match_capture_push_linux_x86_64");       // append this capture string
    emitter.label("__rt_preg_match_capture_empty_string_linux_x86_64");
    emitter.instruction("xor esi, esi");                                        // empty unmatched capture has a null pointer
    emitter.instruction("xor edx, edx");                                        // empty unmatched capture has zero length
    emitter.label("__rt_preg_match_capture_push_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", matches_array_off)); // reload matches array pointer
    emitter.instruction("call __rt_array_push_str");                            // append capture string to matches array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", matches_array_off)); // save possibly-grown matches array pointer
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload capture index after helper calls
    emitter.instruction("add r9, 1");                                           // advance to next capture index
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", group_idx_off)); // save next capture index
    emitter.instruction("jmp __rt_preg_match_capture_group_loop_linux_x86_64"); // continue materializing captures

    emitter.label("__rt_preg_match_capture_success_linux_x86_64");
    native_words::emit_release_stack_vector(emitter, regmatches_ptr_off, native_words::RetireSlot::Keep);
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", matches_array_off)); // return matches array pointer in rdx
    emitter.instruction("mov eax, 1");                                          // report that preg_match found a match
    emitter.instruction("jmp __rt_preg_match_capture_ret_linux_x86_64");        // share helper epilogue

    emitter.label("__rt_preg_match_capture_no_match_linux_x86_64");
    native_words::emit_release_stack_vector(emitter, regmatches_ptr_off, native_words::RetireSlot::Keep);
    emitter.instruction("jmp __rt_preg_match_capture_empty_linux_x86_64");      // allocate and return the empty matches array

    emitter.label("__rt_preg_match_capture_malloc_fail_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // reload opaque handle after capture-buffer allocation failed
    emitter.bl_c("elephc_pcre2_v1_free");                                       // free compiled regex resources before returning no match

    emitter.label("__rt_preg_match_capture_compile_fail_linux_x86_64");
    emitter.instruction("xor edi, edi");                                        // empty array capacity for the failed call
    emitter.instruction("mov esi, 16");                                         // empty matches array still uses string slot metadata
    emitter.instruction("call __rt_array_new");                                 // allocate empty indexed string matches array
    emitter.instruction("mov rdx, rax");                                        // return empty matches array pointer in rdx
    emitter.instruction("mov rax, -1");                                         // failure sentinel: PHP false
    emitter.instruction("jmp __rt_preg_match_capture_ret_linux_x86_64");        // return the false sentinel and empty matches
    emitter.label("__rt_preg_match_capture_empty_linux_x86_64");
    emitter.instruction("xor edi, edi");                                        // empty array capacity for the no-match path
    emitter.instruction("mov esi, 16");                                         // empty matches array still uses string slot metadata
    emitter.instruction("call __rt_array_new");                                 // allocate empty indexed string matches array
    emitter.instruction("mov rdx, rax");                                        // return empty matches array pointer in rdx
    emitter.instruction("xor eax, eax");                                        // report no match

    emitter.label("__rt_preg_match_capture_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release capture helper local storage
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return match flag in rax and matches array in rdx
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};
    use crate::codegen_support::RuntimeFeatures;

    /// Every supported `preg_match` ABI compiles patterns through the length-aware shim.
    ///
    /// The runtime is emitted separately from user assembly, so this pins the ARM64 and x86_64
    /// helpers that consume the stripped pattern length instead of a NUL-terminated copy.
    #[test]
    fn preg_match_compiles_patterns_by_explicit_length_on_every_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = crate::codegen::generate_runtime_with_features(
                8_388_608,
                target,
                RuntimeFeatures {
                    regex: true,
                    ..RuntimeFeatures::none()
                },
            );
            let start = asm
                .find("\n__rt_preg_match:")
                .unwrap_or_else(|| panic!("{target:?}: preg_match helper is emitted"));
            let rest = &asm[start + 1..];
            let end = rest
                .find("\n__rt_preg_match_capture:")
                .map_or(asm.len(), |offset| start + 1 + offset);
            let helper = &asm[start..end];
            assert!(
                helper.contains("elephc_pcre2_v1_compile_len"),
                "{target:?}: the helper must use the length-aware PCRE2 compile entry point"
            );
            let legacy_calls = helper
                .match_indices("elephc_pcre2_v1_compile")
                .filter(|(index, _)| !helper[*index..].starts_with("elephc_pcre2_v1_compile_len"))
                .count();
            assert_eq!(
                legacy_calls, 0,
                "{target:?}: the helper must not fall back to the NUL-terminated compile"
            );
        }
    }
}
