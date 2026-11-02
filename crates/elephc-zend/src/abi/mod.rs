//! Purpose:
//! The extension ABI's data definitions: every type, constant and C-defined
//! function an extension's C code sees, declared once in Rust.
//!
//! Called from:
//! - The engine modules, through `use crate::abi::*`.
//! - `build.rs`, which renders these files (in `ABI_FILES` order) as the C
//!   header extensions compile against.
//!
//! Key details:
//! - Only data lives here; behaviour is in the engine modules, whose
//!   `#[no_mangle]` functions `build.rs` declares as well.

pub mod c_side;
pub mod engine;
pub mod runtime;
pub mod scalars;
pub mod values;
mod value_layout;

pub use c_side::*;
pub use engine::*;
pub use runtime::*;
pub use scalars::*;
pub use values::*;
