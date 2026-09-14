//! Purpose:
//! Implements the shared mbstring engine consumed by native programs and Magician.
//!
//! Called from:
//! - mbstring backend adapters and the bridge's focused compatibility tests.
//!
//! Key details:
//! - Unicode data is pinned to the PHP baseline rather than the Rust toolchain.
//! - Algorithms operate on decoded codepoints so every encoding shares semantics.

pub mod abi;
pub mod arrays;
pub mod coercion;
pub mod encoding;
pub mod detect;
pub mod error;
pub mod input;
pub mod mime;
pub mod regex;
pub mod state;
pub mod text;
pub mod unicode;
