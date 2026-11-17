//! Purpose:
//! Transaction monitoring in a library: a `--with-apm --apm-collector` cdylib
//! counts every call its host makes into an export, and sends the counts to
//! `elephc monitor --apm --listen`.
//!
//! Called from:
//! - The library integration test binary, with the actual compiler and a C host.
//!
//! Key details:
//! - The host is the one `test_cdylib_builds_and_host_calls_exports` runs, so
//!   the trampolines are checked to leave every export's result, status,
//!   string output and stack-passed argument exactly as they were.
//! - The library sends from `elephc_shutdown`, so the collector has the
//!   counts as soon as the host is done.

use super::*;
use std::time::{Duration, Instant};

/// Picks an ephemeral localhost port by binding :0 and releasing it.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Waits until something accepts connections on `address`.
fn wait_listening(address: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while std::net::TcpStream::connect(address).is_err() {
        assert!(Instant::now() < deadline, "nothing listens on {address}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The collector process, stopped however the test ends: a failed assertion
/// must not leave it running (it held the harness's output pipe open).
struct Collector(std::process::Child);

impl Collector {
    /// Asks the collector to stop (SIGTERM: it writes what it holds) and waits.
    fn stop(mut self) -> std::process::ExitStatus {
        Command::new("kill").arg("-TERM").arg(self.0.id().to_string()).status().unwrap();
        let status = self.0.wait().expect("the collector stops");
        std::mem::forget(self);
        status
    }
}

impl Drop for Collector {
    /// Kills the collector when the test ends early.
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The columns of `route`'s row in a transactions table.
fn row<'a>(table: &'a str, route: &str) -> Vec<&'a str> {
    let line = table
        .lines()
        .find(|line| line.starts_with(route))
        .unwrap_or_else(|| panic!("no row for {route}:\n{table}"));
    line[route.len()..].split_whitespace().collect()
}

/// Every call the host makes is a `CALL <export>` transaction, failed when
/// it returned a non-zero status; the host sees exactly what it saw without
/// monitoring.
#[test]
fn test_cdylib_export_calls_are_transactions() {
    let dir = make_test_dir("elephc_cdylib_apm");
    fs::write(dir.join("auth.php"), EXPORT_PHP).unwrap();
    let address = format!("127.0.0.1:{}", free_port());
    let output = elephc_command(&dir)
        .args(["--emit", "cdylib", "--with-apm", "--apm-collector", &address, "auth.php"])
        .output()
        .expect("failed to run elephc");
    assert!(output.status.success(), "cdylib compilation failed:\n{}", String::from_utf8_lossy(&output.stderr));
    let lib_path = dir.join(shared_lib_name("auth"));
    let key = lib_path.with_extension("key");
    assert!(key.exists(), "a --with-apm build writes its key beside it: {key:?}");

    let store = dir.join("store");
    let collector = Collector(
        Command::new(elephc_bin())
            .args(["monitor", "--apm", "--listen", &address, "--resolution", "1", "--key"])
            .arg(&key)
            .arg("--store")
            .arg(&store)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to start the collector"),
    );
    wait_listening(&address);

    let host = compile_c_host(&dir, HOST_C, "host");
    let run = Command::new(&host).arg(&lib_path).output().expect("failed to run the C host");
    assert!(
        run.status.success(),
        "C host run failed (exit {:?}):\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "42 0 1\n", "the trampolines change no result");

    // The counts arrive with the host's shutdown; give the collector a moment,
    // then stop it so it writes what it holds.
    std::thread::sleep(Duration::from_millis(1500));
    let stopped = collector.stop();
    assert!(stopped.success(), "the collector exits cleanly: {stopped:?}");

    let browse = Command::new(elephc_bin())
        .args(["monitor", "--apm", "--store"])
        .arg(&store)
        .output()
        .expect("failed to browse the store");
    let table = String::from_utf8_lossy(&browse.stdout).to_string();
    assert!(browse.status.success(), "{table}\n{}", String::from_utf8_lossy(&browse.stderr));
    assert_eq!(row(&table, "CALL add_i64")[0], "2", "{table}");
    assert_eq!(row(&table, "CALL validate_token")[0], "2", "{table}");
    assert_eq!(row(&table, "CALL fixed_label")[0], "1", "{table}");
    // A thrown exception, and an invalid argument, are failed calls.
    let thrown = row(&table, "CALL scalar_throw");
    assert_eq!(thrown[0], "1", "{table}");
    assert!(thrown.contains(&"100.0%"), "the throw failed the call: {table}");
    let composed = row(&table, "CALL compose_label");
    assert_eq!(composed[0], "2", "{table}");
    assert!(composed.contains(&"50.0%"), "one of the two calls failed: {table}");
    fs::remove_dir_all(&dir).ok();
}
