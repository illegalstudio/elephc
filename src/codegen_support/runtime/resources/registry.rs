//! Purpose:
//! Selects the target-specific opaque resource-registry emitter.
//! The selected implementation exports one ABI-compatible helper surface.
//!
//! Called from:
//! - `crate::codegen_support::runtime::resources::emit_resource_runtime()`.
//!
//! Key details:
//! - macOS AArch64 and Linux AArch64 share the same register ABI.
//! - Linux x86_64 uses the System V AMD64 ABI.

mod aarch64;
mod x86_64;

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the resource registry for the active target architecture.
pub(super) fn emit_resource_registry(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => aarch64::emit_resource_registry_aarch64(emitter),
        Arch::X86_64 => x86_64::emit_resource_registry_x86_64(emitter),
    }
}
