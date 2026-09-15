//! Purpose:
//! Emits `__rt_throw_named_parameter_overwrite`, `__rt_throw_unknown_named_parameter` and
//! `__rt_throw_positional_after_named`, PHP's three catchable named-argument binding `Error`s.
//!
//! Called from:
//! - `crate::codegen::runtime_callable_invoker`, when a descriptor invoker binds an associative
//!   argument container onto a physical signature.
//! - `crate::codegen::lower_inst::hashes`, through `Op::ThrowNamedParameterOverwrite`, when the
//!   descriptor argument-unpacking walk rejects a name the container already carries.
//!
//! Key details:
//! - Two of the three parameter names are only known at run time: a key the caller supplied, or a
//!   declared name the alias check matched. Those messages are therefore composed around the
//!   name, exactly the way `__rt_throw_object_not_array` composes a class name. The ordering
//!   refusal has no run-time part, so its Throwable points straight at the constant bytes and
//!   skips both the concatenation and the persist.
//! - `__rt_concat` reads its LEFT operand from the string-result pair and its RIGHT one from a
//!   different pair per target, so both are spelled out below rather than assumed.
//! - The name pair is stashed in the frame across the concatenations: `x9`-`x15` and the x86_64
//!   caller-saved registers do not survive a call.
//! - Neither helper returns. `__rt_throw_current` unwinds to the nearest handler, or reports
//!   the uncaught Throwable and exits like PHP when there is none. That is also why clobbering
//!   callee-saved registers here would be harmless, and why it is still avoided.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::{
    NAMED_PARAMETER_OVERWRITE_PREFIX, NAMED_PARAMETER_OVERWRITE_SUFFIX,
    POSITIONAL_AFTER_NAMED_MSG, UNKNOWN_NAMED_PARAMETER_PREFIX,
};
use crate::codegen_support::sentinels::{
    emit_throwable_creation_line_unknown, x86_64_heap_kind_word,
};

/// How one named-argument `Error` builds its message.
enum NamedParameterMessage {
    /// `<prefix>$<name>` plus an optional trailing clause, composed around the run-time name.
    AroundName {
        /// Data symbol holding the text before the name, `$` sigil included.
        prefix_symbol: &'static str,
        /// Byte length of `prefix_symbol`.
        prefix_len: usize,
        /// Optional data symbol and byte length for the text after the name.
        suffix: Option<(&'static str, usize)>,
    },
    /// A message with no run-time part, taken verbatim from a data symbol.
    Fixed {
        /// Data symbol holding the whole message.
        symbol: &'static str,
        /// Byte length of `symbol`.
        len: usize,
    },
}

/// Emits `__rt_throw_named_parameter_overwrite`.
///
/// Input: the parameter name as `x0`/`x1` (pointer, byte length) on ARM64 and `rdi`/`rsi` on
/// x86_64. Raises `Named parameter $<name> overwrites previous argument`. Never returns.
pub fn emit_throw_named_parameter_overwrite(emitter: &mut Emitter) {
    emit_named_parameter_error(
        emitter,
        "__rt_throw_named_parameter_overwrite",
        "named_parameter_overwrite",
        NamedParameterMessage::AroundName {
            prefix_symbol: "_named_parameter_overwrite_prefix",
            prefix_len: NAMED_PARAMETER_OVERWRITE_PREFIX.len(),
            suffix: Some((
                "_named_parameter_overwrite_suffix",
                NAMED_PARAMETER_OVERWRITE_SUFFIX.len(),
            )),
        },
    );
}

/// Emits `__rt_throw_unknown_named_parameter`.
///
/// Input: the parameter name as `x0`/`x1` (pointer, byte length) on ARM64 and `rdi`/`rsi` on
/// x86_64. Raises `Unknown named parameter $<name>`. Never returns.
pub fn emit_throw_unknown_named_parameter(emitter: &mut Emitter) {
    emit_named_parameter_error(
        emitter,
        "__rt_throw_unknown_named_parameter",
        "unknown_named_parameter",
        NamedParameterMessage::AroundName {
            prefix_symbol: "_unknown_named_parameter_prefix",
            prefix_len: UNKNOWN_NAMED_PARAMETER_PREFIX.len(),
            suffix: None,
        },
    );
}

/// Emits `__rt_throw_positional_after_named`.
///
/// Takes no input. Raises `Cannot use positional argument after named argument`, PHP's refusal
/// of a positional entry that follows a name in the same argument container. Never returns.
pub fn emit_throw_positional_after_named(emitter: &mut Emitter) {
    emit_named_parameter_error(
        emitter,
        "__rt_throw_positional_after_named",
        "positional_after_named",
        NamedParameterMessage::Fixed {
            symbol: "_positional_after_named_msg",
            len: POSITIONAL_AFTER_NAMED_MSG.len(),
        },
    );
}

/// Emits one named-argument binding `Error` thrower.
fn emit_named_parameter_error(
    emitter: &mut Emitter,
    label: &str,
    comment: &str,
    message: NamedParameterMessage,
) {
    if emitter.target.arch == Arch::X86_64 {
        emit_named_parameter_error_x86_64(emitter, label, comment, message);
        return;
    }
    emit_named_parameter_error_aarch64(emitter, label, comment, message);
}

/// Emits one named-parameter `Error` thrower for ARM64.
fn emit_named_parameter_error_aarch64(
    emitter: &mut Emitter,
    label: &str,
    comment: &str,
    message: NamedParameterMessage,
) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: throw {comment} Error ---"));
    emitter.label_global(label);

    // Stack (64 bytes): [sp, #0] the message pair, [sp, #16] the borrowed parameter-name pair.
    emitter.instruction("sub sp, sp, #64");                                     // reserve message state and frame linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable Error-construction frame

    match message {
        NamedParameterMessage::AroundName { prefix_symbol, prefix_len, suffix } => {
            emitter.instruction("str x0, [sp, #16]");                           // stash the parameter-name pointer across the concatenations
            emitter.instruction("str x1, [sp, #24]");                           // stash its byte length too
            abi::emit_symbol_address(emitter, "x1", prefix_symbol);             // concat left operand pointer
            emitter.instruction(&format!("mov x2, #{prefix_len}"));             // concat left operand length
            emitter.instruction("ldr x3, [sp, #16]");                           // right operand: the parameter name
            emitter.instruction("ldr x4, [sp, #24]");                           // and its byte length
            emitter.instruction("bl __rt_concat");                              // build the message up to the parameter name
            if let Some((suffix_symbol, suffix_len)) = suffix {
                abi::emit_symbol_address(emitter, "x3", suffix_symbol);         // right operand pointer
                emitter.instruction(&format!("mov x4, #{suffix_len}"));         // right operand length
                emitter.instruction("bl __rt_concat");                          // append the trailing clause
            }
            emitter.instruction("bl __rt_str_persist");                         // give the Error stable message ownership
        }
        NamedParameterMessage::Fixed { symbol, len } => {
            // A constant lives for the whole process, so the Throwable can borrow those bytes
            // directly: no concatenation to run and no persisted copy to own.
            abi::emit_symbol_address(emitter, "x1", symbol);                    // the whole message pointer
            emitter.instruction(&format!("mov x2, #{len}"));                    // the whole message byte length
        }
    }
    emitter.instruction("stp x1, x2, [sp]");                                    // preserve the message pair across the allocation

    emitter.instruction("mov x0, #56");                                         // canonical Throwable payload size
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the Error object payload
    emitter.instruction("mov x9, #6");                                          // heap kind 6 identifies a throwable object
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation as a runtime object
    emitter.instruction("bl __rt_object_handle_acquire");                       // bind the Error to its PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "x9", "_spl_error_class_id", 0);
    emitter.instruction("str x9, [x0]");                                        // stamp the per-program Error class id
    emitter.instruction("ldp x10, x11, [sp]");                                  // recover the persisted message pair
    emitter.instruction("str x10, [x0, #8]");                                   // message pointer
    emitter.instruction("str x11, [x0, #16]");                                  // message byte length
    // __rt_heap_alloc recycles blocks without zeroing, so every remaining slot is written here.
    emitter.instruction("str xzr, [x0, #24]");                                  // code = 0
    emit_throwable_creation_line_unknown(emitter, "x0");
    emitter.instruction("str xzr, [x0, #40]");                                  // previous = null
    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);              // publish the Throwable for the unwinder
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the local frame
    emitter.instruction("b __rt_throw_current");                                // unwind, or report it uncaught and exit like PHP
}

/// Emits one named-parameter `Error` thrower for x86_64.
fn emit_named_parameter_error_x86_64(
    emitter: &mut Emitter,
    label: &str,
    comment: &str,
    message: NamedParameterMessage,
) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: throw {comment} Error ---"));
    emitter.label_global(label);

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an Error-construction frame
    emitter.instruction("sub rsp, 32");                                         // reserve the message and name pairs, keeping rsp aligned

    match message {
        NamedParameterMessage::AroundName { prefix_symbol, prefix_len, suffix } => {
            emitter.instruction("mov QWORD PTR [rbp - 24], rdi");               // stash the parameter-name pointer across the concatenations
            emitter.instruction("mov QWORD PTR [rbp - 32], rsi");               // stash its byte length too
            emitter.instruction(&format!("lea rax, [rip + {prefix_symbol}]"));  // concat left operand pointer
            emitter.instruction(&format!("mov rdx, {prefix_len}"));             // concat left operand length
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // right operand: the parameter name
            emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");               // and its byte length
            abi::emit_call_label(emitter, "__rt_concat");                       // build the message up to the parameter name
            if let Some((suffix_symbol, suffix_len)) = suffix {
                emitter.instruction(&format!("lea rdi, [rip + {suffix_symbol}]")); // right operand pointer
                emitter.instruction(&format!("mov rsi, {suffix_len}"));         // right operand length
                abi::emit_call_label(emitter, "__rt_concat");                   // append the trailing clause
            }
            abi::emit_call_label(emitter, "__rt_str_persist");                  // give the Error stable message ownership
        }
        NamedParameterMessage::Fixed { symbol, len } => {
            // A constant lives for the whole process, so the Throwable can borrow those bytes
            // directly: no concatenation to run and no persisted copy to own.
            emitter.instruction(&format!("lea rax, [rip + {symbol}]"));         // the whole message pointer
            emitter.instruction(&format!("mov rdx, {len}"));                    // the whole message byte length
        }
    }
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the message pointer across the allocation
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the message byte length

    emitter.instruction("mov rax, 56");                                         // canonical Throwable payload size
    abi::emit_call_label(emitter, "__rt_heap_alloc");                           // allocate the Error object payload (rax = payload)
    emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(6))); // magic + kind 6 identifies a throwable object
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the uniform heap header
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");                // bind the Error to its PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_error_class_id", 0);
    emitter.instruction("mov QWORD PTR [rax], r10");                            // stamp the per-program Error class id
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // recover the message pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // recover the message byte length
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // message pointer
    emitter.instruction("mov QWORD PTR [rax + 16], r11");                       // message byte length
    // __rt_heap_alloc recycles blocks without zeroing, so every remaining slot is written here.
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // code = 0
    emit_throwable_creation_line_unknown(emitter, "rax");
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous = null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // publish the Throwable for the unwinder
    emitter.instruction("mov rsp, rbp");                                        // release the local frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_throw_current");                              // unwind, or report it uncaught and exit like PHP
}
