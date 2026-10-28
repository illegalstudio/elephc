//! Purpose:
//! Verifies coroutine cancellation and suspended owner lifetimes across HTTP boundaries.
//!
//! Called from:
//! - Persistent web application lifecycle integration tests.
//!
//! Key details:
//! - Early generator breaks and abandoned Fibers must release locals before the next request.
//! - An application-owned suspended Fiber retains its frame through unrelated request churn.

use super::*;

/// Repeated early breaks and suspended-frame cycles keep immediate cleanup and stable PHP memory.
#[test]
fn web_application_coroutine_cancellation_reclaims_request_frames_after_errors() {
    let fixture = Fixture(make_test_dir("web_application_coroutine_cancellation"));
    let binary = application(&fixture, r#"<?php
class CoroutineValue {
    public static int $destroyed = 0;
    public static int $closed = 0;
    public string $payload;
    public function __construct() { $this->payload = str_repeat('x', 4096); }
    public function __destruct() { self::$destroyed++; }
}
function requestRows(bool $fail) {
    $local = new CoroutineValue();
    try { yield 1; yield strlen($local->payload); }
    finally {
        CoroutineValue::$closed++;
        if ($fail) { throw new RuntimeException('request finally failed'); }
    }
}
class CoroutineCycle {
    public $iterator = null;
    public string $payload;
    public function __construct() {
        $this->payload = str_repeat('x', 4096);
        $iterator = $this->rows();
        $iterator->current();
        $this->iterator = $iterator;
    }
    public function rows() {
        $owners = [$this];
        try { yield 1; yield count($owners); }
        finally { CoroutineValue::$closed++; }
    }
    public function __destruct() { CoroutineValue::$destroyed++; }
}
class Application implements Elephc\Web\WorkerApplication {
    private int $requests = 0;
    private ?WeakReference $previous = null;
    public function handle(Elephc\Web\Request $request): void {
        $live = memory_get_usage();
        $destroyed = CoroutineValue::$destroyed;
        $closed = CoroutineValue::$closed;
        $dead = $this->previous === null || $this->previous->get() === null;
        $this->requests++;
        if ($request->path === '/fiber') {
            $fiber = new Fiber(static function(): void {
                $local = new CoroutineValue();
                try { Fiber::suspend(1); echo strlen($local->payload); }
                finally { CoroutineValue::$closed++; }
            });
            $fiber->start();
        } elseif ($request->path === '/cycle') {
            $owner = new CoroutineCycle();
            $this->previous = WeakReference::create($owner);
        } else {
            foreach (requestRows($request->path === '/fail') as $row) { break; }
        }
        echo $this->requests, ':', $destroyed, ':', $closed, ':', $live, ':', $dead ? 'dead' : 'alive';
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut baseline = 0;
    for request in 1..=512 {
        let response = body(&addr, ["/generator", "/fiber", "/cycle"][(request - 1) % 3]);
        let fields: Vec<_> = response.split(':').collect();
        assert_eq!(&fields[..3], &[request.to_string(), (request - 1).to_string(), (request - 1).to_string()], "{response}");
        assert_eq!(fields[4], "dead", "{response}");
        let live = fields[3].parse::<usize>().unwrap();
        if request == 32 { baseline = live; }
        if request > 32 { assert!(live <= baseline + 32_768, "suspended request frames accumulated: {baseline} -> {response}"); }
    }
    let failed = http_get(&addr, "/fail");
    assert!(failed.starts_with("HTTP/1.1 500"), "{failed}");
    let response = body(&addr, "/generator");
    assert!(response.starts_with("514:513:513:"), "{response}");
}

/// A rooted suspended frame stays valid across requests and closes only after explicit retirement.
#[test]
fn web_application_coroutine_cancellation_preserves_retained_frames_until_drop() {
    let fixture = Fixture(make_test_dir("web_application_retained_frame"));
    let binary = application(&fixture, r#"<?php
class RetainedCoroutineValue {
    public static int $destroyed = 0;
    public static int $closed = 0;
    public string $payload;
    public function __construct() { $this->payload = str_repeat('x', 4096); }
    public function __destruct() { self::$destroyed++; }
}
class Application implements Elephc\Web\WorkerApplication {
    private ?Fiber $saved = null;
    public function handle(Elephc\Web\Request $request): void {
        if ($request->path === '/open') {
            $fiber = new Fiber(static function(string $argument): void {
                $local = new RetainedCoroutineValue();
                try { Fiber::suspend(); echo strlen($local->payload), ':', strlen($argument), '|'; Fiber::suspend(); }
                finally { RetainedCoroutineValue::$closed++; }
            });
            $fiber->start(str_repeat('a', 4096));
            $this->saved = $fiber;
        }
        if ($request->path === '/resume') { $this->saved->resume(); }
        if ($request->path === '/drop') { $this->saved = null; }
        gc_collect_cycles();
        echo RetainedCoroutineValue::$destroyed, ':', RetainedCoroutineValue::$closed;
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    assert_eq!(body(&addr, "/open"), "0:0");
    for _ in 0..16 { assert_eq!(body(&addr, "/churn"), "0:0"); }
    assert_eq!(body(&addr, "/resume"), "4096:4096|0:0");
    assert_eq!(body(&addr, "/drop"), "1:1");
    assert_eq!(body(&addr, "/check"), "1:1");
}

/// Eval preserves internal cancellation across repeated request-local Fiber teardown.
#[test]
fn web_application_coroutine_eval_cancellation_skips_catches_and_releases_temporaries() {
    let fixture = Fixture(make_test_dir("web_application_coroutine_eval"));
    let binary = application(&fixture, r#"<?php
class EvalBoundaryCount {
    public static int $closed = 0;
    public static int $caught = 0;
    public static int $tail = 0;
}
function evalApplicationPause(): void {
    $temporary = str_repeat('x', 4096);
    Fiber::suspend(); echo strlen($temporary);
}
class Application implements Elephc\Web\WorkerApplication {
    private int $requests = 0;
    public function handle(Elephc\Web\Request $request): void {
        $live = memory_get_usage();
        $fiber = new Fiber(static function(): void {
            eval('try { evalApplicationPause(); } catch (Throwable $error) { EvalBoundaryCount::$caught++; } finally { EvalBoundaryCount::$closed++; }');
            EvalBoundaryCount::$tail++;
        });
        $fiber->start(); unset($fiber);
        $this->requests++;
        echo $this->requests, ':', EvalBoundaryCount::$closed, ':', EvalBoundaryCount::$caught, ':', EvalBoundaryCount::$tail, ':', $live;
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut baseline = 0;
    for request in 1..=300 {
        let response = body(&addr, "/");
        let fields: Vec<_> = response.split(':').collect();
        assert_eq!(&fields[..4], &[request.to_string(), request.to_string(), "0".into(), "0".into()], "{response}");
        let live = fields[4].parse::<usize>().unwrap();
        if request == 32 { baseline = live; }
        if request > 32 { assert!(live <= baseline + 32_768, "eval unwind temporaries accumulated: {baseline} -> {response}"); }
    }
}

/// Delegation and fresh start argument boxes do not accumulate across HTTP boundaries.
#[test]
fn web_application_coroutine_boundaries_reclaim_delegates_and_start_arguments() {
    let fixture = Fixture(make_test_dir("web_application_coroutine_boundaries"));
    let binary = application(&fixture, r#"<?php
class BoundaryCount { public static int $closed = 0; }
function boundaryRows() {
    try { yield 1; yield 2; } finally { BoundaryCount::$closed++; }
}
function boundaryDelegation() { yield from boundaryRows(); }
class Application implements Elephc\Web\WorkerApplication {
    private int $requests = 0;
    public function handle(Elephc\Web\Request $request): void {
        $live = memory_get_usage();
        $closed = BoundaryCount::$closed;
        $this->requests++;
        if ($request->path === '/fiber') {
            $fiber = new Fiber(static function(string $value): void {
                try { Fiber::suspend(); }
                finally {
                    if (strlen($value) !== 4096) { throw new Exception('invalid start argument'); }
                    BoundaryCount::$closed++;
                }
            });
            $fiber->start(str_repeat('x', 4096));
        } else {
            foreach (boundaryDelegation() as $row) { break; }
        }
        echo $this->requests, ':', $closed, ':', $live;
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut baseline = 0;
    for request in 1..=512 {
        let response = body(&addr, if request % 2 == 0 { "/fiber" } else { "/delegate" });
        let fields: Vec<_> = response.split(':').collect();
        assert_eq!(&fields[..2], &[request.to_string(), (request - 1).to_string()], "{response}");
        let live = fields[2].parse::<usize>().unwrap();
        if request == 32 { baseline = live; }
        if request > 32 { assert!(live <= baseline + 16_384, "coroutine temporary owners accumulated: {baseline} -> {response}"); }
    }
}
