//! Purpose:
//! Provides the binary entry point for the compiler and native dependency commands.
//! Hands the process arguments to the library's command dispatch.
//!
//! Called from:
//! - The operating system when running the `elephc` executable.
//!
//! Key details:
//! - Declares no modules: the compiler lives in the `elephc` library, so the binary
//!   is only linked, never a second compilation of the module tree.

/// Entry point for the `elephc` binary.
///
/// Runs [`elephc::entry::run`] on the stack established by
/// [`elephc::compiler_stack::with_compiler_stack`].
fn main() {
    elephc::compiler_stack::with_compiler_stack(elephc::entry::run)
}
