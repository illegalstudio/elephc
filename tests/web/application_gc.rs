//! Purpose:
//! Checks immediate cyclic cleanup without scanning unchanged application data.
//!
//! Called from:
//! - Persistent application HTTP integration tests.
//!
//! Key details:
//! - Native diagnostics count candidate nodes instead of asserting unstable wall-clock limits.
//! - The shared application graph stays large while request garbage remains bounded.

use super::*;

/// Unchanged retained objects do not enter later collections, even with per-request cycles.
#[test]
fn web_application_gc_skips_unchanged_retained_graph() {
    let fixture = Fixture(make_test_dir("web_application_gc_retained"));
    let binary = compile_web_with_flags(&fixture.0, r#"<?php
extern function __elephc_web_gc_visited(): int;
class RetainedValue { public function __construct(public int $value) {} }
class TemporaryCycle {
    public static int $destroyed = 0;
    public ?TemporaryCycle $self = null;
    public function __destruct() { self::$destroyed++; }
}

class Application implements Elephc\Web\WorkerApplication {
    private array $items = [];
    private int $count = 0;
    public function __construct() {
        for ($i = 0; $i < 20000; $i++) { $this->items[] = new RetainedValue($i); }
    }
    public function handle(Elephc\Web\Request $request): void {
        $visited = __elephc_web_gc_visited();
        $this->count++;
        $cycle = new TemporaryCycle();
        $cycle->self = $cycle;
        echo $this->count, ':', count($this->items), ':', TemporaryCycle::$destroyed, ':', $visited;
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#, "app", &["--web-lifecycle=application"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut previous_visited = 0;
    for count in 1..=8 {
        let response = body(&addr, "/");
        let parts: Vec<_> = response.split(':').collect();
        assert_eq!(&parts[..3], &[count.to_string(), "20000".to_string(), (count - 1).to_string()], "{response}");
        let visited = parts[3].parse::<usize>().unwrap();
        if count > 2 {
            assert!(visited >= previous_visited && visited - previous_visited < 1000,
                "unchanged retained graph was scanned between requests: {previous_visited} -> {response}");
        }
        previous_visited = visited;
    }
}

/// Old cycles, resurrection, and a throwing destructor retain their request-boundary timing.
#[test]
fn web_application_gc_recounts_detached_and_resurrected_cycles_after_error() {
    let fixture = Fixture(make_test_dir("web_application_gc_recount"));
    let binary = application(&fixture, r#"<?php
class CandidateNode {
    public static int $destroyed = 0;
    public static ?CandidateNode $rescued = null;
    public ?CandidateNode $next = null;
    public array $floats = [1.25, 2.5];
    public array $strings = ['first', 'second'];
    public function __construct(public bool $rescue = false, public bool $fail = false) {}
    public function __destruct() {
        self::$destroyed++;
        if ($this->rescue) { $this->rescue = false; self::$rescued = $this; }
        if ($this->fail) { throw new RuntimeException('cycle destructor failure'); }
    }
}
class Application implements Elephc\Web\WorkerApplication {
    private int $count = 0;
    private ?CandidateNode $saved = null;
    private ?WeakReference $old = null;
    private ?WeakReference $resurrected = null;
    public function handle(Elephc\Web\Request $request): void {
        $this->count++;
        if ($this->count === 1) {
            $node = new CandidateNode();
            $node->next = $node;
            $this->saved = $node;
            $this->old = WeakReference::create($node);
        }
        if ($this->count === 2) { $this->saved = null; }
        if ($this->count === 3) {
            $node = new CandidateNode(true);
            $node->next = $node;
            $this->resurrected = WeakReference::create($node);
        }
        if ($this->count === 5) {
            CandidateNode::$rescued = null;
            $node = new CandidateNode(false, true);
            $node->next = $node;
        }
        echo $this->count, ':', CandidateNode::$destroyed, ':',
            $this->old->get() === null ? 'dead' : 'alive', ':',
            CandidateNode::$rescued === null ? 'empty' : 'rescued', ':',
            $this->resurrected === null || $this->resurrected->get() === null ? 'dead' : 'alive';
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    assert_eq!(body(&addr, "/"), "1:0:alive:empty:dead");
    assert_eq!(body(&addr, "/"), "2:0:alive:empty:dead");
    assert_eq!(body(&addr, "/"), "3:1:dead:empty:alive");
    assert_eq!(body(&addr, "/"), "4:2:dead:rescued:alive");
    let failed = http_get(&addr, "/");
    assert!(failed.starts_with("HTTP/1.1 500"), "{failed}");
    assert_eq!(body(&addr, "/"), "6:3:dead:empty:dead");
    assert_eq!(body(&addr, "/"), "7:3:dead:empty:dead");
}

/// A queued address can be freed and reused without becoming a false cycle candidate.
#[test]
fn web_application_gc_reused_candidate_addresses_preserve_live_owners() {
    let fixture = Fixture(make_test_dir("web_application_gc_reuse"));
    let binary = application(&fixture, r#"<?php
class ReusedNode {
    public static int $destroyed = 0;
    public function __construct(public string $marker) {}
    public function __destruct() { self::$destroyed++; }
}
class Application implements Elephc\Web\WorkerApplication {
    private int $count = 0;
    private ?ReusedNode $keep = null;
    public function handle(Elephc\Web\Request $request): void {
        $this->count++;
        if ($this->count > 1 && $this->keep->marker !== 'keep-' . ($this->count - 1)) {
            throw new RuntimeException('retained value was reclaimed');
        }
        for ($i = 0; $i < 200; $i++) {
            $node = new ReusedNode(str_repeat('temporary', 16));
            $alias = $node;
            unset($alias);
            unset($node);
            $replacement = new ReusedNode('keep-' . $this->count);
            if ($i === 199) { $this->keep = $replacement; }
            unset($replacement);
        }
        echo $this->count, ':', $this->keep->marker, ':', ReusedNode::$destroyed, ':', memory_get_usage();
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut warm = 0;
    for count in 1..=25 {
        let response = body(&addr, "/");
        let parts: Vec<_> = response.split(':').collect();
        assert_eq!(&parts[..3], &[count.to_string(), format!("keep-{count}"), (count * 400 - 1).to_string()], "{response}");
        let live = parts[3].parse::<usize>().unwrap();
        if count == 5 { warm = live; }
        if count > 5 { assert!(live <= warm + 1024, "temporary owners grew: {warm} -> {live}"); }
    }
}

/// Saturated incoming counts retain exact root decisions with queued and pinned metadata present.
#[test]
fn web_application_gc_saturated_candidate_counts_reclaim_cycles() {
    let fixture = Fixture(make_test_dir("web_application_gc_saturation"));
    let binary = application(&fixture, r#"<?php
class FanInCycle {
    public static int $destroyed = 0;
    public array $links = [];
    public function __destruct() {
        if (count($this->links) !== 9000) { throw new RuntimeException('incomplete cycle snapshot'); }
        self::$destroyed++;
    }
}
class Application implements Elephc\Web\WorkerApplication {
    private int $count = 0;
    public function handle(Elephc\Web\Request $request): void {
        $this->count++;
        $node = new FanInCycle();
        $aliases = [];
        for ($i = 0; $i < 9000; $i++) { $aliases[] = $node; }
        $node->links = $aliases;
        unset($aliases);
        unset($node);
        gc_collect_cycles();
        echo $this->count, ':', FanInCycle::$destroyed, ':', memory_get_usage();
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut warm = 0;
    for count in 1..=10 {
        let response = body(&addr, "/");
        let parts: Vec<_> = response.split(':').collect();
        assert_eq!(&parts[..2], &[count.to_string(), count.to_string()], "{response}");
        let live = parts[2].parse::<usize>().unwrap();
        if count == 3 { warm = live; }
        if count > 3 { assert!(live <= warm + 4096, "cycle owners grew: {warm} -> {live}"); }
    }
}

/// Borrowing services through object spines and container maps does not re-trace their retained data.
#[test]
fn web_application_gc_proves_deep_and_map_held_services() {
    let fixture = Fixture(make_test_dir("web_application_gc_deep_proofs"));
    let binary = compile_web_with_flags(&fixture.0, r#"<?php
extern function __elephc_web_gc_visited(): int;
class ProofLeaf { public function __construct(public int $value) {} }
class ProofService {
    private array $items = [];
    public function __construct() {
        for ($i = 0; $i < 20000; $i++) { $this->items[] = new ProofLeaf($i); }
    }
    public function size(): int { return count($this->items); }
}
class ProofKernel { public ProofService $service; public function __construct() { $this->service = new ProofService(); } }
class ProofCycle {
    public static int $destroyed = 0;
    public ?ProofCycle $self = null;
    public function __destruct() { self::$destroyed++; }
}
class Application implements Elephc\Web\WorkerApplication {
    private ProofKernel $kernel;
    private array $services = [];
    private int $requests = 0;
    public function __construct() {
        $this->kernel = new ProofKernel();
        $this->services['router'] = new ProofService();
    }
    public function handle(Elephc\Web\Request $request): void {
        $visited = __elephc_web_gc_visited();
        $this->requests++;
        $cycle = new ProofCycle();
        $cycle->self = $cycle;
        if ($request->path === '/object') {
            $service = $this->kernel->service;
            echo $service->size();
        } elseif ($request->path === '/map') {
            $service = $this->services['router'];
            echo $service->size();
        } else { echo 'none'; }
        echo ':', $visited, ':', $this->requests, ':', ProofCycle::$destroyed;
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#, "app", &["--web-lifecycle=application", "--heap-size=67108864"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut previous = 0;
    for (index, path) in ["/none", "/object", "/map", "/object", "/map", "/none", "/object", "/map"].iter().enumerate() {
        let response = body(&addr, path);
        let parts: Vec<_> = response.split(':').collect();
        assert_eq!(parts[0], if *path == "/none" { "none" } else { "20000" }, "{response}");
        assert_eq!(parts[2], (index + 1).to_string(), "{response}");
        assert_eq!(parts[3], index.to_string(), "request cycle cleanup was deferred: {response}");
        let visited = parts[1].parse::<usize>().unwrap();
        if index > 1 {
            assert!(visited >= previous && visited - previous < 1000,
                "a pure retained service read re-traced the application graph: {previous} -> {response}");
        }
        previous = visited;
    }
}

/// Rehashing, replacement and tag changes invalidate cached paths without postponing cyclic cleanup.
#[test]
fn web_application_gc_proofs_follow_current_edges_after_mutation() {
    let fixture = Fixture(make_test_dir("web_application_gc_changed_proofs"));
    let binary = compile_web_with_flags(&fixture.0, r#"<?php
extern function __elephc_web_gc_visited(): int;
class ChangedLeaf { public function __construct(public int $value) {} }
class ChangedService {
    public static int $destroyed = 0;
    public ?ChangedService $self = null;
    private array $items = [];
    public function __construct(public int $id) {
        $this->self = $this;
        for ($i = 0; $i < 2000; $i++) { $this->items[] = new ChangedLeaf($i); }
    }
    public function size(): int { return count($this->items); }
    public function __destruct() { self::$destroyed++; }
}
class ChangedKernel {
    public ?ChangedService $service;
    public WeakReference $weak;
    public function __construct() {
        $service = new ChangedService(2);
        $this->weak = WeakReference::create($service);
        $this->service = $service;
    }
}
class Application implements Elephc\Web\WorkerApplication {
    private array $services = [];
    private ChangedKernel $kernel;
    private WeakReference $oldMap;
    private WeakReference $oldObject;
    private int $requests = 0;
    public function __construct() {
        $service = new ChangedService(1);
        $this->oldMap = WeakReference::create($service);
        $this->services['router'] = $service;
        $this->kernel = new ChangedKernel();
        $this->oldObject = $this->kernel->weak;
    }
    public function handle(Elephc\Web\Request $request): void {
        $visited = __elephc_web_gc_visited();
        $this->requests++;
        if ($request->path === '/rehash') {
            for ($i = 0; $i < 100; $i++) { $this->services['key' . $i] = $i; }
        }
        if ($request->path === '/replace') { $this->services['router'] = new ChangedService(3); }
        if ($request->path === '/detach') { $this->kernel->service = null; }
        if ($request->path === '/retag') { $this->services['router'] = 42; }
        echo $this->requests, ':', ChangedService::$destroyed, ':',
            $this->oldMap->get() === null ? 'dead' : 'alive', ':',
            $this->oldObject->get() === null ? 'dead' : 'alive', ':', $visited;
        if ($request->path !== '/retag' && $request->path !== '/final') {
            $service = $this->services['router'];
            echo ':', $service->id, ':', $service->size();
        }
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#, "app", &["--web-lifecycle=application"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    assert!(body(&addr, "/read").ends_with(":1:2000"));
    body(&addr, "/rehash");
    for phase in ["rehash", "replace"] {
        if phase == "replace" { body(&addr, "/replace"); }
        let mut previous = 0;
        for read in 0..6 {
            let response = body(&addr, "/read");
            let parts: Vec<_> = response.split(':').collect();
            assert_eq!(parts[5], if phase == "rehash" { "1" } else { "3" }, "{response}");
            assert_eq!(parts[6], "2000", "newly retained request values were corrupted: {response}");
            if phase == "replace" {
                assert_eq!(parts[1], "1", "detached map cycle survived its boundary: {response}");
                assert_eq!(parts[2], "dead", "{response}");
            }
            assert_eq!(parts[3], "alive", "{response}");
            let visited = parts[4].parse::<usize>().unwrap();
            if read >= 4 {
                assert!(visited >= previous && visited - previous < 1000,
                    "new or rehashed paths did not earn a reusable proof: {previous} -> {response}");
            }
            previous = visited;
        }
    }
    body(&addr, "/detach");
    let detached = body(&addr, "/read");
    let parts: Vec<_> = detached.split(':').collect();
    assert_eq!(&parts[1..4], &["2", "dead", "dead"], "detached object cycle survived its boundary: {detached}");
    body(&addr, "/retag");
    let retagged = body(&addr, "/final");
    let parts: Vec<_> = retagged.split(':').collect();
    assert_eq!(&parts[1..4], &["3", "dead", "dead"], "retagged value retained its old cycle: {retagged}");

}

/// Large temporary batches and final retained graph teardown remain bounded in a real worker.
#[test]
fn web_application_gc_large_small_block_batches_shutdown_promptly() {
    use std::os::unix::process::CommandExt;
    let fixture = Fixture(make_test_dir("web_application_gc_small_block_shutdown"));
    let source = r#"<?php
class SmallLeaf { public function __construct(public int $value) {} }
class SmallService {
    public array $items = [];
    public function __construct(int $count) {
        for ($i = 0; $i < $count; $i++) { $this->items[] = new SmallLeaf($i); }
    }
}
class Application implements Elephc\Web\WorkerApplication {
    private SmallService $saved;
    private int $requests = 0;
    public function __construct() { $this->saved = new SmallService(100000); }
    public function handle(Elephc\Web\Request $request): void {
        $count = $request->path === '/large' ? 50000 : 1000;
        $temporary = new SmallService($count);
        $this->requests++;
        echo $this->requests, ':', count($temporary->items), ':', count($this->saved->items);
    }
    public function shutdown(): void { file_put_contents('__LOG__', (string) $this->requests); }
}
Elephc\Web\Worker::register(new Application());
"#;
    let log = fixture.0.join("lifecycle.log").to_string_lossy().replace('\\', "\\\\").replace('\'', "\\'");
    let binary = compile_web_with_flags(&fixture.0, &source.replace("__LOG__", &log), "app",
        &["--web-lifecycle=application", "--heap-size=67108864"]);
    let addr = format!("127.0.0.1:{}", free_port());
    struct GroupServer(ServerGuard);
    impl Drop for GroupServer {
        /// Stops every process owned by this fixture before the server guard reaps the master.
        fn drop(&mut self) {
            let group = format!("-{}", self.0.id());
            let _ = Command::new("kill").args(["-KILL", "--", &group])
                .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
        }
    }
    let child = Command::new(&binary).args(["--listen", &addr, "--workers", "1"])
        .process_group(0).spawn().expect("start isolated process group");
    let mut server = GroupServer(ServerGuard::new(child));
    wait_until_ready(&addr);
    assert_eq!(body(&addr, "/large"), "1:50000:100000");
    assert_eq!(body(&addr, "/small"), "2:1000:100000");
    assert_eq!(body(&addr, "/small"), "3:1000:100000");
    assert!(Command::new("kill").arg("-TERM").arg(server.0.id().to_string()).status().unwrap().success());
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(status) = server.0.try_wait().unwrap() {
            assert!(status.success(), "worker supervisor failed: {status}");
            break;
        }
        assert!(Instant::now() < deadline, "retained small-block graph did not shut down within eight seconds");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(fs::read_to_string(fixture.0.join("lifecycle.log")).unwrap(), "3");
}
