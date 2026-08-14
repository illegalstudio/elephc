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

/// THE FOUR PHP-STREAM OPTIONS STAY UNSUPPORTED IN `eval()`, and — the part worth pinning —
/// they stay unsupported the SAFE way: `false` plus the honest warning, not a fatal.
///
/// They used to be `KIND_UNSUPPORTED`, which the interpreter already funnelled into that
/// warning. Giving them their own `KIND_STREAM` for the AOT implementation moved them out
/// of that arm, and anything the interpreter does not recognize falls through to its
/// scalar-type guard — where a stream resource is none of int/string/float/bool and the
/// answer is an UNCATCHABLE fatal. `crates/elephc-magician/src/interpreter/builtins/curl/
/// handle.rs` names `KIND_STREAM` alongside `KIND_SHARE`/`KIND_CALLBACK` to prevent that,
/// and this fixture is what would notice if it stopped doing so.
///
/// The AOT half of the same program shows the divergence in the other direction: compiled
/// code writes the body to the stream, `eval()` refuses the option.
#[test]
fn eval_rejects_the_stream_options_with_a_warning_not_a_fatal() {
    if skip_without_curl_native("eval_rejects_the_stream_options_with_a_warning_not_a_fatal") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let output = compile_and_run_capture(&format!(
        r#"<?php
        // A real top-level curl call, so the prelude is injected and the bridge linked
        // (see this file's header).
        $path = tempnam(sys_get_temp_dir(), "elephc-curl-eval");
        $sink = fopen($path, "w+b");
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_FILE, $sink);
        curl_exec($ch);
        curl_close($ch);
        fclose($sink);
        echo "aot=", file_get_contents($path), "\n";
        unlink($path);

        // The same four options inside eval(): each answers false, none is fatal, and the
        // script keeps running to print the marker below.
        $results = eval('
            $path = tempnam(sys_get_temp_dir(), "elephc-curl-eval2");
            $sink = fopen($path, "w+b");
            $ch = curl_init();
            $out = "";
            foreach ([CURLOPT_FILE, CURLOPT_WRITEHEADER, CURLOPT_INFILE, CURLOPT_STDERR] as $option) {{
                $out .= curl_setopt($ch, $option, $sink) ? "t" : "f";
            }}
            curl_close($ch);
            fclose($sink);
            unlink($path);
            return $out;
        ');
        echo "eval=", $results, "\n";
        echo "alive\n";
        "#
    ));
    assert_eq!(output.stdout, "aot=hello-curl\neval=ffff\nalive\n");
    // The MESSAGE is the AOT one verbatim. The `Warning: ` PREFIX is not: the interpreter
    // emits through its own generic warning channel, which does not prepend the label (or
    // a newline) the compiled `__elephc_curl_setopt_unsupported_warning` does. That is a
    // pre-existing eval-vs-AOT formatting difference across every eval warning, not
    // something these four options introduce, so it is asserted as-is rather than
    // papered over.
    for option in ["10001", "10029", "10009", "10037"] {
        assert!(
            output.stderr.contains(&format!(
                "curl_setopt(): Option {option} is not supported by this build"
            )),
            "each stream option must warn in eval(); stderr was: {}",
            output.stderr
        );
    }
}
