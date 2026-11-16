//! Purpose:
//! Checks final ownership retirement for native callback bindings in application workers.
//!
//! Called from:
//! - The persistent application HTTP integration tests.
//!
//! Key details:
//! - A libc call takes a real callback without invoking it, retaining a captured cycle.
//! - Graceful worker teardown must collect that cycle after retiring the native lease.

use super::*;

/// Callback-captured cycles finalize after the application hook and native lease retirement.
#[test]
fn web_application_shutdown_collects_cycles_detached_by_extern_callbacks() {
    let fixture = Fixture(make_test_dir("web_application_extern_callbacks"));
    let log = fixture.0.join("lifecycle.log").to_string_lossy().replace('\\', "\\\\").replace('\'', "\\'");
    let source = r#"<?php
extern "c" {
    function malloc(int $bytes): int;
    function free(int $pointer): void;
    function qsort(int $base, int $count, int $size, callable $compare): void;
}
class CallbackCycle {
    public ?CallbackCycle $next = null;
    public function __destruct() { file_put_contents('__LOG__', "destroyed\n", FILE_APPEND); }
}
class Application implements Elephc\Web\WorkerApplication {
    public function handle(Elephc\Web\Request $request): void {
        $node = new CallbackCycle();
        $node->next = $node;
        $pointer = malloc(8);
        qsort($pointer, 0, 8, static function(int $left, int $right) use ($node): int {
            if ($node->next === null) { return 1; }
            return $left <=> $right;
        });
        free($pointer);
        echo $request->path;
    }
    public function shutdown(): void { file_put_contents('__LOG__', "shutdown\n", FILE_APPEND); }
}
Elephc\Web\Worker::register(new Application());
"#.replace("__LOG__", &log);
    // The extern declarations belong to this native bootstrap, outside strict PHP sources.
    let binary = compile_web_with_flags(&fixture.0, &source, "app", &["--web-lifecycle=application"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&binary, &addr, "1");
    assert_eq!(body(&addr, "/callback"), "/callback");
    let log = fixture.0.join("lifecycle.log");
    assert!(!log.exists(), "the native binding keeps its captured owner until shutdown");
    server.kill().unwrap();
    server.wait().unwrap();
    assert_eq!(fs::read_to_string(log).unwrap(), "shutdown\ndestroyed\n");
}
