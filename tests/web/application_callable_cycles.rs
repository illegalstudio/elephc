//! Purpose:
//! Exercises native callable cycles and retained captures across real HTTP boundaries.
//!
//! Called from:
//! - Persistent application lifecycle integration tests.
//!
//! Key details:
//! - Temporary cycles must destroy immediately with stable live memory and dead weak observers.
//! - Application-owned captures survive requests and are reclaimed after their last owner retires.

use super::*;

const HOLDERS: &str = include_str!("../fixtures/callable_cycles.php");

/// Closures, arrows, reference captures, nested descriptors, bindings and Fiber cycles do not accumulate.
#[test]
fn web_application_callable_cycles_release_every_request() {
    let fixture = Fixture(make_test_dir("web_application_callable_cycles"));
    let source = format!(r#"{HOLDERS}
class Application implements Elephc\Web\WorkerApplication {{
    private int $requests = 0;
    private ?WeakReference $previous = null;
    public function handle(Elephc\Web\Request $request): void {{
        $live = memory_get_usage();
        $dead = $this->previous === null || $this->previous->get() === null;
        $holder = new CapturedCycleHolder($request->path);
        $this->previous = WeakReference::create($holder);
        $this->requests++;
        echo $this->requests, ':', CapturedCycleHolder::$destroyed, ':', $live, ':',
            $dead ? 'dead' : 'alive', ':', $holder->invoke();
    }}
    public function shutdown(): void {{}}
}}
Elephc\Web\Worker::register(new Application());
"#);
    let binary = application(&fixture, &source);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut baseline = 0;
    let paths = ["/closure", "/arrow", "/value", "/reference", "/nested", "/bound", "/method", "/fiber", "/array", "/hash", "/boxed", "/ref-callable"];
    for request in 1..=512 {
        let response = body(&addr, paths[(request - 1) % paths.len()]);
        let fields: Vec<_> = response.split(':').collect();
        assert_eq!(fields[0], request.to_string(), "{response}");
        assert_eq!(fields[1], (request - 1).to_string(), "captured cycle escaped its boundary: {response}");
        assert_eq!(&fields[3..], &["dead", "4096"], "{response}");
        let live = fields[2].parse::<usize>().unwrap();
        if request == 32 { baseline = live; }
        if request > 32 { assert!(live <= baseline + 32_768, "temporary callable memory grew: {baseline} -> {response}"); }
    }
    let failed = http_get(&addr, "/throw");
    assert!(failed.starts_with("HTTP/1.1 500"), "{failed}");
    let response = body(&addr, "/closure");
    let fields: Vec<_> = response.split(':').collect();
    assert_eq!(&fields[..2], &["514", "513"], "cleanup after a throwing captured destructor failed: {response}");
    assert_eq!(&fields[3..], &["dead", "4096"], "{response}");
}

/// A callable retained during one request owns its captured object until a later request releases it.
#[test]
fn web_application_callable_captures_survive_until_replaced() {
    let fixture = Fixture(make_test_dir("web_application_retained_callable"));
    let source = format!(r#"{HOLDERS}
class Application implements Elephc\Web\WorkerApplication {{
    private Closure $read;
    private bool $active = false;
    private ?WeakReference $weak = null;
    public function handle(Elephc\Web\Request $request): void {{
        if ($request->path === '/make') {{
            $holder = new CapturedCycleHolder('/reference');
            $this->read = static function() use ($holder): void {{ echo ':', strlen($holder->payload); }};
            $this->weak = WeakReference::create($holder);
            $this->active = true;
        }}
        if ($request->path === '/drop') {{ $this->read = static function(): void {{}}; $this->active = false; }}
        echo CapturedCycleHolder::$destroyed, ':', $this->weak !== null && $this->weak->get() === null ? 'dead' : 'alive';
        if ($this->active) {{ $read = $this->read; $read(); }}
    }}
    public function shutdown(): void {{}}
}}
Elephc\Web\Worker::register(new Application());
"#);
    let binary = application(&fixture, &source);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    for path in ["/make", "/read", "/read"] { assert_eq!(body(&addr, path), "0:alive:4096"); }
    assert_eq!(body(&addr, "/drop"), "0:alive");
    assert_eq!(body(&addr, "/check"), "1:dead");
}
