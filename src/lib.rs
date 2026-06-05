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
/// Compiler pipeline for autoloading classes.
pub mod codegen;
/// Conditional compilation directives.
pub mod conditional;
/// Error and warning reporting.
pub mod errors;
/// Intrinsic call handling.
pub mod intrinsics;
/// Lexer for tokenizing PHP source.
pub mod lexer;
/// Magic constant substitution.
pub mod magic_constants;
/// Name resolution and mangling.
pub mod names;
/// Namespace and use resolution.
pub mod name_resolver;
/// Optimizer passes.
pub mod optimize;
/// Parser for PHP syntax.
pub mod parser;
/// PDO (SQLite) standard-library prelude injection.
pub mod pdo_prelude;
/// Resolution of includes.
pub mod resolver;
/// Source span tracking.
pub mod span;
mod string_bytes;
/// Termination and exit handling.
pub mod termination;
/// Type system and checking.
pub mod types;