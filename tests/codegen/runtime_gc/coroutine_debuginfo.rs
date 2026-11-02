//! Purpose:
//! Keeps private coroutine cancellation out of PHP debug-info exception reports.
//!
//! Called from:
//! - Focused native coroutine and ownership integration tests.
//!
//! Key details:
//! - Fatal fixtures need the actual exit status as well as both output streams.
//! - Real finally exceptions retain the ordinary warning and trace path.

use crate::support::*;

/// Compiles one fatal fixture and preserves the native process status for assertions.
fn run_fatal(source: &str) -> std::process::Output {
    let dir = make_cli_test_dir("elephc_coroutine_debuginfo");
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        /// Retires only the directory created for this native fixture.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let _cleanup = Cleanup(dir.clone());
    let (user, runtime, requirements) =
        compile_source_to_asm_with_options(source, &dir, 8 * 1024 * 1024, false, false);
    let runtime_object = runtime_obj_for_asm(&runtime);
    let object = dir.join("test.o");
    let binary = dir.join("test");
    assemble_from_stdin(&user, &object);
    link_binary(&object, &runtime_object, &binary, &requirements, &default_link_paths(), &[]);
    run_binary(&binary, &dir)
}

/// Abandoning a suspended debug-info method prints PHP's fatal without rendering cancellation.
#[test]
fn test_gc_coroutine_debuginfo_cancellation_is_private() {
    for dump in ["var_dump", "print_r"] {
        for keep in ["", "$keep = str_repeat('x', $argc + 2);"] {
            let source = r#"<?php
class D {
    public function __debugInfo(): array {
        echo "in\n";
        Fiber::suspend(1);
        return [];
    }
}
$f = new Fiber(function (): void { __DUMP__(new D()); echo "after\n"; });
$f->start();
echo "suspended\n";
__KEEP__
$f = null;
echo "end\n";
"#.replace("__DUMP__", dump).replace("__KEEP__", keep);
            let out = run_fatal(&source);
            assert_eq!(out.status.code(), Some(255));
            assert!(out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
            let stdout = String::from_utf8(out.stdout).unwrap();
            let (prefix, location) = stdout.split_once("\nFatal error: __debuginfo() must return an array in ").unwrap();
            assert_eq!(prefix, "in\nsuspended\n", "{dump}: {stdout}");
            let filename = location.strip_suffix(" on line 9\n").unwrap();
            assert!(!filename.is_empty() && !filename.contains('\n'), "{stdout}");
        }
    }
}

/// A real finally exception still receives PHP's warning, message and trace before the fatal.
#[test]
fn test_gc_coroutine_debuginfo_real_finally_error_keeps_warning() {
    let out = run_fatal(r#"<?php
class D {
    public function __debugInfo(): array {
        try { Fiber::suspend(1); }
        finally { throw new Exception('real'); }
    }
}
$f = new Fiber(function (): void { var_dump(new D()); });
$f->start();
$f = null;
"#);
    assert_eq!(out.status.code(), Some(255));
    assert!(out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("\nWarning: Uncaught Exception: real"), "{stdout}");
    assert!(stdout.contains("\nStack trace:\n"), "{stdout}");
    assert!(stdout.contains("\nFatal error: __debuginfo() must return an array in "), "{stdout}");
}
