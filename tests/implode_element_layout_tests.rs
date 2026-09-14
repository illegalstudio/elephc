//! Purpose:
//! Regression tests for `implode()` over every element layout an indexed array can carry,
//! reached both from a statically typed array and through a `Mixed`-typed one.
//!
//! Called from:
//! - `cargo test --test implode_element_layout_tests` through Rust's test harness.
//!
//! Key details:
//! - `__rt_implode` is chosen whenever the element type is not statically known, so it has
//!   to dispatch on the array's own `value_type` tag. It used to handle two layouts and read
//!   every other one as `(ptr, len)` string pairs: an int or float array had each element
//!   DEREFERENCED as a pointer (a segfault), and a bool array rendered every element empty.
//! - The `Mixed` probes route through `eval()` because that is the only way to obtain a
//!   `mixed`-typed array without the checker narrowing it back to a concrete element type.
//! - Float spelling is asserted against several shapes, not one: PHP renders floats at
//!   `precision = 14` with `zend_gcvt` fixups, so a whole number, a fraction and an
//!   exponential each have their own spelling and a `%f`-style renderer passes none of them.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp
//!   dir. Host-target only.

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

/// Compiles and runs one PHP source, returning its stdout.
fn compile_and_run(dir: &Path, source: &str) -> String {
    let probe = dir.join("main.php");
    fs::write(&probe, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&probe);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "compilation failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let run = Command::new(dir.join("main"))
        .output()
        .expect("failed to run binary");
    assert!(
        run.status.success(),
        "binary failed (exit {:?}):\nstdout: {}\nstderr: {}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// Builds a probe that joins a `mixed`-typed array, written through `eval()`.
fn mixed_probe(literal: &str) -> String {
    format!(
        r#"<?php
function f(): mixed {{ return {literal}; }}
echo implode(",", (array) f());
"#
    )
}

/// Verifies a `mixed`-typed array joins correctly for every element layout.
///
/// Int and float elements SEGFAULTED before the layout dispatch existed; bool elements
/// rendered empty. The empty-array and single-element cases are in the table because the
/// dispatch runs before the loop and must not mis-handle a zero- or one-iteration join.
#[test]
fn a_mixed_typed_array_joins_every_element_layout() {
    let dir = make_test_dir("implode_mixed_layout");
    let cases = [
        ("[1, 2]", "1,2"),
        ("[1.5, 2.5]", "1.5,2.5"),
        ("[true, false]", "1,"),
        (r#"["a", "b"]"#, "a,b"),
        ("[]", ""),
        ("[42]", "42"),
        ("[-7, 0, 7]", "-7,0,7"),
    ];
    for (literal, expected) in cases {
        let actual = compile_and_run(&dir, &mixed_probe(literal));
        assert_eq!(actual, expected, "implode of mixed {literal} rendered wrongly");
    }
}

/// Verifies a statically typed float array joins, which used to be a backend refusal.
///
/// "unsupported EIR backend feature: implode array element PHP type Float" — there was no
/// float renderer at all, so this is a new surface rather than only a crash fix.
#[test]
fn a_statically_typed_float_array_joins() {
    let dir = make_test_dir("implode_static_float");

    let joined = compile_and_run(&dir, r#"<?php echo implode(",", [1.5, 2.5, 3.25]);"#);

    assert_eq!(joined, "1.5,2.5,3.25");
}

/// Verifies floats keep PHP's spelling across the shapes `zend_gcvt` treats differently.
///
/// A whole number renders without a fraction, a repeating fraction is cut at `precision = 14`,
/// and a large magnitude goes exponential with an unpadded exponent. A renderer that used C's
/// `%f` would fail all three while still passing `1.5`.
#[test]
fn float_elements_keep_phps_spelling() {
    let dir = make_test_dir("implode_float_spelling");

    let joined = compile_and_run(
        &dir,
        r#"<?php echo implode("|", [1.0, 0.1, -2.75, 1.0E+25, 100.0, 0.5]);"#,
    );

    assert_eq!(joined, "1|0.1|-2.75|1.0E+25|100|0.5");
}

/// Verifies the glue survives between float elements.
///
/// This is the cursor invariant: `__rt_ftoa` formats into the same shared buffer the join is
/// writing to and advances the offset by the bytes it emitted, so an offset left parked at
/// the result start made the second element overwrite the glue. `implode(",", [1.5, 2.5])`
/// rendered `1.52222` — the right first element, then garbage.
#[test]
fn glue_survives_between_float_elements() {
    let dir = make_test_dir("implode_float_glue");

    let joined = compile_and_run(
        &dir,
        r#"<?php echo implode("<SEP>", [1.5, 2.5, 3.5, 4.5]), "|", strlen(implode("<SEP>", [1.5, 2.5]));"#,
    );

    assert_eq!(joined, "1.5<SEP>2.5<SEP>3.5<SEP>4.5|11");
}

/// Verifies a long float join does not walk off its buffer or lose elements.
///
/// The expected string is what `php -n` prints for this exact program, not a value derived
/// by hand — the first draft of this assertion was wrong about the tail and the length while
/// elephc was right.
#[test]
fn a_long_float_join_stays_intact() {
    let dir = make_test_dir("implode_float_long");

    let joined = compile_and_run(
        &dir,
        r#"<?php
$parts = [];
for ($i = 0; $i < 40; $i++) { $parts[] = $i + 0.5; }
$joined = implode(",", $parts);
echo substr($joined, 0, 11), "|", substr($joined, -9), "|", strlen($joined);
"#,
    );

    assert_eq!(joined, "0.5,1.5,2.5|38.5,39.5|189");
}
