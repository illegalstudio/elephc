//! Purpose:
//! End-to-end fixtures for PHP's curl MULTI interface: two loopback transfers driven in
//! parallel through `curl_multi_exec()`/`curl_multi_select()`, the completion queue
//! (`curl_multi_info_read()`) and its handle IDENTITY guarantee, `curl_multi_getcontent()`,
//! attach/detach bookkeeping (`curl_multi_get_handles()`, `curl_multi_remove_handle()`),
//! and the multi error surface (`curl_multi_errno()`/`curl_multi_strerror()`).
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - THE IDENTITY FIXTURE IS THE LOAD-BEARING ONE. `curl_multi_info_read()`'s `handle`
//!   key must be the SAME PHP object that was added (`$info['handle'] === $ch`), not a
//!   fresh `CurlHandle` wrapped around the same native id — two objects owning one native
//!   handle would double-free it at teardown. `multi_info_read_returns_the_same_handle_object`
//!   pins the identity and `multi_attached_handle_survives_unset` pins that the multi's own
//!   reference keeps the object alive, both with a balanced-heap assertion.
//! - No fixture reaches the public internet: every URL is the loopback fixture server's
//!   ephemeral port (`/a` and `/b`, distinct bodies so a parallel run is actually proven).
//! - `curl_multi_select()` is called only when there is still something running, exactly as
//!   PHP's own documented loop does; it maps to `curl_multi_wait`, which returns
//!   immediately when no descriptor is waitable, so the loop cannot hang on a fixture that
//!   finished early.

use super::http_fixture::LocalHttpServer;
use crate::support::*;

/// The brief's Step 1: two easy handles with `RETURNTRANSFER`, added to one multi handle,
/// driven to completion through `curl_multi_exec()` and read back with
/// `curl_multi_getcontent()`. Both bodies must come back, each on its own handle.
#[test]
fn multi_runs_two_parallel_transfers() {
    if skip_without_curl_native("multi_runs_two_parallel_transfers") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let url_b = server.url("/b");
    let out = compile_and_run(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $a = curl_init("{url_a}");
        curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
        $b = curl_init("{url_b}");
        curl_setopt($b, CURLOPT_RETURNTRANSFER, true);
        curl_multi_add_handle($mh, $a);
        curl_multi_add_handle($mh, $b);
        $running = 0;
        do {{
            $status = curl_multi_exec($mh, $running);
            if ($running > 0) {{
                curl_multi_select($mh, 1.0);
            }}
        }} while ($running > 0 && $status === CURLM_OK);
        echo curl_multi_getcontent($a), "|", curl_multi_getcontent($b), "\n";
        echo $status === CURLM_OK ? "ok\n" : "err\n";
        "#
    ));
    assert_eq!(out, "body-a|body-b\nok\n");
}

/// `curl_multi_exec()`'s `$still_running` is a BY-REFERENCE parameter: the caller's own
/// variable has to change, which is the only way the documented PHP loop can terminate.
/// Asserted before the loop drains (`>= 1` while transfers are live) and after (`0`).
#[test]
fn multi_exec_updates_still_running_by_reference() {
    if skip_without_curl_native("multi_exec_updates_still_running_by_reference") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let out = compile_and_run(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $a = curl_init("{url_a}");
        curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
        curl_multi_add_handle($mh, $a);
        $running = -1;
        curl_multi_exec($mh, $running);
        echo $running >= 1 ? "live" : "dead", "\n";
        do {{
            curl_multi_exec($mh, $running);
            if ($running > 0) {{
                curl_multi_select($mh, 1.0);
            }}
        }} while ($running > 0);
        echo $running, "\n";
        "#
    ));
    assert_eq!(out, "live\n0\n");
}

/// THE IDENTITY GUARANTEE. `curl_multi_info_read()`'s `handle` key is the SAME PHP object
/// that was added, so `$info['handle'] === $ch` — php-src's
/// `_php_curl_multi_find_easy_handle` contract. A fresh `CurlHandle` wrapped around the
/// same native id would compare unequal here AND double-free at teardown, so this fixture
/// also runs under `--gc-stats` and asserts the heap is balanced.
#[test]
fn multi_info_read_returns_the_same_handle_object() {
    if skip_without_curl_native("multi_info_read_returns_the_same_handle_object") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let url_b = server.url("/b");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $a = curl_init("{url_a}");
        curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
        $b = curl_init("{url_b}");
        curl_setopt($b, CURLOPT_RETURNTRANSFER, true);
        curl_multi_add_handle($mh, $a);
        curl_multi_add_handle($mh, $b);
        $running = 0;
        do {{
            curl_multi_exec($mh, $running);
            if ($running > 0) {{
                curl_multi_select($mh, 1.0);
            }}
        }} while ($running > 0);
        $seen = 0;
        $queued = -1;
        while (true) {{
            $info = curl_multi_info_read($mh, $queued);
            if ($info === false) {{
                break;
            }}
            $seen++;
            echo $info['msg'] === CURLMSG_DONE ? "D" : "?";
            echo $info['result'] === CURLE_OK ? "0" : "E";
            $handle = $info['handle'];
            echo ($handle === $a || $handle === $b) ? "S" : "X";
            echo curl_multi_getcontent($handle) === "" ? "?" : "B";
        }}
        echo "\n", $seen, " ", $queued, "\n";
        "#
    ));
    assert_eq!(output.stdout, "D0SBD0SB\n2 0\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(
        allocs, frees,
        "the identity map must not leak or double-free a CurlHandle"
    );
}

/// The `$queued_messages` by-reference parameter is OPTIONAL: omitting it must compile and
/// run (the lowering hazard this task's brief called out), and an empty queue must answer
/// `false` without touching a caller variable — php-src returns before its own
/// `ZEND_TRY_ASSIGN_REF_LONG`.
#[test]
fn multi_info_read_without_the_optional_by_reference_argument() {
    if skip_without_curl_native("multi_info_read_without_the_optional_by_reference_argument") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let out = compile_and_run(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $a = curl_init("{url_a}");
        curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
        curl_multi_add_handle($mh, $a);
        $running = 0;
        do {{
            curl_multi_exec($mh, $running);
        }} while ($running > 0);
        $first = curl_multi_info_read($mh);
        echo $first === false ? "none" : "msg", "\n";
        $keep = 7;
        $second = curl_multi_info_read($mh, $keep);
        echo $second === false ? "none" : "msg", " ", $keep, "\n";
        "#
    ));
    assert_eq!(out, "msg\nnone 7\n");
}

/// `curl_multi_getcontent()` does NOT consume the captured body — php-src's
/// `RETURN_STR_COPY` leaves the buffer in place — and answers `null` (not `""`) for a
/// handle that was never capturing, which is a genuinely different answer.
#[test]
fn multi_getcontent_is_repeatable_and_null_without_returntransfer() {
    if skip_without_curl_native("multi_getcontent_is_repeatable_and_null_without_returntransfer") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let url_b = server.url("/b");
    let out = compile_and_run(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $a = curl_init("{url_a}");
        curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
        $b = curl_init("{url_b}");
        curl_multi_add_handle($mh, $a);
        curl_multi_add_handle($mh, $b);
        $running = 0;
        do {{
            curl_multi_exec($mh, $running);
            if ($running > 0) {{
                curl_multi_select($mh, 1.0);
            }}
        }} while ($running > 0);
        echo curl_multi_getcontent($a), "|", curl_multi_getcontent($a), "\n";
        echo curl_multi_getcontent($b) === null ? "null" : "notnull", "\n";
        "#
    ));
    // `$b` streams to stdout (no RETURNTRANSFER), so its body lands there ahead of the
    // echoes, exactly as it would in PHP CLI.
    assert_eq!(out, "body-bbody-a|body-a\nnull\n");
}

/// `curl_multi_get_handles()` (PHP 8.5) lists the attached easy handles IN ADD ORDER and
/// as the same objects, and `curl_multi_remove_handle()` takes one back out — the
/// bookkeeping half of the identity map.
#[test]
fn multi_get_handles_reports_add_order_and_tracks_removal() {
    if skip_without_curl_native("multi_get_handles_reports_add_order_and_tracks_removal") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let url_b = server.url("/b");
    let out = compile_and_run(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $a = curl_init("{url_a}");
        $b = curl_init("{url_b}");
        echo count(curl_multi_get_handles($mh)), "\n";
        curl_multi_add_handle($mh, $a);
        curl_multi_add_handle($mh, $b);
        $handles = curl_multi_get_handles($mh);
        echo count($handles), ($handles[0] === $a ? "A" : "x"), ($handles[1] === $b ? "B" : "x"), "\n";
        echo curl_multi_remove_handle($mh, $a) === CURLM_OK ? "removed" : "kept", "\n";
        $left = curl_multi_get_handles($mh);
        echo count($left), ($left[0] === $b ? "B" : "x"), "\n";
        "#
    ));
    assert_eq!(out, "0\n2AB\nremoved\n1B\n");
}

/// A handle removed WHILE THE TRANSFER IS STILL RUNNING stops being driven by the multi
/// handle (the other one still completes) and stays fully usable afterwards: libcurl
/// leaves a detached easy handle alive, and its `CurlHandle` object still owns it.
#[test]
fn multi_remove_handle_mid_flight_leaves_the_handle_usable() {
    if skip_without_curl_native("multi_remove_handle_mid_flight_leaves_the_handle_usable") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let url_b = server.url("/b");
    let out = compile_and_run(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $a = curl_init("{url_a}");
        curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
        $b = curl_init("{url_b}");
        curl_setopt($b, CURLOPT_RETURNTRANSFER, true);
        curl_multi_add_handle($mh, $a);
        curl_multi_add_handle($mh, $b);
        $running = 0;
        curl_multi_exec($mh, $running);
        echo curl_multi_remove_handle($mh, $a) === CURLM_OK ? "detached" : "attached", "\n";
        echo count(curl_multi_get_handles($mh)), "\n";
        do {{
            curl_multi_exec($mh, $running);
            if ($running > 0) {{
                curl_multi_select($mh, 1.0);
            }}
        }} while ($running > 0);
        echo curl_multi_getcontent($b), "\n";
        echo curl_exec($a), "\n";
        "#
    ));
    assert_eq!(out, "detached\n1\nbody-b\nbody-a\n");
}

/// THE MULTI HANDLE'S MAP HOLDS A REAL REFERENCE. `unset($ch)` on an attached handle must
/// not tear the native easy handle down — php-src takes a refcount on every added handle
/// for exactly this reason — so the transfer still completes, the completion message still
/// names the object, and the heap still balances at exit.
#[test]
fn multi_attached_handle_survives_unset() {
    if skip_without_curl_native("multi_attached_handle_survives_unset") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $a = curl_init("{url_a}");
        curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
        curl_multi_add_handle($mh, $a);
        unset($a);
        $running = 0;
        do {{
            curl_multi_exec($mh, $running);
            if ($running > 0) {{
                curl_multi_select($mh, 1.0);
            }}
        }} while ($running > 0);
        $handles = curl_multi_get_handles($mh);
        $kept = $handles[0];
        $queued = 0;
        $info = curl_multi_info_read($mh, $queued);
        if ($info !== false) {{
            echo $info['handle'] === $kept ? "same" : "other", "\n";
        }}
        echo curl_multi_getcontent($kept), "\n";
        "#
    ));
    assert_eq!(output.stdout, "same\nbody-a\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(
        allocs, frees,
        "an attached handle that outlives its variable must still be freed exactly once"
    );
}

/// The multi error surface: `curl_multi_errno()` reports the last `CURLMcode` and
/// `curl_multi_strerror()` translates it — from the `CURLM_*` numbering space, NOT
/// `CURLE_*`. Adding the same handle twice is the loudest reachable failure
/// (`CURLM_ADDED_ALREADY`).
#[test]
fn multi_errno_and_strerror_report_the_multi_code_space() {
    if skip_without_curl_native("multi_errno_and_strerror_report_the_multi_code_space") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let out = compile_and_run(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $a = curl_init("{url_a}");
        echo curl_multi_add_handle($mh, $a), curl_multi_errno($mh), "\n";
        $again = curl_multi_add_handle($mh, $a);
        echo $again === CURLM_ADDED_ALREADY ? "dup" : "ok", " ", curl_multi_errno($mh), "\n";
        echo count(curl_multi_get_handles($mh)), "\n";
        echo curl_multi_strerror(CURLM_OK), "\n";
        echo curl_multi_strerror(CURLM_ADDED_ALREADY), "\n";
        "#
    ));
    assert_eq!(
        out,
        "00\ndup 7\n1\nNo error\nThe easy handle is already added to a multi handle\n"
    );
}

/// `curl_multi_setopt()` applies a real `CURLMOPT_*` long, refuses the callback option
/// this build cannot carry with `false` plus PHP's warning, and raises
/// php-src's `ValueError` for a number that is not a multi option at all.
#[test]
fn multi_setopt_applies_longs_and_refuses_the_rest() {
    if skip_without_curl_native("multi_setopt_applies_longs_and_refuses_the_rest") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $mh = curl_multi_init();
        echo curl_multi_setopt($mh, CURLMOPT_MAXCONNECTS, 4) ? "T" : "F";
        echo curl_multi_setopt($mh, CURLMOPT_PIPELINING, CURLPIPE_MULTIPLEX) ? "T" : "F";
        echo curl_multi_setopt($mh, CURLMOPT_MAX_TOTAL_CONNECTIONS, 2) ? "T" : "F";
        echo "\n";
        try {
            curl_multi_setopt($mh, 999999, 1);
            echo "no-throw\n";
        } catch (ValueError $e) {
            echo "ValueError\n";
        }
        "#,
    );
    assert_eq!(out, "TTT\nValueError\n");
}

/// The `CURLMOPT_PUSHFUNCTION` refusal is its own fixture because it must ALSO print PHP's
/// warning, not merely answer `false` — an inert `true` (or a silent `false`) is exactly
/// what this build's unsupported-option contract forbids.
#[test]
fn multi_setopt_pushfunction_warns_and_returns_false() {
    if skip_without_curl_native("multi_setopt_pushfunction_warns_and_returns_false") {
        return;
    }
    let output = compile_and_run_capture(
        r#"<?php
        $mh = curl_multi_init();
        echo curl_multi_setopt($mh, CURLMOPT_PUSHFUNCTION, 1) ? "T" : "F", "\n";
        "#,
    );
    assert_eq!(output.stdout, "F\n");
    assert!(
        output.diagnostics.contains(
            "Warning: curl_multi_setopt(): Option 20014 is not supported by this build"
        ),
        "the refusal must say so through PHP's warning channel; diagnostics were: {}",
        output.diagnostics
    );
}

/// PUNCH-LIST ITEM 7: `curl_multi_setopt()` CLASSIFIES THE OPTION BEFORE IT TYPE-CHECKS
/// THE VALUE, the order php-src uses (one `switch (option)`) and the order `curl_setopt()`
/// already used. A closure on `CURLMOPT_PUSHFUNCTION` is therefore the ordinary
/// "unsupported by this build" answer — `false` plus PHP's warning —
/// not a `TypeError` about scalar types, and a closure on an option number that is not a
/// multi option at all is php-src's `ValueError`. Measured on PHP 8.4.20:
/// `curl_multi_setopt($mh, 999999, function () {})` raises `ValueError`, and a non-callable
/// array on `CURLMOPT_PUSHFUNCTION` raises a TypeError about a CALLBACK — never the
/// `string|int|float|bool` complaint the value-first order produced here.
#[test]
fn multi_setopt_classifies_the_option_before_type_checking_the_value() {
    if skip_without_curl_native("multi_setopt_classifies_the_option_before_type_checking_the_value")
    {
        return;
    }
    let output = compile_and_run_capture(
        r#"<?php
        $mh = curl_multi_init();
        try {
            echo curl_multi_setopt($mh, CURLMOPT_PUSHFUNCTION, function () { return 0; }) ? "T" : "F", "\n";
        } catch (TypeError $e) {
            echo "TypeError: ", $e->getMessage(), "\n";
        }
        try {
            curl_multi_setopt($mh, 999999, function () { return 0; });
            echo "no-throw\n";
        } catch (ValueError $e) {
            echo "ValueError\n";
        } catch (TypeError $e) {
            echo "TypeError\n";
        }
        // A non-scalar on a REAL carryable option is still a TypeError, unchanged.
        try {
            curl_multi_setopt($mh, CURLMOPT_MAXCONNECTS, [1]);
            echo "no-throw\n";
        } catch (TypeError $e) {
            echo "TypeError\n";
        }
        "#,
    );
    assert_eq!(output.stdout, "F\nValueError\nTypeError\n");
    assert!(
        output.diagnostics.contains(
            "Warning: curl_multi_setopt(): Option 20014 is not supported by this build"
        ),
        "the closure must reach the unsupported-option warning, not a type error; diagnostics were: {}",
        output.diagnostics
    );
}

/// THE DRIFT RATCHET FOR `curl_multi_setopt()`'s PRELUDE-SIDE OPTION TABLE. The prelude
/// classifies the option before it type-checks the value (the fixture above), which means
/// it carries its own copy of the frozen `CURLMOPT_*` numbers next to `multi_option_kind`'s
/// (`crates/elephc-curl/src/multi.rs`). Those two must move together: if the frozen surface
/// grows a `CURLMOPT_*` and only the bridge learns it, the crate's own
/// `multi_option_table_matches_the_frozen_surface` ratchet stays green while PHP code gets
/// a `ValueError` for a perfectly valid option — a hard, wrong failure with the bridge
/// never consulted.
///
/// THE OPTION LIST IS READ FROM THE FROZEN SURFACE AT TEST TIME, not hand-copied, so a new
/// `CURLMOPT_*` constant enters this fixture the moment it is registered and fails here
/// until the prelude classifies it too. The asserted property is only apply-or-refuse
/// (`true`/`false`, warning allowed) versus `ValueError` — it says nothing about WHICH of
/// the two an option gets, so it does not duplicate the fixtures above.
#[test]
fn every_registered_multi_option_is_classified_by_the_prelude() {
    if skip_without_curl_native("every_registered_multi_option_is_classified_by_the_prelude") {
        return;
    }
    let surface = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/docs/curl_surface.json"),
    )
    .expect("the frozen curl surface must be readable");
    let surface: serde_json::Value =
        serde_json::from_slice(&surface).expect("the frozen curl surface must be valid JSON");
    let mut names: Vec<&str> = surface["constants"]
        .as_object()
        .expect("constants must be an object")
        .keys()
        .filter(|name| name.starts_with("CURLMOPT_"))
        .map(String::as_str)
        .collect();
    names.sort_unstable();
    assert!(!names.is_empty(), "the frozen surface must register CURLMOPT_* constants");
    let entries = names
        .iter()
        .map(|name| format!("            \"{name}\" => {name},\n"))
        .collect::<String>();

    let output = compile_and_run_capture(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $options = [
{entries}        ];
        echo count($options), "\n";
        foreach ($options as $name => $option) {{
            try {{
                curl_multi_setopt($mh, $option, 1);
            }} catch (ValueError $e) {{
                echo "VALUEERROR ", $name, "\n";
            }} catch (TypeError $e) {{
                echo "TYPEERROR ", $name, "\n";
            }}
        }}
        echo "classified\n";
        "#
    ));
    assert_eq!(
        output.stdout,
        format!("{}\nclassified\n", names.len()),
        "every frozen CURLMOPT_* must be classified by the prelude, never rejected as \
         'not a valid cURL multi option': the prelude's table (src/curl_prelude.rs, \
         curl_multi_setopt) has drifted from multi_option_kind's"
    );
}

/// `curl_multi_init()` mints a real `CurlMultiHandle` object (PHP 8's object model, not a
/// resource), and `curl_multi_close()` is the no-op PHP 8 documents — the handle stays
/// usable after it.
#[test]
fn multi_init_returns_an_object_and_close_is_a_no_op() {
    if skip_without_curl_native("multi_init_returns_an_object_and_close_is_a_no_op") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $mh = curl_multi_init();
        echo get_class($mh), "\n";
        echo ($mh instanceof CurlMultiHandle) ? "yes\n" : "no\n";
        curl_multi_close($mh);
        echo curl_multi_errno($mh), "\n";
        "#,
    );
    assert_eq!(out, "CurlMultiHandle\nyes\n0\n");
}

/// A multi handle is an OBJECT in PHP 8, so it must consume no `fopen()`-visible resource
/// id — the kind-7 exclusion in `__rt_mixed_from_value`. A shifted id here would silently
/// renumber every stream in a curl-using program.
#[test]
fn multi_handle_does_not_consume_a_resource_id() {
    if skip_without_curl_native("multi_handle_does_not_consume_a_resource_id") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $before = fopen("php://memory", "r+");
        $mh = curl_multi_init();
        $ch = curl_init();
        $after = fopen("php://memory", "r+");
        echo (int) $after - (int) $before, "\n";
        "#,
    );
    assert_eq!(out, "1\n");
}

/// A multi handle whose transfers ran is freed exactly once at teardown, together with
/// every easy handle it drove: the kind-7 destructor is the only release path, exactly as
/// kind 6 is for `CurlHandle`.
#[test]
fn multi_handles_are_freed_exactly_once() {
    if skip_without_curl_native("multi_handles_are_freed_exactly_once") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let url_b = server.url("/b");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        function fetch_pair(): string {{
            $mh = curl_multi_init();
            $a = curl_init("{url_a}");
            curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
            $b = curl_init("{url_b}");
            curl_setopt($b, CURLOPT_RETURNTRANSFER, true);
            curl_multi_add_handle($mh, $a);
            curl_multi_add_handle($mh, $b);
            $running = 0;
            do {{
                curl_multi_exec($mh, $running);
                if ($running > 0) {{
                    curl_multi_select($mh, 1.0);
                }}
            }} while ($running > 0);
            $body = curl_multi_getcontent($a) . curl_multi_getcontent($b);
            curl_multi_remove_handle($mh, $a);
            unset($mh);
            unset($a);
            unset($b);
            return $body;
        }}
        echo fetch_pair(), fetch_pair(), "\n";
        "#
    ));
    assert_eq!(output.stdout, "body-abody-bbody-abody-b\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(allocs, frees, "multi teardown must be balanced");
}

/// THE DRAIN LOOP WITH THE OPTIONAL ARGUMENT OMITTED, under `--gc-stats`, across several
/// complete multi runs: `curl_multi_info_read($mh)` called until it answers `false`, with no
/// `$queued_messages` variable at all.
///
/// This is the shape that made the caller-side cell for an omitted optional by-reference
/// argument worth fixing: it used to leak one 16-byte block per call, which a long-running
/// program repeats forever. The balance assertion here is the curl-facing half of
/// `runtime_gc::omitted_by_ref_default_args`, which pins the same property with no curl
/// involved.
///
/// The loop is written in the `while (true) { … break; }` form rather than php.net's
/// `while ($info = curl_multi_info_read($mh))` because ASSIGNMENT-IN-A-LOOP-CONDITION leaks
/// the assigned value on every iteration in this compiler — a separate, pre-existing defect
/// with nothing to do with by-reference arguments (measured curl-free: a plain
/// `while ($x = f())` over an array-returning `f()` leaks one array per iteration).
/// `multi_info_read_php_documented_loop_shape` below covers that spelling's BEHAVIOUR.
#[test]
fn multi_info_read_drain_loop_is_heap_balanced() {
    if skip_without_curl_native("multi_info_read_drain_loop_is_heap_balanced") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let url_b = server.url("/b");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        function drain(): int {{
            $mh = curl_multi_init();
            $a = curl_init("{url_a}");
            curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
            $b = curl_init("{url_b}");
            curl_setopt($b, CURLOPT_RETURNTRANSFER, true);
            curl_multi_add_handle($mh, $a);
            curl_multi_add_handle($mh, $b);
            $running = 0;
            do {{
                curl_multi_exec($mh, $running);
                if ($running > 0) {{
                    curl_multi_select($mh, 1.0);
                }}
            }} while ($running > 0);
            $done = 0;
            while (true) {{
                $info = curl_multi_info_read($mh);
                if ($info === false) {{
                    break;
                }}
                if ($info['result'] === CURLE_OK) {{
                    $done++;
                }}
            }}
            return $done;
        }}
        echo drain(), drain(), drain(), "\n";
        "#
    ));
    assert_eq!(output.stdout, "222\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(
        allocs, frees,
        "the drain loop with the optional by-reference argument omitted must not leak: {}",
        output.stderr
    );
}

/// php.net's own spelling of the drain loop — `while ($info = curl_multi_info_read($mh))` —
/// produces the right ANSWERS: two completed transfers, each naming the handle it belongs to,
/// and the loop terminates on the `false` that ends the queue.
///
/// Balance is asserted by `multi_info_read_drain_loop_is_heap_balanced` on the equivalent
/// `while (true)` form instead, because assignment-in-a-loop-condition leaks the assigned
/// value in this compiler regardless of what produced it (see that fixture's comment).
#[test]
fn multi_info_read_php_documented_loop_shape() {
    if skip_without_curl_native("multi_info_read_php_documented_loop_shape") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url_a = server.url("/a");
    let url_b = server.url("/b");
    let out = compile_and_run(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $a = curl_init("{url_a}");
        curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
        $b = curl_init("{url_b}");
        curl_setopt($b, CURLOPT_RETURNTRANSFER, true);
        curl_multi_add_handle($mh, $a);
        curl_multi_add_handle($mh, $b);
        $running = 0;
        do {{
            curl_multi_exec($mh, $running);
            if ($running > 0) {{
                curl_multi_select($mh, 1.0);
            }}
        }} while ($running > 0);
        $seen = "";
        while ($info = curl_multi_info_read($mh)) {{
            $seen .= $info['result'] === CURLE_OK ? "0" : "E";
            $seen .= $info['handle'] === $a ? "A" : ($info['handle'] === $b ? "B" : "?");
        }}
        echo $seen, "\n";
        "#
    ));
    assert!(
        out == "0A0B\n" || out == "0B0A\n",
        "both transfers must be reported, each naming its own handle; got {out:?}"
    );
}
