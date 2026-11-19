//! Purpose:
//! The `elephc_ffi` bridge: PHP's `ext/ffi` runtime surface (C declaration parsing,
//! `FFI\CData`/`FFI\CType` metadata, memory operations, and dynamic C calls) behind a
//! panic-free C ABI linked into compiled programs that use `FFI`.
//!
//! Called from:
//! - Compiled PHP programs through the `elephc_ffi_*` entry points declared by the
//!   compiler's AST-built FFI prelude.
//! - `cargo test -p elephc-ffi` for the parser and layout unit tests.
//!
//! Key details:
//! - The bridge owns no PHP values: handles are pointer-sized integers and strings cross
//!   as bytes, matching the other elephc bridge crates.
//! - `types` and `parser` are dependency-free; `context`, `memory`, `call`, and `abi`
//!   add the runtime surfaces.

pub mod abi;
pub mod binary;
pub mod call;
pub mod callback;
pub mod context;
pub mod memory;
pub mod parser;
#[cfg(feature = "php-src-fixtures")]
pub mod php_src_fixtures;
pub mod place;
pub mod types;
pub mod zend_compat;
