//! Purpose:
//! Provides end-to-end codegen tests for `serialize()` / `unserialize()` and object-property
//! projections, exercising their runtime helpers through compiled PHP programs.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the serialize codegen test module.
//!
//! Key details:
//! - Output must match PHP's serialize() wire format byte-for-byte for the scalar
//!   subset (null/bool/int/float/string); array support is added in a later increment.
//! - Round-trips go through both helpers so a regression in either is caught.
//! - Object casts and `get_object_vars()` retain PHP visibility, key, and built-in property rules.
//! - Classes PHP refuses to (un)serialize are covered by the `not_serializable` submodule.

use crate::support::*;
use elephc::codegen_support::platform::Target;

#[path = "serialize/not_serializable.rs"]
mod not_serializable;

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

/// Verifies truncated scalar wire values are rejected before any declared
/// length or delimiter can advance the parser beyond the source buffer.
#[test]
fn test_unserialize_rejects_truncated_scalar_wire_values() {
    let out = compile_and_run(
        r#"<?php
echo unserialize('s:100:"A";') === false ? 'false' : 'accepted', "|";
echo unserialize('s:3:"ab";') === false ? 'false' : 'accepted', "|";
echo unserialize('i:123') === false ? 'false' : 'accepted', "|";
echo unserialize('d:1.25') === false ? 'false' : 'accepted';
"#,
    );
    assert_eq!(out, "false|false|false|false");
}

/// Verifies truncated container, reference, null, and boolean encodings fail
/// before nested keys, values, or fixed punctuation are read past the input.
#[test]
fn test_unserialize_rejects_truncated_structural_wire_values() {
    let out = compile_and_run(
        r#"<?php
echo unserialize('a:1:{i:0;s:1:"x";') === false ? 'false' : 'accepted', "|";
echo unserialize('a:1:{i:0;') === false ? 'false' : 'accepted', "|";
echo unserialize('R:1') === false ? 'false' : 'accepted', "|";
echo unserialize('N') === false ? 'false' : 'accepted', "|";
echo unserialize('b:1') === false ? 'false' : 'accepted';
"#,
    );
    assert_eq!(out, "false|false|false|false|false");
}

/// Verifies an object missing its closing delimiter is rejected without
/// invoking lifecycle hooks on a partially parsed instance.
#[test]
fn test_unserialize_does_not_wakeup_truncated_objects() {
    let out = compile_and_run(
        r#"<?php
class Probe {
    public function __wakeup(): void { echo "WAKEUP"; }
}

echo unserialize('O:5:"Probe":0:{') === false ? 'false' : 'accepted';
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies semantically invalid nested values fail the containing object parse
/// without dereferencing a null child box or invoking object lifecycle hooks.
#[test]
fn test_unserialize_rejects_invalid_nested_object_values() {
    let out = compile_and_run(
        r#"<?php
class Plain { public $value; }
class Magic {
    public function __unserialize(array $data): void { echo "HOOK"; }
}

echo unserialize('O:5:"Plain":1:{s:5:"value";d:nope;}') === false ? 'false' : 'accepted', "|";
echo unserialize('O:5:"Magic":1:{s:5:"value";d:nope;}') === false ? 'false' : 'accepted';
"#,
    );
    assert_eq!(out, "false|false");
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

/// Regression: deeply nested `php_serialize` session upload-progress data must
/// retain its array-valued Mixed cell when the nested entry is read and re-encoded.
#[test]
fn test_unserialize_session_upload_progress_payload() {
    let out = compile_and_run(
        r#"<?php
$raw = 'a:1:{s:21:"upload_progress_mykey";a:5:{s:10:"start_time";i:1784207462;s:14:"content_length";i:266;s:15:"bytes_processed";i:266;s:4:"done";b:1;s:5:"files";a:1:{i:0;a:7:{s:10:"field_name";s:1:"f";s:4:"name";s:5:"x.bin";s:8:"tmp_name";s:0:"";s:5:"error";i:0;s:4:"done";b:1;s:10:"start_time";i:1784207462;s:15:"bytes_processed";i:20;}}}}}';
$decoded = unserialize($raw);
echo serialize($decoded['upload_progress_mykey']);
"#,
    );
    assert_eq!(
        out,
        "a:5:{s:10:\"start_time\";i:1784207462;s:14:\"content_length\";i:266;s:15:\"bytes_processed\";i:266;s:4:\"done\";b:1;s:5:\"files\";a:1:{i:0;a:7:{s:10:\"field_name\";s:1:\"f\";s:4:\"name\";s:5:\"x.bin\";s:8:\"tmp_name\";s:0:\"\";s:5:\"error\";i:0;s:4:\"done\";b:1;s:10:\"start_time\";i:1784207462;s:15:\"bytes_processed\";i:20;}}}"
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

/// `serialize()` leaves out every typed property that was never initialized, whatever its
/// visibility, and counts only the ones it writes, as PHP does. Reading such a slot used to print
/// its uninitialized marker as a value, or crash on a string property. The round trip keeps the
/// property uninitialized, so assigning it later serializes it normally. Measured on PHP 8.5.10.
#[test]
fn test_serialize_omits_uninitialized_typed_properties() {
    let out = compile_and_run(
        r#"<?php
class Account {
    private int $id;
    private int $version = 3;
    protected string $owner;
    protected string $label = 'main';
    public ?int $limit = null;
    public array $tags;
    public float $rate;
}
$account = new Account();
echo str_replace("\0", '~', serialize($account)), "\n";
$copy = unserialize(serialize($account));
echo get_class($copy), "\n";
$copy->tags = ['a'];
$copy->rate = 1.5;
echo str_replace("\0", '~', serialize($copy)), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "O:7:\"Account\":3:{s:16:\"~Account~version\";i:3;s:8:\"~*~label\";s:4:\"main\";",
            "s:5:\"limit\";N;}\n",
            "Account\n",
            "O:7:\"Account\":5:{s:16:\"~Account~version\";i:3;s:8:\"~*~label\";s:4:\"main\";",
            "s:5:\"limit\";N;s:4:\"tags\";a:1:{i:0;s:1:\"a\";}s:4:\"rate\";d:1.5;}\n",
        )
    );
}

/// The default walk counts the initialized properties before writing any of them and checks each
/// slot again as it writes it, exactly like php-src's `php_var_serialize_intern`. So when writing
/// an earlier property runs a hook that initializes or unsets a later one, the payload is PHP's own,
/// count and all, even though PHP cannot unserialize it either. Measured on PHP 8.5.10.
#[test]
fn test_serialize_counts_properties_before_writing_them_like_php() {
    let out = compile_and_run(
        r#"<?php
class Outer { public ?Inner $in = null; public int $later; }
class Inner {
    public Outer $outer;
    public function __sleep(): array { $this->outer->later = 5; return []; }
}
$o = new Outer(); $i = new Inner(); $i->outer = $o; $o->in = $i;
echo serialize($o), "\n";
class Outer2 { public ?Inner2 $in = null; public int $later = 3; }
class Inner2 {
    public Outer2 $outer;
    public function __sleep(): array { $outer = $this->outer; unset($outer->later); return []; }
}
$o2 = new Outer2(); $i2 = new Inner2(); $i2->outer = $o2; $o2->in = $i2;
echo serialize($o2), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "O:5:\"Outer\":1:{s:2:\"in\";O:5:\"Inner\":0:{}s:5:\"later\";i:5;}\n",
            "O:6:\"Outer2\":2:{s:2:\"in\";O:6:\"Inner2\":0:{}}\n",
        )
    );
}

/// `__sleep()` writes only the named properties that hold a value, and the count in front of the
/// body covers exactly those: an uninitialized typed property (a `Closure` one included, which
/// must not trigger the Closure refusal) is left out, as in PHP. The count is only known once
/// every name was looked up, so it is written after the body and moved in front of it; nested
/// `__sleep()` objects, a back-reference and a two-digit count keep the payload byte-exact and
/// round-trippable. Measured on PHP 8.5.10.
#[test]
fn test_serialize_sleep_counts_only_initialized_properties() {
    let out = compile_and_run(
        r#"<?php
class Inner {
    public int $id;
    public string $tag = 'in';
    public function __sleep(): array { return ['id', 'tag']; }
}
class Wide {
    public int $p0 = 0; public int $p1 = 1; public int $p2 = 2; public int $p3 = 3;
    public int $p4 = 4; public int $p5 = 5; public int $p6 = 6; public int $p7 = 7;
    public int $p8 = 8; public int $p9 = 9; public int $p10 = 10; public int $gap;
    public Inner $inner;
    public ?Inner $again = null;
    public array $list = [];
    public function __sleep(): array {
        return ['p0', 'p1', 'p2', 'p3', 'p4', 'p5', 'p6', 'p7', 'p8', 'p9', 'p10', 'gap', 'inner', 'again', 'list'];
    }
}
class Task {
    public Closure $callback;
    public int $n = 3;
    public function __sleep(): array { return ['callback', 'n']; }
}
$wide = new Wide();
$wide->inner = new Inner();
$wide->again = $wide->inner;
$wide->list = [new Inner(), 'x'];
$wire = serialize($wide);
echo $wire, "\n";
$copy = unserialize($wire);
echo $copy->p10, " ", $copy->inner->tag, " ", var_export($copy->again === $copy->inner, true), "\n";
echo serialize(new Task()), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "O:4:\"Wide\":14:{s:2:\"p0\";i:0;s:2:\"p1\";i:1;s:2:\"p2\";i:2;s:2:\"p3\";i:3;",
            "s:2:\"p4\";i:4;s:2:\"p5\";i:5;s:2:\"p6\";i:6;s:2:\"p7\";i:7;s:2:\"p8\";i:8;",
            "s:2:\"p9\";i:9;s:3:\"p10\";i:10;s:5:\"inner\";O:5:\"Inner\":1:{s:3:\"tag\";s:2:\"in\";}",
            "s:5:\"again\";r:13;s:4:\"list\";a:2:{i:0;O:5:\"Inner\":1:{s:3:\"tag\";s:2:\"in\";}",
            "i:1;s:1:\"x\";}}\n",
            "10 in true\n",
            "O:4:\"Task\":1:{s:1:\"n\";i:3;}\n",
        )
    );
}

/// `unserialize()` initializes the typed properties it restores, including ones declared without
/// a default: reading them works afterwards, and serializing the copy again reproduces the
/// payload (a still-uninitialized nested property stays left out). The restored slots used to
/// keep their uninitialized marker, so reads failed and a second `serialize()` dropped them.
/// Measured on PHP 8.5.10.
#[test]
fn test_unserialize_initializes_typed_properties_without_defaults() {
    let out = compile_and_run(
        r#"<?php
class Point {
    public int $x;
    public array $tags;
    public Point $next;
    public mixed $m;
    public function __construct() { $this->x = 1; $this->tags = ['a']; $this->m = 'k'; }
}
$p = new Point();
$p->next = new Point();
$wire = serialize($p);
echo $wire, "\n";
$copy = unserialize($wire);
echo $copy->next->x, " ", $copy->tags[0], " ", $copy->m, "\n";
echo serialize($copy) === $wire ? "same\n" : "changed\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "O:5:\"Point\":4:{s:1:\"x\";i:1;s:4:\"tags\";a:1:{i:0;s:1:\"a\";}",
            "s:4:\"next\";O:5:\"Point\":3:{s:1:\"x\";i:1;s:4:\"tags\";a:1:{i:0;s:1:\"a\";}",
            "s:1:\"m\";s:1:\"k\";}s:1:\"m\";s:1:\"k\";}\n",
            "1 a k\n",
            "same\n",
        )
    );
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

/// Verifies `allowed_classes=false` prevents object hydration and suppresses
/// `__wakeup`, matching PHP's `__PHP_Incomplete_Class` safety boundary.
#[test]
fn test_unserialize_allowed_classes_false_blocks_object_hydration() {
    let out = compile_and_run(
        r#"<?php
class GuardedPayload {
    public int $value = 7;
    public function __wakeup(): void { echo "WAKE"; }
}
$wire = serialize(new GuardedPayload());
$value = unserialize($wire, ['allowed_classes' => false]);
echo get_class($value), "\n";
"#,
    );
    assert_eq!(out, "__PHP_Incomplete_Class\n");
}

/// Verifies an `allowed_classes` allow-list hydrates and wakes only the named
/// class while representing every other serialized object as incomplete.
#[test]
fn test_unserialize_allowed_classes_allow_list_is_enforced() {
    let out = compile_and_run(
        r#"<?php
class AllowedPayload {
    public function __wakeup(): void { echo "ALLOWED_WAKE\n"; }
}
class BlockedPayload {
    public function __wakeup(): void { echo "BLOCKED_WAKE\n"; }
}
$wire = serialize([new AllowedPayload(), new BlockedPayload()]);
$values = unserialize($wire, ['allowed_classes' => ['AllowedPayload']]);
echo get_class($values[0]), "|", get_class($values[1]), "\n";
"#,
    );
    assert_eq!(
        out,
        "ALLOWED_WAKE\nAllowedPayload|__PHP_Incomplete_Class\n"
    );
}

/// Verifies `allowed_classes` accepts associative arrays and inspects their
/// values as class names without imposing packed integer keys.
#[test]
fn test_unserialize_allowed_classes_accepts_associative_value_lists() {
    let out = compile_and_run(
        r#"<?php
class AssociativeAllowedPayload { public int $value = 7; }
$wire = serialize(new AssociativeAllowedPayload());
$decoded = unserialize($wire, [
    'allowed_classes' => ['primary' => 'AssociativeAllowedPayload'],
]);
echo get_class($decoded), ':', $decoded->value;
"#,
    );
    assert_eq!(out, "AssociativeAllowedPayload:7");
}

/// Verifies object entries in `allowed_classes` raise PHP's conversion `Error`
/// with the offending class name instead of an allow-list `TypeError`.
#[test]
fn test_unserialize_allowed_classes_object_entry_reports_conversion_error() {
    let out = compile_and_run(
        r#"<?php
class InvalidAllowedClassEntry {}
try {
    unserialize('i:1;', ['allowed_classes' => [new InvalidAllowedClassEntry()]]);
    echo 'NO_ERROR';
} catch (Throwable $e) {
    echo get_class($e), '|', $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Error|Object of class InvalidAllowedClassEntry could not be converted to string"
    );
}

/// Verifies stringable object entries are converted to class names before the
/// `allowed_classes` membership check, matching PHP's object conversion rules.
#[test]
fn test_unserialize_allowed_classes_accepts_stringable_object_entries() {
    let out = compile_and_run(
        r#"<?php
class StringableAllowedPayload { public int $value = 9; }
class AllowedClassName {
    public function __toString(): string { return 'StringableAllowedPayload'; }
}
$wire = serialize(new StringableAllowedPayload());
$decoded = unserialize($wire, ['allowed_classes' => [new AllowedClassName()]]);
echo get_class($decoded), ':', $decoded->value;
"#,
    );
    assert_eq!(out, "StringableAllowedPayload:9");
}

/// Verifies a nested `unserialize()` triggered by an allowed hydration hook
/// cannot replace the outer call's `allowed_classes` policy. The later blocked
/// object must remain incomplete and its hook must never run.
#[test]
fn test_unserialize_allowed_classes_survives_reentrant_wakeup() {
    let out = compile_and_run(
        r#"<?php
class ReentrantAllowedPayload {
    public function __wakeup(): void {
        try { throw new Exception("caught inside hook"); }
        catch (Exception $e) { echo "HOOK_CAUGHT\n"; }
        unserialize('i:1;');
        echo "ALLOWED_WAKE\n";
    }
}
class ReentrantBlockedPayload {
    public function __wakeup(): void { echo "BLOCKED_WAKE\n"; }
}
$wire = serialize([new ReentrantAllowedPayload(), new ReentrantBlockedPayload()]);
$values = unserialize($wire, ['allowed_classes' => ['ReentrantAllowedPayload']]);
echo get_class($values[0]), "|", get_class($values[1]), "\n";
"#,
    );
    assert_eq!(
        out,
        "HOOK_CAUGHT\nALLOWED_WAKE\nReentrantAllowedPayload|__PHP_Incomplete_Class\n"
    );
}

/// Verifies the options operand is type-checked before runtime hash access,
/// matching PHP's `TypeError` for a scalar second argument.
#[test]
fn test_unserialize_rejects_scalar_options_without_memory_access() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Payload {}
$wire = serialize(new Payload());
unserialize($wire, "not-an-array");
"#,
    );
    assert!(
        err.contains("Argument #2") && err.contains("array"),
        "expected the PHP-compatible options TypeError, got: {err}"
    );
}

/// Verifies a scalar hidden behind a runtime `mixed` value is tag-checked before
/// `__rt_hash_get`, not only rejected when its AST type is statically obvious.
#[test]
fn test_unserialize_rejects_runtime_mixed_scalar_options() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function runtime_options(int $mode): mixed {
    if ($mode > 0) { return "not-an-array"; }
    return [];
}
class Payload {}
$wire = serialize(new Payload());
unserialize($wire, runtime_options($argc));
"#,
    );
    assert!(
        err.contains("Argument #2") && err.contains("array"),
        "expected runtime options tag validation, got: {err}"
    );
}

/// Verifies a caught TypeError for statically invalid options closes the
/// unserialize context, so later fiber suspension and parsing still work.
#[test]
fn test_unserialize_static_options_type_error_cleans_runtime_state() {
    let out = compile_and_run(
        r#"<?php
try {
    unserialize('i:1;', 42);
    echo "NO_TYPE_ERROR|";
} catch (TypeError $e) {
    echo "TYPE_ERROR|";
}

$fiber = new Fiber(function(): void {
    Fiber::suspend("READY");
});
try {
    echo $fiber->start(), "|";
} catch (FiberError $e) {
    echo "POISONED:", $e->getMessage(), "|";
}
echo unserialize('i:2;');
"#,
    );
    assert_eq!(out, "TYPE_ERROR|READY|2");
}

/// Verifies options hidden behind `mixed` raise the same catchable TypeError as
/// statically invalid options and leave the next unserialize call operational.
#[test]
fn test_unserialize_runtime_mixed_options_type_error_is_catchable() {
    let out = compile_and_run(
        r#"<?php
function runtime_invalid_options(int $mode): mixed {
    if ($mode > 0) { return 42; }
    return [];
}

try {
    unserialize('i:1;', runtime_invalid_options($argc));
    echo "NO_TYPE_ERROR|";
} catch (TypeError $e) {
    echo "TYPE_ERROR|";
}
echo unserialize('i:2;');
"#,
    );
    assert_eq!(out, "TYPE_ERROR|2");
}

/// Verifies an invalid `allowed_classes` policy raises a catchable TypeError
/// and releases its unserialize context before the next parser invocation.
#[test]
fn test_unserialize_allowed_classes_type_error_is_catchable() {
    let out = compile_and_run(
        r#"<?php
try {
    unserialize('i:1;', ['allowed_classes' => 'Payload']);
    echo "NO_TYPE_ERROR|";
} catch (TypeError $e) {
    echo "TYPE_ERROR|";
}
echo unserialize('i:2;');
"#,
    );
    assert_eq!(out, "TYPE_ERROR|2");
}

/// Verifies invalid options, policies, and allow-list entries report PHP's exact
/// catchable TypeError messages, including the offending runtime type.
#[test]
fn test_unserialize_option_type_errors_match_php_messages() {
    let out = compile_and_run(
        r#"<?php
function runtime_invalid_unserialize_options(): mixed { return 42; }

try { unserialize('i:1;', 42); }
catch (TypeError $e) { echo $e->getMessage(), "\n"; }

try { unserialize('i:1;', runtime_invalid_unserialize_options()); }
catch (TypeError $e) { echo $e->getMessage(), "\n"; }

try { unserialize('i:1;', ['allowed_classes' => 'Payload']); }
catch (TypeError $e) { echo $e->getMessage(), "\n"; }

try { unserialize('i:1;', ['allowed_classes' => ['stdClass', 42]]); }
catch (TypeError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(
        out,
        concat!(
            "unserialize(): Argument #2 ($options) must be of type array, int given\n",
            "unserialize(): Argument #2 ($options) must be of type array, int given\n",
            "unserialize(): Option \"allowed_classes\" must be of type array|bool, string given\n",
            "unserialize(): Option \"allowed_classes\" must be an array of class names, int given",
        )
    );
}

/// Verifies empty indexed options arrays are accepted both with a concrete
/// array type and when boxed behind `mixed`, without calling the hash runtime
/// on an indexed-array payload.
#[test]
fn test_unserialize_accepts_empty_indexed_options_arrays() {
    let out = compile_and_run(
        r#"<?php
class Payload {}
function runtime_options(): mixed { return []; }
$wire = serialize(new Payload());
echo get_class(unserialize($wire, [])), "\n";
echo get_class(unserialize($wire, runtime_options())), "\n";
"#,
    );
    assert_eq!(out, "Payload\nPayload\n");
}

/// Verifies every `allowed_classes` allow-list element is validated as a class
/// name before the runtime can interpret scalar payload bytes as pointers.
#[test]
fn test_unserialize_rejects_non_string_allowed_class_entries() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Payload {}
$wire = serialize(new Payload());
unserialize($wire, ['allowed_classes' => [1, 2, 3]]);
"#,
    );
    assert!(
        err.contains("allowed_classes") && err.contains("class names"),
        "expected a controlled allow-list TypeError, got: {err}"
    );
}

/// Verifies an invalid scalar `allowed_classes` value fails closed instead of
/// silently reverting to the allow-all policy.
#[test]
fn test_unserialize_rejects_scalar_allowed_classes_policy() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Payload {}
$wire = serialize(new Payload());
unserialize($wire, ['allowed_classes' => 'Payload']);
"#,
    );
    assert!(
        err.contains("allowed_classes") && err.contains("array|bool"),
        "expected a controlled allowed_classes TypeError, got: {err}"
    );
}

/// Verifies the linux-x86_64 allow-list scan derives its 16-byte string-cell
/// offset with encodable instructions instead of an invalid x86 scale factor.
#[test]
fn test_unserialize_x86_64_allowed_classes_uses_encodable_string_stride() {
    let target = Target::parse("linux-x86_64").expect("linux-x86_64 is a supported target");
    let runtime_asm = elephc::codegen::generate_runtime(8_388_608, target);

    for expected in [
        "mov rax, r10",
        "shl rax, 4",
        "add r11, rax",
        "add r11, 24",
    ] {
        assert!(
            runtime_asm.contains(expected),
            "x86_64 unserialize allow-list scan is missing {expected}"
        );
    }
    assert!(
        !runtime_asm.contains("lea r11, [r11 + r10 * 16 + 24]"),
        "x86_64 unserialize emitted an invalid scale-16 address operand"
    );
}

/// Verifies allowed-class scans extract the array element tag from header bits 8 through 14.
#[test]
fn test_unserialize_allowed_classes_extracts_the_encoded_element_tag() {
    let arm_target = Target::parse("macos-aarch64").expect("macos-aarch64 is supported");
    let arm_runtime = elephc::codegen::generate_runtime(8_388_608, arm_target);
    assert!(
        arm_runtime.contains("ubfx x11, x11, #8, #7"),
        "AArch64 allow-list scan does not extract the encoded element tag"
    );

    let x86_target = Target::parse("linux-x86_64").expect("linux-x86_64 is supported");
    let x86_runtime = elephc::codegen::generate_runtime(8_388_608, x86_target);
    assert!(
        x86_runtime.contains("shr r11, 8") && x86_runtime.contains("and r11, 0x7f"),
        "x86_64 allow-list scan does not extract the encoded element tag"
    );
}

/// Verifies the linux-x86_64 incomplete-object allocator materializes the
/// full-width heap marker in a register before storing it into memory.
#[test]
fn test_unserialize_x86_64_incomplete_object_uses_encodable_heap_marker_store() {
    let target = Target::parse("linux-x86_64").expect("linux-x86_64 is a supported target");
    let runtime_asm = elephc::codegen::generate_runtime(8_388_608, target);

    assert!(
        runtime_asm.contains("mov r10, 0x454c504800000004"),
        "x86_64 incomplete object is missing its full-width heap marker"
    );
    assert!(
        runtime_asm.contains("mov QWORD PTR [rax - 8], r10"),
        "x86_64 incomplete object does not store the materialized heap marker"
    );
    assert!(
        !runtime_asm.contains("mov QWORD PTR [rax - 8], 0x454c504800000004"),
        "x86_64 incomplete object emitted an unencodable imm64 memory store"
    );
}

/// Verifies a dynamically computed but valid string allow-list is accepted;
/// eager validation must not reject every non-literal policy expression.
#[test]
fn test_unserialize_accepts_runtime_string_allowed_class_list() {
    let out = compile_and_run(
        r#"<?php
class Payload { public int $value = 7; }
function runtime_allow_list(): array { return ['Payload']; }
$wire = serialize(new Payload());
$decoded = unserialize($wire, ['allowed_classes' => runtime_allow_list()]);
echo get_class($decoded), ':', $decoded->value;
"#,
    );
    assert_eq!(out, "Payload:7");
}

/// Verifies a dynamically computed integer allow-list is rejected from its
/// runtime array value-type tag before any element is read as string storage.
#[test]
fn test_unserialize_rejects_runtime_integer_allowed_class_list() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Payload {}
function runtime_allow_list(): array { return [1, 2, 3]; }
$wire = serialize(new Payload());
unserialize($wire, ['allowed_classes' => runtime_allow_list()]);
"#,
    );
    assert!(
        err.contains("allowed_classes") && err.contains("class names"),
        "expected a controlled runtime allow-list TypeError, got: {err}"
    );
}

/// Verifies a dynamically computed heterogeneous allow-list validates every
/// boxed Mixed element instead of assuming the string pointer/length layout.
#[test]
fn test_unserialize_rejects_runtime_mixed_allowed_class_list() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Payload {}
function runtime_allow_list(): array { return ['Payload', 1]; }
$wire = serialize(new Payload());
unserialize($wire, ['allowed_classes' => runtime_allow_list()]);
"#,
    );
    assert!(
        err.contains("allowed_classes") && err.contains("class names"),
        "expected per-element runtime allow-list validation, got: {err}"
    );
}

/// Verifies blocked objects retain their serialized properties and original
/// class name when re-serialized as `__PHP_Incomplete_Class`.
#[test]
fn test_unserialize_incomplete_class_preserves_wire_properties() {
    let out = compile_and_run(
        r#"<?php
class Payload { public int $value = 7; }
$wire = serialize(new Payload());
$blocked = unserialize($wire, ['allowed_classes' => false]);
echo serialize($blocked);
"#,
    );
    assert_eq!(out, "O:7:\"Payload\":1:{s:5:\"value\";i:7;}");
}

/// Verifies incomplete-object properties are re-serialized semantically so
/// nested back-reference indices are rebased to the new outer value graph.
#[test]
fn test_unserialize_incomplete_class_rebases_nested_references() {
    let out = compile_and_run(
        r#"<?php
class Child { public int $v = 1; }
class Payload { public mixed $first; public mixed $again; }
$child = new Child();
$payload = new Payload();
$payload->first = $child;
$payload->again = $child;
$blocked = unserialize(serialize($payload), ['allowed_classes' => false]);
echo serialize([$blocked]);
"#,
    );
    assert_eq!(
        out,
        "a:1:{i:0;O:7:\"Payload\":2:{s:5:\"first\";O:5:\"Child\":1:{s:1:\"v\";i:1;}s:5:\"again\";r:3;}}"
    );
}

/// Verifies releasing a blocked object uses its synthetic payload layout rather
/// than indexing class metadata with the reserved class id `-2`.
#[test]
fn test_unserialize_incomplete_class_can_be_destroyed_safely() {
    let out = compile_and_run(
        r#"<?php
class Payload { public int $value = 7; }
$blocked = unserialize(
    'O:7:"Payload":1:{s:5:"value";i:7;}',
    ['allowed_classes' => false]
);
unset($blocked);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies PHP-visible object introspection exposes the original class name
/// and retained properties of an `__PHP_Incomplete_Class` value.
#[test]
fn test_unserialize_incomplete_class_exposes_retained_properties() {
    let out = compile_and_run(
        r#"<?php
class Payload { public int $first = 42; }
$blocked = unserialize(serialize(new Payload()), ['allowed_classes' => false]);
$cast = (array) $blocked;
$vars = get_object_vars($blocked);
echo $cast['__PHP_Incomplete_Class_Name'], "|", $cast['first'], "|";
echo $vars['__PHP_Incomplete_Class_Name'], "|", $vars['first'];
"#,
    );
    assert_eq!(out, "Payload|42|Payload|42");
}

/// Verifies the native builtin keeps PHP's case-insensitive lookup and
/// namespace fallback while exposing ordinary public object properties.
#[test]
fn test_get_object_vars_is_case_insensitive_with_namespace_fallback() {
    let out = compile_and_run(
        r#"<?php
namespace AuditFixture;
class Payload { public int $value = 7; }
$vars = GET_OBJECT_VARS(new Payload());
echo $vars['value'];
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies native `get_object_vars()` follows PHP's lexical visibility for
/// global, child-class, and parent-class call sites.
#[test]
fn test_get_object_vars_respects_lexical_class_scope() {
    let out = compile_and_run(
        r#"<?php
class BaseProfile {
    private int $basePrivate = 1;
    protected int $baseProtected = 2;
    public int $basePublic = 3;

    public function baseView(): string {
        $vars = get_object_vars($this);
        ksort($vars);
        return implode(',', array_keys($vars));
    }
}

class ChildProfile extends BaseProfile {
    private int $childPrivate = 4;
    protected int $childProtected = 5;
    public int $childPublic = 6;

    public function childView(): string {
        $vars = get_object_vars($this);
        ksort($vars);
        return implode(',', array_keys($vars));
    }
}

$profile = new ChildProfile();
$global = get_object_vars($profile);
ksort($global);
echo implode(',', array_keys($global)), "\n";
echo $profile->childView(), "\n";
echo $profile->baseView();
"#,
    );
    assert_eq!(
        out,
        "basePublic,childPublic\nbaseProtected,basePublic,childPrivate,childProtected,childPublic\nbasePrivate,baseProtected,basePublic,childProtected,childPublic"
    );
}

/// Verifies protected properties are visible between sibling subclasses when
/// their lexical scope descends from the property's declaring base class.
#[test]
fn test_get_object_vars_uses_protected_property_declaring_class_scope() {
    let out = compile_and_run(
        r#"<?php
class SharedBase {
    protected string $shared = 'visible';
}

class Inspector extends SharedBase {
    public static function inspect(SharedBase $object): void {
        $vars = get_object_vars($object);
        ksort($vars);
        echo implode(',', array_keys($vars));
    }
}

class Sibling extends SharedBase {}
Inspector::inspect(new Sibling());
"#,
    );
    assert_eq!(out, "shared");
}

/// Verifies a closure declared inside a method inherits that method's class
/// scope when `get_object_vars()` projects private and protected properties.
#[test]
fn test_get_object_vars_closure_inherits_method_lexical_scope() {
    let out = compile_and_run(
        r#"<?php
class ScopedBox {
    private string $private = 'p';
    protected string $protected = 'q';
    public string $public = 'r';

    public function inspect(): void {
        $inspect = function (): void {
            $vars = get_object_vars($this);
            ksort($vars);
            echo implode(',', array_keys($vars));
        };
        $inspect();
    }
}

(new ScopedBox())->inspect();
"#,
    );
    assert_eq!(out, "private,protected,public");
}

/// Verifies numeric-string dynamic property names retain PHP's integer hash
/// keys when projected by `get_object_vars()` or an object-to-array cast.
#[test]
fn test_object_projection_preserves_numeric_dynamic_property_keys() {
    let out = compile_and_run(
        r#"<?php
$object = new stdClass();
$object->{'9'} = 'nine';
echo serialize(get_object_vars($object)), "\n";
echo serialize((array) $object);
"#,
    );
    assert_eq!(
        out,
        "a:1:{i:9;s:4:\"nine\";}\na:1:{i:9;s:4:\"nine\";}"
    );
}

/// User-defined object casts must continue to retain visibility-mangled private properties.
#[test]
fn test_user_object_array_cast_keeps_private_mangled_property() {
    let out = compile_and_run(
        r#"<?php
class CastProperties {
    private string $hidden = 'secret';
    public string $name = 'public';
}
$cast = (array) new CastProperties();
echo count($cast), '|';
echo array_key_exists("\0CastProperties\0hidden", $cast) ? '1' : '0', '|';
echo array_key_exists('name', $cast) ? '1' : '0';
"#,
    );
    assert_eq!(out, "2|1|1");
}

/// Builtin Reflection objects expose PHP's public `ReflectionParameter::$name` in array casts
/// and get_object_vars(), while their compiler-only `__*` backing slots stay invisible.
#[test]
fn test_reflection_parameter_array_cast_exposes_only_public_name() {
    let out = compile_and_run(
        r#"<?php
$parameter = new ReflectionParameter("strlen", "string");
$cast = (array) $parameter;
$vars = get_object_vars($parameter);
echo serialize($cast), "|", count($vars), "|", $vars["name"], "|", $parameter->name, "|";
foreach ($cast as $value) {
    if (is_object($value)) { echo get_class($value), ":", (string) $value, "|"; }
}
echo "end";
"#,
    );
    assert_eq!(out, "a:1:{s:4:\"name\";s:6:\"string\";}|1|string|string|end");
}

/// Reflection objects deny serialization with a catchable Exception, as PHP does.
#[test]
fn test_serialize_reflection_parameter_throws_exception() {
    let out = compile_and_run(
        r#"<?php
$parameter = new ReflectionParameter("strlen", "string");
try {
    serialize($parameter);
    echo 'serialized';
} catch (Exception $error) {
    echo get_class($error), ':', $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Exception:Serialization of 'ReflectionParameter' is not allowed"
    );
}

/// The denial holds wherever the serializer meets a Reflection object: behind `mixed`, nested at
/// any depth in an array, and on a subclass that declares its own `__serialize()`, which PHP
/// still refuses. A plain array is untouched. Expected output measured on PHP 8.5.10.
#[test]
fn test_serialize_denies_reflection_objects_wherever_they_sit() {
    let out = compile_and_run(
        r#"<?php
class Mine extends ReflectionClass {
    public function __serialize(): array { return ['x' => 1]; }
}
function attempt(string $label, mixed $value): void {
    try {
        echo $label, ": ", serialize($value), "\n";
    } catch (Exception $e) {
        echo $label, ": ", get_class($e), " ", $e->getMessage(), "\n";
    }
}
function f(int $a) {}
$param = (new ReflectionFunction('f'))->getParameters()[0];
attempt("direct", $param);
attempt("nested", [1, $param]);
attempt("deep", ['k' => [new ReflectionFunction('f')]]);
attempt("method", new ReflectionMethod('Mine', '__serialize'));
attempt("subclass", new Mine('Mine'));
attempt("plain", [1, 'a' => true]);
try { echo serialize([$param]); } catch (Exception $e) { echo "typed-array: ", $e->getMessage(), "\n"; }
echo "after\n";
"#,
    );
    assert_eq!(
        out,
        "direct: direct: Exception Serialization of 'ReflectionParameter' is not allowed\n\
         nested: nested: Exception Serialization of 'ReflectionParameter' is not allowed\n\
         deep: deep: Exception Serialization of 'ReflectionFunction' is not allowed\n\
         method: method: Exception Serialization of 'ReflectionMethod' is not allowed\n\
         subclass: subclass: Exception Serialization of 'Mine' is not allowed\n\
         plain: a:2:{i:0;i:1;s:1:\"a\";b:1;}\n\
         typed-array: Serialization of 'ReflectionParameter' is not allowed\n\
         after\n"
    );
}

/// The same denial covers the other internal classes PHP marks not serializable that elephc
/// models as objects: `Generator`, `Fiber`, and `SplFileInfo` with its subclasses. Expected
/// output measured on PHP 8.5.10.
#[test]
fn test_serialize_denies_generators_fibers_and_file_info() {
    let out = compile_and_run(
        r#"<?php
function gen() { yield 1; }
$cases = [
    'generator' => gen(),
    'fiber' => new Fiber(fn() => 1),
    'splfileinfo' => new SplFileInfo(__FILE__),
    'splfileobject' => new SplFileObject(__FILE__),
];
foreach ($cases as $label => $value) {
    try { echo $label, ": ", serialize($value), "\n"; }
    catch (Exception $e) { echo get_class($e), " ", $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(
        out,
        "generator: Exception Serialization of 'Generator' is not allowed\n\
         fiber: Exception Serialization of 'Fiber' is not allowed\n\
         splfileinfo: Exception Serialization of 'SplFileInfo' is not allowed\n\
         splfileobject: Exception Serialization of 'SplFileObject' is not allowed\n"
    );
}

/// Verifies an array cast of a runtime `mixed` value preserves PHP semantics
/// for scalar, null, and already-array payload tags.
#[test]
fn test_mixed_array_cast_dispatches_non_object_runtime_tags() {
    let out = compile_and_run(
        r#"<?php
function runtime_value(int $case): mixed {
    if ($case === 0) { return 7; }
    if ($case === 1) { return null; }
    if ($case === 2) { return 'x'; }
    return [4, 5];
}

for ($case = 0; $case < 4; $case++) {
    echo serialize((array) runtime_value($case)), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "a:1:{i:0;i:7;}\n",
            "a:0:{}\n",
            "a:1:{i:0;s:1:\"x\";}\n",
            "a:2:{i:0;i:4;i:1;i:5;}\n",
        )
    );
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

/// Verifies object boxes enter the value registry before their bodies are
/// decoded, preserving direct self-references for both hydration paths.
#[test]
fn test_unserialize_object_self_references() {
    let out = compile_and_run(
        r#"<?php
class Plain { public $self; }
$plain = unserialize('O:5:"Plain":1:{s:4:"self";r:1;}');
echo serialize($plain), "|";

class Magic {
    public $self;
    public function __unserialize(array $data): void { $this->self = $data['self']; }
}
$magic = unserialize('O:5:"Magic":1:{s:4:"self";r:1;}');
echo $magic->self === $magic ? "magic-same" : "magic-diff";
"#,
    );
    assert_eq!(out, "O:5:\"Plain\":1:{s:4:\"self\";r:1;}|magic-same");
}

/// Verifies unknown serialized classes use PHP's incomplete-object container
/// and can still resolve references to the object currently being decoded.
#[test]
fn test_unserialize_unknown_class_becomes_incomplete_object() {
    let out = compile_and_run(
        r#"<?php
$value = unserialize('O:7:"Missing":1:{s:4:"self";r:1;}');
echo get_class($value), "|", serialize($value);
"#,
    );
    assert_eq!(
        out,
        "__PHP_Incomplete_Class|O:7:\"Missing\":1:{s:4:\"self\";r:1;}",
    );
}

/// `unserialize()` of a null into a boxed-Mixed property (untyped, `?string`, `?float`) stores the
/// parsed null cell like any other value.
///
/// It stored the in-band `NULL_SENTINEL` word instead, which is not a Mixed cell pointer, so the
/// first reader of the property — `=== null`, `var_dump`, `json_encode` — dereferenced it and the
/// program segfaulted.
#[test]
fn test_unserialize_null_into_mixed_properties() {
    let out = compile_and_run(
        r#"<?php
class P { public $n = 5; public ?string $m = "a"; public ?float $f = 1.5; }
$p = new P();
$p->n = $argc > 5 ? 1 : null;
$p->m = $argc > 5 ? "z" : null;
$p->f = $argc > 5 ? 2.5 : null;
$u = unserialize(serialize($p));
var_dump($u->n === null, $u->m === null, isset($u->f), $u->n ?? "dflt");
echo json_encode($u), "\n";
var_dump($u);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "bool(false)\n",
            "string(4) \"dflt\"\n",
            "{\"n\":null,\"m\":null,\"f\":null}\n",
            "object(P)#2 (3) {\n",
            "  [\"n\"]=>\n",
            "  NULL\n",
            "  [\"m\"]=>\n",
            "  NULL\n",
            "  [\"f\"]=>\n",
            "  NULL\n",
            "}\n",
        )
    );
}

/// `unserialize()` into a tagged `?int` property stores an int and a null as themselves, and
/// raises PHP's `TypeError` for a string, float, bool or array instead of writing the wrong
/// payload (#1629). The heap is not asserted clean: `unserialize()` of an object leaks on main
/// whatever the property type (#1562).
#[test]
fn test_unserialize_mismatched_value_into_nullable_int_property_raises_type_error() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public ?int $n = null; }
$out = "";
for ($i = 0; $i < 20 + $argc; $i++) {
    $out = "";
    foreach (['i:7;', 'N;', 's:3:"abc";', 'd:1.5;', 'b:1;', 'a:0:{}'] as $payload) {
        try {
            $o = unserialize('O:1:"C":1:{s:1:"n";' . $payload . '}');
            $out .= json_encode($o->n) . " ";
        } catch (Throwable $error) {
            $out .= get_class($error) . " ";
        }
    }
}
echo $out, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "7 null TypeError TypeError TypeError TypeError \n");
}

/// The exact wording of PHP 8.4's typed-property hydration `TypeError` for every mismatched
/// scalar, array and null, plus the values PHP accepts (int and null for `?int`, int for
/// `?float`).
#[test]
fn test_unserialize_typed_property_type_error_matches_php() {
    let out = compile_and_run(
        r#"<?php
class C { public ?int $n = null; }
class F { public ?float $f = null; }
class S { public ?string $s = null; }
class B { public ?bool $b = null; }
$cases = [
    ['C', 's:3:"abc";'], ['C', 's:1:"5";'], ['C', 'd:1.5;'], ['C', 'd:2;'], ['C', 'b:1;'],
    ['C', 'i:7;'], ['C', 'N;'], ['F', 'i:3;'], ['F', 's:3:"1.5";'], ['S', 'i:9;'],
    ['B', 'i:1;'], ['C', 'a:0:{}'],
];
foreach ($cases as [$cls, $payload]) {
    $prop = ['C' => 'n', 'F' => 'f', 'S' => 's', 'B' => 'b'][$cls];
    $data = 'O:1:"' . $cls . '":1:{s:1:"' . $prop . '";' . $payload . '}';
    try {
        $o = unserialize($data);
        echo $cls, " ", $payload, " => ", json_encode($o->$prop), "\n";
    } catch (Throwable $e) {
        echo $cls, " ", $payload, " => ", get_class($e), ": ", $e->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "C s:3:\"abc\"; => TypeError: Cannot assign string to property C::$n of type ?int\n",
            "C s:1:\"5\"; => TypeError: Cannot assign string to property C::$n of type ?int\n",
            "C d:1.5; => TypeError: Cannot assign float to property C::$n of type ?int\n",
            "C d:2; => TypeError: Cannot assign float to property C::$n of type ?int\n",
            "C b:1; => TypeError: Cannot assign true to property C::$n of type ?int\n",
            "C i:7; => 7\n",
            "C N; => null\n",
            "F i:3; => 3\n",
            "F s:3:\"1.5\"; => TypeError: Cannot assign string to property F::$f of type ?float\n",
            "S i:9; => TypeError: Cannot assign int to property S::$s of type ?string\n",
            "B i:1; => TypeError: Cannot assign int to property B::$b of type ?bool\n",
            "C a:0:{} => TypeError: Cannot assign array to property C::$n of type ?int\n",
        )
    );
}

/// The message names a non-nullable type without the `?`, resolves a mismatched object's
/// concrete class, and spells a private property by its plain name rather than the NUL-mangled
/// wire key. The values PHP accepts still hydrate.
#[test]
fn test_unserialize_typed_property_type_error_names_declared_type_and_plain_property() {
    let out = compile_and_run(
        r#"<?php
class P { public int $i = 0; protected string $s = ""; private ?bool $b = null; }
class O { public ?stdClass $o = null; }
class A { public array $a = []; }
function show(string $cls, string $key, string $payload): void {
    try {
        $o = unserialize('O:1:"' . $cls . '":1:{' . $key . $payload . '}');
        echo $cls, " => ok\n";
    } catch (Throwable $e) {
        echo $cls, " => ", $e->getMessage(), "\n";
    }
}
show('P', 's:1:"i";', 'N;');
show('P', 's:4:"' . "\0" . '*' . "\0" . 's";', 'i:5;');
show('P', 's:4:"' . "\0" . 'P' . "\0" . 'b";', 'i:1;');
show('P', 's:4:"' . "\0" . 'P' . "\0" . 'b";', 'b:0;');
show('O', 's:1:"o";', 'a:0:{}');
show('O', 's:1:"o";', 'O:8:"stdClass":0:{}');
show('A', 's:1:"a";', 'i:3;');
show('A', 's:1:"a";', 'a:1:{i:0;i:1;}');
"#,
    );
    assert_eq!(
        out,
        concat!(
            "P => Cannot assign null to property P::$i of type int\n",
            "P => Cannot assign int to property P::$s of type string\n",
            "P => Cannot assign int to property P::$b of type ?bool\n",
            "P => ok\n",
            "O => Cannot assign array to property O::$o of type ?stdClass\n",
            "O => ok\n",
            "A => Cannot assign int to property A::$a of type array\n",
            "A => ok\n",
        )
    );
}

/// PHP widens an `int` into a `float` property on hydration, for both the nullable (`?float`,
/// boxed storage) and non-nullable (`float`, inline slot) shapes, so the stored value must be a
/// real float rather than the integer bits reinterpreted as a double.
#[test]
fn test_unserialize_widens_int_into_float_properties() {
    let out = compile_and_run(
        r#"<?php
class F { public ?float $f = null; }
class G { public float $g = 0.0; }
$f = unserialize('O:1:"F":1:{s:1:"f";i:3;}');
$g = unserialize('O:1:"G":1:{s:1:"g";i:3;}');
var_dump($f->f, $g->g);
var_dump(is_float($f->f), is_float($g->g));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "float(3)\n",
            "float(3)\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}

/// PHP names the class that DECLARES a typed property in the hydration `TypeError`, not the
/// concrete class being hydrated, so an inherited property reports the parent.
#[test]
fn test_unserialize_typed_property_type_error_names_the_declaring_class() {
    let out = compile_and_run(
        r#"<?php
class Parent1 { public int $p = 0; }
class Child1 extends Parent1 {}
try {
    unserialize('O:6:"Child1":1:{s:1:"p";s:3:"abc";}');
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Cannot assign string to property Parent1::$p of type int\n"
    );
}

/// An untyped property (`public $x = 5;`) has no PHP declaration to enforce: elephc infers an
/// `int` slot from the default, but PHP accepts any value there, so hydration must not raise a
/// `TypeError` naming a declaration the source never wrote.
#[test]
fn test_unserialize_untyped_property_with_inferred_default_accepts_any_value() {
    let out = compile_and_run(
        r#"<?php
class PU { public $x = 5; }
$u = unserialize('O:2:"PU":1:{s:1:"x";s:3:"abc";}');
echo "ok\n";
"#,
    );
    assert_eq!(out, "ok\n");
}

/// PHP coerces an `int` into a `float` member when the type has a `float` member and no `int`
/// member (`float|string`), but keeps the `int` when the type also declares `int` (`int|float`).
#[test]
fn test_unserialize_widens_int_into_a_float_union_member() {
    let out = compile_and_run(
        r#"<?php
class FS { public float|string $fs = 0; }
class Num { public int|float $num = 0; }
$fs = unserialize('O:2:"FS":1:{s:2:"fs";i:1;}');
$num = unserialize('O:3:"Num":1:{s:3:"num";i:1;}');
var_dump($fs->fs, $num->num);
var_dump(is_float($fs->fs), is_int($num->num));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "float(1)\n",
            "int(1)\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}

/// PHP prints union members in its canonical order (string, int, float), not the source order,
/// so the hydration `TypeError` text matches reference PHP regardless of how the union is written.
#[test]
fn test_unserialize_type_error_uses_php_canonical_union_order() {
    let out = compile_and_run(
        r#"<?php
class A { public int|float $x = 0; }
class B { public float|string $y = 0; }
try {
    unserialize('O:1:"A":1:{s:1:"x";s:3:"abc";}');
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
try {
    unserialize('O:1:"B":1:{s:1:"y";a:0:{}}');
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Cannot assign string to property A::$x of type int|float\n",
            "Cannot assign array to property B::$y of type string|float\n",
        )
    );
}

/// A `false` payload is spelled `false` (not `bool`), and an object rejected by a scalar slot
/// names its concrete class — the case that resolves the class name before releasing the box.
#[test]
fn test_unserialize_type_error_spells_false_and_object() {
    let out = compile_and_run(
        r#"<?php
class C { public ?int $n = null; }
try {
    unserialize('O:1:"C":1:{s:1:"n";b:0;}');
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
try {
    unserialize('O:1:"C":1:{s:1:"n";O:8:"stdClass":0:{}}');
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Cannot assign false to property C::$n of type ?int\n",
            "Cannot assign stdClass to property C::$n of type ?int\n",
        )
    );
}
