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
/// `ext/curl` easy-handle standard-library prelude injection (`CurlHandle` + `curl_*`).
pub mod curl_prelude;
/// Error and warning reporting.
pub mod errors;
mod eval_aot;
/// `#[Export]` attribute scan for cdylib emission.
pub mod exports;
/// PHP variadic-argument introspection (`func_num_args`/`func_get_args`/`func_get_arg`) desugaring.
pub mod func_args;
/// The program-wide set of names some body declares `global`, shared by the checker and lowering.
pub(crate) mod global_decls;
mod progress;
/// Image (GD/Exif/Imagick/Gmagick/Cairo) standard-library prelude injection.
pub mod dir_prelude;
pub mod gz_prelude;
pub mod hash_prelude;
pub mod image_prelude;
/// Intrinsic call handling.
pub mod intrinsics;
/// Intermediate representation used by the EIR backend track.
pub mod ir;
/// AST-to-EIR lowering pass used by `--emit-ir` diagnostics.
pub mod ir_lower;
/// IR-level analyses and transforms (liveness, intervals, register allocation).
pub mod ir_passes;
/// Ordered typed final-link inputs shared with integration-test providers.
#[doc(hidden)]
pub mod link_plan;
/// Lexer for tokenizing PHP source.
pub mod lexer;
/// Conditionally-injected `DateTimeZone::listIdentifiers` filtering prelude.
pub mod list_id_prelude;
/// Magic constant substitution.
pub mod magic_constants;
/// Curated project-native dependency management and artifact resolution.
#[doc(hidden)]
pub mod native_deps;
/// Namespace and use resolution.
pub mod name_resolver;
/// Name resolution and mangling.
pub mod names;
/// Compile-time OPcache introspection data (directive matrix).
pub mod opcache;
/// `opcache_get_configuration()` standard-library prelude injection.
pub mod opcache_prelude;
/// Optimizer passes.
pub mod optimize;
/// Parser for PHP syntax.
pub mod parser;
/// Selected PHP compatibility version for version-sensitive compiler surfaces.
pub mod php_version;
/// PDO (SQLite) standard-library prelude injection.
pub mod pdo_prelude;
/// mysqli (MySQL / MariaDB over the elephc_pdo bridge) prelude injection.
pub mod mysqli_prelude;

/// PHP language-profile selection and profile-dependence analysis.
pub mod php_profile;
/// Reachability pruning of injected prelude declarations.
pub(crate) mod prelude_prune;
/// Resolution of includes.
pub mod resolver;
/// PHP `sscanf`/`fscanf` engine prelude injection.
pub mod similar_text_prelude;
pub mod scanf_prelude;
/// Physical source-file classification and per-file language profiles.
pub mod source;
/// Source span tracking.
pub mod span;
/// `--strict-php` mode state and PHP-compatibility audit pass.
pub mod strict_php;
mod string_bytes;
/// Canonical HTTP-request superglobal set and shared type helper.
pub mod superglobals;
/// Rust builder for the synthetic PHP class surfaces the compiler injects itself.
pub mod synthetic_class;
/// Termination and exit handling.
pub mod termination;
/// Type system and checking.
pub mod stream_compliance;
pub mod types;
/// Conditionally-injected timezone-introspection prelude (extern + marshalling).
pub mod tz_prelude;
/// Conditionally-injected `var_export` prelude (elephc-PHP rendering function).
pub mod var_export_prelude;
/// Conditionally-injected PHP version-surface prelude (`zend_version`, `php_sapi_name`,
/// `ini_restore`).
pub mod version_prelude;
/// Conditionally-injected `--web` request prelude (extern declarations for bridge getters).
pub mod web_prelude;
