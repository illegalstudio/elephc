//! Purpose:
//! Verifies JSON output completes without corrupting isolated handler processes.
//!
//! Called from:
//! - `tests/web_tests.rs` through Rust's integration test harness.
//!
//! Key details:
//! - Pool reuse and disposable-child reaping detect crashes after successful output.
//! - Fixture and server guards clean up on success and assertion failure.

use super::*;

/// Removes this test's generated sources, binaries, cache, and diagnostic log.
struct JsonFixture(PathBuf);

impl Drop for JsonFixture {
    /// Deletes only the fixture directory owned by this test.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Exercises minimal and nested JSON output, then checks handler lifecycle health.
fn assert_isolated_json_output(mode: &str) {
    let fixture = JsonFixture(make_test_dir("web_isolated_json"));
    let flag = format!("--web-isolation={mode}");
    let binary = compile_web_with_flags(&fixture.0, r#"<?php
header('Content-Type: application/json');
if ($_SERVER['REQUEST_URI'] === '/minimal') {
    echo json_encode(['pid' => getmypid()], JSON_THROW_ON_ERROR);
} else {
    echo json_encode([
        'pid' => getmypid(),
        'data' => ['message' => 'hello world', 'items' => [1, 2, 3]],
        'ok' => true,
    ], JSON_THROW_ON_ERROR);
}
"#, "app", &["--strict-php", &flag]);
    let diagnostics_path = fixture.0.join("server.stderr");
    let diagnostics = fs::File::create(&diagnostics_path).unwrap();
    let addr = format!("127.0.0.1:{}", free_port());
    let mut command = Command::new(&binary);
    command.args(["--listen", &addr, "--workers", "1", "--handler-concurrency", "1"]);
    if mode == "pool" {
        command.args(["--max-handler-requests", "0"]);
    }
    let mut server = command.stderr(Stdio::from(diagnostics)).spawn_group().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect(&addr).is_err() {
        assert!(server.leader_state() == LeaderState::Running && Instant::now() < deadline,
            "{mode}: server did not start: {}", fs::read_to_string(&diagnostics_path).unwrap());
        std::thread::sleep(Duration::from_millis(25));
    }
    let worker = wait_for_direct_child_count(server.id(), 1)[0];
    let broker = wait_for_direct_child_count(worker, 1)[0];
    let mut initial_pid = None;
    for request in 0..8 {
        let minimal = request % 2 == 0;
        let path = if minimal { "/minimal" } else { "/application" };
        let response = http_get_with_timeout(&addr, path, Duration::from_secs(5)).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{mode}: {response}");
        let json: serde_json::Value = serde_json::from_str(raw_response_body(&response))
            .unwrap_or_else(|error| panic!("{mode}: incomplete JSON: {error}: {response}"));
        let pid = json["pid"].as_u64().expect("numeric handler PID") as u32;
        if minimal {
            assert_eq!(json, serde_json::json!({"pid": pid}));
        } else {
            assert_eq!(json, serde_json::json!({"pid": pid,
                "data": {"message": "hello world", "items": [1, 2, 3]}, "ok": true}));
        }
        if mode == "pool" {
            let expected = *initial_pid.get_or_insert(pid);
            assert_eq!(pid, expected, "JSON output replaced a pool handler");
            assert_eq!(wait_for_direct_child_count(broker, 1), vec![expected]);
        } else {
            wait_for_processes_to_exit(&[pid]);
        }
        let diagnostics = fs::read_to_string(&diagnostics_path).unwrap();
        assert!(!diagnostics.contains("terminated by signal")
            && !diagnostics.contains("exited with status"), "{mode}: {diagnostics}");
    }
    server.kill().unwrap();
    server.wait().unwrap();
    let diagnostics = fs::read_to_string(diagnostics_path).unwrap();
    assert!(!diagnostics.contains("terminated by signal")
        && !diagnostics.contains("exited with status"), "{mode}: {diagnostics}");
}

/// JSON output must keep one pool child healthy across minimal and nested responses.
#[test]
fn web_isolated_json_output_keeps_pool_handler_healthy() {
    assert_isolated_json_output("pool");
}

/// Disposable JSON handlers must exit successfully after completing their response.
#[test]
fn web_isolated_json_output_exits_request_handlers_cleanly() {
    assert_isolated_json_output("request");
}
