//! Purpose:
//! Interpreter tests pinning `LVALUE = &SOURCE` where the LVALUE is an array ELEMENT reached
//! through a property or a static property -- `$this->data["bag"] = &$rows;` and
//! `self::$darwinCache[$dir] = &self::$darwinCache[$k];`, which is `error-handler`'s
//! `DebugClassLoader`.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::element_reference_bind`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - The INTERPRETER already handled these: `EvalStmt::ArrayReferenceBind` writes through a
//!   general lvalue and `evaluate_plain_assignment_location` accepts a property or static
//!   property root. Only the two parse paths never offered it the chance -- each read the `=`
//!   and then demanded a VALUE, so the `&` was an unexpected token.
//! - A reference is SYMMETRIC, so each test writes through BOTH ends. Asserting only that the
//!   original reaches the alias passes against a plain copy made at bind time.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse element reference bind fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute element reference bind fragment");
    values.output.clone()
}

/// Verifies an element of a PROPERTY binds to a by-reference parameter, both ways.
///
/// `php -n` 8.5.6 prints `2;3`. The second number is the one that matters: it writes through the
/// property element and reads the caller's variable, so a bind that had quietly copied would
/// print `2;2`.
#[test]
fn a_property_element_binds_to_a_by_reference_parameter() {
    assert_eq!(
        out(
            br#"class Proxy {
    public array $data = [];
    public function attach(array &$rows): void { $this->data["bag"] = &$rows; }
}
$rows = [1];
$p = new Proxy();
$p->attach($rows);
$rows[] = 2;
echo count($p->data["bag"]), ";";
$p->data["bag"][] = 3;
echo count($rows);"#
        ),
        "2;3",
    );
}

/// Verifies two elements of the same STATIC property alias each other, both ways.
///
/// `php -n` 8.5.6 prints `2;3`. This is `DebugClassLoader`'s
/// `self::$darwinCache[$dir] = &self::$darwinCache[$k];` -- the target and the source are both
/// elements of the same static array, which is what makes writing through either end the only
/// honest assertion.
#[test]
fn two_static_property_elements_alias_each_other() {
    assert_eq!(
        out(
            br#"class Cache {
    public static array $store = [];
    public static function alias(string $a, string $b): void {
        self::$store[$a] = [1];
        self::$store[$b] = &self::$store[$a];
    }
}
Cache::alias("x", "y");
Cache::$store["x"][] = 9;
echo count(Cache::$store["y"]), ";";
Cache::$store["y"][] = 8;
echo count(Cache::$store["x"]);"#
        ),
        "2;3",
    );
}

/// Verifies a NESTED local element target binds, which needs no property at all.
///
/// `php -n` 8.5.6 prints `2`. This half already worked and is pinned beside the other two: with
/// the fix disabled it stays green while they go red, which says which paths the fix bought.
#[test]
fn a_nested_local_element_binds_as_it_already_did() {
    assert_eq!(
        out(
            br#"$outer = [];
$inner = [1];
$outer["k"]["j"] = &$inner;
$inner[] = 2;
echo count($outer["k"]["j"]);"#
        ),
        "2",
    );
}
