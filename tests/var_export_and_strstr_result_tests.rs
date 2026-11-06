//! Purpose:
//! End-to-end tests for two result-type defects around builtins whose PHP return type is not a
//! single concrete type:
//!
//! - BUG 1 (REGRESSION) — `var_export($x, true)` stopped typing as `string`. It is injected as
//!   elephc-PHP (`src/var_export_prelude.rs`) with ONE body serving both PHP modes
//!   (`return $rendered;` and `return null;`), so once `wider_type` stopped resolving `Void`
//!   away, EVERY call inferred `Union([Str, Void])` and the everyday
//!   `function f(): string { return var_export($x, true); }` failed to compile with
//!   "Function 'f' return type expects Str, got Union([Str, Void])".
//! - BUG 2 — `strstr()` never returned `false`. A miss produced the EMPTY STRING, so the
//!   idiomatic `if (strstr($h, $n) !== false)` was ALWAYS TRUE and a miss was indistinguishable
//!   from a hit on an empty suffix. Its third parameter (`$before_needle`) was also declared but
//!   capped away by `max_args: 2`.
//!
//! Called from:
//! - `cargo test --test var_export_and_strstr_result_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   compile a plain executable, run it, and assert stdout — the same harness style as
//!   `array_result_type_tests` / `null_value_codegen_tests` / `opcache_ini_tests`. Host-target
//!   only (macOS aarch64 local).
//! - EVERY expected value here was taken from reference PHP 8.5.6 (`php -d xdebug.mode=off`)
//!   BEFORE the fix was written, including the `var_export` float/array layout and the exact
//!   `strstr` miss value.
//! - REGRESSION ANCHOR (BUG 1): `var_export`'s literal-flag call sites are retargeted by
//!   `crate::name_resolver::expressions::rewrite_var_export_return_flag` at prelude helpers whose
//!   inferred return type matches the mode — `__elephc_var_export_str` (`: string`) for a literal
//!   `true`, `__elephc_var_export_echo` (prints, returns `null`) otherwise. A RUNTIME flag keeps
//!   the two-mode `var_export`, whose `string|null` is the honest PHP type; the mode is then
//!   selected at run time by the prelude's `if`, not guessed at compile time. This mirrors the
//!   flag-aware `check` `print_r` already had.
//! - REGRESSION ANCHOR (BUG 2): `strstr` is typed `string|false` by
//!   `crate::builtins::string::strstr::check` (backend representation: boxed `Mixed`, exactly
//!   what `phpversion($ext)` uses for its `string|false`), and `strings::lower_strstr` boxes BOTH
//!   arms — the selected substring with runtime tag 1, PHP's `false` with tag 3.
//! - LEAK GUARD: `strstr_in_a_loop_leaves_no_live_heap_blocks` compiles with `--heap-debug` and
//!   asserts a clean summary. It is NOT decoration — it is what caught the ownership half of
//!   BUG 2: boxing made the result stop aliasing the haystack, but `RuntimeFnId::Strstr` was
//!   still in the default `MayAliasArguments` bucket, which pinned an owned haystack temporary
//!   for the boxed result's whole lifetime and leaked one block per iteration.
//! - NEGATIVE CONTROL for the `wider_type` change this regression came from:
//!   `unhinted_function_returning_null_still_infers_nullable` pins that an unhinted function that
//!   can return null STILL yields `NULL` (not `""`). Fixing BUG 1 must not undo that.
//! - NOT-IMPLEMENTED PIN: `strstr`'s PHP siblings (`stristr`, `strrchr`, `strpbrk`, `strchr`)
//!   do not exist in elephc at all, so they never shared BUG 2 — they are hard compile errors,
//!   not silently wrong values. `strstr_siblings_are_rejected_rather_than_silently_wrong` pins
//!   that, so whoever adds one has to come here and give it the `string|false` treatment.
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
        "elephc compile unexpectedly SUCCEEDED:\n{raw}"
    );
    elephc_diagnostics(&raw)
}

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than anything
/// elephc emitted: GNU `ld` on Linux reports the static-`getaddrinfo` glibc notes and the
/// `.note.GNU-stack` deprecation, while Apple's linker stays silent. elephc's own lines start
/// with `error`/`warning`, so anchoring on those prefixes isolates its diagnostics — and still
/// surfaces an UNEXPECTED elephc diagnostic, which an allow-list would have hidden.
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

/// Runs a `--heap-debug` binary and returns `(stdout, heap summary lines from stderr)`.
fn run_binary_with_heap_report(bin: &Path) -> (String, String) {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let heap = combined
        .lines()
        .filter(|line| line.contains("HEAP DEBUG"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !heap.is_empty(),
        "no HEAP DEBUG output — the --heap-debug build did not report, so this test proves nothing:\n{combined}"
    );
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        heap,
    )
}

// ---------------------------------------------------------------------------
// BUG 1 — `var_export`'s `$return` flag decides the result type
// ---------------------------------------------------------------------------

/// Verifies `var_export()` of an array of floats does not exhaust the shared concat scratch.
///
/// Floats are the expensive case: `__elephc_var_export_float` finds the shortest round-tripping
/// precision and then rebuilds the digit string through a dozen `substr`/`str_replace`/concat
/// steps. Every one of those lands in the 64 KiB `_concat_buf`, and until the fix NONE of them
/// were ever reclaimed — the statement-boundary reset was skipped for any statement whose span
/// was not from source, which is every statement in an injected prelude.
///
/// The failure was not a clean one. Below about a kilobyte of output the result was simply
/// CORRECT; past it `var_export()` returned a nine-byte string with no diagnostic at all; past
/// that it fatalled with `sprintf(): formatted result exceeds the 65536-byte string buffer`,
/// naming a function the program never called and a size the result came nowhere near.
/// `var_export(opcache_get_configuration())` was the report that surfaced it: 54 directives,
/// two of them floats, 2048 bytes in reference PHP.
///
/// The three sizes are the point. A single size cannot tell "works" from "works below the
/// threshold", and the silent-wrong-answer band sat between the two that were easiest to try.
/// Expected values are reference PHP 8.5's, byte for byte.
#[test]
fn var_export_of_many_floats_does_not_exhaust_the_concat_scratch() {
    let dir = make_test_dir("var_export_float_scratch");
    for (count, expected) in [(24usize, 959usize), (32, 1279), (64, 2559)] {
        let source = format!(
            r#"<?php
$a = [];
for ($i = 0; $i < {count}; $i++) {{ $a["opcache.directive.name.$i"] = 0.005; }}
echo strlen(var_export($a, true)), "\n";
"#
        );
        let bin = compile(&dir, &source, &format!("floats{count}"));
        assert_eq!(
            run_binary(&bin).trim(),
            expected.to_string(),
            "var_export of {count} floats diverged from reference PHP"
        );
    }

    // The shape that was reported: a mixed array whose floats sit among other types.
    let source = r#"<?php
$a = [];
for ($i = 0; $i < 32; $i++) {
    $k = "opcache.directive.name.$i";
    $a[$k] = ($i % 2) ? true : 0.005;
}
echo strlen(var_export($a, true)), "\n";
"#;
    let bin = compile(&dir, source, "mixed32");
    assert_eq!(run_binary(&bin).trim(), "1263");
}

/// THE REGRESSION ANCHOR, verbatim: `var_export($x, true)` used inside a `: string` function.
///
/// Reference PHP 8.5.6 (`php -d xdebug.mode=off`) prints `42`. elephc rejected the program with
/// `error[1:7]: Function 'f' return type expects Str, got Union([Str, Void])` — the two-mode
/// prelude body's inferred `string|null` leaking into a call site whose flag is a literal.
#[test]
fn var_export_with_literal_true_returns_a_string() {
    let dir = make_test_dir("var_export_literal_true");
    let src = "<?php function f(): string { return var_export(42, true); } echo f(), \"\\n\";";
    let bin = compile(&dir, src, "literal_true");
    assert_eq!(run_binary(&bin), "42\n");
}

/// Return mode renders every scalar and the array form exactly as reference PHP does, and the
/// result is a `string` in each case (concatenated into an `echo` list, which a `Union` result
/// would still allow — the `: string` anchor above is what pins the TYPE).
///
/// Reference PHP 8.5.6 for this exact program:
/// `42` / `'it\'s'` / `1.5` / `true` / `NULL` / the indented `array ( … )` block.
#[test]
fn var_export_return_mode_renders_like_reference_php() {
    let dir = make_test_dir("var_export_render");
    let src = "<?php \
        echo var_export(42, true), \"\\n\"; \
        echo var_export(\"it's\", true), \"\\n\"; \
        echo var_export(1.5, true), \"\\n\"; \
        echo var_export(true, true), \"\\n\"; \
        echo var_export(null, true), \"\\n\"; \
        echo var_export([1, 'k' => 2], true), \"\\n\";";
    let bin = compile(&dir, src, "render");
    assert_eq!(
        run_binary(&bin),
        "42\n'it\\'s'\n1.5\ntrue\nNULL\narray (\n  0 => 1,\n  'k' => 2,\n)\n"
    );
}

/// Echo mode PRINTS and returns `null`, for both the omitted and the literal-`false` flag.
///
/// Reference PHP 8.5.6 prints `42|NULL` then `'s'|NULL` for this program: the value reaches
/// stdout and the captured result is `NULL`, NOT the rendered text and NOT `""`.
#[test]
fn var_export_echo_mode_prints_and_returns_null() {
    let dir = make_test_dir("var_export_echo_mode");
    let src = "<?php \
        $r = var_export(42); echo '|'; var_dump($r); \
        $r2 = var_export(\"s\", false); echo '|'; var_dump($r2);";
    let bin = compile(&dir, src, "echo_mode");
    assert_eq!(run_binary(&bin), "42|NULL\n's'|NULL\n");
}

/// A RUNTIME `$return` flag keeps the honest `string|null` type and selects the mode at RUN TIME.
///
/// Reference PHP 8.5.6 prints `string(3) "'A'"` for the truthy flag, then `'B'` to stdout followed
/// by `NULL` for the falsy one. Guessing either mode at compile time would get one of these rows
/// wrong; this is the row `print_r` answers with `Mixed` for the same reason.
#[test]
fn var_export_with_a_runtime_flag_selects_the_mode_at_run_time() {
    let dir = make_test_dir("var_export_runtime_flag");
    let src = "<?php \
        $on = strlen(\"ab\") === 2; \
        $off = strlen(\"ab\") === 3; \
        var_dump(var_export(\"A\", $on)); \
        echo '|'; \
        var_dump(var_export(\"B\", $off));";
    let bin = compile(&dir, src, "runtime_flag");
    assert_eq!(run_binary(&bin), "string(3) \"'A'\"\n|'B'NULL\n");
}

/// The PHP 8 named-argument spelling gets the same flag-aware treatment as the positional one.
///
/// Reference PHP 8.5.6: `var_export(value: 7, return: true)` is `string(1) "7"`, and
/// `var_export(value: 8)` prints `8` and yields `NULL`.
#[test]
fn var_export_named_arguments_pick_the_same_mode() {
    let dir = make_test_dir("var_export_named_args");
    let src = "<?php \
        var_dump(var_export(value: 7, return: true)); \
        $r = var_export(value: 8); echo '|'; var_dump($r);";
    let bin = compile(&dir, src, "named_args");
    assert_eq!(run_binary(&bin), "string(1) \"7\"\n8|NULL\n");
}

/// Echo mode's `null` result still satisfies a `?string` return hint, so the retargeting did not
/// trade one over-narrow type for another.
///
/// Reference PHP 8.5.6 prints `|` (the marker), then `1` (echoed from inside `h`), then `NULL`
/// (the returned value) — `|1NULL`.
#[test]
fn var_export_echo_mode_result_satisfies_a_nullable_string_hint() {
    let dir = make_test_dir("var_export_nullable_hint");
    let src = "<?php \
        function h(mixed $v): ?string { return var_export($v, false); } \
        echo '|'; var_dump(h(1));";
    let bin = compile(&dir, src, "nullable_hint");
    assert_eq!(run_binary(&bin), "|1NULL\n");
}

/// NEGATIVE CONTROL: a user-declared `var_export` is NOT hijacked by the rewrite.
///
/// The rewrite's guard is the presence of the prelude's internal `__elephc_var_export_str`, which
/// `inject_if_used` declares ONLY when it injects — and it declines to inject when the program
/// declares its own `var_export`. Note this program is elephc-specific: reference PHP 8.5.6
/// refuses it outright with "Cannot redeclare function var_export()". elephc deliberately lets a
/// user definition win (see `crate::var_export_prelude`), and that behavior is unchanged here.
#[test]
fn user_declared_var_export_is_not_retargeted() {
    let dir = make_test_dir("var_export_user_declared");
    let src = "<?php \
        function var_export(mixed $v, bool $r = false): string { return \"MINE:\" . $v; } \
        echo var_export(5, true), \"\\n\"; \
        echo var_export(6), \"\\n\";";
    let bin = compile(&dir, src, "user_declared");
    assert_eq!(run_binary(&bin), "MINE:5\nMINE:6\n");
}

/// NEGATIVE CONTROL: a `...$args` SPREAD hides both the argument count and the `$return` value, so
/// the call must keep the real two-mode `var_export` and resolve its mode at run time.
///
/// Reference PHP 8.5.6 for `$args = [42, true]; var_dump(var_export(...$args));` is
/// `string(2) "42"` — return mode, NOTHING printed. A rewrite that read the spread as the
/// `$value` positional printed `42` and returned `NULL` instead.
#[test]
fn var_export_with_a_spread_argument_list_keeps_the_runtime_path() {
    let dir = make_test_dir("var_export_spread");
    let src = "<?php $args = [42, true]; var_dump(var_export(...$args));";
    let bin = compile(&dir, src, "spread");
    assert_eq!(run_binary(&bin), "string(2) \"42\"\n");
}

/// NEGATIVE CONTROL: a NAMESPACED `App\var_export` is a different function and is not retargeted
/// either, even though its last segment reads `var_export`.
///
/// The rewrite matches the RESOLVED name and requires it to carry no namespace qualifier —
/// `rewrite_date_procedural_alias`'s last-segment matching would have hijacked this one.
/// Reference PHP 8.5.6 prints `NS:5` then `NS:6` for this program.
#[test]
fn namespaced_var_export_is_not_retargeted() {
    let dir = make_test_dir("var_export_namespaced");
    let src = "<?php \n\
        namespace App;\n\
        function var_export(mixed $v, bool $r = false): string { return \"NS:\" . $v . ($r ? \"\" : \"\"); }\n\
        echo var_export(5, true), \"\\n\";\n\
        echo var_export(6), \"\\n\";\n";
    let bin = compile(&dir, src, "namespaced");
    assert_eq!(run_binary(&bin), "NS:5\nNS:6\n");
}

/// NEGATIVE CONTROL for the `wider_type` change that caused BUG 1: an UNHINTED function that can
/// return null must still infer a nullable type, so `return null` yields PHP `NULL` rather than
/// being coerced into the other arm's zero value (`""`).
///
/// Reference PHP 8.5.6: `string(1) "s"` then `NULL`. This is the behavior `null_value_codegen_tests`
/// pins; fixing `var_export` must not resolve `Void` away again to get there.
#[test]
fn unhinted_function_returning_null_still_infers_nullable() {
    let dir = make_test_dir("var_export_wider_type_control");
    let src = "<?php \
        function g(mixed $x) { if ($x) { return \"s\"; } return null; } \
        var_dump(g(true)); \
        var_dump(g(false));";
    let bin = compile(&dir, src, "wider_type_control");
    assert_eq!(run_binary(&bin), "string(1) \"s\"\nNULL\n");
}

// ---------------------------------------------------------------------------
// BUG 2 — `strstr()` returns `string|false`
// ---------------------------------------------------------------------------

/// THE REGRESSION ANCHOR, verbatim: a `strstr()` miss is `false`, not `""`.
///
/// Reference PHP 8.5.6 prints `bool(false)`; elephc printed `string(0) ""`.
#[test]
fn strstr_miss_returns_false() {
    let dir = make_test_dir("strstr_miss");
    let src = "<?php $r = strstr(\"hello\", \"zzz\"); var_dump($r);";
    let bin = compile(&dir, src, "miss");
    assert_eq!(run_binary(&bin), "bool(false)\n");
}

/// A hit still returns the suffix starting at the needle, including the empty-needle case PHP 8
/// accepts (which returns the WHOLE haystack, not `false`).
///
/// Reference PHP 8.5.6: `string(3) "llo"` / `string(5) "hello"` / `string(16) "@example.com"`-ish
/// rows as asserted below.
#[test]
fn strstr_hit_returns_the_suffix() {
    let dir = make_test_dir("strstr_hit");
    let src = "<?php \
        var_dump(strstr(\"hello\", \"ll\")); \
        var_dump(strstr(\"hello\", \"\")); \
        var_dump(strstr(\"hello\", \"h\")); \
        var_dump(strstr(\"hello\", \"o\"));";
    let bin = compile(&dir, src, "hit");
    assert_eq!(
        run_binary(&bin),
        "string(3) \"llo\"\nstring(5) \"hello\"\nstring(5) \"hello\"\nstring(1) \"o\"\n"
    );
}

/// THE POINT OF THE BUG: the idiomatic `!== false` / `=== false` guards now discriminate.
///
/// Reference PHP 8.5.6 prints `F`,`T`,`T`,`F` for these four rows. Before the fix a miss returned
/// `""`, so `strstr(...) !== false` was ALWAYS TRUE and `=== false` ALWAYS FALSE — silently wrong
/// for a very common check.
#[test]
fn strstr_false_comparison_idioms_discriminate() {
    let dir = make_test_dir("strstr_idioms");
    let src = "<?php \
        echo strstr(\"hello\", \"zzz\") !== false ? \"T\" : \"F\", \"\\n\"; \
        echo strstr(\"hello\", \"ll\") !== false ? \"T\" : \"F\", \"\\n\"; \
        echo strstr(\"hello\", \"zzz\") === false ? \"T\" : \"F\", \"\\n\"; \
        echo strstr(\"hello\", \"ll\") === false ? \"T\" : \"F\", \"\\n\";";
    let bin = compile(&dir, src, "idioms");
    assert_eq!(run_binary(&bin), "F\nT\nT\nF\n");
}

/// The third parameter (`$before_needle`) returns the part BEFORE the needle, and a miss is still
/// `false` in that mode — it was declared but capped away by `max_args: 2`.
///
/// Reference PHP 8.5.6: `string(2) "he"` / `bool(false)` / `string(4) "user"`.
#[test]
fn strstr_before_needle_returns_the_prefix() {
    let dir = make_test_dir("strstr_before_needle");
    let src = "<?php \
        var_dump(strstr(\"hello\", \"ll\", true)); \
        var_dump(strstr(\"hello\", \"zzz\", true)); \
        var_dump(strstr(\"user@example.com\", \"@\", true)); \
        var_dump(strstr(\"hello\", \"ll\", false));";
    let bin = compile(&dir, src, "before_needle");
    assert_eq!(
        run_binary(&bin),
        "string(2) \"he\"\nbool(false)\nstring(4) \"user\"\nstring(3) \"llo\"\n"
    );
}

/// A RUNTIME `$before_needle` flag selects prefix vs suffix at run time, including through a
/// `mixed`-typed truthy value (PHP takes any truthy expression there, not just a `bool`).
///
/// Reference PHP 8.5.6 for these rows: `string(4) "path"` / `string(9) "/to/file"`-shaped suffix /
/// `bool(false)` / `string(1) "a"` for the `"x"` truthy string / `string(4) "XbXc"` for `""`.
#[test]
fn strstr_runtime_before_needle_flag_selects_at_run_time() {
    let dir = make_test_dir("strstr_runtime_flag");
    let src = "<?php \
        $h = \"path/to/file\"; \
        $before = strlen(\"ab\") === 2; \
        $after = strlen(\"ab\") === 3; \
        var_dump(strstr($h, \"/\", $before)); \
        var_dump(strstr($h, \"/\", $after)); \
        var_dump(strstr($h, \"zz\", $before)); \
        foreach ([\"x\", \"\"] as $f) { var_dump(strstr(\"aXbXc\", \"X\", $f)); }";
    let bin = compile(&dir, src, "runtime_flag");
    assert_eq!(
        run_binary(&bin),
        "string(4) \"path\"\nstring(8) \"/to/file\"\nbool(false)\nstring(1) \"a\"\nstring(4) \"XbXc\"\n"
    );
}

/// A `string|false` result still behaves like a string in string contexts: `echo` of the `false`
/// miss prints nothing, concatenation works, and `strlen()` reads the boxed payload.
///
/// Reference PHP 8.5.6: `[]` for the miss, `cat=-b`, `3`.
#[test]
fn strstr_result_still_works_in_string_contexts() {
    let dir = make_test_dir("strstr_string_context");
    let src = "<?php \
        echo '[', strstr(\"hello\", \"zzz\"), \"]\\n\"; \
        echo \"cat=\" . strstr(\"a-b\", \"-\") . \"\\n\"; \
        echo strlen(strstr(\"hello\", \"ll\")), \"\\n\";";
    let bin = compile(&dir, src, "string_context");
    assert_eq!(run_binary(&bin), "[]\ncat=-b\n3\n");
}

/// The boxed `string|false` result must survive its haystack: the substring is a PERSISTED copy
/// made by `__rt_mixed_from_value`, not a borrowed slice into a temporary that has already died.
///
/// Reference PHP 8.5.6 prints the same `-0-suffix|-1-suffix|-2-suffix|` line.
#[test]
fn strstr_result_outlives_a_temporary_haystack() {
    let dir = make_test_dir("strstr_outlives_haystack");
    let src = "<?php \
        function pick(int $i): string { return \"prefix-\" . $i . \"-suffix\"; } \
        $keep = []; \
        for ($i = 0; $i < 3; $i++) { $keep[] = strstr(pick($i), \"-\"); } \
        foreach ($keep as $v) { echo $v, '|'; } \
        echo \"\\n\";";
    let bin = compile(&dir, src, "outlives");
    assert_eq!(run_binary(&bin), "-0-suffix|-1-suffix|-2-suffix|\n");
}

/// LEAK GUARD, and the reason `RuntimeFnId::Strstr` had to move out of `MayAliasArguments`.
///
/// Boxing the result made it stop aliasing the haystack, but the ownership contract still said it
/// might, which pinned each owned haystack/needle temporary for the boxed result's whole lifetime
/// — one leaked block per iteration for `strstr($h, $cond ? "a" : "b")` in a loop. `Strpos`, whose
/// `int|false` is boxed the same way, was already `Fresh`; `Strstr` now is too.
///
/// This asserts a CLEAN heap on the host target only. Per this repo's hard-won lesson, a green
/// macOS/aarch64 heap probe is NOT proof of correctness — the linux-x86_64 `--heap-debug` guard in
/// CI is the authority — but a REGRESSION here is still a real signal.
#[test]
fn strstr_in_a_loop_leaves_no_live_heap_blocks() {
    let dir = make_test_dir("strstr_heap");
    let src = "<?php \
        $n = 0; \
        for ($i = 0; $i < 100; $i++) { \
            $r = strstr(\"hello world\", $i % 2 === 0 ? \"wor\" : \"zzz\"); \
            if ($r !== false) { $n = $n + 1; } \
        } \
        for ($i = 0; $i < 20; $i++) { $r = strstr(\"a/b\", \"/\", $i % 2 === 0); } \
        echo $n, \"\\n\";";
    let bin = compile_with_flags(&dir, src, "heap", &["--heap-debug"]);
    let (stdout, heap) = run_binary_with_heap_report(&bin);
    assert_eq!(stdout, "50\n");
    assert!(
        heap.contains("leak summary: clean"),
        "strstr leaked heap blocks:\n{heap}"
    );
}

/// Arity: the third parameter is accepted, a fourth is not, and one argument is still too few.
///
/// Reference PHP 8.5.6 raises "strstr() expects at least 2 arguments, 1 given" and "expects at
/// most 3 arguments, 4 given" — the same 2..=3 window elephc reports as "takes 2 or 3 arguments".
#[test]
fn strstr_accepts_two_or_three_arguments() {
    let dir = make_test_dir("strstr_arity");
    let too_few = compile_expecting_failure(&dir, "<?php strstr(\"abc\");", "too_few");
    assert!(
        too_few.contains("strstr() takes 2 or 3 arguments"),
        "unexpected diagnostics for a 1-argument call: {too_few}"
    );
    let too_many = compile_expecting_failure(
        &dir,
        "<?php strstr(\"abc\", \"b\", true, 1);",
        "too_many",
    );
    assert!(
        too_many.contains("strstr() takes 2 or 3 arguments"),
        "unexpected diagnostics for a 4-argument call: {too_many}"
    );
}

/// NOT-IMPLEMENTED PIN: `strstr`'s PHP siblings never shared BUG 2 because elephc does not
/// implement them at all — each is a hard "Undefined function" compile error rather than a
/// silently wrong value.
///
/// Reference PHP 8.5.6 for a miss on each: `stristr("hello","ZZZ")`, `strrchr("hello","z")`,
/// `strpbrk("hello","zq")` and `strchr("hello","zzz")` all return `bool(false)`, exactly like
/// `strstr`. Whoever implements one must give it the same `string|false` treatment — this test
/// failing is the reminder to come back here.
#[test]
fn strstr_siblings_are_rejected_rather_than_silently_wrong() {
    let dir = make_test_dir("strstr_siblings");
    for sibling in ["stristr", "strrchr", "strpbrk", "strchr"] {
        let src = format!("<?php var_dump({sibling}(\"hello\", \"z\"));");
        let diagnostics = compile_expecting_failure(&dir, &src, sibling);
        assert!(
            diagnostics.contains(&format!("Undefined function: {sibling}")),
            "{sibling} no longer reports as undefined — implement it as string|false and pin it here instead: {diagnostics}"
        );
    }
}

// ---------------------------------------------------------------------------
// BUG 3 (issue B17) — `var_export()` of an OBJECT returned the empty string.
//
// The prelude's renderer had arms for int/float/bool/null/string/array and a
// bare `return '';` for everything else, so `var_export(new Foo)` printed
// NOTHING while PHP prints a parsable `\Foo::__set_state(array( … ))`.
//
// PHP's object layout is NOT the array layout, which is the whole reason this
// needed its own branch rather than a reuse of the array one: an ENTRY KEY sits
// at `indent + 3` (arrays use `indent + 2`) while the value and the closing line
// keep the array indents. Every expectation below is reference PHP 8.4.20's
// output for the same program, taken with `php -d xdebug.mode=off`.
// ---------------------------------------------------------------------------

/// The headline repro: a plain class renders `\Class::__set_state(array( … ))`
/// with three-space entry keys, a trailing comma on every entry, and `))`.
#[test]
fn var_export_of_a_plain_object_renders_set_state() {
    let dir = make_test_dir("var_export_object");
    let src = "<?php class Foo { public $a = 1; public $b = 'x'; public $c = 1.5; public $d = true; } \
        var_export(new Foo); echo \"\\n\";";
    let bin = compile(&dir, src, "object_set_state");
    assert_eq!(
        run_binary(&bin),
        "\\Foo::__set_state(array(\n   'a' => 1,\n   'b' => 'x',\n   'c' => 1.5,\n   'd' => true,\n))\n"
    );
}

/// `stdClass` is the ONE class PHP exports as a cast instead of `__set_state`,
/// because it has no such method. An empty one still writes both lines.
#[test]
fn var_export_of_stdclass_renders_an_object_cast() {
    let dir = make_test_dir("var_export_stdclass");
    let src = "<?php echo var_export(new stdClass, true), \"\\n\";";
    let bin = compile(&dir, src, "stdclass_cast");
    assert_eq!(run_binary(&bin), "(object) array(\n)\n");
}

/// A NESTED object and a NESTED array inside an object. Both are preceded by a
/// newline and the `indent + 2` pad, and both close at `indent + 2` — the layout
/// PHP produces and the reason the object branch cannot share the array padding.
#[test]
fn var_export_of_nested_containers_matches_php_indentation() {
    let dir = make_test_dir("var_export_object_nested");
    let src = "<?php \
        class Foo { public $a = 1; } \
        class Bar { public ?Foo $o = null; public $arr = [1, 2]; \
            function __construct() { $this->o = new Foo; } } \
        var_export(new Bar); echo \"\\n\";";
    let bin = compile(&dir, src, "object_nested");
    assert_eq!(
        run_binary(&bin),
        concat!(
            "\\Bar::__set_state(array(\n",
            "   'o' => \n",
            "  \\Foo::__set_state(array(\n",
            "     'a' => 1,\n",
            "  )),\n",
            "   'arr' => \n",
            "  array (\n",
            "    0 => 1,\n",
            "    1 => 2,\n",
            "  ),\n",
            "))\n",
        )
    );
}

/// An object inside an ARRAY: the array branch must give an object value the same
/// `\n` + pad prefix it already gave a nested array, or the `\Foo::` would land on
/// the key line.
#[test]
fn var_export_of_objects_inside_an_array() {
    let dir = make_test_dir("var_export_object_in_array");
    let src = "<?php class Foo { public $a = 1; } \
        var_export([new Foo, 'k' => new stdClass]); echo \"\\n\";";
    let bin = compile(&dir, src, "object_in_array");
    assert_eq!(
        run_binary(&bin),
        concat!(
            "array (\n",
            "  0 => \n",
            "  \\Foo::__set_state(array(\n",
            "     'a' => 1,\n",
            "  )),\n",
            "  'k' => \n",
            "  (object) array(\n",
            "  ),\n",
            ")\n",
        )
    );
}

/// An ENUM case exports as the parsable constant expression `\Enum::Case`, with
/// no body at all, for pure and backed enums alike.
#[test]
fn var_export_of_enum_cases_renders_the_case_constant() {
    let dir = make_test_dir("var_export_enum");
    let src = "<?php \
        enum Status { case Active; } \
        enum Suit: string { case Hearts = 'H'; } \
        enum Lvl: int { case Low = 3; } \
        echo var_export(Status::Active, true), '|', var_export(Suit::Hearts, true), \
            '|', var_export(Lvl::Low, true), \"\\n\"; \
        var_export(['e' => Status::Active]); echo \"\\n\";";
    let bin = compile(&dir, src, "enum_cases");
    assert_eq!(
        run_binary(&bin),
        concat!(
            "\\Status::Active|\\Suit::Hearts|\\Lvl::Low\n",
            "array (\n",
            "  'e' => \n",
            "  \\Status::Active,\n",
            ")\n",
        )
    );
}

/// `var_export` prints the BARE property name for a protected/private property —
/// no `:protected` / `:C:private` annotation, unlike `print_r` — and OMITS an
/// uninitialized typed property entirely.
#[test]
fn var_export_prints_bare_property_names_and_skips_uninitialized_ones() {
    let dir = make_test_dir("var_export_visibility");
    let src = "<?php \
        class P { public $a = 1; protected $b = 2; private $c = 3; } \
        class U { public int $t; public $d = 1; } \
        var_export(new P); echo \"\\n\"; \
        var_export(new U); echo \"\\n\";";
    let bin = compile(&dir, src, "object_visibility");
    assert_eq!(
        run_binary(&bin),
        concat!(
            "\\P::__set_state(array(\n",
            "   'a' => 1,\n",
            "   'b' => 2,\n",
            "   'c' => 3,\n",
            "))\n",
            "\\U::__set_state(array(\n",
            "   'd' => 1,\n",
            "))\n",
        )
    );
}

/// Return mode over an object: the same bytes handed back as a `string`, with the
/// length pinned so a truncated or over-copied render could not pass on the text
/// alone. Reference PHP 8.4.20 reports 64.
#[test]
fn var_export_object_return_mode_is_a_string_of_the_same_bytes() {
    let dir = make_test_dir("var_export_object_return");
    let src = "<?php class P { public $a = 1; protected $b = 2; private $c = 3; } \
        $s = var_export(new P, true); echo strlen($s), \"\\n\", $s, \"\\n\";";
    let bin = compile(&dir, src, "object_return_mode");
    assert_eq!(
        run_binary(&bin),
        concat!(
            "64\n",
            "\\P::__set_state(array(\n",
            "   'a' => 1,\n",
            "   'b' => 2,\n",
            "   'c' => 3,\n",
            "))\n",
        )
    );
}

/// LEAK GUARD for the object branch: `__elephc_object_prop_value` hands back a
/// FRESHLY boxed Mixed cell for every property, so exporting objects in a loop
/// must end with nothing live. Handing back the property's own cell instead would
/// alias object storage into a caller-released temporary; boxing without the
/// `Fresh` ownership declaration would leak one cell per property per call.
#[test]
fn var_export_object_loop_leaves_no_live_heap_blocks() {
    let dir = make_test_dir("var_export_object_heap");
    let src = "<?php \
        class Foo { public $a = 1; public $b = 'xyz'; public $arr = [1, 2]; } \
        class Bar { public ?Foo $o = null; function __construct() { $this->o = new Foo; } } \
        $total = 0; \
        for ($i = 0; $i < 8; $i++) { $total += strlen(var_export(new Bar, true)); } \
        echo $total, \"\\n\";";
    let bin = compile_with_flags(&dir, src, "object_heap", &["--heap-debug"]);
    let (stdout, heap) = run_binary_with_heap_report(&bin);
    // Reference PHP 8.4.20 renders 167 bytes for one `var_export(new Bar, true)`.
    assert_eq!(stdout, format!("{}\n", 8 * 167));
    assert!(
        heap.contains("leak summary: clean"),
        "var_export of objects leaked heap blocks:\n{heap}"
    );
}

/// LEAK GUARD for the STRING paths, which leaked long before objects existed.
///
/// `__elephc_var_export_escape` used to take `mixed $s` and cast it inside. Passing a `string`
/// value to a `mixed` parameter BOXES it into a fresh Mixed cell that nothing releases, so every
/// exported string value and every exported string KEY leaked one heap block — in every program
/// that called `var_export` on anything containing a string. The helper now takes `string` and
/// each caller casts into a `string` local first, so no box is created at the call boundary.
///
/// This is deliberately separate from the object leak guard: it fails on a plain array and does
/// not need an object at all.
#[test]
fn var_export_string_loop_leaves_no_live_heap_blocks() {
    let dir = make_test_dir("var_export_string_heap");
    let src = "<?php \
        $total = 0; \
        for ($i = 0; $i < 8; $i++) { \
            $total += strlen(var_export('zz', true)); \
            $total += strlen(var_export(['k' => 'zz'], true)); \
        } \
        echo $total, \"\\n\";";
    let bin = compile_with_flags(&dir, src, "string_heap", &["--heap-debug"]);
    let (stdout, heap) = run_binary_with_heap_report(&bin);
    // Reference PHP 8.4.20: `'zz'` is 4 bytes, `array (\n  'k' => 'zz',\n)` is 24.
    assert_eq!(stdout, format!("{}\n", 8 * (4 + 24)));
    assert!(
        heap.contains("leak summary: clean"),
        "var_export of strings leaked heap blocks:\n{heap}"
    );
}
