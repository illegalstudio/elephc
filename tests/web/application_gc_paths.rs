//! Purpose:
//! Bounds shared proof-prefix work on deep graphs and validates cache invalidation.
//!
//! Called from:
//! - Persistent application HTTP integration tests.
//!
//! Key details:
//! - Real request counters expose quadratic positive and negative path validation.
//! - A detached cyclic list must be reclaimed after a previously memoized live walk.

use super::*;

/// Value and reference captures prove retained service graphs without repeating their full traversal.
#[test]
fn web_application_gc_proves_services_retained_only_by_callable_captures() {
    for capture in ["$service", "&$service"] {
        let fixture = Fixture(make_test_dir("web_application_callable_service_proofs"));
        let source = r#"<?php
extern function __elephc_web_gc_visited(): int;
class CapturedChainNode {
    public static int $destroyed = 0;
    public ?CapturedChainNode $next = null;
    public function __destruct() { self::$destroyed++; }
}
class CapturedService {
    public CapturedChainNode $head;
    public int $hits = 0;
    public function __construct() {
        $this->head = new CapturedChainNode();
        $cursor = $this->head;
        for ($index = 0; $index < 20000; $index++) {
            $cursor->next = new CapturedChainNode();
            $cursor = $cursor->next;
        }
        $cursor->next = $this->head;
    }
    public function hit(): int { return ++$this->hits; }
}
class Application implements Elephc\Web\WorkerApplication {
    private Closure $handler;
    private WeakReference $weak;
    public function __construct() { $this->install(); }
    private function install(): void {
        $service = new CapturedService();
        $this->weak = WeakReference::create($service);
        $this->handler = static function() use (__CAPTURE__): int { $local = $service; return $local->hit(); };
    }
    public function handle(Elephc\Web\Request $request): void {
        $visited = __elephc_web_gc_visited();
        if ($request->path === '/drop') { $this->handler = static function(): int { return 0; }; }
        if ($request->path === '/replace') { $this->install(); }
        $handler = $this->handler;
        echo $visited, ':', $handler(), ':', CapturedChainNode::$destroyed, ':', $this->weak->get() === null ? 'dead' : 'alive';
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#.replace("__CAPTURE__", capture);
        let binary = compile_web_with_flags(&fixture.0, &source, "app", &["--web-lifecycle=application", "--heap-size=67108864"]);
        let addr = format!("127.0.0.1:{}", free_port());
        let _server = spawn_server(&binary, &addr, "1");
        let mut previous = 0;
        for request in 1..=30 {
            let response = body(&addr, "/");
            let fields: Vec<_> = response.split(':').collect();
            assert_eq!(&fields[1..], &[request.to_string(), "0".to_string(), "alive".to_string()], "{capture}: {response}");
            let visited = fields[0].parse::<usize>().unwrap();
            if request > 3 { assert!(visited >= previous && visited - previous < 1000, "captured service was rescanned: {capture}, {previous} -> {response}"); }
            previous = visited;
        }
        body(&addr, "/drop");
        let response = body(&addr, "/");
        let fields: Vec<_> = response.split(':').collect();
        assert_eq!(&fields[1..], &["0", "20001", "dead"], "stale callable proof retained a detached graph: {capture}, {response}");
        let response = body(&addr, "/replace");
        assert!(response.ends_with(":1:20001:alive"), "replacement capture was not valid: {capture}, {response}");
        let response = body(&addr, "/");
        assert!(response.ends_with(":2:20001:alive"), "replacement capture did not survive: {capture}, {response}");
    }
}

/// Cold queues, repeated deep reads, invalidated head edges and cyclic teardown stay linear.
#[test]
fn web_application_gc_memoizes_live_and_unproved_deep_paths() {
    let fixture = Fixture(make_test_dir("web_application_gc_deep_memo"));
    let binary = compile_web_with_flags(&fixture.0, r#"<?php
extern function __elephc_web_gc_visited(): int;
class PathNode {
    public static int $destroyed = 0;
    public ?PathNode $next = null;
    public function __construct(public int $value) {}
    public function __destruct() { self::$destroyed++; }
}
class Application implements Elephc\Web\WorkerApplication {
    private ?PathNode $head = null;
    private WeakReference $weak;
    public function __construct() {
        $tail = null;
        for ($i = 0; $i < 4000; $i++) {
            $node = new PathNode($i);
            if ($tail === null) { $tail = $node; }
            $node->next = $this->head;
            $this->head = $node;
        }
        $tail->next = $this->head;
        $this->weak = WeakReference::create($node);
    }
    public function handle(Elephc\Web\Request $request): void {
        $visited = __elephc_web_gc_visited();
        $limit = (int) substr($request->path, 1);
        $sum = 0;
        $node = $this->head;
        for ($i = 0; $i < $limit && $node !== null; $i++) {
            $sum += $node->value;
            $node = $node->next;
        }
        if ($request->path === '/push') {
            $node = new PathNode(-1);
            $node->next = $this->head;
            $this->head = $node;
        }
        if ($request->path === '/drop') { $this->head = null; }
        echo $visited, ':', $sum, ':', PathNode::$destroyed, ':', $this->weak->get() === null ? 'dead' : 'alive';
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#, "app", &["--web-lifecycle=application", "--heap-size=67108864"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut previous = 0;
    for (index, path) in ["/0", "/0", "/1000", "/0", "/2000", "/0", "/4000", "/0", "/4000", "/0",
        "/push", "/push", "/push", "/push", "/0", "/drop", "/0", "/0"].iter().enumerate() {
        let response = body(&addr, path);
        let parts: Vec<_> = response.split(':').collect();
        let visited = parts[0].parse::<usize>().unwrap();
        if index > 0 {
            assert!(visited >= previous && visited - previous < 64 * 4000,
                "deep cached prefixes repeated superlinear work: {previous} -> {response}, after request {index}");
        }
        if *path == "/1000" { assert_eq!(parts[1], "3499500", "{response}"); }
        if *path == "/2000" { assert_eq!(parts[1], "5999000", "{response}"); }
        if *path == "/4000" { assert_eq!(parts[1], "7998000", "{response}"); }
        if index < 15 { assert_eq!(&parts[2..], &["0", "alive"], "{response}"); }
        if index >= 16 { assert_eq!(&parts[2..], &["4004", "dead"], "memoization retained a detached cycle: {response}"); }
        previous = visited;
    }
}

/// Sparse scalar tables charge their capacity scan and cannot rebuild on every small write budget.
#[test]
fn web_application_gc_rebuild_cost_includes_scalar_hash_slots() {
    let fixture = Fixture(make_test_dir("web_application_gc_scalar_cost"));
    let binary = compile_web_with_flags(&fixture.0, r#"<?php
extern function __elephc_web_gc_visited(): int;
extern function __elephc_web_gc_rebuild_cost(): int;
class SlotCycle {
    public static int $destroyed = 0;
    public ?SlotCycle $self = null;
    public function __destruct() { self::$destroyed++; }
}
class Application implements Elephc\Web\WorkerApplication {
    private array $strings = [];
    private ?Elephc\Web\Request $last = null;
    private int $requests = 0;
    public function __construct() {
        $strings = ['k0' => 'value'];
        for ($i = 1; $i < 16384; $i++) { $strings['k' . $i] = 'value'; }
        for ($i = 1; $i < 16384; $i++) { unset($strings['k' . $i]); }
        $this->strings = $strings;
    }
    public function handle(Elephc\Web\Request $request): void {
        $visited = __elephc_web_gc_visited();
        $this->last = $request;
        $this->requests++;
        $cycle = new SlotCycle();
        $cycle->self = $cycle;
        echo $this->requests, ':', $visited, ':', __elephc_web_gc_rebuild_cost(), ':',
            $this->strings['k0'], ':', count($this->strings), ':', SlotCycle::$destroyed;
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#, "app", &["--web-lifecycle=application", "--heap-size=67108864"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut previous = 0;
    for request in 1..=300 {
        let response = body(&addr, "/save");
        let parts: Vec<_> = response.split(':').collect();
        assert_eq!(parts[0], request.to_string(), "{response}");
        assert_eq!(&parts[3..5], &["value", "1"], "{response}");
        assert_eq!(parts[5], (request - 1).to_string(), "temporary cycles escaped their boundary: {response}");
        let visited = parts[1].parse::<usize>().unwrap();
        let rebuild_cost = parts[2].parse::<usize>().unwrap();
        assert!(rebuild_cost >= 16384, "rebuild charge omitted sparse scalar slots: {response}");
        assert!(visited >= rebuild_cost, "cumulative work omitted a full cache construction: {response}");
        if request > 2 {
            assert!(visited >= previous && visited - previous < 1000,
                "a tiny earned budget rescanned a sparse scalar table: {previous} -> {response}");
        }
        previous = visited;
    }
}
