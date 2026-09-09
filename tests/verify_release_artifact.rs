//! Purpose:
//! Fixture tests for `scripts/verify-release-artifact.sh`: a packed curl
//! archive is compile-probed after `native add curl` in the empty WORKDIR, a
//! packed xml archive after `native add libxml2`, archive-less capabilities
//! stay skipped, and a real link failure still fails.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (`cargo test --test verify_release_artifact`).
//!
//! Key details:
//! - The script unpacks into an empty prefix on purpose, so these tests ship a
//!   mock `elephc` inside a tarball rather than the real compiler.
//! - The mock accepts `native add curl` before `--with-curl` and `native add
//!   libxml2` before `--with-xml`; it never downloads or builds catalog sources.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Directory of this crate, used to locate the packaging probe script.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Isolated scratch directory unique across parallel test threads.
fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "elephc_verify_artifact_{}_{}_{:?}_{}",
        label,
        std::process::id(),
        std::thread::current().id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Writes a mock packaged `elephc` that implements the probe's exact calls.
///
/// `--print-capabilities` reports `tls` (archive-only), `curl` and `xml`
/// (archive plus managed native package), `regex` and `mysqli` (no archive).
/// `--with-tls` always "links". `--with-curl` fail-closes unless `native add
/// curl` has already created a project marker, and `--with-xml` likewise
/// unless `native add libxml2` has. A `broken` capability, when advertised,
/// always fails with a truncated-archive error so a real link failure still
/// FAILs without inventing a native add.
fn write_mock_elephc(path: &Path, advertise_broken: bool) {
    let broken_line = if advertise_broken {
        "echo -e 'bridge\\tbroken\\tlibelephc_broken.a'\n"
    } else {
        ""
    };
    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
state_dir="${{ELEPHC_MOCK_STATE:-$here/../.mock-state}}"
mkdir -p "$state_dir"
case "${{1:-}}" in
  --version)
    echo "elephc 0.0.0-fixture"
    ;;
  --print-capabilities)
    echo -e 'bridge\ttls\tlibelephc_tls.a'
    echo -e 'bridge\tcurl\tlibelephc_curl.a'
    echo -e 'bridge\txml\tlibelephc_xml.a'
    {broken_line}    echo -e 'capability\tregex'
    echo -e 'capability\tmysqli'
    ;;
  --with-tls)
    printf '#!/bin/sh\necho ok\n' > probe
    chmod +x probe
    ;;
  --with-curl)
    if [ ! -f "$state_dir/added-curl" ]; then
      echo "native project error: curl support requires managed native package curl" >&2
      echo "project: not found (searched from )" >&2
      echo "recovery: cd -- '' && elephc native add curl" >&2
      exit 1
    fi
    printf '#!/bin/sh\necho ok\n' > probe
    chmod +x probe
    ;;
  --with-xml)
    if [ ! -f "$state_dir/added-libxml2" ]; then
      echo "native project error: xml support requires managed native package libxml2" >&2
      echo "project: not found (searched from )" >&2
      echo "recovery: cd -- '' && elephc native add libxml2" >&2
      exit 1
    fi
    printf '#!/bin/sh\necho ok\n' > probe
    chmod +x probe
    ;;
  --with-broken)
    echo "ld: archive is truncated" >&2
    exit 1
    ;;
  native)
    if [ "${{2:-}}" = "add" ] && [ -n "${{3:-}}" ]; then
      touch "$state_dir/added-$3"
      echo "mock: native add $3"
      exit 0
    fi
    echo "unexpected native invocation: $*" >&2
    exit 1
    ;;
  *)
    echo "unexpected invocation: $*" >&2
    exit 1
    ;;
esac
"#
    );
    fs::write(path, script).expect("write mock elephc");
    let mut perms = fs::metadata(path).expect("stat mock elephc").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod mock elephc");
}

/// Packs the mock compiler and named archives into a `.tar.gz` the probe accepts.
fn pack_tarball(staging: &Path, tarball: &Path, advertise_broken: bool) {
    let pack = staging.join("pack");
    fs::create_dir_all(&pack).expect("create pack dir");
    write_mock_elephc(&pack.join("elephc"), advertise_broken);
    fs::write(pack.join("libelephc_tls.a"), b"tls-archive\n").expect("write tls archive");
    fs::write(pack.join("libelephc_curl.a"), b"curl-archive\n").expect("write curl archive");
    fs::write(pack.join("libelephc_xml.a"), b"xml-archive\n").expect("write xml archive");
    if advertise_broken {
        fs::write(pack.join("libelephc_broken.a"), b"broken-archive\n")
            .expect("write broken archive");
    }
    let status = Command::new("tar")
        .args(["czf"])
        .arg(tarball)
        .args(["-C"])
        .arg(&pack)
        .args(["elephc", "libelephc_tls.a", "libelephc_curl.a", "libelephc_xml.a"])
        .args(if advertise_broken {
            vec!["libelephc_broken.a"]
        } else {
            vec![]
        })
        .status()
        .expect("run tar");
    assert!(status.success(), "tar failed: {status}");
}

/// Runs `scripts/verify-release-artifact.sh` against `tarball` and returns output plus success.
fn run_probe(tarball: &Path) -> (bool, String) {
    let script = repo_root().join("scripts/verify-release-artifact.sh");
    let output = Command::new("bash")
        .arg(&script)
        .arg(tarball)
        .output()
        .expect("run verify-release-artifact.sh");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let combined = format!("{stdout}{stderr}");
    (output.status.success(), combined)
}

/// A tarball that packs `libelephc_curl.a` must `ok` curl after add-first, not FAIL.
#[test]
fn packed_curl_is_ok_after_native_add() {
    let dir = scratch("curl_ok");
    let tarball = dir.join("elephc-fixture.tar.gz");
    pack_tarball(&dir, &tarball, false);

    let (ok, log) = run_probe(&tarball);
    assert!(ok, "probe should pass after add-first native add; log:\n{log}");
    assert!(
        log.contains("ok    bridge curl (libelephc_curl.a)"),
        "curl must be reported ok after native add then one --with-curl; log:\n{log}"
    );
    assert!(
        log.contains("adding managed native package curl before --with-curl"),
        "curl must native-add first, before the compile; log:\n{log}"
    );
    assert!(
        log.contains("mock: native add curl"),
        "the packaged binary must have seen native add curl; log:\n{log}"
    );
    assert!(
        !log.contains("running packaged 'native add curl'"),
        "must not log the old fail-then-retry line; log:\n{log}"
    );
    assert!(
        !log.contains("requires managed native package"),
        "add-first must not compile-fail before native add; log:\n{log}"
    );
    assert!(
        log.contains("ok    bridge xml (libelephc_xml.a)"),
        "xml must be reported ok after native add libxml2 then one --with-xml; log:\n{log}"
    );
    assert!(
        log.contains("adding managed native package libxml2 before --with-xml"),
        "xml must native-add libxml2 first, before the compile; log:\n{log}"
    );
    assert!(
        log.contains("mock: native add libxml2"),
        "the packaged binary must have seen native add libxml2; log:\n{log}"
    );
    assert!(
        log.contains("ok    bridge tls (libelephc_tls.a)"),
        "archive-only bridges must still pass; log:\n{log}"
    );
    assert!(
        log.contains("skip  capability regex (needs no archive from this tarball)"),
        "regex skip must be unchanged; log:\n{log}"
    );
    assert!(
        log.contains("skip  capability mysqli (needs no archive from this tarball)"),
        "mysqli skip must be unchanged; log:\n{log}"
    );
    assert!(
        !log.contains("FAIL"),
        "no capability should fail; log:\n{log}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// A truncated-archive compile must FAIL without inventing a native add for that capability.
#[test]
fn truncated_archive_still_fails_without_inventing_native_add() {
    let dir = scratch("broken");
    let tarball = dir.join("elephc-fixture.tar.gz");
    pack_tarball(&dir, &tarball, true);

    let (ok, log) = run_probe(&tarball);
    assert!(!ok, "a truncated-archive style failure must fail the probe; log:\n{log}");
    assert!(
        log.contains("FAIL  bridge broken: --with-broken did not link"),
        "broken must fail as a real link error; log:\n{log}"
    );
    assert!(
        log.contains("ld: archive is truncated"),
        "the original linker error must be shown; log:\n{log}"
    );
    assert!(
        !log.contains("native add broken") && !log.contains("mock: native add broken"),
        "a truncated archive must not invent a native add; log:\n{log}"
    );
    assert!(
        log.contains("ok    bridge curl (libelephc_curl.a)")
            && log.contains("mock: native add curl"),
        "curl should still native-add first in the same run; log:\n{log}"
    );
    assert!(
        log.contains("ok    bridge xml (libelephc_xml.a)")
            && log.contains("mock: native add libxml2"),
        "xml should still native-add libxml2 first in the same run; log:\n{log}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The probe's xml branch mirrors curl's exactly: the add happens BEFORE the one
/// compile, and a tarball whose binary refuses `--with-xml` without the package is
/// therefore never "retried" from a FAIL line. Proved by running the mock directly:
/// `--with-xml` fail-closes until `native add libxml2` has been seen.
#[test]
fn mock_with_xml_fails_closed_until_native_add_libxml2() {
    let dir = scratch("xml_fail_closed");
    let mock = dir.join("elephc");
    write_mock_elephc(&mock, false);
    let state = dir.join("state");

    let before = Command::new(&mock)
        .arg("--with-xml")
        .env("ELEPHC_MOCK_STATE", &state)
        .current_dir(&dir)
        .output()
        .expect("run mock --with-xml");
    assert!(!before.status.success(), "--with-xml must fail before native add libxml2");
    let stderr = String::from_utf8_lossy(&before.stderr);
    assert!(
        stderr.contains("requires managed native package libxml2")
            && stderr.contains("elephc native add libxml2"),
        "mock must mirror the compiler's diagnostic; stderr:\n{stderr}"
    );

    let add = Command::new(&mock)
        .args(["native", "add", "libxml2"])
        .env("ELEPHC_MOCK_STATE", &state)
        .current_dir(&dir)
        .status()
        .expect("run mock native add libxml2");
    assert!(add.success());
    let after = Command::new(&mock)
        .arg("--with-xml")
        .env("ELEPHC_MOCK_STATE", &state)
        .current_dir(&dir)
        .status()
        .expect("run mock --with-xml after add");
    assert!(after.success(), "--with-xml must link once libxml2 is added");
    assert!(dir.join("probe").is_file());

    let _ = fs::remove_dir_all(&dir);
}

/// Nightly still packs the curl bridge; the verify hole is the probe, not #730.
#[test]
fn nightly_still_packs_elephc_curl() {
    let body = fs::read_to_string(repo_root().join(".github/workflows/nightly.yml"))
        .expect("read nightly workflow");
    assert!(
        body.contains("-p elephc-curl"),
        "nightly.yml must still build elephc-curl; do not revert #730"
    );
    assert!(
        body.contains("Cache managed native"),
        "nightly verify-artifact must cache ~/.cache/elephc/native so a cold \
         native add curl can finish"
    );
}
