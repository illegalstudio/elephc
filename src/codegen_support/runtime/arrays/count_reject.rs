//! Purpose:
//! Emits the two helpers `count()` needs to raise PHP's `TypeError` instead of answering `0`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - The `count()` lowering, ahead of `__rt_mixed_count`.
//!
//! Key details:
//! - `count()` on anything but an array or a `Countable` is a `TypeError` in PHP, not a quiet
//!   zero. Answering `0` made `count($x) === 0` read a non-empty string, a null and an integer
//!   as empty collections — a wrong answer with nothing to notice it by.
//! - PHP names the type with the VALUE's own spelling, so a boolean reports `true` or `false`
//!   rather than `bool`. All eight names were read off `php -n` 8.5.6 rather than guessed.
//! - The rejection is a separate probe rather than a new result from `__rt_mixed_count`, because
//!   that helper tail-calls the SPL counters: anything it returned beside the count would be
//!   whatever the tail call happened to leave behind.
//! - A non-`Countable` OBJECT is deliberately still counted as `0` here. PHP names the class in
//!   that message, which needs a class-name lookup and a composed string; the scalar and null
//!   arms are the ones a program reaches.

use crate::codegen_support::runtime::data::COUNT_TYPE_ERROR_MESSAGES;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen::sentinels::emit_branch_if_null_container;

/// Symbol holding the message for reject index `i`.
fn message_symbol(index: usize) -> String {
    format!("_count_type_err_{index}")
}

/// Emits `__rt_count_reject_index(mixed)`.
///
/// Answers the index of the `TypeError` message PHP would raise, or `-1` when the value really
/// is countable and `__rt_mixed_count` should run. Indices follow [`COUNT_TYPE_ERROR_MESSAGES`].
pub fn emit_count_reject_index(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: count_reject_index ---");
    emitter.label_global("__rt_count_reject_index");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x0, __rt_cri_null");                       // a missing value reads as null
            emitter.instruction("ldr x9, [x0]");                                // the boxed tag
            emitter.instruction("cmp x9, #4");                                  // indexed array
            emitter.instruction("b.eq __rt_cri_container");
            emitter.instruction("cmp x9, #5");                                  // associative array
            emitter.instruction("b.eq __rt_cri_container");
            emitter.instruction("cmp x9, #6");                                  // object: left to the existing Countable dispatch
            emitter.instruction("b.eq __rt_cri_ok");
            emitter.instruction("cmp x9, #0");
            emitter.instruction("b.eq __rt_cri_int");
            emitter.instruction("cmp x9, #1");
            emitter.instruction("b.eq __rt_cri_string");
            emitter.instruction("cmp x9, #2");
            emitter.instruction("b.eq __rt_cri_float");
            emitter.instruction("cmp x9, #3");
            emitter.instruction("b.eq __rt_cri_bool");
            emitter.instruction("cmp x9, #9");
            emitter.instruction("b.eq __rt_cri_resource");
            emitter.instruction("b __rt_cri_null");                             // tag 8 and anything unknown
            emitter.label("__rt_cri_container");
            emitter.instruction("ldr x9, [x0, #8]");                            // the payload the box carries
            emit_branch_if_null_container(emitter, "x9", "x10", "__rt_cri_null");
            emitter.instruction("b __rt_cri_ok");
            emitter.label("__rt_cri_bool");
            emitter.instruction("ldr x9, [x0, #8]");                            // php spells the VALUE, not the type
            emitter.instruction("cbz x9, __rt_cri_false");
            emitter.instruction("mov x0, #3");                                  // "true"
            emitter.instruction("ret");
            emitter.label("__rt_cri_false");
            emitter.instruction("mov x0, #4");                                  // "false"
            emitter.instruction("ret");
            emitter.label("__rt_cri_int");
            emitter.instruction("mov x0, #0");
            emitter.instruction("ret");
            emitter.label("__rt_cri_string");
            emitter.instruction("mov x0, #1");
            emitter.instruction("ret");
            emitter.label("__rt_cri_float");
            emitter.instruction("mov x0, #2");
            emitter.instruction("ret");
            emitter.label("__rt_cri_null");
            emitter.instruction("mov x0, #5");
            emitter.instruction("ret");
            emitter.label("__rt_cri_resource");
            emitter.instruction("mov x0, #6");
            emitter.instruction("ret");
            emitter.label("__rt_cri_ok");
            emitter.instruction("mov x0, #-1");                                 // countable: let __rt_mixed_count answer
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_cri_null_x86");                        // a missing value reads as null
            emitter.instruction("mov r10, QWORD PTR [rax]");                    // the boxed tag
            emitter.instruction("cmp r10, 4");                                  // indexed array
            emitter.instruction("je __rt_cri_container_x86");
            emitter.instruction("cmp r10, 5");                                  // associative array
            emitter.instruction("je __rt_cri_container_x86");
            emitter.instruction("cmp r10, 6");                                  // object: left to the existing Countable dispatch
            emitter.instruction("je __rt_cri_ok_x86");
            emitter.instruction("cmp r10, 0");
            emitter.instruction("je __rt_cri_int_x86");
            emitter.instruction("cmp r10, 1");
            emitter.instruction("je __rt_cri_string_x86");
            emitter.instruction("cmp r10, 2");
            emitter.instruction("je __rt_cri_float_x86");
            emitter.instruction("cmp r10, 3");
            emitter.instruction("je __rt_cri_bool_x86");
            emitter.instruction("cmp r10, 9");
            emitter.instruction("je __rt_cri_resource_x86");
            emitter.instruction("jmp __rt_cri_null_x86");                       // tag 8 and anything unknown
            emitter.label("__rt_cri_container_x86");
            emitter.instruction("mov r10, QWORD PTR [rax + 8]");                // the payload the box carries
            emit_branch_if_null_container(emitter, "r10", "r11", "__rt_cri_null_x86");
            emitter.instruction("jmp __rt_cri_ok_x86");
            emitter.label("__rt_cri_bool_x86");
            emitter.instruction("mov r10, QWORD PTR [rax + 8]");                // php spells the VALUE, not the type
            emitter.instruction("test r10, r10");
            emitter.instruction("jz __rt_cri_false_x86");
            emitter.instruction("mov rax, 3");                                  // "true"
            emitter.instruction("ret");
            emitter.label("__rt_cri_false_x86");
            emitter.instruction("mov rax, 4");                                  // "false"
            emitter.instruction("ret");
            emitter.label("__rt_cri_int_x86");
            emitter.instruction("xor eax, eax");
            emitter.instruction("ret");
            emitter.label("__rt_cri_string_x86");
            emitter.instruction("mov rax, 1");
            emitter.instruction("ret");
            emitter.label("__rt_cri_float_x86");
            emitter.instruction("mov rax, 2");
            emitter.instruction("ret");
            emitter.label("__rt_cri_null_x86");
            emitter.instruction("mov rax, 5");
            emitter.instruction("ret");
            emitter.label("__rt_cri_resource_x86");
            emitter.instruction("mov rax, 6");
            emitter.instruction("ret");
            emitter.label("__rt_cri_ok_x86");
            emitter.instruction("mov rax, -1");                                 // countable: let __rt_mixed_count answer
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_count_type_message(index)`, answering the message in the string-result registers.
///
/// AArch64 answers `x1`/`x2`; x86_64 answers `rax`/`rdx` — the pair the dynamic throwable path
/// expects, so the caller hands it straight to `emit_type_error_from_string_result`.
pub fn emit_count_type_message(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: count_type_message ---");
    emitter.label_global("__rt_count_type_message");
    for (index, message) in COUNT_TYPE_ERROR_MESSAGES.iter().enumerate() {
        let symbol = message_symbol(index);
        let next = format!("__rt_ctm_{index}_no");
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp x0, #{index}"));
                emitter.instruction(&format!("b.ne {next}"));
                abi::emit_symbol_address(emitter, "x1", &symbol);
                emitter.instruction(&format!("mov x2, #{}", message.len()));
                emitter.instruction("ret");
                emitter.label(&next);
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp rax, {index}"));
                emitter.instruction(&format!("jne {next}_x86"));
                abi::emit_symbol_address(emitter, "rax", &symbol);
                emitter.instruction(&format!("mov rdx, {}", message.len()));
                emitter.instruction("ret");
                emitter.label(&format!("{next}_x86"));
            }
        }
    }
    // Unreachable for any index the probe answers; report the null wording rather than a
    // dangling pointer if a future arm forgets its message.
    let fallback = message_symbol(5);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", &fallback);
            emitter.instruction(&format!("mov x2, #{}", COUNT_TYPE_ERROR_MESSAGES[5].len()));
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rax", &fallback);
            emitter.instruction(&format!("mov rdx, {}", COUNT_TYPE_ERROR_MESSAGES[5].len()));
            emitter.instruction("ret");
        }
    }
}

/// The label/message pairs the fixed-string table has to publish for the helper above.
pub(crate) fn count_type_error_symbols() -> Vec<(String, &'static str)> {
    COUNT_TYPE_ERROR_MESSAGES
        .iter()
        .enumerate()
        .map(|(index, message)| (message_symbol(index), *message))
        .collect()
}
