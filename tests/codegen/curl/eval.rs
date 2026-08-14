//! Purpose:
//! End-to-end fixtures proving `eval()` and compiled PHP call the SAME `elephc_curl_*`
//! ABI: `curl_version()` inside
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
//! - This is the first codegen test that actually links BOTH `elephc_magician`
//!   built `--features curl` AND `elephc_curl` together, automating what was originally
//!   verified by hand.

use super::http_fixture::LocalHttpServer;
use crate::support::*;

/// `eval('return curl_version();')['version']` equals the AOT `curl_version()['version']`,
/// run through the real compiled pipeline rather
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

/// ITEM 6 (WP-B, curl punch list): `curl_multi_*`/`curl_share_*`/`curl_file_create` are
/// deliberately unimplemented in `eval()` (this family's module doc, "Scope shipped vs.
/// deferred"). Before the fix, an unrecognized-by-eval name FELL THROUGH to
/// `context.native_function()`, which — whenever the host program also links
/// `elephc_curl`, exactly the condition this test's top-level `curl_version()` call
/// creates — resolved to the REAL AOT prelude function and silently handed back a working
/// `CurlMultiHandle`/`CurlShareHandle`/`CURLFile` object instead of failing. The fix
/// intercepts these names and rejects them with eval's own honest "unsupported construct"
/// fatal (the same one any other undefined-in-eval name already produces) — an
/// UNCATCHABLE process exit, not a PHP-catchable exception, so this asserts the process
/// exit code and stderr message rather than wrapping in try/catch.
#[test]
fn eval_curl_multi_share_and_file_create_are_rejected_not_working_aot_objects() {
    if skip_without_curl_native(
        "eval_curl_multi_share_and_file_create_are_rejected_not_working_aot_objects",
    ) {
        return;
    }
    let cases = [
        ("curl_multi_init();", "curl_multi_init"),
        ("curl_share_init();", "curl_share_init"),
        (r#"curl_file_create("/etc/hosts");"#, "curl_file_create"),
    ];
    for (call, label) in cases {
        let output = compile_and_run_capture(&format!(
            r#"<?php
            curl_version();
            $r = eval('$h = {call} return get_class($h);');
            echo "unexpectedly reached: ", $r;
            "#
        ));
        assert!(
            !output.success,
            "{label}(): eval() must fail instead of returning a working AOT object; stdout was: {}",
            output.stdout
        );
        assert!(
            output
                .stderr
                .contains("eval() fragment uses an unsupported construct"),
            "{label}(): stderr was: {}",
            output.stderr
        );
        assert!(
            output.stdout.is_empty(),
            "{label}(): must never reach the echo after eval(); stdout was: {}",
            output.stdout
        );
    }
}

/// The class-construction half of item 6: `new CURLFile(...)`/`new CURLStringFile(...)`
/// inside `eval()` used to construct a real AOT object the same way the function names
/// above did (verified before this fix: `get_class($f) === "CURLFile"`), through
/// `values.new_object()`'s own native-class fallback. Same fatal, same reasoning.
#[test]
fn eval_new_curlfile_is_rejected_not_a_working_aot_object() {
    if skip_without_curl_native("eval_new_curlfile_is_rejected_not_a_working_aot_object") {
        return;
    }
    let output = compile_and_run_capture(
        r#"<?php
        curl_version();
        $r = eval('$f = new CURLFile("/etc/hosts"); return get_class($f);');
        echo "unexpectedly reached: ", $r;
        "#,
    );
    assert!(
        !output.success,
        "new CURLFile(...) must fail instead of returning a working AOT object; stdout was: {}",
        output.stdout
    );
    assert!(
        output
            .stderr
            .contains("eval() fragment uses an unsupported construct"),
        "stderr was: {}",
        output.stderr
    );
}

/// Review follow-up to item 6: the direct-literal-name interception
/// (`eval_curl_multi_share_and_file_create_are_rejected_not_working_aot_objects` above)
/// does not cover every path that can resolve a "Named" callable to
/// `context.native_function()`. `call_user_func('curl_multi_init')` resolves through
/// `registry::callable::object_dispatch::eval_named_callable_with_call_user_func_values`'s
/// OWN native-function fallback — a completely separate call site from `eval_call`'s — so
/// it used to bypass the guard entirely and return a real, working `CurlMultiHandle`
/// object. Same fix (the same `eval_curl_deferred_function_name` check, added to that
/// fallback too), same fatal.
#[test]
fn eval_call_user_func_of_a_deferred_curl_name_is_rejected() {
    if skip_without_curl_native("eval_call_user_func_of_a_deferred_curl_name_is_rejected") {
        return;
    }
    let output = compile_and_run_capture(
        r#"<?php
        curl_version();
        $r = eval('$h = call_user_func("curl_multi_init"); return get_class($h);');
        echo "unexpectedly reached: ", $r;
        "#,
    );
    assert!(
        !output.success,
        "call_user_func('curl_multi_init') must fail instead of returning a working AOT \
        object; stdout was: {}",
        output.stdout
    );
    assert!(
        output
            .stderr
            .contains("eval() fragment uses an unsupported construct"),
        "stderr was: {}",
        output.stderr
    );
}

/// Review follow-up to item 6, the variable-function shape: `$f = 'curl_multi_init'; $f();`
/// resolves through `registry::callable::array_dispatch::eval_callable_with_call_array_args`
/// (via `expressions::calls::eval_dynamic_call`) — yet ANOTHER separate native-function
/// fallback from both `eval_call`'s literal-name dispatch and `call_user_func`'s own path
/// above. Same fix, same fatal.
#[test]
fn eval_variable_function_call_of_a_deferred_curl_name_is_rejected() {
    if skip_without_curl_native("eval_variable_function_call_of_a_deferred_curl_name_is_rejected")
    {
        return;
    }
    let output = compile_and_run_capture(
        r#"<?php
        curl_version();
        $r = eval('$f = "curl_share_init"; $h = $f(); return get_class($h);');
        echo "unexpectedly reached: ", $r;
        "#,
    );
    assert!(
        !output.success,
        "$f = 'curl_share_init'; $f(); must fail instead of returning a working AOT object; \
        stdout was: {}",
        output.stdout
    );
    assert!(
        output
            .stderr
            .contains("eval() fragment uses an unsupported construct"),
        "stderr was: {}",
        output.stderr
    );
}

/// ITEMS 8 & 9 (WP-B, curl punch list): every curl_*() function that takes a `$handle`
/// must throw a catchable `\TypeError` for a non-`CurlHandle` value, matching real PHP
/// 8.4.20's own wording (verified against the real interpreter: `curl_close("x")` ->
/// `TypeError: curl_close(): Argument #1 ($handle) must be of type CurlHandle, string
/// given`) — `curl_close()` used to accept literally anything with no check at all, and
/// `curl_escape()`/`curl_unescape()` hard-faulted (an uncatchable process abort) instead of
/// throwing. This proves catchability end to end: the exception is caught by ordinary PHP
/// `try`/`catch` running inside `eval()`, and the script keeps running afterward.
#[test]
fn eval_curl_handle_functions_throw_a_catchable_type_error_for_a_non_handle_value() {
    if skip_without_curl_native(
        "eval_curl_handle_functions_throw_a_catchable_type_error_for_a_non_handle_value",
    ) {
        return;
    }
    let output = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            $out = [];
            try {
                curl_close("not a handle");
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            try {
                curl_escape(42, "a b");
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            try {
                curl_unescape(null, "a%20b");
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            $out[] = "alive";
            return implode("|", $out);
        ');
        echo $r;
        "#,
    );
    // The "given" type name is AOT's OWN `gettype()`-based wording (`crate::curl_prelude`'s
    // `$given` ternary in, e.g., `curl_multi_add_handle`), not real php-src's newer
    // "int"/"bool"/"float" short names — verified against the real AOT binary
    // (`curl_multi_add_handle($mh, 42)` -> "...must be of type CurlHandle, integer
    // given"). This is a pre-existing AOT/php-src divergence this test intentionally
    // mirrors rather than papers over: the goal is eval-matches-AOT, not eval-matches-real-
    // php for a wording AOT itself does not use.
    assert_eq!(
        output,
        "curl_close(): Argument #1 ($handle) must be of type CurlHandle, string given\
        |curl_escape(): Argument #1 ($handle) must be of type CurlHandle, integer given\
        |curl_unescape(): Argument #1 ($handle) must be of type CurlHandle, null given\
        |alive"
    );
}

/// ITEM 10 (WP-B, curl punch list): `curl_setopt()`'s catchable-exception paths — an
/// unrecognized option number, `CURLOPT_SAFE_UPLOAD` set falsy, and a non-scalar `$value`
/// for an ordinary option — used to be `RuntimeFatal` (an uncatchable process abort) in
/// `eval()` while AOT throws a catchable `\ValueError`/`\TypeError`
/// (`crate::curl_prelude::curl_setopt`'s own guards, mirrored verbatim here). Proven end to
/// end with `try`/`catch` running inside `eval()`, script alive afterward.
#[test]
fn eval_curl_setopt_throws_catchable_errors_for_invalid_option_and_non_scalar_value() {
    if skip_without_curl_native(
        "eval_curl_setopt_throws_catchable_errors_for_invalid_option_and_non_scalar_value",
    ) {
        return;
    }
    let output = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            $ch = curl_init();
            $out = [];
            try {
                curl_setopt($ch, 999999999, "x");
            } catch (\ValueError $e) {
                $out[] = get_class($e) . ":" . $e->getMessage();
            }
            try {
                curl_setopt($ch, CURLOPT_SAFE_UPLOAD, false);
            } catch (\ValueError $e) {
                $out[] = get_class($e) . ":" . $e->getMessage();
            }
            try {
                curl_setopt($ch, CURLOPT_URL, ["not", "scalar"]);
            } catch (\TypeError $e) {
                $out[] = get_class($e) . ":" . $e->getMessage();
            }
            $out[] = "alive";
            return implode("|", $out);
        ');
        echo $r;
        "#,
    );
    assert_eq!(
        output,
        "ValueError:curl_setopt(): Argument #2 ($option) is not a valid cURL option\
        |ValueError:curl_setopt(): Disabling safe uploads is no longer supported\
        |TypeError:curl_setopt(): Argument #3 ($value) must be of type string|int|float|bool, array given\
        |alive"
    );
}

/// Review follow-up to item 10: `999999999` above still fits in `i32` (max ~2.1 billion),
/// so it never exercised the actual bug. `curl_setopt()` used to narrow `$option` to `i32`
/// BEFORE classifying it — `i32::try_from(2**40)` fails — so an option number outside
/// `i32` range hard-faulted (uncatchable) instead of hitting the SAME "not a valid cURL
/// option" `ValueError` an in-range-but-unrecognized number gets, even though AOT
/// classifies against the full `i64` `$option` first and throws the identical catchable
/// error for this shape too. Fixed by classifying before narrowing (`handle.rs`'s
/// `eval_curl_setopt_apply`).
#[test]
fn eval_curl_setopt_option_number_outside_i32_throws_catchably() {
    if skip_without_curl_native("eval_curl_setopt_option_number_outside_i32_throws_catchably") {
        return;
    }
    let output = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            $ch = curl_init();
            try {
                curl_setopt($ch, 2 ** 40, 1);
                return "no exception";
            } catch (\ValueError $e) {
                return get_class($e) . ":" . $e->getMessage() . ":alive";
            }
        ');
        echo $r;
        "#,
    );
    assert_eq!(
        output,
        "ValueError:curl_setopt(): Argument #2 ($option) is not a valid cURL option:alive"
    );
}

/// ITEM 11 (WP-B, curl punch list): a string-list option (`CURLOPT_HTTPHEADER`) rejects a
/// non-array `$value` and rejects an array containing a non-scalar item with a catchable
/// `\TypeError`, matching `crate::curl_prelude::curl_setopt`'s own two guards verbatim —
/// eval used to answer `false` for the first case and silently `(string)`-cast the second.
#[test]
fn eval_curl_setopt_slist_option_throws_catchable_type_errors_for_non_scalar_items() {
    if skip_without_curl_native(
        "eval_curl_setopt_slist_option_throws_catchable_type_errors_for_non_scalar_items",
    ) {
        return;
    }
    let output = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            $ch = curl_init();
            $out = [];
            try {
                curl_setopt($ch, CURLOPT_HTTPHEADER, "not-an-array");
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            try {
                curl_setopt($ch, CURLOPT_HTTPHEADER, ["X-Ok: 1", ["nested-array"]]);
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            $out[] = curl_setopt($ch, CURLOPT_HTTPHEADER, ["X-Ok: 1", 42, 3.5, true]) ? "scalars-ok" : "scalars-failed";
            return implode("|", $out);
        ');
        echo $r;
        "#,
    );
    assert_eq!(
        output,
        "curl_setopt(): Argument #3 ($value) must be of type array, string given\
        |curl_setopt(): Argument #3 ($value) must be an array of strings for this option\
        |scalars-ok"
    );
}

// ITEM 19 (WP-B, curl punch list) end-to-end coverage note: `EvalStreamResources::drop`
// cannot release a retained `CURLOPT_PRIVATE` value (`Drop::drop` receives no
// `RuntimeValueOps`), so the release now happens one step earlier, in
// `crate::ffi::context::__elephc_eval_context_free` — see that function's own doc and
// `crate::stream_resources::curl::EvalStreamResources::release_curl_easy_private_values`'s
// for the full mechanism. An end-to-end `--gc-stats` fixture was attempted here but
// dropped: `curl_init()`'s own eval-owned handle cell (resource kind 5, "no destructor
// runs" by design, same as `hash_init()`'s `HashContext`) and other curl-bridge one-time
// overhead dominate the process-wide alloc/free counts enough that a single retained
// string cell is not a reliable signal above that noise (measured: identical imbalances
// with the fix present, with the fix reverted, and with an explicit `curl_reset()` that
// independently exercises the SAME already-tested release path). The precise, deterministic
// regression coverage lives in `crate::interpreter::tests::builtins_curl`'s
// `release_curl_easy_private_values_releases_every_still_retained_entry` and
// `..._skips_handles_with_no_stored_private_value` (magician unit tests, `--features curl`),
// which exercise the storage-layer method directly with no such confound.
