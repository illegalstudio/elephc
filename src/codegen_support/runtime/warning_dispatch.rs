//! Purpose:
//! Routes ordinary runtime warning lines through PHP error handlers and reporting masks.
//!
//! Called from:
//! - Existing runtime warning producers through __rt_diag_warning.
//!
//! Key details:
//! - Fragments are joined before dispatch, and detached before a reentrant warning.
//! - Volatile registers are preserved because legacy producers used a leaf writer.
//! - An unwind activation releases owned message and argument storage on throws.

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

/// Joins legacy warning fragments and dispatches each completed line exactly once.
pub(super) fn emit_warning_dispatch(e: &mut Emitter) {
    let arm = e.target.arch == Arch::AArch64;
    let result = abi::int_result_reg(e);
    let a0 = abi::int_arg_reg_name(e.target, 0);
    let a1 = abi::int_arg_reg_name(e.target, 1);
    let scratch = if arm { "x10" } else { "r10" };
    let input_ptr = if arm { 8 } else { 32 };
    let input_len = if arm { 16 } else { 24 };
    e.label_global("__rt_diag_warning");
    abi::emit_frame_prologue(e, FRAME);
    volatile_registers(e, false);
    arg(e, 0, input_len);
    abi::emit_reg_move(e, result, a0);
    abi::emit_branch_if_int_result_zero(e, "__rt_warning_done");
    abi::emit_load_symbol_to_reg(e, scratch, "_rt_diag_pending_len", 0);
    abi::emit_store_to_sp(e, scratch, PREFIX);
    arg(e, 1, input_len);
    ins(e, "adds x1, x1, x10", "add rsi, r10");
    ins(e, "b.hs __rt_heap_exhausted", "jc __rt_heap_exhausted");
    abi::emit_store_to_sp(e, a1, LENGTH);
    abi::emit_load_symbol_to_reg(e, a0, "_rt_diag_pending_ptr", 0);
    abi::emit_call_label(e, &e.target.extern_symbol("realloc"));
    abi::emit_branch_if_int_result_zero(e, "__rt_heap_exhausted");
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
    ins(e, "add x10, x0, x1", "lea r10, [rdi + rsi]");
    ins(e, "ldrb w10, [x10, #-1]", "movzx r10d, BYTE PTR [r10 - 1]");
    ins(e, "cmp w10, #10", "cmp r10d, 10");
    ins(e, "b.ne __rt_warning_done", "jne __rt_warning_done");
    abi::emit_store_zero_to_symbol(e, "_rt_diag_pending_ptr", 0);
    abi::emit_store_zero_to_symbol(e, "_rt_diag_pending_len", 0);
    for offset in [ARGS, RESULT, HANDLED, PREFIX] {
        abi::emit_load_int_immediate(e, result, 0);
        save(e, offset);
    }
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
    emit_activation(e);
    abi::emit_load_symbol_to_reg(e, a0, "_php_error_handler_callable", 0);
    arg(e, 1, ARGS);
    abi::emit_call_label(e, "__rt_error_handler_invoke");
    save(e, RESULT);
    // Only the exact PHP boolean false selects the default warning path.
    ins(e, "ldr x10, [x0]", "mov r10, QWORD PTR [rax]");
    ins(e, "cmp x10, #3", "cmp r10, 3");
    ins(e, "b.ne __rt_warning_handled", "jne __rt_warning_handled");
    ins(e, "ldr x10, [x0, #8]", "mov r10, QWORD PTR [rax + 8]");
    ins(e, "cbz x10, __rt_warning_unpublish", "test r10, r10");
    if !arm { e.instruction("jz __rt_warning_unpublish"); } // boolean false falls through to default output
    e.label("__rt_warning_handled");
    abi::emit_load_int_immediate(e, result, 1);
    save(e, HANDLED);
    e.label("__rt_warning_unpublish");
    abi::emit_load_temporary_stack_slot(e, scratch, ACTIVATION);
    abi::emit_store_reg_to_symbol(e, scratch, "_exc_call_frame_top", 0);
    e.label("__rt_warning_default");
    abi::emit_load_temporary_stack_slot(e, result, HANDLED);
    abi::emit_branch_if_int_result_nonzero(e, "__rt_warning_release");
    abi::emit_load_symbol_to_reg(e, result, "_php_error_reporting", 0);
    abi::emit_load_temporary_stack_slot(e, scratch, LEVEL);
    ins(e, "and x0, x0, x10", "and rax, r10");
    abi::emit_branch_if_int_result_zero(e, "__rt_warning_release");
    if arm {
        arg(e, 1, BUFFER);
        arg(e, 2, LENGTH);
    } else {
        arg(e, 0, BUFFER);
        arg(e, 1, LENGTH);
    }
    abi::emit_call_label(e, "__rt_diag_write");
    e.label("__rt_warning_release");
    abi::emit_temporary_stack_address(e, a0, 0);
    abi::emit_call_label(e, "__rt_warning_cleanup");
    e.label("__rt_warning_done");
    volatile_registers(e, true);
    abi::emit_frame_restore(e, FRAME);
    e.instruction("ret");                                                       // preserve the legacy warning producer's live registers
    emit_cleanup(e);
    emit_reset(e);
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
                abi::emit_load_temporary_stack_slot(e, len, LENGTH);
                abi::emit_load_temporary_stack_slot(e, scratch, PREFIX);
                ins(e, "add x1, x1, x10", "add rax, r10");
                ins(e, "sub x2, x2, x10", "sub rdx, r10");
                ins(e, "sub x2, x2, #1", "sub rdx, 1");
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
        abi::emit_load_from_address(e, result, result, offset);
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
            for symbol in ["__rt_diag_warning:", "__rt_warning_cleanup:", "__rt_diag_reset:", "__rt_error_handler_invoke"] {
                assert!(asm.contains(symbol), "{target:?}: {symbol}");
            }
            assert!(asm.contains(&target.extern_symbol("realloc")), "{target:?}");
            assert!(asm.contains(if target.arch == Arch::AArch64 { "str q31" } else { "movdqu XMMWORD PTR" }), "{target:?}");
        }
    }
}
