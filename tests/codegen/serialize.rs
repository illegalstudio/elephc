//! Purpose:
//! Provides end-to-end codegen tests for the `serialize()` / `unserialize()` builtins.
//! Exercises the runtime serialize/unserialize helpers through compiled PHP programs.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the serialize codegen test module.
//!
//! Key details:
//! - Output must match PHP's serialize() wire format byte-for-byte for the scalar
//!   subset (null/bool/int/float/string); array support is added in a later increment.
//! - Round-trips go through both helpers so a regression in either is caught.

use crate::support::*;

/// Verifies `serialize()` formats each scalar type exactly like PHP's wire format.
#[test]
fn test_serialize_scalars_match_php_wire_format() {
    let out = compile_and_run(
        r#"<?php
echo serialize(42), "\n";
echo serialize(-7), "\n";
echo serialize(0), "\n";
echo serialize(true), "\n";
echo serialize(false), "\n";
echo serialize(null), "\n";
echo serialize("hello"), "\n";
echo serialize(""), "\n";
echo serialize(3.14), "\n";
echo serialize(0.0), "\n";
"#,
    );
    assert_eq!(
        out,
        "i:42;\ni:-7;\ni:0;\nb:1;\nb:0;\nN;\ns:5:\"hello\";\ns:0:\"\";\nd:3.14;\nd:0;\n",
    );
}

/// Verifies `serialize()` preserves exact byte length for strings with quotes and
/// special bytes (serialize does not escape, unlike JSON).
#[test]
fn test_serialize_string_is_unescaped_byte_length() {
    let out = compile_and_run(
        r#"<?php echo serialize("a\"b\\c");"#,
    );
    // 5 bytes: a " b \ c, written verbatim between the quotes.
    assert_eq!(out, "s:5:\"a\"b\\c\";");
}

/// Verifies `unserialize()` reconstructs each scalar type from its wire form.
#[test]
fn test_unserialize_scalars_round_trip() {
    let out = compile_and_run(
        r#"<?php
var_dump(unserialize("i:42;"));
var_dump(unserialize("i:-7;"));
var_dump(unserialize("b:1;"));
var_dump(unserialize("b:0;"));
var_dump(unserialize("N;"));
var_dump(unserialize("s:5:\"hello\";"));
"#,
    );
    assert_eq!(
        out,
        "int(42)\nint(-7)\nbool(true)\nbool(false)\nNULL\nstring(5) \"hello\"\n",
    );
}

/// Verifies a full `unserialize(serialize($x))` round-trip preserves scalar values.
#[test]
fn test_serialize_unserialize_round_trip_preserves_values() {
    let out = compile_and_run(
        r#"<?php
var_dump(unserialize(serialize(12345)));
var_dump(unserialize(serialize("round trip")));
var_dump(unserialize(serialize(2.5)));
var_dump(unserialize(serialize(true)));
var_dump(unserialize(serialize(null)));
"#,
    );
    assert_eq!(
        out,
        "int(12345)\nstring(10) \"round trip\"\nfloat(2.5)\nbool(true)\nNULL\n",
    );
}

/// Verifies `unserialize()` returns PHP `false` on malformed or unsupported input.
#[test]
fn test_unserialize_failure_returns_false() {
    let out = compile_and_run(
        r#"<?php
var_dump(unserialize("garbage"));
var_dump(unserialize(""));
"#,
    );
    assert_eq!(out, "bool(false)\nbool(false)\n");
}

/// Verifies `serialize()` of indexed and associative arrays matches PHP's a:n:{...} form.
#[test]
fn test_serialize_arrays_match_php_wire_format() {
    let out = compile_and_run(
        r#"<?php
echo serialize([1, 2, 3]), "\n";
echo serialize(["a" => 1, "b" => 2]), "\n";
echo serialize(["x" => "hello", "y" => 3.5, "z" => true]), "\n";
echo serialize([10 => "ten", 20 => "twenty"]), "\n";
echo serialize([]), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}\n",
            "a:2:{s:1:\"a\";i:1;s:1:\"b\";i:2;}\n",
            "a:3:{s:1:\"x\";s:5:\"hello\";s:1:\"y\";d:3.5;s:1:\"z\";b:1;}\n",
            "a:2:{i:10;s:3:\"ten\";i:20;s:6:\"twenty\";}\n",
            "a:0:{}\n",
        ),
    );
}

/// Verifies nested arrays serialize recursively with the correct inner a:n:{...} blocks.
#[test]
fn test_serialize_nested_arrays() {
    let out = compile_and_run(
        r#"<?php echo serialize(["nested" => [1, 2], "k" => "v"]);"#,
    );
    assert_eq!(out, "a:2:{s:6:\"nested\";a:2:{i:0;i:1;i:1;i:2;}s:1:\"k\";s:1:\"v\";}");
}

/// Verifies `unserialize()` rebuilds indexed and associative arrays, checking the
/// reconstructed structure both by `var_dump` and by re-serializing to the same bytes.
#[test]
fn test_unserialize_arrays_round_trip() {
    let out = compile_and_run(
        r#"<?php
var_dump(unserialize("a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}"));
var_dump(unserialize('a:2:{s:1:"a";i:1;s:1:"b";s:3:"two";}'));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "array(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n  [2]=>\n  int(3)\n}\n",
            "array(2) {\n  [\"a\"]=>\n  int(1)\n  [\"b\"]=>\n  string(3) \"two\"\n}\n",
        ),
    );
}

/// Verifies a serialize -> unserialize -> serialize round-trip of nested arrays is
/// byte-identical, proving the rebuilt hash matches PHP's structure exactly.
#[test]
fn test_unserialize_arrays_reserialize_identity() {
    let out = compile_and_run(
        r#"<?php
echo serialize(unserialize('a:2:{s:1:"x";i:5;s:6:"nested";a:2:{i:0;b:1;i:1;d:2.5;}}')), "\n";
echo serialize(unserialize(serialize(["k" => "v", "n" => [1, 2, 3]]))), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "a:2:{s:1:\"x\";i:5;s:6:\"nested\";a:2:{i:0;b:1;i:1;d:2.5;}}\n",
            "a:2:{s:1:\"k\";s:1:\"v\";s:1:\"n\";a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}}\n",
        ),
    );
}

/// Verifies non-finite floats serialize to PHP's INF/-INF/NAN spellings and round-trip.
#[test]
fn test_serialize_non_finite_floats() {
    let out = compile_and_run(
        r#"<?php
echo serialize(INF), "\n";
echo serialize(-INF), "\n";
echo serialize(NAN), "\n";
var_dump(unserialize("d:INF;"));
var_dump(is_nan(unserialize("d:NAN;")));
"#,
    );
    assert_eq!(out, "d:INF;\nd:-INF;\nd:NAN;\nfloat(INF)\nbool(true)\n");
}

/// Regression: floats that serialize in exponential notation must use PHP's
/// uppercase `'E'` exponent marker (`d:1.0E+20;`), matching `serialize`/
/// `var_export` and distinct from `json_encode`'s lowercase `'e'`. Before the
/// `__rt_json_ftoa` exponent-char parameter, the shared formatter emitted `'e'`
/// here, breaking byte-for-byte PHP compatibility. Covers a positive and a
/// negative mantissa, a negative exponent, and a three-digit exponent.
#[test]
fn test_serialize_exponential_floats_use_uppercase_e() {
    let out = compile_and_run(
        r#"<?php
echo serialize(1e20), "\n";
echo serialize(1.5e-10), "\n";
echo serialize(-2.5e-8), "\n";
echo serialize(1e100), "\n";
"#,
    );
    assert_eq!(out, "d:1.0E+20;\nd:1.5E-10;\nd:-2.5E-8;\nd:1.0E+100;\n");
}

/// Regression: an exponential float round-trips through `unserialize` (libc
/// `strtod` accepts the uppercase `E`) and re-`serialize` reproduces PHP's exact
/// bytes, confirming the serialize and unserialize paths agree on `'E'`.
#[test]
fn test_serialize_exponential_float_round_trip() {
    let out = compile_and_run(
        r#"<?php
var_dump(serialize(1.0e20) === "d:1.0E+20;");
var_dump(unserialize("d:1.0E+20;") === 1.0e20);
echo serialize(unserialize("d:1.0E+20;")), "\n";
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nd:1.0E+20;\n");
}

/// Verifies object serialization (Stage A): public/protected/private mangled keys,
/// declaration order, mixed-typed properties, null, nested objects, and objects
/// inside indexed/associative arrays — all byte-exact with the PHP interpreter.
#[test]
fn test_serialize_objects_plain() {
    let out = compile_and_run(
        r#"<?php
class Point { public int $x = 1; protected int $y = 2; private int $z = 3; }
echo serialize(new Point()), "\n";
class Mixed1 { public $a = "hi"; public $b = [1, 2]; public $n = null; public $f = 1.5; }
echo serialize(new Mixed1()), "\n";
class Base { public $base = "B"; }
class Derived extends Base { public $own = "D"; protected $p = 7; }
echo serialize(new Derived()), "\n";
echo serialize([new Point(), "tail"]), "\n";
echo serialize(["k" => new Point()]), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "O:5:\"Point\":3:{s:1:\"x\";i:1;s:4:\"\0*\0y\";i:2;s:8:\"\0Point\0z\";i:3;}\n",
            "O:6:\"Mixed1\":4:{s:1:\"a\";s:2:\"hi\";s:1:\"b\";a:2:{i:0;i:1;i:1;i:2;}s:1:\"n\";N;s:1:\"f\";d:1.5;}\n",
            "O:7:\"Derived\":3:{s:4:\"base\";s:1:\"B\";s:3:\"own\";s:1:\"D\";s:4:\"\0*\0p\";i:7;}\n",
            "a:2:{i:0;O:5:\"Point\":3:{s:1:\"x\";i:1;s:4:\"\0*\0y\";i:2;s:8:\"\0Point\0z\";i:3;}i:1;s:4:\"tail\";}\n",
            "a:1:{s:1:\"k\";O:5:\"Point\":3:{s:1:\"x\";i:1;s:4:\"\0*\0y\";i:2;s:8:\"\0Point\0z\";i:3;}}\n",
        ),
    );
}

/// Verifies `unserialize()` reconstructs objects: a `Point` round-trips with a
/// readable public property and byte-identical re-serialization (proving the
/// protected/private slots survived), mixed-typed and inherited properties
/// restore, and objects nested inside arrays rebuild — all matching PHP.
#[test]
fn test_unserialize_objects_round_trip() {
    let out = compile_and_run(
        r#"<?php
class Point { public int $x = 1; protected int $y = 2; private int $z = 3; }
$s = serialize(new Point());
$o = unserialize($s);
echo $o->x, "\n";
echo (serialize($o) === $s ? "identity" : "DIFF"), "\n";
class Mixed1 { public $a = "hi"; public $b = [1, 2]; public $n = null; public $f = 1.5; }
$m = unserialize(serialize(new Mixed1()));
echo $m->a, "|", $m->b[0], "|", $m->b[1], "|", $m->f, "\n";
class Base { public $base = "B"; }
class Derived extends Base { public $own = "D"; protected $p = 7; }
$d = unserialize(serialize(new Derived()));
echo $d->base, $d->own, "\n";
$arr = unserialize(serialize([new Point(), "tail"]));
echo $arr[0]->x, $arr[1], "\n";
"#,
    );
    assert_eq!(out, "1\nidentity\nhi|1|2|1.5\nBD\n1tail\n");
}

/// Verifies object serialization via the `__serialize()` magic method (Stage C):
/// the object body is the returned array's pairs (hash and indexed returns), the
/// class name still wraps it, an internal string concat survives the concat-buffer
/// rewind, nesting inside an outer array preserves the prefix, and a nested array
/// inside the returned data serializes recursively — all byte-exact with PHP.
#[test]
fn test_serialize_objects_via_serialize_magic() {
    let out = compile_and_run(
        r#"<?php
class P { public int $x = 1; protected int $y = 2; private int $z = 3;
    public function __serialize(): array { return ['x' => $this->x, 'y' => $this->y, 'z' => $this->z]; } }
echo serialize(new P()), "\n";
class Q { public $a = 5; public $b = "hi";
    public function __serialize(): array { return [$this->a, $this->b]; } }
echo serialize(new Q()), "\n";
class C { public $a = "foo"; public $b = "bar";
    public function __serialize(): array { return ['combined' => $this->a . "-" . $this->b, 'len' => 7]; } }
echo serialize(new C()), "\n";
echo serialize(["wrap" => new C(), "after" => "z"]), "\n";
class D { public function __serialize(): array { return ['nested' => [1, 2, 3], 'k' => 'v']; } }
echo serialize(new D()), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "O:1:\"P\":3:{s:1:\"x\";i:1;s:1:\"y\";i:2;s:1:\"z\";i:3;}\n",
            "O:1:\"Q\":2:{i:0;i:5;i:1;s:2:\"hi\";}\n",
            "O:1:\"C\":2:{s:8:\"combined\";s:7:\"foo-bar\";s:3:\"len\";i:7;}\n",
            "a:2:{s:4:\"wrap\";O:1:\"C\":2:{s:8:\"combined\";s:7:\"foo-bar\";s:3:\"len\";i:7;}s:5:\"after\";s:1:\"z\";}\n",
            "O:1:\"D\":2:{s:6:\"nested\";a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}s:1:\"k\";s:1:\"v\";}\n",
        ),
    );
}

/// Verifies object serialization via the legacy `__sleep()` magic method (Stage C):
/// only the named properties are emitted, in `__sleep()`'s order, each written with
/// its PHP-mangled key (public `x`, private `\0S\0z`) — byte-exact with PHP.
#[test]
fn test_serialize_objects_via_sleep_magic() {
    let out = compile_and_run(
        r#"<?php
class S { public int $x = 1; protected int $y = 2; private int $z = 3;
    public function __sleep(): array { return ['x', 'z']; } }
echo serialize(new S()), "\n";
"#,
    );
    assert_eq!(out, "O:1:\"S\":2:{s:1:\"x\";i:1;s:4:\"\0S\0z\";i:3;}\n");
}

/// Verifies object unserialization via the `__unserialize()` magic method (Stage C):
/// the `O:` body is parsed into an associative array and passed to
/// `__unserialize($this, $data)`, which restores the object. Round-trips an int
/// and a string property and re-serializes to byte-identical output.
#[test]
fn test_unserialize_objects_via_unserialize_magic() {
    let out = compile_and_run(
        r#"<?php
class C {
    public $x = 0;
    public $label = "";
    public function __serialize(): array { return ['x' => $this->x, 'label' => $this->label]; }
    public function __unserialize(array $d): void { $this->x = $d['x']; $this->label = $d['label']; }
}
$c = new C(); $c->x = 42; $c->label = "hello";
$s = serialize($c);
$r = unserialize($s);
echo $r->x, "|", $r->label, "\n";
echo (serialize($r) === $s ? "identity" : "DIFF"), "\n";
"#,
    );
    assert_eq!(out, "42|hello\nidentity\n");
}

/// Verifies object unserialization via the legacy `__sleep()`/`__wakeup()` pair
/// (Stage C): `__sleep()` persists a subset of properties, properties restore by
/// name on read, and `__wakeup()` runs afterwards to recompute derived state.
#[test]
fn test_unserialize_objects_via_wakeup_magic() {
    let out = compile_and_run(
        r#"<?php
class S {
    public $x = 1;
    public $tag = "";
    public function __sleep(): array { return ['x']; }
    public function __wakeup(): void { $this->tag = "woke"; }
}
$s = new S(); $s->x = 7; $s->tag = "orig";
$r = unserialize(serialize($s));
echo "x=", $r->x, " tag=", $r->tag, "\n";
class W { public $a = 1; public $b = 2; public $sum = 0;
    public function __wakeup(): void { $this->sum = $this->a + $this->b; } }
$w = new W(); $w->a = 10; $w->b = 20;
$rw = unserialize(serialize($w));
echo $rw->a, " ", $rw->b, " ", $rw->sum, "\n";
"#,
    );
    assert_eq!(out, "x=7 tag=woke\n10 20 30\n");
}

/// Verifies object-identity back-references in `serialize()` (Stage D): a repeated
/// object emits `r:<index>;` using PHP's global value counter (every value,
/// including scalars and the array container, consumes an index; keys do not).
/// Byte-identical to PHP across shared objects and an interleaved scalar.
#[test]
fn test_serialize_object_back_references() {
    let out = compile_and_run(
        r#"<?php
class P { public $v = 0; }
$a = new P(); $a->v = 1;
$b = new P(); $b->v = 2;
echo serialize([$a, $b, $a]), "\n";
echo serialize([1, $a, $a]), "\n";
$c = new P();
echo serialize([$c, $c, $c]), "\n";
echo serialize($a), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "a:3:{i:0;O:1:\"P\":1:{s:1:\"v\";i:1;}i:1;O:1:\"P\":1:{s:1:\"v\";i:2;}i:2;r:2;}\n",
            "a:3:{i:0;i:1;i:1;O:1:\"P\":1:{s:1:\"v\";i:1;}i:2;r:3;}\n",
            "a:3:{i:0;O:1:\"P\":1:{s:1:\"v\";i:0;}i:1;r:2;i:2;r:2;}\n",
            "O:1:\"P\":1:{s:1:\"v\";i:1;}\n",
        ),
    );
}

/// Verifies the `r:` back-reference round-trip on `unserialize()` (Stage D): a
/// repeated object rebuilds as one shared instance (=== identity preserved), both
/// aliases read the same value, and re-serialization reproduces the `r:` structure
/// byte-identically with PHP.
#[test]
fn test_unserialize_object_back_references() {
    let out = compile_and_run(
        r#"<?php
class P { public $v = 0; }
$a = new P(); $a->v = 7;
$arr = unserialize(serialize([$a, $a]));
echo $arr[0]->v, $arr[1]->v, "\n";
echo ($arr[0] === $arr[1] ? "same" : "diff"), "\n";
echo serialize($arr), "\n";
"#,
    );
    assert_eq!(
        out,
        "77\nsame\na:2:{i:0;O:1:\"P\":1:{s:1:\"v\";i:7;}i:1;r:2;}\n",
    );
}
