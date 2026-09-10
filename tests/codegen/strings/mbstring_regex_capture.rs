//! Purpose:
//! Tests PHP capture calls through the AOT mbstring reference adapter.
//!
//! Called from:
//! - The codegen integration test harness with the managed Oniguruma fixture.
//!
//! Key details:
//! - Output references preserve caller identity and are never copied as input values.
//! - These tests exercise real PHP calls without replacing generated function bodies.

use crate::support::*;

/// Searches without an output variable and honors case-insensitive and namespace builtin lookup.
#[test]
fn test_mbstring_regex_capture_optional_output() {
    let source = r#"<?php
namespace CaptureCalls;
var_dump(Mb_ErEg("bc", "abc"), mb_ereg("bc", "aBC"), mb_eregi("bc", "aBC"));
var_dump(\mb_ereg("猫", "黒猫"), \mb_eregi("ä", "Ä"));
"#;
    assert_eq!(compile_and_run(source), "bool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\n");
}

/// Publishes named captures to an undefined local and empties it after an unsuccessful search.
#[test]
fn test_mbstring_regex_capture_undefined_local() {
    let source = r#"<?php
var_dump(mb_ereg("(?<key>a)", "za", $matches));
echo count($matches), ":", $matches[0], ":", $matches[1], ":", $matches["key"], "\n";
var_dump(mb_ereg("x", "a", $matches));
echo count($matches), "\n";
var_dump(mb_eregi("(a)(b)?", "A", $matches));
echo count($matches), ":", $matches[0], ":", $matches[1], "\n";
var_dump($matches[2]);
"#;
    assert_eq!(compile_and_run(source), "bool(true)\n3:a:a:a\nbool(false)\n0\nbool(true)\n3:A:A\nbool(false)\n");
}

/// Leaves the original scalar intact when argument validation fails before capture initialization.
#[test]
fn test_mbstring_regex_capture_validation_preserves_local() {
    let source = r#"<?php
$matches = 7;
try { mb_ereg("", "a", $matches); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
var_dump($matches);
var_dump(mb_eregi("a", "A", $matches));
echo $matches[0], "\n";
"#;
    assert_eq!(compile_and_run(source), "mb_ereg(): Argument #1 ($pattern) must not be empty\nint(7)\nbool(true)\nA\n");
}

/// Evaluates named inputs in source order while preserving the original output reference.
#[test]
fn test_mbstring_regex_capture_named_arguments() {
    let source = r#"<?php
function input(string $label, string $value): string { echo $label, "\n"; return $value; }
var_dump(mb_ereg(matches: $matches, string: input("subject", "za"), pattern: input("pattern", "(?<key>a)")));
echo $matches["key"], "\n";
"#;
    assert_eq!(compile_and_run(source), "subject\npattern\nbool(true)\na\n");
}

/// Preserves alias identity while an old-value destructor observes and changes the output cell.
#[test]
fn test_mbstring_regex_capture_alias_destructor() {
    let source = r#"<?php
class CaptureOldValue {
    public function __construct(public Closure $observe, public bool $fail) {}
    public function __destruct() {
        ($this->observe)();
        mb_regex_set_options("i");
        if ($this->fail) { throw new RuntimeException("capture destructor"); }
    }
}
function run_capture(mixed $matches, bool $fail): void {
    $alias =& $matches;
    $observe = function() use (&$alias): void {
        echo $alias === null ? "visible:null\n" : "visible:other\n";
        $alias = ["during" => "callback"];
    };
    $matches = new CaptureOldValue($observe, $fail);
    mb_regex_set_options("r");
    try { var_dump(mb_ereg("(?<key>a)", "A", $alias)); }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo count($matches), ":", $alias[0], ":", $matches["key"], ":", $alias[1], "\n";
}
run_capture(null, false);
run_capture(null, true);
echo "done\n";
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, concat!(
        "visible:null\nbool(true)\n3:A:A:A\n",
        "visible:null\ncaught:capture destructor\n3:A:A:A\ndone\n"));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Promotes capture storage on both predecessors even when an earlier alias exists only on one branch.
#[test]
fn test_mbstring_regex_capture_conditional_reference() {
    let source = r#"<?php
function capture_branch(mixed $matches, bool $promote): void {
    if ($promote) { $alias =& $matches; }
    var_dump(mb_ereg("a", "a", $matches));
    echo $matches[0], "\n";
}
capture_branch(null, false);
capture_branch(null, true);
"#;
    assert_eq!(compile_and_run(source), "bool(true)\na\nbool(true)\na\n");
}

/// Preserves the output identity while a later input expression replaces its previous value.
#[test]
fn test_mbstring_regex_capture_named_value_assignment() {
    let source = r#"<?php
function assign_capture(mixed $matches): void {
    $alias =& $matches;
    var_dump(mb_ereg(matches: $matches, pattern: "a", string: ($matches = "a")));
    echo $matches[0], ":", $alias[0], "\n";
}
assign_capture(null);
"#;
    assert_eq!(compile_and_run(source), "bool(true)\na:a\n");
}

/// Refuses untracked parameter and property alias storage before emitting an unsafe wrapper pointer.
#[test]
fn test_mbstring_regex_capture_unsupported_reference_forms() {
    for source in [
        "<?php function capture(mixed &$matches): bool { return mb_ereg('a', 'a', $matches); } function run(mixed $matches): void { var_dump(capture($matches)); } run(null);",
        "<?php class CaptureBox { public mixed $value = null; } $matches = null; $alias =& $matches; $box = new CaptureBox(); $alias =& $box->value; var_dump(mb_ereg('a', 'a', $alias));",
    ] {
        let error = compile_source_expect_backend_error(source);
        assert!(error.contains("mbstring output requires a managed Mixed local reference"), "{error}");
    }
}
