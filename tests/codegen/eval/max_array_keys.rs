//! Purpose:
//! Regression tests for saturated integer keys in dynamically supplied eval source.
//!
//! Called from:
//! - `tests/codegen/eval.rs` through the codegen integration test harness.
//!
//! Key details:
//! - Source arrives through argv so these cases exercise Magician rather than literal AOT.
//! - Output and clean-heap assertions cover value ordering and partial-insertion owners.

use crate::support::compile_and_run_with_heap_debug_per_argv;

/// Replays opaque eval fragments and checks PHP output, warnings and final heap ownership.
fn assert_dynamic_eval_cases(cases: &[(&str, &str)]) {
    let arguments = cases.iter().map(|(source, _)| vec![*source]).collect::<Vec<_>>();
    let argvs = arguments.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let outputs = compile_and_run_with_heap_debug_per_argv("<?php eval($argv[1]);", &argvs);
    for (output, (source, expected)) in outputs.iter().zip(cases) {
        assert!(output.success, "program failed: {}", output.stderr);
        assert_eq!(output.stdout, *expected);
        assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "source: {source}\n{}", output.stderr);
    }
}

/// Literal values and reference offsets run before an occupied implicit key raises Error.
#[test]
fn test_dynamic_eval_max_array_key_value_effects_and_exceptions() {
    assert_dynamic_eval_cases(&[
        (r#"function mark() { echo "mark:"; return 2; }
try { $a = [PHP_INT_MAX => 1, mark()]; }
catch (Error $error) { echo $error->getMessage(); unset($error); }"#,
         "mark:Cannot add element to the array as the next element is already occupied"),
        (r#"function mark() { echo "mark:"; throw new Exception("value"); }
try { $a = [PHP_INT_MAX => 1, mark()]; }
catch (Throwable $error) { echo get_class($error), ":", $error->getMessage(); unset($error); }"#,
         "mark:Exception:value"),
        (r#"function mark() { echo "mark:"; return 0; }
$values = [2];
try { $a = [PHP_INT_MAX => 1, &$values[mark()]]; }
catch (Error $error) { echo $error->getMessage(), ":"; unset($error); }
echo $values[0];"#,
         "mark:Cannot add element to the array as the next element is already occupied:2"),
        (r#"function mark() { echo "mark:"; return 2; }
$a = [PHP_INT_MAX - 1 => 1, mark()];
echo count($a), ":", $a[PHP_INT_MAX];"#,
         "mark:2:2"),
    ]);
}

/// Direct and runtime-selected pushes fill PHP_INT_MAX once and retain partial updates.
#[test]
fn test_dynamic_eval_max_array_key_push_fills_last_two_keys() {
    assert_dynamic_eval_cases(&[(r#"
$source = [PHP_INT_MAX - 2 => 1];
$a = $source;
echo array_push($a, 2, 3), ":", $a[PHP_INT_MAX - 1], ":", $a[PHP_INT_MAX], ":", count($source), ":";
$b = [PHP_INT_MAX - 2 => 1];
$push = "array_push";
echo $push($b, 2, 3), ":", $b[PHP_INT_MAX], ":";
$c = [PHP_INT_MAX - 2 => 1];
try { array_push($c, 2, 3, 4); }
catch (Error $error) { echo $error->getMessage(), ":"; unset($error); }
echo count($c), ":", $c[PHP_INT_MAX - 1], ":", $c[PHP_INT_MAX];
"#, "3:2:3:1:3:3:Cannot add element to the array as the next element is already occupied:3:2:3")]);
}

/// Failed literal insertion releases its new value and the partial array on every iteration.
#[test]
fn test_dynamic_eval_max_array_key_literal_releases_object_values() {
    let source = r#"
class Payload {
    public function __construct(public int $id) {}
    public function __destruct() { echo "drop", $this->id, "\n"; }
}
function mark() { echo "mark\n"; return new Payload(2); }
for ($i = 0; $i < 40; $i++) {
    try { $a = [PHP_INT_MAX => new Payload(1), mark()]; }
    catch (Error $error) { echo "caught\n"; unset($error); }
    $pressure = str_repeat("x", 300);
    unset($pressure);
}
echo "done\n";
"#;
    let expected = "mark\ndrop2\ndrop1\ncaught\n".repeat(40) + "done\n";
    assert_dynamic_eval_cases(&[(source, &expected)]);
}

/// Inserted objects outlive argument variables while rejected values release independently.
#[test]
fn test_dynamic_eval_max_array_key_push_retains_inserted_objects() {
    assert_dynamic_eval_cases(&[(r#"
class Payload {
    public function __construct(public int $id) {}
    public function __destruct() { echo "drop", $this->id, "\n"; }
}
$first = new Payload(1);
$second = new Payload(2);
$third = new Payload(3);
$rejected = new Payload(4);
$a = [PHP_INT_MAX - 2 => $first];
try { array_push($a, $second, $third, $rejected); }
catch (Error $error) { echo $error->getMessage(), "\n"; unset($error); }
unset($first, $second, $third, $rejected);
echo "kept:", $a[PHP_INT_MAX - 2]->id, $a[PHP_INT_MAX - 1]->id, $a[PHP_INT_MAX]->id, "\n";
unset($a);
echo "done\n";
"#, "Cannot add element to the array as the next element is already occupied\ndrop4\nkept:123\ndrop1\ndrop2\ndrop3\ndone\n")]);
}
