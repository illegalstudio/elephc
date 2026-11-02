//! Purpose:
//! Regression tests for `implode()` over elements that are not plain strings, ints or bools:
//! objects (php calls `__toString()`), floats, and boxed elements that hold an object.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - An array of objects or floats was refused at compile time (`unsupported EIR backend
//!   feature: implode array element PHP type Object("T")`), and a boxed object joined as an empty
//!   string. An object without `__toString()` is php's catchable `Error`, which the optimizer
//!   dropped along with its `try` while `implode()` was classified as unable to throw.

use crate::support::*;

/// Typed object arrays, float arrays, mixed and associative arrays all join like php.
#[test]
fn test_implode_converts_objects_and_floats_like_php() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $n = "t") {}
    public function __toString(): string { return "<" . $this->n . str_repeat("!", 2) . ">"; }
}
function join_all(array $a): string { return implode("+", $a); }
$mixed = [new T("a"), 1, "x", 2.5, null, false, new T("b")];
echo implode(", ", $mixed), "\n";
echo join_all($mixed), "\n";
echo implode($mixed), "\n";
echo implode("|", ["k" => new T("h"), "j" => 3]), "\n";
$objs = [new T("o1"), new T("o2")];
echo implode("/", $objs), " ", join(":", $objs), "\n";
echo implode(",", [0.1 + 0.2, 1e100, -0.0, 3.0]), "\n";
echo "pre:" . implode(",", [new T("x"), "y"]) . ":post\n";
"#,
    );
    assert_eq!(
        out,
        "<a!!>, 1, x, 2.5, , , <b!!>\n\
         <a!!>+1+x+2.5+++<b!!>\n\
         <a!!>1x2.5<b!!>\n\
         <h!!>|3\n\
         <o1!!>/<o2!!> <o1!!>:<o2!!>\n\
         0.3,1.0E+100,-0,3\n\
         pre:<x!!>,y:post\n"
    );
}

/// An object without `__toString()` raises php's Error, catchable inside a function.
#[test]
fn test_implode_non_stringable_object_error_is_catchable() {
    let out = compile_and_run(
        r#"<?php
class P {}
function f() { try { return implode(",", [1, new P()]); } catch (Error $e) { return "caught " . $e->getMessage(); } }
function g() { try { $s = implode(",", [1, new P()]); return $s; } catch (Error $e) { return "caught " . get_class($e); } }
echo f(), "\n", g(), "\n";
"#,
    );
    assert_eq!(
        out,
        "caught Object of class P could not be converted to string\ncaught Error\n"
    );
}

/// A class declared by eval converts through the eval bridge.
#[test]
fn test_implode_converts_eval_declared_objects() {
    let out = compile_and_run(
        r#"<?php
eval('class E { public function __construct(public string $v = "e") {} public function __toString(): string { return "[" . $this->v . "]"; } }');
function j(array $a): string { return implode(",", $a); }
echo j([new E("x"), 1, "s"]), "\n";
echo implode("|", [new E(), new E("y")]), "\n";
"#,
    );
    assert_eq!(out, "[x],1,s\n[e]|[y]\n");
}

/// The converted strings, the joined result and the objects are all released.
#[test]
fn test_implode_object_conversion_releases_everything() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T {
    public function __construct(public string $n) {}
    public function __toString(): string { return "<" . $this->n . ">"; }
}
function run(): void {
    $len = 0;
    for ($i = 0; $i < 5; $i++) {
        $len += strlen(implode(",", [new T("a"), 1, "x", 2.5]));
        $len += strlen(implode(",", [new T("b"), new T("c")]) . join([new T("d")]));
        $len += strlen(implode(",", ["k" => new T("e"), "j" => 2]));
        $len += strlen(implode(",", [1.5, 0.1 + 0.2]));
    }
    echo $len, "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "165\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
