//! Purpose:
//! Verifies web environment bootstrap cost, environment access, and request isolation.
//!
//! Called from:
//! - `tests/web_tests.rs` through Rust's integration test harness.
//!
//! Key details:
//! - Compares allocation counters in fresh workers with controlled process environments.
//! - Fixture and server guards clean up on success and assertion failure.

use super::*;

/// Removes only this test's fixture directory after its server guards have stopped.
struct EnvironmentFixture(PathBuf);

impl Drop for EnvironmentFixture {
    /// Cleans up source, binaries, assembly, and the fixture's isolated runtime cache.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Starts one persistent worker with a controlled environment and optional unused entries.
fn environment_server(binary: &Path, addr: &str, extra_environment: bool) -> ServerGuard {
    let mut command = Command::new(binary);
    command.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("ELEPHC_WEB_TEST_ENV", "present")
        .args(["--listen", addr, "--workers", "1"]);
    if extra_environment {
        for index in 0..128 {
            command.env(format!("ELEPHC_UNUSED_ENV_{index}"), "x".repeat(128));
        }
    }
    let guard = command.spawn_group().expect("spawn environment fixture");
    wait_until_ready(addr);
    guard
}

/// Returns a fresh worker's first response under one of the controlled environments.
fn environment_first_response(binary: &Path, extra_environment: bool) -> String {
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = environment_server(binary, &addr, extra_environment);
    raw_response_body(&http_get(&addr, "/")).trim().to_owned()
}

/// Unused environment size adds no native allocations, while getenv still sees the process value.
#[test]
fn web_environment_unused_entries_do_not_allocate() {
    let dir = EnvironmentFixture(make_test_dir("web_environment_unused"));
    for (stem, prefix) in [
        ("direct", ""),
        ("literal_callable", "$fn = 'strlen'; $fn('ok');"),
    ] {
        let source = format!(r#"<?php
{prefix}
$allocations = memory_get_allocations();
echo $allocations, ':', getenv('ELEPHC_WEB_TEST_ENV');
"#);
        let binary = compile_web(&dir.0, &source, stem);
        let small = environment_first_response(&binary, false);
        let large = environment_first_response(&binary, true);
        eprintln!("{stem} environment allocations: minimal={small}, 128 extra entries={large}");
        assert!(small.ends_with(":present"), "{small}");
        assert_eq!(small, large, "unused environment entries must not allocate ({stem})");
    }
}

/// Direct and callback access sees a fresh environment on every persistent-worker request.
#[test]
fn web_environment_observed_values_reset_between_requests() {
    let dir = EnvironmentFixture(make_test_dir("web_environment_observed"));
    let binary = compile_web(&dir.0, r#"<?php
function env_value(): string { return (string) $_ENV['ELEPHC_WEB_TEST_ENV']; }
$reader = fn() => env_value();
echo $reader(), ':', isset($_ENV['request_local']) ? 'leaked' : 'fresh';
echo ':', isset($_ENV['ELEPHC_UNUSED_ENV_0']) ? 'restored' : 'missing';
$_ENV['ELEPHC_WEB_TEST_ENV'] = 'changed';
$_ENV['request_local'] = 'local';
unset($_ENV['ELEPHC_UNUSED_ENV_0']);
echo ':', getenv('ELEPHC_WEB_TEST_ENV');
"#, "app");
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = environment_server(&binary, &addr, true);
    for _ in 0..3 {
        let response = http_get(&addr, "/");
        assert_eq!(raw_response_body(&response).trim(), "present:fresh:restored:present");
    }
}

/// Environment reads confined to an autoloaded class retain a fresh copy on each request.
#[test]
fn web_environment_autoloaded_observer_retains_population() {
    let dir = EnvironmentFixture(make_test_dir("web_environment_autoloaded"));
    fs::create_dir(dir.0.join("src")).unwrap();
    fs::write(dir.0.join("composer.json"), r#"{"autoload":{"psr-4":{"WebEnv\\":"src/"}}}"#).unwrap();
    fs::write(dir.0.join("src/Reader.php"), r#"<?php
namespace WebEnv;
class Reader {
    public static function value(): string {
        $before = (string) $_ENV['ELEPHC_WEB_TEST_ENV'];
        $_ENV['ELEPHC_WEB_TEST_ENV'] = 'changed';
        return $before;
    }
}
"#).unwrap();
    let binary = compile_web(&dir.0, "<?php echo \\WebEnv\\Reader::value();", "app");
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = environment_server(&binary, &addr, false);
    for _ in 0..3 {
        let response = http_get(&addr, "/");
        assert_eq!(raw_response_body(&response).trim(), "present");
    }
}

/// Runtime-sourced eval retains environment population even without an inline $_ENV reference.
#[test]
fn web_environment_opaque_eval_retains_population() {
    let dir = EnvironmentFixture(make_test_dir("web_environment_eval"));
    let binary = compile_web(&dir.0, r#"<?php
$code = (string) getenv('ELEPHC_WEB_TEST_ENV');
eval($code);
"#, "app");
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = Command::new(&binary)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("ELEPHC_WEB_TEST_ENV", "echo $_ENV['ELEPHC_EVAL_VALUE']; $_ENV['ELEPHC_EVAL_VALUE'] = 'changed';")
        .env("ELEPHC_EVAL_VALUE", "eval-present")
        .args(["--listen", &addr, "--workers", "1"])
        .spawn_group()
        .expect("spawn eval environment fixture");
    wait_until_ready(&addr);
    for _ in 0..2 {
        let response = http_get(&addr, "/");
        assert_eq!(raw_response_body(&response).trim(), "eval-present");
    }
}

/// Literal and computed eval target names retain fresh environment values in AOT fragments.
#[test]
fn web_environment_computed_eval_retains_population() {
    let dir = EnvironmentFixture(make_test_dir("web_environment_computed_eval"));
    for (stem, binding) in [
        ("literal", "$fn = 'eval';"),
        ("concat", "$fn = 'ev' . 'al';"),
        ("propagated", "$prefix = 'ev'; $fn = $prefix . 'al';"),
    ] {
        let source = format!(r#"<?php
{binding}
$fn('echo $_ENV["ELEPHC_WEB_TEST_ENV"]; $_ENV["ELEPHC_WEB_TEST_ENV"] = "changed";');
"#);
        let binary = compile_web(&dir.0, &source, stem);
        let addr = format!("127.0.0.1:{}", free_port());
        let _server = environment_server(&binary, &addr, false);
        for _ in 0..2 {
            let response = http_get(&addr, "/");
            assert_eq!(raw_response_body(&response).trim(), "present", "{stem}");
        }
    }
}
