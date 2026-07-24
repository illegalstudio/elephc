//! Purpose:
//! Publishes elephc TLS bridge entry points into runtime function-pointer slots.
//! Keeps TLS bridge setup available to EIR without coupling it to I/O builtin lowering.
//!
//! Called from:
//! - `crate::codegen::block_emit` for web/bootstrap paths.
//! - `crate::codegen::lower_inst::builtins::io` for HTTPS and stream crypto builtins.
//!
//! Key details:
//! - Publishing happens in user assembly so only programs that use TLS helpers
//!   reference the elephc-tls staticlib entry points during linking.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::{Arch, Platform}};

/// Stores elephc-tls C entry point addresses in the runtime indirection slots.
pub(crate) fn publish_tls_function_pointers(emitter: &mut Emitter) {
    const ENTRIES: &[(&str, &str)] = &[
        ("elephc_tls_connect", "_elephc_tls_connect_fn"),
        (
            "elephc_tls_connect_with_options",
            "_elephc_tls_connect_with_options_fn",
        ),
        ("elephc_tls_connect_insecure", "_elephc_tls_connect_insecure_fn"),
        ("elephc_tls_connect_cafile", "_elephc_tls_connect_cafile_fn"),
        ("elephc_tls_connect_capath", "_elephc_tls_connect_capath_fn"),
        ("elephc_tls_connect_peer_name", "_elephc_tls_connect_peer_name_fn"),
        ("elephc_tls_write", "_elephc_tls_write_fn"),
        ("elephc_tls_read", "_elephc_tls_read_fn"),
        ("elephc_tls_close", "_elephc_tls_close_fn"),
        ("elephc_tls_handshake", "_elephc_tls_handshake_fn"),
        ("elephc_tls_attach_fd", "_elephc_tls_attach_fd_fn"),
        (
            "elephc_tls_attach_fd_with_options",
            "_elephc_tls_attach_fd_with_options_fn",
        ),
        (
            "elephc_tls_attach_fd_client_cert",
            "_elephc_tls_attach_fd_client_cert_fn",
        ),
        (
            "elephc_tls_connect_client_cert",
            "_elephc_tls_connect_client_cert_fn",
        ),
    ];
    match emitter.target.arch {
        Arch::AArch64 => {
            for (c_name, slot) in ENTRIES {
                let extern_sym = emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(emitter, "x9", &extern_sym);
                abi::emit_symbol_address(emitter, "x10", slot);
                emitter.instruction("str x9, [x10]");                           // publish the elephc-tls entry into its runtime slot
            }
        }
        Arch::X86_64 => {
            for (c_name, slot) in ENTRIES {
                if emitter.platform == Platform::Windows {
                    abi::emit_symbol_address(emitter, "r9", &format!("__rt_tls_abi_{c_name}"));
                } else {
                    let extern_sym = emitter.target.extern_symbol(c_name);
                    abi::emit_extern_symbol_address(emitter, "r9", &extern_sym);
                }
                abi::emit_store_reg_to_symbol(emitter, "r9", slot, 0);          // publish the elephc-tls entry into its runtime slot
            }
        }
    }
}

/// Emits Windows x86_64 adapters that translate the compiler's internal SysV
/// register convention to the MS x64 ABI used by Rust `extern "C"` exports.
pub(crate) fn emit_tls_abi_adapters(emitter: &mut Emitter) {
    if emitter.platform != Platform::Windows || emitter.target.arch != Arch::X86_64 {
        return;
    }

    for name in [
        "elephc_tls_connect",
        "elephc_tls_connect_insecure",
        "elephc_tls_write",
        "elephc_tls_read",
        "elephc_tls_attach_fd",
    ] {
        emit_adapter_3(emitter, name);
    }
    emit_adapter_5(emitter, "elephc_tls_attach_fd_with_options");
    emit_adapter_4(emitter, "elephc_tls_connect_with_options");
    emit_adapter_1(emitter, "elephc_tls_handshake");
    for name in [
        "elephc_tls_connect_cafile",
        "elephc_tls_connect_capath",
        "elephc_tls_connect_peer_name",
    ] {
        emit_adapter_5(emitter, name);
    }
    for name in [
        "elephc_tls_attach_fd_client_cert",
        "elephc_tls_connect_client_cert",
    ] {
        emit_adapter_7(emitter, name);
    }
    emit_adapter_1(emitter, "elephc_tls_close");
}

/// Emits one one-argument SysV-to-MS x64 TLS export adapter.
fn emit_adapter_1(emitter: &mut Emitter, name: &str) {
    emitter.label_global(&format!("__rt_tls_abi_{name}"));
    emitter.instruction("sub rsp, 40");                                         // reserve MS x64 shadow space and preserve call alignment
    emitter.instruction("mov rcx, rdi");                                        // translate positional argument one into the MS x64 register
    emitter.instruction(&format!("call {}", emitter.target.extern_symbol(name))); // invoke the Rust extern C export with the native Windows ABI
    emitter.instruction("add rsp, 40");                                         // release shadow space and restore the internal stack
    emitter.instruction("ret");                                                 // return the bridge result unchanged
    emitter.blank();
}

/// Emits one three-argument SysV-to-MS x64 TLS export adapter.
fn emit_adapter_3(emitter: &mut Emitter, name: &str) {
    emitter.label_global(&format!("__rt_tls_abi_{name}"));
    emitter.instruction("sub rsp, 40");                                         // reserve MS x64 shadow space and preserve call alignment
    emitter.instruction("mov r8, rdx");                                         // preserve and translate positional argument three first
    emitter.instruction("mov rdx, rsi");                                        // translate positional argument two
    emitter.instruction("mov rcx, rdi");                                        // translate positional argument one
    emitter.instruction(&format!("call {}", emitter.target.extern_symbol(name))); // invoke the Rust extern C export with the native Windows ABI
    emitter.instruction("add rsp, 40");                                         // release shadow space and restore the internal stack
    emitter.instruction("ret");                                                 // return the bridge result unchanged
    emitter.blank();
}

/// Emits one four-argument SysV-to-MS x64 TLS export adapter.
fn emit_adapter_4(emitter: &mut Emitter, name: &str) {
    emitter.label_global(&format!("__rt_tls_abi_{name}"));
    emitter.instruction("sub rsp, 40");                                         // reserve MS x64 shadow space and preserve call alignment
    emitter.instruction("mov r9, rcx");                                         // preserve and translate positional argument four first
    emitter.instruction("mov r8, rdx");                                         // translate positional argument three
    emitter.instruction("mov rdx, rsi");                                        // translate positional argument two
    emitter.instruction("mov rcx, rdi");                                        // translate positional argument one
    emitter.instruction(&format!("call {}", emitter.target.extern_symbol(name))); // invoke the Rust extern C export with the native Windows ABI
    emitter.instruction("add rsp, 40");                                         // release shadow space and restore the internal stack
    emitter.instruction("ret");                                                 // return the bridge result unchanged
    emitter.blank();
}

/// Emits one five-argument SysV-to-MS x64 TLS export adapter.
fn emit_adapter_5(emitter: &mut Emitter, name: &str) {
    emitter.label_global(&format!("__rt_tls_abi_{name}"));
    emitter.instruction("sub rsp, 56");                                         // reserve shadow, one stack argument, and alignment padding
    emitter.instruction("mov r10, rcx");                                        // preserve SysV positional argument four before overwriting rcx
    emitter.instruction("mov r11, r8");                                         // preserve SysV positional argument five before overwriting r8
    emitter.instruction("mov r8, rdx");                                         // translate positional argument three
    emitter.instruction("mov rdx, rsi");                                        // translate positional argument two
    emitter.instruction("mov rcx, rdi");                                        // translate positional argument one
    emitter.instruction("mov r9, r10");                                         // translate positional argument four
    emitter.instruction("mov QWORD PTR [rsp + 32], r11");                       // place positional argument five above MS shadow space
    emitter.instruction(&format!("call {}", emitter.target.extern_symbol(name))); // invoke the Rust extern C export with the native Windows ABI
    emitter.instruction("add rsp, 56");                                         // release shadow, stack argument, and padding
    emitter.instruction("ret");                                                 // return the bridge result unchanged
    emitter.blank();
}

/// Emits one seven-argument SysV-to-MS x64 TLS export adapter.
fn emit_adapter_7(emitter: &mut Emitter, name: &str) {
    emitter.label_global(&format!("__rt_tls_abi_{name}"));
    emitter.instruction("sub rsp, 72");                                         // reserve shadow plus three MS stack arguments and preserve alignment
    emitter.instruction("mov r10, rcx");                                        // preserve SysV positional argument four before overwriting rcx
    emitter.instruction("mov r11, r8");                                         // preserve SysV positional argument five before overwriting r8
    emitter.instruction("mov rax, r9");                                         // preserve SysV positional argument six before overwriting r9
    emitter.instruction("mov r8, rdx");                                         // translate positional argument three
    emitter.instruction("mov rdx, rsi");                                        // translate positional argument two
    emitter.instruction("mov rcx, rdi");                                        // translate positional argument one
    emitter.instruction("mov r9, r10");                                         // translate positional argument four
    emitter.instruction("mov QWORD PTR [rsp + 32], r11");                       // place positional argument five above MS shadow space
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // place positional argument six in the next MS stack slot
    emitter.instruction("mov r10, QWORD PTR [rsp + 80]");                       // load SysV positional argument seven from above the adapter frame
    emitter.instruction("mov QWORD PTR [rsp + 48], r10");                       // place positional argument seven in the third MS stack slot
    emitter.instruction(&format!("call {}", emitter.target.extern_symbol(name))); // invoke the Rust extern C export with the native Windows ABI
    emitter.instruction("add rsp, 72");                                         // release shadow and the three stack arguments
    emitter.instruction("ret");                                                 // return the bridge result unchanged
    emitter.blank();
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::codegen_support::platform::Target;

    use super::*;

    /// Verifies Windows publishing selects ABI adapters and a seven-argument
    /// adapter retrieves SysV argument seven before filling MS x64 stack slots.
    #[test]
    fn windows_tls_exports_use_ms_x64_adapters() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_tls_abi_adapters(&mut emitter);
        publish_tls_function_pointers(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_tls_abi_elephc_tls_attach_fd_client_cert:"));
        assert!(asm.contains("__rt_tls_abi_elephc_tls_attach_fd_with_options:"));
        assert!(asm.contains("__rt_tls_abi_elephc_tls_connect_with_options:"));
        assert!(asm.contains("__rt_tls_abi_elephc_tls_handshake:"));
        assert!(asm.contains("lea r9, [rip + __rt_tls_abi_elephc_tls_connect_with_options]"));
        assert!(asm.contains("mov r10, QWORD PTR [rsp + 80]"));
        assert!(asm.contains("mov QWORD PTR [rsp + 48], r10"));
        assert!(asm.contains("lea r9, [rip + __rt_tls_abi_elephc_tls_attach_fd]"));
    }

    /// Guards the STARTTLS lowering contract that distinguishes explicit
    /// crypto method zero from null/omission and forwards a reusable source
    /// session through both target ABIs.
    #[test]
    fn stream_crypto_lowering_forwards_method_presence_and_source_session() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let source =
            fs::read_to_string(root.join("src/codegen/lower_inst/builtins/io.rs"))
                .expect("read stream crypto lowering");
        for fragment in [
            "str x9, [sp, #32]",
            "mov QWORD PTR [rsp + 32], 1",
            "str x0, [sp, #40]",
            "mov QWORD PTR [rsp + 40], rax",
            "ldr x3, [sp, #128]",
            "ldr x4, [sp, #136]",
            "mov rcx, QWORD PTR [rsp + 128]",
            "mov r8, QWORD PTR [rsp + 136]",
        ] {
            assert!(
                source.contains(fragment),
                "missing STARTTLS ABI fragment: {fragment}"
            );
        }
    }

    /// Guards every generated-assembly TLS slot consumer so published pointers
    /// stay explicitly classified as compiler-runtime ABI entries. On Windows
    /// these slots contain SysV-to-MSx64 adapters and must not be remapped again.
    #[test]
    fn tls_slot_consumers_use_published_bridge_calls() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for (relative, expected_calls) in [
            ("src/codegen/lower_inst/builtins/io.rs", 8usize),
            (
                "src/codegen_support/runtime/io/file_get_contents_url.rs",
                2,
            ),
            ("src/codegen_support/runtime/io/fread.rs", 2),
            ("src/codegen_support/runtime/io/ftp.rs", 8),
            ("src/codegen_support/runtime/io/fwrite.rs", 2),
            ("src/codegen_support/runtime/io/https.rs", 8),
            ("src/codegen_support/runtime/io/tls_session_table.rs", 2),
        ] {
            let source = fs::read_to_string(root.join(relative))
                .expect("TLS slot consumer source must be readable");
            assert_eq!(
                source.matches("emit_published_bridge_call").count(),
                expected_calls,
                "{relative} must classify every TLS slot call through emit_published_bridge_call"
            );
        }
    }
}
