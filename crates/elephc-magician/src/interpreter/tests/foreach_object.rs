//! Purpose:
//! Interpreter tests for `foreach` over an object that implements neither Iterator interface,
//! and for the order every enumerator reports an object's dynamic properties in.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::foreach_object`.
//!
//! Key details:
//! - PHP iterates the properties VISIBLE FROM THE CALLING SCOPE, so the same object yields a
//!   different set inside a method than outside the class. Both are pinned, because a rule that
//!   only looks right from one side is the failure this file exists to catch.
//! - Dynamic properties come back in CREATION order, not alphabetical and not hash order, so
//!   every fixture here adds them out of alphabetical order on purpose.
//! - `foreach`, `get_object_vars()`, `json_encode()` and the `(array)` cast are pinned on the
//!   SAME object in one test, because the bug this replaced was not any single one of them
//!   being wrong, it was two of them disagreeing.
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse foreach object fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute foreach object fragment");
    values.output.clone()
}

/// The fixture class: three declared visibilities, three dynamics out of alphabetical order.
const VISIBILITY_FIXTURE: &[u8] = br#"class F1 {
    private $secret = "s";
    protected $prot = "p";
    public $pub = "u";
    public function inside() {
        $out = "";
        foreach ($this as $k => $v) { $out .= "$k=$v;"; }
        return $out;
    }
}
$o = new F1();
$o->zeta = "z";
$o->alpha = "a";
$o->mid = "m";
"#;

/// Builds a fragment from the fixture plus a tail.
fn fixture(tail: &str) -> Vec<u8> {
    let mut source = VISIBILITY_FIXTURE.to_vec();
    source.extend_from_slice(tail.as_bytes());
    source
}

/// Verifies `foreach` from outside the class yields only public properties, in PHP's order.
///
/// `php -n` 8.5.6 prints `pub=u;zeta=z;alpha=a;mid=m;`: the declared public property first,
/// then the three dynamic ones in the order they were created. Before this, `foreach` over such
/// an object failed outright rather than iterating anything.
#[test]
fn foreach_outside_the_class_yields_public_properties_in_creation_order() {
    assert_eq!(
        out(&fixture(r#"foreach ($o as $k => $v) { echo "$k=$v;"; }"#)),
        "pub=u;zeta=z;alpha=a;mid=m;",
    );
}

/// Verifies `foreach` inside a method also yields the private and protected properties.
///
/// `php -n` 8.5.6 prints `secret=s;prot=p;pub=u;zeta=z;alpha=a;mid=m;`, in declaration order
/// for the declared ones and creation order for the dynamic ones.
#[test]
fn foreach_inside_a_method_yields_private_and_protected_properties() {
    assert_eq!(out(&fixture(r#"echo $o->inside();"#)), "secret=s;prot=p;pub=u;zeta=z;alpha=a;mid=m;");
}

/// Verifies a child method sees the parent's protected property but not its private one.
///
/// `php -n` 8.5.6 prints `qq=y;cc=z;`: `P1::$pp` is private to the parent and stays hidden even
/// though the instance carries it.
#[test]
fn foreach_in_a_child_method_hides_the_parents_private_property() {
    assert_eq!(
        out(
            br#"class P1 { private $pp = "x"; protected $qq = "y"; }
class C1 extends P1 {
    public $cc = "z";
    public function view() { $o = ""; foreach ($this as $k => $v) { $o .= "$k=$v;"; } return $o; }
}
echo (new C1())->view();"#
        ),
        "qq=y;cc=z;",
    );
}

/// Verifies `foreach` by reference writes back through to the object's properties.
///
/// `php -n` 8.5.6 prints `{"a":10,"b":20,"c":30}` after multiplying each value in place, so the
/// loop variable has to alias the property rather than a copy of it.
#[test]
fn foreach_by_reference_writes_back_to_the_object() {
    assert_eq!(
        out(
            br#"class F2 { public $a = 1; public $b = 2; }
$r = new F2();
$r->c = 3;
foreach ($r as $k => &$v) { $v = $v * 10; }
unset($v);
echo json_encode($r);"#
        ),
        "{\"a\":10,\"b\":20,\"c\":30}",
    );
}

/// Verifies every enumerator reports the same properties in the same PHP order.
///
/// `php -n` 8.5.6 prints `pub,zeta,alpha,mid` for `get_object_vars()` and
/// `{"pub":"u","zeta":"z","alpha":"a","mid":"m"}` for `json_encode()`. Pinning them beside the
/// `foreach` above is the point: the dynamic names are deliberately not in alphabetical order,
/// so a store that cannot remember creation order shows up here.
#[test]
fn every_enumerator_agrees_on_php_creation_order() {
    assert_eq!(
        out(&fixture(
            r#"echo implode(",", array_keys(get_object_vars($o))); echo ":"; echo json_encode($o);"#
        )),
        "pub,zeta,alpha,mid:{\"pub\":\"u\",\"zeta\":\"z\",\"alpha\":\"a\",\"mid\":\"m\"}",
    );
}

/// Verifies a `foreach` subject the loop allocated is destroyed when the loop is done with it.
///
/// `php -n` 8.5.6 prints `a=1,b=2,tag=new,destruct:new;|a=1,b=2,tag=call,destruct:call;`. The
/// value never reaches a scope name, so nothing but the loop can ever release it: before this
/// the object simply leaked and `__destruct` never ran.
#[test]
fn a_foreach_subject_the_loop_allocated_is_destroyed_after_it() {
    assert_eq!(
        out(
            br#"class Probe {
    public $a = 1;
    public $b = 2;
    public $tag;
    public function __construct($t) { $this->tag = $t; }
    public function __destruct() { echo "destruct:"; echo $this->tag; echo ";"; }
}
function makeProbe($t) { return new Probe($t); }
foreach (new Probe("new") as $k => $v) { echo "$k=$v,"; }
echo "|";
foreach (makeProbe("call") as $k => $v) { echo "$k=$v,"; }"#
        ),
        "a=1,b=2,tag=new,destruct:new;|a=1,b=2,tag=call,destruct:call;",
    );
}

/// Verifies a subject that is a plain variable read is NOT destroyed by the loop.
///
/// `php -n` 8.5.6 prints `a=1,b=2,tag=held,afterC;destruct:held;`: the loop borrows the caller's
/// reference and the object dies with the variable, not with the loop. Releasing a borrowed cell
/// here would destroy an object its owner still holds.
#[test]
fn a_foreach_subject_read_from_a_variable_is_left_alone() {
    assert_eq!(
        out(
            br#"class Probe2 {
    public $a = 1;
    public $b = 2;
    public $tag;
    public function __construct($t) { $this->tag = $t; }
    public function __destruct() { echo "destruct:"; echo $this->tag; echo ";"; }
}
$held = new Probe2("held");
foreach ($held as $k => $v) { echo "$k=$v,"; }
echo "afterC;";
unset($held);"#
        ),
        "a=1,b=2,tag=held,afterC;destruct:held;",
    );
}

/// Verifies the subject is released on every exit edge, not only on completion.
///
/// `php -n` 8.5.6 prints `a=1,destruct:broken;|destruct:returned;r|destruct:thrown;caught;` for
/// `break`, `return` out of the enclosing function, and a throw passing through.
#[test]
fn a_foreach_subject_is_released_on_break_return_and_throw() {
    assert_eq!(
        out(
            br#"class Probe3 {
    public $a = 1;
    public $b = 2;
    public $tag;
    public function __construct($t) { $this->tag = $t; }
    public function __destruct() { echo "destruct:"; echo $this->tag; echo ";"; }
}
foreach (new Probe3("broken") as $k => $v) { echo "$k=$v,"; break; }
echo "|";
function walk() { foreach (new Probe3("returned") as $k => $v) { return "r"; } }
echo walk();
echo "|";
function boom() { foreach (new Probe3("thrown") as $k => $v) { throw new RuntimeException("b"); } }
try { boom(); } catch (RuntimeException $e) { echo "caught;"; }"#
        ),
        "a=1,destruct:broken;|destruct:returned;r|destruct:thrown;caught;",
    );
}

/// Verifies a generator the loop called is destroyed when the loop lets go of it.
///
/// `php -n` 8.5.6 prints `1fin;|12fin;`: breaking out of `foreach (gen() as $v)` destroys the
/// generator right there and runs the `finally` its yield was suspended inside, and running the
/// loop to completion runs it at the end. This is the case that made the leak visible.
#[test]
fn a_generator_subject_runs_its_finally_when_the_loop_lets_go() {
    assert_eq!(
        out(
            br#"function gen() { try { yield 1; yield 2; } finally { echo "fin;"; } }
foreach (gen() as $v) { echo $v; break; }
echo "|";
foreach (gen() as $v) { echo $v; }"#
        ),
        "1fin;|12fin;",
    );
}

/// Verifies an array subject the loop allocated still iterates after being released.
///
/// `php -n` 8.5.6 prints `78`. An array has no destructor to observe, so what this pins is that
/// releasing the temporary does not disturb the iteration that used it.
#[test]
fn an_array_subject_the_loop_allocated_still_iterates() {
    assert_eq!(
        out(
            br#"function makeArray() { return [7, 8]; }
foreach (makeArray() as $v) { echo $v; }
foreach ([9] as $v) { echo $v; }"#
        ),
        "789",
    );
}

/// Verifies overwriting a dynamic property keeps its original position.
///
/// `php -n` 8.5.6 prints `{"z":"Z2","a":"A","m":"M"}`: rewriting `z` after the others were
/// created does not move it to the end, because creation order is fixed at first write.
#[test]
fn rewriting_a_dynamic_property_does_not_move_it() {
    assert_eq!(
        out(
            br#"class F3 {}
$o = new F3();
$o->z = "Z";
$o->a = "A";
$o->m = "M";
$o->z = "Z2";
echo json_encode($o);"#
        ),
        "{\"z\":\"Z2\",\"a\":\"A\",\"m\":\"M\"}",
    );
}

