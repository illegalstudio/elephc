//! Purpose:
//! Reproduces ownership and lifecycle defects found by the independent application review.
//!
//! Called from:
//! - The application module in the web integration test binary.
//!
//! Key details:
//! - Exercises callbacks outside handle(), implicit stream close and in-flight termination.
//! - Reuses the parent's fixture and process guards on every executable test host.

use super::*;

/// Bootstrap, shutdown and both request-end release paths always expose valid superglobal arrays.
#[test]
fn web_application_review_superglobals_are_valid_in_every_phase() {
    let fixture = Fixture(make_test_dir("web_application_review_globals"));
    let binary = application(&fixture, r#"<?php
class EndValue {
    public static int $destroyed = 0;
    public ?EndValue $cycle = null;
    public function __destruct() {
        $size = count($_GET) + count($_SERVER) + count($_ENV);
        $_SERVER['during_cleanup'] = 'yes';
        $_ENV['during_cleanup'] = 'yes';
        self::$destroyed++;
    }
}
class ShutdownValue {
    public function __destruct() {
        $size = count($_GET);
        $_SERVER['shutdown_destructor'] = 'yes';
        file_put_contents('__LOG__', ':released', FILE_APPEND);
    }
}
class Application implements Elephc\Web\WorkerApplication {
    public function __construct(public string $config) {}
    public function handle(Elephc\Web\Request $request): void {
        $cycle = new EndValue();
        $cycle->cycle = $cycle;
        $_SERVER['owner'] = new EndValue();
        echo $this->config, ':', EndValue::$destroyed, ':',
            isset($_ENV['APP_BOOT']) ? 'stale' : 'fresh', ':',
            isset($_SERVER['during_cleanup']) ? 'stale' : 'fresh';
    }
    public function shutdown(): void {
        $size = count($_SERVER) + count($_GET) + count($_ENV);
        $_SERVER['shutdown'] = 'yes';
        $_SESSION['shutdown_owner'] = new ShutdownValue();
        file_put_contents('__LOG__', 'stop:' . EndValue::$destroyed);
    }
}
if (count($_ENV) === 0 || count($_SERVER) === 0) { throw new RuntimeException('missing process environment'); }
$_SERVER['APP_BOOT'] = 'boot';
$_ENV['APP_BOOT'] = 'boot';
Elephc\Web\Worker::register(new Application($_ENV['APP_BOOT']));
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&binary, &addr, "1");
    assert_eq!(body(&addr, "/"), "boot:0:fresh:fresh");
    assert_eq!(body(&addr, "/"), "boot:2:fresh:fresh");
    server.kill().unwrap();
    server.wait().unwrap();
    assert_eq!(fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap(), "stop:4:released");
}

/// An implicitly destroyed filtered descriptor cannot alter the next request's plain stream.
#[test]
fn web_application_review_implicit_close_retires_fd_filters() {
    let fixture = Fixture(make_test_dir("web_application_review_filters"));
    let binary = application(&fixture, r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    private int $count = 0;
    public function handle(Elephc\Web\Request $request): void {
        $this->count++;
        $stream = fopen('__LOG__', 'a');
        if ($this->count === 1) {
            stream_filter_append($stream, 'string.toupper', STREAM_FILTER_WRITE);
            fwrite($stream, 'first|');
        } else {
            fwrite($stream, 'second');
            fclose($stream);
        }
        echo 'ok';
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut connection = TcpStream::connect(&addr).unwrap();
    connection.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    for _ in 0..2 {
        connection.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        assert_eq!(raw_response_body(&read_complete_http_response(&mut connection)).trim(), "ok");
    }
    assert_eq!(fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap(), "FIRST|second");
}

/// Temporary wrapper instances retire their slots while an application-owned stream stays open.
#[test]
fn web_application_review_implicit_close_releases_wrapper_slots() {
    let fixture = Fixture(make_test_dir("web_application_review_wrappers"));
    let binary = application(&fixture, r#"<?php
class ReviewWrapper {
    public static int $opened = 0;
    public static int $closed = 0;
    public function stream_open($path, $mode, $options, &$opened): bool { self::$opened++; return true; }
    public function stream_close(): void {
        self::$closed++;
        if (self::$closed % 17 === 0) { throw new RuntimeException('throwing stream close'); }
    }
    public function stream_read(int $length): string { return 'data'; }
    public function stream_eof(): bool { return true; }
}
class Application implements Elephc\Web\WorkerApplication {
    private mixed $saved = null;
    public function handle(Elephc\Web\Request $request): void {
        if ($this->saved === null) { $this->saved = fopen('review://saved', 'r'); }
        $temporary = fopen('review://temporary', 'r');
        if ($temporary === false) { throw new RuntimeException('exhausted wrapper slots'); }
        echo ReviewWrapper::$opened, ':', ReviewWrapper::$closed, ':', fread($this->saved, 4);
    }
    public function shutdown(): void {
        fclose($this->saved);
        file_put_contents('__LOG__', (string) ReviewWrapper::$closed);
    }
}
stream_wrapper_register('review', 'ReviewWrapper');
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&binary, &addr, "1");
    for count in 1..=300 {
        if count % 17 == 0 {
            assert!(http_get(&addr, "/").starts_with("HTTP/1.1 500"));
        } else {
            assert_eq!(body(&addr, "/"), format!("{}:{}:data", count + 1, count - 1));
        }
    }
    server.kill().unwrap();
    server.wait().unwrap();
    assert_eq!(fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap(), "301");
}

/// SIGTERM drains a request blocked in a native socket read without changing its successful body.
#[test]
fn web_application_review_shutdown_preserves_blocking_read() {
    for timeout in ["", "stream_set_timeout($stream, 2);"] {
    let fixture = Fixture(make_test_dir("web_application_review_signal"));
    let slow = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = slow.local_addr().unwrap().port();
    let source = r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    public function handle(Elephc\Web\Request $request): void {
        $stream = fsockopen('127.0.0.1', __PORT__);
        __TIMEOUT__
        echo fread($stream, 32);
        fclose($stream);
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#.replace("__PORT__", &port.to_string()).replace("__TIMEOUT__", timeout);
    let binary = application(&fixture, &source);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&binary, &addr, "1");
    let (accepted, receiver) = std::sync::mpsc::channel();
    let peer = std::thread::spawn(move || {
        let (mut connection, _) = slow.accept().unwrap();
        accepted.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        connection.write_all(b"hello-from-slow").unwrap();
    });
    let request_addr = addr.clone();
    let request = std::thread::spawn(move || http_get(&request_addr, "/"));
    receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    server.kill().unwrap();
    peer.join().unwrap();
    let response = request.join().unwrap();
    server.wait().unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_eq!(raw_response_body(&response).trim(), "hello-from-slow", "{response}");
    }
}

/// Context updates separate nested application-owned arrays and retire each replaced option owner.
#[test]
fn web_application_review_context_option_preserves_application_alias() {
    let fixture = Fixture(make_test_dir("web_application_review_context_cow"));
    let binary = application(&fixture, r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    private array $options = ['http' => ['header' => 'original']];
    public function handle(Elephc\Web\Request $request): void {
        $context = stream_context_create($this->options);
        stream_context_set_option($context, 'http', 'header', 'updated');
        $after = stream_context_get_options($context);
        echo $this->options['http']['header'], ':', $after['http']['header'];
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    for _ in 0..20 { assert_eq!(body(&addr, "/"), "original:updated"); }
}

/// Reserved phase methods reject request-time calls without detaching the active application.
#[test]
fn web_application_review_rejects_out_of_phase_worker_methods() {
    let fixture = Fixture(make_test_dir("web_application_review_phases"));
    let binary = application(&fixture, r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    private int $count = 0;
    public function handle(Elephc\Web\Request $request): void {
        $this->count++;
        try { Elephc\Web\Worker::stop(); echo 'bad-stop'; }
        catch (LogicException) { echo 'stop-blocked:'; }
        try { Elephc\Web\Worker::completeBootstrap(); echo 'bad-commit'; }
        catch (LogicException) { echo 'commit-blocked:'; }
        elephc_web_application_commit_bootstrap();
        try { Elephc\Web\Worker::failBootstrap(); echo 'bad-fail'; }
        catch (LogicException) { echo 'fail-blocked:'; }
        echo $this->count;
    }
    public function shutdown(): void { file_put_contents('__LOG__', 'shutdown'); }
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&binary, &addr, "1");
    for count in 1..=3 {
        assert_eq!(body(&addr, "/"), format!("stop-blocked:commit-blocked:fail-blocked:{count}"));
    }
    server.kill().unwrap();
    server.wait().unwrap();
    assert_eq!(fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap(), "shutdown");
}

/// Suspended coroutine stacks and their transfer values survive cleanup between HTTP callbacks.
#[test]
fn web_application_review_fibers_keep_suspended_contexts() {
    let fixture = Fixture(make_test_dir("web_application_review_fibers"));
    let binary = application(&fixture, r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    private ?Fiber $ints = null;
    private ?Fiber $heap = null;
    private ?Fiber $cat = null;
    private int $count = 0;
    public function handle(Elephc\Web\Request $request): void {
        $this->count++;
        if ($this->count === 1) {
            $this->ints = new Fiber(function (): void { for ($i = 1; $i < 4; $i++) { Fiber::suspend($i * 10); } });
            $this->heap = new Fiber(function (): void { for ($i = 1; $i < 4; $i++) { Fiber::suspend(str_repeat('h', $i)); } });
            $this->cat = new Fiber(function (): void { for ($i = 1; $i < 4; $i++) { Fiber::suspend('c' . $i); } });
            echo $this->ints->start(), ':', $this->heap->start(), ':', $this->cat->start();
        } else {
            echo $this->ints->resume(), ':', $this->heap->resume(), ':', $this->cat->resume();
        }
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    for count in 1..=3 {
        assert_eq!(body(&addr, "/"), format!("{}:{}:c{count}", count * 10, "h".repeat(count)));
    }
}

/// Native child processes inherit normal termination behavior in every application phase.
#[test]
fn web_application_review_children_keep_original_signal_mask() {
    let fixture = Fixture(make_test_dir("web_application_review_child_signals"));
    let binary = application(&fixture, r#"<?php
function checkChildren(): void {
    $command = 'echo ran >> "__LOG__.ran"; kill -TERM $$; echo inherited >> "__LOG__.bad"';
    shell_exec($command);
    exec($command);
    system($command);
    passthru($command);
    $pipe = popen($command, 'r');
    stream_get_contents($pipe);
    pclose($pipe);
}
class Application implements Elephc\Web\WorkerApplication {
    public function handle(Elephc\Web\Request $request): void {
        checkChildren();
        echo 'ok';
    }
    public function shutdown(): void {
        checkChildren();
        file_put_contents('__LOG__', 'stopped');
    }
}
checkChildren();
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&binary, &addr, "1");
    assert_eq!(body(&addr, "/"), "ok");
    server.kill().unwrap();
    server.wait().unwrap();
    assert_eq!(fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap(), "stopped");
    assert_eq!(fs::read_to_string(fixture.0.join("lifecycle.log.ran")).unwrap().lines().count(), 15);
    assert!(!fixture.0.join("lifecycle.log.bad").exists(), "a shell child survived its SIGTERM");
}

/// Fork and exec share the callback context across their independently linked bridge archives.
#[test]
fn web_application_review_pcntl_children_keep_original_signal_mask() {
    let fixture = Fixture(make_test_dir("web_application_review_pcntl_signals"));
    let binary = application(&fixture, r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    public function handle(Elephc\Web\Request $request): void {
        $pid = pcntl_fork();
        if ($pid === 0) {
            pcntl_exec('/bin/sh', ['-c', 'kill -TERM $$; exit 17']);
            exit(99);
        }
        $status = 0;
        pcntl_waitpid($pid, $status);
        echo (pcntl_wifsignaled($status) && pcntl_wtermsig($status) === SIGTERM) ? 'terminated' : 'bad';
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    assert_eq!(body(&addr, "/"), "terminated");
    assert_eq!(body(&addr, "/"), "terminated");
}

/// A fork child drops worker dispositions while preserving PHP-installed ignored signals.
#[test]
fn web_application_review_fork_children_drop_worker_signal_handler() {
    let fixture = Fixture(make_test_dir("web_application_review_fork_handlers"));
    let binary = application(&fixture, r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    public function handle(Elephc\Web\Request $request): void {
        $pid = pcntl_fork();
        if ($pid === 0) {
            exec('kill -TERM ' . getmypid());
            usleep(200000);
            exit(5);
        }
        $status = 0;
        pcntl_waitpid($pid, $status);
        echo (pcntl_wifsignaled($status) && pcntl_wtermsig($status) === SIGTERM) ? 'terminated:' : 'bad:';
        pcntl_signal(SIGINT, SIG_IGN);
        $pid = pcntl_fork();
        if ($pid === 0) {
            exec('kill -INT ' . getmypid());
            usleep(200000);
            exit(5);
        }
        pcntl_waitpid($pid, $status);
        echo (pcntl_wifexited($status) && pcntl_wexitstatus($status) === 5) ? 'ignored' : 'bad';
    }
    public function shutdown(): void { file_put_contents('__LOG__', 'stopped'); }
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&binary, &addr, "1");
    for _ in 0..2 { assert_eq!(body(&addr, "/"), "terminated:ignored"); }
    server.kill().unwrap();
    server.wait().unwrap();
    assert_eq!(fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap(), "stopped");
}

/// Bootstrap and handler signal-mask changes survive the callback termination guard.
#[test]
fn web_application_review_callback_preserves_application_signal_mask() {
    let fixture = Fixture(make_test_dir("web_application_review_callback_mask"));
    let binary = application(&fixture, r#"<?php
class Application implements Elephc\Web\WorkerApplication {
    private int $count = 0;
    public function handle(Elephc\Web\Request $request): void {
        $this->count++;
        $old = [];
        pcntl_sigprocmask(SIG_UNBLOCK, [SIGUSR2], $old);
        echo getmypid(), ':', in_array(SIGUSR1, $old, true) ? 'blocked' : 'bad';
        pcntl_sigprocmask(SIG_UNBLOCK, [SIGUSR1]);
        if ($this->count % 2 === 0) { pcntl_sigprocmask(SIG_BLOCK, [SIGUSR1]); }
    }
    public function shutdown(): void {
        $old = [];
        pcntl_sigprocmask(SIG_UNBLOCK, [SIGUSR2], $old);
        file_put_contents('__LOG__', in_array(SIGUSR1, $old, true) ? 'blocked' : 'unblocked');
    }
}
pcntl_sigprocmask(SIG_BLOCK, [SIGUSR1]);
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&binary, &addr, "1");
    let first = body(&addr, "/");
    let pid = first.strip_suffix(":blocked").expect("bootstrap mask must survive");
    assert_eq!(body(&addr, "/"), format!("{pid}:bad"));
    assert_eq!(body(&addr, "/"), format!("{pid}:blocked"));
    assert_eq!(body(&addr, "/"), format!("{pid}:bad"));
    server.kill().unwrap();
    server.wait().unwrap();
    assert_eq!(fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap(), "blocked");
}
