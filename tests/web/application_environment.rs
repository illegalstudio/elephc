//! Purpose:
//! Verifies environment demand and fresh request state in persistent application mode.
//!
//! Called from:
//! - The environment module in `tests/web_tests.rs`.
//!
//! Key details:
//! - Allocation samples exclude the complete entrywise environment copy during bootstrap.
//! - Controlled environments and one worker expose source/autoload demand and request leaks.

use super::*;

/// Samples request allocations in a fresh application worker after its bootstrap completes.
fn application_allocation_responses(binary: &Path, extra_environment: bool) -> Vec<String> {
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = environment_server(binary, &addr, extra_environment);
    (0..3).map(|_| {
        let response = http_get(&addr, "/");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        raw_response_body(&response).trim().to_owned()
    }).collect()
}

/// Application requests allocate independently of unused environment size, including literal calls.
#[test]
fn web_application_environment_unused_entries_do_not_allocate() {
    let dir = EnvironmentFixture(make_test_dir("web_application_environment_unused"));
    for (stem, prefix) in [
        ("direct", ""),
        ("literal_callable", "$fn = 'strlen'; $fn('ok');"),
    ] {
        let source = format!(r#"<?php
class Application implements Elephc\Web\WorkerApplication {{
    private int $previous = 0;
    public function __construct() {{ $this->previous = memory_get_allocations(); }}
    public function handle(Elephc\Web\Request $request): void {{
        {prefix}
        $now = memory_get_allocations();
        echo $now - $this->previous, ':', getenv('ELEPHC_WEB_TEST_ENV');
        $this->previous = $now;
    }}
    public function shutdown(): void {{}}
}}
Elephc\Web\Worker::register(new Application());
"#);
        let binary = compile_web_with_flags(&dir.0, &source, stem, &["--web-lifecycle=application"]);
        let small = application_allocation_responses(&binary, false);
        let large = application_allocation_responses(&binary, true);
        eprintln!("{stem} application allocations: minimal={small:?}, 128 extra entries={large:?}");
        assert!(small.iter().all(|response| response.ends_with(":present")), "{small:?}");
        assert_eq!(small, large, "unused environment entries must not allocate per request ({stem})");
    }
}

/// Bootstrap environment snapshots survive fresh request copies and reset after a throwing request.
#[test]
fn web_application_environment_observers_preserve_bootstrap_and_isolation() {
    let dir = EnvironmentFixture(make_test_dir("web_application_environment_observed"));
    let binary = compile_web_with_flags(&dir.0, r#"<?php
function environment_value(): string { return (string) $_ENV['ELEPHC_WEB_TEST_ENV']; }
class Application implements Elephc\Web\WorkerApplication {
    public function __construct(public string $config, public array $snapshot) {}
    public function handle(Elephc\Web\Request $request): void {
        $reader = fn() => environment_value();
        echo $this->config, ':', $this->snapshot['ELEPHC_WEB_TEST_ENV'], ':', $reader(), ':',
            isset($_ENV['request_local']) ? 'leaked' : 'fresh', ':',
            isset($_ENV['ELEPHC_UNUSED_ENV_0']) ? 'restored' : 'missing', ':',
            isset($_SERVER['bootstrap_local']) ? 'leaked' : 'fresh', ':',
            getenv('ELEPHC_WEB_TEST_ENV');
        $_ENV['ELEPHC_WEB_TEST_ENV'] = 'changed';
        $_ENV['request_local'] = 'local';
        unset($_ENV['ELEPHC_UNUSED_ENV_0']);
        if ($request->path === '/fail') { throw new RuntimeException('request failure'); }
    }
    public function shutdown(): void {}
}
$config = (string) $_ENV['ELEPHC_WEB_TEST_ENV'] . '/' . (string) $_SERVER['ELEPHC_WEB_TEST_ENV'];
$_SERVER['bootstrap_local'] = 'bootstrap';
Elephc\Web\Worker::register(new Application($config, $_ENV));
"#, "app", &["--web-lifecycle=application", "--strict-php"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = environment_server(&binary, &addr, true);
    for path in ["/", "/fail", "/", "/"] {
        let response = http_get(&addr, path);
        if path == "/fail" {
            assert!(response.starts_with("HTTP/1.1 500"), "{response}");
        } else {
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
            assert_eq!(raw_response_body(&response).trim(), "present/present:present:present:fresh:restored:fresh:present");
        }
    }
}

/// A SERVER-only bootstrap reader retains its process values while request metadata stays fresh.
#[test]
fn web_application_environment_server_only_bootstrap_is_preserved() {
    let dir = EnvironmentFixture(make_test_dir("web_application_environment_bootstrap"));
    let binary = compile_web_with_flags(&dir.0, r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    public function __construct(public string $config) {}
    public function handle(Elephc\Web\Request $request): void {
        echo $this->config, ':', $_SERVER['REQUEST_METHOD'], ':',
            isset($_SERVER['request_local']) ? 'leaked' : 'fresh';
        $_SERVER['request_local'] = 'local';
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application((string) $_SERVER['ELEPHC_WEB_TEST_ENV']));
"#, "app", &["--web-lifecycle=application", "--strict-php"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = environment_server(&binary, &addr, false);
    for _ in 0..3 {
        let response = http_get(&addr, "/");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert_eq!(raw_response_body(&response).trim(), "present:GET:fresh");
    }
}

/// Environment demand confined to Composer-loaded application code reaches request preparation.
#[test]
fn web_application_environment_autoloaded_observer_retains_population() {
    let dir = EnvironmentFixture(make_test_dir("web_application_environment_autoloaded"));
    fs::create_dir(dir.0.join("src")).unwrap();
    fs::write(dir.0.join("composer.json"), r#"{"autoload":{"psr-4":{"AppEnv\\":"src/"}}}"#).unwrap();
    fs::write(dir.0.join("src/Application.php"), r#"<?php
namespace AppEnv;
class Application implements \Elephc\Web\WorkerApplication {
    private string $config = '';
    public function __construct() { $this->config = (string) $_ENV['ELEPHC_WEB_TEST_ENV']; }
    public function handle(\Elephc\Web\Request $request): void {
        echo $this->config, ':', $_ENV['ELEPHC_WEB_TEST_ENV'], ':',
            isset($_ENV['request_local']) ? 'leaked' : 'fresh';
        $_ENV['ELEPHC_WEB_TEST_ENV'] = 'changed';
        $_ENV['request_local'] = 'local';
    }
    public function shutdown(): void {}
}
"#).unwrap();
    let binary = compile_web_with_flags(&dir.0,
        "<?php \\Elephc\\Web\\Worker::register(new \\AppEnv\\Application());",
        "app", &["--web-lifecycle=application", "--strict-php"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = environment_server(&binary, &addr, false);
    for _ in 0..3 {
        let response = http_get(&addr, "/");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert_eq!(raw_response_body(&response).trim(), "present:present:fresh");
    }
}

/// Session option scratch storage works without an application environment observer.
#[test]
fn web_application_environment_omission_preserves_session_storage() {
    let dir = EnvironmentFixture(make_test_dir("web_application_environment_session"));
    let binary = compile_web_with_flags(&dir.0, r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    public function handle(Elephc\Web\Request $request): void {
        $before = session_get_cookie_params();
        session_set_cookie_params(['lifetime' => 123, 'path' => '/application']);
        $after = session_get_cookie_params();
        session_id('app-environment');
        session_start(['use_cookies' => false, 'cache_limiter' => '']);
        $count = (int) ($_SESSION['count'] ?? 0) + 1;
        $_SESSION['count'] = $count;
        echo $count, ':', session_status() === PHP_SESSION_ACTIVE ? 'active' : 'inactive', ':',
            $before['path'], ':', $after['path'], ':', $after['lifetime'];
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#, "app", &["--web-lifecycle=application", "--strict-php"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = Command::new(&binary)
        .env_clear().env("PATH", "/usr/bin:/bin")
        .env("ELEPHC_SESSION_SAVE_PATH", &dir.0)
        .args(["--listen", &addr, "--workers", "1"])
        .spawn_group().expect("spawn application session fixture");
    wait_until_ready(&addr);
    for count in 1..=3 {
        let response = http_get(&addr, "/");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert_eq!(raw_response_body(&response).trim(), format!("{count}:active:/:/application:123"));
    }
}
