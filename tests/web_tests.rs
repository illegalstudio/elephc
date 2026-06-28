//! Purpose:
//! End-to-end tests for `--web`: compile PHP into a prefork HTTP server binary,
//! launch it with `--listen`, drive it over raw TCP, and assert the response.
//!
//! Called from:
//! - `cargo test --test web_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an
//!   isolated temp dir with an isolated runtime cache, mirroring cdylib_tests.
//! - The HTTP client is a hand-written minimal HTTP/1.1 request over a
//!   std::net::TcpStream so the test pulls in no HTTP client dependency.
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

/// Verifies that a worker which dies mid-request is respawned, so the single-worker
/// server keeps serving subsequent requests.
#[test]
fn web_worker_respawns_after_crash() {
    let dir = make_test_dir("web_respawn");
    let src = "<?php if (($_SERVER['REQUEST_URI'] ?? '') === '/crash') { exit(1); } echo \"alive\";";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    assert!(http_request(&addr, "GET", "/", &[], "").ends_with("alive"));
    // Crash the only worker (the connection is dropped mid-handler).
    let _ = try_http_get(&addr, "/crash");
    // The master must respawn a worker; retry until / serves again.
    let mut served = false;
    for _ in 0..40 {
        if try_http_get(&addr, "/").ends_with("alive") {
            served = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(served, "worker was not respawned after a crash");
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
