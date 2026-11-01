//! Purpose:
//! Interpreter tests pinning PHP's return-BY-REFERENCE: `function &f()` handing a writable
//! reference back to `$r = &f();`, which is `dependency-injection`'s `ExtensionTrait` and
//! `KernelTrait` and `http-kernel`'s `ProfilerListener`.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::by_ref_return`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - WHATEVER OUTLIVES THE CALL OWNS ITS REFERENCE. The activation scope is drained the moment
//!   the call returns, so the reference the callee leaves behind carries a RETAINED cell, and
//!   its target is rewritten to one that outlives the frame: a static local names the context
//!   store, a by-reference parameter's element names the CALLER's target, and a plain local --
//!   whose storage really does die -- keeps only the cell.
//! - Every case writes THROUGH the alias and reads the storage back by another route. Reading
//!   the alias itself proves nothing: it holds the value either way.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse by-ref return fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute by-ref return fragment");
    values.output.clone()
}

/// Runs one fragment and returns what it echoed plus the warnings it raised.
fn out_with_warnings(fragment: &[u8]) -> (String, Vec<String>) {
    let program = parse_fragment(fragment).expect("parse by-ref return fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute by-ref return fragment");
    (values.output.clone(), values.warnings.clone())
}

/// Verifies a STATIC LOCAL returned by reference is written through by the caller.
///
/// `php -n` 8.5.6 prints `5`. The second call reads the context's static-local store, so this is
/// the case that proves the write reached the storage and not just the alias.
#[test]
fn a_static_local_returned_by_reference_is_written_through() {
    assert_eq!(
        out(
            br#"function &counter() { static $n = 0; return $n; }
$c = &counter();
$c += 5;
echo counter();"#
        ),
        "5",
    );
}

/// Verifies a PROPERTY, a property ELEMENT and a STATIC PROPERTY are each written through.
///
/// `php -n` 8.5.6 prints `9;42;200`. Each is read back through a different route than the alias
/// -- a second method, the property array, the static property -- so none of them can pass on
/// the alias alone.
#[test]
fn a_property_an_element_and_a_static_property_are_written_through() {
    assert_eq!(
        out(
            br#"class Box {
    private $v = 1;
    public array $items = ["a" => 10];
    public static $s = 100;
    public function &get() { return $this->v; }
    public function read() { return $this->v; }
    public function &item() { return $this->items["a"]; }
    public static function &stat() { return self::$s; }
}
$b = new Box();
$r = &$b->get();
$r = 9;
echo $b->read(), ";";
$e = &$b->item();
$e = 42;
echo $b->items["a"], ";";
$s = &Box::stat();
$s = 200;
echo Box::$s;"#
        ),
        "9;42;200",
    );
}

/// Verifies an element of a BY-REFERENCE PARAMETER reaches the caller's own array.
///
/// `php -n` 8.5.6 prints `77`. The returned target has to be the CALLER's storage: the callee's
/// parameter name is gone with the activation, so a target naming that name would write nowhere.
#[test]
fn an_element_of_a_by_reference_parameter_reaches_the_caller() {
    assert_eq!(
        out(
            br#"function &relay(array &$a) { return $a["k"]; }
$arr = ["k" => 1];
$p = &relay($arr);
$p = 77;
echo $arr["k"];"#
        ),
        "77",
    );
}

/// Verifies a CHAINED bind aliases only the inner name, and the alias still writes through.
///
/// `php -n` 8.5.6 prints `4;8`. `$x` copied what the call returned; `$y` is the alias, and
/// writing it reaches the static local, which the second call reads back.
#[test]
fn a_chained_bind_aliases_the_inner_name_and_writes_through() {
    assert_eq!(
        out(
            br#"function &g() { static $v = 4; return $v; }
$x = $y = &g();
$y = 8;
echo $x, ";", g();"#
        ),
        "4;8",
    );
}

/// Verifies a PLAIN LOCAL returned by reference keeps its value in the alias.
///
/// `php -n` 8.5.6 prints `6`. The local's storage dies with the call in php too -- the value
/// survives only because the reference is refcounted -- so the alias holding the write is the
/// whole of the observable behaviour.
#[test]
fn a_plain_local_returned_by_reference_keeps_its_value_in_the_alias() {
    assert_eq!(
        out(
            br#"function &plain() { $local = 3; return $local; }
$pl = &plain();
$pl = 6;
echo $pl;"#
        ),
        "6",
    );
}

/// Verifies a NON-reference return under `&` raises php's notice and binds a copy.
///
/// `php -n` 8.5.6 prints `5;2` and raises
/// `Notice: Only variable references should be returned by reference` once per call. It is a
/// notice and not an error: the program keeps running and the caller gets a copy.
#[test]
fn a_non_reference_return_notices_and_binds_a_copy() {
    let (output, warnings) = out_with_warnings(
        br#"function &lit() { return 5; }
$a = &lit();
echo $a, ";";
function &sum() { $x = 1; return $x + 1; }
$b = &sum();
echo $b;"#,
    );
    assert_eq!(output, "5;2");
    assert_eq!(
        warnings,
        vec![
            "Only variable references should be returned by reference".to_string(),
            "Only variable references should be returned by reference".to_string(),
        ]
    );
}
