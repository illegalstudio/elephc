//! Purpose:
//! Describes optional value-retirement paths without importing compiler feature types.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

/// Optional runtime paths that own special storage or foreign resources.
#[derive(Clone, Copy, Default)]
pub struct ValueFeatures {
    /// Enables eval bridge retirement support.
    pub eval_bridge: bool,
    /// Enables fiber retirement support.
    pub fiber: bool,
    /// Enables generator retirement support.
    pub generator: bool,
    /// Enables native reference validation retirement support.
    pub native_reference_validation: bool,
    /// Enables directory resource retirement support.
    pub directory_resource: bool,
    /// Enables popen resource retirement support.
    pub popen_resource: bool,
}
