//! Purpose:
//! Emits the PHP 8.6 userspace `stream_read()` excess-data tracker and the legacy warning.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::platform::emit_platform_runtime`.
//! - `__rt_user_wrapper_fread` after the returned string exceeds the requested count.
//!
//! Key details:
//! - Under PHP 8.6 the helper records the wrapper class name, excess byte count, and the
//!   number of excess events into PHP-visible globals. The injected `fread()` and
//!   `stream_get_contents()` wrappers turn those into `StreamError` objects and apply the
//!   stream context's mode/store/handler policy, including nested operations.
//! - Earlier profiles keep emitting php-src's legacy warning directly, because no PHP-side
//!   dispatcher is injected there.
//! - Both target architectures share one message layout; only register moves differ.

use crate::codegen_support::{abi, compile_php_version, emit::Emitter, platform::Arch};
use crate::php_version::PhpVersion;

const PREFIX: &[u8] = b"Warning: fread(): ";
const AFTER_CLASS: &[u8] = b"::stream_read - read ";
const AFTER_EXCESS: &[u8] = b" bytes more data than requested (";
const AFTER_TOTAL: &[u8] = b" read, 8192 max) - excess data will be lost\n";

/// Emits `__rt_user_wrapper_read_excess_warning`.
pub(crate) fn emit_user_wrapper_read_excess_warning(emitter: &mut Emitter) {
    if compile_php_version() >= PhpVersion::Php86 {
        if emitter.target.arch == Arch::X86_64 {
            emit_x86_64_track(emitter);
        } else {
            emit_aarch64_track(emitter);
        }
        return;
    }
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64_warning(emitter);
    } else {
        emit_aarch64_warning(emitter);
    }
}

/// Returns the PHP-visible global symbol for one stream-read tracking field.
fn tracking_symbol(name: &str) -> String {
    crate::names::ir_global_symbol(name)
}

/// Emits the AArch64 excess tracker used by the PHP 8.6 dispatcher.
fn emit_aarch64_track(emitter: &mut Emitter) {
    let count = tracking_symbol("__elephc_stream_read_count");
    let excess = tracking_symbol("__elephc_stream_read_excess");
    let class = tracking_symbol("__elephc_stream_read_class");

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_read_excess_track ---");
    emitter.label_global("__rt_user_wrapper_read_excess_warning");

    // Frame: 96 bytes. [16] wrapper object, [24] returned, [32] requested,
    // [48] excess, [56] new count, [64] class ptr, [72] class len.
    emitter.instruction("sub sp, sp, #96");                                     // reserve the tracking helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the wrapper object for its class name
    emitter.instruction("str x1, [sp, #24]");                                   // save the returned string length
    emitter.instruction("str x2, [sp, #32]");                                   // save the requested chunk length
    emitter.instruction("sub x9, x1, x2");                                      // excess = returned - requested
    emitter.instruction("str x9, [sp, #48]");                                   // keep the excess byte count

    // -- count = previous event count + 1 --
    abi::emit_symbol_address(emitter, "x10", &count);
    emitter.instruction("ldr x0, [x10]");                                       // previous boxed event count
    emitter.instruction("cbz x0, __rt_uwrt_count_one");                         // a missing count starts at one
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    emitter.instruction("cmp x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("b.ne __rt_uwrt_count_one");                            // a non-int count restarts at one
    emitter.instruction("add x9, x1, #1");                                      // increment the previous event count
    emitter.instruction("b __rt_uwrt_count_ready");                             // continue with the new count
    emitter.label("__rt_uwrt_count_one");
    emitter.instruction("mov x9, #1");                                          // first excess event in this operation
    emitter.label("__rt_uwrt_count_ready");
    emitter.instruction("str x9, [sp, #56]");                                   // preserve the new count across boxing
    abi::emit_symbol_address(emitter, "x10", &count);
    emitter.instruction("ldr x0, [x10]");                                       // reload the previous box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #56]");                                   // new event count payload
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "x10", &count);
    emitter.instruction("str x0, [x10]");                                       // publish the boxed event count

    // -- excess = current event's extra byte count --
    abi::emit_symbol_address(emitter, "x10", &excess);
    emitter.instruction("ldr x0, [x10]");                                       // previous boxed excess
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("ldr x1, [sp, #48]");                                   // excess payload
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "x10", &excess);
    emitter.instruction("str x0, [x10]");                                       // publish the boxed excess

    // -- class name from the dense class-id table --
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the wrapper object
    emitter.instruction("ldr x11, [x11]");                                      // object header word 0 = runtime class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");                                      // number of named class ids
    emitter.instruction("cmp x11, x10");                                        // is the class id within the table?
    emitter.instruction("b.hs __rt_uwrt_anon");                                 // unknown ids contribute no name
    abi::emit_symbol_address(emitter, "x12", "_class_name_entries");
    emitter.instruction("add x12, x12, x11, lsl #4");                           // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("ldr x1, [x12]");                                       // class-name pointer
    emitter.instruction("ldr x2, [x12, #8]");                                   // class-name length
    emitter.instruction("b __rt_uwrt_named");                                   // emit the resolved name
    emitter.label("__rt_uwrt_anon");
    abi::emit_symbol_address(emitter, "x1", "_class_name_missing");             // empty fallback name
    emitter.instruction("mov x2, #0");                                          // zero-length class name
    emitter.label("__rt_uwrt_named");
    emitter.instruction("str x1, [sp, #64]");                                   // preserve the class-name pointer
    emitter.instruction("str x2, [sp, #72]");                                   // preserve the class-name length
    abi::emit_symbol_address(emitter, "x10", &class);
    emitter.instruction("ldr x0, [x10]");                                       // previous boxed class name
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("ldr x1, [sp, #64]");                                   // class-name pointer payload
    emitter.instruction("ldr x2, [sp, #72]");                                   // class-name byte length
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "x10", &class);
    emitter.instruction("str x0, [x10]");                                       // publish the boxed class name

    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the wrapper reader
}

/// Emits the x86_64 excess tracker used by the PHP 8.6 dispatcher.
fn emit_x86_64_track(emitter: &mut Emitter) {
    let count = tracking_symbol("__elephc_stream_read_count");
    let excess = tracking_symbol("__elephc_stream_read_excess");
    let class = tracking_symbol("__elephc_stream_read_class");

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_read_excess_track ---");
    emitter.label_global("__rt_user_wrapper_read_excess_warning");

    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve the tracking scratch slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the wrapper object for its class name
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the returned string length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested chunk length
    emitter.instruction("sub rdx, QWORD PTR [rbp - 24]");                       // excess = returned - requested
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // keep the excess byte count

    // -- count = previous event count + 1 --
    abi::emit_symbol_address(emitter, "r10", &count);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // previous boxed event count
    emitter.instruction("test rax, rax");                                       // is there a previous count?
    emitter.instruction("jz __rt_uwrt_count_one_x86");                          // a missing count starts at one
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    emitter.instruction("test rax, rax");                                       // runtime tag 0 = int
    emitter.instruction("jnz __rt_uwrt_count_one_x86");                         // a non-int count restarts at one
    emitter.instruction("lea r9, [rdi + 1]");                                   // increment the previous event count
    emitter.instruction("jmp __rt_uwrt_count_ready_x86");                       // continue with the new count
    emitter.label("__rt_uwrt_count_one_x86");
    emitter.instruction("mov r9, 1");                                           // first excess event in this operation
    emitter.label("__rt_uwrt_count_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // preserve the new count across boxing
    abi::emit_symbol_address(emitter, "r10", &count);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // reload the previous box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // new event count payload
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a second word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "r10", &count);
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the boxed event count

    // -- excess = current event's extra byte count --
    abi::emit_symbol_address(emitter, "r10", &excess);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // previous boxed excess
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // excess payload
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a second word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "r10", &excess);
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the boxed excess

    // -- class name from the dense class-id table --
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the wrapper object
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // object header word 0 = runtime class id
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");
    emitter.instruction("cmp r11, QWORD PTR [r10]");                            // is the class id within the table?
    emitter.instruction("jae __rt_uwrt_anon_x86");                              // unknown ids contribute no name
    abi::emit_symbol_address(emitter, "r10", "_class_name_entries");
    emitter.instruction("shl r11, 4");                                          // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("add r10, r11");                                        // advance to this class's entry
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // class-name length
    emitter.instruction("jmp __rt_uwrt_named_x86");                             // emit the resolved name
    emitter.label("__rt_uwrt_anon_x86");
    abi::emit_symbol_address(emitter, "rdi", "_class_name_missing");            // empty fallback name
    emitter.instruction("xor esi, esi");                                        // zero-length class name
    emitter.label("__rt_uwrt_named_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // preserve the class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rsi");                       // preserve the class-name length
    abi::emit_symbol_address(emitter, "r10", &class);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // previous boxed class name
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release it before overwriting
    emitter.instruction("mov rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // class-name pointer payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // class-name byte length
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_symbol_address(emitter, "r10", &class);
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the boxed class name

    emitter.instruction("mov rsp, rbp");                                        // discard the helper slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the wrapper reader
}

/// Emits the AArch64 warning helper.
fn emit_aarch64_warning(emitter: &mut Emitter) {
    let prefix = "_uwrw_prefix";
    let prefix_len = PREFIX.len();
    let after_class = "_uwrw_after_class";
    let after_class_len = AFTER_CLASS.len();
    let after_excess = "_uwrw_after_excess";
    let after_excess_len = AFTER_EXCESS.len();
    let after_total = "_uwrw_after_total";
    let after_total_len = AFTER_TOTAL.len();

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_read_excess_warning ---");
    emitter.label_global("__rt_user_wrapper_read_excess_warning");

    // Frame: 96 bytes. [0..16] x29/x30, [16] object, [24] returned, [32] requested,
    //   [40] concat cursor, [48] excess, [56] total.
    emitter.instruction("sub sp, sp, #96");                                     // reserve the warning helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the wrapper object for its class name
    emitter.instruction("str x1, [sp, #24]");                                   // save the returned string length
    emitter.instruction("str x2, [sp, #32]");                                   // save the requested chunk length

    abi::emit_symbol_address(emitter, "x1", prefix);
    emitter.instruction(&format!("mov x2, #{prefix_len}"));                     // warning prefix byte length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");

    // -- class name from the dense class-id table --
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the wrapper object
    emitter.instruction("ldr x11, [x11]");                                      // object header word 0 = runtime class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");                                      // number of named class ids
    emitter.instruction("cmp x11, x10");                                        // is the class id within the table?
    emitter.instruction("b.hs __rt_uwrw_anon");                                 // unknown ids contribute no name
    abi::emit_symbol_address(emitter, "x12", "_class_name_entries");
    emitter.instruction("add x12, x12, x11, lsl #4");                           // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("ldr x1, [x12]");                                       // class-name pointer
    emitter.instruction("ldr x2, [x12, #8]");                                   // class-name length
    emitter.instruction("b __rt_uwrw_named");                                   // emit the resolved name
    emitter.label("__rt_uwrw_anon");
    abi::emit_symbol_address(emitter, "x1", "_class_name_missing");             // empty fallback name
    emitter.instruction("mov x2, #0");                                          // zero-length fragment
    emitter.label("__rt_uwrw_named");
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");

    abi::emit_symbol_address(emitter, "x1", after_class);
    emitter.instruction(&format!("mov x2, #{after_class_len}"));                // `::stream_read - read ` length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");

    // -- excess = returned - requested --
    emitter.instruction("ldr x9, [sp, #24]");                                   // returned length
    emitter.instruction("ldr x10, [sp, #32]");                                  // requested length
    emitter.instruction("sub x9, x9, x10");                                     // excess byte count
    emitter.instruction("str x9, [sp, #48]");                                   // keep excess for the total below
    abi::emit_symbol_address(emitter, "x11", "_concat_off");
    emitter.instruction("ldr x12, [x11]");                                      // snapshot the concat cursor
    emitter.instruction("str x12, [sp, #40]");                                  // preserve it across itoa
    emitter.instruction("mov x0, x9");                                          // format the excess count
    abi::emit_call_label(emitter, "__rt_itoa");
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the pre-itoa concat cursor
    abi::emit_symbol_address(emitter, "x11", "_concat_off");
    emitter.instruction("str x12, [x11]");                                      // restore it for surrounding code

    abi::emit_symbol_address(emitter, "x1", after_excess);
    emitter.instruction(&format!("mov x2, #{after_excess_len}"));               // ` bytes more data than requested (` length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");

    // -- total = 8192 + excess --
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload excess
    emitter.instruction("add x0, x9, #8192");                                   // PHP reports the full 8192-byte chunk
    abi::emit_symbol_address(emitter, "x11", "_concat_off");
    emitter.instruction("ldr x12, [x11]");                                      // snapshot the concat cursor again
    emitter.instruction("str x12, [sp, #40]");                                  // preserve it across itoa
    abi::emit_call_label(emitter, "__rt_itoa");
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the pre-itoa concat cursor
    abi::emit_symbol_address(emitter, "x11", "_concat_off");
    emitter.instruction("str x12, [x11]");                                      // restore it

    abi::emit_symbol_address(emitter, "x1", after_total);
    emitter.instruction(&format!("mov x2, #{after_total_len}"));                // final warning suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");

    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the wrapper reader
}

/// Emits the x86_64 warning helper.
fn emit_x86_64_warning(emitter: &mut Emitter) {
    let prefix = "_uwrw_prefix";
    let prefix_len = PREFIX.len();
    let after_class = "_uwrw_after_class";
    let after_class_len = AFTER_CLASS.len();
    let after_excess = "_uwrw_after_excess";
    let after_excess_len = AFTER_EXCESS.len();
    let after_total = "_uwrw_after_total";
    let after_total_len = AFTER_TOTAL.len();

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_read_excess_warning ---");
    emitter.label_global("__rt_user_wrapper_read_excess_warning");

    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve warning scratch slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the wrapper object for its class name
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the returned string length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested chunk length

    abi::emit_symbol_address(emitter, "rdi", prefix);
    emitter.instruction(&format!("mov esi, {prefix_len}"));                     // warning prefix byte length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");

    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the wrapper object
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // object header word 0 = runtime class id
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");
    emitter.instruction("cmp r11, QWORD PTR [r10]");                            // is the class id within the table?
    emitter.instruction("jae __rt_uwrw_anon_x86");                              // unknown ids contribute no name
    abi::emit_symbol_address(emitter, "r10", "_class_name_entries");
    emitter.instruction("shl r11, 4");                                          // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("add r10, r11");                                        // advance to this class's entry
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // class-name length
    emitter.instruction("jmp __rt_uwrw_named_x86");                             // emit the resolved name
    emitter.label("__rt_uwrw_anon_x86");
    abi::emit_symbol_address(emitter, "rdi", "_class_name_missing");            // empty fallback name
    emitter.instruction("xor esi, esi");                                        // zero-length fragment
    emitter.label("__rt_uwrw_named_x86");
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");

    abi::emit_symbol_address(emitter, "rdi", after_class);
    emitter.instruction(&format!("mov esi, {after_class_len}"));                // `::stream_read - read ` length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");

    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // returned length
    emitter.instruction("sub r9, QWORD PTR [rbp - 24]");                        // excess byte count
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // keep excess for the total below
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // snapshot the concat cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve it across itoa
    emitter.instruction("mov rax, r9");                                         // format the excess count
    abi::emit_call_label(emitter, "__rt_itoa");
    emitter.instruction("mov rdi, rax");                                        // formatted excess pointer
    emitter.instruction("mov rsi, rdx");                                        // formatted excess length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the pre-itoa concat cursor
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);            // restore it for surrounding code

    abi::emit_symbol_address(emitter, "rdi", after_excess);
    emitter.instruction(&format!("mov esi, {after_excess_len}"));               // ` bytes more data than requested (` length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");

    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload excess
    emitter.instruction("add rax, 8192");                                       // PHP reports the full 8192-byte chunk
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // snapshot the concat cursor again
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve it across itoa
    abi::emit_call_label(emitter, "__rt_itoa");
    emitter.instruction("mov rdi, rax");                                        // formatted total pointer
    emitter.instruction("mov rsi, rdx");                                        // formatted total length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the pre-itoa concat cursor
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);            // restore it

    abi::emit_symbol_address(emitter, "rdi", after_total);
    emitter.instruction(&format!("mov esi, {after_total_len}"));                // final warning suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");

    emitter.instruction("mov rsp, rbp");                                        // discard the helper slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the wrapper reader
}
