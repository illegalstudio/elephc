//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of indexed array array search, merge, and union builtins, including search, search not found is strict false, and search assigned not found is strict false.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `array_search` returns the 0-based integer index of the first match.
#[test]
fn test_array_search() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(20, $a);
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies `array_search` returns strict `false` (===) when the value is absent.
#[test]
fn test_array_search_not_found_is_strict_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(99, $a) === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

/// Regression: assigning the result of `array_search` to a variable before comparing
/// must still yield strict `false`, not a falsy zero or empty string.
#[test]
fn test_array_search_assigned_not_found_is_strict_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$result = array_search(99, $a);
echo $result === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

/// Verifies that `array_search` returns index `0` (not `false`) when the target is
/// at the first position, and that `=== false` correctly distinguishes the two.
#[test]
fn test_array_search_zero_index_is_not_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(10, $a) === false ? "miss" : "zero";
"#,
    );
    assert_eq!(out, "zero");
}

/// Verifies `array_key_exists` returns true for an existing integer key and false for a missing key.
#[test]
fn test_array_key_exists() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
if (array_key_exists(1, $a)) { echo "yes"; }
if (!array_key_exists(5, $a)) { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

/// Verifies `array_merge` concatenates two indexed arrays and preserves all elements.
#[test]
fn test_array_merge() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = array_merge($a, $b);
echo count($c);
echo $c[0] . $c[1] . $c[2] . $c[3];
"#,
    );
    assert_eq!(out, "41234");
}

/// Verifies `array_merge` uses the right operand element type when the left array is empty.
#[test]
fn test_array_merge_empty_left_uses_right_element_type() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$b = [3, 4];
$c = array_merge($a, $b);
echo count($c);
echo ":";
echo $c[0] . $c[1];
"#,
    );
    assert_eq!(out, "2:34");
}

/// Verifies the `+` operator keeps the left operand's values when both arrays
/// have the same numeric key (left wins semantics).
#[test]
fn test_indexed_array_union_keeps_left_duplicate_numeric_keys() {
    let out = compile_and_run(
        r#"<?php
$left = [10, 20];
$right = [99, 88, 77];
$result = $left + $right;
echo count($result) . ":" . $result[0] . "," . $result[1] . "," . $result[2];
"#,
    );
    assert_eq!(out, "3:10,20,77");
}

/// Verifies the `+` operator appends right-side string-keyed values that do not
/// exist in the left array (right-side keys are preserved for non-conflicting entries).
#[test]
fn test_indexed_array_union_string_values_append_missing_suffix() {
    let out = compile_and_run(
        r#"<?php
$left = ["left"];
$right = ["ignored", "added"];
$result = $left + $right;
echo count($result) . ":" . $result[0] . "," . $result[1];
"#,
    );
    assert_eq!(out, "2:left,added");
}

/// Verifies that an empty left array combined with `+` produces a result whose
/// indices and count mirror the right operand.
#[test]
fn test_indexed_array_union_empty_left_copies_right_layout() {
    let out = compile_and_run(
        r#"<?php
$result = [] + ["first", "second"];
echo count($result) . ":" . $result[0] . "," . $result[1];
"#,
    );
    assert_eq!(out, "2:first,second");
}

/// Verifies the boxed-carry reducer accepts a gradual array and explicit initial value.
#[test]
fn test_array_reduce_gradual_array_with_initial_value() {
    let output = compile_and_run(
        r#"<?php
function total(mixed $values): int {
    return array_reduce($values, static fn(mixed $carry, mixed $value): int => $carry + $value, 10);
}
echo total([1, 2, 3]);
"#,
    );
    assert_eq!(output, "16");
}

/// Verifies the compatibility helper sorts a gradual indexed array in place.
#[test]
fn test_sort_mixed_compatibility_helper() {
    let out = compile_and_run(
        r#"<?php
function sorted(array $values): array {
    sort($values);
    return $values;
}
$result = sorted(["charlie", "alpha", "bravo"]);
echo implode(",", $result);
"#,
    );
    assert_eq!(out, "alpha,bravo,charlie");
}

/// Verifies the gradual sort helper accepts an `array|false` producer after truthiness narrowing.
#[test]
fn test_sort_array_or_false_compatibility_helper() {
    let out = compile_and_run(
        r#"<?php
$values = preg_grep('/^[a-z]+$/', ["123", "charlie", "alpha", "bravo"]);
if (!$values) {
    echo "empty";
} else {
    sort($values);
    echo implode(",", $values);
}
"#,
    );
    assert_eq!(out, "alpha,bravo,charlie");
}

/// Verifies array builtins defer an `array|false` boundary to their runtime array-tag guards.
#[test]
fn test_array_builtins_accept_array_or_false_runtime_storage() {
    let out = compile_and_run(
        r#"<?php
function maybeValues(bool $available): array|false {
    return $available ? [3, 1, 2, 3] : false;
}
$values = maybeValues(true);
echo in_array(2, $values, true) ? "yes:" : "no:";
echo implode(",", array_keys($values)), ":";
$values = array_unique($values);
array_unshift($values, 0);
echo array_shift($values), ":", array_pop($values), ":";
sort($values);
echo array_is_list($values) ? implode(",", $values) : "not-list";
"#,
    );
    assert_eq!(out, "yes:0,1,2,3:0:2:1,3");
}

/// Verifies gradual membership dispatches associative values and rejects a non-array runtime arm.
#[test]
fn test_in_array_array_or_false_assoc_and_type_error() {
    let out = compile_and_run(
        r#"<?php
function maybeAssoc(bool $available): array|false {
    return $available ? ["first" => "2", "second" => 3] : false;
}
$values = maybeAssoc(true);
echo in_array(2, $values) ? "loose:" : "missing:";
echo in_array(2, $values, true) ? "strict-int:" : "not-strict-int:";
echo in_array("2", $values, true) ? "strict-string\n" : "not-strict-string\n";
try {
    in_array(1, maybeAssoc(false));
} catch (TypeError $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "loose:not-strict-int:strict-string\nin_array(): Argument #2 ($haystack) must be of type array, false given"
    );
}

/// Verifies associative prepend order, integer-key renumbering, and copy-on-write isolation.
#[test]
fn test_array_unshift_assoc_mixed_rebuild_semantics() {
    let out = compile_and_run(
        r#"<?php
$values = ["name" => "old", 5 => "tail", "last" => "end"];
$alias = $values;
$count = array_unshift($values, "first", "second");
echo $count, ":", $values[0], ":", $values[1], ":", $values["name"], ":", $values[2], ":", $values["last"], "\n";
echo count($alias), ":", $alias["name"], ":", $alias[5], ":", $alias["last"];
"#,
    );
    assert_eq!(out, "5:first:second:old:tail:end\n3:old:tail:end");
}

/// Verifies the compatibility helper handles runtime string lists and preserves surviving keys.
#[test]
fn test_array_diff_string_compatibility_helper() {
    let out = compile_and_run(
        r#"<?php
function leftDiffValues(): array {
    return ["alpha", "bravo", "alpha"];
}
function rightDiffValues(): array {
    return ["alpha"];
}
$result = array_diff(leftDiffValues(), rightDiffValues());
echo count($result), ":", $result[1];
"#,
    );
    assert_eq!(out, "1:bravo");
}

/// Verifies `array_diff()` uses PHP string-cast comparison across gradual scalar values.
#[test]
fn test_array_diff_gradual_values_use_string_comparison() {
    let out = compile_and_run(
        r#"<?php
function gradualDiffValues(): array {
    return [1, "2", 3];
}
$result = array_diff(gradualDiffValues(), ["1", 2]);
echo count($result), ":", $result[2];
"#,
    );
    assert_eq!(out, "1:3");
}

/// Verifies `array_intersect()` handles runtime string lists and preserves matching keys.
#[test]
fn test_array_intersect_string_compatibility_helper() {
    let out = compile_and_run(
        r#"<?php
function leftIntersectValues(): array {
    return ["alpha", "bravo", "alpha"];
}
function rightIntersectValues(): array {
    return ["alpha"];
}
$result = array_intersect(leftIntersectValues(), rightIntersectValues());
echo count($result), ":", $result[0], ":", $result[2];
"#,
    );
    assert_eq!(out, "2:alpha:alpha");
}

/// A value read through a gradual keyed boundary can participate in an array intersection.
#[test]
fn test_array_intersect_accepts_gradual_array_operand() {
    let out = compile_and_run(
        r#"<?php
function matching($group): array {
    return array_intersect($group['values'], ['alpha']);
}
$result = matching(['values' => ['named' => 'alpha', 2 => 'missing']]);
echo count($result), ':', $result['named'];
"#,
    );
    assert_eq!(out, "1:alpha");
}

/// Verifies `array_flip()` dispatches on the runtime tags stored in a heterogeneous packed array.
#[test]
fn test_array_flip_mixed_indexed_runtime_values() {
    let out = compile_and_run(
        r#"<?php
function flipInput(): array {
    return [1, "x", 2];
}
var_dump(array_flip(flipInput()));
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [1]=>\n  int(0)\n  [\"x\"]=>\n  int(1)\n  [2]=>\n  int(2)\n}\n"
    );
}

/// Verifies an empty declared-array return can be flipped even when its widened key type is Mixed.
#[test]
fn test_array_flip_empty_declared_array() {
    let out = compile_and_run(
        r#"<?php
function allowedValues(): array {
    return [];
}
var_dump(array_flip(allowedValues()));
"#,
    );
    assert_eq!(out, "array(0) {\n}\n");
}

/// Verifies gradual indexed and associative sources are flipped through runtime shape
/// validation while the owned normalization and boxed result remain heap-balanced.
#[test]
fn test_array_flip_gradual_runtime_shapes_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function flipGradual(mixed $input): mixed {
    return array_flip($input);
}
var_dump(flipGradual([1, "x"]));
var_dump(flipGradual([2 => "a", "b" => 9]));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(2) {\n  [1]=>\n  int(0)\n  [\"x\"]=>\n  int(1)\n}\narray(2) {\n  [\"a\"]=>\n  int(2)\n  [9]=>\n  string(1) \"b\"\n}\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a gradual array element can produce a statically associative flip result whose
/// values are the original integer indexes.
#[test]
fn test_array_flip_gradual_source_with_associative_result() {
    let out = compile_and_run(
        r#"<?php
function flipFirst(array $groups): array {
    return array_flip($groups[0]);
}
$flipped = flipFirst([['first', 'second']]);
echo $flipped['first'], ':', $flipped['second'];
"#,
    );
    assert_eq!(out, "0:1");
}

/// Verifies the compatibility helper searches Mixed lists with loose and strict comparison.
#[test]
fn test_array_search_mixed_compatibility_helper() {
    let out = compile_and_run(
        r#"<?php
function find(mixed $needle, array $haystack, bool $strict = false): int|false {
    return array_search($needle, $haystack, $strict);
}
echo find("2", [1, 2, 3]), ":";
var_dump(find("2", [1, 2, 3], true));
echo find("bravo", [1, "bravo", false], true);
"#,
    );
    assert_eq!(out, "1:bool(false)\n1");
}

/// Verifies `array_merge` supports PHP's zero, one, and variadic packed-array forms while
/// preserving scalar and string payload ownership across intermediate merge results.
#[test]
fn test_array_merge_zero_single_and_variadic_arguments() {
    let out = compile_and_run(
        r#"<?php
$empty = array_merge();
$single = array_merge([1, 2]);
$many = array_merge([1], [2, 3], [], [4], [5, 6], [7]);
$strings = array_merge(["a"], ["b", "c"], ["d"]);
echo count($empty), ":", implode(",", $single), ":", implode(",", $many), ":", implode(",", $strings);
"#,
    );
    assert_eq!(out, "0:1,2:1,2,3,4,5,6,7:a,b,c,d");
}

/// Verifies `array_merge` joins two arrays of two genuinely DIFFERENT classes.
///
/// The gate demanded both element types be identical, so any pair of object element types the
/// checker named differently refused to lower and took the whole compile with it. They produce
/// byte-for-byte the same code: one refcounted pointer per slot, stamped `object`, whatever class
/// it names. Heap-clean, so the merge itself is proved to own and release exactly its slots.
#[test]
fn test_array_merge_of_differently_named_object_elements() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Node {
    private string $label;
    public function __construct(string $label) { $this->label = $label; }
    public function getLabel(): string { return $this->label; }
}
class Tag {
    public function __construct(public string $name) {}
}
$mixedClasses = array_merge([new Node('n')], [new Tag('t')]);
echo count($mixedClasses), "\n";
echo get_class($mixedClasses[0]), "|", get_class($mixedClasses[1]), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2\nNode|Tag\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies `array_merge([$this], $this->getAllPrevious())` lowers and walks the whole chain.
///
/// This is the shape both `FlattenException::toArray()` and `FlattenException::getAsString()` are
/// written in: the first element is typed as the unspecified object `$this` is, the second from
/// the `array` return of the accessor, and demanding the two match refused the whole compile.
/// No heap assertion here — the residual this fixture leaves is the `?Node $previous` chain still
/// reachable at exit, which `test_array_merge_of_differently_named_object_elements` shows is not
/// the merge, and which the same program leaves behind with the `array_merge` call removed.
#[test]
fn test_array_merge_of_this_and_a_declared_object_array() {
    let out = compile_and_run(
        r#"<?php
class Node {
    private ?Node $previous = null;
    private string $label;

    public function __construct(string $label) { $this->label = $label; }

    public function setPrevious(?Node $previous): void { $this->previous = $previous; }

    public function getPrevious(): ?Node { return $this->previous; }

    /** @return Node[] */
    public function getAllPrevious(): array {
        $out = [];
        $e = $this->getPrevious();
        while ($e !== null) {
            $out[] = $e;
            $e = $e->getPrevious();
        }
        return $out;
    }

    public function getLabel(): string { return $this->label; }

    public function toArray(): array {
        $labels = [];
        foreach (array_merge([$this], $this->getAllPrevious()) as $node) {
            $labels[] = $node->getLabel();
        }
        return $labels;
    }

    public function getAsString(): string {
        $out = '';
        foreach (array_reverse(array_merge([$this], $this->getAllPrevious())) as $node) {
            $out .= $node->getLabel() . ';';
        }
        return $out;
    }
}

$a = new Node('a');
$b = new Node('b');
$c = new Node('c');
$b->setPrevious($a);
$c->setPrevious($b);
echo implode(',', $c->toArray()), "\n";
echo $c->getAsString(), "\n";
echo implode(',', $a->toArray()), "\n";
echo $a->getAsString(), "\n";
"#,
    );
    assert_eq!(out, "c,b,a\na;b;c;\na\na;\n");
}
