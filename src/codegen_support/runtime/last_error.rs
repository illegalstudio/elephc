//! Purpose:
//! Owns the process's last-diagnostic record, the slot `error_get_last()` publishes.
//!
//! Called from:
//! - `crate::codegen_support::runtime::diagnostics::emit_diagnostics`, unconditionally: every
//!   diagnostic producer (the warning dispatcher, `trigger_error()`) writes the record.
//! - The internal builtins `__elephc_last_error_int` / `__elephc_last_error_str`, which the
//!   `error_handling_prelude` reads it through.
//!
//! Key details:
//! - php records in `php_error_cb`, the INTERNAL handler: every diagnostic a user handler did not
//!   take (none installed, outside its mask, or the handler returned exactly `false`) is recorded,
//!   including one `@` or `error_reporting()` silenced. A handler that took it records nothing and
//!   leaves the previous record standing. The producers call `__rt_last_error_record` exactly on
//!   that path.
//! - The message and file are COPIED into libc storage the record owns. Neither source outlives
//!   the call (the dispatcher frees its joined line; a PHP string may be released), and the
//!   elephc heap is wiped between `--web` requests while this record must not dangle.
//! - Type `0` means "nothing recorded": php's levels are all non-zero, so no separate flag.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Frame bytes for `__rt_last_error_record`: six argument slots plus the frame footer.
const RECORD_FRAME: usize = 80;
const LEVEL: usize = 0;
const MESSAGE_PTR: usize = 8;
const MESSAGE_LEN: usize = 16;
const FILE_PTR: usize = 24;
const FILE_LEN: usize = 32;
const LINE: usize = 40;

/// Data symbols holding the record; declared by `data::fixed`.
pub(crate) const LAST_ERROR_SYMBOLS: [&str; 6] = [
    "_rt_last_error_type",
    "_rt_last_error_line",
    "_rt_last_error_message_ptr",
    "_rt_last_error_message_len",
    "_rt_last_error_file_ptr",
    "_rt_last_error_file_len",
];

/// Emits the record writer, the clear, and the two field readers.
pub(super) fn emit_last_error(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: last diagnostic record ---");
    emit_record(e);
    emit_clear(e);
    emit_int_reader(e);
    emit_str_reader(e);
}

/// `__rt_last_error_record(level, message_ptr, message_len, file_ptr, file_len, line)`.
fn emit_record(e: &mut Emitter) {
    e.label_global("__rt_last_error_record");
    abi::emit_frame_prologue(e, RECORD_FRAME);
    for (index, offset) in [LEVEL, MESSAGE_PTR, MESSAGE_LEN, FILE_PTR, FILE_LEN, LINE]
        .into_iter()
        .enumerate()
    {
        abi::emit_store_to_sp(e, abi::int_arg_reg_name(e.target, index), offset);
    }
    emit_copy_field(e, "message", MESSAGE_PTR, MESSAGE_LEN);
    emit_copy_field(e, "file", FILE_PTR, FILE_LEN);
    let scratch = abi::int_result_reg(e);
    abi::emit_load_temporary_stack_slot(e, scratch, LEVEL);
    abi::emit_store_reg_to_symbol(e, scratch, "_rt_last_error_type", 0);
    abi::emit_load_temporary_stack_slot(e, scratch, LINE);
    abi::emit_store_reg_to_symbol(e, scratch, "_rt_last_error_line", 0);
    abi::emit_frame_restore(e, RECORD_FRAME);
    e.instruction("ret");                                                       // the record now owns copies of both strings
    e.label("__rt_last_error_allocation_failed");
    abi::emit_jump(e, "__rt_heap_allocation_failed");
}

/// Copies one string argument into the record's libc buffer for `field`.
///
/// `realloc` reuses the previous buffer, so a program raising thousands of warnings does not
/// churn the allocator. One spare byte keeps the size non-zero, where `realloc` may free.
fn emit_copy_field(e: &mut Emitter, field: &str, ptr_slot: usize, len_slot: usize) {
    let ptr_symbol = format!("_rt_last_error_{field}_ptr");
    let len_symbol = format!("_rt_last_error_{field}_len");
    let a0 = abi::int_arg_reg_name(e.target, 0);
    let a1 = abi::int_arg_reg_name(e.target, 1);
    let a2 = abi::int_arg_reg_name(e.target, 2);
    let result = abi::int_result_reg(e);
    abi::emit_load_symbol_to_reg(e, a0, &ptr_symbol, 0);
    abi::emit_load_temporary_stack_slot(e, a1, len_slot);
    match e.target.arch {
        Arch::AArch64 => e.instruction("add x1, x1, #1"),                      // reserve one spare byte so the request is never zero-sized
        Arch::X86_64 => e.instruction("add rsi, 1"),                            // reserve one spare byte so the request is never zero-sized
    }
    abi::emit_call_label(e, &e.target.extern_symbol("realloc"));
    abi::emit_branch_if_int_result_zero(e, "__rt_last_error_allocation_failed");
    abi::emit_store_reg_to_symbol(e, result, &ptr_symbol, 0);
    abi::emit_reg_move(e, a0, result);
    abi::emit_load_temporary_stack_slot(e, a1, ptr_slot);
    abi::emit_load_temporary_stack_slot(e, a2, len_slot);
    abi::emit_call_label(e, &e.target.extern_symbol("memcpy"));
    abi::emit_load_temporary_stack_slot(e, result, len_slot);
    abi::emit_store_reg_to_symbol(e, result, &len_symbol, 0);
}

/// `__rt_last_error_clear()`: frees both copies and returns the slot to "nothing recorded".
fn emit_clear(e: &mut Emitter) {
    e.label_global("__rt_last_error_clear");
    abi::emit_frame_prologue(e, 16);
    let a0 = abi::int_arg_reg_name(e.target, 0);
    for field in ["message", "file"] {
        abi::emit_load_symbol_to_reg(e, a0, &format!("_rt_last_error_{field}_ptr"), 0);
        abi::emit_call_label(e, &e.target.extern_symbol("free"));
    }
    for symbol in LAST_ERROR_SYMBOLS {
        abi::emit_store_zero_to_symbol(e, symbol, 0);
    }
    abi::emit_frame_restore(e, 16);
    e.instruction("ret");                                                       // error_get_last() answers null again
}

/// `__rt_last_error_int(field)`: 0 = type (0 when nothing is recorded), 1 = line, 2 = clear.
fn emit_int_reader(e: &mut Emitter) {
    e.label_global("__rt_last_error_int");
    let result = abi::int_result_reg(e);
    match e.target.arch {
        Arch::AArch64 => {
            e.instruction("cmp x0, #2");                                        // field 2 clears the record
            e.instruction("b.eq __rt_last_error_int_clear");                    // run the clear and answer 0
            e.instruction("cmp x0, #1");                                        // field 1 reads the line
            e.instruction("b.eq __rt_last_error_int_line");                     // answer the recorded line
        }
        Arch::X86_64 => {
            e.instruction("cmp rdi, 2");                                        // field 2 clears the record
            e.instruction("je __rt_last_error_int_clear");                      // run the clear and answer 0
            e.instruction("cmp rdi, 1");                                        // field 1 reads the line
            e.instruction("je __rt_last_error_int_line");                       // answer the recorded line
        }
    }
    abi::emit_load_symbol_to_reg(e, result, "_rt_last_error_type", 0);
    e.instruction("ret");                                                       // answer the recorded level, 0 when none
    e.label("__rt_last_error_int_line");
    abi::emit_load_symbol_to_reg(e, result, "_rt_last_error_line", 0);
    e.instruction("ret");                                                       // answer the recorded line
    e.label("__rt_last_error_int_clear");
    abi::emit_frame_prologue(e, 16);
    abi::emit_call_label(e, "__rt_last_error_clear");
    abi::emit_load_int_immediate(e, result, 0);
    abi::emit_frame_restore(e, 16);
    e.instruction("ret");                                                       // the clear answers 0
}

/// `__rt_last_error_str(field)`: 0 = message, 1 = file, as a fresh owned string.
fn emit_str_reader(e: &mut Emitter) {
    e.label_global("__rt_last_error_str");
    abi::emit_frame_prologue(e, 16);
    let (ptr, len) = abi::string_result_regs(e);
    match e.target.arch {
        Arch::AArch64 => {
            e.instruction("cmp x0, #1");                                        // field 1 reads the file
            e.instruction("b.eq __rt_last_error_str_file");                     // load the recorded file
        }
        Arch::X86_64 => {
            e.instruction("cmp rdi, 1");                                        // field 1 reads the file
            e.instruction("je __rt_last_error_str_file");                       // load the recorded file
        }
    }
    abi::emit_load_symbol_to_reg(e, ptr, "_rt_last_error_message_ptr", 0);
    abi::emit_load_symbol_to_reg(e, len, "_rt_last_error_message_len", 0);
    abi::emit_jump(e, "__rt_last_error_str_loaded");
    e.label("__rt_last_error_str_file");
    abi::emit_load_symbol_to_reg(e, ptr, "_rt_last_error_file_ptr", 0);
    abi::emit_load_symbol_to_reg(e, len, "_rt_last_error_file_len", 0);
    e.label("__rt_last_error_str_loaded");
    match e.target.arch {
        Arch::AArch64 => e.instruction("cbnz x2, __rt_last_error_str_copy"),   // a recorded string has bytes to copy
        Arch::X86_64 => {
            e.instruction("test rdx, rdx");                                     // a recorded string has bytes to copy
            e.instruction("jnz __rt_last_error_str_copy");                      // copy it into an owned string
        }
    }
    abi::emit_symbol_address(e, ptr, "_rt_warning_prefix");
    e.label("__rt_last_error_str_copy");
    abi::emit_call_label(e, "__rt_str_persist");
    abi::emit_frame_restore(e, 16);
    e.instruction("ret");                                                       // the caller owns the copy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Both targets emit every entry and own their strings through libc.
    #[test]
    fn last_error_record_emits_every_entry_on_both_targets() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_last_error(&mut emitter);
            let asm = emitter.output();
            for symbol in [
                "__rt_last_error_record:",
                "__rt_last_error_clear:",
                "__rt_last_error_int:",
                "__rt_last_error_str:",
            ] {
                assert!(asm.contains(symbol), "{target:?}: {symbol}");
            }
            assert!(asm.contains(&target.extern_symbol("realloc")), "{target:?}");
            assert!(asm.contains(&target.extern_symbol("free")), "{target:?}");
        }
    }
}
