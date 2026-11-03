//! Purpose:
//! Emits the `__rt_trim_warning` diagnostic builder and owns the exact
//! `php_charmask` invalid-range message text shared with the fixed data section.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::trim_mask::emit_trim_mask`.
//! - `crate::codegen_support::runtime::data::fixed` for the message literals.
//!
//! Key details:
//! - The builder appends `Warning: <name>(): <message>\n` through the shared
//!   diagnostic fragment channel and dispatches it once.
//! - The caller passes the PHP-visible function name, so trim/ltrim/rtrim/chop
//!   and the mb_* fast paths keep their own warning prefixes.
//! - The invalid-range location suffix remains the caller's diagnostic-site
//!   responsibility; the message itself is complete like the other runtime
//!   warnings.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// The `Warning: ` prefix shared by every trim invalid-range diagnostic.
pub(super) const PREFIX_SYMBOL: &str = "_trim_warning_prefix";
pub(super) const PREFIX_LEN: usize = 9;
/// The `(): ` separator between the function name and the message.
pub(super) const CALL_SYMBOL: &str = "_trim_warning_call";
pub(super) const CALL_LEN: usize = 4;
/// The trailing newline dispatched as the final warning fragment.
pub(super) const NEWLINE_SYMBOL: &str = "_trim_warning_nl";
pub(super) const NEWLINE_LEN: usize = 1;

pub(super) const RANGE_LEFT_SYMBOL: &str = "_trim_range_left";
pub(super) const RANGE_LEFT_LEN: usize =
    "Invalid '..'-range, no character to the left of '..'".len();
pub(super) const RANGE_RIGHT_SYMBOL: &str = "_trim_range_right";
pub(super) const RANGE_RIGHT_LEN: usize =
    "Invalid '..'-range, no character to the right of '..'".len();
pub(super) const RANGE_ORDER_SYMBOL: &str = "_trim_range_order";
pub(super) const RANGE_ORDER_LEN: usize =
    "Invalid '..'-range, '..'-range needs to be incrementing".len();
pub(super) const RANGE_GENERIC_SYMBOL: &str = "_trim_range_generic";
pub(super) const RANGE_GENERIC_LEN: usize = "Invalid '..'-range".len();

/// The complete set of immutable strings the trim warning path references.
pub(crate) const STATIC_STRINGS: &[(&str, &str)] = &[
    (PREFIX_SYMBOL, "Warning: "),
    (CALL_SYMBOL, "(): "),
    (NEWLINE_SYMBOL, "\n"),
    (
        RANGE_LEFT_SYMBOL,
        "Invalid '..'-range, no character to the left of '..'",
    ),
    (
        RANGE_RIGHT_SYMBOL,
        "Invalid '..'-range, no character to the right of '..'",
    ),
    (
        RANGE_ORDER_SYMBOL,
        "Invalid '..'-range, '..'-range needs to be incrementing",
    ),
    (RANGE_GENERIC_SYMBOL, "Invalid '..'-range"),
];

/// Emits the target-specific `__rt_trim_warning` fragment builder.
pub(super) fn emit_trim_warning_builder(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits the AArch64 builder.
///
/// ABI: x0=name_ptr, x1=name_len, x2=message_ptr, x3=message_len.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trim_warning ---");
    emitter.label_global("__rt_trim_warning");
    emitter.instruction("stp x29, x30, [sp, #-48]!");                           // preserve linkage and reserve the four message words
    emitter.instruction("mov x29, sp");                                         // establish a stable fragment-builder frame
    emitter.instruction("str x0, [sp, #16]");                                   // save the PHP-visible function name pointer
    emitter.instruction("str x1, [sp, #24]");                                   // save the PHP-visible function name length
    emitter.instruction("str x2, [sp, #32]");                                   // save the invalid-range message pointer
    emitter.instruction("str x3, [sp, #40]");                                   // save the invalid-range message length
    abi::emit_symbol_address(emitter, "x1", PREFIX_SYMBOL);
    emitter.instruction(&format!("mov x2, #{PREFIX_LEN}"));                     // pass the Warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the function name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the function name length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "x1", CALL_SYMBOL);
    emitter.instruction(&format!("mov x2, #{CALL_LEN}"));                       // pass the function-call separator length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("ldr x1, [sp, #32]");                                   // pass the invalid-range message pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // pass the invalid-range message length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "x1", NEWLINE_SYMBOL);
    emitter.instruction(&format!("mov x2, #{NEWLINE_LEN}"));                    // pass the warning newline length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // dispatch the completed warning
    emitter.instruction("ldp x29, x30, [sp], #48");                             // restore linkage and release the builder frame
    emitter.instruction("ret");                                                 // return to the mask parser
}

/// Emits the x86_64 builder.
///
/// ABI: rdi=name_ptr, rsi=name_len, rdx=message_ptr, rcx=message_len.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trim_warning ---");
    emitter.label_global("__rt_trim_warning");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable fragment-builder frame
    emitter.instruction("sub rsp, 32");                                         // reserve the four message words while keeping calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the PHP-visible function name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the PHP-visible function name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the invalid-range message pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the invalid-range message length
    abi::emit_symbol_address(emitter, "rdi", PREFIX_SYMBOL);
    emitter.instruction(&format!("mov esi, {PREFIX_LEN}"));                     // pass the Warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the function name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the function name length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "rdi", CALL_SYMBOL);
    emitter.instruction(&format!("mov esi, {CALL_LEN}"));                       // pass the function-call separator length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the invalid-range message pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the invalid-range message length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, "rdi", NEWLINE_SYMBOL);
    emitter.instruction(&format!("mov esi, {NEWLINE_LEN}"));                    // pass the warning newline length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // dispatch the completed warning
    emitter.instruction("add rsp, 32");                                         // release the fragment-builder frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the mask parser
}
