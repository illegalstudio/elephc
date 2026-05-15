//! Purpose:
//! Exposes the compiler modules used by integration tests and library consumers.
//! Keeps frontend, analysis, optimization, and codegen namespaces available from one crate root.
//!
//! Called from:
//! - External crates and Rust integration tests that import `elephc`.
//!
//! Key details:
//! - Public module boundaries here are part of the crate-facing compiler API.

pub mod autoload;
pub mod codegen;
pub mod conditional;
pub mod errors;
pub mod lexer;
pub mod magic_constants;
pub mod names;
pub mod name_resolver;
pub mod optimize;
pub mod parser;
pub mod resolver;
pub mod span;
pub mod termination;
pub mod types;
