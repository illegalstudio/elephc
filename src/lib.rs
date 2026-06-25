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
/// EIR-consuming assembly backend track.
pub mod codegen_ir;
/// Conditional compilation directives.
pub mod conditional;
/// Error and warning reporting.
pub mod errors;
/// `#[Export]` attribute scan for cdylib emission.
pub mod exports;
/// Intrinsic call handling.
pub mod intrinsics;
/// Intermediate representation used by the EIR backend track.
pub mod ir;
/// AST-to-EIR lowering pass used by `--emit-ir` diagnostics.
pub mod ir_lower;
/// IR-level analyses and transforms (liveness, intervals, register allocation).
pub mod ir_passes;
/// Lexer for tokenizing PHP source.
pub mod lexer;
/// Conditionally-injected `DateTimeZone::listIdentifiers` filtering prelude.
pub mod list_id_prelude;
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
/// Image (GD/Exif/Imagick/Gmagick/Cairo) standard-library prelude injection.
pub mod image_prelude;
/// Resolution of includes.
pub mod resolver;
/// Source span tracking.
pub mod span;
mod string_bytes;
/// Canonical HTTP-request superglobal set and shared type helper.
pub mod superglobals;
/// Termination and exit handling.
pub mod termination;
/// Type system and checking.
pub mod types;
/// Conditionally-injected timezone-introspection prelude (extern + marshalling).
pub mod tz_prelude;
/// Conditionally-injected `var_export` prelude (elephc-PHP rendering function).
pub mod var_export_prelude;
/// Conditionally-injected `--web` request prelude (extern declarations for bridge getters).
pub mod web_prelude;
