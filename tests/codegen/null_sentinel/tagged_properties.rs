//! Purpose:
//! End-to-end coverage of the runtime object walkers over a tagged nullable-int PROPERTY:
//! `var_dump`, `print_r`, `var_export`, `json_encode`, `serialize`/`unserialize`,
//! `get_object_vars()` and `(array)`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A `?int` / `int|null` property keeps an inline `{payload, tag}` pair in its slot, and the
//!   class descriptors those walkers read used to give it the union's static tag 7 ("boxed
//!   Mixed"), so each walker dereferenced the integer payload as a cell pointer (#1503). The
//!   descriptors now carry `TAGGED_SCALAR_PROPERTY_TAG`, resolved from the slot's high word.
//! - Every fixture forces the tagged representation, holds both an integer and a null, and feeds
//!   at least one value through `$argc` so constant folding cannot hollow it out. Expected output
//!   is real `php` 8.5 output for the exact fixture text.

use super::*;

/// `var_dump()` of an object with a `?int` property, the issue's reproduction (#1503).
///
/// It printed `["n"]=>` and then segfaulted, whether the property held `5` or `null`: the
/// descriptor tag 7 sent the integer payload to the Mixed unboxer as a pointer. `int|null` is the
/// same representation under another spelling and takes the same path.
#[test]
fn test_var_dump_of_a_tagged_nullable_int_property() {
    let out = compile_and_run_tagged(
        r#"<?php
class P { public ?int $n = null; public string $s = "x"; }
class U { public int|null $n = 9; }
$p = new P(); $p->n = 5;
var_dump($p);
var_dump(new P());
var_dump(new U());
$u = new U(); $u->n = $argc > 5 ? 1 : null;
var_dump($u);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (2) {\n",
            "  [\"n\"]=>\n",
            "  int(5)\n",
            "  [\"s\"]=>\n",
            "  string(1) \"x\"\n",
            "}\n",
            "object(P)#2 (2) {\n",
            "  [\"n\"]=>\n",
            "  NULL\n",
            "  [\"s\"]=>\n",
            "  string(1) \"x\"\n",
            "}\n",
            "object(U)#2 (1) {\n",
            "  [\"n\"]=>\n",
            "  int(9)\n",
            "}\n",
            "object(U)#2 (1) {\n",
            "  [\"n\"]=>\n",
            "  NULL\n",
            "}\n",
        )
    );
}

/// `print_r()`, `var_export()` and `json_encode()` read the same slot through their own descriptor
/// tables, and all three crashed the same way before the tag was resolved from the slot.
#[test]
fn test_print_r_var_export_and_json_encode_of_a_tagged_nullable_int_property() {
    let out = compile_and_run_tagged(
        r#"<?php
class P { public ?int $n = null; public string $s = "x"; }
function mk(?int $v): P { $p = new P(); $p->n = $v; return $p; }
$set = mk($argc + 4);
$null = mk($argc > 5 ? 1 : null);
print_r($set); echo "\n";
print_r($null); echo "\n";
echo print_r($set, true), "|", var_export($set, true), "\n";
var_export($null); echo "\n";
echo json_encode($set), "|", json_encode($null), "\n";
echo json_encode([$set, $null], JSON_PRETTY_PRINT), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "P Object\n",
            "(\n",
            "    [n] => 5\n",
            "    [s] => x\n",
            ")\n",
            "\n",
            "P Object\n",
            "(\n",
            "    [n] => \n",
            "    [s] => x\n",
            ")\n",
            "\n",
            "P Object\n",
            "(\n",
            "    [n] => 5\n",
            "    [s] => x\n",
            ")\n",
            "|\\P::__set_state(array(\n",
            "   'n' => 5,\n",
            "   's' => 'x',\n",
            "))\n",
            "\\P::__set_state(array(\n",
            "   'n' => NULL,\n",
            "   's' => 'x',\n",
            "))\n",
            "{\"n\":5,\"s\":\"x\"}|{\"n\":null,\"s\":\"x\"}\n",
            "[\n",
            "    {\n",
            "        \"n\": 5,\n",
            "        \"s\": \"x\"\n",
            "    },\n",
            "    {\n",
            "        \"n\": null,\n",
            "        \"s\": \"x\"\n",
            "    }\n",
            "]\n",
        )
    );
}

/// `serialize()` writes the property from its runtime tag, and `unserialize()` must write the
/// `{payload, tag}` pair back rather than a boxed cell, at the edges of the int range as well as
/// for null and across every visibility.
#[test]
fn test_serialize_roundtrip_of_a_tagged_nullable_int_property() {
    let out = compile_and_run_tagged(
        r#"<?php
class P { public ?int $n = null; protected ?int $m = 3; private int|null $k = null; }
$p = new P(); $p->n = PHP_INT_MAX;
$q = new P(); $q->n = $argc > 5 ? 1 : PHP_INT_MIN;
$r = new P();
foreach ([$p, $q, $r] as $o) {
    $s = serialize($o);
    echo str_replace("\0", "~", $s), "\n";
    $back = unserialize($s);
    var_dump($back);
    var_dump($back->n === null, $back->n ?? "dflt");
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "O:1:\"P\":3:{s:1:\"n\";i:9223372036854775807;s:4:\"~*~m\";i:3;s:4:\"~P~k\";N;}\n",
            "object(P)#4 (3) {\n",
            "  [\"n\"]=>\n",
            "  int(9223372036854775807)\n",
            "  [\"m\":protected]=>\n",
            "  int(3)\n",
            "  [\"k\":\"P\":private]=>\n",
            "  NULL\n",
            "}\n",
            "bool(false)\n",
            "int(9223372036854775807)\n",
            "O:1:\"P\":3:{s:1:\"n\";i:-9223372036854775808;s:4:\"~*~m\";i:3;s:4:\"~P~k\";N;}\n",
            "object(P)#5 (3) {\n",
            "  [\"n\"]=>\n",
            "  int(-9223372036854775808)\n",
            "  [\"m\":protected]=>\n",
            "  int(3)\n",
            "  [\"k\":\"P\":private]=>\n",
            "  NULL\n",
            "}\n",
            "bool(false)\n",
            "int(-9223372036854775808)\n",
            "O:1:\"P\":3:{s:1:\"n\";N;s:4:\"~*~m\";i:3;s:4:\"~P~k\";N;}\n",
            "object(P)#4 (3) {\n",
            "  [\"n\"]=>\n",
            "  NULL\n",
            "  [\"m\":protected]=>\n",
            "  int(3)\n",
            "  [\"k\":\"P\":private]=>\n",
            "  NULL\n",
            "}\n",
            "bool(true)\n",
            "string(4) \"dflt\"\n",
        )
    );
}

/// `get_object_vars()` (from outside and from inside the class) and the `(array)` cast box the
/// property from its runtime tag, and an uninitialized `?int` property is still omitted.
#[test]
fn test_object_projections_of_a_tagged_nullable_int_property() {
    let out = compile_and_run_tagged(
        r#"<?php
class P {
    public ?int $n = null;
    protected ?int $m = 3;
    public ?int $u;
    public function inside(): array { return get_object_vars($this); }
}
$p = new P(); $p->n = $argc + 6;
$q = new P();
var_dump(get_object_vars($p));
var_dump(get_object_vars($q));
var_dump($p->inside());
foreach ((array)$q as $k => $v) { echo str_replace("\0", "~", $k), "=", var_export($v, true), "\n"; }
print_r($q); echo "\n";
var_export($q); echo "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "array(1) {\n",
            "  [\"n\"]=>\n",
            "  int(7)\n",
            "}\n",
            "array(1) {\n",
            "  [\"n\"]=>\n",
            "  NULL\n",
            "}\n",
            "array(2) {\n",
            "  [\"n\"]=>\n",
            "  int(7)\n",
            "  [\"m\"]=>\n",
            "  int(3)\n",
            "}\n",
            "n=NULL\n",
            "~*~m=3\n",
            "P Object\n",
            "(\n",
            "    [n] => \n",
            "    [m:protected] => 3\n",
            ")\n",
            "\n",
            "\\P::__set_state(array(\n",
            "   'n' => NULL,\n",
            "   'm' => 3,\n",
            "))\n",
        )
    );
}

/// Promoted, inherited, protected and private `?int` properties, an object nested in another
/// object and in an array, and a clone that must copy the pair rather than share a cell.
#[test]
fn test_tagged_nullable_int_properties_in_nested_and_inherited_objects() {
    let out = compile_and_run_tagged(
        r#"<?php
class C {
    public function __construct(public ?int $a = null, protected ?int $b = 7, private int|null $c = null) {}
}
class D extends C { public ?int $d = null; }
class W { public ?D $inner = null; public ?int $w = 1; }
$list = [];
for ($i = 0; $i < 3; $i++) {
    $x = new D($i === 1 ? null : $i * $argc);
    $x->d = $i === 2 ? null : -$i;
    $list[] = $x;
}
$w = new W(); $w->inner = $list[1];
var_dump($w);
print_r($list[2]); echo "\n";
echo json_encode($list), "\n";
$c = clone $list[0];
$c->a = 40;
var_dump($c->a, $list[0]->a);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "object(W)#4 (2) {\n",
            "  [\"inner\"]=>\n",
            "  object(D)#2 (4) {\n",
            "    [\"a\"]=>\n",
            "    NULL\n",
            "    [\"b\":protected]=>\n",
            "    int(7)\n",
            "    [\"c\":\"C\":private]=>\n",
            "    NULL\n",
            "    [\"d\"]=>\n",
            "    int(-1)\n",
            "  }\n",
            "  [\"w\"]=>\n",
            "  int(1)\n",
            "}\n",
            "D Object\n",
            "(\n",
            "    [a] => 2\n",
            "    [b:protected] => 7\n",
            "    [c:C:private] => \n",
            "    [d] => \n",
            ")\n",
            "\n",
            "[{\"a\":0,\"d\":0},{\"a\":null,\"d\":-1},{\"a\":2,\"d\":null}]\n",
            "int(40)\n",
            "int(0)\n",
        )
    );
}
