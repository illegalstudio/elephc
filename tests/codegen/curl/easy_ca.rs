//! Purpose:
//! End-to-end fixtures for RUNTIME CA-TRUST-STORE DISCOVERY (`crates/elephc-curl/src/ca.rs`):
//! that a compiled binary verifies HTTPS against a store it found at RUN time rather than
//! the one libcurl's `configure` happened to find at BUILD time, and that everything a PHP
//! program does about CA configuration still outranks it.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - `CURLINFO_CAINFO` CANNOT BE USED AS THE OBSERVABLE, and finding that out is what
//!   shaped this file. libcurl 8.21.0 answers it from the COMPILE-TIME `CURL_CA_BUNDLE`
//!   constant and nothing else (`lib/getinfo.c:174`; its own man page calls it the
//!   "default built-in CA certificate path"). MEASURED: after
//!   `curl_easy_setopt(CURLOPT_CAINFO, "/tmp/whatever")`, `CURLINFO_CAINFO` still reports
//!   `/etc/ssl/cert.pem`. So `curl_getinfo($ch)["cainfo"]` reports what the BUILD machine
//!   had, discovery or no discovery, in elephc exactly as in php — and every fixture below
//!   observes the trust store the only way it is observable: by what a real TLS handshake
//!   does.
//! - THE TWO `CURLcode`s ARE THE WHOLE VOCABULARY. Against `tls_fixture.rs`'s self-signed
//!   loopback server with verification ON, libcurl answers `60`
//!   (`CURLE_PEER_FAILED_VERIFICATION`, PHP's `CURLE_SSL_CACERT`) when the CA store in
//!   force was READ successfully and simply does not vouch for the fixture, and `77`
//!   (`CURLE_SSL_CACERT_BADFILE`) when the store could not be read at all. Pointing
//!   `$CURL_CA_BUNDLE` at a path that does not exist therefore makes "which bundle is in
//!   force" directly visible: `77` means the discovered one, `60` means something readable
//!   replaced it.
//! - THE ENVIRONMENT IS SET ON THE COMPILED PROGRAM, not on the test process
//!   (`compile_and_run_with_env`). The bridge resolves its answer ONCE per process, at the
//!   first `curl_init()`, so a `putenv()` from inside the program would be a race against
//!   the very thing under test; and mutating the test harness's own environment would leak
//!   into every other fixture running in parallel in the same binary.
//! - WHAT THESE DEPEND ON, PRECISELY. Every fixture that sets `$CURL_CA_BUNDLE` pins the
//!   discovered bundle to a path that exists nowhere, so its assertion holds on any
//!   machine regardless of CA layout. The ONE fixture that runs with no environment at all
//!   (`a_verified_transfer_has_a_readable_ca_store_without_any_configuration`) does depend
//!   on the runner having SOME readable trust store — which is the property it exists to
//!   assert, and which holds on every developer machine and every CI image here. What no
//!   fixture depends on is WHICH store that is, or whether it is the baked-in one.
//! - THE ORDERED PROBE LIST IS NOT COVERED HERE, deliberately: the branch that matters —
//!   the baked-in path is MISSING — is by definition unreachable on the machine that baked
//!   it. It is unit-tested against an injected existence predicate in
//!   `crates/elephc-curl/src/tests.rs`'s `ca_discovery` instead, and the fixtures below
//!   cover the other half, that a decision once made really reaches the TLS backend.
//! - No fixture here reaches the public internet; every URL is the loopback fixture's own
//!   `127.0.0.1` port.

use std::ffi::OsStr;

use super::tls_fixture::{unrelated_ca_bundle, LocalHttpsServer};
use crate::support::*;

/// An absolute path that cannot exist, used as `$CURL_CA_BUNDLE` to make "the bridge's
/// discovered bundle is what is in force" observable as `CURLcode` 77.
const MISSING_BUNDLE: &str = "/nonexistent-elephc-runtime-ca-discovery.pem";

/// An existing but EMPTY directory, for the `CURLOPT_CAPATH` fixture. A capath is a
/// directory of hash-named certificates; an empty one is accepted by OpenSSL's `by_dir`
/// lookup (which records the directory rather than scanning it) and vouches for nothing,
/// so it contributes a real-but-useless store without introducing a second trust anchor
/// that could confuse the assertion.
fn empty_capath_dir() -> std::path::PathBuf {
    use std::sync::OnceLock;
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = std::env::temp_dir()
            .join(format!("elephc-curl-empty-capath-{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("curl ca fixture: create empty capath dir");
        path
    })
    .clone()
}

/// The PHP tail every fixture below shares: run the transfer and print `F`/`X` for
/// failed/succeeded followed by libcurl's `CURLcode`.
const REPORT_ERRNO: &str = r#"
        $body = curl_exec($ch);
        echo $body === false ? "F" : "X";
        echo curl_errno($ch), "\n";
        "#;

/// THE BASELINE, AND THE ONE FIXTURE THAT RUNS WITH NO ENVIRONMENT AT ALL: a verified
/// HTTPS transfer that sets no CA option whatsoever still has a READABLE trust store in
/// force, so it is rejected with `60` (the store does not vouch for the self-signed
/// fixture) rather than `77` (no store could be read).
///
/// On the machine that built libcurl this is discovery's branch 1 — the baked-in bundle
/// exists, so the bridge changes nothing and this is the behavior that predates the
/// feature. On a machine where the baked path is gone it is branch 2, and the assertion is
/// the same because that is the entire point: the observable behavior of a working HTTPS
/// client must not depend on which machine compiled it.
#[test]
fn a_verified_transfer_has_a_readable_ca_store_without_any_configuration() {
    if skip_without_curl_native("a_verified_transfer_has_a_readable_ca_store_without_any_configuration")
    {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, true);
        {REPORT_ERRNO}"#
    ));
    assert_eq!(out, "F60\n");
}

/// `$CURL_CA_BUNDLE` REACHES THE TLS BACKEND: the same transfer, with the environment
/// naming a bundle that does not exist, fails with `77` instead of the `60` above. Only a
/// `CURLOPT_CAINFO` that actually arrived at libcurl can produce that answer, so this is
/// the proof that the bridge's discovery both ran and applied its decision.
///
/// The value is deliberately NOT existence-checked by the bridge: an operator naming a
/// bundle is an instruction, and a wrong one has to fail closed like this rather than be
/// silently replaced by a guess from the probe list.
#[test]
fn the_curl_ca_bundle_environment_variable_is_honored() {
    if skip_without_curl_native("the_curl_ca_bundle_environment_variable_is_honored") {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let out = compile_and_run_with_env(
        &format!(
            r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, true);
        {REPORT_ERRNO}"#
        ),
        &[("CURL_CA_BUNDLE", OsStr::new(MISSING_BUNDLE))],
    );
    assert_eq!(out, "F77\n");
}

/// AN EXPLICIT `CURLOPT_CAINFO` ALWAYS WINS. With the environment pointing discovery at a
/// missing bundle (which alone answers `77`), a program that sets its own readable bundle
/// gets `60` — its value, not the bridge's, is what libcurl verified against.
#[test]
fn an_explicit_cainfo_overrides_discovery() {
    if skip_without_curl_native("an_explicit_cainfo_overrides_discovery") {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let bundle = unrelated_ca_bundle();
    let bundle_path = bundle.display();
    let out = compile_and_run_with_env(
        &format!(
            r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, true);
        curl_setopt($ch, CURLOPT_CAINFO, "{bundle_path}");
        {REPORT_ERRNO}"#
        ),
        &[("CURL_CA_BUNDLE", OsStr::new(MISSING_BUNDLE))],
    );
    assert_eq!(out, "F60\n");
}

/// `CURLOPT_CAPATH` COMPOSES WITH THE DISCOVERED BUNDLE INSTEAD OF REPLACING IT, exactly
/// as a capath composes with libcurl's own baked-in bundle in a stock build. A capath is a
/// different option slot from `CURLOPT_CAINFO`, and libcurl loads both when both are set,
/// so the trust store here is `discovered ∪ capath` — `77`, because the discovered bundle
/// is the missing file `$CURL_CA_BUNDLE` names and libcurl gives up on an unreadable CA
/// FILE before it ever loads the CApath.
///
/// AN EARLIER REVISION OF THE BRIDGE CLEARED THE DISCOVERED BUNDLE HERE and this fixture
/// asserted `60`. Both were wrong, and libcurl's own verbose trace is what showed it:
/// `curl_easy_setopt(CURLOPT_CAINFO, NULL)` resets `custom_cafile`, which is the exact
/// condition under which libcurl RE-INJECTS its compile-time default, so the retire
/// sequence produced
///
/// ```text
/// *   CAfile: /etc/ssl/cert.pem        <- the BAKED default, back again
/// *   CApath: /tmp/…/empty-capath
/// ```
///
/// — never "capath alone", and on a machine where that baked path does not exist it would
/// have turned a perfectly good capath into a hard `77`. See
/// `crates/elephc-curl/src/abi.rs`'s `apply_discovered_cainfo` for the full measurement.
///
/// `60` here would mean the discovered bundle had been taken back out and libcurl had
/// re-injected the baked default, which on this machine is readable.
#[test]
fn setting_capath_keeps_the_discovered_bundle() {
    if skip_without_curl_native("setting_capath_keeps_the_discovered_bundle") {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let capath = empty_capath_dir();
    let capath_path = capath.display();
    let out = compile_and_run_with_env(
        &format!(
            r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, true);
        curl_setopt($ch, CURLOPT_CAPATH, "{capath_path}");
        {REPORT_ERRNO}"#
        ),
        &[("CURL_CA_BUNDLE", OsStr::new(MISSING_BUNDLE))],
    );
    assert_eq!(out, "F77\n");
}

/// `curl_reset()` RE-RUNS DISCOVERY. `curl_easy_reset` restores libcurl's own defaults,
/// which on a foreign machine means the dead baked-in path, so a handle that was reset
/// would otherwise be LESS portable than a fresh one. Here the program's own readable
/// bundle (`60` on its own) is wiped by the reset and the discovered bundle is back in
/// force, so the answer flips to `77`.
#[test]
fn curl_reset_re_applies_discovery() {
    if skip_without_curl_native("curl_reset_re_applies_discovery") {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let bundle = unrelated_ca_bundle();
    let bundle_path = bundle.display();
    let out = compile_and_run_with_env(
        &format!(
            r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_CAINFO, "{bundle_path}");
        curl_reset($ch);
        curl_setopt($ch, CURLOPT_URL, "{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, true);
        {REPORT_ERRNO}"#
        ),
        &[("CURL_CA_BUNDLE", OsStr::new(MISSING_BUNDLE))],
    );
    assert_eq!(out, "F77\n");
}

/// `curl_copy_handle()` CARRIES THE DISCOVERED BUNDLE like every other option:
/// `curl_easy_duphandle` re-`strdup`s every string option, so the copy owns its own
/// allocation of the same path and answers `77` exactly as the original would.
#[test]
fn copy_handle_carries_the_discovered_bundle() {
    if skip_without_curl_native("copy_handle_carries_the_discovered_bundle") {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let out = compile_and_run_with_env(
        &format!(
            r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, true);
        $copy = curl_copy_handle($ch);
        unset($ch);
        $ch = $copy;
        {REPORT_ERRNO}"#
        ),
        &[("CURL_CA_BUNDLE", OsStr::new(MISSING_BUNDLE))],
    );
    assert_eq!(out, "F77\n");
}

/// THE COPY IS NOT RE-DISCOVERED, which is the half that could have gone wrong: a copy of
/// a handle whose program had set `CURLOPT_CAINFO` itself must keep THAT value, not have
/// it overwritten by a fresh discovery pass. The source's readable bundle survives the
/// copy, so the answer is `60`, not the `77` the fixture above gets.
#[test]
fn copy_handle_keeps_an_explicit_cainfo_instead_of_re_discovering() {
    if skip_without_curl_native("copy_handle_keeps_an_explicit_cainfo_instead_of_re_discovering") {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let bundle = unrelated_ca_bundle();
    let bundle_path = bundle.display();
    let out = compile_and_run_with_env(
        &format!(
            r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, true);
        curl_setopt($ch, CURLOPT_CAINFO, "{bundle_path}");
        $copy = curl_copy_handle($ch);
        unset($ch);
        $ch = $copy;
        {REPORT_ERRNO}"#
        ),
        &[("CURL_CA_BUNDLE", OsStr::new(MISSING_BUNDLE))],
    );
    assert_eq!(out, "F60\n");
}

/// DISCOVERY IS NOT AN AOT-ONLY FEATURE: `eval()`'s curl builtins call the same
/// `elephc_curl_easy_init` in the same bridge, so a handle created inside `eval()` carries
/// the same discovered bundle and answers the same `77`.
///
/// The top-level `curl_init()` is required, not decorative — see `eval.rs`'s module doc:
/// curl detection cannot see inside an `eval()` string, so without a real curl call the
/// prelude would never be injected and the bridge never linked.
#[test]
fn eval_handles_get_the_same_discovered_bundle() {
    if skip_without_curl_native("eval_handles_get_the_same_discovered_bundle") {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let out = compile_and_run_with_env(
        &format!(
            r#"<?php
        curl_close(curl_init());
        $code = '
            $ch = curl_init("{url}");
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, true);
            $body = curl_exec($ch);
            return ($body === false ? "F" : "X") . curl_errno($ch);
        ';
        echo eval($code), "\n";
        "#
        ),
        &[("CURL_CA_BUNDLE", OsStr::new(MISSING_BUNDLE))],
    );
    assert_eq!(out, "F77\n");
}
