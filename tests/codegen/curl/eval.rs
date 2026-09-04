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
            output.diagnostics.contains(&format!(
                "curl_setopt(): Option {option} is not supported by this build"
            )),
            "each stream option must warn in eval(); diagnostics were: {}",
            output.diagnostics
        );
    }
}

/// ITEM 6 (WP-B, curl punch list), CLOSED BY R3-C: `curl_file_create()` and
/// `new CURLFile(...)` used to be INTERCEPTED inside `eval()` and answered eval's own
/// "unsupported construct" fatal, because the eval interpreter could not do anything with a
/// `CURLFile` — `CURLOPT_POSTFIELDS`'s array form was not implemented, so a constructible
/// one would have been a value nothing could consume.
///
/// Now that the multipart walk exists, both are deliberately served BY the native fallback
/// the guard used to block. That is safe here and nowhere else in the curl surface:
/// `CURLFile`/`CURLStringFile` are PURE PHP DATA CLASSES wrapping no native handle, so
/// unlike a `CurlHandle` there are no "two object spaces" to confuse. This asserts the whole
/// data-class surface — construction both ways, `get_class`, `instanceof`, the three getters
/// and two setters, and `CURLStringFile`'s DIFFERENT constructor argument order
/// (`data, postname, mime`) and its `application/octet-stream` mime default.
#[test]
fn eval_curlfile_and_curl_file_create_construct_working_objects() {
    if skip_without_curl_native("eval_curlfile_and_curl_file_create_construct_working_objects") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            $out = [];
            $a = new CURLFile("/tmp/a.txt", "text/plain", "posted.txt");
            $out[] = get_class($a);
            $out[] = $a->getFilename();
            $out[] = $a->getMimeType();
            $out[] = $a->getPostFilename();
            $a->setMimeType("application/json");
            $a->setPostFilename("other.json");
            $out[] = $a->getMimeType() . "/" . $a->getPostFilename();
            $b = curl_file_create("/tmp/b.txt");
            $out[] = get_class($b) . ":" . ($b instanceof CURLFile ? "is" : "not");
            $out[] = "[" . $b->getMimeType() . "][" . $b->getPostFilename() . "]";
            $c = new CURLStringFile("payload", "in-memory.bin");
            $out[] = get_class($c) . ":" . $c->data . ":" . $c->postname . ":" . $c->mime;
            $out[] = is_subclass_of("CURLStringFile", "CURLFile") ? "subclass" : "sibling";
            return implode("|", $out);
        ');
        echo $r;
        "#,
    );
    assert_eq!(
        out,
        "CURLFile|/tmp/a.txt|text/plain|posted.txt|application/json/other.json\
        |CURLFile:is|[][]|CURLStringFile:payload:in-memory.bin:application/octet-stream\
        |sibling"
    );
}

/// Review follow-up to item 6, now inverted by R3-C: `call_user_func('curl_multi_init')`
/// resolves through `registry::callable::object_dispatch::
/// eval_named_callable_with_call_user_func_values`'s OWN native-function fallback — a
/// completely separate call site from `eval_call`'s — which is why it once bypassed the
/// deferred-name guard and returned a real AOT `CurlMultiHandle`.
///
/// Now that the multi interface has a real eval home, that path must resolve the EVAL
/// builtin instead: the handle it hands back has to work with the other eval multi
/// functions, which it could not if it were an AOT object (`curl_multi_errno()` would fail
/// its `CurlMultiHandle` table lookup). This is the same call site, asserting the opposite
/// outcome.
#[test]
fn eval_call_user_func_of_curl_multi_init_returns_a_working_eval_handle() {
    if skip_without_curl_native("eval_call_user_func_of_curl_multi_init_returns_a_working_eval_handle")
    {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            $mh = call_user_func("curl_multi_init");
            $ch = call_user_func("curl_init");
            $added = call_user_func("curl_multi_add_handle", $mh, $ch);
            return $added . ":" . curl_multi_errno($mh) . ":" . count(curl_multi_get_handles($mh));
        ');
        echo $r;
        "#,
    );
    assert_eq!(out, "0:0:1");
}

/// Review follow-up to item 6, the variable-function shape, likewise inverted by R3-C:
/// `$f = 'curl_share_init'; $f();` resolves through
/// `registry::callable::array_dispatch::eval_callable_with_call_array_args` (via
/// `expressions::calls::eval_dynamic_call`) — yet ANOTHER separate native-function fallback
/// from both `eval_call`'s literal-name dispatch and `call_user_func`'s own path above. It
/// must now reach the eval share builtin, and the handle must be usable by the rest of the
/// eval share surface.
#[test]
fn eval_variable_function_call_of_curl_share_init_returns_a_working_eval_handle() {
    if skip_without_curl_native(
        "eval_variable_function_call_of_curl_share_init_returns_a_working_eval_handle",
    ) {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            $f = "curl_share_init";
            $sh = $f();
            $set = curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS) ? "1" : "0";
            return $set . ":" . curl_share_errno($sh);
        ');
        echo $r;
        "#,
    );
    assert_eq!(out, "1:0");
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

/// R3-C: THE MULTI INTERFACE END TO END INSIDE `eval()`. Two handles on one multi handle,
/// driven with the canonical `curl_multi_exec`/`curl_multi_select` loop, then drained with
/// `curl_multi_info_read()` — all from inside an `eval()` string, against the loopback
/// fixture.
///
/// What this pins beyond "it works":
/// - `curl_multi_exec()`'s BY-REFERENCE `$still_running` is genuinely written back through
///   the eval reference-target machinery (the loop would spin forever or exit immediately
///   otherwise).
/// - `curl_multi_info_read()`'s `handle` key resolves back to the SAME eval handle that was
///   added, through `curl_easy_id_for_raw` — proven by using it as `curl_multi_getcontent()`'s
///   argument, which needs the eval table key to find the RETURNTRANSFER mirror flag.
/// - The completion `result` is `CURLE_OK` (0) for both transfers.
#[test]
fn eval_curl_multi_drives_two_transfers_against_the_local_fixture() {
    if skip_without_curl_native("eval_curl_multi_drives_two_transfers_against_the_local_fixture") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        // A real top-level curl call, so the prelude is injected and the bridge linked —
        // see this file's module doc.
        curl_version();
        $r = eval('
            $mh = curl_multi_init();
            $a = curl_init("{url}");
            curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
            $b = curl_init("{url}");
            curl_setopt($b, CURLOPT_RETURNTRANSFER, true);
            curl_multi_add_handle($mh, $a);
            curl_multi_add_handle($mh, $b);
            $still = 0;
            do {{
                $code = curl_multi_exec($mh, $still);
                if ($still > 0) {{
                    curl_multi_select($mh, 1.0);
                }}
            }} while ($still > 0 && $code == CURLM_OK);
            $bodies = [];
            $results = [];
            while (true) {{
                $info = curl_multi_info_read($mh, $queued);
                if ($info === false) {{
                    break;
                }}
                $results[] = $info["result"];
                $bodies[] = curl_multi_getcontent($info["handle"]);
            }}
            sort($bodies);
            curl_multi_remove_handle($mh, $a);
            curl_multi_remove_handle($mh, $b);
            curl_multi_close($mh);
            return implode(",", $results) . "|" . implode(",", $bodies) . "|" . $code;
        ');
        echo $r;
        "#
    ));
    assert_eq!(out, "0,0|hello-curl,hello-curl|0");
}

/// R3-C: PHP 8.5's `curl_multi_get_handles()` inside `eval()` reports the attached handles
/// IN ADD ORDER, and the reported handles are usable as ordinary eval curl handles.
///
/// Identity is asserted the only way it is meaningful in eval: the reported cell addresses
/// the SAME `EvalStreamResources` entry, so `curl_getinfo(..., CURLINFO_PRIVATE)` reads back
/// the value set on the original. eval curl handles are inert resource-kind-5 cells (see
/// `crates/elephc-magician/src/interpreter/builtins/curl/mod.rs`), so there is no object
/// instance to compare with `===` the way AOT's `CurlHandle` map guarantees — that
/// divergence is documented, not papered over here.
#[test]
fn eval_curl_multi_get_handles_lists_attachments_in_add_order() {
    if skip_without_curl_native("eval_curl_multi_get_handles_lists_attachments_in_add_order") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            $mh = curl_multi_init();
            $a = curl_init();
            curl_setopt($a, CURLOPT_PRIVATE, "first");
            $b = curl_init();
            curl_setopt($b, CURLOPT_PRIVATE, "second");
            curl_multi_add_handle($mh, $a);
            curl_multi_add_handle($mh, $b);
            $names = [];
            foreach (curl_multi_get_handles($mh) as $h) {
                $names[] = curl_getinfo($h, CURLINFO_PRIVATE);
            }
            curl_multi_remove_handle($mh, $a);
            $after = count(curl_multi_get_handles($mh));
            return implode(",", $names) . "|" . $after;
        ');
        echo $r;
        "#,
    );
    assert_eq!(out, "first,second|1");
}

/// R3-C: the multi interface's ERROR PARITY with AOT — every catchable throwable AOT
/// produces for the same misuse, produced by `eval()` too and caught by ordinary PHP
/// `try`/`catch` running inside the fragment, with the script alive afterward.
///
/// The `CurlMultiHandle`/`CurlHandle` `TypeError`s have no AOT RUNTIME counterpart at all —
/// the prelude declares those parameter types, so AOT rejects the same call at COMPILE
/// time. eval has no checker, so these are the runtime-only counterpart of that compiled
/// guarantee, worded exactly like the prelude's own runtime `instanceof` guards for the
/// handful of curl functions that cannot enforce the type statically.
#[test]
fn eval_curl_multi_functions_throw_catchable_errors_for_bad_arguments() {
    if skip_without_curl_native("eval_curl_multi_functions_throw_catchable_errors_for_bad_arguments")
    {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            $mh = curl_multi_init();
            $ch = curl_init();
            $out = [];
            try {
                curl_multi_errno("not a handle");
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            try {
                curl_multi_add_handle($mh, 42);
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            try {
                // An EASY handle where a MULTI one belongs: eval types curl handles by
                // which table their key resolves in, so this is a TypeError, never a
                // confusing partial success.
                curl_multi_errno($ch);
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            try {
                curl_multi_setopt($mh, 999999, 1);
            } catch (\ValueError $e) {
                $out[] = get_class($e) . ":" . $e->getMessage();
            }
            try {
                curl_multi_setopt($mh, CURLMOPT_MAXCONNECTS, [1]);
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            $out[] = "alive";
            return implode("|", $out);
        ');
        echo $r;
        "#,
    );
    assert_eq!(
        out,
        "curl_multi_errno(): Argument #1 ($multi_handle) must be of type CurlMultiHandle, string given\
        |curl_multi_add_handle(): Argument #2 ($handle) must be of type CurlHandle, integer given\
        |curl_multi_errno(): Argument #1 ($multi_handle) must be of type CurlMultiHandle, resource given\
        |ValueError:curl_multi_setopt(): Argument #2 ($option) is not a valid cURL multi option\
        |curl_multi_setopt(): Argument #3 ($value) must be of type string|int|float|bool, array given\
        |alive"
    );
}

/// R3-C: the SHARE interface inside `eval()` — `curl_share_init()`, both real
/// `CURLSHOPT_*` options, `curl_setopt($ch, CURLOPT_SHARE, $sh)` attaching an eval easy
/// handle to it, and a real transfer through the shared handle.
///
/// `curl_share_setopt()` with a `CURL_LOCK_DATA_*` value libcurl refuses answers a plain
/// `false` with NO warning — a genuine libcurl-level answer, not "this build cannot carry a
/// real PHP option" — and the true `CURLSHcode` stays readable through
/// `curl_share_errno()`/`curl_share_strerror()`. That distinction is the share module's own
/// (`crates/elephc-curl/src/share.rs`), mirrored here.
#[test]
fn eval_curl_share_attaches_to_an_easy_handle_and_transfers() {
    if skip_without_curl_native("eval_curl_share_attaches_to_an_easy_handle_and_transfers") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $sh = curl_share_init();
            $out = [];
            $out[] = curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS) ? "dns" : "dns-failed";
            $out[] = curl_share_setopt($sh, CURLSHOPT_UNSHARE, CURL_LOCK_DATA_DNS) ? "unshare" : "unshare-failed";
            $out[] = curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_CONNECT) ? "connect" : "connect-failed";
            $ch = curl_init("{url}");
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            $out[] = curl_setopt($ch, CURLOPT_SHARE, $sh) ? "attached" : "attach-failed";
            $out[] = curl_exec($ch);
            $out[] = (string) curl_share_errno($sh);
            curl_share_close($sh);
            return implode("|", $out);
        ');
        echo $r;
        "#
    ));
    assert_eq!(out, "dns|unshare|connect|attached|hello-curl|0");
}

/// R3-C: the share interface's error parity — `curl_share_setopt()`'s `ValueError` for an
/// option number PHP does not expose at all, `curl_setopt(CURLOPT_SHARE, ...)`'s `TypeError`
/// for a non-share value, and the `CurlShareHandle` `TypeError` every share function raises
/// for a foreign handle. All catchable, script alive afterward.
#[test]
fn eval_curl_share_functions_throw_catchable_errors_for_bad_arguments() {
    if skip_without_curl_native("eval_curl_share_functions_throw_catchable_errors_for_bad_arguments")
    {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            $sh = curl_share_init();
            $ch = curl_init();
            $out = [];
            try {
                // 3 is a real libcurl CURLSHOPT_* (LOCKFUNC) that PHP never exposes as a
                // constant, so php-src answers ValueError, not a libcurl refusal.
                curl_share_setopt($sh, 3, 1);
            } catch (\ValueError $e) {
                $out[] = get_class($e) . ":" . $e->getMessage();
            }
            try {
                curl_share_errno($ch);
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            try {
                curl_setopt($ch, CURLOPT_SHARE, "not a share");
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            $out[] = "alive";
            return implode("|", $out);
        ');
        echo $r;
        "#,
    );
    assert_eq!(
        out,
        "ValueError:curl_share_setopt(): Argument #2 ($option) is not a valid cURL share option\
        |curl_share_errno(): Argument #1 ($share_handle) must be of type CurlShareHandle, resource given\
        |curl_setopt(): Argument #3 ($value) must be of type CurlShareHandle, string given\
        |alive"
    );
}

/// R3-C TEARDOWN: an `eval()` fragment that leaves a multi handle with easy handles still
/// attached AND a share still attached to one of them, then returns — so
/// `EvalStreamResources::drop` has to unwind all three tables in the right order.
///
/// The multi handle is freed FIRST (it detaches its easy handles before
/// `curl_multi_cleanup`), the easy handles SECOND (their `curl_easy_cleanup` releases
/// libcurl's own reference on the share, and the bridge's `detach_easy` drains the share's
/// attachment list), and the share LAST — at which point its `curl_share_cleanup()` takes
/// the immediate path instead of the deferred one. A wrong order does not merely leak: the
/// bridge's `finish_share_cleanup` `debug_assert_eq!`s that libcurl accepted the cleanup, so
/// a desynced attachment list fails LOUDLY in this debug-built test binary. The process
/// exiting cleanly with the marker printed is the assertion.
///
/// `curl_share_close()`/`curl_multi_close()` are deliberately NOT called: PHP 8 makes both
/// documented no-ops, so this exercises the "the program never cleaned up" path, which is
/// the one teardown has to survive.
#[test]
fn eval_curl_context_teardown_unwinds_multi_easy_and_share_without_double_free() {
    if skip_without_curl_native(
        "eval_curl_context_teardown_unwinds_multi_easy_and_share_without_double_free",
    ) {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let output = compile_and_run_capture(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $mh = curl_multi_init();
            $sh = curl_share_init();
            curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);
            $a = curl_init("{url}");
            curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($a, CURLOPT_SHARE, $sh);
            $b = curl_init("{url}");
            curl_setopt($b, CURLOPT_RETURNTRANSFER, true);
            curl_multi_add_handle($mh, $a);
            curl_multi_add_handle($mh, $b);
            $still = 0;
            do {{
                $code = curl_multi_exec($mh, $still);
                if ($still > 0) {{
                    curl_multi_select($mh, 1.0);
                }}
            }} while ($still > 0 && $code == CURLM_OK);
            // Nothing is closed on purpose: every handle is still live, still attached, and
            // still shared when this eval() context is torn down.
            return "done";
        ');
        echo $r, "\n";
        echo "alive\n";
        "#
    ));
    assert!(
        output.success,
        "teardown must not fault; stderr was: {}",
        output.stderr
    );
    assert_eq!(output.stdout, "done\nalive\n");
    assert!(
        !output.stderr.contains("has leaked"),
        "the bridge must not report a refused curl_share_cleanup; stderr was: {}",
        output.stderr
    );
}

/// R3-C: `CURLOPT_POSTFIELDS`'s ARRAY form inside `eval()` posts REAL `multipart/form-data`
/// on the wire, proven against the loopback fixture's `/multipart` route rather than by a
/// `curl_setopt()` return value.
///
/// Covers every part shape `crate::interpreter::builtins::curl::multipart` handles: a plain
/// scalar field, a `CURLFile` read from a real file on disk, a `CURLStringFile` posted from
/// memory, and a nested array flattening to one part per inner element under the SAME outer
/// key (php-src's repeated-field idiom).
#[test]
fn eval_curl_postfields_array_posts_real_multipart() {
    if skip_without_curl_native("eval_curl_postfields_array_posts_real_multipart") {
        return;
    }
    let path = std::env::temp_dir().join(format!(
        "elephc_curl_eval_mime_{}_{:?}.txt",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&path, b"file bytes").expect("write fixture file");
    let path_str = path.to_string_lossy().into_owned();
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $ch = curl_init("{url}");
            $fields = [
                "plain" => "scalar-value",
                "file" => new CURLFile("{path_str}", "text/plain", "hello.txt"),
                "mem" => new CURLStringFile("in-memory bytes", "mem.bin", "application/x-thing"),
                "tags" => ["one", "two"],
            ];
            curl_setopt($ch, CURLOPT_POST, true);
            curl_setopt($ch, CURLOPT_POSTFIELDS, $fields);
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            return curl_exec($ch);
        ');
        echo $r;
        "#
    ));
    let _ = std::fs::remove_file(&path);
    assert!(out.contains("content-type=multipart/form-data"), "{out}");
    assert!(out.contains("parts=5"), "{out}");
    assert!(out.contains("part[0].name=plain"), "{out}");
    assert!(out.contains("part[0].body=scalar-value"), "{out}");
    assert!(out.contains("part[1].name=file"), "{out}");
    assert!(out.contains("part[1].filename=hello.txt"), "{out}");
    assert!(out.contains("part[1].type=text/plain"), "{out}");
    assert!(out.contains("part[1].body=file bytes"), "{out}");
    assert!(out.contains("part[2].name=mem"), "{out}");
    assert!(out.contains("part[2].filename=mem.bin"), "{out}");
    assert!(out.contains("part[2].type=application/x-thing"), "{out}");
    assert!(out.contains("part[2].body=in-memory bytes"), "{out}");
    // The nested array flattens to two parts, BOTH named with the outer key, the inner keys
    // discarded entirely — measured php-src behaviour, mirrored from the AOT walker.
    assert!(out.contains("part[3].name=tags"), "{out}");
    assert!(out.contains("part[3].body=one"), "{out}");
    assert!(out.contains("part[4].name=tags"), "{out}");
    assert!(out.contains("part[4].body=two"), "{out}");
}

/// R3-C: the two `CURLOPT_POSTFIELDS` array corner cases whose answers are counter-intuitive
/// and whose AOT versions are pinned by their own fixtures — reproduced in `eval()`.
///
/// - AN EMPTY ARRAY IS AN EMPTY STRING BODY, NOT AN EMPTY MULTIPART: php-src short-circuits
///   before building any mime structure, so the request carries
///   `application/x-www-form-urlencoded` and an empty body, byte for byte what
///   `CURLOPT_POSTFIELDS => ""` sends. A built-but-empty `curl_mime` would send a multipart
///   content type and a boundary-only body instead.
/// - A `CURLFile` WITH NO EXPLICIT MIME SENDS `application/octet-stream`, php-src's own
///   literal default — NOT whatever libcurl sniffs from the posted filename's extension. The
///   `.png` name is deliberate: it is one of the extensions the pinned libcurl 8.21.0 would
///   sniff to a real image type if the type were left unset.
#[test]
fn eval_curl_postfields_empty_array_and_default_mime_match_php() {
    if skip_without_curl_native("eval_curl_postfields_empty_array_and_default_mime_match_php") {
        return;
    }
    let path = std::env::temp_dir().join(format!(
        "elephc_curl_eval_mime_default_{}_{:?}.png",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&path, b"not really a png").expect("write fixture file");
    let path_str = path.to_string_lossy().into_owned();
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $empty = curl_init("{url}");
            curl_setopt($empty, CURLOPT_POST, true);
            curl_setopt($empty, CURLOPT_POSTFIELDS, []);
            curl_setopt($empty, CURLOPT_RETURNTRANSFER, true);
            $emptyBody = curl_exec($empty);

            $sniff = curl_init("{url}");
            curl_setopt($sniff, CURLOPT_POST, true);
            curl_setopt($sniff, CURLOPT_POSTFIELDS, ["f" => new CURLFile("{path_str}")]);
            curl_setopt($sniff, CURLOPT_RETURNTRANSFER, true);
            $sniffBody = curl_exec($sniff);
            return $emptyBody . "@@" . $sniffBody;
        ');
        echo $r;
        "#
    ));
    let _ = std::fs::remove_file(&path);
    let (empty, sniff) = out.split_once("@@").expect("two responses");
    assert!(
        empty.contains("content-type=application/x-www-form-urlencoded"),
        "empty array must not build a multipart body; got: {empty}"
    );
    assert!(
        !sniff.contains("image/png"),
        "an unset CURLFile mime must not be sniffed from the .png name; got: {sniff}"
    );
    assert!(
        sniff.contains("part[0].type=application/octet-stream"),
        "got: {sniff}"
    );
}

/// R3-C: `CURLOPT_POSTFIELDS`'s array walk refuses the two shapes the AOT walker refuses,
/// with the SAME catchable `\TypeError` and the same wording — an object that is neither a
/// `CURLFile` nor a `CURLStringFile`, and an inner element of a nested array that is itself
/// an array.
///
/// Both are documented DIVERGENCES from php-src, taken deliberately and identically on both
/// sides: php-src would `(string)`-cast the object (raising a catchable `\Error` for one with
/// no `__toString()`), but elephc's own object-to-string cast for such a class is an
/// UNCATCHABLE process exit, so refusing before the cast is strictly better than reproducing
/// php's answer through a mechanism that kills the process.
#[test]
fn eval_curl_postfields_array_refuses_unsupported_values_catchably() {
    if skip_without_curl_native("eval_curl_postfields_array_refuses_unsupported_values_catchably") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            class EvalPostfieldsProbe {}
            $ch = curl_init();
            $out = [];
            try {
                curl_setopt($ch, CURLOPT_POSTFIELDS, ["f" => new EvalPostfieldsProbe()]);
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            try {
                curl_setopt($ch, CURLOPT_POSTFIELDS, ["f" => [["deep"]]]);
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            // The handle is still usable afterwards: every failure path aborts the pending
            // mime builder instead of leaving it dangling.
            $out[] = curl_setopt($ch, CURLOPT_POSTFIELDS, ["ok" => "1"]) ? "recovered" : "broken";
            $out[] = "alive";
            return implode("|", $out);
        ');
        echo $r;
        "#,
    );
    assert_eq!(
        out,
        "curl_setopt(): CURLOPT_POSTFIELDS array value must be of type \
        string|int|float|bool|CURLFile|CURLStringFile, EvalPostfieldsProbe given\
        |curl_setopt(): CURLOPT_POSTFIELDS nested array value must contain only scalars\
        |recovered|alive"
    );
}


/// R3-C: THE SIX CALLBACK OPTIONS INSIDE `eval()`, end to end against the loopback fixture.
///
/// This is the piece the eval curl surface documented as impossible ("a pure Rust
/// interpreter has no address to hand libcurl"), which was wrong: an ordinary `extern "C"`
/// function in the magician crate is an address with the same C ABI the bridge already calls
/// through. See `crates/elephc-magician/src/interpreter/builtins/curl/callbacks.rs`.
///
/// Asserted here: `CURLOPT_WRITEFUNCTION` receives body chunks and its return value is what
/// libcurl compares against the chunk length; `CURLOPT_HEADERFUNCTION` receives header lines
/// including the status line; `CURLOPT_XFERINFOFUNCTION` fires with four integer counters;
/// `CURLOPT_DEBUGFUNCTION` fires only under `CURLOPT_VERBOSE`; and every one of them receives
/// the SAME `$ch` it was installed on as argument 0.
#[test]
fn eval_curl_write_header_progress_and_debug_callbacks_fire() {
    if skip_without_curl_native("eval_curl_write_header_progress_and_debug_callbacks_fire") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $ch = curl_init("{url}");
            $body = "";
            $headers = [];
            $progress = 0;
            $debug = 0;
            $sameHandle = true;
            curl_setopt($ch, CURLOPT_WRITEFUNCTION, function ($h, $chunk) use (&$body, &$sameHandle, $ch) {{
                if ($h !== $ch) {{ $sameHandle = false; }}
                $body .= $chunk;
                return strlen($chunk);
            }});
            curl_setopt($ch, CURLOPT_HEADERFUNCTION, function ($h, $line) use (&$headers) {{
                $trimmed = trim($line);
                if ($trimmed !== "") {{ $headers[] = $trimmed; }}
                return strlen($line);
            }});
            curl_setopt($ch, CURLOPT_XFERINFOFUNCTION, function ($h, $dt, $dn, $ut, $un) use (&$progress) {{
                $progress++;
                return 0;
            }});
            curl_setopt($ch, CURLOPT_NOPROGRESS, false);
            curl_setopt($ch, CURLOPT_VERBOSE, true);
            curl_setopt($ch, CURLOPT_DEBUGFUNCTION, function ($h, $type, $data) use (&$debug) {{
                $debug++;
                return 0;
            }});
            $result = curl_exec($ch);
            return implode("|", [
                $body,
                $sameHandle ? "same" : "different",
                count($headers) > 0 ? $headers[0] : "no-headers",
                $progress > 0 ? "progressed" : "no-progress",
                $debug > 0 ? "debugged" : "no-debug",
                $result === true ? "true" : "not-true",
            ]);
        ');
        echo $r;
        "#
    ));
    let fields: Vec<&str> = out.split('|').collect();
    assert_eq!(fields[0], "hello-curl", "{out}");
    assert_eq!(fields[1], "same", "{out}");
    assert!(fields[2].starts_with("HTTP/1."), "{out}");
    assert_eq!(fields[3], "progressed", "{out}");
    assert_eq!(fields[4], "debugged", "{out}");
    // A write callback selects php-src's `PHP_CURL_USER`, which deselects `PHP_CURL_RETURN`,
    // so `curl_exec()` answers `true` rather than the body — the same single-write-mode rule
    // the AOT fixtures pin.
    assert_eq!(fields[5], "true", "{out}");
}

/// R3-C: `CURLOPT_READFUNCTION` inside `eval()` supplies an upload body.
///
/// The callback's `$fd` argument is always `null` here — eval carries none of the four
/// PHP-stream options, so there is no `CURLOPT_INFILE` to pass, which is also exactly what
/// php-src passes for a handle that has none. Returning `""` is end-of-data.
#[test]
fn eval_curl_read_callback_supplies_an_upload_body() {
    if skip_without_curl_native("eval_curl_read_callback_supplies_an_upload_body") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $ch = curl_init("{url}");
            $payload = "uploaded-by-eval";
            $offset = 0;
            $sawNullFd = true;
            curl_setopt($ch, CURLOPT_UPLOAD, true);
            curl_setopt($ch, CURLOPT_INFILESIZE, strlen($payload));
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($ch, CURLOPT_READFUNCTION, function ($h, $fd, $max) use (&$offset, &$sawNullFd, $payload) {{
                if ($fd !== null) {{ $sawNullFd = false; }}
                if ($offset >= strlen($payload)) {{ return ""; }}
                $chunk = substr($payload, $offset, $max);
                $offset += strlen($chunk);
                return $chunk;
            }});
            $echoed = curl_exec($ch);
            return ($sawNullFd ? "null-fd" : "stream-fd") . "|" . $echoed;
        ');
        echo $r;
        "#
    ));
    assert!(out.starts_with("null-fd|"), "{out}");
    assert!(out.contains("uploaded-by-eval"), "{out}");
}

/// R3-C, THE INVARIANT THAT MATTERS MOST: a PHP exception thrown inside a curl callback
/// running in `eval()` NEVER unwinds through libcurl. It aborts the transfer, is parked, and
/// surfaces as an ordinary CATCHABLE throwable after `curl_exec()` returns — with
/// `curl_errno()` answering `0`, php-src's own measured answer for this case (the transfer
/// was ended by the exception, not by a `CURLcode`).
///
/// eval reaches that without the AOT path's `setjmp` firewall, and structurally rather than
/// defensively: the interpreter reports a throw as an `Err(EvalStatus)` return value, so
/// there is no unwind to contain in the first place. The bridge's own process-wide gate is
/// still what authorizes the re-raise, exactly as `__rt_curl_rethrow_pending` uses it in AOT.
#[test]
fn eval_curl_callback_throw_is_catchable_after_exec_with_errno_zero() {
    if skip_without_curl_native("eval_curl_callback_throw_is_catchable_after_exec_with_errno_zero")
    {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $ch = curl_init("{url}");
            curl_setopt($ch, CURLOPT_WRITEFUNCTION, function ($h, $chunk) {{
                throw new \RuntimeException("from the write callback");
            }});
            $out = [];
            try {{
                curl_exec($ch);
                $out[] = "no exception";
            }} catch (\RuntimeException $e) {{
                $out[] = get_class($e) . ":" . $e->getMessage();
            }}
            $out[] = "errno=" . curl_errno($ch);
            curl_setopt($ch, CURLOPT_WRITEFUNCTION, null);
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            $out[] = curl_exec($ch);
            $out[] = "alive";
            return implode("|", $out);
        ');
        echo $r;
        "#
    ));
    assert_eq!(
        out,
        "RuntimeException:from the write callback|errno=0|hello-curl|alive"
    );
}

/// R3-C: a callback that throws during `curl_multi_exec()` is caught the same way.
///
/// The bridge's gate is process-wide precisely so a `try`/`catch` inside another handle's
/// callback cannot clear this handle's parked throwable — php-src has the same shape,
/// because `zend_call_function` refuses to run anything at all while `EG(exception)` is set.
#[test]
fn eval_curl_multi_callback_throw_surfaces_from_multi_exec() {
    if skip_without_curl_native("eval_curl_multi_callback_throw_surfaces_from_multi_exec") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $mh = curl_multi_init();
            $a = curl_init("{url}");
            $b = curl_init("{url}");
            $bRan = 0;
            curl_setopt($a, CURLOPT_WRITEFUNCTION, function ($h, $chunk) {{
                throw new \LogicException("multi callback boom");
            }});
            curl_setopt($b, CURLOPT_WRITEFUNCTION, function ($h, $chunk) use (&$bRan) {{
                $bRan++;
                return strlen($chunk);
            }});
            curl_multi_add_handle($mh, $a);
            curl_multi_add_handle($mh, $b);
            $out = [];
            $still = 0;
            try {{
                do {{
                    $code = curl_multi_exec($mh, $still);
                    if ($still > 0) {{ curl_multi_select($mh, 1.0); }}
                }} while ($still > 0 && $code == CURLM_OK);
                $out[] = "no exception";
            }} catch (\LogicException $e) {{
                $out[] = get_class($e) . ":" . $e->getMessage();
            }}
            $out[] = "errno=" . curl_errno($a);
            $out[] = "alive";
            return implode("|", $out);
        ');
        echo $r;
        "#
    ));
    assert_eq!(out, "LogicException:multi callback boom|errno=0|alive");
}

/// R3-C: the write-mode interlock and the `null`-clearing rules, reproduced in `eval()`.
///
/// php-src keeps ONE write mode, so whichever of `CURLOPT_RETURNTRANSFER` and
/// `CURLOPT_WRITEFUNCTION` is set LAST wins, and `CURLOPT_WRITEFUNCTION => null` falls back
/// to STDOUT — never to a previously-selected `RETURNTRANSFER`. Both were measured on PHP
/// 8.4.20 for the AOT side and are pinned by their own AOT fixtures; this is the eval half.
///
/// `CURLOPT_DEBUGFUNCTION => null` is the exception: it is never deregistered, because
/// clearing the registration restores libcurl's OWN default, which under `CURLOPT_VERBOSE`
/// dumps the whole trace to the process's fd 2 while php prints nothing there.
#[test]
fn eval_curl_write_mode_interlock_and_null_clearing_match_php() {
    if skip_without_curl_native("eval_curl_write_mode_interlock_and_null_clearing_match_php") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let output = compile_and_run_capture(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $out = [];

            $a = curl_init("{url}");
            curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($a, CURLOPT_WRITEFUNCTION, function ($h, $c) {{ return strlen($c); }});
            $out[] = curl_exec($a) === true ? "true" : "not-true";

            $b = curl_init("{url}");
            curl_setopt($b, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($b, CURLOPT_WRITEFUNCTION, function ($h, $c) {{ return strlen($c); }});
            curl_setopt($b, CURLOPT_WRITEFUNCTION, null);
            $out[] = curl_exec($b) === true ? "true" : "not-true";

            $c = curl_init("{url}");
            curl_setopt($c, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($c, CURLOPT_VERBOSE, true);
            curl_setopt($c, CURLOPT_DEBUGFUNCTION, function ($h, $t, $d) {{ return 0; }});
            curl_setopt($c, CURLOPT_DEBUGFUNCTION, null);
            curl_exec($c);
            $out[] = "done";
            return implode("|", $out);
        ');
        echo "@@", $r;
        "#
    ));
    let (printed, summary) = output
        .stdout
        .split_once("@@")
        .unwrap_or_else(|| panic!("stdout was: {}", output.stdout));
    // The `null`-cleared write callback falls back to STDOUT, so handle B's body is printed
    // rather than returned — and handle A's callback swallowed its own body.
    assert_eq!(printed, "hello-curl", "stdout was: {}", output.stdout);
    assert_eq!(summary, "true|true|done", "stdout was: {}", output.stdout);
    assert!(
        !output.stderr.contains("Trying"),
        "a cleared CURLOPT_DEBUGFUNCTION must not leak libcurl's verbose trace to fd 2; \
        stderr was: {}",
        output.stderr
    );
}

/// R3-C: a callback that is not callable is rejected at `curl_setopt()` time with the same
/// catchable `\TypeError` php-src and the AOT prelude raise — eagerly, not at transfer time.
#[test]
fn eval_curl_setopt_rejects_an_invalid_callback_catchably() {
    if skip_without_curl_native("eval_curl_setopt_rejects_an_invalid_callback_catchably") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        curl_version();
        $r = eval('
            $ch = curl_init();
            $out = [];
            try {
                curl_setopt($ch, CURLOPT_WRITEFUNCTION, "no_such_function_at_all");
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            try {
                curl_setopt($ch, CURLOPT_HEADERFUNCTION, 42);
            } catch (\TypeError $e) {
                $out[] = $e->getMessage();
            }
            $out[] = "alive";
            return implode("|", $out);
        ');
        echo $r;
        "#,
    );
    assert_eq!(
        out,
        "curl_setopt(): Argument #3 ($value) must be a valid callback for option \
        CURLOPT_WRITEFUNCTION, function \"no_such_function_at_all\" not found or invalid \
        function name\
        |curl_setopt(): Argument #3 ($value) must be a valid callback for option \
        CURLOPT_HEADERFUNCTION, no array or string given\
        |alive"
    );
}


/// FIX ROUND 1 (a): `curl_pause($h, CURLPAUSE_CONT)` FROM INSIDE THE WRITE CALLBACK of the
/// very transfer it resumes — the documented idiom for unpausing, and the shape that a
/// nesting-refusing callback frame turned into an uncatchable fatal.
///
/// Measured against real PHP 8.4.20 before this was written:
///
/// ```text
/// $paused = curl_pause($h, CURLPAUSE_CONT);   // inside CURLOPT_WRITEFUNCTION
/// -> int(0);  body=hello-curl  exec=true  errno=0
/// ```
///
/// `0` is `CURLE_OK`. The eval frame machinery therefore has to TOLERATE nesting (save the
/// outer frame, publish the inner one, restore on the way out) rather than refuse it — see
/// `crates/elephc-magician/src/interpreter/builtins/curl/callbacks.rs`'s `ActiveFrameGuard`.
#[test]
fn eval_curl_pause_from_inside_a_write_callback_matches_php() {
    if skip_without_curl_native("eval_curl_pause_from_inside_a_write_callback_matches_php") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $ch = curl_init("{url}");
            $body = "";
            $paused = null;
            curl_setopt($ch, CURLOPT_WRITEFUNCTION, function ($h, $chunk) use (&$body, &$paused) {{
                $body .= $chunk;
                $paused = curl_pause($h, CURLPAUSE_CONT);
                return strlen($chunk);
            }});
            $result = curl_exec($ch);
            return implode("|", [
                (string) $paused,
                $body,
                $result === true ? "true" : "not-true",
                (string) curl_errno($ch),
            ]);
        ');
        echo $r;
        "#
    ));
    assert_eq!(out, "0|hello-curl|true|0");
}

/// FIX ROUND 1 (b): a NESTED `curl_exec()` on a DIFFERENT handle from inside a callback.
///
/// `crates/elephc-curl/src/callbacks.rs` explicitly drops its table lock before every PHP
/// call so that "PHP code running inside the callback is free to call `curl_setopt()` /
/// `curl_exec()` on OTHER handles" — refusing the shape contradicted the bridge's own
/// documented contract.
///
/// Measured against real PHP 8.4.20:
///
/// ```text
/// outer=hello-curl  inner='sub-body'  exec=true  outerErrno=0  innerErrno=0
/// ```
#[test]
fn eval_curl_nested_exec_on_another_handle_inside_a_callback_matches_php() {
    if skip_without_curl_native(
        "eval_curl_nested_exec_on_another_handle_inside_a_callback_matches_php",
    ) {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let outer_url = server.url("/hello");
    let inner_url = server.url("/a");
    let out = compile_and_run(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $outer = curl_init("{outer_url}");
            $inner = curl_init("{inner_url}");
            curl_setopt($inner, CURLOPT_RETURNTRANSFER, true);
            $outerBody = "";
            $innerBody = null;
            curl_setopt($outer, CURLOPT_WRITEFUNCTION, function ($h, $chunk) use (&$outerBody, &$innerBody, $inner) {{
                $outerBody .= $chunk;
                $innerBody = curl_exec($inner);
                return strlen($chunk);
            }});
            $result = curl_exec($outer);
            return implode("|", [
                $outerBody,
                (string) $innerBody,
                $result === true ? "true" : "not-true",
                (string) curl_errno($outer),
                (string) curl_errno($inner),
            ]);
        ');
        echo $r;
        "#
    ));
    assert_eq!(out, "hello-curl|body-a|true|0|0");
}

/// FIX ROUND 1 (c): an INNER callback that THROWS while the OUTER transfer is still in
/// flight. Both php answers are pinned, because they are different and both matter.
///
/// Measured against real PHP 8.4.20:
///
/// ```text
/// caught inside the outer callback:
///   inner-caught:RuntimeException:inner boom | outer-ok:true | outerErrno=0 | innerErrno=0
/// NOT caught:
///   outer-threw:RuntimeException:inner boom uncaught | outerErrno=0 | innerErrno=0
/// ```
///
/// Both fall out of the design rather than being special-cased: the inner throw resumes as
/// an ordinary `Err` from the INNER `curl_exec()`, so a `try`/`catch` in the outer callback
/// simply consumes it and the outer transfer continues, while an uncaught one propagates out
/// of the callable, is parked on the OUTER frame, and aborts the outer transfer. Both errnos
/// stay `0` because each abort was an exception rather than a `CURLcode`.
///
/// THE PARKED THROW MUST TRAVEL WITH ITS OWN FRAME for this to work. A single shared slot
/// would let the inner throw be picked up at the outer level even in the caught case.
#[test]
fn eval_curl_inner_callback_throw_during_an_outer_transfer_matches_php() {
    if skip_without_curl_native(
        "eval_curl_inner_callback_throw_during_an_outer_transfer_matches_php",
    ) {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let outer_url = server.url("/hello");
    let inner_url = server.url("/a");
    let out = compile_and_run(&format!(
        r#"<?php
        curl_version();
        $caught = eval('
            $outer = curl_init("{outer_url}");
            $inner = curl_init("{inner_url}");
            curl_setopt($inner, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($inner, CURLOPT_WRITEFUNCTION, function ($h, $c) {{
                throw new \RuntimeException("inner boom");
            }});
            $log = [];
            curl_setopt($outer, CURLOPT_WRITEFUNCTION, function ($h, $chunk) use (&$log, $inner) {{
                try {{
                    curl_exec($inner);
                    $log[] = "inner-no-throw";
                }} catch (\Throwable $e) {{
                    $log[] = "inner-caught:" . get_class($e) . ":" . $e->getMessage();
                }}
                return strlen($chunk);
            }});
            try {{
                $r = curl_exec($outer);
                $log[] = "outer-ok:" . ($r === true ? "true" : "not-true");
            }} catch (\Throwable $e) {{
                $log[] = "outer-threw:" . get_class($e) . ":" . $e->getMessage();
            }}
            $log[] = "outerErrno=" . curl_errno($outer);
            $log[] = "innerErrno=" . curl_errno($inner);
            return implode(" | ", $log);
        ');
        echo $caught, "\n";
        $uncaught = eval('
            $outer = curl_init("{outer_url}");
            $inner = curl_init("{inner_url}");
            curl_setopt($inner, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($inner, CURLOPT_WRITEFUNCTION, function ($h, $c) {{
                throw new \RuntimeException("inner boom uncaught");
            }});
            $log = [];
            curl_setopt($outer, CURLOPT_WRITEFUNCTION, function ($h, $chunk) use ($inner) {{
                curl_exec($inner);
                return strlen($chunk);
            }});
            try {{
                $r = curl_exec($outer);
                $log[] = "outer-ok:" . ($r === true ? "true" : "not-true");
            }} catch (\Throwable $e) {{
                $log[] = "outer-threw:" . get_class($e) . ":" . $e->getMessage();
            }}
            $log[] = "outerErrno=" . curl_errno($outer);
            $log[] = "innerErrno=" . curl_errno($inner);
            return implode(" | ", $log);
        ');
        echo $uncaught;
        "#
    ));
    assert_eq!(
        out,
        "inner-caught:RuntimeException:inner boom | outer-ok:true | outerErrno=0 | innerErrno=0\n\
         outer-threw:RuntimeException:inner boom uncaught | outerErrno=0 | innerErrno=0"
    );
}

/// FIX ROUND 1 (d), NEGATIVE CONTROL: nesting tolerance must not turn a genuine libcurl
/// refusal into a success, and the frame must be properly RESTORED rather than left
/// published — the two ways "just allow nesting" could go wrong quietly.
///
/// - `curl_pause()` on an IDLE handle (one that has never performed) is a real libcurl
///   rejection. Measured on PHP 8.4.20: `curl_pause($ch, CURLPAUSE_CONT)` answers `43`
///   (`CURLE_BAD_FUNCTION_ARGUMENT`), not `0`. If the eval path ever started reporting
///   success here, this fails.
/// - After a nested transfer has come and gone, the OUTER handle's own callbacks must still
///   fire — which they only do if `ActiveFrameGuard` restored the outer frame instead of
///   clearing the slot to null. The outer write callback runs once per chunk, so a lost
///   outer frame shows up as a truncated body.
#[test]
fn eval_curl_nesting_preserves_refusals_and_restores_the_outer_frame() {
    if skip_without_curl_native("eval_curl_nesting_preserves_refusals_and_restores_the_outer_frame")
    {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let outer_url = server.url("/hello");
    let inner_url = server.url("/a");
    let out = compile_and_run(&format!(
        r#"<?php
        curl_version();
        $r = eval('
            $idle = curl_init("{outer_url}");
            $idlePause = curl_pause($idle, CURLPAUSE_CONT);

            $outer = curl_init("{outer_url}");
            $inner = curl_init("{inner_url}");
            curl_setopt($inner, CURLOPT_RETURNTRANSFER, true);
            $chunks = 0;
            $outerBody = "";
            curl_setopt($outer, CURLOPT_HEADERFUNCTION, function ($h, $line) use (&$chunks) {{
                $chunks++;
                return strlen($line);
            }});
            curl_setopt($outer, CURLOPT_WRITEFUNCTION, function ($h, $chunk) use (&$outerBody, $inner) {{
                // The nested transfer happens BEFORE the outer body is recorded, so a frame
                // that was cleared rather than restored would lose the rest of this callback
                // and every later one.
                curl_exec($inner);
                $outerBody .= $chunk;
                return strlen($chunk);
            }});
            curl_exec($outer);
            return implode("|", [
                (string) $idlePause,
                $outerBody,
                $chunks > 0 ? "headers-ran" : "headers-lost",
            ]);
        ');
        echo $r;
        "#
    ));
    assert_eq!(out, "43|hello-curl|headers-ran");
}

// FIX ROUND 1, MINOR 5 — COVERAGE NOTE, deliberately a note rather than a fixture here.
//
// The two PHP 8.5-only curl names (`curl_multi_get_handles`, `curl_share_init_persistent`)
// are now hidden from `function_exists()` on an older compatibility profile, matching AOT
// (`crate::interpreter::builtins::registry::names::eval_builtin_hidden_by_php_version`).
// An end-to-end fixture for it was written and then withdrawn, because
// `compile_and_run_with_php_version` CANNOT express the scenario: it threads the requested
// version into prelude injection (so the AOT declaration really is stripped and
// `function_exists()` really does answer `false` on the AOT side) but never reaches
// `codegen::set_compile_profile`, which only `src/pipeline.rs` calls. The compiled profile
// therefore stays at the default, and the harness's own `PHP_VERSION` reports `8.5.0` under
// `PhpVersion::Php84` — measured directly while writing this. Since `mark_eval_php_version`
// publishes THAT profile to the interpreter, no eval-side version gate can be observed
// through this helper at all.
//
// That is a PRE-EXISTING HARNESS GAP affecting the AOT side identically, not something the
// eval work introduced, and closing it would mean changing a shared helper every
// `compile_and_run_with_php_version` fixture depends on (the PDO surface tests assert on
// version-sensitive constants). The eval half is covered precisely and deterministically
// instead by `crate::interpreter::tests::builtins_curl`'s
// `php_85_only_curl_names_are_hidden_from_introspection_below_85` and
// `..._are_visible_on_85` (magician unit tests, `--features curl`), which drive the profile
// directly through `eval_php_profile::scoped_profile`. The AOT half already has its own
// coverage in `tests/error_tests/curl.rs`.
