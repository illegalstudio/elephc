//! Purpose:
//! End-to-end coverage for append l-values beyond a trailing `$var[] = $v` (issue #845): an
//! append in the middle of a nested write, an append used as an assignment expression, `$this[]`
//! and call-result appends on an `ArrayAccess` object, and an append onto an array call result.
//!
//! Called from:
//! - `cargo test --test codegen_tests arrays::append_lvalues`.
//!
//! Key details:
//! - Every expected output was produced by PHP 8.5 from the same fixture.
//! - The heap test runs the shapes in a loop at two iteration counts, so a
//!   per-iteration leak cannot hide behind a value that is merely live at exit.

use crate::support::*;

/// Verifies `$a['k'][]['to'][]['email'] = $v` creates a new element at each `[]` and writes
/// through it, that a later write can extend an existing element, and that a copy taken before
/// the write is unchanged.
#[test]
fn test_append_in_the_middle_of_a_nested_write() {
    let out = compile_and_run(
        r#"<?php
$body = [];
$body['personalizations'][]['to'][]['email'] = 'ok';
$body['personalizations'][]['to'][]['email'] = 'ok2';
$body['personalizations'][0]['to'][]['email'] = 'ok3';
echo $body['personalizations'][0]['to'][0]['email'], "\n";
echo json_encode($body), "\n";
$copy = $body;
$body['personalizations'][]['to'][] = 'x';
echo count($copy['personalizations']), " ", count($body['personalizations']), "\n";
echo json_encode($copy), "\n";
$m = [];
$m[][] = 1;
$m[][] = 2;
echo json_encode($m), "\n";
"#,
    );
    assert_eq!(
        out,
        "ok\n\
         {\"personalizations\":[{\"to\":[{\"email\":\"ok\"},{\"email\":\"ok3\"}]},{\"to\":[{\"email\":\"ok2\"}]}]}\n\
         2 3\n\
         {\"personalizations\":[{\"to\":[{\"email\":\"ok\"},{\"email\":\"ok3\"}]},{\"to\":[{\"email\":\"ok2\"}]}]}\n\
         [[1],[2]]\n"
    );
}

/// Verifies mid-chain appends inside methods and functions, onto a typed property, a static
/// property, and a local, which is where framework code writes them.
#[test]
fn test_append_in_the_middle_of_property_and_local_writes_inside_methods() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    public array $items = [];
    public static array $groups = [];
    public function add(string $name): void {
        $this->items[]['name'] = $name;
        self::$groups[]['tags'][] = $name;
    }
}
$h = new Holder();
$h->add('a');
$h->add('b');
echo json_encode($h->items), "\n";
echo json_encode(Holder::$groups), "\n";
function mk(): array { $r = []; $r[]["a"][] = 1; return $r; }
echo json_encode(mk()), "\n";
"#,
    );
    assert_eq!(
        out,
        "[{\"name\":\"a\"},{\"name\":\"b\"}]\n\
         [{\"tags\":[\"a\"]},{\"tags\":[\"b\"]}]\n\
         [{\"a\":[1]}]\n"
    );
}

/// Verifies a mid-chain append evaluates its dimensions before the value, and that the value
/// reads the container as it was before the new element was appended.
#[test]
fn test_append_chain_evaluation_order() {
    let out = compile_and_run(
        r#"<?php
function ix($n) { echo "ix$n\n"; return $n; }
function rv() { echo "rv\n"; return 'v'; }
$e = [];
$e[ix('a')][][ix('b')] = rv();
echo json_encode($e), "\n";
$list = [1];
$list[][] = count($list);
echo json_encode($list), "\n";
"#,
    );
    assert_eq!(out, "ixa\nixb\nrv\n{\"a\":[{\"b\":\"v\"}]}\n[1,[1]]\n");
}

/// Verifies an append used as an assignment expression performs the append and yields the
/// assigned value, including chained and nested-append forms.
#[test]
fn test_append_as_assignment_expression() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$x = ($a[] = 5);
echo $x, " ", count($a), "\n";
echo ($a[] = 7), "\n";
$y = $a[] = 9;
echo $y, " ", json_encode($a), "\n";
$s = [];
$r = ($s['q'][] = 'str');
echo $r, " ", json_encode($s), "\n";
$p = [];
$q = [];
$p[] = $q[] = 'both';
echo json_encode($p), json_encode($q), "\n";
"#,
    );
    assert_eq!(
        out,
        "5 1\n7\n9 [5,7,9]\nstr {\"q\":[\"str\"]}\n[\"both\"][\"both\"]\n"
    );
}

/// Verifies `$this[] = $v` inside an `ArrayAccess` class, `$obj[] = $v`, and an append onto a
/// function or static-method result holding the object all call `offsetSet(null, $v)`.
#[test]
fn test_append_on_array_access_object_calls_offset_set_with_null() {
    let out = compile_and_run(
        r#"<?php
class Bag implements ArrayAccess {
    public $log = [];
    public function offsetExists($offset): bool { return false; }
    public function offsetGet($offset): mixed { return null; }
    public function offsetSet($offset, $value): void {
        $this->log[] = ($offset === null ? "null" : $offset) . "=" . $value;
    }
    public function offsetUnset($offset): void {}
    public function add($value) { $this[] = $value; }
    public function addTwice($value) { $this[] = $value; $this[] = $value * 10; }
}
function pass(Bag $bag): Bag { return $bag; }
class Registry {
    public static function pass(Bag $bag): Bag { return $bag; }
}
$bag = new Bag();
$bag->add(3);
$bag->addTwice(4);
$bag[] = 5;
pass($bag)[] = 6;
Registry::pass($bag)[] = 7;
echo implode(",", $bag->log), "\n";
"#,
    );
    assert_eq!(out, "null=3,null=4,null=40,null=5,null=6,null=7\n");
}

/// Verifies an append onto an array call result evaluates the call and discards the write.
#[test]
fn test_append_on_array_call_result_is_discarded() {
    let out = compile_and_run(
        r#"<?php
function values() { echo "called\n"; return [1]; }
values()[] = 2;
echo json_encode(values()), "\n";
"#,
    );
    assert_eq!(out, "called\ncalled\n[1]\n");
}

/// Verifies every new append shape releases what it allocates: the loop runs 10 and 1000
/// times (`$argc` keeps the count opaque to constant folding), the result matches PHP at both
/// counts, and the heap ends clean at both.
#[test]
fn test_append_lvalues_heap_is_clean_at_two_iteration_counts() {
    let source = r#"<?php
class Bag implements ArrayAccess {
    public $d = [];
    public function offsetExists($o): bool { return true; }
    public function offsetGet($o): mixed { return 1; }
    public function offsetSet($o, $v): void { $this->d[] = $v; }
    public function offsetUnset($o): void {}
    public function add($v) { $this[] = $v; }
}
function values() { return [1, 2]; }
function pass(Bag $b): Bag { return $b; }
$n = $argc * __N__;
$total = 0;
for ($i = 0; $i < $n; $i++) {
    $body = [];
    $body['p'][]['to'][]['email'] = 'user' . $i;
    $body['p'][]['to'][]['email'] = 'other';
    $copy = $body;
    $body['p'][0]['to'][]['email'] = 'x' . $i;
    $m = [];
    $m[][] = $i;
    $a = [];
    $x = ($a[] = 'v' . $i);
    $y = $a['k'][] = [$i];
    $bag = new Bag();
    $bag->add('s' . $i);
    pass($bag)[] = 't' . $i;
    values()[] = 3;
    $total += count($body['p']) + count($copy['p']) + count($m) + count($a) + count($bag->d) + strlen($x) + count($y);
}
echo $total, "\n";
"#;
    for (iterations, expected) in [("10", "120\n"), ("1000", "13890\n")] {
        let out = compile_and_run_with_heap_debug(&source.replace("__N__", iterations));
        assert_eq!(out.stdout, expected);
        assert!(
            out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected a clean heap, got: {}",
            out.stderr
        );
    }
}
