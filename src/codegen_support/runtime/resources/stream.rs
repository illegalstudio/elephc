//! Purpose:
//! Selects the target-specific stream-state resource emitter.
//! These helpers adopt OS descriptors behind opaque handles and resolve them safely.
//!
//! Called from:
//! - `crate::codegen_support::runtime::resources::emit_resource_runtime()`.
//!
//! Key details:
//! - Descriptor extraction accepts Live and Closing resources but rejects Closed slots.
//! - An adoption failure closes the already-acquired descriptor.

mod aarch64;
mod x86_64;

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits stream adoption and lookup helpers for the active target architecture.
pub(super) fn emit_stream_resources(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => aarch64::emit_stream_resources_aarch64(emitter),
        Arch::X86_64 => x86_64::emit_stream_resources_x86_64(emitter),
    }
}
