//! Purpose:
//! Checks each failed application-bootstrap mode before the worker can serve requests.
//!
//! Called from:
//! - `tests/web/application.rs` through the web integration harness.
//!
//! Key details:
//! - Each test compiles one program so the CI watchdog bounds one failure mode.
//! - Startup, diagnostics, destruction, and connection checks retain their original limits.

use super::*;

/// Provides a registered application whose destruction is visible in an isolated fixture log.
const PARTIAL_APPLICATION: &str = r#"
class PartialApplication implements Elephc\Web\WorkerApplication {
    public function handle(Elephc\Web\Request $request): void {}
    public function shutdown(): void { file_put_contents('__LOG__', "shutdown\n", FILE_APPEND); }
    public function __destruct() { file_put_contents('__LOG__', "destroy\n", FILE_APPEND); }
}
Elephc\Web\Worker::register(new PartialApplication());
"#;

/// Preserves the complete failed-startup contract for one independently compiled bootstrap.
fn assert_failed_bootstrap(bootstrap: &str) {
    let fixture = Fixture(make_test_dir("web_application_boot_failure"));
    let binary = application(&fixture, &format!("<?php {bootstrap}"));
    let addr = format!("127.0.0.1:{}", free_port());
    let stderr_path = fixture.0.join("bootstrap.stderr");
    let stderr_file = fs::File::create(&stderr_path).unwrap();
    let mut server = ServerGuard::new(Command::new(binary).args(["--listen", &addr, "--workers", "1"]).stdout(Stdio::null()).stderr(Stdio::from(stderr_file)).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(25);
    let status = loop {
        assert!(TcpStream::connect(&addr).is_err(), "failed bootstrap accepted a connection");
        if let Some(status) = server.try_wait().unwrap() { break status; }
        assert!(Instant::now() < deadline, "startup crash-loop protection did not stop the master");
        std::thread::sleep(Duration::from_millis(25));
    };
    assert!(!status.success(), "failed application startup must report failure");
    let diagnostics = fs::read_to_string(stderr_path).unwrap();
    if bootstrap.contains("throw new RuntimeException") {
        assert!(diagnostics.contains("bootstrap RuntimeException: "), "{diagnostics}");
        assert!(diagnostics.contains(" at ") && diagnostics.contains("app.php:"), "{diagnostics}");
    }
    if bootstrap.contains("PartialApplication") {
        let log = fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap();
        assert!(!log.is_empty());
        assert!(log.lines().all(|line| line == "destroy"), "{log}");
    }
}

/// An immediate bootstrap exception must never open the listener.
#[test]
fn web_application_failed_bootstrap_never_serves_requests_throw() {
    assert_failed_bootstrap("throw new RuntimeException('bootstrap failed');");
}

/// A delayed bootstrap exception must remain guarded during every startup attempt.
#[test]
fn web_application_failed_bootstrap_never_serves_requests_slow_throw() {
    assert_failed_bootstrap("usleep(1100000); throw new RuntimeException('slow bootstrap failed');");
}

/// Finishing without registration must report failure without accepting a connection.
#[test]
fn web_application_failed_bootstrap_never_serves_requests_missing_registration() {
    assert_failed_bootstrap("echo 'no registration';");
}

/// A registered application must be destroyed when later bootstrap code throws.
#[test]
fn web_application_failed_bootstrap_never_serves_requests_throw_after_registration() {
    assert_failed_bootstrap(&format!("{PARTIAL_APPLICATION} throw new RuntimeException('after registration');"));
}

/// Returning after registration must not publish an incompletely bootstrapped application.
#[test]
fn web_application_failed_bootstrap_never_serves_requests_return_after_registration() {
    assert_failed_bootstrap(&format!("{PARTIAL_APPLICATION} return;"));
}

/// Duplicate registration must destroy partial applications without opening the listener.
#[test]
fn web_application_failed_bootstrap_never_serves_requests_duplicate_registration() {
    assert_failed_bootstrap(&format!("{PARTIAL_APPLICATION} Elephc\\Web\\Worker::register(new PartialApplication());"));
}

/// A premature completion call must not bypass the worker's bootstrap transaction.
#[test]
fn web_application_failed_bootstrap_never_serves_requests_early_completion() {
    assert_failed_bootstrap(&format!("{PARTIAL_APPLICATION} Elephc\\Web\\Worker::completeBootstrap(); return;"));
}
