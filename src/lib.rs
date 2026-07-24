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
/// Builtin catalog and signature metadata snapshots.
pub mod builtin_metadata;
/// Single-source builtin registry: catalog, signatures, type-check, and lowering dispatch.
pub mod builtins;
/// Canonical EIR-consuming assembly backend and public codegen helpers.
pub mod codegen;
/// Shared target/runtime support used by the EIR backend.
#[doc(hidden)]
pub mod codegen_support;
/// Conditional compilation directives.
pub mod conditional;
/// Error and warning reporting.
pub mod errors;
mod eval_aot;
/// `#[Export]` attribute scan for cdylib emission.
pub mod exports;
mod progress;
/// Image (GD/Exif/Imagick/Gmagick/Cairo) standard-library prelude injection.
pub mod image_prelude;
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
/// Namespace and use resolution.
pub mod name_resolver;
/// Name resolution and mangling.
pub mod names;
/// Optimizer passes.
pub mod optimize;
/// Parser for PHP syntax.
pub mod parser;
/// PDO (SQLite) standard-library prelude injection.
pub mod pdo_prelude;
/// Resolution of includes.
pub mod resolver;
mod source_path;
/// Source span tracking.
pub mod span;
/// `--strict-php` mode state and PHP-compatibility audit pass.
pub mod strict_php;
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
