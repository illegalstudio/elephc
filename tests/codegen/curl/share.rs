//! Purpose:
//! End-to-end fixtures for PHP's curl SHARE interface: `curl_share_init()`/`curl_setopt()`'s
//! `CURLOPT_SHARE` attach point/`curl_share_setopt()`/`curl_share_errno()`/
//! `curl_share_strerror()`/`curl_share_close()`, the SHARE LIFETIME (freeing a share while
//! an easy handle is still attached to it must be safe and must not leak — PHP-visible
//! behaviour only; see `crates/elephc-curl/src/tests.rs::native_share` for the Rust-level
//! tests that observe the DEFERRED `curl_share_cleanup()` outcome directly), and PHP 8.5's
//! `curl_share_init_persistent()`.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - libcurl 8.21.0 REFCOUNTS a share (`crates/elephc-curl/src/share.rs`'s module doc):
//!   `curl_share_cleanup()` while an easy handle still references it fails
//!   (`CURLSHE_IN_USE`) rather than corrupting anything, so an unhandled failure there is a
//!   silent PERMANENT LEAK, not a use-after-free. The bridge closes that by DEFERRING the
//!   real cleanup call until every attached easy handle has genuinely detached — this
//!   crate's PHP-level fixtures below can only observe the FUNCTIONAL consequence (a
//!   transfer still succeeding after `unset()`, and a balanced `--gc-stats` heap for the
//!   PHP-visible Mixed cell); the deferred-cleanup outcome ITSELF (`CURLSHE_OK`, and only
//!   after the last detach) is a bridge-internal fact only the Rust-level tests can probe.
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

/// THE BRIEF'S OWN SCENARIO. A share is attached to an easy handle via `CURLOPT_SHARE`, a
/// real transfer runs, the share is then freed FIRST (`unset($sh)`) while the easy handle
/// is still live, and a SECOND real transfer on that same easy handle must still succeed —
/// now genuinely still sharing (the bridge DEFERS the real `curl_share_cleanup()` call
/// rather than forcing it or forcing an unlink; `crates/elephc-curl/src/share.rs`'s module
/// doc). `--gc-stats` asserts the PHP-visible heap is balanced (the share's Mixed cell
/// freed exactly once); the bridge-internal fact that the underlying `curl_share_cleanup()`
/// call itself is deferred and only later succeeds is not observable from PHP and is
/// covered instead by `crate::tests::native_share`'s Rust-level tests.
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

/// Re-attaching an easy handle to a DIFFERENT share must detach it from the FIRST one (so
/// the first share's own bookkeeping — and any deferred free waiting on it — does not keep
/// waiting on an attachment that has already moved to the second share) — the
/// re-attachment bookkeeping `elephc_curl_easy_set_share`/`detach_easy` document. Balance
/// under `--gc-stats` is what would catch a leak or double-free if that bookkeeping were
/// wrong.
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

/// `curl_copy_handle()` on a share-attached source does NOT inherit the attachment — which
/// is PHP'S OWN BEHAVIOR, re-measured on PHP 8.4.20 / libcurl 8.19.0 after a punch-list
/// item claimed the opposite: with only the COPY alive, `curl_share_setopt()` on the share
/// still succeeds (libcurl reports `CURLSHE_IN_USE` for as long as any easy handle is
/// attached, and the two-real-handles control confirms the probe), and PHP's
/// `WeakReference` shows the copy holds no reference to the share object either. The
/// bridge matches by construction: `elephc_curl_easy_duphandle`
/// (`crates/elephc-curl/src/abi.rs`, which carries the full transcript) explicitly clears
/// `CURLOPT_SHARE` on the copy, the same "re-point rather than inherit" rule that function
/// already applies to `WRITEDATA`/`ERRORBUFFER`. Proven here by freeing
/// the share AFTER copying and confirming both the original and the copy still perform
/// cleanly: if the copy had silently inherited a raw, untracked `CURLSH *` reference, the
/// bridge's `attached` list would UNDERCOUNT real attachments, so freeing the share once
/// the ORIGINAL alone detaches would attempt the real `curl_share_cleanup()` while the
/// copy still held a live libcurl-level reference — `CURLSHE_IN_USE`, not `CURLSHE_OK`
/// (caught loudly by `crate::share::finish_share_cleanup`'s `debug_assert_eq!` at the
/// bridge level; this PHP-level fixture only asserts the functional consequence, that both
/// handles keep transferring cleanly).
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

/// IMPORTANT-1 END-TO-END COVERAGE: a share attached to a MULTI-DRIVEN easy handle, freed
/// (`unset($sh)`) before the multi loop has driven the transfer to completion — a shape
/// only reachable through `curl_multi_exec()`, which is explicitly non-blocking and can
/// leave a transfer genuinely in flight between PHP statements. The removed force-unlink
/// design used to call `curl_setopt($ch, CURLOPT_SHARE, NULL)` synchronously from inside
/// `unset($sh)`, regardless of whether the attached easy handle had a transfer running
/// under the multi handle at that moment; the deferred design never touches a live easy
/// handle's `CURLOPT_SHARE` at all, so this is safe by construction now. The deterministic,
/// non-flaky version of this scenario (which also probes the actual `curl_share_cleanup()`
/// outcome, not just the absence of a crash) lives at the Rust level:
/// `crate::tests::native_share::share_freed_while_attached_via_multi_defers_cleanup_until_
/// the_easy_is_freed`. This fixture exercises the SAME shape through the real compiled
/// prelude/builtins/lowering path instead, asserting the functional outcome only.
#[test]
fn share_freed_while_multi_attached_still_completes_the_transfer() {
    if skip_without_curl_native("share_freed_while_multi_attached_still_completes_the_transfer")
    {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $sh = curl_share_init();
        curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);
        $mh = curl_multi_init();
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SHARE, $sh);
        curl_multi_add_handle($mh, $ch);
        // Free the share before the multi loop has run at all -- curl_multi_add_handle()
        // alone does not start the transfer, so this is the earliest point a share can be
        // freed while genuinely still attached to a multi-managed easy handle.
        unset($sh);
        $running = 0;
        do {{
            curl_multi_exec($mh, $running);
            if ($running > 0) {{
                curl_multi_select($mh, 1.0);
            }}
        }} while ($running > 0);
        $body = curl_multi_getcontent($ch);
        echo $body === "hello-curl" ? "ok\n" : "fail\n";
        "#
    ));
    assert_eq!(out, "ok\n");
}
