//! Purpose:
//! Regression tests for `?string` extern parameters and returns, the `const char*` that C APIs
//! accept or return as NULL (`SDL_CreateRenderer(window, NULL)`, `SDL_GetError()`, `getenv()`).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The C fixture reports exactly what each `char*` parameter received (NULL or the length of
//!   the bytes), returns NULL on request, and echoes its argument pointer back, so both
//!   directions and the copy-before-free ordering of a pass-through return are observable.
//! - Arguments cover a literal string, a literal `null`, and boxed `string|null` values decided
//!   only at runtime (`$argc`), because each takes a different lowering path. A `mixed` scalar
//!   passes its string form; a `mixed` array or object is a fatal error.
//! - `getenv()` checks a real libc NULL return on every target without a fixture, reading a
//!   variable the runner sets instead of one the CI environment may lack.
//! - The fixture is assembled into the test program itself, so no shared library is needed.

use super::*;

/// C fixture whose functions take and return `char*` values that may be NULL.
const NULLABLE_STRING_FIXTURE: &str = r#"
#include <stddef.h>
#include <string.h>

// Reports what a `?string` parameter received: -1 for NULL, otherwise the C string length.
long elephc_test_nullable_len(const char *s) { return s == NULL ? -1 : (long)strlen(s); }

// Returns NULL for 0, a non-empty static string for 1, and an empty static string otherwise.
const char *elephc_test_maybe_name(long which) {
    if (which == 0) {
        return NULL;
    }
    return which == 1 ? "first" : "";
}

// Hands its argument pointer back, NULL included, so the caller must copy before freeing it.
const char *elephc_test_echo(const char *s) { return s; }

// Counts the NULL arguments around a plain `string` parameter that is never NULL.
long elephc_test_count_nulls(const char *a, const char *b, const char *c) {
    return (long)(a == NULL) + (long)(b == NULL) + (long)(c == NULL);
}
"#;

/// PHP declarations of the fixture functions, prepended to every test program.
const NULLABLE_STRING_DECLS: &str = r#"<?php
extern function elephc_test_nullable_len(?string $s): int;
extern function elephc_test_maybe_name(int $which): ?string;
extern function elephc_test_echo(?string $s): ?string;
extern function elephc_test_count_nulls(?string $a, string $b, ?string $c): int;
"#;

/// Compiles the fixture declarations followed by `body` together with the C fixture and returns
/// the program's stdout.
fn run_nullable_string_program(body: &str) -> String {
    compile_and_run_with_c_fixture(
        &format!("{NULLABLE_STRING_DECLS}{body}"),
        NULLABLE_STRING_FIXTURE,
    )
}

/// Compiles the fixture declarations followed by `body` together with the C fixture, optionally
/// with `--gc-stats`, and returns the whole program output (stdout, stderr, exit status), so a
/// test can compare allocation and free counts or inspect a fatal error.
fn run_nullable_string_program_capture(body: &str, gc_stats: bool) -> ProgramOutput {
    let dir = make_cli_test_dir("elephc_test_nullable_cstr");
    let source = format!("{NULLABLE_STRING_DECLS}{body}");
    let (mut assembly, runtime, libraries) =
        compile_source_to_asm_with_options(&source, &dir, 8_388_608, gc_stats, false);
    let fixture_asm = compile_c_fixture_asm(&dir, "fixture", NULLABLE_STRING_FIXTURE, &[]);
    append_c_fixture_asm(&mut assembly, &fixture_asm);
    let output = assemble_and_run_capture(
        &assembly,
        &runtime_obj_for_asm(&runtime),
        &dir,
        &libraries,
        &default_link_paths(),
        &[],
    );
    let _ = fs::remove_dir_all(&dir);
    output
}

/// Verifies a `?string` parameter receives NULL for a literal `null` and a null-terminated copy
/// for a string literal, an empty string, and a string local.
#[test]
fn test_ffi_extern_nullable_string_param_passes_null_or_a_copy() {
    let out = run_nullable_string_program(
        r#"
$name = "hello";
echo elephc_test_nullable_len(null) . "," . elephc_test_nullable_len("abc") . ","
    . elephc_test_nullable_len("") . "," . elephc_test_nullable_len($name);
"#,
    );
    assert_eq!(out, "-1,3,0,5");
}

/// Verifies a boxed `string|null` argument is decided from its runtime value: null passes NULL
/// and a string passes its bytes, whether it comes from a user function, a ternary over `$argc`,
/// or a loop that alternates both.
#[test]
fn test_ffi_extern_nullable_string_param_reads_boxed_values_at_runtime() {
    let out = run_nullable_string_program(
        r#"
function pick(int $i): ?string {
    return $i > 0 ? "x" . $i : null;
}
for ($i = 0; $i < 3; $i++) {
    echo elephc_test_nullable_len(pick($i)) . ";";
}
$missing = $argc > 5 ? "never" : null;
$present = $argc > 0 ? "argv" : null;
echo elephc_test_nullable_len($missing) . ";" . elephc_test_nullable_len($present);
"#,
    );
    assert_eq!(out, "-1;2;2;-1;4");
}

/// Verifies a `mixed` argument reaches a `?string` parameter as its PHP string form: an int as
/// its digits, a float as its PHP text, `true` as "1", `false` as "", a string as itself, and
/// null as NULL.
#[test]
fn test_ffi_extern_nullable_string_param_casts_mixed_scalars() {
    let out = run_nullable_string_program(
        r#"
function len_of(mixed $value): int {
    return elephc_test_nullable_len($value);
}
echo len_of(42) . ";" . len_of(1.5) . ";" . len_of(true) . ";" . len_of(false) . ";"
    . len_of("abc") . ";" . len_of(null);
"#,
    );
    assert_eq!(out, "2;3;1;0;3;-1");
}

/// Verifies an array or an object reaching a `?string` parameter through `mixed` is a fatal
/// error, named after the extern function and parameter, instead of running `__toString()` or
/// an error handler while earlier arguments' C-string copies cannot be released on a throw.
#[test]
fn test_ffi_extern_nullable_string_param_rejects_arrays_and_objects_at_runtime() {
    for payload in ["[1, 2]", "new Label()"] {
        let out = run_nullable_string_program_capture(
            &format!(
                r#"
class Label {{
    public function __toString(): string {{
        return "label";
    }}
}}
function len_of(mixed $value): int {{
    return elephc_test_nullable_len($value);
}}
echo "before;";
echo len_of({payload});
echo "after";
"#
            ),
            false,
        );
        assert!(!out.success, "{payload}: expected a fatal error, got {}", out.stdout);
        assert_eq!(out.stdout, "before;", "{payload}");
        assert!(
            out.stderr.contains(
                "Fatal error: Extern function elephc_test_nullable_len() parameter $s expects ?string, got an array or object"
            ),
            "{payload}: {}",
            out.stderr
        );
    }
}

/// Verifies NULL and non-NULL `?string` arguments mix with a plain `string` parameter in one
/// call, each landing in its own C argument register.
#[test]
fn test_ffi_extern_nullable_string_params_mix_with_string_params() {
    let out = run_nullable_string_program(
        r#"
$none = $argc > 5 ? "never" : null;
echo elephc_test_count_nulls(null, "b", null) . ","
    . elephc_test_count_nulls("a", "b", "c") . ","
    . elephc_test_count_nulls($none, "", "c");
"#,
    );
    assert_eq!(out, "2,0,1");
}

/// Verifies a NULL `?string` return reads as PHP null (for `var_dump`, `===`, and `??`) while
/// non-NULL returns, the empty string included, read as owned strings.
#[test]
fn test_ffi_extern_nullable_string_return_reads_null_as_php_null() {
    let out = run_nullable_string_program(
        r#"
var_dump(elephc_test_maybe_name(0));
var_dump(elephc_test_maybe_name(1));
var_dump(elephc_test_maybe_name(2));
echo (elephc_test_maybe_name(0) ?? "default") . "\n";
echo (elephc_test_maybe_name(0) === null ? "null" : "string") . "\n";
echo (elephc_test_maybe_name(2) === null ? "null" : "string") . "\n";
$kept = elephc_test_maybe_name(1);
echo $kept . "\n";
"#,
    );
    assert_eq!(
        out,
        "NULL\nstring(5) \"first\"\nstring(0) \"\"\ndefault\nnull\nstring\nfirst\n"
    );
}

/// Verifies a C function that returns the very pointer it received reads back the argument's
/// bytes: the return is copied before the call-scoped argument copy is freed, and NULL comes
/// back as PHP null.
#[test]
fn test_ffi_extern_nullable_string_round_trips_through_c() {
    let out = run_nullable_string_program(
        r#"
var_dump(elephc_test_echo(null));
var_dump(elephc_test_echo("round trip"));
$built = str_repeat("ab", 3);
var_dump(elephc_test_echo($built));
$maybe = $argc > 0 ? "boxed" : null;
var_dump(elephc_test_echo($maybe));
"#,
    );
    assert_eq!(
        out,
        "NULL\nstring(10) \"round trip\"\nstring(6) \"ababab\"\nstring(5) \"boxed\"\n"
    );
}

/// Verifies `?string` externs dispatch through `call_user_func()` with a literal callback name
/// and with a runtime callback name, which goes through the generated extern wrapper.
#[test]
fn test_ffi_extern_nullable_string_call_user_func_paths() {
    let out = run_nullable_string_program(
        r#"
var_dump(call_user_func("elephc_test_echo", null));
var_dump(call_user_func("elephc_test_echo", "literal"));
$callback = $argc > 0 ? "elephc_test_echo" : "ELEPHC_TEST_ECHO";
var_dump(call_user_func($callback, null));
var_dump(call_user_func($callback, "dynamic"));
"#,
    );
    assert_eq!(
        out,
        "NULL\nstring(7) \"literal\"\nNULL\nstring(7) \"dynamic\"\n"
    );
}

/// Verifies `?string` calls leak nothing: every argument copy is freed after the call, a NULL
/// argument frees nothing, a boxed string or int argument leaves no intermediate string cast
/// behind (the regression: forwarding a PHP `?string` parameter leaked one string per call),
/// and every boxed return (string or null) is released with its owner. The baseline only
/// declares the externs; the variant makes every kind of call inside functions so their locals
/// are released before the counts are printed.
#[test]
fn test_ffi_extern_nullable_string_calls_free_every_copy_and_box() {
    let baseline = run_nullable_string_program_capture(
        r#"
function forward(?string $s): int {
    return 0;
}
function forward_mixed(mixed $value): int {
    return 0;
}
function run_calls(int $n): void {
}
run_calls($argc);
"#,
        true,
    );
    assert!(baseline.success, "baseline program failed: {}", baseline.stderr);
    let out = run_nullable_string_program_capture(
        r#"
function forward(?string $s): int {
    return elephc_test_nullable_len($s);
}
function forward_mixed(mixed $value): int {
    return elephc_test_nullable_len($value);
}
function run_calls(int $n): void {
    for ($i = 0; $i < 8; $i++) {
        forward($n > 0 ? "repeated" : null);
        forward($n > 5 ? "never" : null);
        forward_mixed($n);
    }
    elephc_test_nullable_len(null);
    elephc_test_nullable_len("abc");
    $missing = $n > 5 ? "never" : null;
    $present = $n > 0 ? "yz" : null;
    elephc_test_nullable_len($missing);
    elephc_test_nullable_len($present);
    elephc_test_count_nulls(null, "b", "c");
    $kept = elephc_test_echo("kept");
    $none = elephc_test_echo(null);
    echo $kept;
    echo $none === null ? "n" : "s";
    elephc_test_maybe_name(1);
    elephc_test_maybe_name(0);
}
run_calls($argc);
"#,
        true,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(
        allocs - baseline_allocs,
        frees - baseline_frees,
        "{}",
        out.stderr
    );
}

/// Verifies libc `getenv()` declared with a `?string` return reads an unset variable's NULL as
/// PHP null and a set variable as its value, on every target without a fixture. The set
/// variable is provided by the runner, so the test does not depend on the CI environment.
#[test]
fn test_ffi_extern_getenv_nullable_return_reads_unset_as_null() {
    let out = compile_and_run_with_env(
        r#"<?php
extern function getenv(string $name): ?string;
var_dump(getenv("ELEPHC_TEST_SURELY_UNSET_VARIABLE_7F3A"));
var_dump(getenv("ELEPHC_TEST_NULLABLE_GETENV"));
"#,
        &[("ELEPHC_TEST_NULLABLE_GETENV", std::ffi::OsStr::new("set-value"))],
    );
    assert_eq!(out, "NULL\nstring(9) \"set-value\"\n");
}
