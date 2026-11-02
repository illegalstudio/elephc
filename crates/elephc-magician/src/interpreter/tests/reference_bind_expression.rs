//! Purpose:
//! Interpreter tests pinning `TARGET = &SOURCE` used where a VALUE is wanted --
//! `if (null !== $exists = &self::$cache[$key])` and the chained `$a = $b = &$one;`.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::reference_bind_expression`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - PHP's reference assignment is an EXPRESSION. The parser used to hand its target back
//!   unconsumed so the statement tail could build a binding statement, which works only when a
//!   statement tail is there to catch it: inside a condition nothing was, and the `&` was
//!   refused.
//! - What it EVALUATES TO is the point. The value is the bound value as a COPY, not a second
//!   alias: after `$a = ($b = &$one); $one = 9;` php reports `$b` as 9 and `$a` as the 1 it
//!   copied. A lowering that returned an alias would print `9;9` and still bind correctly.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse reference bind expression fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute reference bind expression fragment");
    values.output.clone()
}

/// Verifies a bind inside a CONDITION binds and yields the bound value.
///
/// `php -n` 8.5.6 prints `11`. Writing through the alias inside the branch has to reach the
/// static property, which is what separates a bind from a read.
#[test]
fn a_bind_inside_a_condition_binds_and_yields_the_bound_value() {
    assert_eq!(
        out(
            br#"class C {
    public static $c = ["x" => 1];
    public function f(): int {
        if (null !== $e = &self::$c["x"]) {
            $e = $e + 10;
            return self::$c["x"];
        }
        return 0;
    }
}
echo (new C())->f();"#
        ),
        "11",
    );
}

/// Verifies the condition is decided by the bound VALUE, so a missing key takes the other branch.
///
/// `php -n` 8.5.6 prints `hit:11;miss`. The second call binds a key that does not exist, reads
/// null through it and falls to the else -- so the value the expression yields is what the `if`
/// tests, not the success of the binding.
#[test]
fn a_missing_key_yields_null_and_takes_the_other_branch() {
    assert_eq!(
        out(
            br#"class D {
    public static array $cache = ["a" => 1];
    public function probe(string $key): string {
        if (null !== $exists = &self::$cache[$key]) {
            $exists = $exists + 10;
            return "hit:" . self::$cache[$key];
        }
        return "miss";
    }
}
$d = new D();
echo $d->probe("a"), ";", $d->probe("b");"#
        ),
        "hit:11;miss",
    );
}

/// Verifies the expression's value is a COPY of the bound value, not a second alias.
///
/// `php -n` 8.5.6 prints `1;5`. `$v` keeps the value it copied while the alias goes on writing
/// through to the array, which is the distinction a lowering that returned the alias would lose.
#[test]
fn the_expression_value_is_a_copy_and_not_a_second_alias() {
    assert_eq!(
        out(
            br#"$src = ["k" => 1];
$v = ($alias = &$src["k"]);
$alias = 5;
echo $v, ";", $src["k"];"#
        ),
        "1;5",
    );
}

/// Verifies a CHAINED bind aliases only the inner target.
///
/// `php -n` 8.5.6 prints `1;9`. `$b` is the alias and follows `$one`; `$a` copied what `$b` held
/// at the time. Both halves are needed: a chain that aliased both would print `9;9`, one that
/// aliased neither would print `1;1`.
#[test]
fn a_chained_bind_aliases_only_the_inner_target() {
    assert_eq!(
        out(
            br#"$one = 1;
$a = $b = &$one;
$one = 9;
echo $a, ";", $b;"#
        ),
        "1;9",
    );
}
