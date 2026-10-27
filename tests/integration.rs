//! Purpose:
//! Integration-test binary gathering every small suite under `tests/` as one module each,
//! so a library edit rebuilds and relinks one binary instead of one per suite.
//!
//! Called from:
//! - `cargo test --test integration` (or `cargo nextest run`) through Rust's test harness.
//!
//! Key details:
//! - Each suite keeps its own file and is mounted with `#[path]`, so its relative
//!   `include_str!` and nested `#[path]` modules resolve exactly as before; test names
//!   are prefixed with the suite, e.g. `web_tests::...`.
//! - `Cargo.toml` sets `autotests = false`: a `tests/*.rs` file is built only when it is
//!   declared here or as a `[[test]]` target. `every_suite_file_is_registered` enforces it.

#[path = "array_flip_assoc_tests.rs"]
mod array_flip_assoc_tests;
#[path = "array_map_result_heap_tests.rs"]
mod array_map_result_heap_tests;
#[path = "array_result_type_tests.rs"]
mod array_result_type_tests;
#[path = "autoload_diagnostics_tests.rs"]
mod autoload_diagnostics_tests;
#[path = "builtin_parity_tests.rs"]
mod builtin_parity_tests;
#[path = "cdylib_tests.rs"]
mod cdylib_tests;
#[path = "compiler_determinism_tests.rs"]
mod compiler_determinism_tests;
#[path = "compiler_security_limits_tests.rs"]
mod compiler_security_limits_tests;
#[path = "error_class_hierarchy_tests.rs"]
mod error_class_hierarchy_tests;
#[path = "eval_argument_count_tests.rs"]
mod eval_argument_count_tests;
#[path = "eval_include_template_tests.rs"]
mod eval_include_template_tests;
#[path = "eval_inline_html_tests.rs"]
mod eval_inline_html_tests;
#[path = "eval_object_handle_tests.rs"]
mod eval_object_handle_tests;
#[path = "eval_resource_id_tests.rs"]
mod eval_resource_id_tests;
#[path = "eval_string_interpolation_tests.rs"]
mod eval_string_interpolation_tests;
#[path = "extension_loaded_tests.rs"]
mod extension_loaded_tests;
#[path = "ffi_buffer_hygiene.rs"]
mod ffi_buffer_hygiene;
#[path = "finally_pending_exception_tests.rs"]
mod finally_pending_exception_tests;
#[path = "foreach_non_iterable_tests.rs"]
mod foreach_non_iterable_tests;
#[path = "fresh_result_ownership_leak_tests.rs"]
mod fresh_result_ownership_leak_tests;
#[path = "function_exists_tests.rs"]
mod function_exists_tests;
#[path = "include_scoped_return_tests.rs"]
mod include_scoped_return_tests;
#[path = "inlined_concat_reset_tests.rs"]
mod inlined_concat_reset_tests;
#[path = "ir_backend_smoke_test.rs"]
mod ir_backend_smoke_test;
#[path = "nan_bool_coercion_tests.rs"]
mod nan_bool_coercion_tests;
#[path = "native_pcre2_shim_tests.rs"]
mod native_pcre2_shim_tests;
#[path = "null_coalesce_merge_tests.rs"]
mod null_coalesce_merge_tests;
#[path = "null_value_codegen_tests.rs"]
mod null_value_codegen_tests;
#[path = "opcache_blacklist_tests.rs"]
mod opcache_blacklist_tests;
#[path = "opcache_env_override_tests.rs"]
mod opcache_env_override_tests;
#[path = "opcache_file_cache_tests.rs"]
mod opcache_file_cache_tests;
#[path = "opcache_ini_tests.rs"]
mod opcache_ini_tests;
#[path = "opcache_jit_status_tests.rs"]
mod opcache_jit_status_tests;
#[path = "opcache_manifest_tests.rs"]
mod opcache_manifest_tests;
#[path = "opcache_preload_tests.rs"]
mod opcache_preload_tests;
#[path = "opcache_restrict_api_tests.rs"]
mod opcache_restrict_api_tests;
#[path = "opcache_runtime_cache_tests.rs"]
mod opcache_runtime_cache_tests;
#[path = "opcache_strict_invalidate_tests.rs"]
mod opcache_strict_invalidate_tests;
#[path = "php_ext_tests.rs"]
mod php_ext_tests;
#[path = "php_profile_floor_tests.rs"]
mod php_profile_floor_tests;
#[path = "php_profile_independence_tests.rs"]
mod php_profile_independence_tests;
#[path = "php_profile_resolve_tests.rs"]
mod php_profile_resolve_tests;
#[path = "php_profile_session_independence_tests.rs"]
mod php_profile_session_independence_tests;
#[path = "php_version_surface_tests.rs"]
mod php_version_surface_tests;
#[path = "print_r_object_tests.rs"]
mod print_r_object_tests;
#[path = "print_r_return_mode_heap_tests.rs"]
mod print_r_return_mode_heap_tests;
#[path = "project_ini_tests.rs"]
mod project_ini_tests;
#[path = "recursive_return_type_tests.rs"]
mod recursive_return_type_tests;
#[path = "resource_id_and_hash_context_tests.rs"]
mod resource_id_and_hash_context_tests;
#[path = "runtime_promoted_array_reader_tests.rs"]
mod runtime_promoted_array_reader_tests;
#[path = "runtime_string_capacity_tests.rs"]
mod runtime_string_capacity_tests;
#[path = "static_property_string_ownership_tests.rs"]
mod static_property_string_ownership_tests;
#[path = "uncaught_exception_report_tests.rs"]
mod uncaught_exception_report_tests;
#[path = "var_dump_nested_tests.rs"]
mod var_dump_nested_tests;
#[path = "var_dump_object_tests.rs"]
mod var_dump_object_tests;
#[path = "var_export_and_strstr_result_tests.rs"]
mod var_export_and_strstr_result_tests;
#[path = "verify_release_artifact.rs"]
mod verify_release_artifact;
#[path = "web_session_tests.rs"]
mod web_session_tests;
#[path = "web_tests.rs"]
mod web_tests;

/// Fails when a `tests/*.rs` suite is neither a module of this binary nor its own
/// `[[test]]` target in `Cargo.toml`: with `autotests = false` such a file would
/// silently never be built or run. A module must be named after its file, because
/// the nextest and CI filters select a suite by that `<stem>::` test-name prefix.
#[test]
fn every_suite_file_is_registered() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("read Cargo.toml");
    let this = std::fs::read_to_string(root.join("tests/integration.rs")).expect("read tests/integration.rs");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(root.join("tests")).expect("list tests/") {
        let name = entry.expect("tests/ entry").file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".rs") else { continue };
        if name == "integration.rs" {
            continue;
        }
        let as_module = format!("#[path = \"{name}\"]\nmod {stem};\n");
        let as_target = format!("path = \"tests/{name}\"");
        let is_target = manifest.lines().any(|line| line.trim() == as_target);
        if !this.contains(&as_module) && !is_target {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "tests/*.rs files that would never run; add each to tests/integration.rs as \
         `#[path = \"<file>\"] mod <stem>;` or as a [[test]] in Cargo.toml: {missing:?}"
    );
}
