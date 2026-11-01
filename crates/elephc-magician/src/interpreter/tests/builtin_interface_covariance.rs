//! Purpose:
//! Interpreter tests pinning that a return type may be NARROWED to a PHP builtin interface --
//! `IteratorAggregate::getIterator(): Traversable` implemented as `getIterator(): Iterator`,
//! which is php's own covariance and the spelling Symfony writes.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtin_interface_covariance`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - The refusal was at CLASS DECLARATION, not at the call: `eval_return_class_type_is_a`
//!   resolved the declared atom against eval-declared classes and eval-declared interfaces only.
//!   A builtin interface reaches neither branch, so `Iterator` had no parents at all and could
//!   never be a `Traversable`, and the whole class became a fatal before any method ran.
//! - The class-side walk (`collect_class_interface_parent_names`) already consulted the builtin
//!   parent table; the interface-side walk did not. One table, two walks, only one of them
//!   using it -- which is why `implements Iterator` worked while `: Iterator` did not.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse covariance fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute covariance fragment");
    values.output.clone()
}

/// Verifies `getIterator(): Iterator` declares, iterates and unpacks.
///
/// `php -n` 8.5.6 prints `p=1;q=2;{"p":1,"q":2};T`. The `foreach` and the unpacking are both
/// asserted because the declaration is what used to fail: with the class refused, every reader
/// downstream fails too, and any one of them alone would leave the cause ambiguous.
#[test]
fn a_get_iterator_narrowed_to_iterator_declares_and_runs() {
    assert_eq!(
        out(
            br#"class Pairs implements Iterator {
    private $i = 0;
    private $keys = ["p", "q"];
    private $vals = [1, 2];
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): mixed { return $this->vals[$this->i]; }
    public function key(): mixed { return $this->keys[$this->i]; }
    public function next(): void { $this->i++; }
}
class Agg implements IteratorAggregate {
    public function getIterator(): Iterator { return new Pairs(); }
}
foreach (new Agg() as $k => $v) { echo $k, "=", $v, ";"; }
echo json_encode([...new Agg()]), ";";
echo (new Agg()) instanceof Traversable ? "T" : "t";"#
        ),
        "p=1;q=2;{\"p\":1,\"q\":2};T",
    );
}

/// Verifies the narrowing also accepts `Generator`, which is a builtin CLASS, not an interface.
///
/// `php -n` 8.5.6 prints `{"g":3}`. `Generator` reaches the same dead end for the same reason
/// -- no eval declaration -- so pinning it beside the interface case says the fix covers the
/// name resolution rather than one hard-coded interface.
#[test]
fn a_get_iterator_narrowed_to_generator_declares_and_runs() {
    assert_eq!(
        out(
            br#"class AggGen implements IteratorAggregate {
    public function getIterator(): Generator { yield "g" => 3; }
}
echo json_encode([...new AggGen()]);"#
        ),
        "{\"g\":3}",
    );
}

/// Verifies an EVAL-DECLARED interface that extends a builtin one narrows too.
///
/// `php -n` 8.5.6 prints `0`. `Deep extends Iterator` is declared in the fragment, so the
/// interface-side walk has a declaration to follow and then has to keep going into the builtin
/// table -- a fix that only special-cased an undeclared name would stop one step short here.
#[test]
fn an_eval_interface_extending_a_builtin_one_narrows_too() {
    assert_eq!(
        out(
            br#"interface Deep extends Iterator {}
class Narrow implements IteratorAggregate {
    public function getIterator(): Deep { return new class implements Deep {
        public function rewind(): void {}
        public function valid(): bool { return false; }
        public function current(): mixed { return null; }
        public function key(): mixed { return null; }
        public function next(): void {}
    }; }
}
echo count([...new Narrow()]);"#
        ),
        "0",
    );
}
