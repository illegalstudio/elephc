//! Purpose:
//! Groups the exported C ABI entry points for the optional eval bridge.
//! Submodules are organized by the handle or operation family they expose.
//!
//! Called from:
//! - `crate` root re-exports for Rust tests and generated-linkage symbols.
//!
//! Key details:
//! - Every exported function installs a panic boundary before touching bridge state.
//! - Helper routines stay private to the FFI layer unless shared across families.

#[cfg(not(test))]
pub mod callables;
pub mod context;
pub mod declared_symbols;
pub(crate) mod dynamic_destructors;
pub mod execute;
#[cfg(not(test))]
pub mod function_calls;
pub mod native_functions;
pub mod native_methods;
#[cfg(not(test))]
pub mod object_construction;
#[cfg(not(test))]
pub mod object_introspection;
pub mod scope;
pub mod symbols;
pub(crate) mod util;

#[cfg(not(test))]
pub use callables::*;
pub use context::*;
pub use declared_symbols::*;
pub use execute::*;
#[cfg(not(test))]
pub use function_calls::*;
pub use native_functions::*;
pub use native_methods::*;
#[cfg(not(test))]
pub use object_construction::*;
#[cfg(not(test))]
pub use object_introspection::*;
pub use scope::*;
pub use symbols::*;

#[cfg(test)]
mod tests;
