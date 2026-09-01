//! Purpose:
//! Formats php-src-compatible native ext/date uncaught stack traces.
//!
//! Called from:
//! - `super::throw_current` when native DateTime/DatePeriod trace state is active.
//!
//! Key details:
//! - Emits target-specific trace text and exits with PHP's fatal status.

use crate::codegen_support::sentinels::THROWABLE_CREATION_LINE_OFFSET;
use crate::codegen_support::{abi, emit::Emitter};

use super::super::data::{
    UNCAUGHT_DATEPERIOD_STACK_PREFIX, UNCAUGHT_DATETIME_FORMAT_PARENT_PREFIX,
    UNCAUGHT_DATETIME_FORMAT_STACK_PREFIX, UNCAUGHT_DATETIME_FORMAT_STACK_SUFFIX,
    UNCAUGHT_TIMEZONE_OFFSET_STACK_PREFIX, UNCAUGHT_TIMEZONE_OFFSET_STACK_SUFFIX,
    UNCAUGHT_TRACE_CLASS_SEPARATOR, UNCAUGHT_TRACE_LINE_PREFIX, UNCAUGHT_TRACE_LOCATION_SEPARATOR,
    UNCAUGHT_TRACE_NEWLINE, UNCAUGHT_TRACE_NEXT_PREFIX, UNCAUGHT_TRACE_NEXT_STACK_PREFIX,
    UNCAUGHT_TRACE_PREFIX,
    UNCAUGHT_UNSERIALIZE_CALL_AFTER_LINE, UNCAUGHT_UNSERIALIZE_CALL_PREFIX,
    UNCAUGHT_UNSERIALIZE_CALL_SUFFIX, UNCAUGHT_UNSERIALIZE_OWNER_SUFFIX,
    UNCAUGHT_UNSERIALIZE_STACK_PREFIX, UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX,
};

const PHP_FATAL_EXIT_STATUS: u32 = 255;

/// Emits php-src's uncaught native-`__unserialize()` trace on AArch64 and exits.
pub(super) fn emit_uncaught_unserialize_trace_aarch64(emitter: &mut Emitter) {
    emit_uncaught_unserialize_trace_body_aarch64(emitter, "_exc_value");
    abi::emit_exit(emitter, PHP_FATAL_EXIT_STATUS);
}

/// Emits the preserved native-unserialize trace without terminating the process.
fn emit_uncaught_unserialize_trace_body_aarch64(
    emitter: &mut Emitter,
    exception_symbol: &str,
) {
    emit_aarch64_static_write(emitter, "_uncaught_trace_prefix", UNCAUGHT_TRACE_PREFIX.len());

    abi::emit_load_symbol_to_reg(emitter, "x20", exception_symbol, 0);
    emitter.instruction("ldr x9, [x20]");                                       // throwable class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // select the throwable class-name row
    emitter.instruction("ldp x1, x2, [x10]");                                  // class-name pointer and byte length
    emit_aarch64_dynamic_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_class_separator",
        UNCAUGHT_TRACE_CLASS_SEPARATOR.len(),
    );

    emitter.instruction("ldr x1, [x20, #8]");                                  // Throwable::$message pointer
    emitter.instruction("ldr x2, [x20, #16]");                                 // Throwable::$message byte length
    emit_aarch64_dynamic_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_location_separator",
        UNCAUGHT_TRACE_LOCATION_SEPARATOR.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_line_prefix",
        UNCAUGHT_TRACE_LINE_PREFIX.len(),
    );
    emit_aarch64_call_line_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_unserialize_stack_prefix",
        UNCAUGHT_UNSERIALIZE_STACK_PREFIX.len(),
    );

    abi::emit_load_symbol_to_reg(emitter, "x1", "_unser_trace_owner_ptr", 0);
    abi::emit_load_symbol_to_reg(emitter, "x2", "_unser_trace_owner_len", 0);
    emit_aarch64_dynamic_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_unserialize_owner_suffix",
        UNCAUGHT_UNSERIALIZE_OWNER_SUFFIX.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_unserialize_call_prefix",
        UNCAUGHT_UNSERIALIZE_CALL_PREFIX.len(),
    );
    emit_aarch64_call_line_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_unserialize_call_after_line",
        UNCAUGHT_UNSERIALIZE_CALL_AFTER_LINE.len(),
    );

    abi::emit_load_symbol_to_reg(emitter, "x1", "_unser_trace_input_ptr", 0);
    abi::emit_load_symbol_to_reg(emitter, "x2", "_unser_trace_input_len", 0);
    emitter.instruction("mov x9, #15");                                         // php-src limits the rendered argument preview to 15 bytes
    emitter.instruction("cmp x2, x9");                                          // is the serialized string shorter than the preview limit?
    emitter.instruction("csel x2, x2, x9, lo");                                 // write min(input_len, 15)
    emit_aarch64_dynamic_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_unserialize_call_suffix",
        UNCAUGHT_UNSERIALIZE_CALL_SUFFIX.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_unserialize_thrown_suffix",
        UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX.len(),
    );
    emit_aarch64_call_line_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_newline",
        UNCAUGHT_TRACE_NEWLINE.len(),
    );
}

/// Emits a native-unserialize trace followed by php-src's chained `Next` Throwable.
pub(super) fn emit_uncaught_unserialize_chained_trace_aarch64(emitter: &mut Emitter) {
    emit_uncaught_unserialize_trace_body_aarch64(emitter, "_unser_trace_exception_ptr");
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_next_prefix",
        UNCAUGHT_TRACE_NEXT_PREFIX.len(),
    );
    emit_aarch64_current_exception_header(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_next_stack_prefix",
        UNCAUGHT_TRACE_NEXT_STACK_PREFIX.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_unserialize_thrown_suffix",
        UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX.len(),
    );
    emit_aarch64_current_exception_line_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_newline",
        UNCAUGHT_TRACE_NEWLINE.len(),
    );
    abi::emit_exit(emitter, PHP_FATAL_EXIT_STATUS);
}

/// Writes the active Throwable's class, message, and source location on AArch64.
fn emit_aarch64_current_exception_header(emitter: &mut Emitter) {
    abi::emit_load_symbol_to_reg(emitter, "x20", "_exc_value", 0);
    emitter.instruction("ldr x9, [x20]");                                       // active replacement Throwable class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // select its class-name row
    emitter.instruction("ldp x1, x2, [x10]");                                  // class-name pointer and byte length
    emit_aarch64_dynamic_write(emitter);
    emitter.instruction("ldr x2, [x20, #16]");                                 // replacement message length
    let no_message = "__rt_uncaught_unserialize_next_no_message";
    emitter.instruction(&format!("cbz x2, {no_message}"));                      // PHP omits the colon for an empty message
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_class_separator",
        UNCAUGHT_TRACE_CLASS_SEPARATOR.len(),
    );
    emitter.instruction("ldr x1, [x20, #8]");                                  // replacement message pointer
    emitter.instruction("ldr x2, [x20, #16]");                                 // replacement message byte length
    emit_aarch64_dynamic_write(emitter);
    emitter.label(no_message);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_location_separator",
        UNCAUGHT_TRACE_LOCATION_SEPARATOR.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_line_prefix",
        UNCAUGHT_TRACE_LINE_PREFIX.len(),
    );
    emit_aarch64_current_exception_line_write(emitter);
}

/// Formats and writes the active Throwable's construction line on AArch64.
fn emit_aarch64_current_exception_line_write(emitter: &mut Emitter) {
    abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_value", 0);
    emitter.instruction(&format!(
        "ldr x0, [x9, #{}]",
        THROWABLE_CREATION_LINE_OFFSET
    ));
    abi::emit_call_label(emitter, "__rt_itoa");
    emit_aarch64_dynamic_write(emitter);
}

/// Emits php-src's uncaught native-`__unserialize()` trace on Linux x86_64 and exits.
pub(super) fn emit_uncaught_unserialize_trace_x86_64(emitter: &mut Emitter) {
    emit_uncaught_unserialize_trace_body_x86_64(emitter, "_exc_value");
    abi::emit_exit(emitter, PHP_FATAL_EXIT_STATUS);
}

/// Emits the preserved native-unserialize trace without terminating the process.
fn emit_uncaught_unserialize_trace_body_x86_64(
    emitter: &mut Emitter,
    exception_symbol: &str,
) {
    emit_x86_64_static_write(emitter, "_uncaught_trace_prefix", UNCAUGHT_TRACE_PREFIX.len());

    abi::emit_load_symbol_to_reg(emitter, "r12", exception_symbol, 0);
    emitter.instruction("mov rax, QWORD PTR [r12]");                            // throwable class id
    abi::emit_symbol_address(emitter, "r13", "_class_name_entries");
    emitter.instruction("shl rax, 4");                                          // class-name rows contain two eight-byte words
    emitter.instruction("add r13, rax");                                        // select the throwable class-name row
    emitter.instruction("mov rsi, QWORD PTR [r13]");                            // class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r13 + 8]");                        // class-name byte length
    emit_x86_64_dynamic_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_class_separator",
        UNCAUGHT_TRACE_CLASS_SEPARATOR.len(),
    );

    emitter.instruction("mov rsi, QWORD PTR [r12 + 8]");                        // Throwable::$message pointer
    emitter.instruction("mov rdx, QWORD PTR [r12 + 16]");                       // Throwable::$message byte length
    emit_x86_64_dynamic_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_location_separator",
        UNCAUGHT_TRACE_LOCATION_SEPARATOR.len(),
    );
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_line_prefix",
        UNCAUGHT_TRACE_LINE_PREFIX.len(),
    );
    emit_x86_64_call_line_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_unserialize_stack_prefix",
        UNCAUGHT_UNSERIALIZE_STACK_PREFIX.len(),
    );

    abi::emit_load_symbol_to_reg(emitter, "rsi", "_unser_trace_owner_ptr", 0);
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_unser_trace_owner_len", 0);
    emit_x86_64_dynamic_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_unserialize_owner_suffix",
        UNCAUGHT_UNSERIALIZE_OWNER_SUFFIX.len(),
    );
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_unserialize_call_prefix",
        UNCAUGHT_UNSERIALIZE_CALL_PREFIX.len(),
    );
    emit_x86_64_call_line_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_unserialize_call_after_line",
        UNCAUGHT_UNSERIALIZE_CALL_AFTER_LINE.len(),
    );

    abi::emit_load_symbol_to_reg(emitter, "rsi", "_unser_trace_input_ptr", 0);
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_unser_trace_input_len", 0);
    emitter.instruction("cmp rdx, 15");                                         // is the serialized string shorter than php-src's preview limit?
    emitter.instruction("mov rax, 15");                                         // candidate capped preview length
    emitter.instruction("cmova rdx, rax");                                      // write min(input_len, 15)
    emit_x86_64_dynamic_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_unserialize_call_suffix",
        UNCAUGHT_UNSERIALIZE_CALL_SUFFIX.len(),
    );
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_unserialize_thrown_suffix",
        UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX.len(),
    );
    emit_x86_64_call_line_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_newline",
        UNCAUGHT_TRACE_NEWLINE.len(),
    );
}

/// Emits a native-unserialize trace followed by php-src's chained `Next` Throwable.
pub(super) fn emit_uncaught_unserialize_chained_trace_x86_64(emitter: &mut Emitter) {
    emit_uncaught_unserialize_trace_body_x86_64(emitter, "_unser_trace_exception_ptr");
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_next_prefix",
        UNCAUGHT_TRACE_NEXT_PREFIX.len(),
    );
    emit_x86_64_current_exception_header(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_next_stack_prefix",
        UNCAUGHT_TRACE_NEXT_STACK_PREFIX.len(),
    );
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_unserialize_thrown_suffix",
        UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX.len(),
    );
    emit_x86_64_current_exception_line_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_newline",
        UNCAUGHT_TRACE_NEWLINE.len(),
    );
    abi::emit_exit(emitter, PHP_FATAL_EXIT_STATUS);
}

/// Writes the active Throwable's class, message, and source location on x86_64.
fn emit_x86_64_current_exception_header(emitter: &mut Emitter) {
    abi::emit_load_symbol_to_reg(emitter, "r12", "_exc_value", 0);
    emitter.instruction("mov rax, QWORD PTR [r12]");                            // active replacement Throwable class id
    abi::emit_symbol_address(emitter, "r13", "_class_name_entries");
    emitter.instruction("shl rax, 4");                                         // class-name rows contain two words
    emitter.instruction("add r13, rax");                                       // select its class-name row
    emitter.instruction("mov rsi, QWORD PTR [r13]");                           // class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r13 + 8]");                       // class-name byte length
    emit_x86_64_dynamic_write(emitter);
    emitter.instruction("mov rdx, QWORD PTR [r12 + 16]");                      // replacement message length
    let no_message = "__rt_uncaught_unserialize_next_no_message_x";
    emitter.instruction("test rdx, rdx");
    emitter.instruction(&format!("jz {no_message}"));                           // PHP omits the colon for an empty message
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_class_separator",
        UNCAUGHT_TRACE_CLASS_SEPARATOR.len(),
    );
    emitter.instruction("mov rsi, QWORD PTR [r12 + 8]");                       // replacement message pointer
    emitter.instruction("mov rdx, QWORD PTR [r12 + 16]");                      // replacement message byte length
    emit_x86_64_dynamic_write(emitter);
    emitter.label(no_message);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_location_separator",
        UNCAUGHT_TRACE_LOCATION_SEPARATOR.len(),
    );
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_line_prefix",
        UNCAUGHT_TRACE_LINE_PREFIX.len(),
    );
    emit_x86_64_current_exception_line_write(emitter);
}

/// Formats and writes the active Throwable's construction line on x86_64.
fn emit_x86_64_current_exception_line_write(emitter: &mut Emitter) {
    abi::emit_load_symbol_to_reg(emitter, "r9", "_exc_value", 0);
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r9 + {}]",
        THROWABLE_CREATION_LINE_OFFSET
    ));
    abi::emit_call_label(emitter, "__rt_itoa");
    emitter.instruction("mov rsi, rax");                                       // move the line bytes into write's buffer register
    emit_x86_64_dynamic_write(emitter);
}

/// Emits php-src's main-only DatePeriod iterator-handler trace on AArch64 and exits.
pub(super) fn emit_uncaught_dateperiod_trace_aarch64(emitter: &mut Emitter) {
    emit_aarch64_static_write(emitter, "_uncaught_trace_prefix", UNCAUGHT_TRACE_PREFIX.len());

    abi::emit_load_symbol_to_reg(emitter, "x20", "_exc_value", 0);
    emitter.instruction("ldr x9, [x20]");                                       // throwable class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // select the throwable class-name row
    emitter.instruction("ldp x1, x2, [x10]");                                  // class-name pointer and byte length
    emit_aarch64_dynamic_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_class_separator",
        UNCAUGHT_TRACE_CLASS_SEPARATOR.len(),
    );
    emitter.instruction("ldr x1, [x20, #8]");                                  // Throwable::$message pointer
    emitter.instruction("ldr x2, [x20, #16]");                                 // Throwable::$message byte length
    emit_aarch64_dynamic_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_location_separator",
        UNCAUGHT_TRACE_LOCATION_SEPARATOR.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_line_prefix",
        UNCAUGHT_TRACE_LINE_PREFIX.len(),
    );
    emit_aarch64_dateperiod_line_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_dateperiod_stack_prefix",
        UNCAUGHT_DATEPERIOD_STACK_PREFIX.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_unserialize_thrown_suffix",
        UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX.len(),
    );
    emit_aarch64_dateperiod_line_write(emitter);
    emit_aarch64_static_write(
        emitter,
        "_uncaught_trace_newline",
        UNCAUGHT_TRACE_NEWLINE.len(),
    );
    abi::emit_exit(emitter, PHP_FATAL_EXIT_STATUS);
}

/// Emits php-src's main-only DatePeriod iterator-handler trace on Linux x86_64 and exits.
pub(super) fn emit_uncaught_dateperiod_trace_x86_64(emitter: &mut Emitter) {
    emit_x86_64_static_write(emitter, "_uncaught_trace_prefix", UNCAUGHT_TRACE_PREFIX.len());

    abi::emit_load_symbol_to_reg(emitter, "r12", "_exc_value", 0);
    emitter.instruction("mov rax, QWORD PTR [r12]");                            // throwable class id
    abi::emit_symbol_address(emitter, "r13", "_class_name_entries");
    emitter.instruction("shl rax, 4");                                         // class-name rows contain two eight-byte words
    emitter.instruction("add r13, rax");                                       // select the throwable class-name row
    emitter.instruction("mov rsi, QWORD PTR [r13]");                            // class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r13 + 8]");                        // class-name byte length
    emit_x86_64_dynamic_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_class_separator",
        UNCAUGHT_TRACE_CLASS_SEPARATOR.len(),
    );
    emitter.instruction("mov rsi, QWORD PTR [r12 + 8]");                        // Throwable::$message pointer
    emitter.instruction("mov rdx, QWORD PTR [r12 + 16]");                       // Throwable::$message byte length
    emit_x86_64_dynamic_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_location_separator",
        UNCAUGHT_TRACE_LOCATION_SEPARATOR.len(),
    );
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_line_prefix",
        UNCAUGHT_TRACE_LINE_PREFIX.len(),
    );
    emit_x86_64_dateperiod_line_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_dateperiod_stack_prefix",
        UNCAUGHT_DATEPERIOD_STACK_PREFIX.len(),
    );
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_unserialize_thrown_suffix",
        UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX.len(),
    );
    emit_x86_64_dateperiod_line_write(emitter);
    emit_x86_64_static_write(
        emitter,
        "_uncaught_trace_newline",
        UNCAUGHT_TRACE_NEWLINE.len(),
    );
    abi::emit_exit(emitter, PHP_FATAL_EXIT_STATUS);
}

/// Emits the procedural/time-format ext/date fatal traces on AArch64 and exits.
pub(super) fn emit_uncaught_date_special_trace_aarch64(emitter: &mut Emitter) {
    emit_aarch64_static_write(emitter, "_uncaught_trace_prefix", UNCAUGHT_TRACE_PREFIX.len());
    abi::emit_load_symbol_to_reg(emitter, "x20", "_exc_value", 0);
    emitter.instruction("ldr x9, [x20]");                                       // throwable class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // select the throwable class-name row
    emitter.instruction("ldp x1, x2, [x10]");                                  // class-name pointer and byte length
    emit_aarch64_dynamic_write(emitter);
    emit_aarch64_static_write(emitter, "_uncaught_trace_class_separator", UNCAUGHT_TRACE_CLASS_SEPARATOR.len());
    emitter.instruction("ldr x1, [x20, #8]");                                  // Throwable::$message pointer
    emitter.instruction("ldr x2, [x20, #16]");                                 // Throwable::$message byte length
    emit_aarch64_dynamic_write(emitter);
    emit_aarch64_static_write(emitter, "_uncaught_trace_location_separator", UNCAUGHT_TRACE_LOCATION_SEPARATOR.len());
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(emitter, "_uncaught_trace_line_prefix", UNCAUGHT_TRACE_LINE_PREFIX.len());
    emit_aarch64_symbol_line_write(emitter, "_date_special_trace_line");

    abi::emit_load_symbol_to_reg(emitter, "x9", "_date_special_trace_kind", 0);
    emitter.instruction("cmp x9, #1");                                         // procedural timezone_offset_get trace?
    emitter.instruction("b.ne __rt_uncaught_date_special_format");             // otherwise render the DateTime::format chain
    emit_aarch64_static_write(
        emitter,
        "_uncaught_timezone_offset_stack_prefix",
        UNCAUGHT_TIMEZONE_OFFSET_STACK_PREFIX.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(emitter, "_uncaught_unserialize_call_prefix", UNCAUGHT_UNSERIALIZE_CALL_PREFIX.len());
    emit_aarch64_symbol_line_write(emitter, "_date_special_trace_line");
    emit_aarch64_static_write(
        emitter,
        "_uncaught_timezone_offset_stack_suffix",
        UNCAUGHT_TIMEZONE_OFFSET_STACK_SUFFIX.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(emitter, "_uncaught_unserialize_thrown_suffix", UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX.len());
    emit_aarch64_symbol_line_write(emitter, "_date_special_trace_line");
    emitter.instruction("b __rt_uncaught_date_special_finish");                // share newline and fatal exit

    emitter.label("__rt_uncaught_date_special_format");
    emit_aarch64_static_write(
        emitter,
        "_uncaught_datetime_format_stack_prefix",
        UNCAUGHT_DATETIME_FORMAT_STACK_PREFIX.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(emitter, "_uncaught_unserialize_call_prefix", UNCAUGHT_UNSERIALIZE_CALL_PREFIX.len());
    emit_aarch64_symbol_line_write(emitter, "_date_special_trace_line");
    emit_aarch64_static_write(
        emitter,
        "_uncaught_datetime_format_parent_prefix",
        UNCAUGHT_DATETIME_FORMAT_PARENT_PREFIX.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(emitter, "_uncaught_unserialize_call_prefix", UNCAUGHT_UNSERIALIZE_CALL_PREFIX.len());
    emit_aarch64_symbol_line_write(emitter, "_date_constructor_trace_line");
    emit_aarch64_static_write(
        emitter,
        "_uncaught_datetime_format_stack_suffix",
        UNCAUGHT_DATETIME_FORMAT_STACK_SUFFIX.len(),
    );
    emit_aarch64_program_source_write(emitter);
    emit_aarch64_static_write(emitter, "_uncaught_unserialize_thrown_suffix", UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX.len());
    emit_aarch64_symbol_line_write(emitter, "_date_special_trace_line");

    emitter.label("__rt_uncaught_date_special_finish");
    emit_aarch64_static_write(emitter, "_uncaught_trace_newline", UNCAUGHT_TRACE_NEWLINE.len());
    abi::emit_exit(emitter, PHP_FATAL_EXIT_STATUS);
}

/// Emits the procedural/time-format ext/date fatal traces on Linux x86_64 and exits.
pub(super) fn emit_uncaught_date_special_trace_x86_64(emitter: &mut Emitter) {
    emit_x86_64_static_write(emitter, "_uncaught_trace_prefix", UNCAUGHT_TRACE_PREFIX.len());
    abi::emit_load_symbol_to_reg(emitter, "r12", "_exc_value", 0);
    emitter.instruction("mov rax, QWORD PTR [r12]");                            // throwable class id
    abi::emit_symbol_address(emitter, "r13", "_class_name_entries");
    emitter.instruction("shl rax, 4");                                         // class-name row stride
    emitter.instruction("add r13, rax");                                       // select the throwable class-name row
    emitter.instruction("mov rsi, QWORD PTR [r13]");                            // class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r13 + 8]");                        // class-name byte length
    emit_x86_64_dynamic_write(emitter);
    emit_x86_64_static_write(emitter, "_uncaught_trace_class_separator", UNCAUGHT_TRACE_CLASS_SEPARATOR.len());
    emitter.instruction("mov rsi, QWORD PTR [r12 + 8]");                        // Throwable::$message pointer
    emitter.instruction("mov rdx, QWORD PTR [r12 + 16]");                       // Throwable::$message byte length
    emit_x86_64_dynamic_write(emitter);
    emit_x86_64_static_write(emitter, "_uncaught_trace_location_separator", UNCAUGHT_TRACE_LOCATION_SEPARATOR.len());
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(emitter, "_uncaught_trace_line_prefix", UNCAUGHT_TRACE_LINE_PREFIX.len());
    emit_x86_64_symbol_line_write(emitter, "_date_special_trace_line");

    abi::emit_load_symbol_to_reg(emitter, "r13", "_date_special_trace_kind", 0);
    emitter.instruction("cmp r13, 1");                                         // procedural timezone_offset_get trace?
    emitter.instruction("jne __rt_uncaught_date_special_format_x");            // otherwise render the DateTime::format chain
    emit_x86_64_static_write(emitter, "_uncaught_timezone_offset_stack_prefix", UNCAUGHT_TIMEZONE_OFFSET_STACK_PREFIX.len());
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(emitter, "_uncaught_unserialize_call_prefix", UNCAUGHT_UNSERIALIZE_CALL_PREFIX.len());
    emit_x86_64_symbol_line_write(emitter, "_date_special_trace_line");
    emit_x86_64_static_write(emitter, "_uncaught_timezone_offset_stack_suffix", UNCAUGHT_TIMEZONE_OFFSET_STACK_SUFFIX.len());
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(emitter, "_uncaught_unserialize_thrown_suffix", UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX.len());
    emit_x86_64_symbol_line_write(emitter, "_date_special_trace_line");
    emitter.instruction("jmp __rt_uncaught_date_special_finish_x");             // share newline and fatal exit

    emitter.label("__rt_uncaught_date_special_format_x");
    emit_x86_64_static_write(emitter, "_uncaught_datetime_format_stack_prefix", UNCAUGHT_DATETIME_FORMAT_STACK_PREFIX.len());
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(emitter, "_uncaught_unserialize_call_prefix", UNCAUGHT_UNSERIALIZE_CALL_PREFIX.len());
    emit_x86_64_symbol_line_write(emitter, "_date_special_trace_line");
    emit_x86_64_static_write(emitter, "_uncaught_datetime_format_parent_prefix", UNCAUGHT_DATETIME_FORMAT_PARENT_PREFIX.len());
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(emitter, "_uncaught_unserialize_call_prefix", UNCAUGHT_UNSERIALIZE_CALL_PREFIX.len());
    emit_x86_64_symbol_line_write(emitter, "_date_constructor_trace_line");
    emit_x86_64_static_write(emitter, "_uncaught_datetime_format_stack_suffix", UNCAUGHT_DATETIME_FORMAT_STACK_SUFFIX.len());
    emit_x86_64_program_source_write(emitter);
    emit_x86_64_static_write(emitter, "_uncaught_unserialize_thrown_suffix", UNCAUGHT_UNSERIALIZE_THROWN_SUFFIX.len());
    emit_x86_64_symbol_line_write(emitter, "_date_special_trace_line");

    emitter.label("__rt_uncaught_date_special_finish_x");
    emit_x86_64_static_write(emitter, "_uncaught_trace_newline", UNCAUGHT_TRACE_NEWLINE.len());
    abi::emit_exit(emitter, PHP_FATAL_EXIT_STATUS);
}

/// Writes one fixed runtime-data string to stderr on AArch64.
pub(super) fn emit_aarch64_static_write(emitter: &mut Emitter, label: &str, len: usize) {
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    abi::emit_symbol_address(emitter, "x1", label);
    abi::emit_load_int_immediate(emitter, "x2", len as i64);
    emitter.syscall(4);
}

/// Writes the dynamic `(x1, x2)` byte slice to stderr on AArch64.
pub(super) fn emit_aarch64_dynamic_write(emitter: &mut Emitter) {
    emitter.instruction("mov x0, #2");                                          // fd = stderr while preserving the prepared buffer registers
    emitter.syscall(4);
}

/// Writes the program source path to stderr on AArch64.
pub(super) fn emit_aarch64_program_source_write(emitter: &mut Emitter) {
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    abi::emit_symbol_address(emitter, "x1", "_program_source_file");
    abi::emit_load_symbol_to_reg(emitter, "x2", "_program_source_file_len", 0);
    emitter.syscall(4);
}

/// Formats and writes the active unserialize call-site line on AArch64.
pub(super) fn emit_aarch64_call_line_write(emitter: &mut Emitter) {
    abi::emit_load_symbol_to_reg(emitter, "x0", "_unser_trace_call_line", 0);
    abi::emit_call_label(emitter, "__rt_itoa");
    emit_aarch64_dynamic_write(emitter);
}

/// Formats and writes the active DatePeriod foreach line on AArch64.
pub(super) fn emit_aarch64_dateperiod_line_write(emitter: &mut Emitter) {
    abi::emit_load_symbol_to_reg(emitter, "x0", "_dateperiod_foreach_trace_line", 0);
    abi::emit_call_label(emitter, "__rt_itoa");
    emit_aarch64_dynamic_write(emitter);
}

/// Formats and writes a source line stored in one runtime data symbol on AArch64.
fn emit_aarch64_symbol_line_write(emitter: &mut Emitter, symbol: &str) {
    abi::emit_load_symbol_to_reg(emitter, "x0", symbol, 0);
    abi::emit_call_label(emitter, "__rt_itoa");
    emit_aarch64_dynamic_write(emitter);
}

/// Writes one fixed runtime-data string to stderr on Linux x86_64.
pub(super) fn emit_x86_64_static_write(emitter: &mut Emitter, label: &str, len: usize) {
    abi::emit_symbol_address(emitter, "rsi", label);
    abi::emit_load_int_immediate(emitter, "rdx", len as i64);
    emit_x86_64_dynamic_write(emitter);
}

/// Writes the dynamic `(rsi, rdx)` byte slice to stderr on Linux x86_64.
pub(super) fn emit_x86_64_dynamic_write(emitter: &mut Emitter) {
    emitter.instruction("mov edi, 2");                                          // fd = stderr
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the prepared byte slice
}

/// Writes the program source path to stderr on Linux x86_64.
pub(super) fn emit_x86_64_program_source_write(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, "rsi", "_program_source_file");
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_program_source_file_len", 0);
    emit_x86_64_dynamic_write(emitter);
}

/// Formats and writes the active unserialize call-site line on Linux x86_64.
pub(super) fn emit_x86_64_call_line_write(emitter: &mut Emitter) {
    abi::emit_load_symbol_to_reg(emitter, "rax", "_unser_trace_call_line", 0);
    abi::emit_call_label(emitter, "__rt_itoa");
    emitter.instruction("mov rsi, rax");                                        // move the formatted line pointer into write's buffer register
    emit_x86_64_dynamic_write(emitter);
}

/// Formats and writes the active DatePeriod foreach line on Linux x86_64.
pub(super) fn emit_x86_64_dateperiod_line_write(emitter: &mut Emitter) {
    abi::emit_load_symbol_to_reg(emitter, "rax", "_dateperiod_foreach_trace_line", 0);
    abi::emit_call_label(emitter, "__rt_itoa");
    emitter.instruction("mov rsi, rax");                                        // move the formatted line pointer into write's buffer register
    emit_x86_64_dynamic_write(emitter);
}

/// Formats and writes a source line stored in one runtime data symbol on Linux x86_64.
fn emit_x86_64_symbol_line_write(emitter: &mut Emitter, symbol: &str) {
    abi::emit_load_symbol_to_reg(emitter, "rax", symbol, 0);
    abi::emit_call_label(emitter, "__rt_itoa");
    emitter.instruction("mov rsi, rax");                                        // move the formatted line pointer into write's buffer register
    emit_x86_64_dynamic_write(emitter);
}
