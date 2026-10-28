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
/// Local Cargo source inputs shared by bridge discovery and integration-test builders.
#[doc(hidden)]
pub mod bridge_sources;
/// Builtin catalog and signature metadata snapshots.
pub mod builtin_metadata;
/// Single-source builtin registry: catalog, signatures, type-check, and lowering dispatch.
pub mod builtins;
/// Canonical EIR-consuming assembly backend and public codegen helpers.
pub mod codegen;
/// Shared target/runtime support used by the EIR backend.
#[doc(hidden)]
pub mod codegen_support;
/// The stack depth a compiler run needs, and the wrapper that supplies it.
pub mod compiler_stack;
/// Conditional compilation directives.
pub mod conditional;
/// `ext/curl` easy-handle standard-library prelude injection (`CurlHandle` + `curl_*`).
pub mod curl_prelude;
/// Termwind-facing DOM HTML subset (`DOMDocument::loadHTML` and node walk).
pub mod dom_html_prelude;
/// Error and warning reporting.
pub mod errors;
mod eval_aot;
/// `#[Export]` attribute scan for cdylib emission.
pub mod exports;
/// PHP variadic-argument introspection (`func_num_args`/`func_get_args`/`func_get_arg`) desugaring.
/// PHPStan/Psalm generic annotations read out of doc comments.
pub mod docblock;
pub mod func_args;
/// Monomorphization support for generic function declarations.
pub mod generics;
/// The program-wide set of names some body declares `global`, shared by the checker and lowering.
pub(crate) mod global_decls;
mod progress;
/// Immutable BCMath Number standard-library prelude injection.
pub mod bcmath_prelude;
pub mod random_prelude;
/// Incremental hashing standard-library prelude injection.
pub mod hash_prelude;
/// Core attribute and sensitive-value class declarations.
pub mod core_prelude;
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
/// Compile-time implementation of PHP's numeric-string grammar.
pub(crate) mod numeric_string;
/// Compile-time OPcache introspection data (directive matrix).
pub mod opcache;
/// `opcache_get_configuration()` standard-library prelude injection.
pub mod opcache_prelude;
pub mod shared_ini_prelude;
/// PHP `(object)` cast standard-library prelude injection.
pub mod object_cast_prelude;
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

/// Hosting support for real PHP extensions (surface derivation from stubs).
pub mod php_ext;
/// PHP language-profile selection and profile-dependence analysis.
pub mod php_profile;
/// Reachability pruning of injected prelude declarations.
pub(crate) mod prelude_prune;
/// Resolution of includes.
pub mod resolver;
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
pub mod testing_prelude;
/// Type system and checking.
pub mod types;
/// Conditionally-injected timezone-introspection prelude (extern + marshalling).
pub mod tz_prelude;
pub mod xml_prelude;
pub mod fileinfo_prelude;
pub mod tokenizer_prelude;
/// Conditionally-injected `var_export` prelude (elephc-PHP rendering function).
pub mod var_export_prelude;
/// Conditionally-injected PHP version-surface prelude (`zend_version`, `php_sapi_name`,
/// `ini_restore`).
pub mod version_prelude;
/// Conditionally-injected `--web` request prelude (extern declarations for bridge getters).
pub mod web_prelude;

// The `elephc` command line. The binary only calls `entry::run`; these modules are
// crate-private so the library's public surface stays the compiler API.

/// Top-level command dispatch the `elephc` binary runs.
pub mod entry;
/// The elephc mark, inlined as a `data:` URI for self-contained generated pages.
pub(crate) mod brand;
/// Aggregated PHP-level call-graph rendering for `elephc monitor`.
pub(crate) mod call_graph;
/// Top-level command parsing, compiler options, and target selection.
pub(crate) mod cli;
/// DWARF debug information for generated user assembly (`--debug-info`).
pub(crate) mod debug_info;
/// Ordered typed final-link plans from compiler, runtime, user, and managed inputs.
pub(crate) mod link_planning;
/// Assembler/linker facade used by the compiler pipeline.
pub(crate) mod linker;
/// `elephc monitor`: sample a compiled program and render a PHP-level profile.
pub(crate) mod monitor;
/// Profiled request slices exported as OpenTelemetry spans over OTLP/HTTP.
pub(crate) mod otlp;
/// The full PHP source to native binary compilation flow.
pub(crate) mod pipeline;
/// Gzip-compressed pprof `Profile` protobuf encoder.
pub(crate) mod pprof_encode;
/// Compile-time build-key material for `--probe`.
pub(crate) mod probe_key;
/// The `[ini]` table of the project's `elephc.toml`.
pub(crate) mod project_ini;
/// `elephc repl`: an eval-backed PHP session from a cached native host.
pub(crate) mod repl;
/// The cached runtime object linked beside generated user code.
pub(crate) mod runtime_cache;
/// The v2 JSON source map for generated assembly.
pub(crate) mod source_map;
/// The `elephc test` subcommand.
pub(crate) mod test_command;
/// Compile-phase durations for CLI timing output.
pub(crate) mod timings;
