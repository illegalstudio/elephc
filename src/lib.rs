//! Purpose:
//! Exposes the compiler modules used by integration tests and library consumers.
//! Keeps frontend, analysis, optimization, and codegen namespaces available from one crate root.
//!
//! Called from:
//! - External crates and Rust integration tests that import `elephc`.
//!
//! Key details:
//! - Public module boundaries here are part of the crate-facing compiler API.

mod brand;
mod call_graph;
mod cli;
mod closed_world;
mod reflection_table;
mod cli_entry;
mod debug_info;
mod mapped_file;
mod compile_thread_context;
mod eager_globals;
mod link_planning;
mod linker;
mod monitor;
mod reflection_parameter_prelude;
mod otlp;
mod pipeline;
mod pprof_encode;
mod probe_key;
mod repl;
mod runtime_cache;
mod session_prelude;
mod source_map;
mod test_command;
mod timings;

/// Runs command-line dispatch with compiler-owned stack management.
pub use cli_entry::run as run_cli;
pub(crate) use cli_entry::{apply_project_ini, emit_ini_override_warnings};

pub mod autoload;
/// Gradual and associative `array_merge()` compatibility prelude.
mod addcslashes_prelude;
/// The by-reference `$count` form of `str_replace()` as an AST-built prelude declaration.
mod str_replace_count_prelude;
/// `natsort()`/`natcasesort()` over gradual arrays, rewritten into `uasort()` at the call site.
mod natsort_gradual_prelude;
/// `htmlspecialchars()`'s `$double_encode` parameter as AST-built prelude declarations.
mod htmlspecialchars_prelude;
mod hash_pbkdf2_prelude;
mod http_build_query_prelude;
mod version_compare_prelude;
pub mod array_merge_prelude;
/// Default-initial-value `array_reduce()` compatibility prelude.
/// Single-argument `assert()` compatibility prelude.
pub mod assert_prelude;
pub mod throwable_string_prelude;
/// PHP's builtin `ErrorException`, declared as an AST-built class.
pub mod error_exception_prelude;
pub mod password_prelude;
pub mod array_edge_prelude;
pub mod array_gradual_prelude;
pub mod array_ucompare_prelude;
pub mod sodium_prelude;
pub mod attribute_prelude;
/// Whole-program PHP symbol usage collection for conditional prelude injection.
pub(crate) mod append_vivify;
pub mod ast_usage;
mod array_filter_prelude;
/// `array_any()` / `array_all()` as AST-built prelude declarations.
mod array_find_prelude;
pub mod lazy_static_defaults;
mod array_splice_prelude;
mod array_map_prelude;
mod array_keys_prelude;
mod array_walk_prelude;
mod proc_prelude;
mod csv_prelude;
mod exec_prelude;
mod stream_select_prelude;
mod min_max_prelude;
mod spread_prelude;
mod count_recursive_prelude;
mod numeric_string_arg_prelude;
mod array_fill_keys_prelude;
mod preg_array_prelude;
mod is_callable_prelude;
pub mod opcache_preload_sources;
pub mod scoped_include_units;
pub(crate) mod closure_from_callable_prelude;
pub(crate) mod locale_prelude;
pub(crate) mod bcmath_number_prelude;
pub(crate) mod stdclass_by_ref_walk;
pub(crate) mod php_token_prelude;
pub(crate) mod globals_snapshot_prelude;
pub(crate) mod closed_world_extract_refs;
pub(crate) mod stream_args_prelude;
/// Narrow PHP-level fallbacks for builtin shapes not handled natively yet.
pub mod backend_gap_prelude;
mod standard_gap_prelude;
mod filter_ext_prelude;
mod string_bitnot_prelude;
mod strnatcmp_prelude;
mod strtok_prelude;
mod randomizer_prelude;
mod builtin_callable_prelude;
mod magic_call_prelude;
pub mod static_ref_capture;
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
/// Project `elephc.toml` `[ini]` manifest entries applied ahead of the command line's `--ini`.
pub mod project_ini;
/// Conditional compilation directives.
pub mod conditional;
/// Execution-time binding for conditionally declared PHP functions.
pub mod conditional_classes;
pub mod conditional_functions;
/// Standard DOM class-surface prelude injection.
pub mod dom_html_prelude;
pub mod dom_prelude;
/// `ext/curl` easy-handle standard-library prelude injection (`CurlHandle` + `curl_*`).
pub mod curl_prelude;
/// Error and warning reporting.
pub mod errors;
/// PHP's SAPI-independent error/exception-handler surface, shared by the `--web` prelude and
/// the pay-for-use CLI injection.
pub mod error_handling_prelude;
mod eval_aot;
/// `#[Export]` attribute scan for cdylib emission.
pub mod exports;
/// Dynamic-filter dispatch prelude used by source-sensitive `filter_var()` lowering.
pub mod filter_var_prelude;
/// Fast non-cryptographic hash-map aliases for the compiler's internal keying.
pub mod fast_hash;
/// PHP variadic-argument introspection (`func_num_args`/`func_get_args`/`func_get_arg`) desugaring.
/// PHPStan/Psalm generic annotations read out of doc comments.
pub mod docblock;
pub mod func_args;
/// Monomorphization support for generic function declarations.
pub mod generics;
pub mod literal_format_split;
pub(crate) mod highlight_prelude;
/// The program-wide set of names some body declares `global`, shared by the checker and lowering.
pub(crate) mod global_decls;
/// `$GLOBALS` literal-key aliasing onto ordinary global storage.
pub mod globals_array;
mod progress;
/// Immutable BCMath Number standard-library prelude injection.
pub mod bcmath_prelude;
/// PHP ext/gmp standard-library prelude injection.
pub mod gmp_prelude;
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
/// `parse_str()` query-string prelude injection.
mod parse_str_prelude;
/// `implode()` fallback for elements the native joiners cannot render.
mod implode_prelude;
/// `array_sum()` / `array_product()` fallback for arrays the native folds cannot sum.
mod array_pad_prelude;
mod array_sum_prelude;
/// UTF-8 mbstring functions with no native implementation.
mod program_declares;
/// Run-time constant table behind `define()` calls with a computed name.
mod dynamic_define_prelude;
/// Selected PHP compatibility version for version-sensitive compiler surfaces.
pub mod php_version;
/// PDO (SQLite) standard-library prelude injection.
pub mod pdo_prelude;
/// mysqli (MySQL / MariaDB over the elephc_pdo bridge) prelude injection.
pub mod mysqli_prelude;

/// Hosting support for real PHP extensions (surface derivation from stubs).
pub mod php_ext;
/// Producer-side SDK validation and registration for real PHP hosts.
pub mod phpext;
/// PHP language-profile selection and profile-dependence analysis.
pub mod php_profile;
/// Reachability pruning of injected prelude declarations.
pub(crate) mod prelude_prune;
/// Resolution of includes.
pub mod resolver;
/// The compile-time SAPI, for the front-end behavior that depends on it.
pub(crate) mod sapi;
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
pub mod entry_return;
pub mod web_prelude;
