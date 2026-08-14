//! Purpose:
//! End-to-end fixtures proving `eval()` and compiled PHP call the SAME `elephc_curl_*`
//! ABI (Task 13, php-curl-family plan, Step 1's TDD acceptance): `curl_version()` inside
//! `eval()` matches the AOT builtin, and a full `curl_init`/`curl_setopt`/`curl_exec`
//! request against the local fixture works from inside `eval()`.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - Every fixture here ALSO calls a real (non-`eval()`) curl function at the top level.
//!   That is not incidental: curl detection (`src/curl_prelude/detect.rs`) walks the
//!   parsed AST and cannot see inside an `eval()` string literal, so a program that only
//!   calls `curl_*` from within `eval()` would never inject the prelude or link
//!   `elephc_curl` — the eval curl homes would exist but have no bridge to call into. A
//!   real top-level `curl_version()`/`curl_init()` call is what makes THIS SAME program
//!   also need `elephc_magician` (it calls `eval()`) and `elephc_curl` (it calls a real
//!   curl builtin) together, which is exactly the combination
//!   `tests/codegen/support/runner.rs`'s `ensure_magician_curl_staticlib`/
//!   `magician_curl_aware_plan` build and link the curl-aware magician archive for.
//! - Mirrors the manual end-to-end verification recorded in the Task 13 report,
//!   automated: this is the first codegen test that actually links BOTH `elephc_magician`
//!   built `--features curl` AND `elephc_curl` together.

use super::http_fixture::LocalHttpServer;
use crate::support::*;

/// `eval('return curl_version();')['version']` equals the AOT `curl_version()['version']`
/// — Task 13's Step 1 acceptance criterion, run through the real compiled pipeline rather
/// than verified by hand.
#[test]
fn eval_curl_version_matches_aot_curl_version() {
    if skip_without_curl_native("eval_curl_version_matches_aot_curl_version") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $aot = curl_version();
        $viaEval = eval('return curl_version();');
        echo ($aot['version'] === $viaEval['version']) ? "match" : "mismatch";
        echo ":", $viaEval['version'];
        // THE KEY ORDER MUST SURVIVE THE EVAL DECODE PATH TOO. Both sides decode the same
        // bridge JSON, but through different `json_decode` implementations (compiled
        // runtime vs interpreter), and a PHP array is ordered — so this compares the
        // orders, not just the key sets, for the outer array and for `feature_list`.
        echo ":", implode(",", array_keys($aot)) === implode(",", array_keys($viaEval)) ? "same-order" : "reordered";
        echo ":", implode(",", array_keys($aot['feature_list'])) === implode(",", array_keys($viaEval['feature_list'])) ? "same-features" : "reordered-features";
        $names = array_keys($viaEval['feature_list']);
        echo ":", $names[0];
        "#,
    );
    assert_eq!(out, "match:8.21.0:same-order:same-features:AsynchDNS");
}

/// A full `curl_init()` + `curl_setopt(CURLOPT_URL, CURLOPT_RETURNTRANSFER)` +
/// `curl_exec()` GET against the loopback fixture, entirely from inside `eval()`: proves
/// the numeric `CURLOPT_*` constants resolve correctly in eval, the handle created inside
/// `eval()` round-trips through `curl_setopt`/`curl_exec` on the SAME table entry, and the
/// transferred body matches what the real ABI call would produce.
#[test]
fn eval_curl_init_setopt_exec_get_against_local_fixture() {
    if skip_without_curl_native("eval_curl_init_setopt_exec_get_against_local_fixture") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        // A real top-level curl call, so this program links `elephc_curl` too — see this
        // file's module doc for why that is required for the eval calls below to have a
        // bridge to call into at all.
        curl_version();
        $body = eval('
            $ch = curl_init();
            curl_setopt($ch, CURLOPT_URL, "{url}");
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            $result = curl_exec($ch);
            $code = curl_getinfo($ch, CURLINFO_HTTP_CODE);
            return [$result, $code];
        ');
        echo $body[0], ":", $body[1];
        "#
    ));
    assert_eq!(out, "hello-curl:200");
}

/// `extension_loaded('curl')` agrees between AOT and `eval()` for a program that needs
/// curl either way — the module doc's "AOT and eval can never disagree" claim, exercised
/// end to end rather than only unit-tested against `cfg!(feature = "curl")` directly.
#[test]
fn eval_extension_loaded_curl_matches_aot() {
    if skip_without_curl_native("eval_extension_loaded_curl_matches_aot") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        curl_version();
        $aot = extension_loaded('curl') ? "1" : "0";
        $viaEval = eval("return extension_loaded('curl') ? '1' : '0';");
        echo $aot, $viaEval;
        "#,
    );
    assert_eq!(out, "11");
}

/// A curl-free eval program must keep compiling without requiring native curl at all —
/// the pay-for-use half of the same guarantee: this test intentionally does NOT call
/// `skip_without_curl_native`, because it must pass on a machine with no native curl
/// package installed at all.
#[test]
fn eval_without_curl_never_requires_the_curl_bridge() {
    let out = compile_and_run(
        r#"<?php
        $r = eval('return 1 + 41;');
        echo $r, ":", extension_loaded('curl') ? "1" : "0";
        "#,
    );
    assert_eq!(out, "42:0");
}
