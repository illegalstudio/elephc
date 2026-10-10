//! Purpose:
//! Emits the `__rt_preg_match_all` count helper and `__rt_preg_match_all_capture`.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - Regex helpers preserve PHP PCRE-flavored inputs for PCRE2 and must preserve match array construction.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Every supported target preserves the original subject and absolute capture offsets.
#[test]
fn preg_match_all_review_runtime_context_on_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen_support::platform::Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        emit_preg_match_all(&mut emitter);
        let runtime = emitter.output();
        assert_eq!(runtime.matches("elephc_pcre2_v1_exec").count(), 2, "{name}");
        let (range_flag, absolute_capture) = match target.arch {
            Arch::AArch64 => ("orr x4, x4, #128", "mov x9, x15"),
            Arch::X86_64 => ("or r8d, 128", "mov r9, r11"),
        };
        assert_eq!(runtime.matches(range_flag).count(), 2, "{name}");
        assert!(runtime.contains(absolute_capture), "{name}");
        assert!(!runtime.contains("advance one byte after a match"), "{name}");
    }
}

/// Selects initial or PCRE2-managed continuation through the private shim flag.
pub(super) fn emit_global_iteration_flags(emitter: &mut Emitter, match_count_off: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [sp, #{}]", match_count_off)); // inspect completed match count
            emitter.instruction("cmp x9, #0");                                  // initial execution has no previous match
            emitter.instruction("cset x4, ne");                                 // request continuation only after success
            emitter.instruction("lsl x4, x4, #16");                             // private PCRE2 global-next flag
            emitter.instruction("orr x4, x4, #128");                            // retain original-subject range execution
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp QWORD PTR [rsp + {}], 0", match_count_off)); // inspect completed match count
            emitter.instruction("setne r8b");                                   // request continuation only after success
            emitter.instruction("movzx r8d, r8b");                              // clear the remaining flag bits
            emitter.instruction("shl r8d, 16");                                 // private PCRE2 global-next flag
            emitter.instruction("or r8d, 128");                                 // retain original-subject range execution
        }
    }
}

/// __rt_preg_match_all: count all non-overlapping matches of regex in subject.
/// Input:  x1=pattern ptr, x2=pattern len, x3=subject ptr, x4=subject len
/// Output: x0=match count
pub(crate) fn emit_preg_match_all(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_match_all_linux_x86_64(emitter);
        super::preg_match_all_capture::emit_preg_match_all_capture(emitter);
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
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let match_count_off = subject_cstr_off + 8;
    let current_pos_off = match_count_off + 8;
    let stack_size = (current_pos_off + 48 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_match_all ---");
    emitter.label_global("__rt_preg_match_all");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_match_all stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // set new frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // save pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // save pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // save subject ptr
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // save subject len

    // -- strip delimiters --
    emitter.instruction("bl __rt_preg_strip");                                  // → x1, x2, x3=flags
    emitter.instruction(&format!("str x3, [sp, #{}]", flags_off));              // save flags

    // -- materialize the PCRE pattern as a C string --
    emitter.instruction("bl __rt_pcre_to_posix");                               // materialize PCRE pattern as a C string
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off));       // save pattern C string

    // -- prepare locale state for regex helpers --
    super::emit_prepare_regex_locale(emitter);

    // -- compile regex --
    emitter.instruction(&format!("add x0, sp, #{}", handle_off));               // pass opaque-handle output storage
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off));       // pass null-terminated pattern
    emitter.instruction(&format!("ldr x2, [sp, #{}]", flags_off));              // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("add x3, sp, #{}", match_slot_count_off));     // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile");                                    // compile without exposing PCRE2-owned layouts
    emitter.instruction("cbnz x0, __rt_preg_match_all_fail");                   // fail

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string
    emitter.bl_c("strlen");                                                      // retain the established C-string subject boundary
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_len_off));        // save the complete subject length for offset matching

    // -- count matches loop --
    emitter.instruction(&format!("str xzr, [sp, #{}]", match_count_off));       // match count = 0
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_cstr_off));       // current position = start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_pos_off));        // save current pos

    emitter.label("__rt_preg_match_all_loop");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_pos_off));        // current subject position
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // pass compiled opaque handle
    emitter.instruction("mov x2, #1");                                          // request only the full-match pair
    emitter.instruction(&format!("add x3, sp, #{}", match_pair_off));           // receive one fixed signed-64-bit offset pair
    emitter.instruction(&format!("ldr x9, [sp, #{}]", current_pos_off));        // reload the search cursor
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_cstr_off));       // pass the original subject for offset matching
    emitter.instruction("sub x9, x9, x1");                                      // compute the absolute starting offset
    emitter.instruction("str x9, [x3]");                                        // publish the input range start
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_len_off));        // load the complete C-string subject length
    emitter.instruction("str x9, [x3, #8]");                                    // publish the input range end
    emit_global_iteration_flags(emitter, match_count_off);
    emitter.bl_c("elephc_pcre2_v1_exec");                                       // execute without exposing PCRE2-owned layouts
    emitter.instruction("cbnz x0, __rt_preg_match_all_done");                   // no more matches

    // -- found a match, increment count --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", match_count_off));        // load count
    emitter.instruction("add x9, x9, #1");                                      // increment
    emitter.instruction(&format!("str x9, [sp, #{}]", match_count_off));        // save count

    // PCRE2 derives continuation offsets/options from its retained match data.
    emitter.instruction("b __rt_preg_match_all_loop");                          // continue

    emitter.label("__rt_preg_match_all_done");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources
    emitter.instruction(&format!("ldr x0, [sp, #{}]", match_count_off));        // return count
    emitter.instruction("b __rt_preg_match_all_ret");                           // return

    emitter.label("__rt_preg_match_all_fail");
    emitter.instruction("mov x0, #0");                                          // return 0 on compile failure

    emitter.label("__rt_preg_match_all_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    super::preg_match_all_capture::emit_preg_match_all_capture(emitter);
}

/// Target-specific implementation of `__rt_preg_match_all` for Linux x86_64.
/// Uses the System V AMD64 ABI: pattern ptr in rdi, pattern len in rsi, subject ptr in rdx, subject len in rcx.
/// Returns the non-overlapping match count in rax.
/// Delegates zero-length progression to PCRE2's UTF/newline-aware global iterator.
fn emit_preg_match_all_linux_x86_64(emitter: &mut Emitter) {
    let handle_off = 0;
    let match_slot_count_off = handle_off + 8;
    let match_pair_off = match_slot_count_off + 8;
    let subject_ptr_off = match_pair_off + 16;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let match_count_off = subject_cstr_off + 8;
    let current_pos_off = match_count_off + 8;
    let stack_size = (current_pos_off + 16 + 15) & !15;
    emitter.blank();
    emitter.comment("--- runtime: preg_match_all ---");
    emitter.label_global("__rt_preg_match_all");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving regex-counting scratch storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the regex object, regmatch buffer, and loop spill slots
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve aligned local storage for the opaque handle, pair, and match count
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", subject_ptr_off)); // preserve the elephc subject pointer across delimiter stripping and regex compilation helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", subject_len_off)); // preserve the elephc subject length across delimiter stripping and regex compilation helper calls
    emitter.instruction("mov rax, rdi");                                        // move the elephc pattern pointer into the delimiter-strip helper input register
    emitter.instruction("mov rdx, rsi");                                        // move the elephc pattern length into the delimiter-strip helper input register
    emitter.instruction("call __rt_preg_strip");                                // strip slash delimiters and gather supported regex flags from the pattern literal
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", flags_off));  // preserve the delimiter-strip helper flags for the later regcomp() call
    emitter.instruction("call __rt_pcre_to_posix");                             // materialize PCRE pattern as a null-terminated C string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // preserve the null-terminated PCRE pattern C string across compilation and loop setup
    super::emit_prepare_regex_locale(emitter);
    emitter.instruction(&format!("lea rdi, [rsp + {}]", handle_off));           // pass opaque-handle output storage
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass the null-terminated PCRE pattern C string as the second regcomp() argument
    emitter.instruction(&format!("mov edx, DWORD PTR [rsp + {}]", flags_off));  // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("lea rcx, [rsp + {}]", match_slot_count_off)); // receive the compiled match-slot count
    emitter.bl_c("elephc_pcre2_v1_compile");                                    // compile without exposing PCRE2-owned layouts
    emitter.instruction("test eax, eax");                                       // did regcomp() succeed and produce a compiled regex object?
    emitter.instruction("jnz __rt_preg_match_all_fail_linux_x86_64");           // failed regex compilation maps to a zero-count result
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload the elephc subject pointer before null-terminating it in the secondary scratch buffer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload the elephc subject length before null-terminating it in the secondary scratch buffer
    emitter.instruction("call __rt_cstr2");                                     // materialize a null-terminated subject C string for repeated PCRE2 regex execution probes
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // preserve the subject C string pointer across the full match-counting loop
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", match_count_off)); // initialize the running non-overlapping match count at zero
    emitter.instruction(                                                        // start the search cursor at the subject beginning
        &format!("mov QWORD PTR [rsp + {}], rax", current_pos_off)
    );
    emitter.instruction("mov rdi, rax");                                        // pass the subject C string to strlen
    emitter.bl_c("strlen");                                                      // retain the established C-string subject boundary
    emitter.instruction(                                                        // save the complete subject length for offset matching
        &format!("mov QWORD PTR [rsp + {}], rax", subject_len_off)
    );

    emitter.label("__rt_preg_match_all_loop_linux_x86_64");
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_pos_off)); // reload the current subject C-string cursor before the next regexec() probe
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // pass compiled opaque handle
    emitter.instruction("mov edx, 1");                                          // request only the full-match pair
    emitter.instruction(&format!("lea rcx, [rsp + {}]", match_pair_off));       // receive one fixed signed-64-bit offset pair
    emitter.instruction("mov r9, rsi");                                         // preserve the search cursor
    emitter.instruction(                                                        // pass the original subject for offset matching
        &format!("mov rsi, QWORD PTR [rsp + {}]", subject_cstr_off)
    );
    emitter.instruction("sub r9, rsi");                                         // compute the absolute starting offset
    emitter.instruction("mov QWORD PTR [rcx], r9");                             // publish the input range start
    emitter.instruction(                                                        // load the complete C-string subject length
        &format!("mov r9, QWORD PTR [rsp + {}]", subject_len_off)
    );
    emitter.instruction("mov QWORD PTR [rcx + 8], r9");                         // publish the input range end
    emit_global_iteration_flags(emitter, match_count_off);
    emitter.bl_c("elephc_pcre2_v1_exec");                                       // execute without exposing PCRE2-owned layouts
    emitter.instruction("test eax, eax");                                       // did regexec() find another match at or after the current cursor?
    emitter.instruction("jnz __rt_preg_match_all_done_linux_x86_64");           // stop counting when regexec() reports no further matches
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", match_count_off)); // reload the running non-overlapping match count before incrementing it
    emitter.instruction("add r9, 1");                                           // count the newly discovered regex match
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", match_count_off)); // preserve the updated match count for the next loop iteration
    // PCRE2 derives continuation offsets/options from its retained match data.
    emitter.instruction("jmp __rt_preg_match_all_loop_linux_x86_64");           // continue counting the remaining non-overlapping regex matches

    emitter.label("__rt_preg_match_all_done_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", handle_off)); // reload compiled opaque handle
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release compiled regex resources before returning the match count
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", match_count_off)); // return the total number of non-overlapping regex matches discovered in the subject
    emitter.instruction("jmp __rt_preg_match_all_ret_linux_x86_64");            // share the common epilogue after the successful match-count path

    emitter.label("__rt_preg_match_all_fail_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // return zero when regex compilation fails for preg_match_all()

    emitter.label("__rt_preg_match_all_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release the opaque-handle, pair, and counting spill storage before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the regex-count helper completes
    emitter.instruction("ret");                                                 // return the preg_match_all() integer count in the x86_64 integer result register
}
