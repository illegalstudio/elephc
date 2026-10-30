//! Purpose:
//! End-to-end regressions for a call that hands a STRING parameter straight back: the caller
//! released the returned string as its own, which freed the block the argument still used.
//!
//! Called from:
//! - `cargo test --test codegen_tests string_param_return` through Rust's test harness.
//!
//! Key details:
//! - `value_is_borrowed_user_call_result` answered only for plain function calls. A method,
//!   static-method or nullsafe call that returns its parameter was always treated as handing
//!   back an owned result, so `$kept = self::keep($path)` released `$path`'s own block. Those
//!   results are now recorded when the call's arguments are released, where the callee's return
//!   summary and signature are at hand.
//! - A builtin's string view (`dirname($path)`, `trim($s)`) is an `Op::RuntimeCall`, which reads
//!   as an owning temporary, so even a function call took `keep(dirname($path))`'s result as an
//!   owner handed through. The view's pointer is the base of `$path`'s block, so releasing it
//!   freed `$path`. A view is now a caller temporary only when its source is.
//! - An owned source still flows through the result: `keep(trim((string) $mixed))` keeps the
//!   cast copy's lifetime with the returned string, and the heap stays clean.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and finished with a clean heap.
fn assert_matches_with_clean_heap(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a static method and an instance method that return their string parameter leave
/// the caller's local intact.
///
/// Measured before the fix: each `$path` printed as freed bytes while
/// `$kept`, persisted from the same block before the release, still read correctly.
#[test]
fn test_a_method_returning_its_string_parameter_leaves_the_argument_intact() {
    assert_matches_with_clean_heap(
        r#"<?php
final class Paths
{
    private static function keep(string $value): string { return $value; }
    private function hold(string $value): string { return $value; }
    public static function viaStatic(string $base, int $i): string
    {
        $path = $base . '/' . $i;
        $kept = self::keep($path);
        $pad = str_repeat('z', 64);
        return $path . "|" . $kept . "|" . strlen($pad);
    }
    public function viaMethod(string $base, int $i): string
    {
        $path = $base . '/' . $i;
        $kept = $this->hold($path);
        $pad = str_repeat('z', 64);
        return $path . "|" . $kept . "|" . strlen($pad);
    }
}
$paths = new Paths();
for ($i = 0; $i < 2; $i++) {
    echo Paths::viaStatic('/var/s' . str_repeat('p', 3), $i), "\n";
    echo $paths->viaMethod('/var/m' . str_repeat('q', 3), $i), "\n";
}
"#,
        "/var/sppp/0|/var/sppp/0|64\n/var/mqqq/0|/var/mqqq/0|64\n/var/sppp/1|/var/sppp/1|64\n/var/mqqq/1|/var/mqqq/1|64\n",
    );
}

/// Verifies a `dirname()` or `trim()` view passed straight to a call that returns it keeps the
/// string it was taken from, through a static method and a plain function alike.
///
/// Measured before the fix: `$path` was freed by the release of the call's result, so the
/// comparison printed `CORRUPT` and the function arm's directory read as freed bytes.
#[test]
fn test_a_builtin_view_returned_through_a_call_keeps_its_source() {
    assert_matches_with_clean_heap(
        r#"<?php
function keep(string $value): string { return $value; }
final class Store
{
    private static function same(string $value): string { return $value; }
    public static function check(string $base, string $key): string
    {
        $path = $base . '/' . $key;
        $directory = self::same(dirname($path));
        $name = keep(trim($key . ' '));
        $pad = str_repeat('z', 300) . $directory;
        return ($path === $base . '/' . $key ? "intact" : "CORRUPT") . " " . $directory . " " . $name . " " . strlen($pad);
    }
}
function check(string $base, string $key): string
{
    $path = $base . '/' . $key;
    $directory = keep(dirname($path));
    $pad = str_repeat('z', 300) . $directory;
    return ($path === $base . '/' . $key ? "intact" : "CORRUPT") . " " . $directory . " " . strlen($pad);
}
foreach (['ab/cd.bin', 'ef/gh.bin'] as $key) {
    echo Store::check('/var/storage/' . str_repeat('p', 3), $key), "\n";
    echo check('/var/storage/' . str_repeat('q', 3), $key), "\n";
}
"#,
        "intact /var/storage/ppp/ab ab/cd.bin 319\nintact /var/storage/qqq/ab 319\nintact /var/storage/ppp/ef ef/gh.bin 319\nintact /var/storage/qqq/ef 319\n",
    );
}

/// Verifies a view of an OWNED source still hands that source's lifetime through the result:
/// the `(string)` cast copy under `trim()` is released with the returned string, not leaked.
/// This shape passed before the fix too; it pins that the view rule did not stop at the view.
#[test]
fn test_a_view_of_an_owned_cast_copy_still_flows_through_the_result() {
    assert_matches_with_clean_heap(r#"<?php
function keep(string $value): string { return $value; }
final class Text
{
    public static function same(string $value): string { return $value; }
}
function clean(mixed $value): string { return keep(trim((string) $value)) . "|" . Text::same(trim((string) $value)); }
foreach (["b", 42, "c"] as $value) {
    echo clean($value), "\n";
}
"#, "b|b\n42|42\nc|c\n");
}
