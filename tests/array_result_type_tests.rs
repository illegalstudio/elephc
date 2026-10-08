//! Purpose:
//! End-to-end tests for two type-inference defects around a builtin's array-shaped result:
//!
//! - BUG A — a `bool`-element array joined by `implode()` rendered `false` as `"0"` instead of
//!   PHP's empty string, and `array_map()`'s CHECKER result element type was the INPUT array's
//!   element type rather than the callback's return type.
//! - BUG B — `array_keys()` refused a `mixed`-typed argument at COMPILE time, so reading an
//!   array out of any `mixed`-shaped value (a prelude builtin such as `opcache_get_status()`,
//!   `json_decode()`, an index read on a `mixed` container) and asking for its keys was a
//!   compile error, while `count()` and `foreach` over the very same expression compiled.
//!
//! Called from:
//! - `cargo test --test array_result_type_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   compile a plain executable, run it, and assert stdout — the same harness style as
//!   `function_exists_tests` / `opcache_ini_tests`. Host-target only (macOS aarch64 local).
//! - Every expected value in this file was taken from reference PHP 8.5.6
//!   (`php -d xdebug.mode=off`), including the `TypeError` message wording.
//! - REGRESSION ANCHOR (BUG A): `implode(",", [true, false])` printed `1,0`. The root cause was
//!   `implode_runtime_label` in `src/codegen/lower_inst/builtins/strings.rs` routing a `Bool`
//!   element array through `__rt_implode_int`, whose `__rt_itoa` pass renders false as `"0"`.
//!   It is NOT specific to `array_map` — the direct literal form was equally wrong.
//! - REGRESSION ANCHOR (BUG B): `array_keys($mixedHoldingAnArray)` failed the compile with
//!   `array_keys() argument must be array`. It is NOT specific to builtins: a plain
//!   `function f(): mixed { return ['a' => 1]; }` reproduced it identically.
//! - NEGATIVE CONTROLS: accepting `mixed` must not make the checker accept a statically wrong
//!   program. `array_keys(42)` / `array_keys("s")` / `array_keys(new C())` must still FAIL the
//!   compile, and `array_keys()` on a `mixed` that is not an array at runtime must raise PHP's
//!   catchable `TypeError` rather than reading the payload word as a container pointer.
//! - REGRESSION ANCHOR (found while checking BUG A's float row): `implode('|', $floats)` over
//!   boxed Mixed floats printed `0.500` instead of `0.5|0`. `__rt_implode` parked `_concat_off`
//!   at its result START for the whole join, so the nested `__rt_mixed_cast_string` →
//!   `__rt_ftoa` formatted its digits back OVER the glue and element bytes already copied.
//!   `__rt_implode` now publishes its live cursor before each nested cast and stamps the
//!   absolute end offset when it finishes. `callback_return_type_matrix_joins_like_php` is that
//!   repro — a regression shows up as a missing separator around the float.
//! - Compile-failure assertions filter stderr through `elephc_diagnostics` because the system
//!   linker (GNU `ld` on Linux) emits warnings macOS does not.

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

/// Runs the compiler on `source` with extra flags and returns its raw output.
fn compile_raw(dir: &Path, source: &str, stem: &str, flags: &[&str]) -> std::process::Output {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.args(flags).arg(&php);
    cmd.output().expect("failed to spawn elephc")
}

/// Compiles `source` to a plain executable with extra compiler flags and returns its path.
fn compile_with_flags(dir: &Path, source: &str, stem: &str, flags: &[&str]) -> PathBuf {
    let output = compile_raw(dir, source, stem, flags);
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Compiles `source` to a plain executable with no extra flags.
fn compile(dir: &Path, source: &str, stem: &str) -> PathBuf {
    compile_with_flags(dir, source, stem, &[])
}

/// Compiles `source` expecting FAILURE and returns elephc's own diagnostics from stderr.
fn compile_expecting_failure(dir: &Path, source: &str, stem: &str) -> String {
    let output = compile_raw(dir, source, stem, &[]);
    let raw = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "elephc compile unexpectedly SUCCEEDED — the checker over-accepted:\n{raw}"
    );
    elephc_diagnostics(&raw)
}

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted: GNU `ld` on Linux reports the static-`getaddrinfo` glibc notes and
/// the `.note.GNU-stack` deprecation, while Apple's linker stays silent. elephc's own lines
/// start with `error`/`warning`, so anchoring on those prefixes isolates its diagnostics — and
/// still surfaces an UNEXPECTED elephc diagnostic, which an allow-list would have hidden.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("error")
                || line.starts_with("Error")
                || line.starts_with("warning")
                || line.starts_with("Warning: ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs a compiled executable and returns its stdout, asserting a clean exit.
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

/// Runs a compiled executable expecting a FATAL exit, returning its combined output.
fn run_binary_expecting_fatal(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        !output.status.success(),
        "compiled binary unexpectedly exited 0 — the runtime array check did not fire"
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

// ---------------------------------------------------------------------------
// BUG A — bool rendering and `array_map`'s result element type
// ---------------------------------------------------------------------------

/// REGRESSION ANCHOR for BUG A's root cause, with NO `array_map` involved.
///
/// PHP stringifies `true` as `"1"` and `false` as the EMPTY string, so
/// `implode(",", [true, false])` is `"1,"` (reference PHP 8.5.6). elephc printed `"1,0"`
/// because a `Bool`-element array was joined by `__rt_implode_int`, which renders every element
/// through `__rt_itoa`. The `[false, false]` row pins the all-empty rendering, and the
/// single-element rows pin that no separator leaks in.
#[test]
fn implode_of_bool_array_renders_false_as_empty_string() {
    let dir = make_test_dir("array_result_implode_bool");
    let src = "<?php \
        echo '[', implode(',', [true, false]), \"]\\n\"; \
        echo '[', implode(',', [false, false]), \"]\\n\"; \
        echo '[', implode(',', [true, true]), \"]\\n\"; \
        echo '[', implode('--', [false, true, false]), \"]\\n\"; \
        echo '[', implode(',', [true]), \"]\\n\"; \
        echo '[', implode(',', [false]), \"]\\n\";";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out, "[1,]\n[,]\n[1,1]\n[--1--]\n[1]\n[]\n",
        "implode() over a bool array must render false as the empty string"
    );
}

/// The bug as originally reported: `array_map()` over a bool-returning callee, joined with
/// `implode()`. Both the INLINE literal source array (which the IR lowerer expands into an
/// `array_new` + inlined callback, producing a statically `array<bool>` result) and the
/// variable source array (which goes through the runtime `__rt_array_map`, producing boxed
/// Mixed cells) must render PHP-identically.
#[test]
fn array_map_over_bool_callback_renders_php_style() {
    let dir = make_test_dir("array_result_map_bool");
    let src = "<?php \
        function is_pos(int $n): bool { return $n > 0; } \
        $src = [1, 0, 2]; \
        echo '[', implode(',', array_map('is_pos', [1, 0, 2])), \"]\\n\"; \
        echo '[', implode(',', array_map('is_pos', $src)), \"]\\n\"; \
        var_export(array_map('is_pos', $src)); echo \"\\n\";";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "[1,,1]\n[1,,1]\n\
         array (\n  0 => true,\n  1 => false,\n  2 => true,\n)\n",
        "array_map() over a bool callback must join as PHP does"
    );
}

/// The reported opcache form, end to end: `array_map('opcache_is_script_cached', …)` over an
/// array of paths. With the cache enabled, the compiled script's own path is cached and a
/// non-existent one is not, so the reference joins to `1,` — the false must be empty, not `0`.
#[test]
fn array_map_over_bool_returning_builtin_renders_php_style() {
    let dir = make_test_dir("array_result_map_opcache");
    let src = "<?php \
        echo '[', implode(',', array_map('opcache_is_script_cached', [__FILE__, 'nope.php'])), \"]\\n\";";
    let bin = compile_with_flags(
        &dir,
        src,
        "app",
        &["--ini", "opcache.enable=1", "--ini", "opcache.enable_cli=1"],
    );
    assert_eq!(
        run_binary(&bin),
        "[1,]\n",
        "a bool-returning builtin mapped over paths must join as PHP does"
    );
}

/// `array_map()`'s CHECKER result element type must be the CALLBACK's return type, not the input
/// array's element type. The probe passes `$mapped[0]` — an `int[]` mapped through a
/// `bool`-returning callback — into a `string` parameter: the diagnostic must name `Bool`.
/// Before the fix it named `Int`, i.e. the input element type leaked through the map.
///
/// The probe declares `strict_types=1` because bool → string is a legal *coercive* binding:
/// reference PHP accepts it and prints `string(1) "1"`, so under the default mode there is no
/// error left to read the element type out of. Strict mode is where PHP throws
/// `TypeError: ... must be of type string, bool given` — verified against php 8.4 — and it is
/// therefore the only probe that still observes the element type through a diagnostic.
/// The sibling `..._follows_a_string_callback` test needs no such directive: its `"n1"` is a
/// non-numeric string, which PHP rejects for an `int` parameter in both modes.
#[test]
fn array_map_result_element_type_is_the_callback_return_type() {
    let dir = make_test_dir("array_result_map_elemty");
    let src = "<?php declare(strict_types=1); \
        function is_pos(int $n): bool { return $n > 0; } \
        function want_string(string $s): string { return $s; } \
        $mapped = array_map('is_pos', [1, 0]); \
        echo want_string($mapped[0]);";
    let diagnostics = compile_expecting_failure(&dir, src, "app");
    assert!(
        diagnostics.contains("expects string, got bool"),
        "array_map() must report the callback's Bool return as the element type, got:\n{diagnostics}"
    );
}

/// Same probe for a `string`-returning callback over an `int[]`: the element type must follow the
/// callback (`Str`), which the old "keep the input element type" rule got wrong in the other
/// direction. Passing the mapped element into an `int` parameter must name `Str`.
#[test]
fn array_map_result_element_type_follows_a_string_callback() {
    let dir = make_test_dir("array_result_map_elemty_str");
    let src = "<?php \
        function label(int $n): string { return 'n' . $n; } \
        function want_int(int $n): int { return $n; } \
        $mapped = array_map('label', [1, 0]); \
        echo want_int($mapped[0]);";
    let diagnostics = compile_expecting_failure(&dir, src, "app");
    assert!(
        diagnostics.contains("expects int, got string"),
        "array_map() must report the callback's Str return as the element type, got:\n{diagnostics}"
    );
}

/// The callee-return-type matrix for BUG A, checked on VALUES rather than on the joined string,
/// so each callback return shape is verified independently of `implode`'s rendering rules:
/// `bool`, `?int` (null on one path), `float`, `string`, and `int|string` (a union).
///
/// This is the "did the fix special-case bool" control: all five must match reference PHP.
#[test]
fn callback_return_type_matrix_maps_correct_values() {
    let dir = make_test_dir("array_result_matrix");
    let src = "<?php \
        function r_bool(int $n): bool { return $n > 0; } \
        function r_null(int $n): ?int { return $n > 0 ? $n : null; } \
        function r_float(int $n): float { return $n / 2; } \
        function r_str(int $n): string { return 'v' . $n; } \
        function r_union(int $n): int|string { return $n > 0 ? $n : 'z'; } \
        $src = [1, 0]; \
        var_export(array_map('r_bool', $src)); echo \"\\n\"; \
        var_export(array_map('r_null', $src)); echo \"\\n\"; \
        var_export(array_map('r_float', $src)); echo \"\\n\"; \
        var_export(array_map('r_str', $src)); echo \"\\n\"; \
        var_export(array_map('r_union', $src)); echo \"\\n\";";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "array (\n  0 => true,\n  1 => false,\n)\n\
         array (\n  0 => 1,\n  1 => NULL,\n)\n\
         array (\n  0 => 0.5,\n  1 => 0.0,\n)\n\
         array (\n  0 => 'v1',\n  1 => 'v0',\n)\n\
         array (\n  0 => 1,\n  1 => 'z',\n)\n",
        "every callback return shape must map to the reference PHP values"
    );
}

/// The joined rendering for the non-bool members of the matrix. `null` renders as the empty
/// string exactly like `false`, a union renders each member per its own runtime tag, and a float
/// renders through `__rt_ftoa` — the row that exposed the `_concat_off` aliasing bug described in
/// this file's preamble. Both the direct-nesting and bind-to-a-local forms are joined here
/// because only the direct-nesting form used to show the corruption.
#[test]
fn callback_return_type_matrix_joins_like_php() {
    let dir = make_test_dir("array_result_matrix_join");
    let src = "<?php \
        function r_null(int $n): ?int { return $n > 0 ? $n : null; } \
        function r_float(int $n): float { return $n / 2; } \
        function r_str(int $n): string { return 'v' . $n; } \
        function r_union(int $n): int|string { return $n > 0 ? $n : 'z'; } \
        $src = [1, 0]; \
        $nulls = array_map('r_null', $src); \
        $floats = array_map('r_float', $src); \
        $strs = array_map('r_str', $src); \
        $unions = array_map('r_union', $src); \
        echo '[', implode('|', $nulls), \"]\\n\"; \
        echo '[', implode('|', $floats), \"]\\n\"; \
        echo '[', implode('|', $strs), \"]\\n\"; \
        echo '[', implode('|', $unions), \"]\\n\"; \
        echo '[', implode('AB', array_map('r_float', $src)), \"]\\n\";";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out, "[1|]\n[0.5|0]\n[v1|v0]\n[1|z]\n[0.5AB0]\n",
        "the mapped arrays must join exactly as reference PHP does"
    );
}

// ---------------------------------------------------------------------------
// BUG B — `array_keys()` on an array read out of a `mixed`-typed value
// ---------------------------------------------------------------------------

/// REGRESSION ANCHOR for BUG B, reduced to the SMALLEST form with no builtin involved: a plain
/// user function declared `: mixed`. This compiled to `array_keys() argument must be array`
/// before the fix, proving the defect was about `mixed` propagation in general, not builtins.
///
/// Both storage shapes are covered because the runtime tag, not the static type, selects the
/// key-materialization path: a hash yields its insertion-order int-or-string keys, and a list
/// yields positional integers.
#[test]
fn array_keys_accepts_a_plain_mixed_returning_user_function() {
    let dir = make_test_dir("array_keys_mixed_user_fn");
    let src = "<?php \
        function m_hash(): mixed { return ['a' => 1, 'b' => 2, 7 => 'x']; } \
        function m_list(): mixed { return ['p', 'q', 'r']; } \
        function m_empty(): mixed { return []; } \
        var_export(array_keys(m_hash())); echo \"\\n\"; \
        var_export(array_keys(m_list())); echo \"\\n\"; \
        var_export(array_keys(m_empty())); echo \"\\n\";";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "array (\n  0 => 'a',\n  1 => 'b',\n  2 => 7,\n)\n\
         array (\n  0 => 0,\n  1 => 1,\n  2 => 2,\n)\n\
         array (\n)\n",
        "array_keys() over a mixed-typed array must produce the reference keys"
    );
}

/// The originally reported form: an INDEX READ on a builtin's `mixed`-shaped return.
/// `opcache_get_status()` is inferred `mixed` (it returns `false` when the cache is disabled),
/// so `$s['scripts']` lands as `mixed` too. `count()` and `foreach` over that same expression
/// always compiled; `array_keys()` did not.
#[test]
fn array_keys_accepts_an_index_read_on_a_builtin_mixed_result() {
    let dir = make_test_dir("array_keys_opcache_status");
    let src = "<?php \
        $s = opcache_get_status(); \
        $keys = array_keys($s['scripts']); \
        echo count($keys), \"\\n\"; \
        echo basename($keys[0]), \"\\n\";";
    let bin = compile_with_flags(
        &dir,
        src,
        "app",
        &["--ini", "opcache.enable=1", "--ini", "opcache.enable_cli=1"],
    );
    assert_eq!(
        run_binary(&bin),
        "1\napp.php\n",
        "array_keys() must report the cached-script keys of opcache_get_status()['scripts']"
    );
}

/// `json_decode(..., true)` is the other everyday `mixed` producer. Both the object shape
/// (string keys) and the array shape (positional keys) must work.
#[test]
fn array_keys_accepts_a_json_decode_result() {
    let dir = make_test_dir("array_keys_json");
    let src = "<?php \
        var_export(array_keys(json_decode('{\"k1\":1,\"k2\":2}', true))); echo \"\\n\"; \
        var_export(array_keys(json_decode('[10,20,30]', true))); echo \"\\n\";";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "array (\n  0 => 'k1',\n  1 => 'k2',\n)\n\
         array (\n  0 => 0,\n  1 => 1,\n  2 => 2,\n)\n",
        "array_keys() over a json_decode() result must produce the reference keys"
    );
}

/// The two guarded forms the audit flagged as "the `is_array()` guard is ignored": an `if`
/// guard and a ternary. Both are now accepted — not because narrowing learned `is_array`, but
/// because `array_keys()` accepts `mixed` outright, so the guard no longer has to carry the
/// acceptance. The guarded value is still the same `mixed` binding in both branches.
#[test]
fn array_keys_compiles_under_an_is_array_guard_and_a_ternary() {
    let dir = make_test_dir("array_keys_guarded");
    let src = "<?php \
        function m(): mixed { return ['a' => 1, 'b' => 2]; } \
        $v = m(); \
        if (is_array($v)) { var_export(array_keys($v)); echo \"\\n\"; } \
        var_export(is_array($v) ? array_keys($v) : []); echo \"\\n\";";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "array (\n  0 => 'a',\n  1 => 'b',\n)\n\
         array (\n  0 => 'a',\n  1 => 'b',\n)\n",
        "an is_array()-guarded array_keys() over a mixed value must compile and run"
    );
}

/// `array_keys()` over a `mixed` must still be usable as an ordinary array afterwards: counted,
/// iterated, and indexed. This guards the result SHAPE (a fresh indexed array of boxed keys),
/// not just the fact that the call compiles.
#[test]
fn array_keys_result_over_mixed_is_a_usable_array() {
    let dir = make_test_dir("array_keys_mixed_usable");
    let src = "<?php \
        function m(): mixed { return ['a' => 1, 5 => 2, 'c' => 3]; } \
        $keys = array_keys(m()); \
        echo count($keys), \"\\n\"; \
        foreach ($keys as $k) { var_dump($k); }";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "3\nstring(1) \"a\"\nint(5)\nstring(1) \"c\"\n",
        "the keys array produced from a mixed source must be countable and iterable"
    );
}

// ---------------------------------------------------------------------------
// BUG B — negative controls: relaxing the checker must not accept wrong programs
// ---------------------------------------------------------------------------

/// NEGATIVE CONTROL: a STATICALLY non-array argument must still be a COMPILE error. Accepting
/// `mixed` defers the array check to the runtime tag; it does not weaken the static rule.
#[test]
fn array_keys_still_rejects_statically_non_array_arguments() {
    for (index, snippet) in [
        "var_export(array_keys(42));",
        "var_export(array_keys('s'));",
        "var_export(array_keys(1.5));",
        "var_export(array_keys(true));",
        "var_export(array_keys(null));",
        "$x = 42; var_export(array_keys($x));",
        "function f(): int { return 1; } var_export(array_keys(f()));",
        "class C {} var_export(array_keys(new C()));",
    ]
    .iter()
    .enumerate()
    {
        let dir = make_test_dir(&format!("array_keys_negative_{index}"));
        let src = format!("<?php {snippet}");
        let diagnostics = compile_expecting_failure(&dir, &src, "app");
        assert!(
            diagnostics.contains("array_keys() argument must be array"),
            "`{snippet}` must still fail the compile, got:\n{diagnostics}"
        );
    }
}

/// NEGATIVE CONTROL at RUNTIME: a `mixed` that does not hold an array must raise PHP's catchable
/// `TypeError` rather than reading the payload word as a container pointer. The message is pinned
/// per payload kind against reference PHP 8.5.6, which names the literal `false`/`true` for
/// bools.
#[test]
fn array_keys_over_a_non_array_mixed_raises_the_php_type_error() {
    for (index, (body, expected)) in [
        ("return null;", "must be of type array, null given"),
        ("return false;", "must be of type array, false given"),
        ("return true;", "must be of type array, true given"),
        ("return 7;", "must be of type array, int given"),
        ("return 'sv';", "must be of type array, string given"),
        ("return 1.5;", "must be of type array, float given"),
    ]
    .iter()
    .enumerate()
    {
        let dir = make_test_dir(&format!("array_keys_type_error_{index}"));
        let src = format!("<?php function m(): mixed {{ {body} }} var_export(array_keys(m()));");
        let out = run_binary_expecting_fatal(&compile(&dir, &src, "app"));
        assert!(
            out.contains(expected),
            "`{body}` must raise `{expected}`, got:\n{out}"
        );
    }
}

/// The `TypeError` must be CATCHABLE, matching php-src: `array_keys()` throws rather than
/// emitting a bare fatal, so a surrounding `try`/`catch (TypeError)` resumes normally.
#[test]
fn the_array_keys_type_error_is_catchable() {
    let dir = make_test_dir("array_keys_type_error_catch");
    let src = "<?php \
        function m(): mixed { return false; } \
        try { var_export(array_keys(m())); } \
        catch (TypeError $e) { echo 'caught: ', $e->getMessage(), \"\\n\"; } \
        echo \"after\\n\";";
    let out = run_binary(&compile(&dir, src, "app"));
    assert_eq!(
        out,
        "caught: array_keys(): Argument #1 ($array) must be of type array, false given\nafter\n",
        "the array_keys() TypeError must be catchable and resume the program"
    );
}
