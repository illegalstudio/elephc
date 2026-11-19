//! Purpose:
//! Supplies memory facts and explicit compiler-owned semantic hooks.
//!
//! Called from:
//! - Compiler memory emission and its host adapters.
//!
//! Key details:
//! - This capability runs only while the compiler emits assembly.

use crate::{GraphLayout, GcStatusSelectors};

/// Supplies memory facts and explicit compiler-owned semantic hooks.
pub trait MemoryContext {
    /// Provides the compiler graph layout contract.
    fn graph_layout(&self) -> GraphLayout;
    /// Provides the compiler gc status selectors contract.
    fn gc_status_selectors(&self) -> GcStatusSelectors;
    /// Returns the configured string scratch capacity.
    fn concat_capacity(&self) -> usize;
    /// Keeps reference-registry status translation in the compiler.
    fn emit_reference_source_success(&mut self);
    /// Supplies the compiler's marker for an uninitialized PHP property payload.
    fn uninitialized_property_sentinel(&self) -> i64;
    /// Emits PHP resource policy inside the Mixed boxing frame, including its resource entry label.
    fn emit_mixed_resource_identity(&mut self);
}
