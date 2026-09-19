//! Purpose:
//! Subprocess regressions for compiler and generated-program recursion limits.
//!
//! Called from:
//! - `cargo test --test compiler_security_limits_tests` through Rust's test harness.
//!
//! Key details:
//! - Potential stack exhaustion always occurs in a child process, never in the test harness.
//! - The two compiler invocations are serialized to keep peak memory bounded.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LIMIT_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Heap capacity used by the large-frame recursion fixture.
///
/// This stays above the runtime stack guard's 64 MiB cap so the fixture reaches
/// the stack diagnostic before accumulated recursive-frame values exhaust the heap.
const LARGE_FRAME_TEST_HEAP_BYTES: usize = 128 * 1024 * 1024;

/// Resolves the compiler binary built for this integration test.
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Creates an isolated directory for one compiler-security fixture.
fn make_test_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "elephc-{name}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create security-limit fixture directory");
    dir
}

/// Verifies a deeply nested hostile AST is rejected with a deterministic
/// compiler diagnostic rather than overflowing the compiler process stack.
#[test]
fn deeply_nested_source_reports_compiler_depth_limit() {
    let _guard = LIMIT_TEST_LOCK.lock().unwrap();
    let dir = make_test_dir("compiler-depth");
    let source = format!(
        "<?php $value = {}0{};",
        "[".repeat(20_000),
        "]".repeat(20_000)
    );
    let php = dir.join("main.php");
    fs::write(&php, source).expect("write deeply nested PHP fixture");

    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .arg(&php)
        .output()
        .expect("spawn compiler depth probe");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "hostile nesting must be rejected");
    assert!(
        stderr.contains("maximum compiler nesting depth exceeded"),
        "expected a controlled depth diagnostic, got status {:?}: {}",
        output.status,
        stderr
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies generated PHP recursion terminates through a runtime guard with a
/// useful fatal message instead of reaching the operating-system stack guard.
#[test]
fn generated_program_reports_runtime_recursion_limit() {
    let _guard = LIMIT_TEST_LOCK.lock().unwrap();
    let dir = make_test_dir("runtime-recursion");
    let php = dir.join("main.php");
    fs::write(
        &php,
        r#"<?php
function descend(int $depth): int {
    if ($depth >= 1000000) { return $depth; }
    return descend($depth + 1);
}
echo descend(0);
"#,
    )
    .expect("write recursive PHP fixture");

    let compile = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .arg(&php)
        .output()
        .expect("compile recursion fixture");
    assert!(
        compile.status.success(),
        "recursion fixture must compile before its runtime guard is tested: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let output = Command::new(dir.join("main"))
        .output()
        .expect("run recursion fixture");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the recursion guard must terminate execution");
    assert_eq!(
        output.status.code(),
        Some(255),
        "the call-stack fatal must use PHP's fatal-error status: {stderr}"
    );
    assert!(
        stderr.contains("Maximum call stack size reached. Infinite recursion?"),
        "expected a controlled runtime recursion diagnostic, got status {:?}: {}",
        output.status,
        stderr
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies recursion accounting includes large generated frame sizes so a
/// spill-heavy function cannot exhaust the native stack before the fatal guard.
#[test]
fn large_frames_report_runtime_recursion_limit_before_stack_overflow() {
    let _guard = LIMIT_TEST_LOCK.lock().unwrap();
    let dir = make_test_dir("runtime-large-frame-recursion");
    let php = dir.join("main.php");
    let mut source = String::from("<?php\nfunction descend_large(int $depth): int {\n");
    for local in 0..128 {
        source.push_str(&format!("    $v{local} = $depth + {local};\n"));
    }
    source.push_str(
        "    if ($depth >= 1000000) { return 0; }\n    $next = descend_large($depth + 1);\n    return $next",
    );
    for local in 0..128 {
        source.push_str(&format!(" + $v{local}"));
    }
    source.push_str(";\n}\necho descend_large(0);\n");
    fs::write(&php, source).expect("write large-frame recursion fixture");

    let compile = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .arg(format!("--heap-size={LARGE_FRAME_TEST_HEAP_BYTES}"))
        .arg(&php)
        .output()
        .expect("compile large-frame recursion fixture");
    assert!(
        compile.status.success(),
        "large-frame recursion fixture must compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let output = Command::new(dir.join("main"))
        .output()
        .expect("run large-frame recursion fixture");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "large-frame recursion must be bounded");
    assert_eq!(
        output.status.code(),
        Some(255),
        "the large-frame call-stack fatal must use PHP's fatal-error status: {stderr}"
    );
    assert!(
        stderr.contains("Maximum call stack size reached. Infinite recursion?"),
        "expected a controlled stack-limit diagnostic, got status {:?}: {stderr}",
        output.status
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a long acyclic include chain is rejected at a deterministic resolver
/// depth budget before native compiler recursion can exhaust the process stack.
#[test]
fn distinct_include_chain_reports_depth_limit() {
    let _guard = LIMIT_TEST_LOCK.lock().unwrap();
    let dir = make_test_dir("include-depth");
    const DEPTH: usize = 1_200;
    for index in 0..DEPTH {
        let next = if index + 1 == DEPTH {
            "<?php echo 'unreachable';".to_string()
        } else {
            format!("<?php require 'f{}.php';", index + 1)
        };
        fs::write(dir.join(format!("f{index}.php")), next).expect("write include-chain member");
    }

    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .arg(dir.join("f0.php"))
        .output()
        .expect("spawn include-depth probe");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "hostile include depth must be rejected");
    assert!(stderr.contains("maximum include depth exceeded"), "expected controlled include-depth diagnostic, got: {stderr}");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies deeply nested serialized values are rejected by the bounded
/// allocation-free preflight instead of overflowing inside `__rt_unser_at`.
#[test]
fn deeply_nested_unserialize_is_rejected_before_runtime_recursion() {
    let _guard = LIMIT_TEST_LOCK.lock().unwrap();
    let dir = make_test_dir("unserialize-depth");
    let depth = 20_000;
    let wire = format!("{}i:1;{}", "a:1:{i:0;".repeat(depth), "}".repeat(depth));
    let php = dir.join("main.php");
    fs::write(&php, format!("<?php var_dump(unserialize({wire:?}));"))
        .expect("write nested unserialize fixture");

    let compile = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .arg(&php)
        .output()
        .expect("compile nested unserialize fixture");
    assert!(compile.status.success(), "fixture compilation failed: {}", String::from_utf8_lossy(&compile.stderr));
    let output = Command::new(dir.join("main")).output().expect("run nested unserialize fixture");
    assert!(output.status.success(), "bounded preflight must reject without crashing: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "bool(false)\n");
    let _ = fs::remove_dir_all(&dir);
}


/// Verifies nesting BELOW the documented limit compiles and runs instead of aborting the
/// compiler (issue #686).
///
/// `MAX_COMPILER_NESTING` allows 1024 levels and the parser reports anything past it, but the
/// front end could not survive the walk: every recursive AST pass costs one frame per level, and
/// the default main-thread stack ran out around 140. `$a = [[[…1…]]]` at 200 levels aborted with
///
///     thread 'main' has overflowed its stack
///     fatal runtime error: stack overflow, aborting
///
/// — no diagnostic, no file, no position, and a source PHP itself compiles.
///
/// The compiler now runs inside `compiler_stack::with_compiler_stack`, sized against that
/// limit, so this fixture checks the two ends of it: 1000 levels produce a working program, and
/// the depth-limit test above still gets its diagnostic rather than an abort. The exit STATUS is
/// asserted too, because a stack overflow and a rejection both fail — only the message tells
/// them apart.
///
/// This one drives the BINARY. Its in-process twin is
/// `codegen::regressions::syntax_edges::test_deeply_nested_literal_compiles_and_runs_in_process`,
/// which runs the same depth through the crate's phases on a libtest worker thread: the two
/// together pin both the CLI and the embedding path.
#[test]
fn nesting_below_the_limit_compiles_and_runs() {
    let _guard = LIMIT_TEST_LOCK.lock().unwrap();
    let dir = make_test_dir("compiler-depth-ok");
    let depth = 1000;
    let source = format!(
        "<?php\n$a = {}1{};\necho count($a);\n",
        "[".repeat(depth),
        "]".repeat(depth)
    );
    let php = dir.join("main.php");
    fs::write(&php, source).expect("write deeply nested PHP fixture");

    let compile = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .arg("-q")
        .arg(&php)
        .output()
        .expect("spawn compiler depth probe");
    let stderr = String::from_utf8_lossy(&compile.stderr);
    assert!(
        !stderr.contains("overflowed its stack") && !stderr.contains("stack overflow"),
        "the compiler must not abort below its own nesting limit: {stderr}"
    );
    assert!(
        compile.status.success(),
        "nesting below the limit must compile, got {:?}: {stderr}",
        compile.status
    );

    let program = dir.join("main");
    let run = Command::new(&program).output().expect("run nested fixture");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "1",
        "host PHP prints 1 for the same source"
    );
    let _ = fs::remove_dir_all(&dir);
}
