//! Purpose:
//! Reproduces archived CI bridge timestamps through real REPL CLI subprocesses.
//!
//! Called from:
//! - `codegen::repl` integration tests on supported desktop hosts.
//!
//! Key details:
//! - Copies only the compiler and the archives recorded by a real host receipt.
//! - Old timestamps and a rejecting Cargo stub expose unintended source rebuilds.
//! - Cold and warm CLI sessions use the archived artifacts without rerunning other cache tests.

use super::{fs, stdout, Fixture};
use crate::support::elephc_cli_bin;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::fs::MetadataExt;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

/// Archived bridges remain reusable even when the checkout sources have newer timestamps.
#[test]
fn test_repl_prebuilt_cache_ignores_checkout_timestamps() {
    let fixture = Fixture::new();
    assert_eq!(stdout(&fixture.run("6 * 7\n", &[])), "int(42)\n");
    let host = fixture.hosts().pop().unwrap();
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(host.parent().unwrap().join("receipt.json")).unwrap(),
    ).unwrap();
    let target = fixture.0.join("archived-target");
    let debug = target.join("debug");
    fs::create_dir_all(&debug).unwrap();
    for dependency in receipt["dependencies"].as_array().unwrap() {
        let source = std::path::Path::new(dependency["path"].as_str().unwrap());
        let archive = debug.join(source.file_name().unwrap());
        fs::copy(source, &archive).unwrap();
        fs::File::options().write(true).open(archive).unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1)).unwrap();
    }
    let compiler = debug.join("elephc");
    fs::copy(elephc_cli_bin(), &compiler).unwrap();

    let tools = fixture.0.join("tools");
    fs::create_dir(&tools).unwrap();
    let cargo = tools.join("cargo");
    fs::write(&cargo, "#!/bin/sh\nprintf 'attempt\\n' >> \"$ELEPHC_REPL_BUILD_ATTEMPTS\"\nexit 1\n").unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o700)).unwrap();
    let attempts = fixture.0.join("cargo-attempts");
    let paths = std::iter::once(tools).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ).collect::<Vec<_>>();
    let cache = fixture.0.join("archived-cache");
    let run = |source: &str| {
        let mut command = Command::new(&compiler);
        command.args(["repl", "--php-version", "8.5"])
            .current_dir(&fixture.0)
            .env("CARGO_TARGET_DIR", "archived-target")
            .env("XDG_CACHE_HOME", &cache)
            .env("CARGO_NET_OFFLINE", "true")
            .env("ELEPHC_REPL_BUILD_ATTEMPTS", &attempts)
            .env("PATH", std::env::join_paths(&paths).unwrap())
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        for dependency in receipt["dependencies"].as_array().unwrap() {
            let name = std::path::Path::new(dependency["path"].as_str().unwrap())
                .file_name().unwrap().to_str().unwrap();
            let library = name.strip_prefix("lib").unwrap().strip_suffix(".a").unwrap();
            command.env(format!("{}_LIB_DIR", library.to_ascii_uppercase()), &debug);
        }
        let mut child = command.spawn().expect("start archived REPL");
        child.stdin.take().unwrap().write_all(source.as_bytes()).unwrap();
        child.wait_with_output().expect("wait for archived REPL")
    };
    assert_eq!(stdout(&run("$n = 42;\n")), "int(42)\n");
    let hosts = fs::read_dir(cache.join("elephc/repl")).unwrap().filter_map(|entry| {
        let host = entry.unwrap().path().join("host");
        host.is_file().then_some(host)
    }).collect::<Vec<_>>();
    assert_eq!(hosts.len(), 1);
    let original = fs::metadata(&hosts[0]).unwrap();
    assert_eq!(stdout(&run("isset($n)\n")), "bool(false)\n");
    assert_eq!(fs::metadata(&hosts[0]).unwrap().ino(), original.ino(), "archived warm session rebuilt its host");
    assert!(!attempts.exists(), "prebuilt CLI sessions attempted to rebuild a bridge");
}
