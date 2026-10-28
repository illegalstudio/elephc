//! Purpose:
//! Bounds retained writes without postponing request garbage collection.
//!
//! Called from:
//! - Persistent application HTTP integration tests.
//!
//! Key details:
//! - A large unchanged graph must not be rebuilt for every newly saved request value.
//! - Cumulative diagnostics count every proof rebuild as well as candidate tracing.

use super::*;

/// New retained objects and Request snapshots do not each trigger a whole-application proof rebuild.
#[test]
fn web_application_gc_amortizes_repeated_retained_writes() {
    let fixture = Fixture(make_test_dir("web_application_gc_retained_writes"));
    let binary = compile_web_with_flags(&fixture.0, r#"<?php
extern function __elephc_web_gc_visited(): int;
extern function __elephc_web_gc_rebuild_cost(): int;
class WriteLeaf { public function __construct(public string $value) {} }
class WriteStore {
    public array $items = [];
    public array $log = [];
    public function __construct() {
        for ($i = 0; $i < 20000; $i++) { $this->items[] = new WriteLeaf(str_repeat('v', 40)); }
    }
    public function remember(WriteLeaf $leaf): void { $this->log[] = $leaf; }
}
class WriteCycle {
    public static int $destroyed = 0;
    public ?WriteCycle $self = null;
    public function __destruct() { self::$destroyed++; }
}
class Application implements Elephc\Web\WorkerApplication {
    private WriteStore $store;
    private ?Elephc\Web\Request $last = null;
    private int $requests = 0;
    public function __construct() { $this->store = new WriteStore(); }
    public function handle(Elephc\Web\Request $request): void {
        $visited = __elephc_web_gc_visited();
        $previous = $this->last === null ? 'none' : $this->last->uri;
        $leaf = new WriteLeaf('w' . $this->requests);
        $this->store->remember($leaf);
        $this->last = $request;
        $cycle = new WriteCycle();
        $cycle->self = $cycle;
        $this->requests++;
        echo $this->requests, ':', count($this->store->log), ':', $previous, ':',
            WriteCycle::$destroyed, ':', $visited, ':', count($this->store->items), ':', __elephc_web_gc_rebuild_cost();
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#, "app", &["--web-lifecycle=application", "--heap-size=67108864"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut first_visited = 0;
    let mut previous_visited = 0;
    let mut initial_scan_cost = 0;
    for request in 1..=30 {
        let response = body(&addr, &format!("/write?i={request}"));
        let parts: Vec<_> = response.split(':').collect();
        assert_eq!(parts[0], request.to_string(), "{response}");
        assert_eq!(parts[1], request.to_string(), "{response}");
        assert_eq!(parts[2], if request == 1 { "none".into() } else { format!("/write?i={}", request - 1) },
            "the preceding retained Request was invalid: {response}");
        assert_eq!(parts[3], (request - 1).to_string(), "cyclic cleanup was postponed: {response}");
        assert_eq!(parts[5], "20000", "{response}");
        let visited = parts[4].parse::<usize>().unwrap();
        if request == 1 {
            first_visited = visited;
            initial_scan_cost = parts[6].parse::<usize>().unwrap();
            assert!(initial_scan_cost >= 20000, "missing constructor graph scan cost: {response}");
        }
        else {
            // The first collection can retire the constructor's queued leases once.
            // Later requests must stay small; startup work includes structural slots
            // and the two memo walks, so its bound uses the measured forest scan cost.
            assert!(visited >= previous_visited, "the cumulative work counter decreased: {response}");
            if request > 2 {
                assert!(visited - previous_visited < 1000,
                    "a small retained write rescanned the unrelated application graph: {previous_visited} -> {response}");
            }
            assert!(visited - first_visited < initial_scan_cost * 3 + 30 * 1000,
                "repeated small writes rebuilt the rooted forest: {first_visited} -> {response}");
        }
        previous_visited = visited;
    }
}
