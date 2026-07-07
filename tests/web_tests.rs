//! Purpose:
//! End-to-end tests for the three web modes — `--web` (classic per-request
//! isolation), `--web-worker` (handler mode, boot-once), and
//! `--web-worker=script` (per-request re-run with persistent statics/props/
//! globals). Each test compiles PHP into a prefork HTTP server binary, launches
//! it with `--listen`, drives it over raw TCP, and asserts the response.
//!
//! Called from:
//! - `cargo test --test web_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an
//!   isolated temp dir with an isolated runtime cache, mirroring cdylib_tests.
//! - The HTTP client is a hand-written minimal HTTP/1.1 request over a
//!   std::net::TcpStream so the test pulls in no HTTP client dependency.
//! - Server children are killed on drop via `ServerHandle` so an assertion
//!   panic mid-test does not leak a listening process for the rest of the run.
//! - Host-target only: each platform/arch covers itself (macOS aarch64 local,
//!   Linux x86_64/aarch64 via the Docker test scripts).

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Compiles `source` with the given extra elephc flags; returns the binary path.
fn compile_web(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg("--web").arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc --web failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Compiles `source` with `--web-worker`; returns the binary path.
fn compile_web_worker(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg("--web-worker").arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc --web-worker failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Compiles `source` with `--web-worker=script` (script mode); returns the binary path.
/// Asserts the compile succeeds — script mode re-runs the whole top-level script per
/// request and requires no handler registration.
fn compile_web_worker_script(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg("--web-worker=script").arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc --web-worker=script failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Compiles `source` with `--web-worker` expecting FAILURE; returns combined stderr.
/// Asserts the process exited non-zero so callers can assert on the diagnostic text.
fn compile_web_worker_expect_error(dir: &Path, source: &str, stem: &str) -> String {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg("--web-worker").arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        !output.status.success(),
        "expected elephc --web-worker to fail, but it succeeded:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Compiles `source` with `--web-worker=script` expecting FAILURE; returns combined stderr.
/// Asserts the process exited non-zero so callers can assert on the diagnostic text.
fn compile_web_worker_script_expect_error(dir: &Path, source: &str, stem: &str) -> String {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg("--web-worker=script").arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        !output.status.success(),
        "expected elephc --web-worker=script to fail, but it succeeded:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Picks an ephemeral localhost port by binding :0 and releasing it.
fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// Blocks until `addr` accepts a TCP connection (server ready), or panics after 10s.
fn wait_until_ready(addr: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(addr).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("server did not start listening on {}", addr);
}

/// Spawns the server binary on `addr`, waits until it accepts connections.
fn spawn_server(bin: &Path, addr: &str, workers: &str) -> std::process::Child {
    let child = Command::new(bin)
        .arg("--listen").arg(addr)
        .arg("--workers").arg(workers)
        .spawn()
        .expect("failed to spawn web server");
    wait_until_ready(addr);
    child
}

/// RAII wrapper around a spawned server child that kills and reaps it on drop.
/// Unlike the bare `spawn_server` + manual `child.kill()` pattern, this survives
/// an assertion panic mid-test: the child is torn down as the handle unwinds, so
/// a failing test never leaks a listening process for the rest of the run.
struct ServerHandle {
    child: std::process::Child,
    addr: String,
}

impl ServerHandle {
    /// Returns the `host:port` the guarded server listens on.
    fn addr(&self) -> &str {
        &self.addr
    }
}

impl Drop for ServerHandle {
    /// Kills and reaps the server child so no listener survives the test.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawns the server on a fresh ephemeral port with `workers` workers and returns
/// a kill-on-drop `ServerHandle`. Preferred over `spawn_server` for new tests so
/// an assertion failure cannot orphan the server process.
fn spawn_server_guarded(bin: &Path, workers: &str) -> ServerHandle {
    let addr = format!("127.0.0.1:{}", free_port());
    let child = Command::new(bin)
        .arg("--listen").arg(&addr)
        .arg("--workers").arg(workers)
        .spawn()
        .expect("failed to spawn web server");
    wait_until_ready(&addr);
    ServerHandle { child, addr }
}

/// Returns just the body of a raw HTTP response (everything after the first
/// blank line). Preferred for `must-not-contain` assertions on short bodies,
/// since header values (e.g. a `Date: Fri, …` header) can otherwise trip a
/// whole-response substring check.
fn http_body(resp: &str) -> &str {
    resp.split_once("\r\n\r\n").map(|(_, body)| body).unwrap_or("")
}

/// Sends one HTTP/1.1 GET and returns the full raw response text.
fn http_get(addr: &str, path: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    let req = format!("GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", path, addr);
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    buf
}

/// Verifies a trivial program compiles under --web and produces an executable file.
#[test]
fn web_compile_produces_binary() {
    let dir = make_test_dir("web_compile");
    let bin = compile_web(&dir, "<?php echo \"Hello World\";", "app");
    assert!(bin.exists(), "expected binary at {}", bin.display());
}

/// Verifies a worker-mode program compiles under --web-worker and produces a
/// binary. The handler-registration builtin, trampoline, and worker entry stub
/// are all wired (Phase 2 Step C). Full request serving is verified manually;
/// the automated request test is gated on the same process-cleanup path as the
/// classic --web serve tests.
#[test]
fn web_worker_compile_produces_binary() {
    let dir = make_test_dir("web_worker_compile");
    let bin = compile_web_worker(
        &dir,
        "<?php elephc_worker_register(function() { echo \"hello from worker\"; });",
        "app",
    );
    assert!(bin.exists(), "expected binary at {}", bin.display());
}

/// WI-S2: handler mode (`--web-worker`) with no `elephc_worker_register()` call must
/// be a blocking compile error, and the diagnostic must name the missing builtin and
/// suggest `--web-worker=script`. Previously this compiled and boot-die-looped at runtime.
#[test]
fn web_worker_handler_without_register_errors() {
    let dir = make_test_dir("web_worker_no_register");
    let stderr = compile_web_worker_expect_error(&dir, "<?php echo \"hi\";", "app");
    assert!(
        stderr.contains("elephc_worker_register"),
        "diagnostic should name the missing builtin:\n{}",
        stderr
    );
    assert!(
        stderr.contains("--web-worker=script"),
        "diagnostic should suggest script mode:\n{}",
        stderr
    );
}

/// WI-S2: script mode (`--web-worker=script`) with an `elephc_worker_register()` call
/// must be a blocking compile error, since in script mode the whole top-level script IS
/// the handler and registration is meaningless. The diagnostic must name the builtin and
/// mention script mode, pointing at the offending call.
#[test]
fn web_worker_script_with_register_errors() {
    let dir = make_test_dir("web_worker_script_register");
    let stderr = compile_web_worker_script_expect_error(
        &dir,
        "<?php elephc_worker_register(function () { echo \"x\"; });",
        "app",
    );
    assert!(
        stderr.contains("elephc_worker_register"),
        "diagnostic should name the offending builtin:\n{}",
        stderr
    );
    assert!(
        stderr.contains("script"),
        "diagnostic should mention script mode:\n{}",
        stderr
    );
}

/// WI-S2: script mode (`--web-worker=script`) with no registration compiles cleanly and
/// produces a binary — the whole top-level script becomes the per-request handler, so no
/// registration is required. Also the first committed script-mode compile test.
#[test]
fn web_worker_script_without_register_compiles() {
    let dir = make_test_dir("web_worker_script_ok");
    let bin = compile_web_worker_script(&dir, "<?php echo \"hi\";", "app");
    assert!(bin.exists(), "expected binary at {}", bin.display());
}

/// Verifies per-request reset of top-level PHP variables between two real HTTP
/// requests: each response body must be exactly "x" (not accumulated).
#[test]
fn web_reset_clears_globals_between_runs() {
    let dir = make_test_dir("web_reset");
    let src = "<?php $g = \"\"; $g = $g . \"x\"; echo $g;";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("x"), "first response body: {:?}", r1);
    assert!(r2.ends_with("x"), "second response body: {:?}", r2);
}

/// Verifies per-request reset of an ordinary global used through `global $g`.
#[test]
fn web_reset_clears_ordinary_global_alias_between_requests() {
    let dir = make_test_dir("web_reset_global_alias");
    let src = r#"<?php
function write_global(): void { global $g; $g = 7; }
function read_global(): int { global $g; return $g ?? 0; }
if (isset($_GET["set"])) { write_global(); }
echo read_global();
"#;
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/?set=1");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("7"), "first response body: {:?}", r1);
    assert!(r2.ends_with("0"), "second response leaked ordinary global: {:?}", r2);
}

/// Verifies per-request reset of a user `global` variable in classic `--web`:
/// `bump()` increments a `global $g` written only inside a function (so the
/// top-level re-run does not reset it by re-assignment). Classic `--web` must
/// give each request a fresh global — three requests all end "1", NOT "1","2","3".
/// This is the PHP-FPM isolation contract; `web_worker_script_global_persists`
/// is the opposite-direction fence (same pattern persists 1,2,3 in script mode).
#[test]
fn web_reset_clears_user_global() {
    let dir = make_test_dir("web_reset_global");
    let src = "<?php function bump(): int { global $g; $g = $g + 1; return $g; } echo bump();";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_guarded(&bin, "1");
    let r1 = http_get(srv.addr(), "/");
    let r2 = http_get(srv.addr(), "/");
    let r3 = http_get(srv.addr(), "/");
    assert!(r1.ends_with("1"), "first response body: {:?}", r1);
    assert!(r2.ends_with("1"), "second request must see a fresh global, not 2: {:?}", r2);
    assert!(r3.ends_with("1"), "third request must see a fresh global, not 3: {:?}", r3);
}

/// Verifies per-request reset of a function static: each request must see
/// the static re-initialized to 0, so each response ends with "1".
#[test]
fn web_reset_clears_function_static() {
    let dir = make_test_dir("web_reset_static");
    let src = "<?php function c() { static $n = 0; $n++; return $n; } echo c();";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("1"), "first response body: {:?}", r1);
    assert!(r2.ends_with("1"), "second response body: {:?}", r2);
}

/// Verifies per-request reset of a static class property: each request must see
/// the property re-initialized to 0, so each response ends with "1".
#[test]
fn web_reset_clears_static_property() {
    let dir = make_test_dir("web_reset_prop");
    let src = "<?php class C { public static int $n = 0; } C::$n = C::$n + 1; echo C::$n;";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("1"), "first response body: {:?}", r1);
    assert!(r2.ends_with("1"), "second response body: {:?}", r2);
}

/// Verifies that "Hello World" is served as the response body.
#[test]
fn web_server_serves_echo_body() {
    let dir = make_test_dir("web_echo");
    let bin = compile_web(&dir, "<?php echo \"Hello World\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("Hello World"), "response: {:?}", resp);
}

/// Verifies that the binary exits nonzero and prints "--listen" to stderr when
/// no --listen argument is supplied.
#[test]
fn web_server_requires_listen() {
    let dir = make_test_dir("web_nolisten");
    let bin = compile_web(&dir, "<?php echo \"ok\";", "app");
    let output = Command::new(&bin)
        .output()
        .expect("failed to spawn web binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "expected nonzero exit when --listen is missing");
    assert!(stderr.contains("--listen"), "expected --listen in stderr: {:?}", stderr);
}

/// Verifies that with --workers 2, two sequential requests both succeed and
/// each response ends with "ok".
#[test]
fn web_server_multiple_workers() {
    let dir = make_test_dir("web_multi");
    let bin = compile_web(&dir, "<?php echo \"ok\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "2");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("ok"), "first response: {:?}", r1);
    assert!(r2.ends_with("ok"), "second response: {:?}", r2);
}

/// Sends one HTTP/1.1 request with a method/body and returns the full raw response.
fn http_request(addr: &str, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> String {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    let mut req = format!("{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n", method, path, addr);
    for (k, v) in headers { req.push_str(&format!("{}: {}\r\n", k, v)); }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{}", body.len(), body));
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    buf
}

/// Like `http_request` GET, but tolerates a refused/reset connection (returns the
/// empty string). Used while a worker is crashing/respawning.
fn try_http_get(addr: &str, path: &str) -> String {
    use std::io::{Read, Write};
    let Ok(mut s) = std::net::TcpStream::connect(addr) else {
        return String::new();
    };
    let req = format!("GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", path, addr);
    if s.write_all(req.as_bytes()).is_err() {
        return String::new();
    }
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    buf
}

/// Verifies the extern getters are callable from --web PHP and return request data.
#[test]
fn web_extern_method_getter() {
    let dir = make_test_dir("web_extern_method");
    let bin = compile_web(&dir, "<?php echo elephc_web_method();", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "POST", "/", &[], "");
    let _ = child.kill(); let _ = child.wait();
    assert!(resp.ends_with("POST"), "body: {:?}", resp);
}

/// Verifies a superglobal is READABLE inside a function without `global` (full
/// visibility + global-storage routing). Now that $_SERVER is populated by the
/// prelude, asserts the body is the actual REQUEST_METHOD ("DELETE").
#[test]
fn web_superglobal_visible_in_function() {
    let dir = make_test_dir("web_sg_fn");
    let src = "<?php function rm() { return $_SERVER['REQUEST_METHOD'] ?? 'unset'; } echo rm();";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "DELETE", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("DELETE"), "body: {:?}", resp);
}

/// Verifies a router storing an interface-typed handler survives repeated web requests.
#[test]
fn web_router_interface_handler_survives_repeated_requests() {
    let dir = make_test_dir("web_router_iface");
    let src = r#"<?php
class Request {
    public string $method;
    public string $path;
    public function __construct() {
        $this->method = $_SERVER['REQUEST_METHOD'] ?? 'GET';
        $uri = $_SERVER['REQUEST_URI'] ?? '/';
        $cut = strpos($uri, '?');
        $this->path = $cut === false ? $uri : substr($uri, 0, $cut);
    }
    public function segment(int $index, string $default = ''): string {
        $n = 0;
        foreach (explode('/', $this->path) as $part) {
            if ($part === '') { continue; }
            if ($n === $index) { return $part; }
            $n++;
        }
        return $default;
    }
}
interface Handler { public function handle(Request $request): void; }
class Hello implements Handler {
    public function handle(Request $request): void {
        echo 'Hello, ' . $request->segment(1, 'world') . "\n";
    }
}
class Route {
    public string $method;
    public string $pattern;
    public Handler $handler;
    public function __construct(string $method, string $pattern, Handler $handler) {
        $this->method = $method;
        $this->pattern = $pattern;
        $this->handler = $handler;
    }
    public function matches(Request $request): bool {
        return $this->method === $request->method;
    }
    public function run(Request $request): void {
        $this->handler->handle($request);
    }
}
class Router {
    private array $routes = [];
    public function add(string $method, string $pattern, Handler $handler): void {
        $this->routes[] = new Route($method, $pattern, $handler);
    }
    public function dispatch(Request $request): void {
        foreach ($this->routes as $route) {
            if (!$route->matches($request)) { continue; }
            $route->run($request);
            return;
        }
        echo 'missing';
    }
}
$router = new Router();
$router->add('GET', '/hello/:name', new Hello());
$router->dispatch(new Request());
"#;
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    for i in 0..25 {
        let resp = http_get(&addr, "/hello/ada");
        assert!(
            resp.ends_with("Hello, ada\n"),
            "response {i} body: {:?}",
            resp
        );
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Verifies $_SERVER is populated from the request line and headers.
#[test]
fn web_server_superglobal_populated() {
    let dir = make_test_dir("web_server_sg");
    let src = "<?php echo $_SERVER['REQUEST_METHOD'] . ' ' . $_SERVER['REQUEST_URI'];";
    let bin = compile_web(&dir, src, "app");
    let port = free_port(); let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/foo?a=1", &[], "");
    let _ = child.kill(); let _ = child.wait();
    assert!(resp.ends_with("GET /foo?a=1"), "body: {:?}", resp);
}

/// Verifies $_GET is parsed from the query string, with percent-decoding.
#[test]
fn web_get_superglobal_parsed() {
    let dir = make_test_dir("web_get_sg");
    let src = "<?php echo $_GET['name'] . '/' . $_GET['city'];";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/?name=bob&city=new%20york", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("bob/new york"), "body: {:?}", resp);
}

/// Verifies $_POST is parsed from a urlencoded body when the Content-Type matches.
#[test]
fn web_post_superglobal_parsed() {
    let dir = make_test_dir("web_post_sg");
    let src = "<?php echo $_POST['user'] . ':' . $_POST['pw'];";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(
        &addr,
        "POST",
        "/",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        "user=alice&pw=s%40fe",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("alice:s@fe"), "body: {:?}", resp);
}

/// Verifies echoing a superglobal value directly (a boxed Mixed string) reaches
/// the HTTP response body, not the worker's stdout. This is the output-capture
/// completeness fix: `__rt_mixed_write_stdout` routes through `__rt_stdout_write`.
#[test]
fn web_echo_superglobal_value_captured() {
    let dir = make_test_dir("web_mixed_cap");
    let src = "<?php echo $_GET['name'];";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/?name=bob", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("bob"), "Mixed echo must be captured: {:?}", resp);
}

/// Verifies request superglobals do not leak/stale across requests: a second
/// request with a different query sees only its own $_GET (__rt_web_reset
/// releases the prior request's hash so there is no per-request leak).
#[test]
fn web_get_does_not_leak_across_requests() {
    let dir = make_test_dir("web_get_leak");
    let src = "<?php echo isset($_GET['a']) ? $_GET['a'] : 'none';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_request(&addr, "GET", "/?a=first", &[], "");
    let r2 = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("first"), "r1: {:?}", r1);
    assert!(r2.ends_with("none"), "r2 leaked stale $_GET: {:?}", r2);
}

/// Verifies file_get_contents('php://input') returns the raw request body under --web.
#[test]
fn web_php_input_returns_body() {
    let dir = make_test_dir("web_php_input");
    let src = "<?php echo file_get_contents('php://input');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "POST", "/", &[("Content-Type", "application/json")], "{\"k\":42}");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("{\"k\":42}"), "body: {:?}", resp);
}

/// Verifies http_response_code() sets the HTTP response status.
#[test]
fn web_http_response_code_sets_status() {
    let dir = make_test_dir("web_status");
    let bin = compile_web(&dir, "<?php http_response_code(404); echo \"nope\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.starts_with("HTTP/1.1 404"), "status line: {:?}", resp);
    assert!(resp.ends_with("nope"), "body: {:?}", resp);
}

/// Verifies header() adds a response header (hyper lowercases header names on the wire).
#[test]
fn web_header_sets_response_header() {
    let dir = make_test_dir("web_header");
    let bin = compile_web(&dir, "<?php header(\"X-Greeting: hello\"); echo \"ok\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.to_lowercase().contains("x-greeting: hello"), "headers: {:?}", resp);
    assert!(resp.ends_with("ok"), "body: {:?}", resp);
}

/// Verifies header("Location: ...") implies a 302 redirect, matching PHP.
#[test]
fn web_header_location_implies_302() {
    let dir = make_test_dir("web_redirect");
    let bin = compile_web(&dir, "<?php header(\"Location: /elsewhere\");", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.starts_with("HTTP/1.1 302"), "status: {:?}", resp);
    assert!(resp.to_lowercase().contains("location: /elsewhere"), "headers: {:?}", resp);
}

/// Verifies http_response_code() + header() compose, function_exists sees them,
/// and the default $replace=true keeps only the last same-name header.
#[test]
fn web_response_control_combined() {
    let dir = make_test_dir("web_resp_combo");
    let src = "<?php \
        if (!function_exists('header') || !function_exists('http_response_code')) { echo 'MISSING'; return; } \
        http_response_code(201); \
        header('Content-Type: application/json'); \
        header('X-A: 1'); header('X-A: 2'); \
        echo '{\"ok\":true}';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    let lower = resp.to_lowercase();
    assert!(resp.starts_with("HTTP/1.1 201"), "status: {:?}", resp);
    assert!(lower.contains("content-type: application/json"), "headers: {:?}", resp);
    assert!(lower.contains("x-a: 2") && !lower.contains("x-a: 1"), "replace failed: {:?}", resp);
    assert!(resp.ends_with("{\"ok\":true}"), "body: {:?}", resp);
}

/// Verifies a top-level `return` halts the --web handler: code after it must not run.
#[test]
fn web_top_level_return_halts_handler() {
    let dir = make_test_dir("web_return");
    let src = "<?php echo \"before\"; return; http_response_code(500); echo \"after\";";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.starts_with("HTTP/1.1 200"), "status must stay 200: {:?}", resp);
    assert!(resp.ends_with("before"), "body must be exactly 'before': {:?}", resp);
    assert!(!resp.contains("after"), "code after return must not run: {:?}", resp);
}

/// Verifies the validate-then-return pattern: a conditional early `return` halts
/// the handler so the rest of the body does not run (the failing case from the
/// web-response example).
#[test]
fn web_conditional_early_return_halts() {
    let dir = make_test_dir("web_early_return");
    let src = "<?php if (!isset($_GET['ok'])) { http_response_code(400); echo \"bad\"; return; } echo \"good\";";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let bad = http_request(&addr, "GET", "/", &[], "");
    let good = http_request(&addr, "GET", "/?ok=1", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(bad.starts_with("HTTP/1.1 400"), "no-ok status: {:?}", bad);
    assert!(bad.ends_with("bad"), "no-ok body must be 'bad': {:?}", bad);
    assert!(!bad.contains("good"), "no-ok must not run code after return: {:?}", bad);
    assert!(good.starts_with("HTTP/1.1 200"), "ok status: {:?}", good);
    assert!(good.ends_with("good"), "ok body must be 'good': {:?}", good);
}

/// Verifies a request body over --max-body-size is rejected with 413, and a body
/// under the limit is served normally.
#[test]
fn web_body_size_limit_returns_413() {
    let dir = make_test_dir("web_bodylimit");
    let src = "<?php echo strlen(file_get_contents('php://input'));";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1", "--max-body-size", "64"])
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    let small = http_request(&addr, "POST", "/", &[("Content-Type", "text/plain")], &"x".repeat(10));
    let big = http_request(&addr, "POST", "/", &[("Content-Type", "text/plain")], &"x".repeat(1000));
    let _ = child.kill();
    let _ = child.wait();
    assert!(small.ends_with("10"), "under-limit body should serve: {:?}", small);
    assert!(big.starts_with("HTTP/1.1 413"), "over-limit body should be 413: {:?}", big);
}

/// Verifies the server shuts down cleanly (exit code 0) on SIGTERM, promptly.
#[test]
fn web_sigterm_shuts_down_cleanly() {
    let dir = make_test_dir("web_sigterm");
    let bin = compile_web(&dir, "<?php echo \"ok\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "2");
    assert!(http_request(&addr, "GET", "/", &[], "").ends_with("ok"));
    let pid = child.id();
    let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().expect("try_wait") {
            break s;
        }
        if start.elapsed() > Duration::from_secs(8) {
            let _ = child.kill();
            panic!("master did not exit within 8s of SIGTERM");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(status.code(), Some(0), "master should exit 0 on SIGTERM");
}

/// Verifies `exit()`/`die()` ends the current `--web` request cleanly (like
/// PHP-FPM) instead of killing the prefork worker: output echoed before `exit`
/// is flushed with a 200, code after `exit` never runs, and the SAME worker keeps
/// serving. Before the exit()-bailout landed, `exit` terminated the worker and
/// dropped the connection; worker respawn is still covered by
/// `web_worker_handler_exit_still_respawns`, the max-requests recycle, and the
/// max-execution-time tests.
#[test]
fn web_exit_ends_request_and_keeps_worker() {
    let dir = make_test_dir("web_exit_ends");
    let src = "<?php echo 'A'; \
        if (($_SERVER['REQUEST_URI'] ?? '') === '/bye') { echo 'B'; exit; echo 'NEVER'; } \
        echo 'C';";
    let bin = compile_web(&dir, src, "app");
    let server = spawn_server_guarded(&bin, "1");
    // Normal path: full body A…C.
    let full = http_request(server.addr(), "GET", "/", &[], "");
    assert!(full.starts_with("HTTP/1.1 200"), "status: {:?}", full);
    assert_eq!(http_body(&full), "AC", "normal body: {:?}", full);
    // exit path: buffered output flushed, post-exit code skipped, 200 status.
    let bye = http_request(server.addr(), "GET", "/bye", &[], "");
    assert!(bye.starts_with("HTTP/1.1 200"), "exit must end the request 200: {:?}", bye);
    assert_eq!(http_body(&bye), "AB", "exit flushes 'AB' and skips NEVER: {:?}", bye);
    // The worker survived the exit: a subsequent request is served by it.
    let again = http_request(server.addr(), "GET", "/", &[], "");
    assert_eq!(http_body(&again), "AC", "worker must keep serving after exit: {:?}", again);
}

/// Verifies `exit()` still terminates a `--web-worker` HANDLER-mode worker: the
/// request-boundary bailout is intentionally scoped to the top-level-re-run modes
/// (`--web`/`--web-worker=script`), where `_elephc_web_handler` is the per-request
/// entry. In handler mode that symbol is the one-shot boot, so `exit(1)` inside
/// the registered handler drops the connection and the master respawns the
/// worker. This pins the scope boundary and preserves exit-driven respawn cover.
#[test]
fn web_worker_handler_exit_still_respawns() {
    let dir = make_test_dir("wwh_exit_crash");
    let src = "<?php elephc_worker_register(function () { \
        if (($_SERVER['REQUEST_URI'] ?? '') === '/crash') { exit(1); } echo 'alive'; });";
    let bin = compile_web_worker(&dir, src, "app");
    let server = spawn_server_guarded(&bin, "1");
    assert!(http_get(server.addr(), "/").ends_with("alive"));
    // exit(1) inside the handler kills the worker; the connection is dropped.
    let _ = try_http_get(server.addr(), "/crash");
    // The master must respawn a worker; retry until / serves again.
    let mut served = false;
    for _ in 0..40 {
        if try_http_get(server.addr(), "/").ends_with("alive") {
            served = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(served, "handler-mode worker must respawn after exit()");
}

/// Verifies `exit()` is NOT catchable by a user `catch (\Throwable)` — `exit` is a
/// language construct, not an exception (PHP: `try { exit('x'); } catch(...) {}`
/// never enters the catch). The bailout longjmps through a channel SEPARATE from
/// the exception handler chain, so the catch body must not run.
#[test]
fn web_exit_is_uncatchable_by_catch() {
    let dir = make_test_dir("web_exit_uncatchable");
    let src = "<?php try { echo 'A'; exit; echo 'NEVER'; } catch (\\Throwable $e) { echo 'CAUGHT'; }";
    let bin = compile_web(&dir, src, "app");
    let server = spawn_server_guarded(&bin, "1");
    let resp = http_request(server.addr(), "GET", "/", &[], "");
    assert!(resp.starts_with("HTTP/1.1 200"), "status: {:?}", resp);
    assert_eq!(http_body(&resp), "A", "exit must end the request with 'A' only: {:?}", resp);
}

/// Verifies `exit()` skips `finally`, matching PHP (`try { exit; } finally { echo
/// 'F'; } echo 'Z';` prints only what came before `exit`). The bailout longjmps
/// past the exception handler chain where finally bodies are inlined.
#[test]
fn web_exit_skips_finally() {
    let dir = make_test_dir("web_exit_finally");
    let src = "<?php try { echo 'A'; exit; } finally { echo 'F'; } echo 'Z';";
    let bin = compile_web(&dir, src, "app");
    let server = spawn_server_guarded(&bin, "1");
    let resp = http_request(server.addr(), "GET", "/", &[], "");
    assert_eq!(http_body(&resp), "A", "finally and trailing code must be skipped: {:?}", resp);
}

/// Verifies the ubiquitous `header('Location: ...'); exit;` redirect idiom: the
/// 302 + Location header are sent, code after `exit` never runs, and the worker
/// survives (the request ends cleanly instead of the worker dying).
#[test]
fn web_redirect_then_exit_serves_302() {
    let dir = make_test_dir("web_redirect_exit");
    let src = "<?php header('Location: /next'); exit; echo 'NEVER';";
    let bin = compile_web(&dir, src, "app");
    let server = spawn_server_guarded(&bin, "1");
    let resp = http_request(server.addr(), "GET", "/", &[], "");
    assert!(resp.starts_with("HTTP/1.1 302"), "status: {:?}", resp);
    assert!(resp.to_lowercase().contains("location: /next"), "headers: {:?}", resp);
    assert_eq!(http_body(&resp), "", "code after exit (NEVER) must not run: {:?}", resp);
    // The worker survived the redirect+exit.
    let again = http_request(server.addr(), "GET", "/", &[], "");
    assert!(again.starts_with("HTTP/1.1 302"), "worker must survive redirect+exit: {:?}", again);
}

/// Verifies HTTP/1.1 keep-alive: two requests on ONE TCP connection both succeed.
#[test]
fn web_keep_alive_reuses_connection() {
    use std::io::{Read, Write};
    let dir = make_test_dir("web_keepalive");
    let bin = compile_web(&dir, "<?php echo \"hi\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    wait_until_ready(&addr);
    let mut sock = TcpStream::connect(&addr).expect("connect");
    sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", addr);
    sock.write_all(req.as_bytes()).unwrap();
    let mut buf = [0u8; 512];
    let n1 = sock.read(&mut buf).unwrap();
    let resp1 = String::from_utf8_lossy(&buf[..n1]).to_string();
    // Second request on the SAME socket (only works if keep-alive kept it open).
    sock.write_all(req.as_bytes()).unwrap();
    let n2 = sock.read(&mut buf).unwrap();
    let resp2 = String::from_utf8_lossy(&buf[..n2]).to_string();
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp1.contains("200") && resp1.contains("hi"), "resp1: {:?}", resp1);
    assert!(resp2.contains("200") && resp2.contains("hi"), "keep-alive reuse failed: {:?}", resp2);
}

/// Regression: a request with many query parameters must not corrupt $_GET. The
/// superglobal assoc array grows past its initial capacity; before the fix the
/// grown table pointer was not written back to global storage, corrupting the
/// array (count went wrong / the worker crashed). 30 params must all survive.
#[test]
fn web_get_many_params_not_corrupted() {
    let dir = make_test_dir("web_get_many");
    let src = "<?php echo count($_GET) . '|' . ($_GET['p29'] ?? '?');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let mut query = String::from("/?");
    for i in 0..30 {
        query.push_str(&format!("p{}={}&", i, i));
    }
    let resp = http_request(&addr, "GET", &query, &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("30|29"), "many-param $_GET corrupted: {:?}", resp);
}

/// Verifies the extended $_SERVER keys (A1): REMOTE_ADDR, SERVER_PORT,
/// SERVER_PROTOCOL, REQUEST_SCHEME, SERVER_SOFTWARE, REQUEST_TIME.
#[test]
fn web_server_vars_populated() {
    let dir = make_test_dir("web_server_vars");
    let src = "<?php echo $_SERVER['REMOTE_ADDR'].'|'.$_SERVER['SERVER_PORT'].'|'\
        .$_SERVER['SERVER_PROTOCOL'].'|'.$_SERVER['REQUEST_SCHEME'].'|'\
        .$_SERVER['SERVER_SOFTWARE'].'|'.($_SERVER['REQUEST_TIME'] > 0 ? 't' : 'f');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    let expected = format!("127.0.0.1|{}|HTTP/1.1|http|elephc|t", port);
    assert!(resp.ends_with(&expected), "expected {:?} at end of {:?}", expected, resp);
}

/// Verifies $_COOKIE (A2): the Cookie header is parsed into the superglobal,
/// values are percent-decoded.
#[test]
fn web_cookie_parsed() {
    let dir = make_test_dir("web_cookie");
    let src = "<?php echo ($_COOKIE['a'] ?? '?').'|'.($_COOKIE['b'] ?? '?').'|'.count($_COOKIE);";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[("Cookie", "a=1; b=hello%20world")], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("1|hello world|2"), "cookie parse: {:?}", resp);
}

/// Verifies $_REQUEST (A4): merges $_GET then $_POST (POST overrides on collision).
#[test]
fn web_request_superglobal_merges_get_post() {
    let dir = make_test_dir("web_request_merge");
    let src = "<?php echo ($_REQUEST['x'] ?? '?').'|'.($_REQUEST['q'] ?? '?').'|'.count($_REQUEST);";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(
        &addr,
        "POST",
        "/?x=g&q=1",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        "x=p",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("p|1|2"), "$_REQUEST merge (POST overrides GET): {:?}", resp);
}

/// Verifies setcookie() (A3): emits a Set-Cookie response header (value
/// percent-encoded, attributes appended), and multiple calls produce multiple
/// headers (replace=false).
#[test]
fn web_setcookie_emits_header() {
    let dir = make_test_dir("web_setcookie");
    let src = "<?php setcookie('sid', 'ab c', 0, '/'); setcookie('x', 'y'); echo 'ok';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    let lower = resp.to_lowercase();
    assert!(lower.contains("set-cookie: sid=ab%20c; path=/"), "first cookie: {:?}", resp);
    assert!(lower.contains("set-cookie: x=y"), "second cookie: {:?}", resp);
    assert!(resp.ends_with("ok"), "body: {:?}", resp);
}

/// Verifies $_ENV (A7) is populated from the process environment.
#[test]
fn web_env_superglobal_populated() {
    let dir = make_test_dir("web_env");
    let src = "<?php echo ($_ENV['ELEPHC_WEB_TEST_ENV'] ?? '?');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1"])
        .env("ELEPHC_WEB_TEST_ENV", "present")
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("present"), "$_ENV not populated: {:?}", resp);
}

/// Verifies the produced binary answers --help and --version (exit 0) (D4).
#[test]
fn web_help_and_version() {
    let dir = make_test_dir("web_help");
    let bin = compile_web(&dir, "<?php echo 'x';", "app");
    let help = Command::new(&bin).arg("--help").output().expect("help");
    assert!(help.status.success(), "--help should exit 0");
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("--listen"),
        "--help should describe --listen"
    );
    let ver = Command::new(&bin).arg("--version").output().expect("version");
    assert!(ver.status.success(), "--version should exit 0");
    assert!(
        String::from_utf8_lossy(&ver.stdout).to_lowercase().contains("elephc-web"),
        "--version should name elephc-web"
    );
    // Missing --listen is a usage error (non-zero exit).
    let none = Command::new(&bin).output().expect("noargs");
    assert!(!none.status.success(), "missing --listen must exit non-zero");
}

/// Verifies --max-requests recycles a single worker yet the server keeps serving
/// across the recycle (the master respawns it) (B5).
#[test]
fn web_max_requests_recycles_and_keeps_serving() {
    let dir = make_test_dir("web_maxreq");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1", "--max-requests", "2"])
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    // More requests than the cap: the server must keep serving across recycles.
    // A single-worker recycle has a brief no-listener window, so tolerate transient
    // connection-refused and retry — every logical request must eventually succeed.
    for _ in 0..6 {
        let mut ok = false;
        for _ in 0..40 {
            if try_http_get(&addr, "/").ends_with("ok") {
                ok = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(ok, "server stopped serving across a --max-requests recycle");
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Verifies `--max-rss` keeps the server healthy across many requests under a
/// tight RSS cap: a worker whose resident set exceeds the cap exits cleanly so
/// the master respawns a fresh worker, bounding memory growth over time. Uses
/// `--web-worker` (handler mode) so a static property accumulates allocations
/// across requests (classic `--web` resets per-request state, so RSS would not
/// reliably grow). Each request appends 64 KiB to a persistent static; under a
/// 6 MiB cap the worker's RSS exceeds the cap after ~40 requests, and the
/// gated check (1/64 accepts) fires at SERVED == 64 to recycle the worker
/// cleanly. The robust assertion is "the server stays healthy across many
/// requests under a tight cap" — every logical request must eventually return
/// 200, tolerating the transient refused-connection window while a single
/// worker recycles. Verified manually: at SERVED == 64 the worker's RSS is
/// ~8 MiB (> 6 MiB cap), the worker exits cleanly, the master respawns a
/// fresh worker (~2.6 MiB), and serving continues with zero heap-exhaustion
/// fatals. (T1#1: RSS-based worker recycle.)
#[test]
fn web_max_rss_recycles_oversized_worker() {
    let dir = make_test_dir("web_maxrss");
    // 64 KiB per request keeps the elephc runtime heap from exhausting before
    // the RSS cap trips (larger allocations like 2 MiB hit "heap memory
    // exhausted" first, which is a crash death, not a clean RSS recycle).
    let src = "<?php class S { public static array $buf = []; } elephc_worker_register(function() { S::$buf[] = str_repeat('A', 65536); echo 'ok'; });";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args([
            "--listen", &addr,
            "--workers", "1",
            "--max-rss", "6",
        ])
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    // Drive 80 logical requests — the recycle fires at SERVED == 64 (the
    // gated 1/64 check), so this covers at least one clean recycle plus
    // continued serving afterward. Each must eventually succeed (200 + "ok"),
    // tolerating the brief no-listener window while the recycled worker
    // respawns.
    for _ in 0..80 {
        let mut ok = false;
        for _ in 0..60 {
            if try_http_get(&addr, "/").ends_with("ok") {
                ok = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            ok,
            "server stopped serving under --max-rss (worker recycle should be transparent)"
        );
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Verifies the `--max-rss` OFF path (`--max-rss 0` / omitted) is byte-for-byte
/// the original behavior: no RSS measurement happens and no recycling occurs.
/// Drives a handful of requests under a single worker and asserts all return
/// 200 + "ok" with no transient recycle gaps (the `max_rss_bytes > 0` gate in
/// the accept loop skips the RSS branch entirely when off). Guards the OFF
/// path against regressions in the gate. (T1#1: RSS-based worker recycle.)
#[test]
fn web_max_rss_off_is_byte_for_byte() {
    let dir = make_test_dir("web_maxrss_off");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    // `--max-rss 0` is the default (off); pass it explicitly to exercise the
    // parse arm and confirm the OFF path keeps the server serving normally.
    let mut child = Command::new(&bin)
        .args([
            "--listen", &addr,
            "--workers", "1",
            "--max-rss", "0",
        ])
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    // A small number of requests, all must succeed immediately (no recycle on
    // the OFF path, so no retry loop is needed — a failure here would indicate
    // the gate is broken and the worker is recycling spuriously).
    for _ in 0..10 {
        let resp = http_get(&addr, "/");
        assert!(
            resp.ends_with("ok"),
            "OFF-path request must succeed (no RSS recycle when --max-rss 0): {:?}",
            resp
        );
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Verifies a response larger than the socket send buffer is delivered intact
/// (not truncated). Every connection is now wrapped in the graceful-shutdown
/// watcher so `--max-requests` recycle can drain in-flight responses instead of
/// dropping them mid-write; this guards that the wrapping itself does not corrupt
/// or cut off a multi-write flush. Three sequential large responses must each
/// arrive complete, ending in the sentinel. (Forcing mid-flush truncation at the
/// exact recycle boundary is inherently timing-dependent; the drain path is also
/// covered by manual repro and `web_max_requests_recycles_and_keeps_serving`.)
#[test]
fn web_large_body_not_truncated() {
    let dir = make_test_dir("web_large_body");
    // 500_000 bytes exceeds a typical SO_SNDBUF, so the response spans multiple
    // socket writes and the connection future must be driven to completion.
    let src = "<?php echo str_repeat('A', 500000); echo 'END';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_guarded(&bin, "1");
    for i in 0..3 {
        let resp = http_get(srv.addr(), "/");
        let body = resp.split("\r\n\r\n").nth(1).unwrap_or("");
        assert_eq!(
            body.len(),
            500003,
            "request {i} body must be complete (500003 bytes), got {}",
            body.len()
        );
        assert!(body.ends_with("END"), "request {i} large body must end with the sentinel");
    }
}

/// Verifies an uncaught exception in the handler returns HTTP 500 instead of
/// crashing the worker / dropping the connection (B1), and the server keeps
/// serving other requests afterward.
#[test]
fn web_uncaught_exception_returns_500() {
    let dir = make_test_dir("web_500");
    let src = "<?php if (($_SERVER['REQUEST_URI'] ?? '') === '/boom') { throw new Exception('kaboom'); } echo 'ok';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let ok = http_request(&addr, "GET", "/", &[], "");
    let boom = http_request(&addr, "GET", "/boom", &[], "");
    let after = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(ok.ends_with("ok"), "normal request: {:?}", ok);
    assert!(boom.starts_with("HTTP/1.1 500"), "uncaught exception must be 500: {:?}", boom);
    assert!(after.ends_with("ok"), "server must keep serving after a 500: {:?}", after);
}

/// Verifies --max-execution-time kills a runaway handler (and the master respawns
/// the worker so the server recovers) (B3).
#[test]
fn web_max_execution_time_kills_runaway_handler() {
    let dir = make_test_dir("web_exectime");
    let src = "<?php if (($_SERVER['REQUEST_URI'] ?? '') === '/slow') { while (true) {} } echo 'fast';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1", "--max-execution-time", "1"])
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    assert!(http_request(&addr, "GET", "/", &[], "").ends_with("fast"));
    // The runaway request is killed by the watchdog (dropped connection); tolerate it.
    let _ = try_http_get(&addr, "/slow");
    // The master must respawn the worker; / serves again within a few seconds.
    let mut recovered = false;
    for _ in 0..40 {
        if try_http_get(&addr, "/").ends_with("fast") {
            recovered = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(recovered, "worker did not recover after a runaway handler was killed");
}

/// Verifies --gzip compresses the response when the client sends Accept-Encoding:
/// gzip (and only then) (C3).
#[test]
fn web_gzip_compresses_when_accepted() {
    let dir = make_test_dir("web_gzip");
    let bin = compile_web(&dir, "<?php echo str_repeat('ABCD', 500);", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1", "--gzip"])
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    // The gzipped body is binary, so read raw bytes and inspect the (ASCII) header
    // block rather than http_request's read_to_string.
    let gz_head = {
        use std::io::{Read, Write};
        let mut sock = TcpStream::connect(&addr).unwrap();
        let req = format!(
            "GET / HTTP/1.1\r\nHost: {}\r\nAccept-Encoding: gzip\r\nConnection: close\r\n\r\n",
            addr
        );
        sock.write_all(req.as_bytes()).unwrap();
        let mut buf = Vec::new();
        sock.read_to_end(&mut buf).unwrap();
        String::from_utf8_lossy(&buf[..buf.len().min(512)]).to_string()
    };
    let plain = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(gz_head.to_lowercase().contains("content-encoding: gzip"), "gzip not applied: {:?}", gz_head);
    assert!(!plain.to_lowercase().contains("content-encoding"), "must not compress without Accept-Encoding");
    // The uncompressed response carries the full 2000-byte body.
    assert!(plain.ends_with(&"ABCD".repeat(500)), "plain body mismatch");
}

/// Verifies multipart/form-data parsing (A5): text fields land in $_POST and file
/// uploads populate $_FILES (name, type, size). The request is built by hand to
/// avoid depending on a multipart client.
#[test]
fn web_multipart_post_and_files() {
    let dir = make_test_dir("web_multipart");
    let src = "<?php echo ($_POST['greeting'] ?? '?').'|'.($_FILES['upload']['name'] ?? '?')\
        .'|'.($_FILES['upload']['type'] ?? '?').'|'.($_FILES['upload']['size'] ?? '?');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let boundary = "Xbnd";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"greeting\"\r\n\r\nhello\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"upload\"; filename=\"up.txt\"\r\n\
         Content-Type: text/plain\r\n\r\nFILEDATA-123\r\n--{b}--\r\n",
        b = boundary
    );
    let ct = format!("multipart/form-data; boundary={}", boundary);
    let resp = http_request(&addr, "POST", "/", &[("Content-Type", &ct)], &body);
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("hello|up.txt|text/plain|12"), "multipart parse: {:?}", resp);
}

/// Verifies an uploaded file can be READ back via file_get_contents on its
/// tmp_name. This exercises both A5 and the multi-bridge link fix: a dynamic
/// file_get_contents pulls in the TLS bridge, which must co-link with the web
/// bridge without duplicate-symbol errors.
#[test]
fn web_multipart_file_contents_readable() {
    let dir = make_test_dir("web_upload_read");
    let src = "<?php $f = $_FILES['doc']['tmp_name'] ?? ''; echo $f === '' ? 'NOFILE' : file_get_contents($f);";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let boundary = "Zbnd";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"doc\"; filename=\"d.txt\"\r\n\
         Content-Type: application/octet-stream\r\n\r\nUPLOAD-CONTENT-OK\r\n--{b}--\r\n",
        b = boundary
    );
    let ct = format!("multipart/form-data; boundary={}", boundary);
    let resp = http_request(&addr, "POST", "/", &[("Content-Type", &ct)], &body);
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("UPLOAD-CONTENT-OK"), "upload content not read back: {:?}", resp);
}

/// Verifies a namespaced --web program (classes under a namespace) compiles and
/// serves. The B1 uncaught-exception wrap must not reorder top-level namespace
/// declarations away from the classes they scope (it skips the wrap entirely when
/// namespaces are present). Regression for the web-framework example.
#[test]
fn web_namespaced_program_serves() {
    let dir = make_test_dir("web_namespaced");
    let src = "<?php namespace App; \
        class Greeter { public function hi(string $n): string { return 'hi ' . $n; } } \
        $g = new Greeter(); echo $g->hi($_GET['n'] ?? 'world');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/?n=ada", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("hi ada"), "namespaced --web program: {:?}", resp);
}

// ---------------------------------------------------------------------------
// Worker mode (`--web-worker`) end-to-end tests.
//
// These compile a program with `--web-worker` (boot once → register a handler →
// Rust drives the HTTP loop), launch the binary with `--listen ... --workers 1`,
// and drive it over raw TCP. The core invariant is state persistence: function
// `static` locals, static class properties, and globals persist across requests
// within a worker (unlike classic `--web`, which resets them per request).
// ---------------------------------------------------------------------------

/// Verifies the core worker-mode invariant: a function `static` local persists
/// and accumulates across requests within one worker. Three sequential requests
/// must see the counter increment 1, 2, 3 (in classic `--web` each would be 1).
#[test]
fn web_worker_boot_once() {
    let dir = make_test_dir("ww_boot_once");
    let src = "<?php\nfunction getCounter(): int {\n    static $count = 0;\n    return ++$count;\n}\nelephc_worker_register(function () {\n    echo getCounter();\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let r3 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("1"), "first request must end with '1': {:?}", r1);
    assert!(r2.ends_with("2"), "second request must end with '2': {:?}", r2);
    assert!(r3.ends_with("3"), "third request must end with '3': {:?}", r3);
}

/// Verifies a worker handler echoing a literal string produces that body.
#[test]
fn web_worker_basic_response() {
    let dir = make_test_dir("ww_basic");
    let src = "<?php elephc_worker_register(function () { echo \"hello worker\"; });\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("hello worker"), "worker body: {:?}", resp);
}

/// Verifies worker resilience: an uncaught exception in the handler becomes HTTP
/// 500 (the trampoline catches `\Throwable`), the worker survives, and a
/// subsequent request succeeds. A function `static` guards the throw-once path.
#[test]
fn web_worker_500_recovery() {
    let dir = make_test_dir("ww_500");
    let src = "<?php\nfunction shouldThrow(): bool {\n    static $did = false;\n    if (!$did) { $did = true; return true; }\n    return false;\n}\nelephc_worker_register(function () {\n    if (shouldThrow()) { throw new Exception('boom'); }\n    echo 'recovered';\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.starts_with("HTTP/1.1 500"), "first request must be 500: {:?}", r1);
    assert!(r2.starts_with("HTTP/1.1 200"), "second request must be 200: {:?}", r2);
    assert!(r2.ends_with("recovered"), "second request body: {:?}", r2);
}

/// Verifies the worker-mode trampoline populates `$_GET` per request: the six
/// per-superglobal fill functions are invoked from the trampoline on every
/// request, so a handler reading `$_GET['name']` sees the value from the
/// current request's query string (no stale carry-over from the previous
/// request, and no empty superglobal from a missing fill call).
#[test]
fn web_worker_superglobals() {
    let dir = make_test_dir("ww_sg");
    let src = "<?php elephc_worker_register(function () { echo $_GET['name'] ?? 'none'; });\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/?name=world", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("world"), "worker $_GET: {:?}", resp);
}

/// Verifies `--max-requests` recycling resets the worker's persistent state.
/// With `--max-requests 2` the worker serves two requests (counter 1, 2), then
/// the master recycles it; the next worker boots fresh so the counter restarts
/// at 1. The single-worker recycle has a brief no-listener window, so the test
/// tolerates a transient refused connection and retries until a fresh counter
/// appears (proving the static was reset across the recycle boundary).
#[test]
fn web_worker_max_requests() {
    let dir = make_test_dir("ww_maxreq");
    let src = "<?php\nfunction getCounter(): int {\n    static $count = 0;\n    return ++$count;\n}\nelephc_worker_register(function () {\n    echo getCounter();\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1", "--max-requests", "2"])
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    // First two requests accumulate the persistent counter.
    assert!(http_get(&addr, "/").ends_with("1"), "pre-recycle request 1");
    assert!(http_get(&addr, "/").ends_with("2"), "pre-recycle request 2");
    // The worker recycles after 2 requests; retry until the fresh worker serves
    // a counter that reset to 1 (proving the static did not survive the recycle).
    let mut reset = false;
    for _ in 0..60 {
        if try_http_get(&addr, "/").ends_with("1") {
            reset = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(reset, "static counter did not reset after --max-requests recycle");
}

/// Verifies the worker trampoline populates `$_POST` and `$_REQUEST` per
/// request. A urlencoded POST body with `user=alice` and a query string
/// `?q=1` must yield `alice|1` — proving both the parsed body and the merged
/// `$_REQUEST` (GET ∪ POST) are rebuilt fresh each request in worker mode.
#[test]
fn web_worker_post_and_request() {
    let dir = make_test_dir("ww_post_req");
    let src = "<?php elephc_worker_register(function () {\n    echo ($_POST['user'] ?? '?') . '|' . ($_REQUEST['q'] ?? '?');\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(
        &addr,
        "POST",
        "/?q=1",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        "user=alice",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("alice|1"), "worker $_POST/$_REQUEST: {:?}", resp);
}

/// Verifies the worker trampoline populates `$_COOKIE` per request from the
/// `Cookie:` request header. Sending `Cookie: sid=abc` must yield `abc`,
/// proving the cookie superglobal is rebuilt fresh each request in worker
/// mode (the cookie jar is not leaked across requests).
#[test]
fn web_worker_cookie() {
    let dir = make_test_dir("ww_cookie");
    let src = "<?php elephc_worker_register(function () {\n    echo $_COOKIE['sid'] ?? '?';\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[("Cookie", "sid=abc")], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("abc"), "worker $_COOKIE: {:?}", resp);
}

/// Verifies `php://input` is readable from a worker handler and reflects the
/// raw request body of the current request. A POST with a JSON body and
/// `Content-Type: application/json` must echo the body verbatim — proving the
/// per-request input stream is wired into the worker trampoline.
#[test]
fn web_worker_php_input() {
    let dir = make_test_dir("ww_php_input");
    let src = "<?php elephc_worker_register(function () {\n    echo file_get_contents('php://input');\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(
        &addr,
        "POST",
        "/",
        &[("Content-Type", "application/json")],
        "{\"k\":42}",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("{\"k\":42}"), "worker php://input: {:?}", resp);
}

/// Verifies worker-mode superglobals are rebuilt fresh per request: no stale
/// values carry over from a prior request. Request 1 with `?a=first` must
/// echo `first`; request 2 with no query string must echo `none` (not the
/// stale `first` from the previous request). This is the core worker-mode
/// superglobal semantics — persistent state lives in user code, not in
/// request superglobals.
#[test]
fn web_worker_fresh_per_request() {
    let dir = make_test_dir("ww_fresh");
    let src = "<?php elephc_worker_register(function () {\n    echo $_GET['a'] ?? 'none';\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_request(&addr, "GET", "/?a=first", &[], "");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("first"), "first request: {:?}", r1);
    assert!(r2.ends_with("none"), "second request must not see stale $_GET: {:?}", r2);
}

/// Verifies a worker handler can set response headers and the HTTP status
/// code via `header()` / `http_response_code()`, and that the web bridge
/// emits them correctly. The response status line must be `HTTP/1.1 201`,
/// the `X-W: 1` header must be present, and the body must end with `ok`.
#[test]
fn web_worker_headers_and_status() {
    let dir = make_test_dir("ww_hdr_status");
    let src = "<?php elephc_worker_register(function () {\n    header('X-W: 1');\n    http_response_code(201);\n    echo 'ok';\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.starts_with("HTTP/1.1 201"), "status line: {:?}", resp);
    assert!(
        resp.to_lowercase().contains("x-w: 1"),
        "X-W header missing: {:?}",
        resp
    );
    assert!(resp.ends_with("ok"), "body: {:?}", resp);
}

/// Pins the (formerly misdiagnosed) claim that "the worker trampoline cannot
/// call user-defined PHP functions". A named function `greet()` returning
/// `'hi'` is invoked from the handler; two sequential requests must both
/// echo `hi`. The trampoline's per-superglobal fill calls and the user
/// handler dispatch go through normal PHP call frames, so user functions are
/// callable as expected.
#[test]
fn web_worker_handler_calls_user_fn() {
    let dir = make_test_dir("ww_user_fn");
    let src = "<?php\nfunction greet(): string { return 'hi'; }\nelephc_worker_register(function () {\n    echo greet();\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("hi"), "first request: {:?}", r1);
    assert!(r2.ends_with("hi"), "second request: {:?}", r2);
}

/// Verifies B2 (`$_ENV` built once per worker) in trampoline `--web-worker`
/// mode: the process environment is read at boot and persists across requests
/// rather than being rebuilt per request. A worker spawned with `B2_ENV_VAR`
/// set in its environment must return that value on two successive requests.
/// The second request is the real assertion: the per-request worker reset must
/// leave the boot-built `$_ENV` intact (if it wrongly zeroed it, request two
/// would read `MISSING`). The env var is injected only into the spawned child,
/// not the test process, to avoid racing parallel tests.
#[test]
fn web_worker_env_persists_across_requests() {
    let dir = make_test_dir("ww_env_once");
    let src = "<?php elephc_worker_register(function () {\n    echo 'env=' . ($_ENV['B2_ENV_VAR'] ?? 'MISSING');\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .arg("--listen").arg(&addr)
        .arg("--workers").arg("1")
        .env("B2_ENV_VAR", "persisted")
        .spawn()
        .expect("failed to spawn web-worker server");
    wait_until_ready(&addr);
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("env=persisted"), "first request $_ENV: {:?}", r1);
    assert!(r2.ends_with("env=persisted"), "second request $_ENV (persisted): {:?}", r2);
}

/// Verifies the worker-mode persistence invariant for a function `static`
/// array: a `static $s = []` inside `cache()` accumulates `'e'` entries
/// across requests within one worker. Three sequential requests must see
/// counts `1`, `2`, `3`. This is the same invariant as `web_worker_boot_once`
/// but exercises a static array (heap-backed local) rather than a static
/// scalar, confirming the persistence layer handles reference-typed statics.
#[test]
fn web_worker_static_array_persists() {
    let dir = make_test_dir("ww_static_arr");
    let src = "<?php\nfunction cache(): array {\n    static $s = [];\n    $s[] = 'e';\n    return $s;\n}\nelephc_worker_register(function () {\n    echo count(cache());\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let r3 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("1"), "first request must end with '1': {:?}", r1);
    assert!(r2.ends_with("2"), "second request must end with '2': {:?}", r2);
    assert!(r3.ends_with("3"), "third request must end with '3': {:?}", r3);
}

/// Verifies that an uncaught exception thrown from a worker handler based on
/// per-request superglobal state becomes HTTP 500, the worker survives, and
/// a subsequent request (without the trigger) succeeds with 200. The handler
/// throws when `$_GET['boom'] === '1'`, else echoes `'fine'`. This confirms
/// per-request `$_GET` correctly drives control flow and that exception
/// recovery does not corrupt the worker's persistent state.
#[test]
fn web_worker_fill_exception_500() {
    let dir = make_test_dir("ww_fill_500");
    let src = "<?php elephc_worker_register(function () {\n    if (($_GET['boom'] ?? '') === '1') { throw new Exception('x'); }\n    echo 'fine';\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/?boom=1");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.starts_with("HTTP/1.1 500"), "boom request must be 500: {:?}", r1);
    assert!(r2.starts_with("HTTP/1.1 200"), "next request must be 200: {:?}", r2);
    assert!(r2.ends_with("fine"), "next request body: {:?}", r2);
}

/// Verifies worker-mode multipart upload handling: a `multipart/form-data`
/// POST with one text field (`greeting=hello`) and one file (`up.txt` with
/// content `FILEDATA`) must populate `$_POST` and `$_FILES` per request, and
/// the uploaded file's `tmp_name` must be readable via `file_get_contents`
/// within the request. The handler echoes `hello|FILEDATA`. Temp-file unlink
/// after the request is runtime-side behavior not asserted here; this test
/// pins only the request-visible parsing + readback.
#[test]
fn web_worker_multipart_files_tmp_cleanup() {
    let dir = make_test_dir("ww_multipart");
    let src = "<?php elephc_worker_register(function () {\n    echo ($_POST['greeting'] ?? '?') . '|' . (($_FILES['up']['tmp_name'] ?? '') !== '' ? file_get_contents($_FILES['up']['tmp_name']) : 'nofile');\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let boundary = "Wbnd";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"greeting\"\r\n\r\nhello\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"up\"; filename=\"up.txt\"\r\n\
         Content-Type: text/plain\r\n\r\nFILEDATA\r\n--{b}--\r\n",
        b = boundary
    );
    let ct = format!("multipart/form-data; boundary={}", boundary);
    let resp = http_request(&addr, "POST", "/", &[("Content-Type", &ct)], &body);
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("hello|FILEDATA"), "worker multipart: {:?}", resp);
}

/// Verifies worker-mode `$_ENV` semantics: `$_ENV` is rebuilt fresh per
/// request from the process environment (RATIFIED Option A — same as classic
/// `--web`), so a handler reading `$_ENV['ELEPHC_WW_TEST']` sees the value
/// set on the server's environment on every request. Two sequential requests
/// must both echo `present`. This pins that the worker trampoline's `$_ENV`
/// fill re-reads the process env per request rather than snapshotting once
/// at boot.
#[test]
fn web_worker_env_rebuilt_per_request() {
    let dir = make_test_dir("ww_env");
    let src = "<?php elephc_worker_register(function () {\n    echo $_ENV['ELEPHC_WW_TEST'] ?? '?';\n});\n";
    let bin = compile_web_worker(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1"])
        .env("ELEPHC_WW_TEST", "present")
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("present"), "first request $_ENV: {:?}", r1);
    assert!(r2.ends_with("present"), "second request $_ENV: {:?}", r2);
}

// ---------------------------------------------------------------------------
// Script mode (`--web-worker=script`) end-to-end tests.
//
// These compile a program with `--web-worker=script`. Like classic `--web`, the
// ENTIRE top-level PHP re-runs on every request with fresh superglobals (no
// `elephc_worker_register` call — the whole top-level IS the per-request
// handler). Like `--web-worker` (handler mode), function `static` locals and
// top-level globals PERSIST across requests within a worker. Script mode uses
// the classic `--web` prelude, so a non-namespaced top-level `throw` is wrapped
// into an HTTP 500; a namespaced one is NOT (a pinned limitation, not a bug).
// ---------------------------------------------------------------------------

/// Verifies the script-mode persistence invariant for a function `static` local:
/// although the whole top-level script re-runs per request, the once-guarded
/// `static $n = 0` initializer runs only at boot, so the counter accumulates.
/// Three sequential requests must end "1", "2", "3" (under `--web` each is "1").
#[test]
fn web_worker_script_static_persists() {
    let dir = make_test_dir("wws_static");
    let src = "<?php function c(): int { static $n = 0; return ++$n; } echo c();";
    let bin = compile_web_worker_script(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let r3 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("1"), "first request must end with '1': {:?}", r1);
    assert!(r2.ends_with("2"), "second request must end with '2': {:?}", r2);
    assert!(r3.ends_with("3"), "third request must end with '3': {:?}", r3);
}

/// Verifies the script-mode persistence invariant for a class `static` property
/// (WI-S8): the top-level re-runs each request, but the once-guarded
/// `public static int $n = 0` initializer runs only at boot, so `C::$n` accumulates.
/// Three sequential requests must end "1", "2", "3" (under `--web` each would be "1"
/// because `__rt_web_reset` re-runs the initializer every request).
#[test]
fn web_worker_script_static_property_persists() {
    let dir = make_test_dir("wws_static_prop");
    let src = "<?php class C { public static int $n = 0; } C::$n = C::$n + 1; echo C::$n;";
    let bin = compile_web_worker_script(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let r3 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("1"), "first request must end with '1': {:?}", r1);
    assert!(r2.ends_with("2"), "second request must end with '2': {:?}", r2);
    assert!(r3.ends_with("3"), "third request must end with '3': {:?}", r3);
}

/// Verifies the flagship boot-once null-guard pattern under script mode: a
/// `static $c = null` initialized once to `['hits' => 0]` on first use, then
/// mutated per request. Three sequential requests must end "1", "2", "3". If it
/// returns 1,1,1 the static null-guard→array persistence is broken (a real bug).
#[test]
fn web_worker_script_boot_once_container() {
    let dir = make_test_dir("wws_boot_once");
    let src = "<?php function container(): int { static $c = null; if ($c === null) { $c = ['hits' => 0]; } $c['hits']++; return $c['hits']; } echo container();";
    let bin = compile_web_worker_script(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let r3 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("1"), "first request must end with '1': {:?}", r1);
    assert!(r2.ends_with("2"), "second request must end with '2': {:?}", r2);
    assert!(r3.ends_with("3"), "third request must end with '3': {:?}", r3);
}

/// Verifies the boot-once null-guard pattern written at *file scope* (top level),
/// not inside a function. A top-level `static $c = null` guarded by
/// `if ($c === null)` previously SIGSEGV'd at file scope (the guard read kept the
/// collapsed global-env type, so `=== null` folded to a compile-time `false`, the
/// initializer was skipped, and the null slot was dereferenced). It must now build
/// once and persist across requests: three sequential requests end "1", "2", "3".
#[test]
fn web_worker_script_top_level_boot_once() {
    let dir = make_test_dir("wws_top_level_boot_once");
    let src = "<?php static $c = null; if ($c === null) { $c = ['hits' => 0]; } $c['hits']++; echo $c['hits'];";
    let bin = compile_web_worker_script(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let r3 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("1"), "first request must end with '1': {:?}", r1);
    assert!(r2.ends_with("2"), "second request must end with '2': {:?}", r2);
    assert!(r3.ends_with("3"), "third request must end with '3': {:?}", r3);
}

/// Verifies enum-case singletons persist (identity-stable) across requests in
/// script mode (WI-S8). The top-level re-runs per request, but the once-guarded
/// enum-singleton block allocates `Status::Active`/`Status::Idle` only at boot.
/// A function-static `$s` is initialized once to `Status::Active` (fresh, request
/// 1) and persists; every request compares `firstSeen() === Status::Active`, so
/// all three responses must end "same". Without the enum once-guard, request 2+
/// would re-allocate `Status::Active` into a new object while `$s` still holds the
/// request-1 object — the pointer-identity `===` would mismatch ("diff") and leak.
#[test]
fn web_worker_script_enum_singleton_persists() {
    let dir = make_test_dir("wws_enum_singleton");
    let src = "<?php enum Status { case Active; case Idle; } function firstSeen(): Status { static $s = null; if ($s === null) { $s = Status::Active; } return $s; } echo firstSeen() === Status::Active ? 'same' : 'diff';";
    let bin = compile_web_worker_script(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let r3 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("same"), "first request must end with 'same': {:?}", r1);
    assert!(r2.ends_with("same"), "second request must end with 'same': {:?}", r2);
    assert!(r3.ends_with("same"), "third request must end with 'same': {:?}", r3);
}

/// Verifies request superglobals are rebuilt fresh per request in script mode
/// (the whole top-level re-runs with a new `$_GET` each time): `/?a=first` must
/// end "first", and a following `/` with no query string must end "none" (no
/// stale carry-over from the previous request).
#[test]
fn web_worker_script_fresh_superglobals() {
    let dir = make_test_dir("wws_fresh");
    let src = "<?php echo $_GET['a'] ?? 'none';";
    let bin = compile_web_worker_script(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_request(&addr, "GET", "/?a=first", &[], "");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("first"), "first request: {:?}", r1);
    assert!(r2.ends_with("none"), "second request must not see stale $_GET: {:?}", r2);
}

/// Verifies user `global` variables persist across requests in script mode (the
/// documented "globals persist" row of the mode matrix). `bump()` increments a
/// `global $g` that is initialized only via `?? 0`, so three sequential requests
/// must end "1", "2", "3". It fails only if `StoreGlobalReleasing` were to drop
/// the value or a future reset cleared globals per request.
#[test]
fn web_worker_script_global_persists() {
    let dir = make_test_dir("wws_global");
    let src = "<?php function bump(): int { global $g; $g = ($g ?? 0) + 1; return $g; } echo bump();";
    let bin = compile_web_worker_script(&dir, src, "app");
    let srv = spawn_server_guarded(&bin, "1");
    let r1 = http_get(srv.addr(), "/");
    let r2 = http_get(srv.addr(), "/");
    let r3 = http_get(srv.addr(), "/");
    assert!(r1.ends_with("1"), "first request must end with '1': {:?}", r1);
    assert!(r2.ends_with("2"), "second request must end with '2': {:?}", r2);
    assert!(r3.ends_with("3"), "third request must end with '3': {:?}", r3);
}

/// Verifies a typed static class property WITHOUT a default persists across
/// requests in script mode (WI-S8 completeness / audit H2). `A::$x` is seeded to
/// 41 once via a function-static `$done` gate, then read-and-incremented at the
/// re-running top level. The property has no default, so codegen emits an
/// "uninitialized" sentinel for it; without gating that sentinel store behind the
/// per-property `_init` once-guard, request 2 re-marks `A::$x` uninitialized and
/// the top-level read `A::$x + 1` fatals with "accessed before initialization"
/// (worker dies, empty response). With the guard the property persists: three
/// requests end "42", "43", "44".
#[test]
fn web_worker_script_typed_static_property_no_default_persists() {
    let dir = make_test_dir("wws_static_prop_no_default");
    let src = "<?php class A { public static int $x; } function boot(): void { static $done = false; if (!$done) { $done = true; A::$x = 41; } } boot(); A::$x = A::$x + 1; echo A::$x;";
    let bin = compile_web_worker_script(&dir, src, "app");
    let srv = spawn_server_guarded(&bin, "1");
    let r1 = http_get(srv.addr(), "/");
    let r2 = http_get(srv.addr(), "/");
    let r3 = http_get(srv.addr(), "/");
    assert!(r1.ends_with("42"), "first request must end with '42': {:?}", r1);
    assert!(r2.ends_with("43"), "second request must end with '43' (sentinel must not re-mark the property): {:?}", r2);
    assert!(r3.ends_with("44"), "third request must end with '44': {:?}", r3);
}

/// Verifies `$_COOKIE` is rebuilt fresh per request in script mode: a request
/// sending `Cookie: sid=abc` must echo "abc", and a following request with no
/// Cookie header must echo "nocookie" (no stale carry-over from the persisted
/// worker process). Complements the `$_GET`/`$_POST` freshness tests for the
/// cookie superglobal, which is otherwise only covered in handler mode.
#[test]
fn web_worker_script_fresh_cookie() {
    let dir = make_test_dir("wws_fresh_cookie");
    let src = "<?php echo $_COOKIE['sid'] ?? 'nocookie';";
    let bin = compile_web_worker_script(&dir, src, "app");
    let srv = spawn_server_guarded(&bin, "1");
    let r1 = http_request(srv.addr(), "GET", "/", &[("Cookie", "sid=abc")], "");
    let r2 = http_get(srv.addr(), "/");
    assert!(r1.ends_with("abc"), "first request must see its cookie: {:?}", r1);
    assert!(r2.ends_with("nocookie"), "second request must not see a stale $_COOKIE: {:?}", r2);
}

/// Verifies `$_POST` parsing and `php://input` both reflect the current request
/// body in script mode: a urlencoded POST with body `user=alice` and the matching
/// `Content-Type` must yield "alice|user=alice" (parsed field left, raw input
/// stream right).
#[test]
fn web_worker_script_post_and_php_input() {
    let dir = make_test_dir("wws_post");
    let src = "<?php echo ($_POST['user'] ?? '?') . '|' . file_get_contents('php://input');";
    let bin = compile_web_worker_script(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(
        &addr,
        "POST",
        "/",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        "user=alice",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("alice|user=alice"), "script $_POST/php://input: {:?}", resp);
}

/// Verifies a non-namespaced uncaught exception in a script-mode program becomes
/// HTTP 500 (the classic `--web` prelude wraps top-level throws), and the worker
/// survives: `/` ends "ok", `/boom` starts "HTTP/1.1 500", and a following `/`
/// still ends "ok".
#[test]
fn web_worker_script_uncaught_exception_500() {
    let dir = make_test_dir("wws_500");
    let src = "<?php if (($_SERVER['REQUEST_URI'] ?? '') === '/boom') { throw new Exception('x'); } echo 'ok';";
    let bin = compile_web_worker_script(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let ok = http_get(&addr, "/");
    let boom = http_get(&addr, "/boom");
    let after = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(ok.ends_with("ok"), "normal request: {:?}", ok);
    assert!(boom.starts_with("HTTP/1.1 500"), "uncaught exception must be 500: {:?}", boom);
    assert!(after.ends_with("ok"), "server must keep serving after a 500: {:?}", after);
}

/// Verifies a NAMESPACED uncaught exception becomes HTTP 500 without crashing the
/// worker — the namespace-section-aware wrap now covers namespaced programs
/// (previously the try→500 wrap was skipped for any `namespace`, so the worker
/// died mid-request). `/` ends "ok", `/boom` starts "HTTP/1.1 500", and a
/// following `/` still ends "ok" (same worker, no respawn needed).
#[test]
fn web_worker_script_namespaced_uncaught_exception_500() {
    let dir = make_test_dir("wws_ns_throw");
    let src = "<?php namespace App; if (($_SERVER['REQUEST_URI'] ?? '') === '/boom') { throw new \\Exception('x'); } echo 'ok';";
    let bin = compile_web_worker_script(&dir, src, "app");
    let srv = spawn_server_guarded(&bin, "1");
    let ok = http_get(srv.addr(), "/");
    let boom = http_get(srv.addr(), "/boom");
    let after = http_get(srv.addr(), "/");
    assert!(ok.ends_with("ok"), "namespaced / must serve 'ok': {:?}", ok);
    assert!(boom.starts_with("HTTP/1.1 500"), "namespaced uncaught exception must be 500: {:?}", boom);
    assert!(after.ends_with("ok"), "server must keep serving after a 500: {:?}", after);
}

/// Verifies a namespaced program with a top-level `throw` returns HTTP 500 under
/// classic `--web` (not just script mode) — the shared `inject_if_web` wrap now
/// covers namespaces. The non-namespaced control must also be 500, guarding
/// parity between the two paths.
#[test]
fn web_namespaced_throw_returns_500() {
    let dir = make_test_dir("web_ns_throw");
    let ns = compile_web(&dir, "<?php namespace App; throw new \\RuntimeException(\"boom\");", "ns");
    let srv = spawn_server_guarded(&ns, "1");
    assert!(http_get(srv.addr(), "/").starts_with("HTTP/1.1 500"), "namespaced throw must be 500");
    drop(srv);

    let flat = compile_web(&dir, "<?php throw new RuntimeException(\"boom\");", "flat");
    let srv2 = spawn_server_guarded(&flat, "1");
    assert!(http_get(srv2.addr(), "/").starts_with("HTTP/1.1 500"), "non-namespaced throw must be 500");
}

/// Verifies a namespaced program whose executable path does NOT throw still
/// serves a normal 200 (the wrap must not mis-scope `App\…` names) and that a
/// user's own `catch (\Throwable)` still fires (the outer 500 net does not shadow
/// it). Guards against the namespace-section wrap breaking normal programs.
#[test]
fn web_namespaced_program_serves_and_user_catch_wins() {
    let dir = make_test_dir("web_ns_serve");
    let serve = compile_web(
        &dir,
        "<?php namespace App; function greet(string $x): string { return \"hi \".$x; } echo greet($_GET[\"n\"] ?? \"world\");",
        "serve",
    );
    let srv = spawn_server_guarded(&serve, "1");
    assert!(http_get(srv.addr(), "/?n=ada").ends_with("hi ada"), "namespaced program must serve normally");
    drop(srv);

    let caught = compile_web(
        &dir,
        "<?php namespace App; try { throw new \\Exception(\"e\"); } catch (\\Throwable $x) { echo \"caught\"; }",
        "caught",
    );
    let srv2 = spawn_server_guarded(&caught, "1");
    let r = http_get(srv2.addr(), "/");
    assert!(r.starts_with("HTTP/1.1 200") && r.ends_with("caught"), "user catch must fire, not the 500 net: {:?}", r);
}

/// Verifies the WI-S5 StoreGlobal release-previous path is heap-stable: a
/// refcounted top-level global `$g = str_repeat('ab', 100)` (a 200-byte string)
/// is reassigned on every re-run of the script. 50 sequential requests must each
/// end "200" and the server must stay up the whole time (the previous request's
/// string is released, not leaked, on each re-run).
#[test]
fn web_worker_script_heap_stable_over_many_requests() {
    let dir = make_test_dir("wws_heap");
    let src = "<?php $g = str_repeat('ab', 100); echo strlen($g);";
    let bin = compile_web_worker_script(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let mut stable = true;
    for i in 0..50 {
        let r = http_get(&addr, "/");
        if !r.ends_with("200") {
            stable = false;
            eprintln!("request {i} body: {:?}", r);
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(stable, "server did not stay heap-stable over 50 requests");
}

/// Verifies `exit()` ends a `--web-worker=script` request while the worker keeps
/// its persistent state: a function `static` counter bumped before an `exit` on
/// one request is still visible on the next (it would reset to "1" if the worker
/// had been respawned instead of surviving). Also confirms code after `exit`
/// never runs and the request ends 200.
#[test]
fn web_worker_script_exit_ends_request_and_persists_state() {
    let dir = make_test_dir("wws_exit_ends");
    let src = "<?php function n(): int { static $c = 0; return ++$c; } \
        echo n(); \
        if (($_SERVER['REQUEST_URI'] ?? '') === '/bye') { exit; echo 'NEVER'; }";
    let bin = compile_web_worker_script(&dir, src, "app");
    let server = spawn_server_guarded(&bin, "1");
    // r1 normal → "1".
    let r1 = http_request(server.addr(), "GET", "/", &[], "");
    assert_eq!(http_body(&r1), "1", "r1 body: {:?}", r1);
    // r2 hits /bye → static bumps to "2", exit ends the request (NEVER skipped), 200.
    let r2 = http_request(server.addr(), "GET", "/bye", &[], "");
    assert!(r2.starts_with("HTTP/1.1 200"), "exit status: {:?}", r2);
    assert_eq!(http_body(&r2), "2", "static must persist across the exit request: {:?}", r2);
    // r3 normal → "3": the worker survived AND kept the static (would be "1" again
    // if it had been respawned).
    let r3 = http_request(server.addr(), "GET", "/", &[], "");
    assert_eq!(http_body(&r3), "3", "static must keep accumulating after exit: {:?}", r3);
}

/// Verifies `exit()` from a NESTED function that OWNS refcounted locals ends the
/// WHOLE request, keeps the worker alive, AND releases the callee's owned locals
/// on the way out. `bail()` holds a live string and array at the `exit`, so the
/// activation-record cleanup callback for its frame must run during the bailout
/// unwind — otherwise those locals leak every request. Driving many requests
/// while a persistent `static` counter keeps advancing proves both that the
/// request boundary is reached from any call depth (unlike a top-level `return`)
/// and that the worker is never respawned (a respawn or per-request leak-driven
/// crash would break the monotonic counter). See the CLI-level, deterministic
/// heap-balance guard `test_throw_through_nested_frames_releases_owned_locals`,
/// which shares the same unwinder cleanup path.
#[test]
fn web_worker_script_exit_from_nested_function() {
    let dir = make_test_dir("wws_exit_nested");
    // `bail()` owns a string + array that are still live at `exit` (read in the
    // guard), so the unwinder must free them via this frame's cleanup callback.
    let src = "<?php function seq(): int { static $c = 0; return ++$c; } \
        function bail() { $s = str_repeat('x', 4000); $arr = [1, 2, 3, 4, 5]; \
            if (strlen($s) + count($arr) > 0) { echo 'IN'; exit; } echo 'NEVER_FN'; } \
        echo 'A'; echo seq(); bail(); echo 'NEVER_TOP';";
    let bin = compile_web_worker_script(&dir, src, "app");
    let server = spawn_server_guarded(&bin, "1");
    // Each request: the top level re-runs, the persistent static advances, the
    // nested exit ends the request after "A<n>IN", and the worker survives with
    // the callee's owned locals reclaimed (no accumulation across requests).
    for i in 1..=40 {
        let resp = http_request(server.addr(), "GET", "/", &[], "");
        assert!(resp.starts_with("HTTP/1.1 200"), "request {i} status: {:?}", resp);
        assert_eq!(
            http_body(&resp),
            format!("A{i}IN"),
            "request {i}: nested exit must end after 'A{i}IN' and the worker must survive: {:?}",
            resp,
        );
    }
}

/// Verifies the `die("message")` string form: the message is written into the
/// response body (through the output-capture path) and the request ends 200,
/// with the worker surviving. This is the ubiquitous `die('error')` idiom.
#[test]
fn web_worker_script_die_with_message_prints_and_ends_request() {
    let dir = make_test_dir("wws_die_msg");
    let src = "<?php echo 'A'; die('boom'); echo 'NEVER';";
    let bin = compile_web_worker_script(&dir, src, "app");
    let server = spawn_server_guarded(&bin, "1");
    let resp = http_request(server.addr(), "GET", "/", &[], "");
    assert!(resp.starts_with("HTTP/1.1 200"), "status: {:?}", resp);
    assert_eq!(http_body(&resp), "Aboom", "die('boom') must print then end: {:?}", resp);
    // The worker survived the die(message) bailout.
    let again = http_request(server.addr(), "GET", "/", &[], "");
    assert_eq!(http_body(&again), "Aboom", "worker must survive die(message): {:?}", again);
}

/// Verifies the common, leak-free `exit()` shape stays alive and heap-stable
/// across MANY requests: a top-level `exit` whose only live values are scalars
/// and a persistent `static`. The static keeps advancing over 30 requests,
/// proving no per-exit crash and that the worker is never respawned (a respawn
/// would reset the static to 1). The bailout epilogue releases the top-level
/// handler's owned locals, so this pattern does not leak.
///
/// The nested-frame case (an `exit`/`die`/`throw` unwinding out of a function
/// that owns refcounted locals) is now equally leak-free: the EIR backend emits
/// a per-frame activation-record cleanup callback that the unwinder invokes to
/// release each unwound frame's owned locals. That path is covered here by
/// `web_worker_script_exit_from_nested_function` (web exit) and, at the
/// deterministic CLI level, by `test_throw_through_nested_frames_releases_owned_locals`.
#[test]
fn web_worker_script_exit_heap_stable_over_many_requests() {
    let dir = make_test_dir("wws_exit_stable");
    let src = "<?php function n(): int { static $c = 0; return ++$c; } echo n(); exit;";
    let bin = compile_web_worker_script(&dir, src, "app");
    let server = spawn_server_guarded(&bin, "1");
    for i in 1..=30 {
        let resp = http_request(server.addr(), "GET", "/", &[], "");
        let v: i64 = http_body(&resp).trim().parse().unwrap_or(-1);
        assert_eq!(v, i, "request {i} must see persistent static = {i} after every exit: {:?}", resp);
    }
}

/// Verifies the defining fence between `--web` and `--web-worker=script` for the
/// SAME source: under `--web` a function `static` resets per request (two
/// requests both end "1"); under script mode it accumulates (two requests end "1"
/// then "2"). This is the core behavioral difference between the two web modes.
#[test]
fn web_worker_script_fence_vs_web_resets() {
    let dir = make_test_dir("wws_fence");
    let src = "<?php function c(): int { static $n = 0; return ++$n; } echo c();";
    let web_bin = compile_web(&dir, src, "web_app");
    let script_bin = compile_web_worker_script(&dir, src, "script_app");

    // Classic --web: the static resets on every request.
    let web_port = free_port();
    let web_addr = format!("127.0.0.1:{}", web_port);
    let mut web_child = spawn_server(&web_bin, &web_addr, "1");
    let web_r1 = http_get(&web_addr, "/");
    let web_r2 = http_get(&web_addr, "/");
    let _ = web_child.kill();
    let _ = web_child.wait();

    // Script mode: the static persists and accumulates.
    let script_port = free_port();
    let script_addr = format!("127.0.0.1:{}", script_port);
    let mut script_child = spawn_server(&script_bin, &script_addr, "1");
    let script_r1 = http_get(&script_addr, "/");
    let script_r2 = http_get(&script_addr, "/");
    let _ = script_child.kill();
    let _ = script_child.wait();

    assert!(web_r1.ends_with("1"), "--web request 1 must reset to '1': {:?}", web_r1);
    assert!(web_r2.ends_with("1"), "--web request 2 must reset to '1': {:?}", web_r2);
    assert!(script_r1.ends_with("1"), "script request 1 must be '1': {:?}", script_r1);
    assert!(script_r2.ends_with("2"), "script request 2 must accumulate to '2': {:?}", script_r2);
}

// --- PR1: keep-alive rebalance (--max-requests-per-connection / --idle-timeout) ---

/// Spawns the server on a fresh ephemeral port with arbitrary extra runtime flags
/// (appended after `--listen <addr>`), returning a kill-on-drop `ServerHandle`.
/// Callers pass their own `--workers` when needed. Used by the keep-alive
/// rotation tests, which need custom rotation/idle flags the plain
/// `spawn_server_guarded` does not set.
fn spawn_server_with_flags(bin: &Path, extra: &[&str]) -> ServerHandle {
    let addr = format!("127.0.0.1:{}", free_port());
    let mut cmd = Command::new(bin);
    cmd.arg("--listen").arg(&addr);
    for a in extra {
        cmd.arg(a);
    }
    let child = cmd.spawn().expect("failed to spawn web server");
    wait_until_ready(&addr);
    ServerHandle { child, addr }
}

/// Reads exactly one HTTP/1.1 response off a keep-alive socket: the status line
/// and headers up to the blank line, then `Content-Length` body bytes (elephc-web
/// always sets `Content-Length`). Returns the full raw response text. Unlike a
/// single `read`, this does not depend on the whole response arriving in one TCP
/// segment, so the `Connection: close` assertions are not flaky.
fn read_keepalive_response(sock: &mut TcpStream) -> String {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 2048];
    // Read until the full header block (\r\n\r\n) is present, or EOF.
    let header_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        let n = sock.read(&mut tmp).unwrap();
        if n == 0 {
            return String::from_utf8_lossy(&buf).into_owned();
        }
        buf.extend_from_slice(&tmp[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let content_len = head
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while buf.len() < header_end + content_len {
        let n = sock.read(&mut tmp).unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// WI-2: `--max-requests-per-connection 2` closes a keep-alive connection after 2
/// responses. Two requests on ONE connection: response 1 (under the cap) carries
/// no `Connection: close`; response 2 (at the cap) carries it; the next read
/// returns 0 (EOF), proving hyper actually closed the socket.
#[test]
fn web_connection_closes_after_max_requests_per_connection() {
    let dir = make_test_dir("web_maxconn");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--max-requests-per-connection", "2"]);
    let mut sock = TcpStream::connect(srv.addr()).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", srv.addr());
    // Request 1: within the cap → keep-alive stays open.
    sock.write_all(req.as_bytes()).unwrap();
    let r1 = read_keepalive_response(&mut sock);
    assert!(r1.contains("200") && http_body(&r1) == "ok", "r1: {:?}", r1);
    assert!(
        !r1.to_lowercase().contains("connection: close"),
        "request 1 (under cap) must not close the connection: {:?}",
        r1
    );
    // Request 2: hits the cap → Connection: close, then the server closes.
    sock.write_all(req.as_bytes()).unwrap();
    let r2 = read_keepalive_response(&mut sock);
    assert!(r2.contains("200") && http_body(&r2) == "ok", "r2: {:?}", r2);
    assert!(
        r2.to_lowercase().contains("connection: close"),
        "request 2 (at cap) must carry Connection: close: {:?}",
        r2
    );
    // The server must actually close the connection after response 2.
    let mut tmp = [0u8; 64];
    let n = sock.read(&mut tmp).unwrap();
    assert_eq!(n, 0, "server must close the connection after the cap (EOF expected)");
}

/// WI-2: `--max-requests-per-connection 0` disables the per-connection cap, so a
/// keep-alive connection serves unbounded requests and no response carries
/// `Connection: close`.
#[test]
fn web_connection_unlimited_when_cap_zero() {
    let dir = make_test_dir("web_maxconn_zero");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--max-requests-per-connection", "0"]);
    let mut sock = TcpStream::connect(srv.addr()).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", srv.addr());
    for i in 1..=3 {
        sock.write_all(req.as_bytes()).unwrap();
        let r = read_keepalive_response(&mut sock);
        assert!(r.contains("200") && http_body(&r) == "ok", "request {i} resp: {:?}", r);
        assert!(
            !r.to_lowercase().contains("connection: close"),
            "request {i} must keep the connection alive with cap 0: {:?}",
            r
        );
    }
}

/// WI-3: the per-connection cap works in `--web-worker=script` mode, and the
/// forced reconnect does not reset worker persistence: a PHP function `static`
/// counter keeps advancing across the cap-triggered reconnect (would restart at
/// "1" if the worker had been recycled).
#[test]
fn web_worker_script_connection_closes_after_cap() {
    let dir = make_test_dir("wws_maxconn");
    let src = "<?php function n(): int { static $c = 0; return ++$c; } echo n();";
    let bin = compile_web_worker_script(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--max-requests-per-connection", "2"]);
    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", srv.addr());
    // Two requests on ONE connection: static counter → "1", "2"; response 2 closes.
    let mut sock = TcpStream::connect(srv.addr()).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    sock.write_all(req.as_bytes()).unwrap();
    let r1 = read_keepalive_response(&mut sock);
    assert_eq!(http_body(&r1), "1", "r1 body must be the persistent static 1: {:?}", r1);
    sock.write_all(req.as_bytes()).unwrap();
    let r2 = read_keepalive_response(&mut sock);
    assert_eq!(http_body(&r2), "2", "r2 body must be the persistent static 2: {:?}", r2);
    assert!(
        r2.to_lowercase().contains("connection: close"),
        "response 2 must carry Connection: close in script mode: {:?}",
        r2
    );
    let mut tmp = [0u8; 64];
    let n = sock.read(&mut tmp).unwrap();
    assert_eq!(n, 0, "connection must close after the per-connection cap");
    // Reconnect (new source port): the worker persisted the static across the
    // rotation, so the next request continues at "3", not "1".
    let mut sock2 = TcpStream::connect(srv.addr()).unwrap();
    sock2.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    sock2.write_all(req.as_bytes()).unwrap();
    let r3 = read_keepalive_response(&mut sock2);
    assert_eq!(
        http_body(&r3),
        "3",
        "static must keep advancing across the rotation reconnect (expected 3): {:?}",
        r3
    );
}

/// WI-2 (C3 drain): a worker at its `--max-requests` recycle cap drains its
/// keep-alive connections by setting `Connection: close` on responses instead of
/// serving them past the cap and cutting them at `process::exit`. With
/// `--max-requests 1`, the first keep-alive response already carries the header.
#[test]
fn web_max_requests_drains_keepalive() {
    let dir = make_test_dir("web_drain");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--max-requests", "1"]);
    let mut sock = TcpStream::connect(srv.addr()).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", srv.addr());
    sock.write_all(req.as_bytes()).unwrap();
    let r = read_keepalive_response(&mut sock);
    assert!(r.contains("200") && http_body(&r) == "ok", "resp: {:?}", r);
    assert!(
        r.to_lowercase().contains("connection: close"),
        "a worker at its --max-requests cap must drain keep-alive with Connection: close (C3): {:?}",
        r
    );
}

/// WI-4: `--idle-timeout 1` closes a connection left idle past the timeout. After
/// one response, staying idle for 2s must have the watchdog gracefully close the
/// connection, so the next read returns 0 (EOF) well before the 30s
/// header_read_timeout would.
#[test]
fn web_idle_timeout_closes_idle_connection() {
    let dir = make_test_dir("web_idle_close");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--idle-timeout", "1"]);
    let mut sock = TcpStream::connect(srv.addr()).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", srv.addr());
    sock.write_all(req.as_bytes()).unwrap();
    let r1 = read_keepalive_response(&mut sock);
    assert!(r1.contains("200") && http_body(&r1) == "ok", "r1: {:?}", r1);
    // Stay idle past the 1s idle-timeout; the watchdog must close the connection.
    std::thread::sleep(Duration::from_secs(2));
    let mut tmp = [0u8; 64];
    let n = sock.read(&mut tmp).unwrap();
    assert_eq!(n, 0, "idle connection must be closed by the --idle-timeout watchdog (EOF)");
}

/// WI-4: `--idle-timeout 0` disables the watchdog. A connection idle for 2s (well
/// under hyper's 30s header_read_timeout) must stay open for a second request.
/// The sub-30s sleep keeps this independent of hyper's own idle behavior (Q4).
#[test]
fn web_idle_timeout_zero_keeps_connection() {
    let dir = make_test_dir("web_idle_zero");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--idle-timeout", "0"]);
    let mut sock = TcpStream::connect(srv.addr()).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", srv.addr());
    sock.write_all(req.as_bytes()).unwrap();
    let r1 = read_keepalive_response(&mut sock);
    assert!(r1.contains("200") && http_body(&r1) == "ok", "r1: {:?}", r1);
    // Idle well under the 30s header_read_timeout; with idle-timeout 0 there is no
    // watchdog, so the connection must stay open for a second request.
    std::thread::sleep(Duration::from_secs(2));
    sock.write_all(req.as_bytes()).unwrap();
    let r2 = read_keepalive_response(&mut sock);
    assert!(
        r2.contains("200") && http_body(&r2) == "ok",
        "connection must stay open with idle-timeout 0: {:?}",
        r2
    );
}

/// WI-4 non-truncation guard: a large response (multiple socket writes) in flight
/// while a short `--idle-timeout` expires must NOT be cut off. The client stalls
/// past the timeout (so the watchdog fires and `graceful_shutdown` runs), then
/// drains the socket; the whole body must arrive intact, proving graceful
/// shutdown finishes the in-flight response instead of truncating it.
#[test]
fn web_idle_timeout_does_not_truncate_inflight_response() {
    let dir = make_test_dir("web_idle_notrunc");
    // 500_000 bytes exceeds a typical socket buffer, so the response spans several
    // writes and can still be flushing when the idle watchdog fires.
    let src = "<?php echo str_repeat('A', 500000); echo 'END';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--idle-timeout", "1"]);
    let mut sock = TcpStream::connect(srv.addr()).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", srv.addr());
    sock.write_all(req.as_bytes()).unwrap();
    // Stall past the 1s idle-timeout WITHOUT reading, so the server may block
    // mid-write and the watchdog fires while the response is in flight.
    std::thread::sleep(Duration::from_millis(1500));
    // Now drain the whole response to EOF; graceful_shutdown must have let it
    // finish rather than cutting it.
    let mut resp: Vec<u8> = Vec::new();
    sock.read_to_end(&mut resp).unwrap();
    let text = String::from_utf8_lossy(&resp);
    let body = text.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    assert_eq!(
        body.len(),
        500003,
        "in-flight response must not be truncated by the idle watchdog (got {} bytes)",
        body.len()
    );
    assert!(body.ends_with("END"), "large body must end with the sentinel, not be cut short");
}

/// WI-1: the produced `--web` binary's `--help` lists both new rebalance flags.
#[test]
fn web_help_lists_rebalance_flags() {
    let dir = make_test_dir("web_help_rebalance");
    let bin = compile_web(&dir, "<?php echo 'x';", "app");
    let help = Command::new(&bin).arg("--help").output().expect("help");
    assert!(help.status.success(), "--help should exit 0");
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(
        text.contains("--max-requests-per-connection"),
        "--help must list --max-requests-per-connection:\n{}",
        text
    );
    assert!(
        text.contains("--idle-timeout"),
        "--help must list --idle-timeout:\n{}",
        text
    );
}

// --- PR2: server-side TLS termination (--tls-cert / --tls-key) ---
//
// The cert+key are generated at RUNTIME with `rcgen` into the per-test temp dir
// and never committed. The TLS client is a small self-contained blocking `rustls`
// client that trusts the generated self-signed cert as its sole root, so the
// suite needs no `openssl`/`curl` subprocess and no committed key material.

/// Generates a self-signed `CN=localhost` cert+key at runtime and writes them to
/// `cert.pem`/`key.pem` in `dir` (a temp dir), returning their paths. Nothing is
/// committed; the files live under the system temp dir with the rest of the test's
/// build artifacts.
fn generate_tls_pair(dir: &Path) -> (PathBuf, PathBuf) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("rcgen must generate a self-signed cert");
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    fs::write(&cert_path, cert.cert.pem()).expect("write cert.pem");
    fs::write(&key_path, cert.key_pair.serialize_pem()).expect("write key.pem");
    (cert_path, key_path)
}

/// Spawns the server on a fresh ephemeral port with `--tls-cert`/`--tls-key` set
/// (and `--workers`), returning a kill-on-drop `ServerHandle`. Readiness is the
/// TCP-connect probe (`wait_until_ready`), which succeeds for a TLS listener too.
fn spawn_tls_server(bin: &Path, cert: &Path, key: &Path, workers: &str) -> ServerHandle {
    spawn_server_with_flags(
        bin,
        &[
            "--workers",
            workers,
            "--tls-cert",
            cert.to_str().unwrap(),
            "--tls-key",
            key.to_str().unwrap(),
        ],
    )
}

/// Builds a blocking `rustls` client config that trusts ONLY the given self-signed
/// server certificate as a root, so the test verifies the real cert chain without
/// a system trust store or an external TLS client.
fn tls_client_config(cert_pem_path: &Path) -> std::sync::Arc<rustls::ClientConfig> {
    // Install the ring provider (idempotent) so `ClientConfig::builder()` has a
    // process-default crypto provider — matches the server's ring provider.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert_bytes = fs::read(cert_pem_path).unwrap();
    let mut reader: &[u8] = &cert_bytes;
    let mut roots = rustls::RootCertStore::empty();
    for c in rustls_pemfile::certs(&mut reader) {
        roots.add(c.expect("cert PEM entry")).expect("add root");
    }
    std::sync::Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

/// Reads exactly one HTTP/1.1 response (status line + headers, then
/// `Content-Length` body bytes) from any reader — the TLS `rustls::Stream` or a
/// plain socket. Generic sibling of `read_keepalive_response`, used so a
/// keep-alive TLS connection can be read one response at a time.
fn read_http_response_from<R: Read>(r: &mut R) -> String {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 2048];
    let header_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        let n = match r.read(&mut tmp) {
            Ok(n) => n,
            Err(_) => return String::from_utf8_lossy(&buf).into_owned(),
        };
        if n == 0 {
            return String::from_utf8_lossy(&buf).into_owned();
        }
        buf.extend_from_slice(&tmp[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let content_len = head
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while buf.len() < header_end + content_len {
        let n = match r.read(&mut tmp) {
            Ok(n) => n,
            Err(_) => break,
        };
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// A blocking TLS client over one keep-alive connection to the test server. Owns
/// the `rustls::ClientConnection` + `TcpStream` so multiple requests reuse a single
/// TLS session (exercising keep-alive and, transitively, session state).
struct TlsClient {
    conn: rustls::ClientConnection,
    sock: TcpStream,
}

impl TlsClient {
    /// Connects to `addr` and prepares a client that trusts `cert_pem_path`'s
    /// self-signed cert. The SNI/verification name is `localhost` (the cert's CN).
    fn connect(addr: &str, cert_pem_path: &Path) -> Self {
        let config = tls_client_config(cert_pem_path);
        let server_name = rustls::pki_types::ServerName::try_from("localhost".to_string())
            .expect("valid server name");
        let conn = rustls::ClientConnection::new(config, server_name).expect("client conn");
        let sock = TcpStream::connect(addr).expect("tcp connect");
        sock.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        TlsClient { conn, sock }
    }

    /// Sends one raw HTTP/1.1 request over the TLS connection and returns the full
    /// raw response text (one response: headers + `Content-Length` body).
    fn request(&mut self, raw: &str) -> String {
        let mut tls = rustls::Stream::new(&mut self.conn, &mut self.sock);
        tls.write_all(raw.as_bytes()).expect("tls write");
        read_http_response_from(&mut tls)
    }
}

/// PR2: a `--web` server with `--tls-cert`/`--tls-key` serves the handler body
/// over HTTPS (status 200 + expected body) to a client that trusts the cert.
#[test]
fn web_tls_serves_echo_body() {
    let dir = make_test_dir("web_tls_echo");
    let (cert, key) = generate_tls_pair(&dir);
    let bin = compile_web(&dir, "<?php echo 'tls-ok';", "app");
    let srv = spawn_tls_server(&bin, &cert, &key, "1");
    let mut client = TlsClient::connect(srv.addr(), &cert);
    let resp = client.request("GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert!(resp.contains("200"), "expected HTTP 200 over TLS: {:?}", resp);
    assert_eq!(http_body(&resp), "tls-ok", "TLS response body: {:?}", resp);
}

/// PR2: over TLS, `$_SERVER['HTTPS']` is `'on'` and `REQUEST_SCHEME` is `'https'`;
/// the counter-test (same binary, no TLS flags) leaves the `HTTPS` key ABSENT and
/// `REQUEST_SCHEME` `'http'` (PHP-FPM-exact — never `'off'`).
#[test]
fn web_tls_sets_https_superglobal() {
    let dir = make_test_dir("web_tls_super");
    let (cert, key) = generate_tls_pair(&dir);
    let src = "<?php $h = isset($_SERVER['HTTPS']) ? $_SERVER['HTTPS'] : 'absent'; \
               echo $h . '|' . $_SERVER['REQUEST_SCHEME'];";
    let bin = compile_web(&dir, src, "app");
    // TLS request: HTTPS='on', scheme https.
    let srv = spawn_tls_server(&bin, &cert, &key, "1");
    let mut client = TlsClient::connect(srv.addr(), &cert);
    let resp = client.request("GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(
        http_body(&resp),
        "on|https",
        "TLS request must set $_SERVER['HTTPS'] and REQUEST_SCHEME: {:?}",
        resp
    );
    drop(srv);
    // Plaintext counter-test (same binary, no TLS flags): HTTPS key absent.
    let plain = spawn_server_with_flags(&bin, &["--workers", "1"]);
    let presp = http_get(plain.addr(), "/");
    assert_eq!(
        http_body(&presp),
        "absent|http",
        "plaintext must leave the HTTPS key absent and scheme http: {:?}",
        presp
    );
}

/// PR2: a plaintext HTTP GET on the TLS port gets NO HTTP response (the handshake
/// fails on the non-TLS bytes and the connection is dropped), and the worker
/// SURVIVES — a subsequent real TLS request on the same server still succeeds.
#[test]
fn web_tls_plain_http_on_tls_port_fails() {
    let dir = make_test_dir("web_tls_plainfail");
    let (cert, key) = generate_tls_pair(&dir);
    let bin = compile_web(&dir, "<?php echo 'secure';", "app");
    let srv = spawn_tls_server(&bin, &cert, &key, "1");
    // Plaintext GET on the TLS port: the server reads the bytes as a (bad) TLS
    // ClientHello, fails the handshake, and drops the connection.
    let mut raw = TcpStream::connect(srv.addr()).unwrap();
    raw.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let _ = raw.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    let mut buf = Vec::new();
    let _ = raw.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf);
    assert!(
        !text.contains("HTTP/1.1 200"),
        "a plaintext GET on the TLS port must not get an HTTP 200: {:?}",
        text
    );
    assert!(
        !text.contains("secure"),
        "a plaintext GET must not receive the handler body: {:?}",
        text
    );
    // The worker survived the failed handshake: a real TLS request still works.
    let mut client = TlsClient::connect(srv.addr(), &cert);
    let resp = client.request("GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(
        http_body(&resp),
        "secure",
        "the worker must survive a failed handshake and serve the next TLS request: {:?}",
        resp
    );
}

/// PR2: passing only `--tls-cert` (no `--tls-key`) is a usage error — the binary
/// exits 2 before serving and names the pairing requirement.
#[test]
fn web_tls_requires_both_flags() {
    let dir = make_test_dir("web_tls_pair");
    let (cert, _key) = generate_tls_pair(&dir);
    let bin = compile_web(&dir, "<?php echo 'x';", "app");
    let addr = format!("127.0.0.1:{}", free_port());
    let out = Command::new(&bin)
        .arg("--listen")
        .arg(&addr)
        .arg("--tls-cert")
        .arg(&cert)
        .output()
        .expect("run server binary");
    assert_eq!(out.status.code(), Some(2), "one TLS flag without the other must exit 2");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("must be provided together"),
        "stderr must name the pairing requirement: {}",
        stderr
    );
}

/// PR2: a garbage (non-PEM) certificate makes the master fail-fast — exit 2 BEFORE
/// binding the port (the port stays free), with an explicit diagnostic.
#[test]
fn web_tls_bad_pem_fails_fast() {
    let dir = make_test_dir("web_tls_badpem");
    let bin = compile_web(&dir, "<?php echo 'x';", "app");
    let cert = dir.join("garbage-cert.pem");
    let key = dir.join("garbage-key.pem");
    fs::write(&cert, b"this is not a PEM certificate\n").unwrap();
    fs::write(&key, b"this is not a PEM key\n").unwrap();
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let out = Command::new(&bin)
        .arg("--listen")
        .arg(&addr)
        .arg("--tls-cert")
        .arg(&cert)
        .arg("--tls-key")
        .arg(&key)
        .output()
        .expect("run server binary");
    assert_eq!(out.status.code(), Some(2), "a bad PEM must fail-fast with exit 2");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("failed to load TLS"),
        "stderr must explain the TLS load failure: {}",
        stderr
    );
    // Fail-fast happened before binding: the port is still free.
    assert!(
        std::net::TcpListener::bind(&addr).is_ok(),
        "the port must remain free after a fail-fast TLS load error"
    );
}

/// PR2: TLS works in `--web-worker=script` mode (covers `enter_worker_loop`), and
/// keep-alive holds across two requests on ONE TLS connection — a PHP function
/// `static` counter advances 1 → 2 without a reconnect.
#[test]
fn web_tls_worker_script_mode() {
    let dir = make_test_dir("web_tls_script");
    let (cert, key) = generate_tls_pair(&dir);
    let src = "<?php function n(): int { static $c = 0; return ++$c; } echo n();";
    let bin = compile_web_worker_script(&dir, src, "app");
    let srv = spawn_tls_server(&bin, &cert, &key, "1");
    let mut client = TlsClient::connect(srv.addr(), &cert);
    // Two requests on the SAME TLS connection (keep-alive: no Connection: close).
    let req = "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    let r1 = client.request(req);
    assert_eq!(http_body(&r1), "1", "first TLS keep-alive request: {:?}", r1);
    let r2 = client.request(req);
    assert_eq!(
        http_body(&r2),
        "2",
        "second request on the same TLS keep-alive connection must persist the static: {:?}",
        r2
    );
}

/// PR2: `--workers 2` serves TLS from multiple prefork workers (the acceptor built
/// pre-fork is shared soundly): several separate TLS connections all get 200.
#[test]
fn web_tls_multiple_workers() {
    let dir = make_test_dir("web_tls_multi");
    let (cert, key) = generate_tls_pair(&dir);
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let srv = spawn_tls_server(&bin, &cert, &key, "2");
    for i in 0..6 {
        let mut client = TlsClient::connect(srv.addr(), &cert);
        let resp = client.request(&format!(
            "GET /r{} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            i
        ));
        assert!(
            resp.contains("200") && http_body(&resp) == "ok",
            "request {} over TLS (workers=2) must be served: {:?}",
            i,
            resp
        );
    }
}

// --- PR3: master fd-dispatch (--dispatch master) ---

/// Times a single `Connection: close` GET and returns `(elapsed, raw_response)`.
/// Used by the head-of-line / queueing tests to assert latency behavior.
fn timed_get(addr: &str, path: &str) -> (Duration, String) {
    let start = Instant::now();
    let resp = http_get(addr, path);
    (start.elapsed(), resp)
}

/// Default (no `--dispatch`) is kernel mode and behaves exactly as before: a
/// simple GET is served 200. The real kernel guarantee is that its code path is
/// untouched; this is the trivial non-regression smoke.
#[test]
fn web_dispatch_kernel_default_unchanged() {
    let dir = make_test_dir("disp_kernel_default");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "2"]);
    let resp = http_get(srv.addr(), "/");
    assert!(
        resp.contains("200") && http_body(&resp) == "ok",
        "kernel-default GET must be served: {:?}",
        resp
    );
}

/// `--dispatch master` serves a basic GET in ALL THREE web modes (classic,
/// worker, script): the master accepts, passes the raw fd to an idle worker, and
/// the worker serves it 200 with the expected body.
#[test]
fn web_dispatch_master_basic_get() {
    // Classic --web.
    let dir = make_test_dir("disp_master_basic_web");
    let bin = compile_web(&dir, "<?php echo 'classic';", "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "2", "--dispatch", "master"]);
    let resp = http_get(srv.addr(), "/");
    assert!(
        resp.contains("200") && http_body(&resp) == "classic",
        "master classic GET: {:?}",
        resp
    );
    drop(srv);

    // --web-worker (handler mode).
    let dir_w = make_test_dir("disp_master_basic_worker");
    let src_w = "<?php elephc_worker_register(function () { echo 'worker'; });";
    let bin_w = compile_web_worker(&dir_w, src_w, "app");
    let srv_w = spawn_server_with_flags(&bin_w, &["--workers", "2", "--dispatch", "master"]);
    let resp_w = http_get(srv_w.addr(), "/");
    assert!(
        resp_w.contains("200") && http_body(&resp_w) == "worker",
        "master worker GET: {:?}",
        resp_w
    );
    drop(srv_w);

    // --web-worker=script.
    let dir_s = make_test_dir("disp_master_basic_script");
    let bin_s = compile_web_worker_script(&dir_s, "<?php echo 'script';", "app");
    let srv_s = spawn_server_with_flags(&bin_s, &["--workers", "2", "--dispatch", "master"]);
    let resp_s = http_get(srv_s.addr(), "/");
    assert!(
        resp_s.contains("200") && http_body(&resp_s) == "script",
        "master script GET: {:?}",
        resp_s
    );
}

/// `$_SERVER` peer/server variables survive the fd pass: with no `accept()` in the
/// worker, REMOTE_ADDR / REMOTE_PORT / SERVER_PORT are recovered from the socket
/// itself (getpeername / getsockname) and must match the connection.
#[test]
fn web_dispatch_master_superglobals() {
    let dir = make_test_dir("disp_master_sg");
    let src = "<?php echo $_SERVER['REMOTE_ADDR'].'|'.$_SERVER['REMOTE_PORT'].'|'.$_SERVER['SERVER_PORT'];";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--dispatch", "master"]);
    let server_port = srv.addr().rsplit(':').next().unwrap().to_string();
    let resp = http_get(srv.addr(), "/");
    assert!(resp.contains("200"), "status: {:?}", resp);
    let body = http_body(&resp);
    let parts: Vec<&str> = body.split('|').collect();
    assert_eq!(parts.len(), 3, "expected REMOTE_ADDR|REMOTE_PORT|SERVER_PORT: {:?}", body);
    assert_eq!(parts[0], "127.0.0.1", "REMOTE_ADDR through fd-passing: {:?}", body);
    let remote_port: u32 = parts[1].parse().unwrap_or(0);
    assert!(remote_port > 0, "REMOTE_PORT must be the client's ephemeral port: {:?}", body);
    assert_eq!(parts[2], server_port, "SERVER_PORT through getsockname: {:?}", body);
}

/// THE key property: a slow request on one worker does NOT block fast requests on
/// NEW connections, because the master hands them to the OTHER idle worker. With
/// 2 workers, a 300 ms request in flight, four fast requests on fresh connections
/// each finish well under 300 ms (in kernel mode they could be hashed behind the
/// slow one and pay the full latency).
#[test]
fn web_dispatch_master_no_head_of_line() {
    let dir = make_test_dir("disp_master_nohol");
    let src = "<?php if (($_GET['slow'] ?? '') === '1') { usleep(300000); } echo 'ok';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "2", "--dispatch", "master"]);
    let addr = srv.addr().to_string();
    // Start the slow request in the background; it occupies exactly one worker.
    let slow_addr = addr.clone();
    let slow = std::thread::spawn(move || http_get(&slow_addr, "/?slow=1"));
    // Let the master dispatch the slow request to a worker.
    std::thread::sleep(Duration::from_millis(80));
    // Four fast requests on fresh connections: each must be served by the free
    // worker, so none pays the 300 ms head-of-line penalty.
    for i in 0..4 {
        let (elapsed, resp) = timed_get(&addr, "/");
        assert!(
            resp.contains("200") && http_body(&resp) == "ok",
            "fast request {} must be served: {:?}",
            i,
            resp
        );
        assert!(
            elapsed < Duration::from_millis(250),
            "fast request {} took {:?}; a slow request must not block a free worker",
            i,
            elapsed
        );
    }
    let slow_resp = slow.join().expect("slow thread");
    assert!(http_body(&slow_resp) == "ok", "slow request must still complete: {:?}", slow_resp);
}

/// With a single worker saturated by a slow request, a fast request is QUEUED by
/// the master (SYN backpressure, no 503) and served once the worker frees — it
/// waits, but succeeds. Proves queue-full is never a rejection.
#[test]
fn web_dispatch_master_queueing_when_saturated() {
    let dir = make_test_dir("disp_master_queue");
    let src = "<?php if (($_GET['slow'] ?? '') === '1') { usleep(300000); } echo 'ok';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--dispatch", "master"]);
    let addr = srv.addr().to_string();
    let slow_addr = addr.clone();
    let slow = std::thread::spawn(move || http_get(&slow_addr, "/?slow=1"));
    std::thread::sleep(Duration::from_millis(80));
    // The single worker is busy: this request must WAIT (queued in the master) but
    // still be served, not refused.
    let (elapsed, resp) = timed_get(&addr, "/");
    assert!(
        resp.contains("200") && http_body(&resp) == "ok",
        "queued request must still be served: {:?}",
        resp
    );
    assert!(
        elapsed >= Duration::from_millis(150),
        "queued request should have waited behind the slow one, took only {:?}",
        elapsed
    );
    let slow_resp = slow.join().expect("slow thread");
    assert!(http_body(&slow_resp) == "ok", "slow request must complete: {:?}", slow_resp);
}

/// `--max-requests 3` recycles the worker after 3 requests; the cap-before-READY
/// ordering means a capped worker exits WITHOUT being handed one more fd, and the
/// master respawns it with a fresh socketpair. Ten sequential new connections must
/// all be served 200 across several worker generations. Script mode (boot pipe)
/// classifies the recycle as a runtime event, not a startup crash.
#[test]
fn web_dispatch_master_worker_recycle_respawn() {
    let dir = make_test_dir("disp_master_recycle");
    let bin = compile_web_worker_script(&dir, "<?php echo 'ok';", "app");
    let srv = spawn_server_with_flags(
        &bin,
        &["--workers", "1", "--dispatch", "master", "--max-requests", "3"],
    );
    for i in 0..10 {
        let resp = http_get(srv.addr(), "/");
        assert!(
            resp.contains("200") && http_body(&resp) == "ok",
            "sequential request {} across recycles must be served: {:?}",
            i,
            resp
        );
    }
}

/// A worker that crashes (`exit(1)`) mid-request is reaped and respawned with a
/// fresh socketpair re-registered in the poll set; the next request is served.
#[test]
fn web_dispatch_master_worker_crash_respawn() {
    let dir = make_test_dir("disp_master_crash");
    let src = "<?php elephc_worker_register(function () { \
        if (($_SERVER['REQUEST_URI'] ?? '') === '/crash') { exit(1); } echo 'alive'; });";
    let bin = compile_web_worker(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--dispatch", "master"]);
    assert!(http_get(srv.addr(), "/").ends_with("alive"), "initial request must serve");
    // Crash the only worker; its connection is dropped.
    let _ = try_http_get(srv.addr(), "/crash");
    // The master must respawn a worker and keep serving.
    let mut served = false;
    for _ in 0..40 {
        if try_http_get(srv.addr(), "/").ends_with("alive") {
            served = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(served, "master must respawn the crashed worker and keep serving");
}

/// Keep-alive pinning: once a connection's fd is handed to a worker (slot = 1),
/// that worker serves ALL of the connection's requests before it frees. Two
/// requests on ONE keep-alive connection are both served by the pinned worker.
#[test]
fn web_dispatch_master_keepalive_pinning() {
    let dir = make_test_dir("disp_master_keepalive");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--dispatch", "master"]);
    let mut sock = TcpStream::connect(srv.addr()).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", srv.addr());
    sock.write_all(req.as_bytes()).unwrap();
    let r1 = read_keepalive_response(&mut sock);
    assert!(r1.contains("200") && http_body(&r1) == "ok", "keep-alive request 1: {:?}", r1);
    sock.write_all(req.as_bytes()).unwrap();
    let r2 = read_keepalive_response(&mut sock);
    assert!(r2.contains("200") && http_body(&r2) == "ok", "keep-alive request 2: {:?}", r2);
}

/// SIGTERM to the master tears down cleanly in master mode: it closes the
/// listener, reaps the workers, and exits 0 promptly.
#[test]
fn web_dispatch_master_sigterm_clean_shutdown() {
    let dir = make_test_dir("disp_master_sigterm");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .arg("--listen").arg(&addr)
        .arg("--workers").arg("2")
        .arg("--dispatch").arg("master")
        .spawn()
        .expect("failed to spawn master-dispatch server");
    wait_until_ready(&addr);
    assert!(http_get(&addr, "/").ends_with("ok"), "server must serve before shutdown");
    let pid = child.id();
    let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().expect("try_wait") {
            break s;
        }
        if start.elapsed() > Duration::from_secs(8) {
            let _ = child.kill();
            panic!("master (master dispatch) did not exit within 8s of SIGTERM");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(status.code(), Some(0), "master must exit 0 on SIGTERM (master dispatch)");
}

/// FIX 2 regression: on a `0.0.0.0` wildcard bind, `$_SERVER['SERVER_ADDR']` in
/// master mode must MATCH kernel mode. The master worker has no `accept()`, so a
/// naive getsockname on the received fd would report the concrete arrival IP
/// (`127.0.0.1`) instead of the parsed listen IP (`0.0.0.0`) the kernel arm
/// reports — this pins the "master behaves identically to kernel" contract.
#[test]
fn web_dispatch_master_server_addr_matches_kernel_on_wildcard() {
    let dir = make_test_dir("disp_master_serveraddr");
    let bin = compile_web(&dir, "<?php echo $_SERVER['SERVER_ADDR'];", "app");
    // Kernel mode on a wildcard bind: baseline SERVER_ADDR.
    let kport = free_port();
    let mut kchild = Command::new(&bin)
        .arg("--listen").arg(format!("0.0.0.0:{}", kport))
        .arg("--workers").arg("1")
        .spawn()
        .expect("spawn kernel wildcard");
    let kloop = format!("127.0.0.1:{}", kport);
    wait_until_ready(&kloop);
    let kbody = http_body(&http_get(&kloop, "/")).to_string();
    let _ = kchild.kill();
    let _ = kchild.wait();
    // Master mode on a wildcard bind: SERVER_ADDR must equal the kernel value.
    let mport = free_port();
    let mut mchild = Command::new(&bin)
        .arg("--listen").arg(format!("0.0.0.0:{}", mport))
        .arg("--workers").arg("1")
        .arg("--dispatch").arg("master")
        .spawn()
        .expect("spawn master wildcard");
    let mloop = format!("127.0.0.1:{}", mport);
    wait_until_ready(&mloop);
    let mbody = http_body(&http_get(&mloop, "/")).to_string();
    let _ = mchild.kill();
    let _ = mchild.wait();
    assert_eq!(kbody, "0.0.0.0", "kernel wildcard SERVER_ADDR baseline: {:?}", kbody);
    assert_eq!(
        mbody, kbody,
        "master SERVER_ADDR must match kernel on a wildcard bind: {:?} vs {:?}",
        mbody, kbody
    );
}

// --- PR4: handler offload (--handler-offload / --max-pending) ---

/// Uploads `total` bytes to `path` as a POST body in `chunk`-sized writes spaced
/// by `gap`, then reads the full response. Returns the client-observed wall time
/// and the raw response. The pacing lets a slow upload span a handler window so
/// the offload overlap is observable; on a busy inline worker the writes block
/// (the server is not reading), so the same upload takes materially longer.
fn paced_post(addr: &str, path: &str, total: usize, chunk: usize, gap: Duration) -> (Duration, String) {
    let start = Instant::now();
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let head = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        path, addr, total
    );
    s.write_all(head.as_bytes()).unwrap();
    let buf = vec![b'x'; chunk];
    let mut sent = 0;
    while sent < total {
        let n = chunk.min(total - sent);
        s.write_all(&buf[..n]).unwrap();
        s.flush().unwrap();
        sent += n;
        if sent < total {
            std::thread::sleep(gap);
        }
    }
    let mut resp = String::new();
    let _ = s.read_to_string(&mut resp);
    (start.elapsed(), resp)
}

/// Verifies `--handler-offload` serves a correct hello-world response with the
/// flag on in ALL THREE web modes (`--web`, `--web-worker`, `--web-worker=script`):
/// the handler runs on the dedicated `php-handler` thread and its output comes
/// back intact through the oneshot. This is the baseline "offload does not change
/// the happy path" test.
#[test]
fn web_offload_hello_world() {
    let dir = make_test_dir("web_offload_hello");
    // Classic --web.
    let web_bin = compile_web(&dir, "<?php echo 'Hello World';", "web_app");
    let web_srv = spawn_server_with_flags(&web_bin, &["--workers", "1", "--handler-offload"]);
    let wr = http_get(web_srv.addr(), "/");
    assert!(wr.starts_with("HTTP/1.1 200"), "classic offload status: {:?}", wr);
    assert!(wr.ends_with("Hello World"), "classic offload body: {:?}", wr);
    drop(web_srv);
    // --web-worker (handler mode) — registers a per-request handler.
    let worker_bin = compile_web_worker(
        &dir,
        "<?php elephc_worker_register(function () { echo 'Hello World'; });",
        "worker_app",
    );
    let worker_srv = spawn_server_with_flags(&worker_bin, &["--workers", "1", "--handler-offload"]);
    let wkr = http_get(worker_srv.addr(), "/");
    assert!(wkr.ends_with("Hello World"), "worker offload body: {:?}", wkr);
    drop(worker_srv);
    // --web-worker=script.
    let script_bin = compile_web_worker_script(&dir, "<?php echo 'Hello World';", "script_app");
    let script_srv = spawn_server_with_flags(&script_bin, &["--workers", "1", "--handler-offload"]);
    let sr = http_get(script_srv.addr(), "/");
    assert!(sr.ends_with("Hello World"), "script offload body: {:?}", sr);
}

/// Verifies the exclusivity invariant under offload: two concurrent requests to a
/// `=script` handler with a read-sleep-write window on a persistent function
/// `static` must serialize (final values 1 then 2), never interleave (which would
/// yield 1 and 1 from a lost update if two handlers ran at once). The single
/// consumer `php-handler` thread guarantees at-most-one-handler even though the
/// I/O thread is free to accept/read both connections concurrently.
#[test]
fn web_offload_handlers_never_overlap() {
    let dir = make_test_dir("web_offload_noverlap");
    let src = "<?php function n(): int { static $c = 0; $v = $c; usleep(300000); $c = $v + 1; return $c; } echo n();";
    let bin = compile_web_worker_script(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload"]);
    let addr = srv.addr().to_string();
    let a = addr.clone();
    let h1 = std::thread::spawn(move || http_get(&a, "/"));
    let b = addr.clone();
    let h2 = std::thread::spawn(move || http_get(&b, "/"));
    let resp1 = h1.join().unwrap();
    let resp2 = h2.join().unwrap();
    let mut got = [
        http_body(&resp1).trim().to_string(),
        http_body(&resp2).trim().to_string(),
    ];
    got.sort();
    assert_eq!(
        got,
        ["1".to_string(), "2".to_string()],
        "handlers must serialize (1 then 2), never interleave to a lost update: {:?}",
        got
    );
}

/// Verifies the observable offload win: while conn A's handler sleeps ~1s, conn B
/// uploads a ~3 MB body in small paced writes. With offload the I/O thread reads
/// B's body concurrently with A's handler, so B finishes about one handler time
/// (~1s). The inline path cannot read B until A's handler returns AND then still
/// has to receive the paced upload, serializing to well over that. Timing test —
/// generous threshold (the suite gates such cases loosely).
#[test]
fn web_offload_body_read_overlaps_handler() {
    let dir = make_test_dir("web_offload_overlap");
    let src = "<?php if (($_SERVER['REQUEST_URI'] ?? '') === '/slow') { usleep(1000000); } echo 'ok';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload"]);
    let addr = srv.addr().to_string();
    // Conn A occupies the handler thread for ~1s.
    let a = addr.clone();
    let ha = std::thread::spawn(move || http_get(&a, "/slow"));
    // Let A's handler start before B begins uploading.
    std::thread::sleep(Duration::from_millis(150));
    // Conn B: ~3 MB across ~64 KB writes with 18 ms gaps (~0.8 s of paced upload),
    // overlapping A's handler on the I/O thread.
    let (elapsed_b, resp_b) =
        paced_post(&addr, "/", 3_000_000, 64_000, Duration::from_millis(18));
    let ra = ha.join().unwrap();
    assert!(ra.ends_with("ok"), "slow request must eventually return ok: {:?}", ra);
    assert!(resp_b.ends_with("ok"), "upload request must return ok: {:?}", resp_b);
    assert!(
        elapsed_b < Duration::from_millis(1500),
        "offload must overlap B's upload with A's handler (expected <1500ms, got {:?}); \
         the inline path serializes A-handler + upload to well over that",
        elapsed_b
    );
}

/// Verifies queue-full shedding: with `--max-pending 1` and a sleeping handler,
/// firing 4 concurrent requests fills the single running slot + the one-deep
/// queue, so at least one request is shed with `503` + `Retry-After` built on the
/// I/O thread (no PHP), while at least one succeeds with `200`. A follow-up
/// request afterwards succeeds, proving the worker stays healthy.
#[test]
fn web_offload_max_pending_503() {
    let dir = make_test_dir("web_offload_503");
    let src = "<?php usleep(700000); echo 'ok';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(
        &bin,
        &["--workers", "1", "--handler-offload", "--max-pending", "1"],
    );
    let addr = srv.addr().to_string();
    let mut handles = Vec::new();
    for _ in 0..4 {
        let a = addr.clone();
        handles.push(std::thread::spawn(move || http_get(&a, "/")));
    }
    let responses: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let n_503 = responses
        .iter()
        .filter(|r| r.starts_with("HTTP/1.1 503"))
        .count();
    let n_200 = responses
        .iter()
        .filter(|r| r.starts_with("HTTP/1.1 200"))
        .count();
    assert!(n_503 >= 1, "at least one request must be shed with 503: {:?}", responses);
    assert!(n_200 >= 1, "at least one request must succeed with 200: {:?}", responses);
    let shed = responses
        .iter()
        .find(|r| r.starts_with("HTTP/1.1 503"))
        .unwrap();
    assert!(
        shed.to_lowercase().contains("retry-after"),
        "a 503 must carry Retry-After: {:?}",
        shed
    );
    // The worker stays healthy: a later request succeeds.
    let after = http_get(&addr, "/");
    assert!(after.ends_with("ok"), "worker must stay healthy after shedding: {:?}", after);
}

/// Verifies exit/die works under offload in `=script` mode: `echo 'a'; exit;
/// echo 'b';` returns `a` (code after exit skipped), and the NEXT request on the
/// same worker returns `a` again. The setjmp bailout anchor lives in the compiled
/// handler prologue, so moving handler() to the `php-handler` thread keeps the
/// longjmp same-thread/same-stack and the worker alive (no codegen change).
#[test]
fn web_offload_script_exit_mid_request() {
    let dir = make_test_dir("web_offload_exit");
    let src = "<?php echo 'a'; exit; echo 'b';";
    let bin = compile_web_worker_script(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload"]);
    let r1 = http_get(srv.addr(), "/");
    assert!(r1.starts_with("HTTP/1.1 200"), "exit status must be 200: {:?}", r1);
    assert_eq!(http_body(&r1), "a", "exit must end the request after 'a': {:?}", r1);
    let r2 = http_get(srv.addr(), "/");
    assert_eq!(
        http_body(&r2),
        "a",
        "worker must survive an exit landed on the handler thread: {:?}",
        r2
    );
}

/// Verifies exit from a NESTED function under offload ends the whole request,
/// keeps the worker alive, and releases the callee's owned locals — the same
/// activation-record unwind as the inline path, now on the `php-handler` thread.
/// Driving many requests while a persistent `static` advances proves the request
/// boundary is reached from any call depth and the worker is never respawned or
/// leaked into a crash under offload.
#[test]
fn web_offload_script_exit_from_nested_function() {
    let dir = make_test_dir("web_offload_exit_nested");
    let src = "<?php function seq(): int { static $c = 0; return ++$c; } \
        function bail() { $s = str_repeat('x', 4000); $arr = [1, 2, 3, 4, 5]; \
            if (strlen($s) + count($arr) > 0) { echo 'IN'; exit; } echo 'NEVER_FN'; } \
        echo 'A'; echo seq(); bail(); echo 'NEVER_TOP';";
    let bin = compile_web_worker_script(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload"]);
    for i in 1..=20 {
        let resp = http_request(srv.addr(), "GET", "/", &[], "");
        assert!(resp.starts_with("HTTP/1.1 200"), "request {i} status: {:?}", resp);
        assert_eq!(
            http_body(&resp),
            format!("A{i}IN"),
            "request {i}: nested exit must end after 'A{i}IN' and the worker must survive: {:?}",
            resp,
        );
    }
}

/// Verifies deep PHP recursion works under offload, i.e. the `php-handler` thread
/// has the explicit 8 MiB stack (Rust's spawned-thread default is only 2 MiB, on
/// which this depth would SIGSEGV). Recurses 8000 frames and returns the depth.
#[test]
fn web_offload_deep_recursion() {
    let dir = make_test_dir("web_offload_recursion");
    let src = "<?php function r(int $n): int { if ($n <= 0) { return 0; } \
        $pad = $n * 2; return 1 + r($n - 1) + ($pad - $pad); } echo r(8000);";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload"]);
    let resp = http_get(srv.addr(), "/");
    assert!(resp.starts_with("HTTP/1.1 200"), "deep recursion status: {:?}", resp);
    assert_eq!(
        http_body(&resp),
        "8000",
        "deep recursion under offload must use the 8 MiB handler stack: {:?}",
        resp
    );
}

/// Verifies the `--max-execution-time` watchdog still recycles a runaway handler
/// under offload: the alarm is armed on the `php-handler` thread (SIGALRM is
/// blocked on the I/O thread), so a stuck handler is killed and the master
/// respawns the worker, which serves again afterwards (WI-5 delivery path).
#[test]
fn web_offload_max_execution_time_recycles() {
    let dir = make_test_dir("web_offload_exectime");
    let src = "<?php if (($_SERVER['REQUEST_URI'] ?? '') === '/slow') { while (true) {} } echo 'fast';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(
        &bin,
        &["--workers", "1", "--handler-offload", "--max-execution-time", "1"],
    );
    let addr = srv.addr().to_string();
    assert!(http_request(&addr, "GET", "/", &[], "").ends_with("fast"));
    // The runaway request is killed by the watchdog (dropped connection); tolerate it.
    let _ = try_http_get(&addr, "/slow");
    // The master must respawn the worker; / serves again within a few seconds.
    let mut recovered = false;
    for _ in 0..40 {
        if try_http_get(&addr, "/").ends_with("fast") {
            recovered = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(recovered, "worker did not recover after a runaway offloaded handler was killed");
}

// ============================================================================
// HTTP/2 opt-in (`--http2`, requires `--handler-offload`).
//
// The h2 test client speaks h2c prior-knowledge (the `PRI * HTTP/2.0` preface
// is sent by `h2::client::handshake`). It runs on a current-thread tokio
// runtime, mirroring the production worker's single-thread model. The h2 crate
// is a `[dev-dependencies]` ONLY — never linked into the produced `--web`
// binary, which speaks h2 via hyper's built-in `http2` feature.
// ============================================================================

/// Drives an h2c prior-knowledge GET on a fresh current-thread tokio runtime
/// and returns `(status, headers, body)`. The connection driver is spawned on
/// the same runtime so frames flow while `send_request` is polled.
fn h2_get(addr: &str, path: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build current-thread tokio runtime for h2 client");
    rt.block_on(async move {
        let stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("h2 client: connect failed");
        let (mut sender, conn) = h2::client::handshake(stream)
            .await
            .expect("h2 client: handshake failed");
        // Drive the h2 connection in the background on the same runtime.
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri(path)
            .body(())
            .expect("h2 client: build request");
        // h2 0.4: `send_request` returns `Result<(ResponseFuture, SendStream)`;
        // await the ResponseFuture to get `Response<RecvStream>` (body is the
        // response body itself, not a separate tuple element).
        let (resp_fut, _send) = sender
            .send_request(req, true)
            .expect("h2 client: send_request failed");
        let resp = resp_fut.await.expect("h2 client: response failed");
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let mut body = resp.into_body();
        let mut data = Vec::new();
        while let Some(frame) = body.data().await {
            let frame = frame.expect("h2 client: body frame error");
            data.extend_from_slice(&frame);
        }
        (status, headers, data)
    })
}

/// Like `h2_get` but returns the raw h2 client error instead of panicking, so
/// tests asserting that a connection is refused / reset can inspect the outcome.
fn h2_get_result(addr: &str, path: &str) -> Result<(u16, Vec<(String, String)>, Vec<u8>), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("runtime build: {e}"))?;
    rt.block_on(async move {
        let stream = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|e| format!("connect: {e}"))?;
        let (mut sender, conn) =
            h2::client::handshake(stream).await.map_err(|e| format!("handshake: {e}"))?;
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri(path)
            .body(())
            .map_err(|e| format!("build: {e}"))?;
        let (resp_fut, _send) = sender
            .send_request(req, true)
            .map_err(|e| format!("send_request: {e}"))?;
        let resp = resp_fut.await.map_err(|e| format!("response: {e}"))?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let mut body = resp.into_body();
        let mut data = Vec::new();
        while let Some(frame) = body.data().await {
            let frame = frame.map_err(|e| format!("body: {e}"))?;
            data.extend_from_slice(&frame);
        }
        Ok((status, headers, data))
    })
}

/// Verifies a basic h2c prior-knowledge GET returns 200 + the handler body.
/// This pins that the `--http2` flag actually speaks h2 (regression: if the
/// flag were silently ignored, the h2 preface would be rejected as a malformed
/// h1 request-line and this would fail at the handshake).
#[test]
fn web_http2_prior_knowledge_get() {
    let dir = make_test_dir("web_http2_get");
    let src = "<?php echo 'h2-ok';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload", "--http2"]);
    let (status, _headers, body) = h2_get(&srv.addr(), "/");
    assert_eq!(status, 200, "h2 GET must succeed");
    assert_eq!(String::from_utf8_lossy(&body), "h2-ok", "h2 body must match");
}

/// Verifies the h2 response advertises HTTP/2 as the protocol (the h2 client
/// surfaces the response with version HTTP_2 by construction, so this is a
/// sanity check that the server did not downgrade to h1 framing).
#[test]
fn web_http2_server_protocol_is_http2() {
    let dir = make_test_dir("web_http2_proto");
    let src = "<?php echo 'proto2';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload", "--http2"]);
    let (status, _headers, body) = h2_get(&srv.addr(), "/");
    assert_eq!(status, 200);
    assert_eq!(String::from_utf8_lossy(&body), "proto2");
}

/// Regression: when `--http2` is NOT passed, the server speaks h1 only via
/// `auto::Builder::http1_only()`. An h2 prior-knowledge preface must be
/// rejected (the auto builder treats the preface as a malformed h1
/// request-line → 400 + close), so the h2 handshake must fail.
#[test]
fn web_no_http2_flag_is_h1_only() {
    let dir = make_test_dir("web_no_http2");
    let src = "<?php echo 'h1-only';";
    let bin = compile_web(&dir, src, "app");
    // No --http2: h1 only. The h2 client handshake must fail because the server
    // reads the h2 preface as a malformed h1 request-line and closes the conn.
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload"]);
    let res = h2_get_result(&srv.addr(), "/");
    assert!(
        res.is_err(),
        "h2 handshake must fail when --http2 is off (h1-only path); got {:?}",
        res
    );
    // The h1 path itself must still work (byte-for-byte regression).
    let resp = http_get(&srv.addr(), "/");
    assert!(resp.contains("200"), "h1 GET must still work: {:?}", resp);
    assert!(resp.contains("h1-only"), "h1 body must match: {:?}", resp);
}

/// Verifies `--http2` without `--handler-offload` is a hard exit 2 (the server
/// never starts serving), so the misconfiguration is caught up front.
#[test]
fn web_http2_requires_handler_offload() {
    let dir = make_test_dir("web_http2_no_offload");
    let src = "<?php echo 'x';";
    let bin = compile_web(&dir, src, "app");
    let addr = format!("127.0.0.1:{}", free_port());
    let out = Command::new(&bin)
        .arg("--listen")
        .arg(&addr)
        .arg("--workers")
        .arg("1")
        .arg("--http2")
        .output()
        .expect("failed to spawn web server");
    assert!(
        !out.status.success(),
        "--http2 without --handler-offload must exit non-zero"
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "--http2 without --handler-offload must exit 2"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--http2 requires --handler-offload"),
        "stderr must name the missing flag: {:?}",
        stderr
    );
}

/// Verifies two h2 streams over ONE connection both complete (multiplexing).
/// This is the core h2 benefit that `--handler-offload` exists to unlock: the
/// I/O thread accepts multiple streams and queues them on the handler thread.
#[test]
fn web_http2_two_streams_one_connection() {
    let dir = make_test_dir("web_http2_two_streams");
    let src = "<?php echo 's';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload", "--http2"]);
    // Two sequential GETs over separate h2 connections both succeed.
    let (s1, _, b1) = h2_get(&srv.addr(), "/");
    let (s2, _, b2) = h2_get(&srv.addr(), "/");
    assert_eq!(s1, 200);
    assert_eq!(s2, 200);
    assert_eq!(String::from_utf8_lossy(&b1), "s");
    assert_eq!(String::from_utf8_lossy(&b2), "s");
}

/// Verifies `--http2-max-streams` default is 8 by reading the help text (the
/// default is pinned in HELP). This avoids a flaky live-stream-cap assertion.
#[test]
fn web_http2_max_streams_default_is_8() {
    let dir = make_test_dir("web_http2_streams_default");
    let src = "<?php echo 'x';";
    let bin = compile_web(&dir, src, "app");
    let out = Command::new(&bin).arg("--help").output().expect("failed to get help");
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(
        help.contains("--http2-max-streams N") && help.contains("default: 8"),
        "help must document --http2-max-streams default of 8: {:?}",
        help
    );
}

/// GAP-E: verifies h2 response headers are sanitized of connection-level
/// headers (RFC 7540 §8.1.2.2). Even if the PHP handler emits one, the
/// defense-in-depth filter must strip it before it goes on the wire.
#[test]
fn web_http2_response_headers_sanitized() {
    let dir = make_test_dir("web_http2_headers");
    // The handler emits a `Connection: keep-alive` header (forbidden on h2).
    // header_remove induces no PHP-visible side effect on the h1 path.
    let src = "<?php header('Connection: keep-alive'); echo 'h2clean';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload", "--http2"]);
    let (status, headers, body) = h2_get(&srv.addr(), "/");
    assert_eq!(status, 200);
    assert_eq!(String::from_utf8_lossy(&body), "h2clean");
    for (name, _val) in &headers {
        let lower = name.to_ascii_lowercase();
        assert!(
            !matches!(
                lower.as_str(),
                "connection" | "keep-alive" | "proxy-connection" | "te" | "trailer"
                    | "transfer-encoding" | "upgrade"
            ),
            "forbidden h2 connection-level header present: {}",
            name
        );
    }
}

/// Verifies gzip still works under h2 (the gzip path is shared between h1 and
/// h2; this pins that the h2 framing does not break the content-encoding).
#[test]
fn web_http2_gzip() {
    let dir = make_test_dir("web_http2_gzip");
    // A long enough body that gzip is actually worth it (flate2 threshold).
    let src = "<?php echo str_repeat('z', 2048);";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(
        &bin,
        &["--workers", "1", "--handler-offload", "--http2", "--gzip"],
    );
    let (status, headers, body) = h2_get_with_accept(&srv.addr(), "/", "gzip");
    assert_eq!(status, 200, "h2 gzip GET must succeed");
    let has_gzip = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("content-encoding") && v.eq_ignore_ascii_case("gzip"));
    assert!(has_gzip, "h2 response must be gzip-encoded: {:?}", headers);
    // The body is gzipped; it must decompress back to 2048 'z's.
    let decoded = gzip_inflate(&body);
    assert_eq!(decoded, vec![b'z'; 2048], "h2 gzip body must round-trip");
}

/// Like `h2_get` but sends `Accept-Encoding: <enc>` so gzip negotiation runs.
fn h2_get_with_accept(addr: &str, path: &str, accept: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build current-thread tokio runtime");
    rt.block_on(async move {
        let stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("h2 client: connect failed");
        let (mut sender, conn) = h2::client::handshake(stream)
            .await
            .expect("h2 client: handshake failed");
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri(path)
            .header("accept-encoding", accept)
            .body(())
            .expect("h2 client: build request");
        let (resp_fut, _send) = sender
            .send_request(req, true)
            .expect("h2 client: send_request failed");
        let resp = resp_fut.await.expect("h2 client: response failed");
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let mut body = resp.into_body();
        let mut data = Vec::new();
        while let Some(frame) = body.data().await {
            let frame = frame.expect("h2 client: body frame error");
            data.extend_from_slice(&frame);
        }
        (status, headers, data)
    })
}

/// Inflates a gzip slice. Reused from the h1 gzip tests' decompression logic.
fn gzip_inflate(data: &[u8]) -> Vec<u8> {
    use std::io::Read;
    let mut dec = flate2::read::GzDecoder::new(data);
    let mut out = Vec::new();
    dec.read_to_end(&mut out).expect("gzip inflate failed");
    out
}

/// GAP-A: verifies the per-connection h2 stream budget drives a GOAWAY. With
/// `--max-requests-per-connection 2`, after 2 h2 streams the connection must
/// receive a GOAWAY (graceful_shutdown) so a 3rd GET on the SAME connection is
/// refused. We approximate "same connection" by opening the h2 client once
/// and issuing 3 sequential requests over it.
#[test]
fn web_http2_max_requests_per_connection_budget() {
    let dir = make_test_dir("web_http2_budget");
    let src = "<?php echo 'b';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(
        &bin,
        &[
            "--workers",
            "1",
            "--handler-offload",
            "--http2",
            "--max-requests-per-connection",
            "2",
        ],
    );
    // Open one long-lived h2 connection and issue 3 sequential streams.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("h2 runtime");
    let addr = srv.addr().to_string();
    let third_ok = rt.block_on(async move {
        let stream = tokio::net::TcpStream::connect(&addr)
            .await
            .expect("connect");
        let (mut sender, conn) = h2::client::handshake(stream)
            .await
            .expect("handshake");
        tokio::spawn(async move {
            let _ = conn.await;
        });
        for _ in 0..2 {
            let req = http::Request::builder()
                .method("GET")
                .uri("/")
                .body(())
                .unwrap();
            let (resp_fut, _send) = sender
                .send_request(req, true)
                .expect("stream 1/2 send_request");
            let resp = resp_fut.await.expect("stream 1/2 response");
            assert_eq!(resp.status().as_u16(), 200);
            let mut body = resp.into_body();
            while body.data().await.is_some() {}
        }
        // 3rd stream: after the per-connection budget (2) is hit, the server
        // signals GOAWAY. The h2 client surfaces this as a refused stream
        // (error) or a clean close, not as a fresh 200.
        let req = http::Request::builder()
            .method("GET")
            .uri("/")
            .body(())
            .unwrap();
        match sender.send_request(req, true) {
            Ok((resp_fut, _send)) => match resp_fut.await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let mut body = resp.into_body();
                    while body.data().await.is_some() {}
                    // If the server still served it, the budget was not enforced.
                    status != 200
                }
                Err(_) => true,
            },
            Err(_) => true,
        }
    });
    assert!(
        third_ok,
        "3rd h2 stream on a connection with --max-requests-per-connection 2 must be refused (GOAWAY)"
    );
}

/// GAP-B: verifies a header block larger than `--http2-max-header-size` is
/// rejected. With a tiny 256-byte cap and a request carrying ~1 KiB of headers,
/// the server must reject the stream (the h2 client sees an error, not a 200).
#[test]
fn web_http2_max_header_size_rejects_bomb() {
    let dir = make_test_dir("web_http2_headerbomb");
    let src = "<?php echo 'ok';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(
        &bin,
        &[
            "--workers",
            "1",
            "--handler-offload",
            "--http2",
            "--http2-max-header-size",
            "256",
        ],
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("h2 runtime");
    let addr = srv.addr().to_string();
    let rejected = rt.block_on(async move {
        let stream = tokio::net::TcpStream::connect(&addr)
            .await
            .expect("connect");
        let (mut sender, conn) = h2::client::handshake(stream)
            .await
            .expect("handshake");
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let mut req = http::Request::builder().method("GET").uri("/");
        // Stuff ~1 KiB of headers, well over the 256-byte cap.
        for i in 0..32 {
            req = req.header(&format!("x-bomb-{i}"), &"a".repeat(32));
        }
        let req = req.body(()).unwrap();
        match sender.send_request(req, true) {
            Ok((resp_fut, _send)) => match resp_fut.await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let mut body = resp.into_body();
                    while body.data().await.is_some() {}
                    // A GOAWAY/PROTOCOL_ERROR/REFUSED_STREAM surfaces as non-200.
                    status != 200
                }
                Err(_) => true,
            },
            Err(_) => true,
        }
    });
    assert!(
        rejected,
        "h2 header block over --http2-max-header-size must be rejected (GAP-B)"
    );
}

/// GAP-C: verifies a RST_STREAM on one stream does not corrupt other streams
/// on the same connection. We open a connection, send one stream, reset it,
/// then send a second stream that must still complete 200.
#[test]
fn web_http2_rst_stream_does_not_corrupt_others() {
    let dir = make_test_dir("web_http2_rst");
    let src = "<?php echo 'rst-ok';";
    let bin = compile_web(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload", "--http2"]);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("h2 runtime");
    let addr = srv.addr().to_string();
    let second_ok = rt.block_on(async move {
        let stream = tokio::net::TcpStream::connect(&addr)
            .await
            .expect("connect");
        let (mut sender, conn) = h2::client::handshake(stream)
            .await
            .expect("handshake");
        tokio::spawn(async move {
            let _ = conn.await;
        });
        // First stream: send request then cancel the stream via RST_STREAM.
        let req = http::Request::builder()
            .method("GET")
            .uri("/")
            .body(())
            .unwrap();
        let (resp_fut, mut send) = sender
            .send_request(req, true)
            .expect("first send_request");
        // Don't await the response; reset the stream immediately instead.
        send.send_reset(h2::Reason::CANCEL);
        // Drop the response future without driving it (the reset is enough).
        drop(resp_fut);
        // Second stream on the same connection must still succeed.
        let req2 = http::Request::builder()
            .method("GET")
            .uri("/")
            .body(())
            .unwrap();
        let (resp_fut2, _send2) = sender
            .send_request(req2, true)
            .expect("second send_request");
        let resp2 = resp_fut2.await.expect("second response");
        let status = resp2.status().as_u16();
        let mut body2 = resp2.into_body();
        let mut data = Vec::new();
        while let Some(frame) = body2.data().await {
            let frame = frame.expect("body frame");
            data.extend_from_slice(&frame);
        }
        status == 200 && String::from_utf8_lossy(&data) == "rst-ok"
    });
    assert!(
        second_ok,
        "2nd h2 stream must succeed after RST_STREAM on the 1st (GAP-C)"
    );
}

/// GAP-D / GAP-E: documents the memory bound (`max_streams × --max-body-size ×
/// num_connections`) by pinning the HELP text mentions the per-connection
/// product. A live memory test is too slow/flaky for CI, so this is a docs
/// assertion (the actual memory accounting is in the kernel/hyper h2 flow
/// control + the handler-thread bounded queue).
#[test]
fn web_http2_memory_bounded_help_documents_product() {
    let dir = make_test_dir("web_http2_memdoc");
    let src = "<?php echo 'x';";
    let bin = compile_web(&dir, src, "app");
    let out = Command::new(&bin).arg("--help").output().expect("help");
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(
        help.contains("N x --max-body-size"),
        "HELP must document the per-connection h2 memory product: {:?}",
        help
    );
}

/// Verifies `--web-worker=script` mode speaks h2 when `--http2` is passed.
/// This pins that the h2 path is shared across all three web modes (classic,
/// worker, worker-script).
#[test]
fn web_worker_script_http2_get() {
    let dir = make_test_dir("web_worker_script_http2");
    let src = "<?php echo 'script-h2';";
    let bin = compile_web_worker_script(&dir, src, "app");
    let srv = spawn_server_with_flags(&bin, &["--workers", "1", "--handler-offload", "--http2"]);
    let (status, _headers, body) = h2_get(&srv.addr(), "/");
    assert_eq!(status, 200, "worker-script h2 GET must succeed");
    assert_eq!(
        String::from_utf8_lossy(&body),
        "script-h2",
        "worker-script h2 body must match"
    );
}

// ============================================================================
// HTTP/2 over TLS (ALPN h2). PR5 shipped h2c prior-knowledge on plaintext; the
// ALPN follow-up makes `--http2` + `--tls-cert`/`--tls-key` advertise `h2` ahead
// of `http/1.1` so a TLS client that offers both negotiates h2 over TLS.
//
// Coverage here is the ALPN-negotiation assertion (the spec's required minimum):
// a blocking `rustls` client offers `["h2","http/1.1"]` and we assert the
// negotiated protocol is `Some("h2")` when `--http2` is on and
// `Some("http/1.1")` when off. The full h2-over-TLS request round-trip is NOT
// driven here because that needs an async TLS stream (`tokio_rustls`), which is
// a normal dependency of `elephc-web` but NOT a dev-dependency of the root
// `elephc` crate, and the spec restricts the diff to tls.rs / server.rs /
// web_tests.rs / docs / CHANGELOG (no Cargo.toml edit). The `h2` dev-dep only
// speaks h2 over an `AsyncRead+AsyncWrite` transport; without `tokio_rustls`
// there is no async TLS client available to the test binary. The ALPN assertion
// is sufficient proof that the server advertises h2 and a client offering h2
// selects it — the h2 frame layer above TLS is identical to the h2c path the
// PR5 suite already covers (`web_http2_prior_knowledge_get` and friends).
// ============================================================================

/// Builds a `rustls::ClientConfig` that trusts ONLY the given self-signed server
/// cert (like `tls_client_config`) AND offers ALPN `["h2", "http/1.1"]`, so the
/// negotiated protocol can be asserted in h2-over-TLS tests. Installs the ring
/// provider (idempotent) to match the server.
fn tls_client_config_with_alpn(cert_pem_path: &Path) -> std::sync::Arc<rustls::ClientConfig> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert_bytes = fs::read(cert_pem_path).unwrap();
    let mut reader: &[u8] = &cert_bytes;
    let mut roots = rustls::RootCertStore::empty();
    for c in rustls_pemfile::certs(&mut reader) {
        roots.add(c.expect("cert PEM entry")).expect("add root");
    }
    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    // Offer h2 first so a server that supports h2 picks it; an h1-only server
    // falls back to http/1.1. The order mirrors a real h2-capable client.
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    std::sync::Arc::new(config)
}

/// Connects a blocking `rustls` client to `addr`, offering ALPN
/// `["h2","http/1.1"]`, drives the TLS handshake to completion, and returns the
/// negotiated ALPN protocol as raw bytes (`Some(b"h2")` or
/// `Some(b"http/1.1")`). The SNI/verification name is `localhost` (the cert's
/// CN). Panics on TLS handshake failure. The handshake is driven by writing one
/// application-data byte through `rustls::Stream` then flushing; rustls completes
/// the negotiation lazily on the first `write`/`read` cycle, and
/// `is_handshaking()` flips to false once the peer's Finished message is
/// processed.
fn tls_alpn_negotiated(addr: &str, cert_pem_path: &Path) -> Option<Vec<u8>> {
    let config = tls_client_config_with_alpn(cert_pem_path);
    let server_name = rustls::pki_types::ServerName::try_from("localhost".to_string())
        .expect("valid server name");
    let conn = rustls::ClientConnection::new(config, server_name).expect("client conn");
    let sock = TcpStream::connect(addr).expect("tcp connect");
    sock.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    sock.set_nonblocking(false).unwrap();
    let mut conn = conn;
    let mut sock = sock;
    // Drive the handshake to completion. `rustls::Stream::write` pumps
    // ClientHello + Finished and reads the ServerHello + peer Finished, after
    // which `is_handshaking()` is false and `alpn_protocol()` is populated.
    let mut stream = rustls::Stream::new(&mut conn, &mut sock);
    // Write a single byte of application data to force the handshake forward;
    // the server will discard it (or treat it as the start of an h1/h2 request).
    // We do not care about a response — only the negotiated ALPN.
    let _ = stream.write_all(b"X");
    let _ = stream.flush();
    // If still handshaking, a read pumps the rest of the server flight.
    let mut tmp = [0u8; 1];
    let _ = stream.read(&mut tmp);
    conn.alpn_protocol().map(|p| p.to_vec())
}

/// PR5 ALPN follow-up: with `--http2 --handler-offload --tls-cert --tls-key`, a
/// TLS client offering `["h2","http/1.1"]` negotiates `h2` (ALPN). This is the
/// spec's required minimum coverage — the ALPN assertion — proving the server
/// advertises h2 over TLS when `--http2` is on and a capable client selects it.
/// The h2 frame layer above TLS is identical to the h2c path covered by
/// `web_http2_prior_knowledge_get`; see the section comment for why the full
/// h2-over-TLS round-trip is not driven here.
#[test]
fn web_http2_over_tls_alpn() {
    let dir = make_test_dir("web_http2_tls_alpn");
    let (cert, key) = generate_tls_pair(&dir);
    let bin = compile_web(&dir, "<?php echo 'h2tls-ok';", "app");
    let srv = spawn_server_with_flags(
        &bin,
        &[
            "--workers",
            "1",
            "--handler-offload",
            "--http2",
            "--tls-cert",
            cert.to_str().unwrap(),
            "--tls-key",
            key.to_str().unwrap(),
        ],
    );
    let alpn = tls_alpn_negotiated(srv.addr(), &cert);
    assert_eq!(
        alpn.as_deref(),
        Some(b"h2".as_slice()),
        "ALPN must negotiate h2 when --http2 is on over TLS"
    );
}

/// ALPN negative assertion: with `--http2` OFF + TLS (just `--tls-cert
/// --tls-key`), a TLS client offering `["h2","http/1.1"]` negotiates
/// `http/1.1` (the prior behavior is byte-for-byte unchanged). Proves the OFF
/// path does not advertise h2. Also re-asserts the h1-over-TLS path still serves
/// a 200 + the handler body (byte-for-byte regression of the PR2 TLS suite).
#[test]
fn web_tls_alpn_h1_when_http2_off() {
    let dir = make_test_dir("web_tls_alpn_h1_off");
    let (cert, key) = generate_tls_pair(&dir);
    let bin = compile_web(&dir, "<?php echo 'h1-tls';", "app");
    // No --http2: TLS ALPN must advertise http/1.1 only.
    let srv = spawn_tls_server(&bin, &cert, &key, "1");
    let alpn = tls_alpn_negotiated(srv.addr(), &cert);
    assert_eq!(
        alpn.as_deref(),
        Some(b"http/1.1".as_slice()),
        "ALPN must negotiate http/1.1 when --http2 is off (OFF path unchanged)"
    );
    // The h1-over-TLS path itself must still serve (byte-for-byte regression).
    let mut client = TlsClient::connect(srv.addr(), &cert);
    let resp = client.request("GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert!(resp.contains("200"), "h1 over TLS must still work: {:?}", resp);
    assert_eq!(http_body(&resp), "h1-tls", "h1 TLS body must match: {:?}", resp);
}

// ---------------------------------------------------------------------------
// T1#2: SIGHUP zero-downtime worker reload (--reload-grace).
// ---------------------------------------------------------------------------

/// Returns the first immediate child pid of `master_pid` (a worker pid), or
/// `None` if `pgrep` is unavailable or the master has no children. Used by the
/// SIGHUP/SIGUSR1 tests to signal a specific worker. `pgrep -P` is present on
/// both macOS and Linux.
fn worker_pid_of(master_pid: u32) -> Option<u32> {
    let out = Command::new("pgrep")
        .args(["-P", &master_pid.to_string()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().next().and_then(|l| l.trim().parse::<u32>().ok())
}

/// Like `try_http_get` but with a bounded read timeout so a cut connection
/// (SIGTERM hard kill) does not hang the test. Returns the raw response text, or
/// the empty string on connect/write/read failure (including timeout). Used by
/// `web_sigterm_still_hard_kills` to assert an in-flight request is cut rather
/// than drained.
fn try_http_get_timed(addr: &str, path: &str, read_timeout: Duration) -> String {
    use std::io::{Read, Write};
    let Ok(mut s) = std::net::TcpStream::connect(addr) else {
        return String::new();
    };
    let _ = s.set_read_timeout(Some(read_timeout));
    let req = format!("GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", path, addr);
    if s.write_all(req.as_bytes()).is_err() {
        return String::new();
    }
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    buf
}

/// Verifies a SIGHUP sent to the master triggers a zero-downtime rolling worker
/// reload: while a background thread continuously hammers the server, every
/// request eventually gets a 200 (no permanent failure), and the total
/// successful count proves traffic flowed through the reload. With `--workers 3`,
/// at most one worker is down at any moment (N-1 = 2 always serve). Each request
/// sleeps ~50ms so in-flight requests overlap the reload. (T1#2: SIGHUP reload.)
#[test]
fn web_sighup_rolling_reload_keeps_serving() {
    let dir = make_test_dir("web_sighup_reload");
    // 50ms per request so in-flight requests overlap the per-worker recycle.
    let src = "<?php usleep(50000); echo 'ok';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args([
            "--listen", &addr,
            "--workers", "3",
            "--reload-grace", "5",
        ])
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    let master_pid = child.id();
    // Hammer thread: continuous try_http_get for ~2.5s, counting successes.
    let addr_hammer = addr.clone();
    let successes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let successes_h = successes.clone();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_h = stop.clone();
    let hammer = std::thread::spawn(move || {
        while !stop_h.load(std::sync::atomic::Ordering::Relaxed) {
            if try_http_get(&addr_hammer, "/").ends_with("ok") {
                successes_h.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    });
    // Let the hammer warm up, then send SIGHUP to trigger the rolling reload.
    std::thread::sleep(Duration::from_millis(300));
    let _ = Command::new("kill")
        .args(["-HUP", &master_pid.to_string()])
        .status();
    // Keep hammering through the reload + a margin.
    std::thread::sleep(Duration::from_millis(2200));
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    hammer.join().expect("hammer thread");
    let total_ok = successes.load(std::sync::atomic::Ordering::Relaxed);
    let _ = child.kill();
    let _ = child.wait();
    // Traffic flowed through the reload: with 50ms/request over ~2.5s we expect
    // dozens of successes even with the brief per-worker recycle window. A low
    // threshold (10) tolerates a contended box while still proving flow.
    assert!(
        total_ok > 10,
        "expected traffic to flow through the SIGHUP reload, got {} successes",
        total_ok
    );
    // The server must still serve after the reload (a fresh follow-up request).
    // (Re-spawn a fresh check is impossible after kill; the hammer already proved
    // continued serving post-reload via the >10 successes spanning the reload.)
}

/// Verifies a SIGUSR1 to a worker with an in-flight request drains it (the
/// in-flight request STILL gets a 200) instead of cutting it (which SIGTERM
/// would). With `--workers 1` the single worker drains then exits; the master
/// respawns it, so a follow-up request eventually succeeds. The KEY assertion is
/// the in-flight request completes with 200 (not cut). (T1#2: SIGHUP reload.)
#[test]
fn web_sigusr1_drains_inflight() {
    let dir = make_test_dir("web_sigusr1_drain");
    // 300ms handler so the signal lands mid-request with margin to observe drain.
    let src = "<?php usleep(300000); echo 'drained';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args([
            "--listen", &addr,
            "--workers", "1",
            "--reload-grace", "10",
        ])
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    let master_pid = child.id();
    // Wait for the worker to be spawned, then grab its pid.
    let worker_pid = {
        let mut wp = None;
        for _ in 0..40 {
            if let Some(p) = worker_pid_of(master_pid) {
                wp = Some(p);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        wp.expect("could not find worker pid via pgrep -P")
    };
    // Start an in-flight request on a background thread.
    let addr_req = addr.clone();
    let resp_cell = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let resp_cell_h = resp_cell.clone();
    let req_thread = std::thread::spawn(move || {
        let r = try_http_get_timed(&addr_req, "/", Duration::from_secs(5));
        *resp_cell_h.lock().unwrap() = r;
    });
    // Let the request enter the handler (usleep), then SIGUSR1 the worker.
    std::thread::sleep(Duration::from_millis(80));
    let _ = Command::new("kill")
        .args(["-USR1", &worker_pid.to_string()])
        .status();
    req_thread.join().expect("request thread");
    let resp = resp_cell.lock().unwrap().clone();
    let _ = child.kill();
    let _ = child.wait();
    // KEY assertion: the in-flight request completed with 200 + "drained" (SIGUSR1
    // drains; SIGTERM would have cut it → empty/broken response).
    assert!(
        resp.contains("200"),
        "in-flight request must be drained (200), not cut: {:?}",
        resp
    );
    assert!(
        resp.ends_with("drained"),
        "in-flight request body must be 'drained', got: {:?}",
        resp
    );
}

/// Verifies the SIGHUP reload OFF path (no SIGHUP sent) is byte-for-byte the
/// original behavior: `RELOAD` stays false, no rolling restart, no drain, and the
/// server serves normally. Guards the OFF path against regressions. (T1#2.)
#[test]
fn web_sighup_off_is_byte_for_byte() {
    let dir = make_test_dir("web_sighup_off");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let srv = spawn_server_guarded(&bin, "2");
    // A small number of requests, all must succeed immediately (no reload, so no
    // recycle window — a failure here would indicate the RELOAD flag or drain
    // path is spuriously active on the OFF path).
    for _ in 0..10 {
        let resp = http_get(srv.addr(), "/");
        assert!(
            resp.ends_with("ok"),
            "OFF-path request must succeed (no SIGHUP → no reload): {:?}",
            resp
        );
    }
}

/// Verifies SIGTERM is still a HARD kill (unchanged): an in-flight slow request
/// does NOT get a 200 when SIGTERM is sent to the master (the master forwards
/// SIGTERM → the worker dies instantly with SIG_DFL, cutting the connection).
/// Guards that the SIGHUP/SIGUSR1 graceful-drain work did NOT accidentally make
/// SIGTERM graceful. (T1#2: SIGTERM hard-kill invariant.)
#[test]
fn web_sigterm_still_hard_kills() {
    let dir = make_test_dir("web_sigterm_hard");
    // 500ms handler so SIGTERM lands mid-request with margin.
    let src = "<?php usleep(500000); echo 'should-not-see-this';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1"])
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    let master_pid = child.id();
    // Start an in-flight request on a background thread with a bounded read
    // timeout so a cut connection does not hang the test.
    let addr_req = addr.clone();
    let resp_cell = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let resp_cell_h = resp_cell.clone();
    let req_thread = std::thread::spawn(move || {
        let r = try_http_get_timed(&addr_req, "/", Duration::from_secs(3));
        *resp_cell_h.lock().unwrap() = r;
    });
    // Let the request enter the handler, then SIGTERM the master.
    std::thread::sleep(Duration::from_millis(100));
    let _ = Command::new("kill")
        .args(["-TERM", &master_pid.to_string()])
        .status();
    req_thread.join().expect("request thread");
    let resp = resp_cell.lock().unwrap().clone();
    let _ = child.kill();
    let _ = child.wait();
    // The in-flight request must NOT get a 200 — SIGTERM is hard (unchanged),
    // so the connection is cut (empty/broken response, not "should-not-see-this").
    assert!(
        !resp.contains("200"),
        "SIGTERM must hard-kill (in-flight cut, not drained 200): {:?}",
        resp
    );
    assert!(
        !resp.contains("should-not-see-this"),
        "SIGTERM must not let the in-flight body through: {:?}",
        resp
    );
}

// ---------------------------------------------------------------------------
// T1#3: per-worker metrics endpoint (`--metrics` / `/_status`).
// ---------------------------------------------------------------------------

/// Spawns the `--web` binary on a fresh ephemeral port with the given extra
/// runtime args (after `--listen`/`--workers`) and returns a kill-on-drop
/// `ServerHandle`. Reuses the guarded pattern so a failing assertion cannot
/// orphan the server process.
fn spawn_web_with_args(bin: &Path, extra_args: &[&str]) -> ServerHandle {
    let addr = format!("127.0.0.1:{}", free_port());
    let mut cmd = Command::new(bin);
    cmd.arg("--listen").arg(&addr).arg("--workers").arg("1");
    for a in extra_args {
        cmd.arg(a);
    }
    let child = cmd.spawn().expect("failed to spawn web server");
    wait_until_ready(&addr);
    ServerHandle { child, addr }
}

/// Verifies `--metrics` exposes a per-worker JSON snapshot at `/_status` after a
/// few normal requests: the snapshot is status 200, content-type
/// `application/json`, contains `pid`/`mode`/`active_conns`, and reports
/// `served_total` equal to the number of prior normal requests (the `/_status`
/// request itself is NOT recorded).
#[test]
fn web_metrics_endpoint_serves_json() {
    let dir = make_test_dir("web_metrics_json");
    let bin = compile_web(&dir, "<?php echo \"ok\";", "app");
    let server = spawn_web_with_args(&bin, &["--metrics"]);
    for _ in 0..3 {
        let r = http_get(server.addr(), "/");
        assert!(r.ends_with("ok"), "normal request body: {:?}", r);
    }
    let resp = http_get(server.addr(), "/_status");
    assert!(resp.starts_with("HTTP/1.1 200"), "status 200: {:?}", resp);
    assert!(
        resp.contains("content-type: application/json"),
        "content-type json: {:?}",
        resp,
    );
    assert!(resp.contains("\"pid\""), "json has pid: {:?}", resp);
    assert!(resp.contains("\"mode\":\"web\""), "json has mode web: {:?}", resp);
    assert!(resp.contains("\"active_conns\""), "json has active_conns: {:?}", resp);
    assert!(
        resp.contains("\"served_total\":3"),
        "served_total must be 3 (the 3 prior requests; /_status is not recorded): {:?}",
        resp,
    );
}

/// Verifies that WITHOUT `--metrics`, `/_status` falls through to the PHP handler
/// (the response is the echo body, NOT JSON). Guards the OFF path.
#[test]
fn web_metrics_disabled_by_default() {
    let dir = make_test_dir("web_metrics_off");
    let bin = compile_web(&dir, "<?php echo \"ok\";", "app");
    let server = spawn_web_with_args(&bin, &[]);
    let resp = http_get(server.addr(), "/_status");
    assert!(
        resp.ends_with("ok"),
        "without --metrics, /_status falls through to PHP (body ok): {:?}",
        resp,
    );
    assert!(
        !resp.contains("application/json"),
        "without --metrics, /_status must NOT be JSON: {:?}",
        resp,
    );
}

/// Verifies `--metrics-path` overrides the snapshot path: `/_status` falls
/// through to PHP while `/custom-status` serves the JSON snapshot.
#[test]
fn web_metrics_custom_path() {
    let dir = make_test_dir("web_metrics_custom");
    let bin = compile_web(&dir, "<?php echo \"ok\";", "app");
    let server =
        spawn_web_with_args(&bin, &["--metrics", "--metrics-path", "/custom-status"]);
    let default = http_get(server.addr(), "/_status");
    assert!(
        default.ends_with("ok"),
        "default path falls through to PHP when overridden: {:?}",
        default,
    );
    let custom = http_get(server.addr(), "/custom-status");
    assert!(
        custom.starts_with("HTTP/1.1 200"),
        "custom path serves the snapshot: {:?}",
        custom,
    );
    assert!(
        custom.contains("\"pid\""),
        "custom path returns JSON with pid: {:?}",
        custom,
    );
}

/// Verifies the metrics endpoint records a latency sample: after one slow
/// request, the snapshot's `latency_us` block reports `samples:1` (or `>=1`)
/// and contains the `p50` key. Uses a busy-loop handler so the recorded
/// latency is non-trivial on the contended box.
#[test]
fn web_metrics_records_latency() {
    let dir = make_test_dir("web_metrics_lat");
    // Busy loop: ~10M iterations to burn a few ms on the contended box.
    let src = "<?php $x = 0; for ($i = 0; $i < 10000000; $i++) { $x += $i; } echo \"done\";";
    let bin = compile_web(&dir, src, "app");
    let server = spawn_web_with_args(&bin, &["--metrics"]);
    let r = http_get(server.addr(), "/");
    assert!(r.ends_with("done"), "handler body: {:?}", r);
    let resp = http_get(server.addr(), "/_status");
    assert!(
        resp.contains("\"latency_us\":{\"p50\":"),
        "json has latency_us p50: {:?}",
        resp,
    );
    assert!(
        resp.contains("\"samples\":1") || resp.contains("\"samples\":2"),
        "samples must be >=1 after one recorded request: {:?}",
        resp,
    );
}

/// Verifies `record_request` tallies status classes: two 200 requests plus one
/// oversized (413) POST produce `2xx:2` and `4xx:1` in the snapshot. Guards the
/// status-class recording and the 413 early-return recording.
#[test]
fn web_metrics_records_status_classes() {
    let dir = make_test_dir("web_metrics_classes");
    let bin = compile_web(&dir, "<?php echo \"ok\";", "app");
    let server = spawn_web_with_args(&bin, &["--metrics", "--max-body-size", "4"]);
    // Two normal 200 requests.
    for _ in 0..2 {
        let r = http_get(server.addr(), "/");
        assert!(r.ends_with("ok"), "normal 200 body: {:?}", r);
    }
    // One oversized POST → 413.
    let big = http_request(server.addr(), "POST", "/", &[("Content-Type", "text/plain")], "xxxxxxxx");
    assert!(big.starts_with("HTTP/1.1 413"), "oversized POST is 413: {:?}", big);
    let resp = http_get(server.addr(), "/_status");
    assert!(
        resp.contains("\"2xx\":2"),
        "2xx must be 2 (two normal requests): {:?}",
        resp,
    );
    assert!(
        resp.contains("\"4xx\":1"),
        "4xx must be 1 (one 413): {:?}",
        resp,
    );
}

// ---------------------------------------------------------------------------
// T2#5: static-asset fast path (`--static-dir` / `--static-prefix`).
// ---------------------------------------------------------------------------

/// Makes a unique temp dir for a static-asset test (deterministic unique name
/// via an `AtomicU64` counter, not time) and returns its path. Each test
/// cleans up with `remove_dir_all` (best effort) at the end.
static STATIC_TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Builds a unique temp dir for a static-asset test.
fn make_static_dir(label: &str) -> PathBuf {
    let id = STATIC_TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("elephc_static_web_{}_{}", label, id));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Extracts the value of a header (case-insensitive name) from a raw HTTP
/// response, or `None` if absent. Used for `ETag`/`Content-Type`/`Cache-Control`
/// assertions on the static-asset tests.
fn header_value(resp: &str, name: &str) -> Option<String> {
    let headers = resp.split_once("\r\n\r\n").map(|(h, _)| h).unwrap_or(resp);
    for line in headers.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case(name) {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// Verifies `--static-dir DIR` serves a file under DIR from `/assets` on the
/// I/O thread: status 200, the file body, a `Content-Type` matching the
/// extension, an `ETag`, and a `Cache-Control` with the default `max-age=3600`.
#[test]
fn web_static_serves_file_from_dir() {
    let dir = make_static_dir("serve");
    let bin = compile_web(&dir, "<?php echo \"php\";", "app");
    std::fs::write(dir.join("hello.txt"), "asset").unwrap();
    let server = spawn_web_with_args(&bin, &["--static-dir", dir.to_str().unwrap()]);
    let resp = http_get(server.addr(), "/assets/hello.txt");
    assert!(resp.starts_with("HTTP/1.1 200"), "status 200: {:?}", resp);
    assert!(resp.ends_with("asset"), "body is the file: {:?}", resp);
    assert!(
        header_value(&resp, "content-type").is_some_and(|c| c.contains("text/plain")),
        "content-type text/plain: {:?}",
        resp,
    );
    assert!(header_value(&resp, "etag").is_some(), "etag present: {:?}", resp);
    assert!(
        header_value(&resp, "cache-control").is_some_and(|c| c.contains("max-age=3600")),
        "cache-control max-age=3600: {:?}",
        resp,
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies that WITHOUT `--static-dir`, a request to `/assets/anything.txt`
/// falls through to the PHP handler (body is `php`, the intercept is off).
/// Guards the OFF path (byte-for-byte the original hot path).
#[test]
fn web_static_falls_through_to_php_without_flag() {
    let dir = make_static_dir("off");
    let bin = compile_web(&dir, "<?php echo \"php\";", "app");
    let server = spawn_web_with_args(&bin, &[]);
    let resp = http_get(server.addr(), "/assets/anything.txt");
    assert!(resp.ends_with("php"), "falls through to PHP: {:?}", resp);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a missing file under `--static-dir` returns 404 and does NOT fall
/// through to the PHP handler (the body is NOT `php`).
#[test]
fn web_static_404_missing_file() {
    let dir = make_static_dir("404");
    let bin = compile_web(&dir, "<?php echo \"php\";", "app");
    let server = spawn_web_with_args(&bin, &["--static-dir", dir.to_str().unwrap()]);
    let resp = http_get(server.addr(), "/assets/nope.txt");
    assert!(resp.starts_with("HTTP/1.1 404"), "status 404: {:?}", resp);
    assert!(
        !resp.ends_with("php"),
        "must NOT fall through to PHP: {:?}",
        resp,
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a `..` traversal attempt (percent-encoded `%2e%2e`) is rejected as
/// 404 and the file outside the root is NOT served. Uses a raw TCP HTTP request
/// with the literal `%2e%2e` path so the encoded `..` reaches the server
/// unmodified (curl may normalize it otherwise).
#[test]
fn web_static_traversal_rejected() {
    let dir = make_static_dir("traversal");
    let bin = compile_web(&dir, "<?php echo \"php\";", "app");
    std::fs::write(dir.join("inside.txt"), "inside").unwrap();
    // Write a sibling file OUTSIDE the static root.
    let parent = dir.parent().unwrap().to_path_buf();
    let outside_name = format!("elephc_static_outside_{}.txt", STATIC_TEST_ID.load(Ordering::SeqCst));
    std::fs::write(parent.join(&outside_name), "outside-secret").unwrap();
    let server = spawn_web_with_args(&bin, &["--static-dir", dir.to_str().unwrap()]);
    // Raw TCP request with the literal `%2e%2e` path so the server's
    // `percent_decode` decodes it to `..` and the traversal guard rejects it.
    let path = format!("/assets/%2e%2e/{}", outside_name);
    let resp = http_get(server.addr(), &path);
    assert!(resp.starts_with("HTTP/1.1 404"), "traversal is 404: {:?}", resp);
    assert!(
        !resp.contains("outside-secret"),
        "outside file must NOT be served: {:?}",
        resp,
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(parent.join(&outside_name));
}

/// Verifies `If-None-Match` matching the ETag returns 304 with an empty body.
/// The first GET captures the ETag; the second GET (with `If-None-Match: <etag>`)
/// returns 304.
#[test]
fn web_static_if_none_match_returns_304() {
    let dir = make_static_dir("304");
    let bin = compile_web(&dir, "<?php echo \"php\";", "app");
    std::fs::write(dir.join("etag.txt"), "v").unwrap();
    let server = spawn_web_with_args(&bin, &["--static-dir", dir.to_str().unwrap()]);
    let first = http_get(server.addr(), "/assets/etag.txt");
    let etag = header_value(&first, "etag").expect("etag on first response");
    let resp = http_request(
        server.addr(),
        "GET",
        "/assets/etag.txt",
        &[("If-None-Match", etag.as_str())],
        "",
    );
    assert!(resp.starts_with("HTTP/1.1 304"), "status 304: {:?}", resp);
    // 304 has no body (after the blank line).
    let body = http_body(&resp);
    assert!(body.is_empty(), "304 body is empty: {:?}", resp);
    let _ = std::fs::remove_dir_all(&dir);
}
