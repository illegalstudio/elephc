//! Purpose:
//! Produces assembly text without PHP or ABI policy.
//!
//! Called from:
//! - Compiler memory emission and its host adapters.
//!
//! Key details:
//! - This capability runs only while the compiler emits assembly.

/// Produces assembly text without PHP or ABI policy.
pub trait AssemblyEmitter {
    /// Provides the compiler line comment prefix contract.
    fn line_comment_prefix(&self) -> &'static str;
    /// Delegates instruction emission to the host.
    fn instruction(&mut self, text: &str);
    /// Delegates label emission to the host.
    fn label(&mut self, name: &str);
    /// Delegates label global emission to the host.
    fn label_global(&mut self, name: &str);
    /// Delegates label shared emission to the host.
    fn label_shared(&mut self, name: &str);
    /// Delegates blank emission to the host.
    fn blank(&mut self);
    /// Delegates comment emission to the host.
    fn comment(&mut self, text: &str);
    /// Emits raw target directives while preserving their exact ordering.
    fn raw(&mut self, text: &str);
}
