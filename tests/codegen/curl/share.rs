//! Purpose:
//! End-to-end fixtures for PHP's curl SHARE interface: `curl_share_init()`/`curl_setopt()`'s
//! `CURLOPT_SHARE` attach point/`curl_share_setopt()`/`curl_share_errno()`/
//! `curl_share_strerror()`/`curl_share_close()`, the SHARE-LIFETIME HAZARD (freeing a share
//! before the easy handle attached to it must be safe, not a use-after-free), and PHP 8.5's
//! `curl_share_init_persistent()`.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - THE LOAD-BEARING FIXTURE is `freeing_the_share_before_the_easy_handle_transfers_safely_
//!   after`: it is the brief's own scenario, run against a REAL loopback transfer both
//!   before and after the share is freed, under `--gc-stats` so a double-free or leak in
//!   the detach-before-cleanup path shows up as an unbalanced heap rather than only as a
//!   crash that might not reproduce every run.
//! - No fixture reaches the public internet: every URL is the loopback fixture server's
//!   ephemeral port (`/hello`), matching every other curl codegen fixture in this suite.
//! - The DNS-share fixture (the brief's Step 1) deliberately asserts only that BOTH
//!   transfers succeed, never on `CURLINFO_NAMELOOKUP_TIME` or any other timing signal —
//!   the brief calls this out explicitly ("don't flake on timing").

use super::http_fixture::LocalHttpServer;
use crate::support::*;

/// The brief's Step 1: two sequential GETs to the same host on one share with
/// `CURL_LOCK_DATA_DNS`. Both must succeed; no timing assertion.
#[test]
fn dns_share_two_sequential_gets_both_succeed() {
    if skip_without_curl_native("dns_share_two_sequential_gets_both_succeed") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $sh = curl_share_init();
        $applied = curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SHARE, $sh);
        $first = curl_exec($ch);
        $second = curl_exec($ch);
        echo $applied ? "shared" : "not-shared", "\n";
        echo ($first === "hello-curl" && $second === "hello-curl") ? "ok\n" : "fail\n";
        "#
    ));
    assert_eq!(out, "shared\nok\n");
}

/// `curl_share_init()` mints a real `CurlShareHandle` object (PHP 8's object model, not a
/// resource), and `curl_share_close()` is the documented no-op — the handle stays usable
/// after it.
#[test]
fn share_init_returns_an_object_and_close_is_a_no_op() {
    if skip_without_curl_native("share_init_returns_an_object_and_close_is_a_no_op") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $sh = curl_share_init();
        echo get_class($sh), "\n";
        echo ($sh instanceof CurlShareHandle) ? "yes\n" : "no\n";
        curl_share_close($sh);
        echo curl_share_errno($sh), "\n";
        "#,
    );
    assert_eq!(out, "CurlShareHandle\nyes\n0\n");
}

/// A share handle is an OBJECT in PHP 8, so it must consume no `fopen()`-visible resource
/// id — the kind-8 exclusion in `__rt_mixed_from_value`. A shifted id here would silently
/// renumber every stream in a curl-using program, exactly the multi-handle regression this
/// mirrors (`multi_handle_does_not_consume_a_resource_id`).
#[test]
fn share_handle_does_not_consume_a_resource_id() {
    if skip_without_curl_native("share_handle_does_not_consume_a_resource_id") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $before = fopen("php://memory", "r+");
        $sh = curl_share_init();
        $ch = curl_init();
        $after = fopen("php://memory", "r+");
        echo (int) $after - (int) $before, "\n";
        "#,
    );
    assert_eq!(out, "1\n");
}

/// `curl_share_setopt()` applies a real `CURLSHOPT_SHARE`/`CURL_LOCK_DATA_*` pair, refuses
/// an unrecognized `CURL_LOCK_DATA_*` VALUE honestly (`false`, no fabricated warning — see
/// `crates/elephc-curl/src/share.rs`'s module doc for why there is no warning bucket the
/// way `curl_setopt()`/`curl_multi_setopt()` need one), and raises php-src's own
/// `ValueError` for an option NUMBER that is not a cURL share option at all.
#[test]
fn share_setopt_applies_and_rejects_honestly() {
    if skip_without_curl_native("share_setopt_applies_and_rejects_honestly") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $sh = curl_share_init();
        echo curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS) ? "T" : "F";
        echo curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_COOKIE) ? "T" : "F";
        echo curl_share_setopt($sh, CURLSHOPT_UNSHARE, CURL_LOCK_DATA_COOKIE) ? "T" : "F";
        echo "\n";
        // An unrecognized CURL_LOCK_DATA_* VALUE: a real libcurl-level refusal
        // (CURLSHE_BAD_OPTION), reported honestly as false with the code retrievable.
        $refused = curl_share_setopt($sh, CURLSHOPT_SHARE, 999999);
        echo $refused ? "T" : "F", " ", (curl_share_errno($sh) !== 0 ? "err" : "ok"), "\n";
        try {
            curl_share_setopt($sh, 999999, 1);
            echo "no-throw\n";
        } catch (ValueError $e) {
            echo "ValueError\n";
        }
        "#,
    );
    assert_eq!(out, "TTT\nF err\nValueError\n");
}

/// `curl_share_errno()`/`curl_share_strerror()` report libcurl's OWN `CURLSHcode` message
/// space — a fresh handle answers `CURLSHE_OK`, and its text is retrievable without ever
/// naming a `CURLSHE_*` PHP constant (php-src does not expose any).
#[test]
fn share_errno_and_strerror_report_the_share_code_space() {
    if skip_without_curl_native("share_errno_and_strerror_report_the_share_code_space") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $sh = curl_share_init();
        echo curl_share_errno($sh), "\n";
        echo curl_share_strerror(0), "\n";
        curl_share_setopt($sh, CURLSHOPT_SHARE, 999999);
        $code = curl_share_errno($sh);
        echo $code !== 0 ? "nonzero" : "zero", "\n";
        $message = curl_share_strerror($code);
        echo strlen($message) > 0 ? "has-message" : "empty", "\n";
        "#,
    );
    assert_eq!(out, "0\nNo error\nnonzero\nhas-message\n");
}

/// `curl_setopt($ch, CURLOPT_SHARE, $value)` refuses a non-share value with PHP 8's
/// `TypeError`, the same honest refusal every other typed `curl_setopt()` argument gets —
/// never a silent `false` for an option that DOES exist and IS carryable by this build.
#[test]
fn curlopt_share_rejects_a_non_share_value() {
    if skip_without_curl_native("curlopt_share_rejects_a_non_share_value") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        try {
            curl_setopt($ch, CURLOPT_SHARE, 42);
            echo "no-throw\n";
        } catch (\TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        "#,
    );
    assert_eq!(
        out,
        "curl_setopt(): Argument #3 ($value) must be of type CurlShareHandle, integer given\n"
    );
}

/// THE LOAD-BEARING FIXTURE — the brief's own scenario and the review's stated #1 target.
/// A share is attached to an easy handle via `CURLOPT_SHARE`, a real transfer runs, the
/// share is then freed FIRST (`unset($sh)`) while the easy handle is still live, and a
/// SECOND real transfer on that same easy handle must still succeed. If the bridge's
/// detach-before-`curl_share_cleanup()` design were missing or wrong, the second transfer
/// (or the easy handle's own eventual teardown) would touch freed memory. `--gc-stats`
/// also asserts the heap is balanced: the share's Mixed cell must be freed exactly once,
/// and freeing it must not double-release anything the easy handle still owns.
#[test]
fn freeing_the_share_before_the_easy_handle_transfers_safely_after() {
    if skip_without_curl_native("freeing_the_share_before_the_easy_handle_transfers_safely_after")
    {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        $sh = curl_share_init();
        curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SHARE, $sh);
        $first = curl_exec($ch);
        unset($sh);
        $second = curl_exec($ch);
        echo ($first === "hello-curl") ? "1ok" : "1fail", " ";
        echo ($second === "hello-curl") ? "2ok" : "2fail", "\n";
        "#
    ));
    assert_eq!(output.stdout, "1ok 2ok\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(
        allocs, frees,
        "freeing the share before the easy handle must not leak or double-free: {}",
        output.stderr
    );
}

/// The REVERSE order — the easy handle freed first, the share afterwards — must also stay
/// balanced: `elephc_curl_easy_free` removes the easy id from the share's own bookkeeping
/// (`crates/elephc-curl/src/share.rs`'s `detach_easy`), so the share's later free does not
/// try to touch an already-freed easy handle.
#[test]
fn freeing_the_easy_handle_before_the_share_is_also_balanced() {
    if skip_without_curl_native("freeing_the_easy_handle_before_the_share_is_also_balanced") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        $sh = curl_share_init();
        curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SHARE, $sh);
        $body = curl_exec($ch);
        unset($ch);
        unset($sh);
        echo $body === "hello-curl" ? "ok\n" : "fail\n";
        "#
    ));
    assert_eq!(output.stdout, "ok\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(
        allocs, frees,
        "freeing the easy handle before the share must not leak or double-free: {}",
        output.stderr
    );
}

/// Re-attaching an easy handle to a DIFFERENT share must detach it from the FIRST one, so
/// the first share's later free does not wrongly clear the easy handle's CURRENT
/// (second) share — the re-attachment bookkeeping `elephc_curl_easy_set_share` documents.
/// Balance under `--gc-stats` is what would catch a double-free if that bookkeeping were
/// wrong (freeing share A would then double-clear share B's own attachment, or worse).
#[test]
fn reattaching_to_a_different_share_detaches_from_the_first() {
    if skip_without_curl_native("reattaching_to_a_different_share_detaches_from_the_first") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        $a = curl_share_init();
        curl_share_setopt($a, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);
        $b = curl_share_init();
        curl_share_setopt($b, CURLSHOPT_SHARE, CURL_LOCK_DATA_COOKIE);
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SHARE, $a);
        curl_setopt($ch, CURLOPT_SHARE, $b);
        unset($a);
        $body = curl_exec($ch);
        echo $body === "hello-curl" ? "ok\n" : "fail\n";
        unset($b);
        unset($ch);
        "#
    ));
    assert_eq!(output.stdout, "ok\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(allocs, frees, "re-attachment bookkeeping must stay balanced: {}", output.stderr);
}

/// PHP 8.5's `curl_share_init_persistent()`: repeated calls with an EQUIVALENT (order- and
/// duplicate-insensitive) `CURL_LOCK_DATA_*` set resolve to the same underlying native
/// share — checked here through the object's own `$__elephc_handle` payload, which is the
/// boxed resource carrying the bridge's share id (compared `===`, PHP's own resource-
/// identity comparison) — and BOTH objects are independently usable when attached to an
/// easy handle.
#[test]
fn persistent_share_reuses_the_same_underlying_share_for_equivalent_options() {
    if skip_without_curl_native(
        "persistent_share_reuses_the_same_underlying_share_for_equivalent_options",
    ) {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $a = curl_share_init_persistent([CURL_LOCK_DATA_DNS, CURL_LOCK_DATA_COOKIE]);
        $b = curl_share_init_persistent([CURL_LOCK_DATA_COOKIE, CURL_LOCK_DATA_DNS, CURL_LOCK_DATA_DNS]);
        echo get_class($a), " ", get_class($b), "\n";
        echo ($a === $b) ? "same-object" : "different-object", "\n";
        echo ($a->__elephc_handle === $b->__elephc_handle) ? "same-share" : "different-share", "\n";
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SHARE, $a);
        $body = curl_exec($ch);
        echo $body === "hello-curl" ? "ok\n" : "fail\n";
        "#
    ));
    assert_eq!(
        out,
        "CurlSharePersistentHandle CurlSharePersistentHandle\ndifferent-object\nsame-share\nok\n"
    );
}

/// A DIFFERENT option set must mint a genuinely different persistent share.
#[test]
fn persistent_share_with_different_options_is_a_different_share() {
    if skip_without_curl_native("persistent_share_with_different_options_is_a_different_share") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $a = curl_share_init_persistent([CURL_LOCK_DATA_DNS]);
        $b = curl_share_init_persistent([CURL_LOCK_DATA_COOKIE]);
        echo ($a->__elephc_handle === $b->__elephc_handle) ? "same" : "different", "\n";
        "#,
    );
    assert_eq!(out, "different\n");
}

/// A persistent share is NEVER freed — `curl_share_close()`/the object's own Mixed-cell
/// teardown is a documented no-op for it. Proven by using the SAME PHP-level object twice
/// across an `unset()`: if teardown had actually run `curl_share_cleanup()`, attaching it
/// again afterwards would set `CURLOPT_SHARE` to a dangling pointer and the transfer would
/// fail or crash instead of succeeding.
#[test]
fn persistent_share_survives_unset_and_stays_usable() {
    if skip_without_curl_native("persistent_share_survives_unset_and_stays_usable") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        function get_share() {{
            return curl_share_init_persistent([CURL_LOCK_DATA_DNS]);
        }}
        $first = get_share();
        unset($first);
        // A fresh PHP object, but — because the underlying native share is process-
        // lifetime — still backed by the SAME live share the first call minted.
        $second = get_share();
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SHARE, $second);
        $body = curl_exec($ch);
        echo $body === "hello-curl" ? "ok\n" : "fail\n";
        "#
    ));
    assert_eq!(out, "ok\n");
}

/// `curl_copy_handle()` on a share-attached source does NOT inherit the attachment at the
/// BRIDGE level — `elephc_curl_easy_duphandle` (`crates/elephc-curl/src/abi.rs`)
/// explicitly clears `CURLOPT_SHARE` on the copy, the same "re-point rather than inherit"
/// rule that function already applies to `WRITEDATA`/`ERRORBUFFER`. Proven here by freeing
/// the share AFTER copying and confirming both the original and the copy still perform
/// cleanly: if the copy had silently inherited a raw, untracked `CURLSH *` pointer, freeing
/// the share (which only detaches ids in ITS OWN `attached` list) would leave the copy
/// pointed at freed memory.
#[test]
fn copy_handle_does_not_inherit_share_attachment() {
    if skip_without_curl_native("copy_handle_does_not_inherit_share_attachment") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        $sh = curl_share_init();
        curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);
        $original = curl_init("{url}");
        curl_setopt($original, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($original, CURLOPT_SHARE, $sh);
        $copy = curl_copy_handle($original);
        unset($sh);
        $originalBody = curl_exec($original);
        $copyBody = curl_exec($copy);
        echo $originalBody === "hello-curl" ? "orig-ok" : "orig-fail", " ";
        echo $copyBody === "hello-curl" ? "copy-ok" : "copy-fail", "\n";
        "#
    ));
    assert_eq!(output.stdout, "orig-ok copy-ok\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(
        allocs, frees,
        "copying a share-attached handle must not leak or double-free: {}",
        output.stderr
    );
}
