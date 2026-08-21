//! Purpose:
//! Wave E: real HTTPS transfers through `curl_exec()` against the loopback TLS fixture in
//! `tls_fixture.rs` — the `CURLOPT_SSL_VERIFYPEER=false` success and the
//! `CURLOPT_SSL_VERIFYPEER=true` rejection of the same self-signed certificate.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - THESE ARE THE PROOF THAT TLS IS WIRED, not just that an option was accepted. The
//!   managed libcurl is built against the managed OpenSSL; until
//!   something actually completes a handshake, nothing distinguishes "HTTPS works" from
//!   "HTTPS fails in a way no test looks at". The success case reads a body back over the
//!   socket; the failure case gets libcurl's own certificate `CURLcode`.
//! - NO SYSTEM CA STORE IS INVOLVED, on purpose. The success case turns verification off
//!   and the failure case supplies its own bundle, so neither depends on what the BUILD
//!   machine (or the running machine) had installed. Which store a transfer that
//!   configures NOTHING ends up using is a separate question, answered by runtime CA
//!   discovery and covered by `easy_ca.rs`.
//! - BOTH `CURLOPT_SSL_VERIFYPEER` AND `CURLOPT_SSL_VERIFYHOST` HAVE TO GO for the success
//!   case. They are independent checks in libcurl: with only the peer check off, the
//!   default `VERIFYHOST=2` still rejects the fixture, whose certificate carries
//!   `CN=elephc-test` and no `127.0.0.1` SAN. Turning off one and expecting success is the
//!   most common way to write this test wrong.
//! - No fixture here reaches the public internet: the URL is always the fixture's own
//!   `127.0.0.1` port. The optional live `https://example.com` smoke the brief allows is
//!   deliberately NOT added — it would be `#[ignore]`d and therefore never run, while
//!   still being a public-internet dependency in the tree.

use super::tls_fixture::{unrelated_ca_bundle, LocalHttpsServer};
use crate::support::*;

/// An HTTPS GET with peer/host verification turned off completes against the local
/// self-signed fixture and returns its body — a real TLS handshake through the managed
/// OpenSSL, not a plaintext fallback (the fixture speaks nothing but TLS).
#[test]
fn https_transfer_succeeds_without_peer_verification() {
    if skip_without_curl_native("https_transfer_succeeds_without_peer_verification") {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, false);
        curl_setopt($ch, CURLOPT_SSL_VERIFYHOST, 0);
        $body = curl_exec($ch);
        echo $body === false ? curl_error($ch) : $body, "\n";
        echo curl_errno($ch), "\n";
        echo curl_getinfo($ch, CURLINFO_HTTP_CODE), "\n";
        echo curl_getinfo($ch, CURLINFO_SCHEME), "\n";
        "#
    ));
    // libcurl reports `CURLINFO_SCHEME` lowercased ("https"), which is also what PHP
    // hands back for the same transfer.
    assert_eq!(out, "hello-tls\n0\n200\nhttps\n");
}

/// The SAME fixture with verification left ON is rejected: `curl_exec()` answers `false`,
/// with libcurl's own certificate `CURLcode` and a non-empty message.
///
/// THE CA BUNDLE IS SUPPLIED BY THE TEST, which is what makes this hermetic. Left to its
/// default, libcurl consults whatever CA path was baked in at BUILD time; on a machine
/// where that path does not exist the transfer still fails, but with
/// `CURLE_SSL_CACERT_BADFILE` (77, "could not read the bundle") rather than
/// `CURLE_PEER_FAILED_VERIFICATION` (60, "the certificate is untrusted") — the next test
/// in this file pins exactly that. Pointing `CURLOPT_CAINFO` at a bundle that is a REAL,
/// parseable certificate but not the one the fixture presents removes the build machine
/// from the question entirely: libcurl can always read it, and the fixture's self-signed
/// certificate is never in it, so the answer is deterministically 60.
///
/// This is the half that proves the success case above is not succeeding by accident: if
/// verification were quietly disabled everywhere, this test would pass a transfer through.
#[test]
fn https_transfer_fails_with_peer_verification() {
    if skip_without_curl_native("https_transfer_fails_with_peer_verification") {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let bundle = unrelated_ca_bundle();
    let bundle_path = bundle.display();
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, true);
        curl_setopt($ch, CURLOPT_CAINFO, "{bundle_path}");
        $body = curl_exec($ch);
        echo $body === false ? "F" : "X";
        echo curl_errno($ch) === CURLE_SSL_CACERT ? "V" : curl_errno($ch);
        echo strlen(curl_error($ch)) > 0 ? "M" : "N";
        echo "\n";
        "#
    ));
    assert_eq!(out, "FVM\n");
}

/// `CURLOPT_CAINFO` is a plain string option that reaches libcurl: pointed at a file that
/// is not a certificate bundle, the same transfer fails with libcurl's CA-file `CURLcode`
/// instead of succeeding — which is how a string option that must affect TLS setup is
/// proved to have been applied rather than swallowed.
#[test]
fn https_cainfo_reaches_the_tls_backend() {
    if skip_without_curl_native("https_cainfo_reaches_the_tls_backend") {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, true);
        curl_setopt($ch, CURLOPT_CAINFO, "/nonexistent-elephc-curl-ca-bundle.pem");
        $body = curl_exec($ch);
        echo $body === false ? "F" : "X";
        echo curl_errno($ch), "\n";
        "#
    ));
    // 77 is CURLE_SSL_CACERT_BADFILE: libcurl could not read the CA bundle it was told to
    // use, which is only reachable if CURLOPT_CAINFO really arrived at the TLS backend.
    assert_eq!(out, "F77\n");
}

/// A copied handle keeps its TLS options: `curl_copy_handle()` duplicates the
/// verification settings alongside everything else, so the copy completes the same
/// handshake the original would have.
#[test]
fn https_options_survive_copy_handle() {
    if skip_without_curl_native("https_options_survive_copy_handle") {
        return;
    }
    let server = LocalHttpsServer::spawn();
    let url = server.url("/secure");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, false);
        curl_setopt($ch, CURLOPT_SSL_VERIFYHOST, 0);
        $copy = curl_copy_handle($ch);
        echo curl_exec($copy), "\n";
        echo curl_getinfo($copy, CURLINFO_HTTP_CODE), "\n";
        "#
    ));
    assert_eq!(out, "hello-tls\n200\n");
}
