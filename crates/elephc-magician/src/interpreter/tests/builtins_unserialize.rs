//! Purpose:
//! Interpreter tests for the `unserialize()` builtin: scalars, arrays, and -- FIRST, not last,
//! per PLAN.md's own warning that a wrong binding here becomes a wrong cache-freshness decision
//! with no crash -- an object payload whose UNMANGLED property keys must land in the DECLARED
//! private/protected slots a real class metadata lookup finds, not in new dynamic public ones.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_unserialize`.
//!
//! Key details:
//! - Every expected value is `php -n` 8.5.6's own output on the same fragment, re-measured in
//!   this session (`scratchpad/ser_verify2.php`) rather than copied from an unverified spec.
//! - A private/protected property can only be read legitimately from OUTSIDE `var_dump`/
//!   reflection by a method declared ON the class, so each object test calls a `dump()` method
//!   defined on the fixture class -- this is what actually distinguishes "bound the declared
//!   slot" from "created a same-named dynamic public property beside a still-default private
//!   one": the latter would make `dump()` keep reading the OLD default, not the payload's value.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse unserialize() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute unserialize() fragment");
    values.output.clone()
}

/// Runs one fragment and returns both its output and any recorded warnings.
fn out_with_warnings(fragment: &[u8]) -> (String, Vec<String>) {
    let program = parse_fragment(fragment).expect("parse unserialize() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute unserialize() fragment");
    (values.output.clone(), values.warnings.clone())
}

/// THE FIRST TEST, PLAN.md's own priority: an unmangled `O:` payload key for a DECLARED private
/// property must bind the private slot itself, readable only from a method ON the class -- not a
/// same-named dynamic public property beside a still-default private one.
///
/// `php -n` 8.5.6: `object(P)#1 (3) { ["a":"P":private]=> string(2) "AA" ["b":protected]=>
/// int(9) ["c"]=> bool(true) }`; this test reads the same three values via a class method,
/// echoing `AA|9|1` (php echoes `true` as `"1"`).
#[test]
fn unserialize_binds_an_unmangled_key_to_the_declared_private_slot() {
    assert_eq!(
        out(br#"class P {
    private string $a = 'x';
    protected int $b = 1;
    public $c = null;
    public function dump() {
        echo $this->a, "|", $this->b, "|", ($this->c === null ? "NULL" : $this->c);
    }
}
$obj = unserialize('O:1:"P":3:{s:1:"a";s:2:"AA";s:1:"b";i:9;s:1:"c";b:1;}');
$obj->dump();"#),
        "AA|9|1",
    );
}

/// Verifies a cyclic object graph round-trips: a property naming its own object, and a child's
/// back-pointer to its enclosing parent, resolve to that object.
///
/// `php -n` 8.5.10: `O:4:"Node":2:{s:4:"self";r:1;s:4:"kids";a:1:{i:0;O:4:"Node":2:{s:4:"self";
/// r:1;s:4:"kids";a:0:{}}}}` then `yes|yes`.
#[test]
fn unserialize_round_trips_a_cyclic_object_graph() {
    assert_eq!(
        out(br#"class CycNode { public $self; public $kids = []; }
$a = new CycNode(); $a->self = $a; $c = new CycNode(); $c->self = $a; $a->kids[] = $c;
$b = unserialize(serialize($a));
echo $b->self === $b ? "yes" : "no", "|", $b->kids[0]->self === $b ? "yes" : "no";"#),
        "yes|yes",
    );
}

/// Verifies enum cases serialize as `E:` by name (a repeat is an `r:` back-reference) and
/// unserialize back to the same singletons.
///
/// `php -n` 8.5.10: `a:3:{i:0;E:11:"Suit:Hearts";i:1;r:2;i:2;E:11:"Suit:Spades";}` then `111`.
#[test]
fn serialize_and_unserialize_round_trip_enum_cases() {
    assert_eq!(
        out(br#"enum Suit { case Hearts; case Spades; }
$s = serialize([Suit::Hearts, Suit::Hearts, Suit::Spades]);
echo $s, "|";
$u = unserialize($s);
echo $u[0] === Suit::Hearts ? 1 : 0, $u[1] === Suit::Hearts ? 1 : 0, $u[2] === Suit::Spades ? 1 : 0;"#),
        "a:3:{i:0;E:11:\"Suit:Hearts\";i:1;r:2;i:2;E:11:\"Suit:Spades\";}|111",
    );
}

/// Verifies `__wakeup()` runs after the properties are restored, innermost object first, and that
/// a throwing `__wakeup()` guard propagates out of `unserialize()`.
///
/// `php -n` 8.5.10: `CBA|9` then `BadMethodCallException` (the class of the caught guard).
#[test]
fn unserialize_runs_wakeup_innermost_first_and_propagates_its_throw() {
    assert_eq!(
        out(br#"class WA { public $b; public $n = 0; function __wakeup() { echo "A|", $this->n; } }
class WB { public $c; function __wakeup() { echo "B"; } }
class WC { function __wakeup() { echo "C"; } }
class WG { function __wakeup() { throw new \BadMethodCallException("no"); } }
unserialize('O:2:"WA":2:{s:1:"b";O:2:"WB":1:{s:1:"c";O:2:"WC":0:{}}s:1:"n";i:9;}');
echo "\n";
try { unserialize('O:2:"WG":0:{}'); echo "missed"; }
catch (\BadMethodCallException $e) { echo get_class($e); }"#),
        "CBA|9\nBadMethodCallException",
    );
}

/// Verifies `newInstanceWithoutConstructor()`-shaped allocation: an empty payload still seeds
/// every declared property's default, because allocation runs BEFORE the (empty) payload loop.
///
/// `php -n` 8.5.6: `object(P)#2 (3) { ["a":"P":private]=> string(1) "x" ["b":protected]=> int(1)
/// ["c"]=> NULL }`; echoes `x|1|NULL`.
#[test]
fn unserialize_seeds_declared_defaults_for_an_empty_payload() {
    assert_eq!(
        out(br#"class P {
    private string $a = 'x';
    protected int $b = 1;
    public $c = null;
    public function dump() {
        echo $this->a, "|", $this->b, "|", ($this->c === null ? "NULL" : $this->c);
    }
}
$obj = unserialize('O:1:"P":0:{}');
$obj->dump();"#),
        "x|1|NULL",
    );
}

/// Verifies every scalar/null shape round-trips: int (positive/negative/zero), float (including
/// `NAN`/`INF`/`-INF`), bool, and null.
///
/// `php -n` 8.5.6: `42:-7:0:1.5:NAN:INF:-INF:1::`.
#[test]
fn unserialize_covers_every_scalar_and_null() {
    assert_eq!(
        out(br#"echo unserialize('i:42;'), ":";
echo unserialize('i:-7;'), ":";
echo unserialize('i:0;'), ":";
echo unserialize('d:1.5;'), ":";
echo is_nan(unserialize('d:NAN;')) ? "NAN" : "not-nan", ":";
echo unserialize('d:INF;') === INF ? "INF" : "not-inf", ":";
echo unserialize('d:-INF;') === -INF ? "-INF" : "not-neg-inf", ":";
echo unserialize('b:1;') ? "1" : "0", ":";
echo unserialize('b:0;') ? "1" : "0", ":";
var_dump(unserialize('N;'));"#),
        "42:-7:0:1.5:NAN:INF:-INF:1:0:NULL\n",
    );
}

/// Verifies strings round-trip with an embedded NUL byte and UTF-8 bytes preserved, using a
/// byte-length prefix rather than a character count.
///
/// `php -n` 8.5.6: `s:3:"a\0b"` unserializes to the 3-byte string `a\0b`, and `s:6:"héllo"`
/// unserializes to the 6-byte UTF-8 string.
#[test]
fn unserialize_strings_preserve_embedded_nul_and_utf8_bytes() {
    assert_eq!(
        out(b"echo strlen(unserialize('s:3:\"a\0b\";')), \":\", unserialize('s:3:\"a\0b\";'), \":\";\necho unserialize('s:6:\"h\xc3\xa9llo\";');"),
        "3:a\0b:h\u{e9}llo",
    );
}

/// Verifies list arrays, mixed int/string keys in insertion order, and nesting three deep.
///
/// `php -n` 8.5.6: `array(2) { [0]=> string(1) "a" ["k"]=> int(7) }` and the 3-deep nest bottoms
/// out at `NULL`.
#[test]
fn unserialize_covers_arrays_mixed_keys_and_nesting() {
    assert_eq!(
        out(br#"$r = unserialize('a:2:{i:0;s:1:"a";s:1:"k";i:7;}');
echo $r[0], ":", $r["k"], ":";
$n = unserialize('a:1:{i:0;a:1:{s:1:"x";a:1:{i:0;N;}}}');
var_dump($n[0]["x"][0]);"#),
        "a:7:NULL\n",
    );
}

/// Verifies the `allowed_classes => false` shape: a `__PHP_Incomplete_Class` object carrying
/// `__PHP_Incomplete_Class_Name` and every payload property as a plain (unmangled, public)
/// dynamic property -- NO declared defaults applied, because the class metadata is never
/// consulted at all for this option.
///
/// `php -n` 8.5.6: `object(__PHP_Incomplete_Class)#2 (2) { ["__PHP_Incomplete_Class_Name"]=>
/// string(1) "P" ["a"]=> string(2) "AA" }`.
#[test]
fn unserialize_with_allowed_classes_false_builds_an_incomplete_class() {
    assert_eq!(
        out(br#"class P { private string $a = 'x'; }
$obj = unserialize('O:1:"P":1:{s:1:"a";s:2:"AA";}', ['allowed_classes' => false]);
echo get_class($obj), "|", $obj->__PHP_Incomplete_Class_Name, "|", $obj->a;"#),
        "__PHP_Incomplete_Class|P|AA",
    );
}

/// Verifies an unknown class name is ALWAYS an incomplete class, regardless of `allowed_classes`
/// (there is nothing to allow -- the class does not exist), with no warning.
///
/// `php -n` 8.5.6: `object(__PHP_Incomplete_Class)#1 (1) { ["__PHP_Incomplete_Class_Name"]=>
/// string(9) "NoSuchCls") }`, no warning.
#[test]
fn unserialize_an_unknown_class_name_is_silently_incomplete() {
    let (output, warnings) =
        out_with_warnings(br#"$obj = unserialize('O:9:"NoSuchCls":0:{}');
echo get_class($obj), "|", $obj->__PHP_Incomplete_Class_Name;"#);
    assert_eq!(output, "__PHP_Incomplete_Class|NoSuchCls");
    assert!(warnings.is_empty(), "expected no warning, got {warnings:?}");
}

/// Verifies `unserialize('')` is `false` with NO warning -- load-bearing because Symfony reads
/// possibly-empty cache files on this exact path.
///
/// `php -n` 8.5.6: `bool(false)`, no warning.
#[test]
fn unserialize_empty_string_is_false_with_no_warning() {
    let program = parse_fragment(br#"return unserialize('');"#).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.get(result), FakeValue::Bool(false));
    assert!(values.warnings.is_empty());
}

/// Verifies malformed input is a catchable-free `false` with a recorded warning worded like
/// php's own `"unserialize(): Error at offset N of M bytes"` -- for a single bad byte with no
/// prior consumption, this scanner's own offset genuinely agrees with php's (both report 0).
///
/// `php -n` 8.5.6: `Warning: unserialize(): Error at offset 0 of 1 bytes ...` then `bool(false)`.
#[test]
fn unserialize_malformed_input_is_false_with_a_warning() {
    let program = parse_fragment(br#"return unserialize('x');"#).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.get(result), FakeValue::Bool(false));
    assert_eq!(
        values.warnings,
        vec!["unserialize(): Error at offset 0 of 1 bytes"],
    );
}

/// Verifies an out-of-i64-range integer literal clamps to `PHP_INT_MAX`/`PHP_INT_MIN` with a
/// recoverable warning, rather than failing -- this is a SUCCESS case, not a `false` case.
///
/// `php -n` 8.5.6: `Warning: unserialize(): Numerical result out of range ...` then
/// `int(9223372036854775807)`.
#[test]
fn unserialize_an_out_of_range_integer_clamps_with_a_warning() {
    let program = parse_fragment(br#"return unserialize('i:99999999999999999999;');"#)
        .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.get(result), FakeValue::Int(i64::MAX));
    assert_eq!(
        values.warnings,
        vec!["unserialize(): Numerical result out of range"],
    );
}

/// Verifies the `TypeError`s: a non-string `$data`, a non-array `$options`, and an
/// `allowed_classes` option that is neither `array` nor `bool`.
///
/// `php -n` 8.5.6: `unserialize(): Argument #1 ($data) must be of type string, array given`,
/// `unserialize(): Argument #2 ($options) must be of type array, null given`,
/// `unserialize(): Option "allowed_classes" must be of type array|bool, string given`.
#[test]
fn unserialize_rejects_wrong_argument_and_option_types() {
    assert_eq!(
        out(br#"function probe($fn) {
    try { $fn(); } catch (\Throwable $e) { return get_class($e) . ": " . $e->getMessage(); }
    return "no-throw";
}
echo probe(fn() => unserialize([])), ":";
echo probe(fn() => unserialize('i:1;', null)), ":";
echo probe(fn() => unserialize('i:1;', ['allowed_classes' => 'nope']));"#),
        "TypeError: unserialize(): Argument #1 ($data) must be of type string, array given:\
TypeError: unserialize(): Argument #2 ($options) must be of type array, null given:\
TypeError: unserialize(): Option \"allowed_classes\" must be of type array|bool, string given",
    );
}

/// Verifies the catchable `ArgumentCountError` for the wrong arity.
///
/// `php -n` 8.5.6: `unserialize() expects at least 1 argument, 0 given` then
/// `unserialize() expects at most 2 arguments, 3 given`.
#[test]
fn unserialize_rejects_wrong_arity_with_a_catchable_argument_count_error() {
    assert_eq!(
        out(br#"try {
    unserialize();
} catch (\ArgumentCountError $e) {
    echo $e->getMessage(), ":";
}
try {
    unserialize('i:1;', [], 3);
} catch (\ArgumentCountError $e) {
    echo $e->getMessage();
}"#),
        "unserialize() expects at least 1 argument, 0 given:\
unserialize() expects at most 2 arguments, 3 given",
    );
}

/// Verifies a `serialize()`/`unserialize()` round trip for a mixed structure -- cheaper than
/// pinning byte-exact output twice, and it exercises both functions agreeing with each other.
#[test]
fn unserialize_round_trips_through_serialize() {
    assert_eq!(
        out(br#"class RT { private $a = 1; protected $b = "x"; public $c = [1, 2, 3]; }
$original = new RT();
$copy = unserialize(serialize($original));
echo $copy->c[0], $copy->c[1], $copy->c[2];"#),
        "123",
    );
}

/// Verifies `function_exists("unserialize")` answers true now that the builtin is registered.
///
/// `php -n` 8.5.6: `bool(true)`.
#[test]
fn unserialize_is_visible_to_function_exists() {
    let program =
        parse_fragment(br#"return function_exists("unserialize");"#).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// `r:N;` and `R:N;` resolve against the values parsed so far, numbered as php numbers them:
/// every value but an `R:` takes an index, an `r:` takes one, and keys take none. `r:` shares the
/// object; `R:` onto a scalar yields an equal value (a copy here, php binds one reference).
///
/// `php -n` 8.5.6: `["x","x"]`, `bool(true)` twice, and `bool(false)` for `r:` onto a string.
#[test]
fn unserialize_resolves_back_references_by_php_numbering() {
    assert_eq!(
        out(br#"echo json_encode(unserialize('a:2:{i:0;s:1:"x";i:1;R:2;}')), "|";
echo json_encode(unserialize('a:3:{i:0;i:7;i:1;R:2;i:2;s:1:"z";}')), "|";
$o = unserialize('a:4:{i:0;O:8:"stdClass":0:{}i:1;r:2;i:2;O:8:"stdClass":0:{}i:3;r:4;}');
var_dump($o[0] === $o[1], $o[2] === $o[3], $o[0] === $o[2]);"#),
        "[\"x\",\"x\"]|[7,7,\"z\"]|bool(true)\nbool(true)\nbool(false)\n",
    );
}

/// A serialize/unserialize round trip of a structure sharing objects reproduces the same wire
/// bytes and the same object identities.
#[test]
fn unserialize_round_trips_shared_objects_to_the_same_wire_bytes() {
    assert_eq!(
        out(br#"class P { public $x = 1; public $y; }
$p = new P; $q = new P;
$s = serialize(["k" => $p, "again" => $p, 3 => $q, "z" => "s", "w" => $q]);
$u = unserialize($s);
echo $s === serialize($u) ? "same" : "different", "|";
var_dump($u["k"] === $u["again"], $u[3] === $u["w"], $u["k"] === $u[3]);"#),
        "same|bool(true)\nbool(true)\nbool(false)\n",
    );
}
