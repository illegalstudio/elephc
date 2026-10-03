//! Purpose:
//! End-to-end tests for RECURSIVE `var_dump()` output: nested arrays and hashes,
//! empty nested containers, mixed string/int keys, and every scalar type rendered
//! at depth. Every expectation here is the byte-for-byte output of the reference
//! PHP interpreter (`php -d xdebug.mode=off`, PHP 8.x) for the same program.
//!
//! Called from:
//! - `cargo test --test var_dump_nested_tests` through Rust's test harness.
//!
//! Key details:
//! - REGRESSION ANCHOR: `var_dump()` used to render EVERY nested array as `NULL`
//!   (`__rt_var_dump_array_mixed` / `__rt_var_dump_hash` fell through to
//!   `__rt_var_dump_emit_null_line` for value tags 4/5), and to DROP the element
//!   entirely when the static element type was itself an array (the walker
//!   lookup in `var_dump_array_walker` returned `None`, so no body was walked at
//!   all). `nested_indexed_array`, `nested_hash` and `empty_nested_array` are
//!   those three repros.
//! - Indentation is 2 spaces per nesting level and the closing `}` aligns with
//!   the line that opened the container; the runtime drives it from the
//!   `_vd_indent` global (`codegen_support::runtime::io::var_dump_walk`).
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an
//!   isolated temp dir, compile a plain executable, run it, and assert stdout —
//!   the same harness style as `opcache_ini_tests` / `extension_loaded_tests`.
//!   Host-target only (macOS aarch64 local).
//! - Compile STDERR is filtered to elephc's OWN diagnostics: on Linux, GNU `ld`
//!   adds static-glibc and `.note.GNU-stack` warnings that Apple's linker never
//!   emits, so an unfiltered assertion would be non-portable.
//! - KNOWN GAPS deliberately not asserted here: objects nested in a container
//!   still render `NULL` (PHP prints `object(C)#id (n) { … }`), and
//!   `*RECURSION*` is unreachable because elephc's parser rejects `$a[k] = &$v`,
//!   so a self-referential array cannot be built in the first place.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
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

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather
/// than anything elephc emitted: GNU `ld` reports static-glibc notes and the
/// `.note.GNU-stack` deprecation, while Apple's linker stays silent. Anchoring on
/// elephc's own line starts isolates its diagnostics — and still surfaces an
/// UNEXPECTED elephc warning, which an allow-list of known messages would hide.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("Warning: ")
                || line.starts_with("warning:")
                || line.starts_with("warning[")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compiles `source`, runs the executable and returns its STDOUT.
///
/// Asserts a clean compile (with no elephc diagnostics) and a clean exit first: a
/// walker that dereferences a wrong-shaped slot shows up as a signal, not as bad
/// text, so the status assertions are load-bearing.
fn run_php(stem: &str, source: &str) -> String {
    let dir = make_test_dir("elephc_var_dump_nested");
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();

    let mut cmd = Command::new(elephc_bin());
    cmd.arg("build");
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(&dir);
    cmd.arg(&php);
    let compile = cmd.output().expect("failed to spawn elephc");
    let raw_stderr = String::from_utf8_lossy(&compile.stderr).into_owned();
    assert!(
        compile.status.success(),
        "elephc compile failed:\n{raw_stderr}"
    );
    let diagnostics = elephc_diagnostics(&raw_stderr);
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostics:\n{diagnostics}"
    );

    run_binary(&dir.join(stem))
}

/// Runs a compiled executable and returns its STDOUT, asserting a clean exit.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A FLAT indexed array of ints is the pre-existing, already-working shape: the
/// recursion work must not shift its indentation or its `array(N) {` frame.
#[test]
fn flat_indexed_array() {
    let out = run_php("flat_indexed", "<?php var_dump([1, 2, 3]);\n");
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n  [2]=>\n  int(3)\n}\n"
    );
}

/// A FLAT hash with string keys — the other already-working shape, quoted keys.
#[test]
fn flat_hash() {
    let out = run_php("flat_hash", "<?php var_dump(['a' => 1, 'b' => 'x']);\n");
    assert_eq!(
        out,
        concat!(
            "array(2) {\n",
            "  [\"a\"]=>\n",
            "  int(1)\n",
            "  [\"b\"]=>\n",
            "  string(1) \"x\"\n",
            "}\n",
        )
    );
}

/// ONE level of nesting inside an indexed array. This is the primary repro: the
/// `[1]` element used to render as `NULL` because the boxed-cell walker had no
/// case for value tag 4/5.
#[test]
fn nested_indexed_array() {
    let out = run_php("nested_indexed", "<?php var_dump([1, [2, 3]]);\n");
    assert_eq!(
        out,
        concat!(
            "array(2) {\n",
            "  [0]=>\n",
            "  int(1)\n",
            "  [1]=>\n",
            "  array(2) {\n",
            "    [0]=>\n",
            "    int(2)\n",
            "    [1]=>\n",
            "    int(3)\n",
            "  }\n",
            "}\n",
        )
    );
}

/// ONE level of nesting inside a hash — the `__rt_var_dump_hash` half of the same
/// defect (`["a"]=>` followed by `NULL`).
#[test]
fn nested_hash() {
    let out = run_php("nested_hash", "<?php var_dump(['a' => ['b' => 1]]);\n");
    assert_eq!(
        out,
        concat!(
            "array(1) {\n",
            "  [\"a\"]=>\n",
            "  array(1) {\n",
            "    [\"b\"]=>\n",
            "    int(1)\n",
            "  }\n",
            "}\n",
        )
    );
}

/// TWO levels of nesting: the indent must grow by exactly 2 per level and each
/// closing brace must align with the line that opened its container.
#[test]
fn two_levels_nested() {
    let out = run_php("two_levels", "<?php var_dump([1, [2, [3, 4]]]);\n");
    assert_eq!(
        out,
        concat!(
            "array(2) {\n",
            "  [0]=>\n",
            "  int(1)\n",
            "  [1]=>\n",
            "  array(2) {\n",
            "    [0]=>\n",
            "    int(2)\n",
            "    [1]=>\n",
            "    array(2) {\n",
            "      [0]=>\n",
            "      int(3)\n",
            "      [1]=>\n",
            "      int(4)\n",
            "    }\n",
            "  }\n",
            "}\n",
        )
    );
}

/// An EMPTY nested array. Separate symptom from the `NULL` rendering: the element
/// used to VANISH entirely (`array(1) {\n}\n`), because the static element type
/// was an array and the walker lookup returned `None`, walking no body at all.
#[test]
fn empty_nested_array() {
    let out = run_php("empty_nested", "<?php var_dump([[]]);\n");
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  array(0) {\n  }\n}\n"
    );
}

/// Empty nested containers INTERLEAVED with a non-empty one, so a zero-count walk
/// cannot leave the indent state wrong for the entries that follow it.
#[test]
fn empty_and_populated_nested_arrays() {
    let out = run_php("empty_mixed", "<?php var_dump([[], [1], []]);\n");
    assert_eq!(
        out,
        concat!(
            "array(3) {\n",
            "  [0]=>\n",
            "  array(0) {\n",
            "  }\n",
            "  [1]=>\n",
            "  array(1) {\n",
            "    [0]=>\n",
            "    int(1)\n",
            "  }\n",
            "  [2]=>\n",
            "  array(0) {\n",
            "  }\n",
            "}\n",
        )
    );
}

/// MIXED string and integer keys in one hash: string keys render quoted, integer
/// keys bare, and the nested container under an integer key keeps the same rules.
#[test]
fn mixed_string_and_int_keys() {
    let out = run_php(
        "mixed_keys",
        "<?php var_dump(['a' => 1, 5 => [7, 'k' => 'v'], 'c' => true]);\n",
    );
    assert_eq!(
        out,
        concat!(
            "array(3) {\n",
            "  [\"a\"]=>\n",
            "  int(1)\n",
            "  [5]=>\n",
            "  array(2) {\n",
            "    [0]=>\n",
            "    int(7)\n",
            "    [\"k\"]=>\n",
            "    string(1) \"v\"\n",
            "  }\n",
            "  [\"c\"]=>\n",
            "  bool(true)\n",
            "}\n",
        )
    );
}

/// EVERY scalar type at depth inside an indexed array: int, float (PHP prints
/// `float(2.5)` and `float(1)` for `1.0`), string with its byte length, both
/// booleans, and null.
#[test]
fn every_scalar_type_at_depth_indexed() {
    let out = run_php(
        "scalars_indexed",
        "<?php var_dump([[1, 2.5, 1.0, \"str\", true, false, null]]);\n",
    );
    assert_eq!(
        out,
        concat!(
            "array(1) {\n",
            "  [0]=>\n",
            "  array(7) {\n",
            "    [0]=>\n",
            "    int(1)\n",
            "    [1]=>\n",
            "    float(2.5)\n",
            "    [2]=>\n",
            "    float(1)\n",
            "    [3]=>\n",
            "    string(3) \"str\"\n",
            "    [4]=>\n",
            "    bool(true)\n",
            "    [5]=>\n",
            "    bool(false)\n",
            "    [6]=>\n",
            "    NULL\n",
            "  }\n",
            "}\n",
        )
    );
}

/// EVERY scalar type at depth inside a hash, reached through the hash iterator's
/// value tags rather than through an indexed array's value_type stamp.
#[test]
fn every_scalar_type_at_depth_hash() {
    let out = run_php(
        "scalars_hash",
        concat!(
            "<?php var_dump(['top' => ",
            "['i' => 7, 'f' => 0.25, 'w' => 1.0, 's' => \"hi\", 'b' => false, 'n' => null]]);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "array(1) {\n",
            "  [\"top\"]=>\n",
            "  array(6) {\n",
            "    [\"i\"]=>\n",
            "    int(7)\n",
            "    [\"f\"]=>\n",
            "    float(0.25)\n",
            "    [\"w\"]=>\n",
            "    float(1)\n",
            "    [\"s\"]=>\n",
            "    string(2) \"hi\"\n",
            "    [\"b\"]=>\n",
            "    bool(false)\n",
            "    [\"n\"]=>\n",
            "    NULL\n",
            "  }\n",
            "}\n",
        )
    );
}

/// Deep nesting past the runtime pad's 64-byte chunk: 40 levels means an 80-space
/// indent on the innermost line, so `__rt_vd_pad` must loop rather than truncate.
#[test]
fn deep_nesting_beyond_one_pad_chunk() {
    const DEPTH: usize = 40;
    let mut literal = String::from("\"leaf\"");
    for _ in 0..DEPTH {
        literal = format!("[{}]", literal);
    }
    let out = run_php("deep_nesting", &format!("<?php var_dump({});\n", literal));

    let mut expected = String::new();
    for level in 0..DEPTH {
        expected.push_str(&format!("{}array(1) {{\n", " ".repeat(level * 2)));
        expected.push_str(&format!("{}[0]=>\n", " ".repeat((level + 1) * 2)));
    }
    expected.push_str(&format!("{}string(4) \"leaf\"\n", " ".repeat(DEPTH * 2)));
    for level in (0..DEPTH).rev() {
        expected.push_str(&format!("{}}}\n", " ".repeat(level * 2)));
    }
    assert_eq!(out, expected);
}

/// A nested dump reached through a FUNCTION RETURN and through a variable, not a
/// literal — the value arrives boxed rather than as a compile-time-shaped array.
#[test]
fn nested_through_variable_and_return() {
    let out = run_php(
        "nested_indirect",
        concat!(
            "<?php\n",
            "function mk(): array { return [1, [2, 3]]; }\n",
            "var_dump(mk());\n",
            "$m = ['k' => [4, 5]];\n",
            "var_dump($m);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "array(2) {\n",
            "  [0]=>\n",
            "  int(1)\n",
            "  [1]=>\n",
            "  array(2) {\n",
            "    [0]=>\n",
            "    int(2)\n",
            "    [1]=>\n",
            "    int(3)\n",
            "  }\n",
            "}\n",
            "array(1) {\n",
            "  [\"k\"]=>\n",
            "  array(2) {\n",
            "    [0]=>\n",
            "    int(4)\n",
            "    [1]=>\n",
            "    int(5)\n",
            "  }\n",
            "}\n",
        )
    );
}

/// Variadic `var_dump()` plus a nested dump captured through `ob_start()`: the
/// indent global must be back at 0 between arguments, and the recursive walk must
/// still route through the ob-aware sink rather than writing straight to fd 1.
#[test]
fn variadic_and_output_buffered() {
    let out = run_php(
        "variadic_ob",
        concat!(
            "<?php\n",
            "var_dump([1, [2]], 3);\n",
            "ob_start();\n",
            "var_dump([[4]]);\n",
            "$c = ob_get_clean();\n",
            "echo 'CAPTURED:', $c;\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "array(2) {\n",
            "  [0]=>\n",
            "  int(1)\n",
            "  [1]=>\n",
            "  array(1) {\n",
            "    [0]=>\n",
            "    int(2)\n",
            "  }\n",
            "}\n",
            "int(3)\n",
            "CAPTURED:array(1) {\n",
            "  [0]=>\n",
            "  array(1) {\n",
            "    [0]=>\n",
            "    int(4)\n",
            "  }\n",
            "}\n",
        )
    );
}
