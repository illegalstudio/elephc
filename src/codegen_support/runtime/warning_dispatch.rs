//! Purpose:
//! Routes ordinary runtime warning lines through PHP error handlers and reporting masks.
//!
//! Called from:
//! - Existing runtime warning producers through __rt_diag_warning.
//!
//! Key details:
//! - Fragments are joined before dispatch, and detached before a reentrant warning.
//! - Producers explicitly finish a diagnostic; newlines inside data never delimit it.
//! - Volatile registers are preserved because legacy producers used a leaf writer.
//! - Every completed diagnostic publishes cleanup before native or host callbacks can throw.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch,
    emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed};
use crate::types::PhpType;

const FRAME: usize = 1024;
const BUFFER: usize = 704;
const LENGTH: usize = 712;
const LEVEL: usize = 720;
const PREFIX: usize = 728;
const ARRAY: usize = 736;
const ARGS: usize = 744;
const RESULT: usize = 752;
const HANDLED: usize = 760;
const COMPLETE: usize = 768;
const ACTIVATION: usize = 800;

/// Selects one instruction while keeping all control-flow and ownership steps shared.
fn ins(e: &mut Emitter, arm: &str, x86: &str) {
    e.instruction(if e.target.arch == Arch::AArch64 { arm } else { x86 });      // emit the matching target instruction for this shared warning step
}

/// Loads one local scratch word into a C argument register.
fn arg(e: &mut Emitter, index: usize, offset: usize) {
    abi::emit_load_temporary_stack_slot(e, abi::int_arg_reg_name(e.target, index), offset);
}

/// Stores the current integer result in the warning frame.
fn save(e: &mut Emitter, offset: usize) {
    abi::emit_store_to_sp(e, abi::int_result_reg(e), offset);
}

/// Preserves or restores every volatile general/SIMD register used by legacy warning producers.
fn volatile_registers(e: &mut Emitter, restore: bool) {
    let arm = e.target.arch == Arch::AArch64;
    let registers = if arm {
        (0..19).map(|index| format!("x{index}")).collect::<Vec<_>>()
    } else {
        ["rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"].into_iter().map(str::to_string).collect()
    };
    for (index, register) in registers.iter().enumerate() {
        if restore { abi::emit_load_temporary_stack_slot(e, register, index * 8); }
        else { abi::emit_store_to_sp(e, register, index * 8); }
    }
    for index in 0..if arm { 32 } else { 16 } {
        let offset = 160 + index * 16;
        let instruction = if arm {
            format!("{} q{index}, [sp, #{offset}]", if restore { "ldr" } else { "str" })
        } else if restore {
            format!("movdqu xmm{index}, XMMWORD PTR [rsp + {offset}]")
        } else {
            format!("movdqu XMMWORD PTR [rsp + {offset}], xmm{index}")
        };
        e.instruction(&instruction);                                            // preserve the complete SIMD value across user callbacks and libc calls
    }
}

/// Saves producer registers before publishing whether this call finishes the diagnostic.
fn emit_entry(e: &mut Emitter, complete: bool) {
    e.label_global(if complete { "__rt_diag_warning" } else { "__rt_diag_warning_fragment" });
    if e.target.arch == Arch::AArch64 {
        // A generic large-frame prologue borrows x9 for its distant footer.
        // Preserve every producer register by using directly addressable single stores.
        e.instruction("sub sp, sp, #1024");                                     // reserve the complete warning frame without scratch registers
        e.instruction("str x29, [sp, #1008]");                                  // preserve caller linkage without borrowing a volatile register
        e.instruction("str x30, [sp, #1016]");                                  // preserve the return address at the fixed frame footer
        e.instruction("add x29, sp, #1008");                                    // establish the standard frame pointer for unwind cleanup
    } else {
        abi::emit_frame_prologue(e, FRAME);
    }
    volatile_registers(e, false);
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), i64::from(complete));
    save(e, COMPLETE);
    abi::emit_jump(e, "__rt_warning_append");
}

/// Joins explicit fragments and dispatches each completed diagnostic exactly once.
pub(super) fn emit_warning_dispatch(e: &mut Emitter) {
    let arm = e.target.arch == Arch::AArch64;
    let result = abi::int_result_reg(e);
    let a0 = abi::int_arg_reg_name(e.target, 0);
    let a1 = abi::int_arg_reg_name(e.target, 1);
    let scratch = if arm { "x10" } else { "r10" };
    let input_ptr = if arm { 8 } else { 32 };
    let input_len = if arm { 16 } else { 24 };
    emit_entry(e, false);
    emit_entry(e, true);
    // Both entries arrive with the same live frame. Keep this continuation in
    // the full-warning atom so ABI analysis starts at its actual prologue.
    // The fragment entry crosses atoms through an unconditional jump only.
    e.label_shared("__rt_warning_append");
    arg(e, 0, input_len);
    abi::emit_reg_move(e, result, a0);
    abi::emit_branch_if_int_result_zero(e, "__rt_warning_done");
    abi::emit_load_symbol_to_reg(e, scratch, "_rt_diag_pending_len", 0);
    abi::emit_store_to_sp(e, scratch, PREFIX);
    arg(e, 1, input_len);
    ins(e, "adds x1, x1, x10", "add rsi, r10");
    ins(e, "b.hs __rt_warning_allocation_failed", "jc __rt_warning_allocation_failed");
    abi::emit_store_to_sp(e, a1, LENGTH);
    abi::emit_load_symbol_to_reg(e, a0, "_rt_diag_pending_ptr", 0);
    abi::emit_call_label(e, &e.target.extern_symbol("realloc"));
    abi::emit_branch_if_int_result_zero(e, "__rt_warning_allocation_failed");
    save(e, BUFFER);
    abi::emit_reg_move(e, a0, result);
    abi::emit_load_temporary_stack_slot(e, scratch, PREFIX);
    ins(e, "add x0, x0, x10", "add rdi, r10");
    arg(e, 1, input_ptr);
    arg(e, 2, input_len);
    abi::emit_call_label(e, &e.target.extern_symbol("memcpy"));
    arg(e, 0, BUFFER);
    arg(e, 1, LENGTH);
    abi::emit_store_reg_to_symbol(e, a0, "_rt_diag_pending_ptr", 0);
    abi::emit_store_reg_to_symbol(e, a1, "_rt_diag_pending_len", 0);
    abi::emit_load_temporary_stack_slot(e, result, COMPLETE);
    abi::emit_branch_if_int_result_zero(e, "__rt_warning_done");
    abi::emit_store_zero_to_symbol(e, "_rt_diag_pending_ptr", 0);
    abi::emit_store_zero_to_symbol(e, "_rt_diag_pending_len", 0);
    for offset in [ARGS, RESULT, HANDLED, PREFIX] {
        abi::emit_load_int_immediate(e, result, 0);
        save(e, offset);
    }
    emit_activation(e);
    abi::emit_load_int_immediate(e, result, 2);
    save(e, LEVEL);
    // Match complete prefixes before stripping them from the handler's message.
    for (symbol, level, prefix, next) in [
        ("_rt_warning_prefix", 2, 9, "__rt_warning_notice"),
        ("_rt_notice_prefix", 8, 8, "__rt_warning_deprecated"),
        ("_rt_deprecated_prefix", 8192, 12, "__rt_warning_category_ready"),
    ] {
        abi::emit_load_temporary_stack_slot(e, result, LENGTH);
        ins(e, &format!("cmp x0, #{prefix}"), &format!("cmp rax, {prefix}"));
        ins(e, &format!("b.lo {next}"), &format!("jb {next}"));
        arg(e, 0, BUFFER);
        abi::emit_symbol_address(e, a1, symbol);
        abi::emit_load_int_immediate(e, abi::int_arg_reg_name(e.target, 2), prefix);
        abi::emit_call_label(e, &e.target.extern_symbol("memcmp"));
        ins(e, "cmp x0, #0", "test eax, eax");
        ins(e, &format!("b.ne {next}"), &format!("jne {next}"));
        abi::emit_load_int_immediate(e, result, level);
        save(e, LEVEL);
        abi::emit_load_int_immediate(e, result, prefix);
        save(e, PREFIX);
        abi::emit_jump(e, "__rt_warning_category_ready");
        e.label(next);
    }
    abi::emit_load_symbol_to_reg(e, result, "_php_error_handler_callable", 0);
    abi::emit_branch_if_int_result_zero(e, "__rt_warning_default");
    abi::emit_load_symbol_to_reg(e, result, "_php_error_handler_mask", 0);
    abi::emit_load_temporary_stack_slot(e, scratch, LEVEL);
    ins(e, "and x0, x0, x10", "and rax, r10");
    abi::emit_branch_if_int_result_zero(e, "__rt_warning_default");
    emit_arguments(e);
    abi::emit_load_symbol_to_reg(e, a0, "_php_error_handler_callable", 0);
    arg(e, 1, ARGS);
    abi::emit_call_label(e, "__rt_error_handler_invoke");
    save(e, RESULT);
    // Only the exact PHP boolean false selects the default warning path.
    ins(e, "ldr x10, [x0]", "mov r10, QWORD PTR [rax]");
    ins(e, "cmp x10, #3", "cmp r10, 3");
    ins(e, "b.ne __rt_warning_handled", "jne __rt_warning_handled");
    ins(e, "ldr x10, [x0, #8]", "mov r10, QWORD PTR [rax + 8]");
    ins(e, "cbz x10, __rt_warning_default", "test r10, r10");
    if !arm {
        e.instruction("jz __rt_warning_default");                              // boolean false selects default output with its cleanup still published
    }
    e.label("__rt_warning_handled");
    abi::emit_load_int_immediate(e, result, 1);
    save(e, HANDLED);
    e.label("__rt_warning_default");
    abi::emit_load_temporary_stack_slot(e, result, HANDLED);
    abi::emit_branch_if_int_result_nonzero(e, "__rt_warning_release");
    abi::emit_load_symbol_to_reg(e, result, "_php_error_reporting", 0);
    abi::emit_load_temporary_stack_slot(e, scratch, LEVEL);
    ins(e, "and x0, x0, x10", "and rax, r10");
    abi::emit_branch_if_int_result_zero(e, "__rt_warning_release");
    // The host extension already formats native warnings through Zend, so a separate
    // newline-only write would surface as an empty warning event in the host handler.
    if !e.phpext_output {
        if arm {
            abi::emit_symbol_address(e, "x1", "_diag_newline");
            abi::emit_load_int_immediate(e, "x2", 1);
        } else {
            abi::emit_symbol_address(e, "rdi", "_diag_newline");
            abi::emit_load_int_immediate(e, "rsi", 1);
        }
        abi::emit_call_label(e, "__rt_diag_write");                             // PHP starts each default diagnostic on its own blank line
    }
    if arm {
        arg(e, 1, BUFFER);
        arg(e, 2, LENGTH);
    } else {
        arg(e, 0, BUFFER);
        arg(e, 1, LENGTH);
    }
    abi::emit_call_label(e, "__rt_diag_write");
    e.label("__rt_warning_release");
    abi::emit_store_zero_to_symbol(e, "_rt_diag_warning_message_len", 0);
    abi::emit_load_temporary_stack_slot(e, scratch, ACTIVATION);
    abi::emit_store_reg_to_symbol(e, scratch, "_exc_call_frame_top", 0);
    abi::emit_temporary_stack_address(e, a0, 0);
    abi::emit_call_label(e, "__rt_warning_cleanup");
    e.label("__rt_warning_done");
    volatile_registers(e, true);
    if arm {
        e.instruction("ldr x30, [sp, #1016]");                                  // restore linkage without clobbering the producer's saved x9
        e.instruction("ldr x29, [sp, #1008]");                                  // reload the caller frame pointer from the fixed footer
        e.instruction("add sp, sp, #1024");                                     // release the frame while preserving every volatile register
    } else {
        abi::emit_frame_restore(e, FRAME);
    }
    e.instruction("ret");                                                       // preserve the legacy warning producer's live registers
    // Conditional branches stay inside this helper's Mach-O atom. Only the
    // unconditional jump crosses to the allocator's shared recovery entry.
    e.label("__rt_warning_allocation_failed");
    abi::emit_jump(e, "__rt_heap_allocation_failed");
    emit_cleanup(e);
    emit_strip_location(e);
    super::arrays::emit_native_frame_owners(e, "__rt_gc_warning_frame_owners", &[ARGS, RESULT]);
    emit_reset(e);
}

/// Emits `__rt_diag_strip_location(ptr, len)`, which removes a trailing
/// `" in <file> on line <N>"` from a handler-facing diagnostic message.
///
/// PHP passes the bare message to a user error handler and provides the file and line as
/// separate arguments. Producers that build one complete default-output line (the ctype
/// deprecations) therefore hand this helper the complete buffer; the default-output path
/// keeps it untouched while the handler argument is cut at the location separator.
fn emit_strip_location(e: &mut Emitter) {
    if e.target.arch == Arch::AArch64 {
        emit_strip_location_aarch64(e);
    } else {
        emit_strip_location_x86_64(e);
    }
}

/// AArch64 variant of `__rt_diag_strip_location`.
fn emit_strip_location_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: diag_strip_location ---");
    e.label_global("__rt_diag_strip_location");
    e.instruction("cmp x2, #9");                                                // a location suffix needs at least " on line "
    e.instruction("b.lo __rt_diag_strip_location_done");                        // shorter messages carry no location suffix
    e.instruction("mov x9, x2");                                                // start the backward search at the message end
    e.instruction("sub x9, x9, #9");                                            // select the last possible " on line " offset
    e.label("__rt_diag_strip_location_outer");
    e.instruction("add x11, x1, x9");                                           // address the candidate line separator
    abi::emit_symbol_address(e, "x12", "_diag_location_on_line");
    e.instruction("mov x13, #0");                                               // compare the marker from its first byte
    e.label("__rt_diag_strip_location_outer_cmp");
    e.instruction("ldrb w14, [x11, x13]");                                      // load one candidate line-separator byte
    e.instruction("ldrb w15, [x12, x13]");                                      // load the matching marker byte
    e.instruction("cmp w14, w15");                                               // compare the two bytes
    e.instruction("b.ne __rt_diag_strip_location_outer_miss");                  // try the previous candidate offset
    e.instruction("add x13, x13, #1");                                          // advance the marker comparison
    e.instruction("cmp x13, #9");                                               // has the complete " on line " matched?
    e.instruction("b.ne __rt_diag_strip_location_outer_cmp");                   // continue comparing the marker
    e.instruction("cmp x9, #4");                                                // does a preceding " in " fit?
    e.instruction("b.lo __rt_diag_strip_location_done");                        // no separator fits before the line marker
    e.instruction("sub x10, x9, #4");                                           // select the last possible " in " offset
    e.label("__rt_diag_strip_location_inner");
    e.instruction("add x11, x1, x10");                                          // address the candidate location separator
    abi::emit_symbol_address(e, "x12", "_diag_location_in");
    e.instruction("mov x13, #0");                                               // compare the marker from its first byte
    e.label("__rt_diag_strip_location_inner_cmp");
    e.instruction("ldrb w14, [x11, x13]");                                      // load one candidate location-separator byte
    e.instruction("ldrb w15, [x12, x13]");                                      // load the matching marker byte
    e.instruction("cmp w14, w15");                                               // compare the two bytes
    e.instruction("b.ne __rt_diag_strip_location_inner_miss");                  // try the previous candidate offset
    e.instruction("add x13, x13, #1");                                          // advance the marker comparison
    e.instruction("cmp x13, #4");                                               // has the complete " in " matched?
    e.instruction("b.ne __rt_diag_strip_location_inner_cmp");                   // continue comparing the marker
    e.instruction("mov x2, x10");                                               // cut the handler message before " in "
    e.instruction("b __rt_diag_strip_location_done");                           // return the stripped message length
    e.label("__rt_diag_strip_location_inner_miss");
    e.instruction("cbz x10, __rt_diag_strip_location_done");                    // no earlier separator exists
    e.instruction("sub x10, x10, #1");                                          // try the previous " in " offset
    e.instruction("b __rt_diag_strip_location_inner");                          // continue the backward search
    e.label("__rt_diag_strip_location_outer_miss");
    e.instruction("cbz x9, __rt_diag_strip_location_done");                     // no earlier line marker exists
    e.instruction("sub x9, x9, #1");                                            // try the previous " on line " offset
    e.instruction("b __rt_diag_strip_location_outer");                          // continue the backward search
    e.label("__rt_diag_strip_location_done");
    e.instruction("ret");                                                       // return with the original or stripped length
}

/// x86_64 variant of `__rt_diag_strip_location`.
fn emit_strip_location_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: diag_strip_location ---");
    e.label_global("__rt_diag_strip_location");
    e.instruction("cmp rsi, 9");                                                // a location suffix needs at least " on line "
    e.instruction("jb __rt_diag_strip_location_done");                          // shorter messages carry no location suffix
    e.instruction("mov r8, rsi");                                               // start the backward search at the message end
    e.instruction("sub r8, 9");                                                 // select the last possible " on line " offset
    e.label("__rt_diag_strip_location_outer");
    e.instruction("lea r11, [rdi + r8]");                                       // address the candidate line separator
    e.instruction("lea r9, [rip + _diag_location_on_line]");                    // materialize the line-separator marker
    e.instruction("xor r10d, r10d");                                            // compare the marker from its first byte
    e.label("__rt_diag_strip_location_outer_cmp");
    e.instruction("movzx eax, BYTE PTR [r11 + r10]");                           // load one candidate line-separator byte
    e.instruction("movzx ecx, BYTE PTR [r9 + r10]");                            // load the matching marker byte
    e.instruction("cmp eax, ecx");                                              // compare the two bytes
    e.instruction("jne __rt_diag_strip_location_outer_miss");                   // try the previous candidate offset
    e.instruction("add r10, 1");                                                // advance the marker comparison
    e.instruction("cmp r10, 9");                                                // has the complete " on line " matched?
    e.instruction("jne __rt_diag_strip_location_outer_cmp");                    // continue comparing the marker
    e.instruction("cmp r8, 4");                                                 // does a preceding " in " fit?
    e.instruction("jb __rt_diag_strip_location_done");                          // no separator fits before the line marker
    e.instruction("lea r8, [r8 - 4]");                                          // select the last possible " in " offset
    e.label("__rt_diag_strip_location_inner");
    e.instruction("lea r11, [rdi + r8]");                                       // address the candidate location separator
    e.instruction("lea r9, [rip + _diag_location_in]");                         // materialize the location-separator marker
    e.instruction("xor r10d, r10d");                                            // compare the marker from its first byte
    e.label("__rt_diag_strip_location_inner_cmp");
    e.instruction("movzx eax, BYTE PTR [r11 + r10]");                           // load one candidate location-separator byte
    e.instruction("movzx ecx, BYTE PTR [r9 + r10]");                            // load the matching marker byte
    e.instruction("cmp eax, ecx");                                              // compare the two bytes
    e.instruction("jne __rt_diag_strip_location_inner_miss");                   // try the previous candidate offset
    e.instruction("add r10, 1");                                                // advance the marker comparison
    e.instruction("cmp r10, 4");                                                // has the complete " in " matched?
    e.instruction("jne __rt_diag_strip_location_inner_cmp");                    // continue comparing the marker
    e.instruction("mov rsi, r8");                                               // cut the handler message before " in "
    e.instruction("jmp __rt_diag_strip_location_done");                         // return the stripped message length
    e.label("__rt_diag_strip_location_inner_miss");
    e.instruction("test r8, r8");                                               // no earlier separator exists
    e.instruction("jz __rt_diag_strip_location_done");                          // stop at the message start
    e.instruction("sub r8, 1");                                                 // try the previous " in " offset
    e.instruction("jmp __rt_diag_strip_location_inner");                        // continue the backward search
    e.label("__rt_diag_strip_location_outer_miss");
    e.instruction("test r8, r8");                                               // no earlier line marker exists
    e.instruction("jz __rt_diag_strip_location_done");                          // stop at the message start
    e.instruction("sub r8, 1");                                                 // try the previous " on line " offset
    e.instruction("jmp __rt_diag_strip_location_outer");                        // continue the backward search
    e.label("__rt_diag_strip_location_done");
    e.instruction("ret");                                                       // return with the original or stripped length
}

/// Builds the four handler arguments with copied strings and owned boxed cells.
fn emit_arguments(e: &mut Emitter) {
    let arm = e.target.arch == Arch::AArch64;
    let result = abi::int_result_reg(e);
    let scratch = if arm { "x10" } else { "r10" };
    for (index, value) in [(0, 4), (1, 8)] {
        abi::emit_load_int_immediate(e, abi::int_arg_reg_name(e.target, index), value);
    }
    abi::emit_call_label(e, "__rt_array_new");
    crate::codegen_support::emit_array_value_type_stamp(e, result, &PhpType::Mixed);
    save(e, ARRAY);
    for index in 0..4 {
        match index {
            0 => {
                abi::emit_load_temporary_stack_slot(e, result, LEVEL);
                emit_box_current_value_as_mixed(e, &PhpType::Int);
            }
            1 => {
                let (ptr, len) = abi::string_result_regs(e);
                abi::emit_load_temporary_stack_slot(e, ptr, BUFFER);
                // A producer may append ` in <file> on line <n>` for the default output.
                // PHP's error handler receives the message alone, so prefer the length the
                // producer recorded before appending that suffix; otherwise use the whole
                // buffered diagnostic.
                abi::emit_load_symbol_to_reg(e, scratch, "_rt_diag_warning_message_len", 0);
                ins(e, "cbnz x10, __rt_warning_message_length_ready", "test r10, r10");
                if !arm { e.instruction("jnz __rt_warning_message_length_ready"); } // keep a recorded message length
                abi::emit_load_temporary_stack_slot(e, scratch, LENGTH);
                e.label("__rt_warning_message_length_ready");
                abi::emit_reg_move(e, len, scratch);
                abi::emit_load_temporary_stack_slot(e, scratch, PREFIX);
                ins(e, "add x1, x1, x10", "add rax, r10");
                ins(e, "sub x2, x2, x10", "sub rdx, r10");
                ins(e, "cbz x2, __rt_warning_message_ready", "test rdx, rdx");
                if !arm {
                    e.instruction("jz __rt_warning_message_ready");             // an empty message has no terminal byte to strip
                }
                ins(e, "add x10, x1, x2", "lea r10, [rax + rdx]");
                ins(e, "ldrb w10, [x10, #-1]", "movzx r10d, BYTE PTR [r10 - 1]");
                ins(e, "cmp w10, #10", "cmp r10d, 10");
                ins(e, "b.ne __rt_warning_message_ready", "jne __rt_warning_message_ready");
                ins(e, "sub x2, x2, #1", "sub rdx, 1");
                e.label("__rt_warning_message_ready");
                if arm {
                    abi::emit_call_label(e, "__rt_diag_strip_location");
                } else {
                    abi::emit_reg_move(e, "rdi", "rax");
                    abi::emit_reg_move(e, "rsi", "rdx");
                    abi::emit_call_label(e, "__rt_diag_strip_location");
                    abi::emit_reg_move(e, "rax", "rdi");
                    abi::emit_reg_move(e, "rdx", "rsi");
                }
                emit_box_current_value_as_mixed(e, &PhpType::Str);
            }
            2 => {
                let (ptr, len) = abi::string_result_regs(e);
                abi::emit_load_symbol_to_reg(e, ptr, "_php_diagnostic_file", 0);
                abi::emit_load_symbol_to_reg(e, len, "_php_diagnostic_file_len", 0);
                emit_box_current_value_as_mixed(e, &PhpType::Str);
            }
            _ => {
                abi::emit_load_symbol_to_reg(e, result, "_php_diagnostic_line", 0);
                emit_box_current_value_as_mixed(e, &PhpType::Int);
            }
        }
        abi::emit_load_temporary_stack_slot(e, scratch, ARRAY);
        abi::emit_store_to_address(e, result, scratch, 24 + index * 8);
    }
    abi::emit_load_temporary_stack_slot(e, result, ARRAY);
    abi::emit_load_int_immediate(e, scratch, 4);
    abi::emit_store_to_address(e, scratch, result, 0);
    emit_box_current_owned_value_as_mixed(e, &PhpType::Array(Box::new(PhpType::Mixed)));
    save(e, ARGS);
}

/// Publishes an owning cleanup activation before entering arbitrary user code.
fn emit_activation(e: &mut Emitter) {
    let scratch = if e.target.arch == Arch::AArch64 { "x10" } else { "r10" };
    abi::emit_load_symbol_to_reg(e, scratch, "_exc_call_frame_top", 0);
    abi::emit_store_to_sp(e, scratch, ACTIVATION);
    abi::emit_symbol_address(e, scratch, "__rt_warning_cleanup");
    abi::emit_store_to_sp(e, scratch, ACTIVATION + 8);
    abi::emit_temporary_stack_address(e, scratch, 0);
    abi::emit_store_to_sp(e, scratch, ACTIVATION + 16);
    abi::emit_load_int_immediate(e, scratch, 0);
    abi::emit_store_to_sp(e, scratch, ACTIVATION + 24);
    abi::emit_store_to_sp(e, scratch, ACTIVATION + 32);
    abi::emit_symbol_address(e, scratch, "__rt_gc_warning_frame_owners");
    abi::emit_store_to_sp(e, scratch, ACTIVATION + 40);
    abi::emit_temporary_stack_address(e, scratch, ACTIVATION);
    abi::emit_store_reg_to_symbol(e, scratch, "_exc_call_frame_top", 0);
}

/// Releases the detached warning buffer and any constructed callback/result boxes.
fn emit_cleanup(e: &mut Emitter) {
    let a0 = abi::int_arg_reg_name(e.target, 0);
    let result = abi::int_result_reg(e);
    e.label_global("__rt_warning_cleanup");
    abi::emit_frame_prologue(e, 32);
    abi::emit_store_to_sp(e, a0, 0);
    for (offset, helper) in [(ARGS, "__rt_decref_mixed"), (RESULT, "__rt_decref_mixed")] {
        abi::emit_load_temporary_stack_slot(e, result, 0);
        let record = if e.target.arch == Arch::AArch64 { "x10" } else { "r10" };
        abi::emit_reg_move(e, record, result);
        abi::emit_load_from_address(e, result, record, offset);
        abi::emit_load_int_immediate(e, if e.target.arch == Arch::AArch64 { "x11" } else { "r11" }, 0);
        abi::emit_store_to_address(e, if e.target.arch == Arch::AArch64 { "x11" } else { "r11" }, record, offset);
        abi::emit_call_label(e, helper);
    }
    abi::emit_load_temporary_stack_slot(e, a0, 0);
    abi::emit_load_from_address(e, a0, a0, BUFFER);
    abi::emit_call_label(e, &e.target.extern_symbol("free"));
    abi::emit_frame_restore(e, 32);
    e.instruction("ret");                                                       // release warning-owned state on normal return and native unwind
}

/// Discards incomplete diagnostic fragments at request/process teardown.
fn emit_reset(e: &mut Emitter) {
    e.label_global("__rt_diag_reset");
    abi::emit_frame_prologue(e, 16);
    let a0 = abi::int_arg_reg_name(e.target, 0);
    abi::emit_load_symbol_to_reg(e, a0, "_rt_diag_pending_ptr", 0);
    abi::emit_call_label(e, &e.target.extern_symbol("free"));
    abi::emit_store_zero_to_symbol(e, "_rt_diag_pending_ptr", 0);
    abi::emit_store_zero_to_symbol(e, "_rt_diag_pending_len", 0);
    abi::emit_frame_restore(e, 16);
    e.instruction("ret");                                                       // never join warning fragments from distinct web requests
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// All supported targets emit the owning dispatcher, full register saves, and mangled libc calls.
    #[test]
    fn warning_dispatch_emits_every_supported_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_warning_dispatch(&mut emitter);
            let asm = emitter.output();
            for symbol in ["__rt_diag_warning:", "__rt_diag_warning_fragment:", "__rt_warning_append:", "__rt_warning_cleanup:", "__rt_diag_reset:", "__rt_error_handler_invoke"] {
                assert!(asm.contains(symbol), "{target:?}: {symbol}");
            }
            assert!(asm.contains(&target.extern_symbol("realloc")), "{target:?}");
            assert!(!asm.contains(".globl __rt_warning_append"), "{target:?}: continuation is not a call entry");
            assert!(!asm.contains(".text.__rt_warning_append"), "{target:?}: continuation must inherit its frame");
            assert!(asm.contains(if target.arch == Arch::AArch64 { "str q31" } else { "movdqu XMMWORD PTR" }), "{target:?}");
            assert!(asm.contains("__rt_warning_allocation_failed:"), "{target:?}");
            assert!(asm.contains(if target.arch == Arch::AArch64 {
                "b __rt_heap_allocation_failed"
            } else {
                "jmp __rt_heap_allocation_failed"
            }), "{target:?}");
            assert!(!asm.contains("__rt_heap_exhausted"), "{target:?}: allocator-local label escaped");
        }
    }

    /// Host extension builds let Zend format the diagnostic and never publish a newline-only event.
    #[test]
    fn warning_dispatch_skips_newline_event_for_phpext_output() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut native = Emitter::new(target);
            emit_warning_dispatch(&mut native);
            assert!(native.output().contains("_diag_newline"), "{target:?}: native output keeps the leading blank line");

            let mut hosted = Emitter::new(target);
            hosted.phpext_output = true;
            emit_warning_dispatch(&mut hosted);
            assert!(!hosted.output().contains("_diag_newline"), "{target:?}: host output must not create an empty warning event");
        }
    }
}
