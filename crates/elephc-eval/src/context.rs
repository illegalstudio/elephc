//! Purpose:
//! Declares the opaque process-level eval context handle.
//! The full implementation will hold dynamic function, class, constant, and
//! builtin registries plus runtime hooks.
//!
//! Called from:
//! - `crate::abi`
//! - `crate::__elephc_eval_execute()`
//!
//! Key details:
//! - The handle is intentionally opaque to generated code.
//! - No Rust-owned layout is promised across the C ABI.

use crate::abi::ABI_VERSION;

/// Process-level eval context passed opaquely across the C ABI.
///
/// Generated code never inspects this layout directly; it only passes pointers
/// back to the eval bridge. Keeping a concrete Rust type here lets the bridge
/// grow dynamic registries without exposing them to generated assembly.
pub struct ElephcEvalContext {
    abi_version: u32,
}

impl ElephcEvalContext {
    /// Creates a context using the current eval bridge ABI version.
    pub const fn new() -> Self {
        Self {
            abi_version: ABI_VERSION,
        }
    }

    /// Creates a context with an explicit ABI version for compatibility tests.
    #[cfg(test)]
    pub const fn for_abi_version(abi_version: u32) -> Self {
        Self { abi_version }
    }

    /// Returns the ABI version this context was created for.
    pub const fn abi_version(&self) -> u32 {
        self.abi_version
    }
}

impl Default for ElephcEvalContext {
    /// Creates the default process-level eval context.
    fn default() -> Self {
        Self::new()
    }
}
