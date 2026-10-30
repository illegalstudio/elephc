//! Purpose:
//! Regression coverage for `preg_replace_callback` releasing its private buffers
//! when the callback throws, including after the output block has grown.
//!
//! Called from:
//! - `cargo test --test codegen_tests preg_replace_callback_throw` through Rust's test harness.
//!
//! Key details:
//! - Expected stdout was measured on PHP 8.5.9. PHP 8.2–8.4 share this behavior:
//!   a throw abandons the replacement, and an invalid pattern, an unknown
//!   modifier, a missing delimiter, or `/e` yields null.
//! - Repeated catches must not accumulate helper-owned blocks. The 65536-byte heap
//!   is the size that previously died after a handful of caught throws.
//! - `--heap-debug` counts elephc heap blocks only. PCRE2's compiled pattern and
//!   match data come from the libc allocator, so this summary cannot see them.
//!   The throw path still frees those pointers through the shim and `free`.
//! - Nested calls, `finally`, closures that capture objects, static callables,
//!   and a generator each match a PHP 8.5.9 transcript and finish with a clean
//!   elephc heap.

use crate::support::*;

/// Compiles and runs one fixture at a chosen heap size, optionally with heap debugging.
fn run_preg_callback(source: &str, heap_size: usize, heap_debug: bool) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "elephc_preg_cb_{}_{:?}_{}",
        std::process::id(),
        std::thread::current().id(),
        id
    ));
    fs::create_dir_all(&dir).unwrap();
    let (user_asm, runtime_asm, requirements) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, heap_debug);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let output = assemble_and_run_capture(
        &user_asm,
        &runtime_obj,
        &dir,
        &requirements,
        &default_link_paths(),
        &[],
    );
    let _ = fs::remove_dir_all(&dir);
    output
}

/// Reads the `live_blocks=` count from a `--heap-debug` leak summary.
///
/// The summary is elephc's heap. A block allocated by PCRE2 with libc `malloc`
/// does not appear here, even when the throw path failed to free it.
fn live_blocks(stderr: &str) -> usize {
    let summary = stderr
        .lines()
        .find(|line| line.contains("leak summary"))
        .unwrap_or_else(|| panic!("no leak summary in:\n{stderr}"));
    if summary.contains("clean") {
        return 0;
    }
    summary
        .split("live_blocks=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("no live_blocks in: {summary}"))
}

/// Builds the fixture. Subjects and callback names depend on `$argc` so the calls
/// survive constant folding. `iterations` repeats both the first-match throw and
/// the grow-then-throw call. The trailing calls return null for a compile failure,
/// `/e`, an unknown modifier, and a missing ending delimiter.
fn fixture(iterations: usize) -> String {
    format!(
        r#"<?php
function keep(array $m): string {{ return "Z"; }}
function boom(array $m): string {{ throw new Exception("x"); }}
function wide(array $m): string {{ return str_repeat("b", 5000); }}
function grow(array $m): string {{
    global $phase;
    if ($phase == 0) {{
        $phase = 1;
        return str_repeat("b", 5000);
    }}
    throw new Exception("g");
}}
function shortq(array $m): string {{ return "Q"; }}
$one = $argc > 0 ? "a" : "b";
$two = $argc > 0 ? "aa" : "bb";
$xa = $argc > 0 ? "xa" : "ya";
$keep = $argc > 0 ? "keep" : "keep";
$boom = $argc > 0 ? "boom" : "boom";
$wide = $argc > 0 ? "wide" : "wide";
$grow = $argc > 0 ? "grow" : "grow";
$shortq = $argc > 0 ? "shortq" : "shortq";
echo preg_replace_callback("/a/", $keep, $one), "\n";
echo strlen(preg_replace_callback("/a/", $wide, $one)), "\n";
echo preg_replace_callback("/a/", $shortq, $xa), "\n";
$dots = "";
for ($i = 0; $i < {iterations}; $i++) {{
    try {{
        preg_replace_callback("/a/", $boom, $one);
        $dots .= "!";
    }} catch (Exception $e) {{
        $dots .= ".";
    }}
}}
echo $dots, "\n";
$w = "";
for ($i = 0; $i < {iterations}; $i++) {{
    $s = preg_replace_callback("/a/", $wide, $one);
    $w .= (strlen($s) == 5000 ? "W" : "w");
}}
echo $w, "\n";
$marks = "";
for ($i = 0; $i < {iterations}; $i++) {{
    $phase = 0;
    try {{
        preg_replace_callback("/a/", $grow, $two);
        $marks .= "!";
    }} catch (Exception $e) {{
        $marks .= "g";
    }}
}}
echo $marks, "\n";
echo (preg_replace_callback("/(/", $keep, "subject") ?? "null"), "\n";
echo (preg_replace_callback("/a/e", $keep, $one) ?? "bad"), "\n";
echo (preg_replace_callback("/a/z", $keep, $one) ?? "null"), "\n";
echo (preg_replace_callback("/abc", $keep, "abc") ?? "null"), "\n";
$kept = null;
try {{
    preg_replace_callback("/(a)/", function($m) use (&$kept) {{
        $kept = $m[0];
        throw new Exception("k");
    }}, $one);
}} catch (Exception $e) {{
    echo $kept, "\n";
}}
$kept_arr = ["z"];
try {{
    preg_replace_callback("/(a)/", function($m) use (&$kept_arr) {{
        $kept_arr = $m;
        throw new Exception("k");
    }}, $one);
}} catch (Exception $e) {{
    echo $kept_arr[0], "\n";
}}
"#
    )
}

/// Stdout measured on PHP 8.5.9 for the same fixture.
fn expected(iterations: usize) -> String {
    format!(
        "Z\n5000\nxQ\n{}\n{}\n{}\nnull\nbad\nnull\nnull\na\na\n",
        ".".repeat(iterations),
        "W".repeat(iterations),
        "g".repeat(iterations),
    )
}

/// Caught throws, including throws after the output block grows, must match PHP
/// and must not leak the helper's subject or output blocks as the loop repeats.
#[test]
fn test_preg_replace_callback_throw_releases_private_buffers() {
    assert_throw_cleanup_does_not_scale(true);
}

/// The same release holds with the EIR optimizer forced off. The runtime helper
/// is not rewritten by that pass, but the caller's fresh-result cleanup is.
#[test]
fn test_preg_replace_callback_throw_releases_private_buffers_without_ir_opt() {
    without_ir_opt(|| assert_throw_cleanup_does_not_scale(false));
}

/// A `[$obj, 'method']` callback owns a runtime descriptor that retains the
/// receiver. A throw must drop that reference, so `__destruct` runs in PHP's
/// order and the descriptor is not left on the elephc heap.
#[test]
fn test_preg_replace_callback_throw_releases_bound_method() {
    let source = r#"<?php
class BoundCallback {
    public function __destruct() { echo "D"; }
    public function fail(array $matches): string {
        echo "M";
        throw new Exception("x");
    }
}
$marks = "";
for ($i = 0; $i < 2; $i++) {
    try {
        preg_replace_callback("/a/", [new BoundCallback(), "fail"], "a");
        $marks .= "!";
    } catch (Exception $e) {
        $marks .= "C";
    }
}
echo $marks, "\n";
$obj = new BoundCallback();
try {
    preg_replace_callback("/a/", [$obj, "fail"], "a");
} catch (Exception $e) {
    echo "C";
}
echo "S";
$obj = null;
echo "E\n";
"#;
    let out = run_preg_callback(source, 8_388_608, true);
    assert!(
        out.success,
        "bound method throw failed\nstdout:\n{}\nstderr:\n{}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "MDMDCC\nMCSDE\n", "stderr:\n{}", out.stderr);
    assert_eq!(
        live_blocks(&out.stderr),
        0,
        "a thrown bound method left a block alive:\n{}",
        out.stderr
    );
}

/// An inner `preg_replace_callback` throw must free the inner receiver before
/// the outer one, and both before the caller's `catch`.
#[test]
fn test_preg_replace_callback_throw_nested_releases_both_receivers() {
    assert_clean_php(
        r#"<?php
class InnerObj {
    public function __destruct() { echo "Di"; }
    public function fail(array $m): string {
        echo "I";
        throw new Exception("x");
    }
}
class OuterObj {
    public function __destruct() { echo "Do"; }
    public function go(array $m): string {
        echo "O";
        return preg_replace_callback("/b/", [new InnerObj(), "fail"], "b");
    }
}
try {
    preg_replace_callback("/a/", [new OuterObj(), "go"], "a");
    echo "!";
} catch (Exception $e) {
    echo "C";
}
echo "E\n";
"#,
        "OIDiDoCE\n",
    );
}

/// `finally` runs after a temporary receiver is destroyed, and a receiver the
/// caller still holds stays alive until that variable is cleared.
#[test]
fn test_preg_replace_callback_throw_finally_orders_destructors() {
    assert_clean_php(
        r#"<?php
class FinTemp {
    public function __destruct() { echo "Dt"; }
    public function fail(array $m): string {
        echo "M";
        throw new Exception("fin");
    }
}
class FinKept {
    public function __destruct() { echo "Dk"; }
    public function fail(array $m): string {
        echo "N";
        throw new Exception("fin");
    }
}
try {
    try {
        preg_replace_callback("/a/", [new FinTemp(), "fail"], "a");
        echo "!";
    } finally {
        echo "Y";
    }
} catch (Exception $e) {
    echo "C";
}
$kept = new FinKept();
try {
    try {
        preg_replace_callback("/a/", [$kept, "fail"], "a");
        echo "!";
    } finally {
        echo "Z";
    }
} catch (Exception $e) {
    echo "C";
}
echo "S";
$kept = null;
echo "E\n";
"#,
        "MDtYCNZCSDkE\n",
    );
}

/// A closure that is the only remaining owner of a captured object must keep
/// that object alive across the throw and drop it when the closure itself dies.
#[test]
fn test_preg_replace_callback_throw_closure_capture_drops_with_closure() {
    assert_clean_php(
        r#"<?php
class CapObj {
    public string $tag;
    public function __construct(string $tag) { $this->tag = $tag; }
    public function __destruct() { echo "D", $this->tag; }
}
$live = new CapObj("v");
$fn = function (array $m) use ($live): string {
    echo "M";
    throw new Exception("cap");
};
$live = null;
try {
    preg_replace_callback("/a/", $fn, "a");
    echo "!";
} catch (Exception $e) {
    echo "C";
}
echo "S";
$fn = null;
echo "E\n";
"#,
        "MCSDvE\n",
    );
}

/// A static callable array does not retain an unrelated object past the throw.
#[test]
fn test_preg_replace_callback_throw_static_callable_stays_clean() {
    assert_clean_php(
        r#"<?php
class StatCb {
    public static function fail(array $m): string {
        echo "M";
        throw new Exception("stat");
    }
}
class StatBox {
    public function __destruct() { echo "Ds"; }
}
$box = new StatBox();
try {
    preg_replace_callback("/a/", ["StatCb", "fail"], "a");
    echo "!";
} catch (Exception $e) {
    echo "C";
}
echo "S";
$box = null;
echo "E\n";
"#,
        "MCSDsE\n",
    );
}

/// A generator that catches a callback throw resumes, then destroys the object
/// its callback closure captured.
#[test]
fn test_preg_replace_callback_throw_inside_generator_releases_capture() {
    assert_clean_php(
        r#"<?php
class GenObj {
    public function __destruct() { echo "Dg"; }
}
function gen_throw(): Generator {
    $obj = new GenObj();
    try {
        preg_replace_callback("/a/", function (array $m) use ($obj): string {
            echo "M";
            throw new Exception("gen");
        }, "a");
        echo "!";
        yield "bad";
    } catch (Exception $e) {
        echo "C";
    }
    yield "Y";
    echo "A";
}
foreach (gen_throw() as $v) {
    echo $v;
}
echo "E\n";
"#,
        "MCYADgE\n",
    );
}

/// Compiles one fixture with heap debugging and requires the PHP transcript plus a clean elephc heap.
fn assert_clean_php(source: &str, expected: &str) {
    let out = run_preg_callback(source, 8_388_608, true);
    assert!(
        out.success,
        "fixture failed\nstdout:\n{}\nstderr:\n{}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, expected, "stderr:\n{}", out.stderr);
    assert_eq!(
        live_blocks(&out.stderr),
        0,
        "throw left an elephc block alive:\n{}",
        out.stderr
    );
}

/// Runs the fixture once and twenty times. Live blocks must not grow with the
/// iteration count. The twenty-iteration run also has to finish in a 64 KiB heap.
fn assert_throw_cleanup_does_not_scale(check_small_heap: bool) {
    let once = run_preg_callback(&fixture(1), 8_388_608, true);
    assert!(
        once.success,
        "one iteration failed\nstdout:\n{}\nstderr:\n{}",
        once.stdout, once.stderr
    );
    assert_eq!(once.stdout, expected(1), "stderr:\n{}", once.stderr);

    let many = run_preg_callback(&fixture(20), 8_388_608, true);
    assert!(
        many.success,
        "twenty iterations failed\nstdout:\n{}\nstderr:\n{}",
        many.stdout, many.stderr
    );
    assert_eq!(many.stdout, expected(20), "stderr:\n{}", many.stderr);
    let (once_blocks, many_blocks) = (live_blocks(&once.stderr), live_blocks(&many.stderr));
    assert_eq!(
        once_blocks, 0,
        "one iteration left an elephc block alive after throw and null paths:\n{}",
        once.stderr
    );
    assert_eq!(
        many_blocks, 0,
        "live blocks scale with caught throws, so a helper buffer is leaking:\n\
         1 iteration -> {once_blocks}\n20 iterations -> {many_blocks}\n\n{}",
        many.stderr
    );

    if check_small_heap {
        let small = run_preg_callback(&fixture(20), 65_536, true);
        assert!(
            small.success,
            "65536-byte heap exhausted\nstdout:\n{}\nstderr:\n{}",
            small.stdout, small.stderr
        );
        assert_eq!(small.stdout, expected(20), "stderr:\n{}", small.stderr);
    }
}
