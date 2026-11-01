//! Purpose:
//! End-to-end value tests for `foreach` over an object that implements neither Iterator
//! interface, and for the order every enumerator reports an object's dynamic properties in.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_foreach_object` through Rust's test harness.
//!
//! Key details:
//! - PHP iterates the properties VISIBLE FROM THE CALLING SCOPE, so the same object yields a
//!   different set inside a method than outside the class. Both are covered, because a rule
//!   that only looks right from one side is the failure these tests exist to catch.
//! - Dynamic properties come back in CREATION order, so every fixture adds them out of
//!   alphabetical order on purpose: a store that cannot remember insertion order, or one that
//!   sorts to look deterministic, fails here and only here.
//! - Every expected string is `php -n` 8.5.6's output. PHP also emits `Deprecated: Creation of
//!   dynamic property C::$p is deprecated`; that goes to stderr, not stdout.

use crate::support::*;

/// Verifies `foreach` outside the class yields public properties in PHP's order.
///
/// `php -n` 8.5.6 prints `pub=u;zeta=z;alpha=a;mid=m;`. Before this, `foreach` over such an
/// object failed outright instead of iterating anything.
#[test]
fn test_eval_foreach_outside_yields_public_properties_in_creation_order() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class F1 {
    private $secret = "s";
    protected $prot = "p";
    public $pub = "u";
}
$o = new F1();
$o->zeta = "z";
$o->alpha = "a";
$o->mid = "m";
foreach ($o as $k => $v) { echo "$k=$v;"; }
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "pub=u;zeta=z;alpha=a;mid=m;");
}

/// Verifies `foreach` inside a method also yields private and protected properties.
///
/// `php -n` 8.5.6 prints `secret=s;prot=p;pub=u;zeta=z;alpha=a;mid=m;`.
#[test]
fn test_eval_foreach_inside_a_method_yields_all_visible_properties() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class F1 {
    private $secret = "s";
    protected $prot = "p";
    public $pub = "u";
    public function inside() { $out = ""; foreach ($this as $k => $v) { $out .= "$k=$v;"; } return $out; }
}
$o = new F1();
$o->zeta = "z";
$o->alpha = "a";
$o->mid = "m";
echo $o->inside();
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "secret=s;prot=p;pub=u;zeta=z;alpha=a;mid=m;");
}

/// Verifies a child method sees the parent's protected property but not its private one.
///
/// `php -n` 8.5.6 prints `qq=y;cc=z;`.
#[test]
fn test_eval_foreach_in_a_child_hides_the_parents_private_property() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class P1 { private $pp = "x"; protected $qq = "y"; }
class C1 extends P1 {
    public $cc = "z";
    public function view() { $o = ""; foreach ($this as $k => $v) { $o .= "$k=$v;"; } return $o; }
}
echo (new C1())->view();
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "qq=y;cc=z;");
}

/// Verifies `foreach` by reference writes back through to the object's properties.
///
/// `php -n` 8.5.6 prints `{"a":10,"b":20,"c":30}`, so the loop variable has to alias the
/// property rather than a copy of it.
#[test]
fn test_eval_foreach_by_reference_writes_back_to_the_object() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class F2 { public $a = 1; public $b = 2; }
$r = new F2();
$r->c = 3;
foreach ($r as $k => &$v) { $v = $v * 10; }
unset($v);
echo json_encode($r);
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "{\"a\":10,\"b\":20,\"c\":30}");
}

/// Verifies every enumerator reports the same properties in the same PHP order.
///
/// `php -n` 8.5.6 prints `pub,zeta,alpha,mid` then `{"pub":"u","zeta":"z","alpha":"a","mid":"m"}`.
/// The dynamic names are deliberately not alphabetical, so a store that sorts them fails here.
#[test]
fn test_eval_every_enumerator_agrees_on_php_creation_order() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class F1 { private $secret = "s"; protected $prot = "p"; public $pub = "u"; }
$o = new F1();
$o->zeta = "z";
$o->alpha = "a";
$o->mid = "m";
echo implode(",", array_keys(get_object_vars($o)));
echo ":";
echo json_encode($o);
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(
        out,
        "pub,zeta,alpha,mid:{\"pub\":\"u\",\"zeta\":\"z\",\"alpha\":\"a\",\"mid\":\"m\"}"
    );
}

/// Verifies `json_encode()` exports public properties only.
///
/// `php -n` 8.5.6 prints `{"c":"3","d":"4"}` for a class carrying a private, a protected, a
/// public and a dynamic property. A protected storage name is not mangled by this interpreter,
/// so an export that filters on the name alone would leak it.
#[test]
fn test_eval_json_encode_exports_public_properties_only() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class Z { private $a = "1"; protected $b = "2"; public $c = "3"; }
$o = new Z();
$o->d = "4";
echo json_encode($o);
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "{\"c\":\"3\",\"d\":\"4\"}");
}

/// Verifies `print_r()` shows non-public properties with PHP's visibility markers and values.
///
/// `php -n` 8.5.6 prints the block below, so a private and a protected property keep their
/// values even though only public ones are mirrored into the object's slots.
#[test]
fn test_eval_print_r_shows_non_public_properties_with_values() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class Z { private $a = "1"; protected $b = "2"; public $c = "3"; }
$o = new Z();
$o->d = "4";
print_r($o);
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(
        out,
        "Z Object\n(\n    [a:Z:private] => 1\n    [b:protected] => 2\n    [c] => 3\n    [d] => 4\n)\n"
    );
}

/// Verifies the same holds for a class a file included at RUN TIME declared.
///
/// `php -n` 8.5.6 prints `loaded;pub=u;zeta=z;alpha=a;mid=m;`. This is the autoload-shaped
/// path: the include path is not a literal, so the file and its class arrive at run time.
#[test]
fn test_runtime_include_foreach_yields_php_order() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$name = \"piece\" . \".php\";\ninclude $name;\n",
            ),
            (
                "piece.php",
                r#"<?php
class S4Iter { private $secret = "s"; public $pub = "u"; }
echo "loaded;";
$o = new S4Iter();
$o->zeta = "z";
$o->alpha = "a";
$o->mid = "m";
foreach ($o as $k => $v) { echo "$k=$v;"; }
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded;pub=u;zeta=z;alpha=a;mid=m;");
}

/// Verifies interpreted code can iterate a COMPILED generator, directly and through an
/// `IteratorAggregate` whose `getIterator()` returns one. Symfony's `CacheWarmerAggregate`
/// runs `foreach ($this->warmers as $warmer)` over a container `RewindableGenerator`; the
/// interpreter drives the generator by method name, and `rewind()` had no bridge slot.
#[test]
fn test_eval_foreach_over_a_compiled_generator() {
    let out = compile_and_run(
        r#"<?php
final class Lazy implements IteratorAggregate {
    public function __construct(private Closure $factory) {}
    public function getIterator(): Traversable { return ($this->factory)(); }
}
$lazy = new Lazy(function () { yield 'a' => 1; yield 'b' => 2; });
$gen = (function () { yield 10; yield 20; })();
eval('foreach ($lazy as $k => $v) { echo "$k=$v;"; } foreach ($gen as $v) { echo "$v;"; }');
"#,
    );
    assert_eq!(out, "a=1;b=2;10;20;");
}
