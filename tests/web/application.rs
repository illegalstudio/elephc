//! Purpose:
//! Exercises retained application ownership and fresh requests over real HTTP.
//!
//! Called from:
//! - `tests/web_tests.rs` through Rust's integration test harness.
//!
//! Key details:
//! - One worker makes PID, identity, counters, and bounded heap growth observable.
//! - Fixture directories and server processes are removed on success and panic.

use super::*;

#[path = "application_review.rs"]
mod review;

#[path = "application_gc.rs"]
mod gc;

#[path = "application_gc_writes.rs"]
mod gc_writes;

#[path = "application_gc_paths.rs"]
mod gc_paths;

#[path = "application_callable_cycles.rs"]
mod callable_cycles;

#[path = "application_native_cycles.rs"]
mod native_cycles;

#[path = "application_coroutines.rs"]
mod coroutines;

#[path = "application_extern_callbacks.rs"]
mod extern_callbacks;

/// Removes only this test's fixture tree after the server guard has stopped.
struct Fixture(PathBuf);

impl Drop for Fixture {
    /// Removes sources, generated artifacts, and logs owned by this fixture.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Compiles a persistent application with an isolated log shared by its forked workers.
fn application(fixture: &Fixture, source: &str) -> PathBuf {
    let log = fixture.0.join("lifecycle.log").to_string_lossy().replace('\\', "\\\\").replace('\'', "\\'");
    compile_web_with_flags(
        &fixture.0, &source.replace("__LOG__", &log), "app", &["--web-lifecycle=application", "--strict-php"],
    )
}

/// Returns a completed successful response body, naming the complete response on failure.
fn body(addr: &str, path: &str) -> String {
    let response = http_get(addr, path);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    raw_response_body(&response).trim().to_string()
}

/// Reads resident memory from the standard Unix process-status tool on executable test hosts.
fn resident_bytes(pid: &str) -> u64 {
    let output = Command::new("ps").args(["-o", "rss=", "-p", pid]).output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().parse::<u64>().unwrap() * 1024
}

/// Application identity and newly retained graphs survive churn and an exceptional request.
#[test]
fn web_application_retains_graphs_and_isolates_requests_after_throw() {
    let fixture = Fixture(make_test_dir("web_application_graphs"));
    let source = r#"<?php
namespace Demo;
use RuntimeException;
use Elephc\Web\Request;
use Elephc\Web\Worker;
use Elephc\Web\WorkerApplication;
class SavedItem { public function __construct(public string $value) {} }
class RequestTemporary {
    public static int $destroyed = 0;
    public function __destruct() { self::$destroyed++; }
}
class Application implements WorkerApplication {
    private int $count = 0;
    private string $text = '';
    private array $values = [];
    private ?SavedItem $saved = null;
    private ?Request $firstRequest = null;
    private array $firstQuery = [];
    public function __construct() { file_put_contents('__LOG__', 'boot:' . getmypid() . "\n", FILE_APPEND); }
    public function handle(Request $request): void {
        $this->count++;
        $marker = new RequestTemporary();
        if ($this->count === 1) {
            $this->text = str_repeat('abc', 64);
            $this->values = ['nested' => [new SavedItem(str_repeat('z', 40))]];
            $this->saved = new SavedItem('retained');
            $this->firstRequest = $request;
            $this->firstQuery = $_GET;
        }
        $temporary = array_fill(0, 1024, str_repeat('temporary', 12));
        if (isset($_GET['fail'])) { throw new RuntimeException('request failure'); }
        echo getmypid(), ':', spl_object_id($this), ':', $this->count, ':',
            strlen($this->text), ':', $this->saved->value, ':',
            $this->values['nested'][0]->value, ':', isset($_GET['leak']) ? 'leaked' : (isset($_GET['mark']) ? 'marked' : 'fresh'),
            ':', $request->path, ':', RequestTemporary::$destroyed, ':',
            $this->firstRequest->path, ':', count($this->firstQuery), ':',
            strlen($request->body), ':', $this->firstRequest->body;
        $_GET['leak'] = 'discard';
    }
    public function shutdown(): void {
        file_put_contents('__LOG__', 'stop:' . getmypid() . "\n", FILE_APPEND);
        throw new RuntimeException('shutdown failure must still release the application');
    }
}
Worker::register(new Application());
"#;
    let binary = application(&fixture, source);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&binary, &addr, "1");
    let first_response = http_request(&addr, "POST", "/first?mark=1", &[("Content-Type", "text/plain")], "retained-body");
    assert!(first_response.starts_with("HTTP/1.1 200"), "{first_response}");
    let first = raw_response_body(&first_response).trim().to_string();
    let fields: Vec<_> = first.split(':').collect();
    assert_eq!(&fields[2..], &["1", "192", "retained", &"z".repeat(40), "marked", "/first", "0", "/first", "1", "13", "retained-body"]);
    let identity = format!("{}:{}", fields[0], fields[1]);
    let failed = http_get(&addr, "/?fail=1");
    assert!(failed.starts_with("HTTP/1.1 500"), "{failed}");
    for count in 3..=20 {
        let response = body(&addr, "/next");
        assert_eq!(response, format!("{identity}:{count}:192:retained:{}:fresh:/next:{}:/first:1:0:retained-body", "z".repeat(40), count - 1));
    }
    server.kill().unwrap();
    server.wait().unwrap();
    let log = fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap();
    assert_eq!(log.lines().collect::<Vec<_>>(), [format!("boot:{}", fields[0]), format!("stop:{}", fields[0])]);
}

/// Replacing bounded application state and discarding large temporaries reaches a steady heap.
#[test]
fn web_application_memory_is_bounded_with_retained_replacements() {
    let fixture = Fixture(make_test_dir("web_application_memory"));
    let binary = application(&fixture, r#"<?php
use Elephc\Web\Request;
use Elephc\Web\Worker;
use Elephc\Web\WorkerApplication;
class TemporaryCycle {
    public ?TemporaryCycle $self = null;
    public function __construct() { $this->self = $this; }
}
class Application implements WorkerApplication {
    private array $cache = [];
    private int $count = 0;
    public function handle(Request $request): void {
        $this->count++;
        if ($this->count > 1 && strlen($this->cache['value']) !== 2048) {
            throw new RuntimeException('retained JSON value lost its storage');
        }
        $this->cache = ['value' => str_repeat('x', 2048), 'sequence' => $this->count];
        $encoded = json_encode($this->cache, JSON_THROW_ON_ERROR);
        $this->cache = json_decode($encoded, true, 512, JSON_THROW_ON_ERROR);
        $temporary = array_fill(0, 1000, str_repeat('temporary', 20));
        $cycle = new TemporaryCycle();
        ob_start();
        $ambientBefore = count(stream_context_get_options(stream_context_get_default()));
        stream_context_set_option(stream_context_get_default(), 'http', 'header', 'X-Probe: 1');
        stream_context_set_option(stream_context_get_default(), 'http', 'user_agent', 'Probe-' . $this->count);
        $cfg = ['http' => ['header' => 'one']];
        $next = [];
        foreach ($cfg as $name => $settings) {
            $updated = [];
            foreach ($settings as $key => $value) { $updated[$key] = $value; }
            $updated['ua'] = 'two';
            $next[$name] = $updated;
        }
        $captured = str_repeat('capture', 32);
        $firstContext = stream_context_create(['http' => ['header' => str_repeat('h', 256)]], [
            'notification' => static function(int $code) use ($captured): void { echo $captured; },
        ]);
        $secondContext = stream_context_create(['http' => ['method' => 'POST']], [
            'notification' => static function(int $code) use ($captured): void { echo $captured; },
        ]);
        if ($this->count % 17 === 0) { throw new RuntimeException('discard request temporaries'); }
        echo $this->count, ':', strlen($this->cache['value']), ':', memory_get_usage(), ':', getmypid(), ':', $ambientBefore;
    }
    public function shutdown(): void {}
}
Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut warm = 0;
    let mut warm_resident = 0;
    for count in 1..=2000 {
        if count % 17 == 0 {
            let response = http_get(&addr, "/");
            assert!(response.starts_with("HTTP/1.1 500"), "{response}");
            continue;
        }
        let response = body(&addr, "/");
        let fields: Vec<_> = response.split(':').collect();
        assert_eq!(fields[0], count.to_string());
        assert_eq!(fields[1], "2048");
        assert_eq!(fields[4], "0", "stream context options leaked between requests");
        let live: usize = fields[2].parse().unwrap();
        if count == 20 { warm = live; warm_resident = resident_bytes(fields[3]); }
        if count > 20 { assert!(live <= warm + 4096, "request {count}: warm={warm}, live={live}"); }
        if count == 2000 {
            let resident = resident_bytes(fields[3]);
            eprintln!("web application memory after 2000 requests: live {warm} -> {live} bytes, RSS {warm_resident} -> {resident} bytes");
            assert!(resident <= warm_resident + 4 * 1024 * 1024, "resident bytes: warm={warm_resident}, end={resident}");
        }
    }
}

/// A planned recycle executes shutdown and a new PID runs a new bootstrap exactly once.
#[test]
fn web_application_recycle_bootstraps_new_worker() {
    let fixture = Fixture(make_test_dir("web_application_recycle"));
    let binary = application(&fixture, r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    private int $count = 0;
    public function __construct() { file_put_contents('__LOG__', 'boot:' . getmypid() . "\n", FILE_APPEND); }
    public function handle(Elephc\Web\Request $request): void { echo getmypid(), ':', ++$this->count; }
    public function shutdown(): void { file_put_contents('__LOG__', 'stop:' . getmypid() . "\n", FILE_APPEND); }
}

Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server_with_args(&binary, &addr, "1", &["--max-requests", "2"]);
    let first = body(&addr, "/");
    let first_pid = first.split(':').next().unwrap();
    assert_eq!(body(&addr, "/"), format!("{first_pid}:2"));
    let mut next = String::new();
    for _ in 0..80 {
        let response = try_http_get(&addr, "/");
        if response.starts_with("HTTP/1.1 200") { next = raw_response_body(&response).trim().to_string(); break; }
        std::thread::sleep(Duration::from_millis(25));
    }
    let (next_pid, count) = next.split_once(':').expect("new worker response");
    assert_ne!(next_pid, first_pid);
    assert_eq!(count, "1");
    server.kill().unwrap();
    server.wait().unwrap();
    let log = fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap();
    assert_eq!(log.lines().collect::<Vec<_>>(), [
        format!("boot:{first_pid}"), format!("stop:{first_pid}"),
        format!("boot:{next_pid}"), format!("stop:{next_pid}"),
    ]);
}

/// Lifecycle validation rejects unsupported combinations before touching PHP sources.
#[test]
fn web_application_cli_rejects_unsupported_lifecycles() {
    let fixture = Fixture(make_test_dir("web_application_cli"));
    for (flags, expected) in [
        (vec!["--web-lifecycle=application"], "--web-lifecycle requires --web"),
        (vec!["--web", "--web-lifecycle=unknown"], "expected request|application"),
        (vec!["--web", "--web-isolation=pool", "--web-lifecycle=application"], "requires --web-isolation=worker"),
        (vec!["--web", "--web-isolation=request", "--web-lifecycle=application"], "requires --web-isolation=worker"),
    ] {
        let result = Command::new(elephc_bin()).args(flags).arg(fixture.0.join("missing.php")).output().unwrap();
        assert!(!result.status.success());
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains(expected), "{stderr}");
    }
}

/// Registration enforces the nominal request/shutdown interface during type checking.
#[test]
fn web_application_registration_is_typed() {
    let fixture = Fixture(make_test_dir("web_application_type"));
    let source = fixture.0.join("invalid.php");
    fs::write(&source, "<?php Elephc\\Web\\Worker::register(new stdClass());").unwrap();
    let result = Command::new(elephc_bin()).args(["--web", "--web-lifecycle=application"]).arg(source).output().unwrap();
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("WorkerApplication"), "{stderr}");
}

/// Failed and missing registrations never admit HTTP requests or trigger application shutdown.
#[test]
fn web_application_failed_bootstrap_never_serves_requests() {
    let partial = r#"
class PartialApplication implements Elephc\Web\WorkerApplication {
    public function handle(Elephc\Web\Request $request): void {}
    public function shutdown(): void { file_put_contents('__LOG__', "shutdown\n", FILE_APPEND); }
    public function __destruct() { file_put_contents('__LOG__', "destroy\n", FILE_APPEND); }
}
Elephc\Web\Worker::register(new PartialApplication());
"#;
    for bootstrap in [
        "throw new RuntimeException('bootstrap failed');".to_string(),
        "usleep(1100000); throw new RuntimeException('slow bootstrap failed');".to_string(),
        "echo 'no registration';".to_string(),
        format!("{partial} throw new RuntimeException('after registration');"),
        format!("{partial} return;"),
        format!("{partial} Elephc\\Web\\Worker::register(new PartialApplication());"),
        format!("{partial} Elephc\\Web\\Worker::completeBootstrap(); return;"),
    ] {
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
}

/// Streams created during bootstrap and retained from a request outlive request resource pruning.
#[test]
fn web_application_retains_streams_and_releases_temporary_resources() {
    let fixture = Fixture(make_test_dir("web_application_streams"));
    let source = r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    private mixed $bootStream = null;
    private mixed $savedStream = null;
    private int $count = 0;
    public function __construct() { $this->bootStream = fopen('__LOG__', 'a'); }
    public function handle(Elephc\Web\Request $request): void {
        $this->count++;
        if ($this->count === 1) { $this->savedStream = fopen('__LOG__', 'a'); }
        $temporary = fopen('__LOG__', 'a');
        fwrite($temporary, 't');
        fwrite($this->bootStream, 'b');
        fwrite($this->savedStream, 's');
        if ($this->count % 7 === 0) { throw new RuntimeException('release temporary stream'); }
        echo $this->count, ':', count(get_resources('stream'));
    }
    public function shutdown(): void { fclose($this->bootStream); fclose($this->savedStream); }
}
Elephc\Web\Worker::register(new Application());
"#;
    let binary = application(&fixture, source);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&binary, &addr, "1");
    let mut resource_count = String::new();
    for count in 1..=40 {
        if count % 7 == 0 {
            assert!(http_get(&addr, "/").starts_with("HTTP/1.1 500"));
        } else {
            let response = body(&addr, "/");
            let (sequence, resources) = response.split_once(':').unwrap();
            assert_eq!(sequence, count.to_string());
            if count == 1 { resource_count = resources.to_string(); }
            assert_eq!(resources, resource_count, "{response}");
        }
    }
    server.kill().unwrap();
    server.wait().unwrap();
    assert_eq!(fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap(), "tbs".repeat(40));
}

/// Both lifecycles keep preload in the master and retire inherited PHP objects before serving.
#[test]
fn web_application_preload_remains_master_only() {
    for lifecycle in ["request", "application"] {
        let fixture = Fixture(make_test_dir("web_application_preload"));
        let log = fixture.0.join("preload.log");
        let preload = fixture.0.join("preload.php");
        fs::write(&preload, format!(r#"<?php
class PreloadedValue {{ public function value(): int {{ return 42; }} }}
global $preloaded;
$preloaded = new PreloadedValue();
file_put_contents('{}', (string) getmypid());
"#, log.display())).unwrap();
        let handler = if lifecycle == "request" {
            r#"<?php global $preloaded; echo $preloaded === null ? 'clean:' : 'stale:'; echo (new PreloadedValue())->value();"#
        } else {
            r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    public function handle(Elephc\Web\Request $request): void {
        global $preloaded;
        echo $preloaded === null ? 'clean:' : 'stale:';
        echo (new PreloadedValue())->value();
    }
    public function shutdown(): void {}
}

Elephc\Web\Worker::register(new Application());
"#
        };
        let preload_option = format!("opcache.preload={}", preload.display());
        let lifecycle_option = format!("--web-lifecycle={lifecycle}");
        let binary = compile_web_with_flags(&fixture.0, handler, "app", &[
            &lifecycle_option, "--ini", "opcache.enable_cli=1", "--ini", &preload_option,
        ]);
        let addr = format!("127.0.0.1:{}", free_port());
        let mut server = spawn_server_with_args(&binary, &addr, "1", &["--max-requests", "2"]);
        assert_eq!(body(&addr, "/"), "clean:42");
        assert_eq!(body(&addr, "/"), "clean:42");
        let mut response = String::new();
        for _ in 0..80 {
            response = try_http_get(&addr, "/");
            if response.starts_with("HTTP/1.1 200") { break; }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(raw_response_body(&response).trim(), "clean:42");
        assert_eq!(fs::read_to_string(log).unwrap(), server.id().to_string());
        server.kill().unwrap();
        server.wait().unwrap();
    }
}

/// The shipped portable application and native adapter compile and demonstrate independent requests.
#[test]
fn web_application_example_reuses_its_counter() {
    let fixture = Fixture(make_test_dir("web_application_example"));
    fs::write(fixture.0.join("application.php"), include_str!("../../examples/web-application/application.php")).unwrap();
    let binary = application(&fixture, include_str!("../../examples/web-application/main.php"));
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let first = body(&addr, "/one?value=abc");
    let second = body(&addr, "/two");
    assert_eq!(first.lines().next(), second.lines().next());
    assert!(first.contains("application requests=1\nprevious path=\ncurrent path=/one\nquery value=abc"), "{first}");
    assert!(second.contains("application requests=2\nprevious path=/one\ncurrent path=/two\nquery value=absent"), "{second}");
}

/// Ambient replacement preserves destructor global writes and contains a throwing capture release.
#[test]
fn web_application_context_replacement_preserves_destructor_effects() {
    let fixture = Fixture(make_test_dir("web_application_context_destructor"));
    let binary = application(&fixture, r#"<?php
global $mutations;
$mutations = 0;
class ContextOwner {
    public function __construct(private bool $throws) {}
    public function touch(): void {}
    public function __destruct() {
        global $mutations;
        $mutations++;
        if ($this->throws) { throw new RuntimeException('context capture destructor'); }
    }
}
function install_notification_owner(bool $throws): void {
    $owner = new ContextOwner($throws);
    stream_context_create([], [
        'notification' => static function(int $code) use ($owner): void { $owner->touch(); },
    ]);
}
class Application implements Elephc\Web\WorkerApplication {
    public function handle(Elephc\Web\Request $request): void {
        global $mutations;
        install_notification_owner(isset($_GET['throw']));
        $before = $mutations;
        $caught = 'ok';
        try {
            stream_context_set_params(stream_context_get_default(), [
                'notification' => static function(int $code): void {},
            ]);
        } catch (RuntimeException $error) { $caught = 'caught'; }
        echo $caught, ':', $mutations - $before;
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    assert_eq!(body(&addr, "/"), "ok:1");
    assert_eq!(body(&addr, "/?throw=1"), "caught:1");
    assert_eq!(body(&addr, "/"), "ok:1");
}
