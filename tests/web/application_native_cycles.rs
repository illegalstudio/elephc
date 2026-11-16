//! Purpose:
//! Checks immediate native object cycle cleanup across persistent HTTP requests.
//!
//! Called from:
//! - Persistent application lifecycle integration tests.
//!
//! Key details:
//! - Generators and SPL storage must reclaim request-owned cycles at every boundary.
//! - Retained native state stays valid until a later request detaches its last root.

use super::*;

const OWNERS: &str = include_str!("../fixtures/native_object_cycles.php");

/// Unknown and blocked-class cycles release at every boundary, including nested throwing destructors.
#[test]
fn web_application_incomplete_object_cycles_release_property_hashes() {
    let fixture = Fixture(make_test_dir("web_application_incomplete_cycles"));
    let binary = application(&fixture, r#"<?php
class Plain {
    public static int $destroyed = 0;
    public static bool $fail = false;
    public mixed $back = null;
    public function __destruct() {
        self::$destroyed++;
        if (self::$fail) { throw new RuntimeException('incomplete cycle cleanup'); }
    }
}
class Application implements Elephc\Web\WorkerApplication {
    private int $requests = 0;
    private ?WeakReference $previous = null;
    public function handle(Elephc\Web\Request $request): void {
        $live = memory_get_usage();
        $dead = $this->previous === null || $this->previous->get() === null;
        Plain::$fail = $request->path === '/throw';
        if ($request->path === '/blocked') {
            $value = unserialize('O:5:"Plain":1:{s:4:"self";r:1;}', ['allowed_classes' => false]);
        } elseif ($request->path === '/known' || $request->path === '/throw') {
            $value = unserialize('O:7:"Missing":1:{s:1:"c";O:5:"Plain":1:{s:4:"back";r:1;}}');
        } else {
            $value = unserialize('O:7:"Missing":2:{s:4:"self";r:1;s:3:"pad";s:4096:"' . str_repeat('x', 4096) . '";}');
        }
        $this->previous = WeakReference::create($value);
        $this->requests++;
        echo $this->requests, ':', $live, ':', $dead ? 'dead' : 'alive', ':', Plain::$destroyed;
    }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new Application());
"#);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let mut baseline = 0;
    let mut destroyed = 0;
    for request in 1..=200 {
        let path = ["/missing", "/blocked", "/known"][(request - 1) % 3];
        let response = body(&addr, path);
        let fields: Vec<_> = response.split(':').collect();
        assert_eq!(fields[0], request.to_string(), "{response}");
        assert_eq!(&fields[2..], &["dead".to_string(), destroyed.to_string()], "{response}");
        let live = fields[1].parse::<usize>().unwrap();
        if request == 32 { baseline = live; }
        if request > 32 { assert!(live <= baseline + 32_768, "incomplete object memory grew: {baseline} -> {response}"); }
        if path == "/known" { destroyed += 1; }
    }
    let failed = http_get(&addr, "/throw");
    assert!(failed.starts_with("HTTP/1.1 500"), "{failed}");
    destroyed += 1;
    let response = body(&addr, "/missing");
    let fields: Vec<_> = response.split(':').collect();
    assert_eq!(&fields[2..], &["dead".to_string(), destroyed.to_string()], "{response}");
}

/// Native owners do not accumulate, including after a throwing cyclic destructor.
#[test]
fn web_application_native_cycles_release_generator_and_spl_every_request() {
    let fixture = Fixture(make_test_dir("web_application_native_cycles"));
    let source = format!(r#"{OWNERS}
class Application implements Elephc\Web\WorkerApplication {{
    private int $requests = 0;
    private ?WeakReference $previous = null;
    public function handle(Elephc\Web\Request $request): void {{
        $live = memory_get_usage();
        $dead = $this->previous === null || $this->previous->get() === null;
        $owner = new NativeCycleOwner($request->path);
        $this->previous = WeakReference::create($owner);
        $this->requests++;
        echo $this->requests, ':', NativeCycleOwner::$destroyed, ':', $live, ':', $dead ? 'dead' : 'alive';
    }}
    public function shutdown(): void {{}}
}}
Elephc\Web\Worker::register(new Application());
"#);
    let binary = application(&fixture, &source);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let paths = ["/fresh", "/started", "/returned", "/closure", "/closure-started", "/fixed", "/stack", "/queue", "/list"];
    let mut baseline = 0;
    for request in 1..=400 {
        let response = body(&addr, paths[(request - 1) % paths.len()]);
        let fields: Vec<_> = response.split(':').collect();
        assert_eq!(fields[0], request.to_string(), "{response}");
        assert_eq!(fields[1], (request - 1).to_string(), "native cycle escaped its boundary: {response}");
        assert_eq!(fields[3], "dead", "{response}");
        let live = fields[2].parse::<usize>().unwrap();
        if request == 32 { baseline = live; }
        if request > 32 { assert!(live <= baseline + 32_768, "native temporary memory grew: {baseline} -> {response}"); }
    }
    let failed = http_get(&addr, "/throw");
    assert!(failed.starts_with("HTTP/1.1 500"), "{failed}");
    let response = body(&addr, "/fresh");
    let fields: Vec<_> = response.split(':').collect();
    assert_eq!(&fields[..2], &["402", "401"], "{response}");
    assert_eq!(fields[3], "dead", "{response}");
}
