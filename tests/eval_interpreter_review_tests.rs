//! Purpose:
//! End-to-end regression tests for defects of the eval interpreter found by the branch review.
//!
//! Called from:
//! - `cargo test --test eval_interpreter_review_tests` through Rust's test harness.
//!
//! Key details:
//! - Every expected output is PINNED FROM REFERENCE `php -n` running the same fragment.
//! - The fragment comes from an environment variable, so the compiler never sees it and every
//!   row runs through the eval interpreter.
//! - Diagnostics are observed through `set_error_handler` so the text does not depend on paths.
//! - Tests invoke the elephc CLI as a subprocess in an isolated temp dir. Host target only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// A program whose only behaviour is the opaque fragment it evaluates.
const EVAL_ONLY: &str = "<?php\n$code = getenv('REVIEW_CODE');\neval($code);\n";

/// Creates an isolated temp dir unique across parallel test threads and processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "{}_{}_{:?}_{}",
        prefix,
        std::process::id(),
        std::thread::current().id(),
        id
    ));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Compiles `source` as `main.php` in a fresh dir under the PHP 8.5 profile.
fn compile(prefix: &str, source: &str) -> PathBuf {
    let dir = make_test_dir(prefix);
    fs::write(dir.join("main.php"), source).unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg(dir.join("main.php"))
        .args(["--php-version", "8.5"])
        .output()
        .expect("failed to spawn elephc");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    dir.join("main")
}

/// Runs `binary` with `code` as the eval'd fragment.
fn run(binary: &Path, code: &str) -> Output {
    Command::new(binary)
        .env("REVIEW_CODE", code)
        .output()
        .expect("failed to run compiled binary")
}

/// Runs `code` and asserts exact stdout, showing stderr on a mismatch.
fn assert_stdout(prefix: &str, code: &str, expected: &str) {
    let binary = compile(prefix, EVAL_ONLY);
    let output = run(&binary, code);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        expected,
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Reading an absent array key inside `eval()` raises php's `Undefined array key` warning with
/// the quoted string or bare integer key; quiet fetches stay silent and a present null is a
/// plain read. `ArrayIterator::offsetGet` warns the same way.
#[test]
fn absent_array_key_reads_warn_inside_eval() {
    let code = "set_error_handler(function($n,$s){echo \"W:$s\\n\";return true;});\n\
$x=['a'=>1]; var_dump($x['missing']); var_dump($x[5]); var_dump($x['a']??0); \
var_dump(isset($x['q'])); var_dump($x['q']['r'] ?? 7);\n\
$it=new ArrayIterator(['a'=>1]); var_dump($it['zz']); var_dump($it->offsetGet('yy')); \
$n=['k'=>null]; var_dump($n['k']);\n";
    let expected = "W:Undefined array key \"missing\"\nNULL\nW:Undefined array key 5\nNULL\nint(1)\n\
bool(false)\nint(7)\nW:Undefined array key \"zz\"\nNULL\nW:Undefined array key \"yy\"\nNULL\nNULL\n";
    assert_stdout("eval_absent_key", code, expected);
}

/// Each closure OBJECT has its own `static` storage, generator closures included: a generator
/// closure created again in a loop starts its counter over (php prints `1212`, the interpreter
/// used to share one slot between the objects and print `1234`).
#[test]
fn generator_closures_keep_statics_per_closure_object() {
    let code = "for ($i = 0; $i < 2; $i++) {\n\
  $g = function () { static $c = 0; $c++; yield $c; };\n\
  foreach ($g() as $x) echo $x; foreach ($g() as $x) echo $x;\n\
}\n\
echo \"\\n\";\n";
    assert_stdout("eval_gen_statics", code, "1212\n");
}
