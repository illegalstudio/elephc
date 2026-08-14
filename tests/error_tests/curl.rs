//! Purpose:
//! Compile-time diagnostic tests for the `ext/curl` prelude surface: `CurlHandle` is
//! `final` and not user-constructible, and the `curl_*` wrappers reject wrong argument
//! counts and wrong argument types.
//!
//! Called from:
//! - `cargo test --test error_tests curl` through Rust's test harness.
//!
//! Key details:
//! - These need only the INJECTED PRELUDE, never a link, so unlike
//!   `tests/codegen/curl/` they run on every machine whether or not the managed native
//!   `curl` package is installed. That is exactly why the two object-model rules
//!   (`final`, private constructor) are pinned here rather than there.
//! - `check_source` already injects the curl prelude between alias collection and name
//!   resolution, mirroring `pipeline::compile`, so the checker sees the prelude's typed
//!   signatures.
//! - Every assertion also checks the message is a REAL diagnostic rather than an
//!   "Undefined function/class" one, which is what a silently broken injection would
//!   produce.

use super::*;

/// Asserts `src` fails to compile and the message contains `needle`, with a guard against
/// a missing-prelude error masquerading as the expected diagnostic.
fn expect_curl_error(src: &str, needle: &str) {
    let error = check_source(src).expect_err("program must fail to compile");
    assert!(
        !error.contains("Undefined function") && !error.contains("Undefined class"),
        "curl prelude was not injected; got: {error}"
    );
    assert!(
        error.contains(needle),
        "expected an error containing {needle:?}, got: {error}"
    );
}

/// PHP's `CurlHandle` is `final`: a session object is minted by `curl_init()` and by
/// nothing else, so a user class cannot extend it.
#[test]
fn curl_handle_cannot_be_extended() {
    expect_curl_error(
        "<?php class MyHandle extends CurlHandle {} $h = curl_init();",
        "final",
    );
}

/// `CurlHandle`'s constructor is private, matching php-src, so `new CurlHandle()` is
/// rejected. A user must go through `curl_init()`.
#[test]
fn curl_handle_cannot_be_constructed() {
    expect_curl_error("<?php $h = new CurlHandle();", "private");
}

/// `curl_setopt()` takes exactly three arguments, so a call missing the value is an arity
/// error rather than a silently ignored option.
#[test]
fn curl_setopt_rejects_wrong_arity() {
    expect_curl_error(
        "<?php $ch = curl_init(); curl_setopt($ch, 10002);",
        "curl_setopt",
    );
}

/// A `curl_*` function that takes a handle rejects a non-handle argument at compile time,
/// which is the earliest point elephc can tell the difference.
#[test]
fn curl_exec_rejects_a_non_handle() {
    expect_curl_error(r#"<?php curl_exec("not a handle");"#, "curl_exec");
}

/// `curl_init()` accepts an optional URL string; passing an int is a type error.
#[test]
fn curl_init_rejects_a_non_string_url() {
    expect_curl_error("<?php $ch = curl_init(42);", "curl_init");
}

/// `curl_setopt()`'s first parameter is typed `CurlHandle`, so passing an int is a
/// compile-time type error naming that class — the earliest point elephc can tell the
/// difference, and the same guard every other `curl_*` wrapper carries.
#[test]
fn curl_setopt_rejects_wrong_handle_type() {
    expect_curl_error(
        r#"<?php curl_setopt(1, CURLOPT_URL, "http://127.0.0.1/");"#,
        "CurlHandle",
    );
}

/// `curl_getinfo()`'s handle parameter is typed the same way, so the wrong-handle
/// diagnostic is not a `curl_setopt()`-only accident.
#[test]
fn curl_getinfo_rejects_wrong_handle_type() {
    expect_curl_error(
        r#"<?php curl_getinfo("nope");"#,
        "CurlHandle",
    );
}

/// `CurlMultiHandle` is `final` and not user-constructible, exactly like `CurlHandle`:
/// `curl_multi_init()` is the only way to obtain one.
#[test]
fn curl_multi_handle_cannot_be_extended_or_constructed() {
    expect_curl_error(
        "<?php class MyMulti extends CurlMultiHandle {} $mh = curl_multi_init();",
        "final",
    );
    expect_curl_error("<?php $mh = new CurlMultiHandle();", "private");
}

/// `curl_multi_exec()`'s `$still_running` is REQUIRED, so a one-argument call is an arity
/// error rather than a call that silently loses the count.
#[test]
fn curl_multi_exec_requires_the_still_running_argument() {
    expect_curl_error(
        "<?php $mh = curl_multi_init(); curl_multi_exec($mh);",
        "curl_multi_exec",
    );
}

/// The multi functions type their handle parameter, so a non-handle is refused at compile
/// time — the same guard the easy surface carries.
#[test]
fn curl_multi_functions_reject_a_non_handle() {
    expect_curl_error(
        r#"<?php $r = 0; curl_multi_exec("nope", $r);"#,
        "CurlMultiHandle",
    );
    expect_curl_error(r#"<?php curl_multi_errno(42);"#, "CurlMultiHandle");
    expect_curl_error(r#"<?php curl_multi_select("nope");"#, "CurlMultiHandle");
}

/// `curl_multi_add_handle()` takes its easy handle as `mixed` (see `curl_prelude`'s header
/// for why the backend forces that), so the WRONG TYPE is caught at RUN time by the
/// wrapper's own `instanceof` guard rather than by the checker. What must still fail at
/// compile time is the arity, which is what this pins — a silently dropped second argument
/// would attach nothing at all.
#[test]
fn curl_multi_add_handle_requires_both_arguments() {
    expect_curl_error(
        "<?php $mh = curl_multi_init(); curl_multi_add_handle($mh);",
        "curl_multi_add_handle",
    );
}

/// `curl_multi_get_handles()` is PHP 8.5 SURFACE ONLY: compiled for
/// 8.4 the prelude must not declare it, so the call is an ordinary "undefined function"
/// error — exactly what that runtime reports — while 8.5 accepts it.
#[test]
fn curl_multi_get_handles_is_undefined_before_php_85() {
    let source = "<?php $mh = curl_multi_init(); $h = curl_multi_get_handles($mh);";
    let error = check_source_for_php_version(source, elephc::php_version::PhpVersion::Php84)
        .expect_err("8.4 must not declare curl_multi_get_handles");
    assert!(
        error.contains("Undefined function") && error.contains("curl_multi_get_handles"),
        "8.4 must report an undefined function, got: {error}"
    );
    assert!(
        check_source_for_php_version(source, elephc::php_version::PhpVersion::Php85).is_ok(),
        "8.5 must declare curl_multi_get_handles"
    );
}

/// `CurlShareHandle` is `final` and not user-constructible, exactly like `CurlHandle`/
/// `CurlMultiHandle`: `curl_share_init()` is the only way to obtain one.
#[test]
fn curl_share_handle_cannot_be_extended_or_constructed() {
    expect_curl_error(
        "<?php class MyShare extends CurlShareHandle {} $sh = curl_share_init();",
        "final",
    );
    expect_curl_error("<?php $sh = new CurlShareHandle();", "private");
}

/// `curl_share_setopt()` takes exactly three arguments, so a call missing the value is an
/// arity error rather than a silently ignored option.
#[test]
fn curl_share_setopt_rejects_wrong_arity() {
    expect_curl_error(
        "<?php $sh = curl_share_init(); curl_share_setopt($sh, CURLSHOPT_SHARE);",
        "curl_share_setopt",
    );
}

/// The share functions type their handle parameter, so a non-handle is refused at compile
/// time — the same guard the easy/multi surfaces carry.
#[test]
fn curl_share_functions_reject_a_non_handle() {
    expect_curl_error(
        r#"<?php curl_share_setopt("nope", CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);"#,
        "CurlShareHandle",
    );
    expect_curl_error(r#"<?php curl_share_errno(42);"#, "CurlShareHandle");
    expect_curl_error(r#"<?php curl_share_close([]);"#, "CurlShareHandle");
}

/// `curl_share_errno()` takes exactly one argument.
#[test]
fn curl_share_errno_rejects_wrong_arity() {
    expect_curl_error(
        "<?php $sh = curl_share_init(); curl_share_errno($sh, 1);",
        "curl_share_errno",
    );
}

/// `curl_share_init_persistent()` / `CurlSharePersistentHandle` are PHP 8.5 SURFACE ONLY,
/// the same gate `curl_multi_get_handles()` has: compiled for 8.4
/// the prelude must not declare either, so each reference is an ordinary "undefined
/// function"/"undefined class" error, while 8.5 accepts both.
#[test]
fn curl_share_init_persistent_is_undefined_before_php_85() {
    let function_source = "<?php $sh = curl_share_init_persistent([CURL_LOCK_DATA_DNS]);";
    let error =
        check_source_for_php_version(function_source, elephc::php_version::PhpVersion::Php84)
            .expect_err("8.4 must not declare curl_share_init_persistent");
    assert!(
        error.contains("Undefined function") && error.contains("curl_share_init_persistent"),
        "8.4 must report an undefined function, got: {error}"
    );
    assert!(
        check_source_for_php_version(function_source, elephc::php_version::PhpVersion::Php85)
            .is_ok(),
        "8.5 must declare curl_share_init_persistent"
    );

    let class_source = "<?php function f(CurlSharePersistentHandle $h): bool { return true; }";
    assert!(
        check_source_for_php_version(class_source, elephc::php_version::PhpVersion::Php84)
            .is_err(),
        "8.4 must not declare CurlSharePersistentHandle"
    );
    assert!(
        check_source_for_php_version(class_source, elephc::php_version::PhpVersion::Php85)
            .is_ok(),
        "8.5 must declare CurlSharePersistentHandle"
    );
}

/// `CURLFile`'s `$filename` is required, so a bare `new CURLFile()` is an arity
/// error — unlike `CurlHandle`/`CurlMultiHandle`/`CurlShareHandle`, `CURLFile` is an
/// ORDINARY, user-constructible class, so this is a plain constructor-arity diagnostic,
/// not a "private constructor" one.
#[test]
fn curlfile_rejects_wrong_arity() {
    expect_curl_error("<?php $f = new CURLFile();", "CURLFile");
}

/// `CURLFile`'s `$filename` is typed `string`, so passing an array (which weak-mode PHP
/// does NOT scalar-coerce, unlike an int or a float) is a compile-time type error.
#[test]
fn curlfile_rejects_a_non_string_filename() {
    expect_curl_error("<?php $f = new CURLFile([1, 2]);", "CURLFile");
}

/// `CURLStringFile`'s `$postname` (unlike `CURLFile`'s optional `$postFilename`) has NO
/// default — php-src's own constructor requires it — so a call with only `$data` is an
/// arity error.
#[test]
fn curlstringfile_rejects_missing_required_postname() {
    expect_curl_error(
        r#"<?php $f = new CURLStringFile("data only");"#,
        "CURLStringFile",
    );
}

/// `curl_file_create()`'s parameters mirror `CURLFile::__construct()`'s, so its arity
/// diagnostic is not a `new CURLFile(...)`-only accident.
#[test]
fn curl_file_create_rejects_wrong_arity() {
    expect_curl_error("<?php $f = curl_file_create();", "curl_file_create");
}

/// Runs the frontend with an explicit PHP compatibility version, so the version-gated
/// halves of the curl prelude can be checked from here. Mirrors `check_source`, which
/// always uses the default (8.5) profile.
fn check_source_for_php_version(
    src: &str,
    php_version: elephc::php_version::PhpVersion,
) -> Result<(), String> {
    let tokens = tokenize(src).map_err(|e| e.message.clone())?;
    let ast = parse(&tokens).map_err(|e| e.message.clone())?;
    let ast = elephc::autoload::collect_aliases(ast);
    let ast = elephc::hash_prelude::inject_if_used(ast, false);
    let ast = elephc::curl_prelude::inject_if_used_for_version(ast, false, php_version);
    let ast = elephc::name_resolver::resolve(ast).map_err(|e| e.message.clone())?;
    let ast = elephc::func_args::desugar(ast).map_err(|e| e.message.clone())?;
    let ast = elephc::optimize::fold_constants(ast);
    types::check(&ast).map_err(|e| e.message.clone())?;
    Ok(())
}
