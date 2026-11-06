//! Purpose:
//! Interpreter tests for the OPcache file functions once they answer about the RUNTIME
//! SCRIPT CACHE: `opcache_is_script_cached()`, `opcache_invalidate()` and
//! `opcache_compile_file()` driven from an eval fragment against real included files.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These drive the same process-wide cache the store tests do, so they take the shared
//!   `script_cache::store::lock_for_test()` guard, which also empties it.
//! - The disabled case is asserted first and separately: it is the state the compile-time
//!   const-folder runs in, and it must keep answering `false` for every path.
//! - Fixtures are real files under the temp directory. The cache's whole job is filesystem
//!   state, so a fake path would prove nothing.

use super::super::*;
use super::support::*;
use crate::script_cache::config::set_config;
use crate::script_cache::store::lock_for_test;
use crate::script_cache::ScriptCacheConfig;

/// Returns a configuration with the runtime script cache serving, as `--web` installs.
fn enabled_config() -> ScriptCacheConfig {
    ScriptCacheConfig {
        enabled: true,
        // Long enough that no test here crosses a revalidation boundary by accident.
        revalidate_freq: 3600,
        // The freshness guard is OFF for these fixtures. They are written microseconds
        // before the assertion, so php-src's default `file_update_protection = 2` would
        // refuse to cache every one of them and these tests would be measuring that guard
        // instead of what they mean to measure. Reference PHP needs the same
        // `-d opcache.file_update_protection=0` for exactly this reason.
        file_update_protection: 0,
        ..ScriptCacheConfig::disabled()
    }
}

/// Writes an includable fixture and returns its path as a PHP single-quoted literal.
fn fixture_literal(name: &str, contents: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "elephc-eval-opcache-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("fixture directory should be creatable");
    let path = dir.join("fixture.php");
    std::fs::write(&path, contents).expect("fixture should be writable");
    format!("'{}'", path.display())
}

/// Runs an eval fragment and returns everything it echoed.
fn run_fragment(source: &str) -> String {
    run_fragment_capturing(source).0
}

/// Runs an eval fragment and returns what it echoed along with the warnings it raised.
fn run_fragment_capturing(source: &str) -> (String, Vec<String>) {
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let warnings = values.warnings.clone();
    (values.output, warnings)
}

/// Verifies a disabled cache keeps every file function answering `false`.
///
/// This is the compile-time const-folder's state, and the pre-existing behaviour: nothing
/// is cached, so no path is cached, nothing can be invalidated, nothing can be compiled.
#[test]
fn a_disabled_cache_answers_false_for_every_file_function() {
    let _guard = lock_for_test();
    set_config(ScriptCacheConfig::disabled());
    let file = fixture_literal("disabled", "<?php $x = 1;");

    let output = run_fragment(&format!(
        r#"echo opcache_is_script_cached({file}) ? '1' : '0';
echo opcache_invalidate({file}) ? '1' : '0';
echo opcache_invalidate({file}, true) ? '1' : '0';
echo opcache_compile_file({file}) ? '1' : '0';"#
    ));

    assert_eq!(output, "0000");
}

/// Verifies an included file becomes reported as cached, and only then.
#[test]
fn including_a_file_makes_it_report_as_cached() {
    let _guard = lock_for_test();
    set_config(enabled_config());
    let file = fixture_literal("included", "<?php $x = 1;");

    let output = run_fragment(&format!(
        r#"echo opcache_is_script_cached({file}) ? '1' : '0';
include {file};
echo opcache_is_script_cached({file}) ? '1' : '0';"#
    ));

    assert_eq!(output, "01");
}

/// Verifies a forced invalidate discards the entry and a plain one does not.
///
/// Both return `true` because the path resolves, which is what php-src reports; only the
/// forced call moves `opcache_is_script_cached()`.
#[test]
fn only_a_forced_invalidate_discards_the_entry() {
    let _guard = lock_for_test();
    set_config(enabled_config());
    let file = fixture_literal("invalidate", "<?php $x = 1;");

    let output = run_fragment(&format!(
        r#"include {file};
echo opcache_invalidate({file}) ? '1' : '0';
echo opcache_is_script_cached({file}) ? '1' : '0';
echo opcache_invalidate({file}, true) ? '1' : '0';
echo opcache_is_script_cached({file}) ? '1' : '0';"#
    ));

    assert_eq!(output, "1110");
}

/// Verifies `opcache_invalidate()` reports whether the PATH RESOLVES, not cache membership.
///
/// php-src's `zend_accel_invalidate()` returns "cached OR resolvable"; an existing but
/// never-included file therefore reports `true`, and a nonexistent one `false`.
#[test]
fn invalidate_reports_whether_the_path_resolves() {
    let _guard = lock_for_test();
    set_config(enabled_config());
    let file = fixture_literal("resolves", "<?php $x = 1;");

    let output = run_fragment(&format!(
        r#"echo opcache_invalidate({file}) ? '1' : '0';
echo opcache_invalidate('/no/such/elephc/fixture.php') ? '1' : '0';"#
    ));

    assert_eq!(output, "10");
}

/// Verifies `opcache_compile_file()` caches a file that was never included.
///
/// This is the divergence the runtime cache closes: the function used to answer `false`
/// for every file outside the compile-time manifest, including ones the binary can run.
#[test]
fn compile_file_caches_a_file_that_was_never_included() {
    let _guard = lock_for_test();
    set_config(enabled_config());
    let file = fixture_literal("compile", "<?php $x = 1;");

    let output = run_fragment(&format!(
        r#"echo opcache_is_script_cached({file}) ? '1' : '0';
echo opcache_compile_file({file}) ? '1' : '0';
echo opcache_is_script_cached({file}) ? '1' : '0';"#
    ));

    assert_eq!(output, "011");
}

/// Verifies `opcache_compile_file()` re-caches an entry a forced invalidate discarded.
#[test]
fn compile_file_restores_a_discarded_entry() {
    let _guard = lock_for_test();
    set_config(enabled_config());
    let file = fixture_literal("restore", "<?php $x = 1;");

    let output = run_fragment(&format!(
        r#"include {file};
echo opcache_invalidate({file}, true) ? '1' : '0';
echo opcache_is_script_cached({file}) ? '1' : '0';
echo opcache_compile_file({file}) ? '1' : '0';
echo opcache_is_script_cached({file}) ? '1' : '0';"#
    ));

    assert_eq!(output, "1011");
}

/// Verifies `opcache_compile_file()` reports `false` AND warns for a file it cannot read.
///
/// Reference PHP 8.5.6 prints the same pair an `include` of a missing file does, naming
/// `opcache_compile_file` as the construct, before returning `false`. Asserting only the
/// `false` would have passed against a silent implementation, which is what this was.
#[test]
fn compile_file_warns_twice_and_reports_false_for_a_missing_file() {
    let _guard = lock_for_test();
    set_config(enabled_config());

    let (output, warnings) = run_fragment_capturing(
        r#"echo opcache_compile_file('/no/such/elephc/fixture.php') ? '1' : '0';"#,
    );

    assert_eq!(output, "0");
    assert_eq!(
        warnings,
        [
            "Warning: opcache_compile_file(/no/such/elephc/fixture.php): \
Failed to open stream: No such file or directory\n",
            "Warning: opcache_compile_file(): Failed opening \
'/no/such/elephc/fixture.php' for inclusion\n",
        ]
    );
}

/// Verifies a successful `opcache_compile_file()` raises no warning at all.
#[test]
fn compile_file_is_silent_when_it_succeeds() {
    let _guard = lock_for_test();
    set_config(enabled_config());
    let file = fixture_literal("silent", "<?php $x = 1;");

    let (output, warnings) = run_fragment_capturing(&format!(
        r#"echo opcache_compile_file({file}) ? '1' : '0';"#
    ));

    assert_eq!(output, "1");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
}

/// Verifies `opcache_is_script_cached_in_file_cache()` stays `false` with the cache on.
///
/// Exact, not a shortfall: php-src returns early on an unset `opcache.file_cache`, and
/// elephc has no on-disk opcode cache to point the directive at.
#[test]
fn the_file_cache_probe_stays_false_with_the_cache_enabled() {
    let _guard = lock_for_test();
    set_config(enabled_config());
    let file = fixture_literal("filecache", "<?php $x = 1;");

    let output = run_fragment(&format!(
        r#"include {file};
echo opcache_is_script_cached({file}) ? '1' : '0';
echo opcache_is_script_cached_in_file_cache({file}) ? '1' : '0';"#
    ));

    assert_eq!(output, "10");
}

/// Verifies a dynamic callable reaches the same cache answers as a direct call.
///
/// The by-values dispatch path receives evaluated handles rather than argument expressions
/// and used to answer `false` unconditionally, so `call_user_func` disagreed with a direct
/// call about a file the cache was actively serving.
#[test]
fn a_dynamic_callable_agrees_with_the_direct_call() {
    let _guard = lock_for_test();
    set_config(enabled_config());
    let file = fixture_literal("callable", "<?php $x = 1;");

    let output = run_fragment(&format!(
        r#"include {file};
echo opcache_is_script_cached({file}) ? '1' : '0';
echo call_user_func('opcache_is_script_cached', {file}) ? '1' : '0';
echo call_user_func('opcache_invalidate', {file}, true) ? '1' : '0';
echo call_user_func('opcache_is_script_cached', {file}) ? '1' : '0';
echo call_user_func('opcache_compile_file', {file}) ? '1' : '0';
echo opcache_is_script_cached({file}) ? '1' : '0';"#
    ));

    assert_eq!(output, "111011");
}

/// Verifies `opcache_reset()` reports `true` once, then `false`, and leaves the cache
/// ANSWERING until the restart is actually applied.
///
/// The once-then-false shape is php-src's: `zend_accel_schedule_restart()` clears the flag
/// `opcache_reset()`'s own guard tests, so only the first call in a request succeeds.
///
/// The third digit is the one that matters here. php-src DEFERS the restart to the next
/// request, so the script is still cached straight after the reset — VERIFIED on reference
/// PHP 8.5.10, which answers `true` there. The flush happens in
/// `script_cache::apply_pending_restart`, at a request boundary a CLI program never reaches.
#[test]
fn reset_reports_true_once_and_defers_the_flush() {
    let _guard = lock_for_test();
    set_config(enabled_config());
    let file = fixture_literal("reset", "<?php $x = 1;");

    let output = run_fragment(&format!(
        r#"include {file};
echo opcache_is_script_cached({file}) ? '1' : '0';
echo opcache_reset() ? '1' : '0';
echo opcache_is_script_cached({file}) ? '1' : '0';
echo opcache_reset() ? '1' : '0';"#
    ));

    assert_eq!(output, "1110", "cached, reset ok, STILL cached, second reset refused");
}

/// Verifies `opcache_reset()` still reports `false` while the cache is disabled.
///
/// This is the compile-time const-folder's state and reference PHP's `php script.php`.
#[test]
fn reset_reports_false_while_the_cache_is_disabled() {
    let _guard = lock_for_test();
    set_config(ScriptCacheConfig::disabled());

    assert_eq!(run_fragment("echo opcache_reset() ? '1' : '0';"), "0");
}
