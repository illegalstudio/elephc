//! Purpose:
//! Emits program-owned startup INI data and initialization for the shared mbstring engine.
//!
//! Called from:
//! - User assembly finalization and the CLI/web request entry before mbstring reset.
//!
//! Key details:
//! - PCRE2 registration is emitted only for programs that match or validate MIME expressions.
//! - Validated startup installation remains idempotent across requests.
//! - Program values never enter cached runtime objects or enable ordinary eval regex capability.
//! - Startup diagnostics cannot execute PHP; library callers receive status after bridge cleanup.

use super::{abi, data_section::{DataSection, DataWord}, emit::Emitter, platform::Arch};
use crate::ir::Module;
use elephc_builtin_contract::mbstring_abi::{ARG_STRING, MbArgV1, MbResultV1};
use elephc_builtin_contract::mbstring_abi::ini::{MbIniHostV1, MbMimeRegexV1};

#[cfg(test)]
mod tests;

/// Emits immutable descriptors plus one native startup entry only for a configured mbstring program.
pub(super) fn emit(module: &Module, emitter: &mut Emitter, data: &mut DataSection) {
    let Some(arguments) = &module.mbstring_startup else { return; };
    assert_eq!(std::mem::size_of::<MbArgV1>(), 32);
    assert_eq!(std::mem::size_of::<MbResultV1>(), 48);
    assert_eq!(std::mem::size_of::<MbMimeRegexV1>(), 40);
    assert_eq!(std::mem::size_of::<MbIniHostV1>(), 24);
    let mut words = Vec::new();
    for value in arguments {
        let (label, length) = data.add_string(value);
        words.extend([DataWord::U64(ARG_STRING), DataWord::U64(0),
            DataWord::Symbol(label), DataWord::U64(length as u64)]);
    }
    let arguments_label = data.add_words(words);
    let count = data.add_words(vec![DataWord::U64(arguments.len() as u64)]);
    let provider = module.required_runtime_features.mbstring_mime.then(|| {
        let mut provider = vec![DataWord::U64(1 | (40 << 32))];
        for symbol in ["elephc_pcre2_v1_mime_compile", "elephc_pcre2_v1_mime_match",
            "elephc_pcre2_v1_mime_free", "elephc_pcre2_v1_error_message"] {
            provider.push(DataWord::Symbol(emitter.target.extern_symbol(symbol)));
        }
        data.add_words(provider)
    });
    let host = data.add_words(vec![DataWord::U64(1 | (24 << 32)), DataWord::U64(0),
        DataWord::Symbol("__rt_mbstring_startup_diagnostic".into())]);
    let arm = emitter.target.arch == Arch::AArch64;
    emit_process_entry(emitter);
    emitter.label_global("__rt_mbstring_startup_status");
    if arm {
        emitter.instruction("sub sp, sp, #80");                                 // reserve a bridge result, status, and native linkage
        emitter.instruction("stp x29, x30, [sp, #64]");                         // preserve request-entry linkage across Rust calls
    } else {
        emitter.instruction("push rbp");                                        // align native calls while preserving request linkage
        emitter.instruction("mov rbp, rsp");                                    // establish a stable startup adapter frame
        emitter.instruction("sub rsp, 64");                                     // reserve the owned result and preserved status
    }
    if let Some(provider) = provider {
        abi::emit_symbol_address(emitter, if arm { "x0" } else { "rdi" }, &provider);
        emitter.bl_c("elephc_mbstring_mime_provider_v1");
        if arm {
            emitter.instruction("cbnz w0, __rt_mbstring_startup_status_done");  // reject an invalid provider before requesting any bridge result
        } else {
            emitter.instruction("test eax, eax");                               // inspect provider validation before startup state changes
            emitter.instruction("jnz __rt_mbstring_startup_status_done");       // preserve fatal integration failure without an uninitialized release
        }
    }
    abi::emit_symbol_address(emitter, if arm { "x0" } else { "rdi" }, &arguments_label);
    abi::emit_load_symbol_to_reg(emitter, if arm { "x1" } else { "rsi" }, &count, 0);
    abi::emit_symbol_address(emitter, if arm { "x2" } else { "rdx" }, &host);
    emitter.instruction(if arm { "mov x3, sp" } else { "mov rcx, rsp" });       // provide empty bridge result storage for this initialization
    emitter.bl_c("elephc_mbstring_configure_v1");
    emitter.instruction(if arm { "str w0, [sp, #48]" } else { "mov DWORD PTR [rsp + 48], eax" }); // preserve initialization status while reclaiming result buffers
    emitter.instruction(if arm { "mov x0, sp" } else { "mov rdi, rsp" });       // return all result storage to its owning Rust allocator
    emitter.bl_c("elephc_mbstring_release_v1");
    emitter.instruction(if arm { "ldr w0, [sp, #48]" } else { "mov eax, DWORD PTR [rsp + 48]" }); // recover status after releasing every published bridge buffer
    emitter.label("__rt_mbstring_startup_status_done");
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #64]");                         // restore host linkage after provider validation or result cleanup
        emitter.instruction("add sp, sp, #80");                                 // retire stack-owned initialization metadata
    } else {
        emitter.instruction("leave");                                           // release result storage without changing the returned status
    }
    emitter.instruction("ret");                                                 // return success or failure without exiting a library host
    diagnostic(emitter, data);
}

/// Applies executable startup failure policy while libraries use the non-exiting status entry.
fn emit_process_entry(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_startup");
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_call_label(emitter, "__rt_mbstring_startup_status");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("cbz w0, __rt_mbstring_startup_ready");             // continue only after configuration has been installed
        emitter.instruction("mov x0, #1");                                      // report failed executable startup as process failure
    } else {
        emitter.instruction("test eax, eax");                                   // inspect the completed initializer status
        emitter.instruction("jz __rt_mbstring_startup_ready");                  // return to the request entry after successful installation
        emitter.instruction("mov edi, 1");                                      // report failed executable startup as process failure
    }
    emitter.bl_c("exit");
    emitter.label("__rt_mbstring_startup_ready");
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

/// Writes complete startup warnings without allowing PHP callbacks during process configuration.
fn diagnostic(emitter: &mut Emitter, data: &mut DataSection) {
    let arm = emitter.target.arch == Arch::AArch64;
    let (warning, _) = data.add_string(b"Warning: ");
    let (deprecated, _) = data.add_string(b"Deprecated: ");
    let (newline, _) = data.add_string(b"\n");
    emitter.label_global("__rt_mbstring_startup_diagnostic");
    if arm {
        emitter.instruction("stp x29, x30, [sp, #-32]!");                       // preserve C linkage and reserve the original message
        emitter.instruction("stp x2, x3, [sp, #16]");                           // retain binary message bytes and length across prefix output
        emitter.instruction("cmp w1, #8192");                                   // select the PHP diagnostic prefix from the supplied level
        emitter.instruction("b.eq __rt_mbstring_startup_deprecated");           // route deprecations through their distinct prefix
    } else {
        emitter.instruction("push rbp");                                        // preserve the C frame and align output calls
        emitter.instruction("mov rbp, rsp");                                    // establish a stable startup diagnostic frame
        emitter.instruction("sub rsp, 16");                                     // reserve the borrowed message descriptor
        emitter.instruction("mov QWORD PTR [rsp], rdx");                        // retain binary message bytes across prefix output
        emitter.instruction("mov QWORD PTR [rsp + 8], rcx");                    // preserve exact length including any embedded NUL
        emitter.instruction("cmp esi, 8192");                                   // distinguish startup warnings from deprecations
        emitter.instruction("je __rt_mbstring_startup_deprecated");             // select the deprecation prefix before writing any bytes
    }
    abi::emit_symbol_address(emitter, if arm { "x1" } else { "rdi" }, &warning);
    emitter.instruction(if arm { "mov x2, #9" } else { "mov esi, 9" });         // provide the complete warning prefix length
    emitter.instruction(if arm { "b __rt_mbstring_startup_prefix" } else { "jmp __rt_mbstring_startup_prefix" }); // share output and message cleanup across levels
    emitter.label("__rt_mbstring_startup_deprecated");
    abi::emit_symbol_address(emitter, if arm { "x1" } else { "rdi" }, &deprecated);
    emitter.instruction(if arm { "mov x2, #12" } else { "mov esi, 12" });       // provide the complete deprecation prefix length
    emitter.label("__rt_mbstring_startup_prefix");
    abi::emit_call_label(emitter, "__rt_diag_warning");
    if arm {
        emitter.instruction("ldp x1, x2, [sp, #16]");                           // restore the borrowed complete message descriptor
    } else {
        emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // restore the binary message pointer after prefix output
        emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                    // restore the original byte count without C-string truncation
    }
    abi::emit_call_label(emitter, "__rt_diag_warning");
    abi::emit_symbol_address(emitter, if arm { "x1" } else { "rdi" }, &newline);
    emitter.instruction(if arm { "mov x2, #1" } else { "mov esi, 1" });         // terminate the startup diagnostic with exactly one newline
    abi::emit_call_label(emitter, "__rt_diag_warning");
    if arm {
        emitter.instruction("mov x0, #0");                                      // report completed delivery without a PHP throwable
        emitter.instruction("ldp x29, x30, [sp], #32");                         // retire borrowed message metadata and restore C linkage
    } else {
        emitter.instruction("xor eax, eax");                                    // report success after all diagnostic bytes were delivered
        emitter.instruction("leave");                                           // restore C linkage after non-reentrant diagnostic output
    }
    emitter.instruction("ret");                                                 // resume startup validation without invoking PHP or unwinding Rust
}
