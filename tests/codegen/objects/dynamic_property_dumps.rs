//! Purpose:
//! End-to-end tests for dumping DYNAMIC properties with `var_dump()`, `print_r()` and
//! `var_export()`: stdClass instances, `#[\AllowDynamicProperties]` classes, and the
//! deprecated runtime-named writes on plain classes (issue #708).
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is reference PHP 8.5 output, byte for byte (stdout only; the
//!   deprecation notice goes to stderr).
//! - The dump walkers append the object's dynamic-property hash after the declared
//!   descriptor rows, in insertion order; a folded `__debugInfo()` projection suppresses it.
//! - The heap fixture exercises all three renderers, including their return modes, in a
//!   loop and requires a clean heap-debug summary.

use crate::support::*;

/// `var_dump`, `print_r` and `var_export` list a stdClass's dynamic properties (issue #708), and an empty stdClass stays empty.
#[test]
fn test_dump_stdclass_dynamic_properties() {
    let out = compile_and_run(r#"<?php
$o = new stdClass();
$o->name = "Ada";
$o->age = 36;
var_dump($o);
print_r($o);
echo "\n";
var_export($o);
echo "\n";
$e = new stdClass();
var_dump($e);
print_r($e);
echo "\n";
var_export($e);
echo "\n";
"#);
    assert_eq!(
        out,
        concat!(
            "object(stdClass)#1 (2) {\n",
            "  [\"name\"]=>\n",
            "  string(3) \"Ada\"\n",
            "  [\"age\"]=>\n",
            "  int(36)\n",
            "}\n",
            "stdClass Object\n",
            "(\n",
            "    [name] => Ada\n",
            "    [age] => 36\n",
            ")\n",
            "\n",
            "(object) array(\n",
            "   'name' => 'Ada',\n",
            "   'age' => 36,\n",
            ")\n",
            "object(stdClass)#2 (0) {\n",
            "}\n",
            "stdClass Object\n",
            "(\n",
            ")\n",
            "\n",
            "(object) array(\n",
            ")\n",
        )
    );
}

/// Dynamic properties follow the declared ones, and the `(n)` header counts them but not an uninitialized typed property.
#[test]
fn test_dump_declared_then_dynamic_properties() {
    let out = compile_and_run(r#"<?php
#[\AllowDynamicProperties]
class Point {
    public int $x = 1;
    protected string $label = "p";
    private ?array $meta = null;
    public int $unset;
}
$p = new Point();
$p->extra = "dyn";
$p->nums = [1, 2];
var_dump($p);
print_r($p);
echo "\n";
var_export($p);
echo "\n";
"#);
    assert_eq!(
        out,
        concat!(
            "object(Point)#1 (5) {\n",
            "  [\"x\"]=>\n",
            "  int(1)\n",
            "  [\"label\":protected]=>\n",
            "  string(1) \"p\"\n",
            "  [\"meta\":\"Point\":private]=>\n",
            "  NULL\n",
            "  [\"unset\"]=>\n",
            "  uninitialized(int)\n",
            "  [\"extra\"]=>\n",
            "  string(3) \"dyn\"\n",
            "  [\"nums\"]=>\n",
            "  array(2) {\n",
            "    [0]=>\n",
            "    int(1)\n",
            "    [1]=>\n",
            "    int(2)\n",
            "  }\n",
            "}\n",
            "Point Object\n",
            "(\n",
            "    [x] => 1\n",
            "    [label:protected] => p\n",
            "    [meta:Point:private] => \n",
            "    [extra] => dyn\n",
            "    [nums] => Array\n",
            "        (\n",
            "            [0] => 1\n",
            "            [1] => 2\n",
            "        )\n",
            "\n",
            ")\n",
            "\n",
            "\\Point::__set_state(array(\n",
            "   'x' => 1,\n",
            "   'label' => 'p',\n",
            "   'meta' => NULL,\n",
            "   'extra' => 'dyn',\n",
            "   'nums' => \n",
            "  array (\n",
            "    0 => 1,\n",
            "    1 => 2,\n",
            "  ),\n",
            "))\n",
        )
    );
}

/// A runtime-named write on a plain class (a deprecated dynamic property) is dumped after the declared property.
#[test]
fn test_dump_deprecated_runtime_named_dynamic_property() {
    let out = compile_and_run(r#"<?php
class Legacy {
    public string $kind = "legacy";
    public function __construct() {
        $k = "added";
        $this->$k = 42;
    }
}
$l = new Legacy();
var_dump($l);
print_r($l);
echo "\n";
var_export($l);
echo "\n";
"#);
    assert_eq!(
        out,
        concat!(
            "object(Legacy)#1 (2) {\n",
            "  [\"kind\"]=>\n",
            "  string(6) \"legacy\"\n",
            "  [\"added\"]=>\n",
            "  int(42)\n",
            "}\n",
            "Legacy Object\n",
            "(\n",
            "    [kind] => legacy\n",
            "    [added] => 42\n",
            ")\n",
            "\n",
            "\\Legacy::__set_state(array(\n",
            "   'kind' => 'legacy',\n",
            "   'added' => 42,\n",
            "))\n",
        )
    );
}

/// Nested arrays and objects held in dynamic properties recurse with PHP's indentation, and an unset one disappears.
#[test]
fn test_dump_nested_dynamic_values_after_unset() {
    let out = compile_and_run(r#"<?php
$n = new stdClass();
$n->a = 1;
$n->f = 1.5;
$n->b = true;
$n->z = null;
$n->list = ["x" => 1, "y" => [2, 3]];
$inner = new stdClass();
$inner->deep = "yes";
$n->child = $inner;
unset($n->f);
var_dump($n);
print_r($n);
echo "\n";
var_export($n);
echo "\n";
var_dump([$inner, "k" => $inner]);
print_r([$inner]);
echo "\n";
"#);
    assert_eq!(
        out,
        concat!(
            "object(stdClass)#1 (5) {\n",
            "  [\"a\"]=>\n",
            "  int(1)\n",
            "  [\"b\"]=>\n",
            "  bool(true)\n",
            "  [\"z\"]=>\n",
            "  NULL\n",
            "  [\"list\"]=>\n",
            "  array(2) {\n",
            "    [\"x\"]=>\n",
            "    int(1)\n",
            "    [\"y\"]=>\n",
            "    array(2) {\n",
            "      [0]=>\n",
            "      int(2)\n",
            "      [1]=>\n",
            "      int(3)\n",
            "    }\n",
            "  }\n",
            "  [\"child\"]=>\n",
            "  object(stdClass)#2 (1) {\n",
            "    [\"deep\"]=>\n",
            "    string(3) \"yes\"\n",
            "  }\n",
            "}\n",
            "stdClass Object\n",
            "(\n",
            "    [a] => 1\n",
            "    [b] => 1\n",
            "    [z] => \n",
            "    [list] => Array\n",
            "        (\n",
            "            [x] => 1\n",
            "            [y] => Array\n",
            "                (\n",
            "                    [0] => 2\n",
            "                    [1] => 3\n",
            "                )\n",
            "\n",
            "        )\n",
            "\n",
            "    [child] => stdClass Object\n",
            "        (\n",
            "            [deep] => yes\n",
            "        )\n",
            "\n",
            ")\n",
            "\n",
            "(object) array(\n",
            "   'a' => 1,\n",
            "   'b' => true,\n",
            "   'z' => NULL,\n",
            "   'list' => \n",
            "  array (\n",
            "    'x' => 1,\n",
            "    'y' => \n",
            "    array (\n",
            "      0 => 2,\n",
            "      1 => 3,\n",
            "    ),\n",
            "  ),\n",
            "   'child' => \n",
            "  (object) array(\n",
            "     'deep' => 'yes',\n",
            "  ),\n",
            ")\n",
            "array(2) {\n",
            "  [0]=>\n",
            "  object(stdClass)#2 (1) {\n",
            "    [\"deep\"]=>\n",
            "    string(3) \"yes\"\n",
            "  }\n",
            "  [\"k\"]=>\n",
            "  object(stdClass)#2 (1) {\n",
            "    [\"deep\"]=>\n",
            "    string(3) \"yes\"\n",
            "  }\n",
            "}\n",
            "Array\n",
            "(\n",
            "    [0] => stdClass Object\n",
            "        (\n",
            "            [deep] => yes\n",
            "        )\n",
            "\n",
            ")\n",
            "\n",
        )
    );
}

/// A `__debugInfo()` projection hides dynamic properties, and a dynamic self-reference renders `*RECURSION*`.
#[test]
fn test_dump_dynamic_properties_debug_info_and_recursion() {
    let out = compile_and_run(r#"<?php
#[\AllowDynamicProperties]
class Masked {
    public int $a = 1;
    private string $secret = "s";
    public function __debugInfo() {
        return ['a' => $this->a];
    }
}
$m = new Masked();
$m->extra = "dyn";
var_dump($m);
print_r($m);
echo "\n";
$o = new stdClass();
$o->name = "loop";
$o->self = $o;
var_dump($o);
print_r($o);
echo "\n";
"#);
    assert_eq!(
        out,
        concat!(
            "object(Masked)#1 (1) {\n",
            "  [\"a\"]=>\n",
            "  int(1)\n",
            "}\n",
            "Masked Object\n",
            "(\n",
            "    [a] => 1\n",
            ")\n",
            "\n",
            "object(stdClass)#2 (2) {\n",
            "  [\"name\"]=>\n",
            "  string(4) \"loop\"\n",
            "  [\"self\"]=>\n",
            "  *RECURSION*\n",
            "}\n",
            "stdClass Object\n",
            "(\n",
            "    [name] => loop\n",
            "    [self] => stdClass Object\n",
            " *RECURSION*\n",
            ")\n",
            "\n",
        )
    );
}

/// Dumping objects with dynamic properties, in every renderer and return mode, leaks nothing.
#[test]
fn test_dump_dynamic_properties_heap_is_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
#[\AllowDynamicProperties]
class HeapPoint {
    public int $x = 1;
    public int $unset;
}
for ($i = 0; $i < 25; $i++) {
    $o = new stdClass();
    $o->name = "Ada" . $i;
    $o->list = ["x" => $i, "y" => [2, 3]];
    $c = new stdClass();
    $c->deep = "v" . $i;
    $o->child = $c;
    $p = new HeapPoint();
    $p->extra = "dyn" . $i;
    $p->nums = [1, $i];
    $o->pt = $p;
    ob_start();
    var_dump($o);
    print_r($o);
    var_export($o);
    ob_end_clean();
    $s = print_r($o, true);
    $t = var_export($p, true);
    if ($i === 24) {
        echo strlen($s), " ", strlen($t);
    }
}
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "525 123", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
