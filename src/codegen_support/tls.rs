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
use crate::codegen_support::{abi, platform::Arch};

/// Stores elephc-tls C entry point addresses in the runtime indirection slots.
pub(crate) fn publish_tls_function_pointers(emitter: &mut Emitter) {
    const ENTRIES: &[(&str, &str)] = &[
        ("elephc_tls_connect", "_elephc_tls_connect_fn"),
        ("elephc_tls_connect_insecure", "_elephc_tls_connect_insecure_fn"),
        ("elephc_tls_connect_cafile", "_elephc_tls_connect_cafile_fn"),
        ("elephc_tls_connect_capath", "_elephc_tls_connect_capath_fn"),
        ("elephc_tls_connect_peer_name", "_elephc_tls_connect_peer_name_fn"),
        ("elephc_tls_write", "_elephc_tls_write_fn"),
        ("elephc_tls_read", "_elephc_tls_read_fn"),
        ("elephc_tls_close", "_elephc_tls_close_fn"),
        ("elephc_tls_attach_fd", "_elephc_tls_attach_fd_fn"),
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
                let extern_sym = emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(emitter, "r9", &extern_sym);
                abi::emit_store_reg_to_symbol(emitter, "r9", slot, 0);          // publish the elephc-tls entry into its runtime slot
            }
        }
    }
}
