//! Purpose:
//! Interpreter tests pinning that ReflectionProperty reads and writes the SAME property store
//! an ordinary `$object->property` access uses.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::reflection_property_store`.
//!
//! Key details:
//! - An eval-declared object keeps its properties in `ElephcEvalContext`, not in the runtime
//!   object's slots. Reflection used to read the runtime slots, which are empty for such an
//!   object, so `getValue()` answered null until a reflection write happened to populate the
//!   slot it was reading.
//! - The reads and the writes are pinned together on purpose: reading the right store while
//!   writing the wrong one leaves the two views disagreeing, which is the state that made the
//!   value appear only on the second read.
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse reflection property fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute reflection property fragment");
    values.output.clone()
}

/// Verifies a declared property's default is visible to reflection before anything writes it.
///
/// `php -n` 8.5.6 prints `s;`. This is the reduced case: the value was `` because the default
/// went into the interpreter's own store at construction while reflection read the runtime
/// object's slots, which no one had written.
#[test]
fn reflection_reads_a_declared_default_without_a_prior_write() {
    assert_eq!(
        out(br#"class N1 { public $id = "s"; } $o = new N1(); $p = new ReflectionProperty("N1", "id"); echo $p->getValue($o); echo ";";"#),
        "s;",
    );
}

/// Verifies reflection and ordinary access agree in BOTH directions after each writes.
///
/// `php -n` 8.5.6 prints `s;w;w;z;`: read the default, write through reflection, read that
/// write back through ordinary access, then write ordinarily and read it back through
/// reflection. Before the fix the two directions used different stores, so a reflection write
/// was invisible to `$o->id` and an ordinary write was invisible to `getValue()`.
#[test]
fn reflection_and_ordinary_access_share_one_property_store() {
    assert_eq!(
        out(
            br#"class N2 { public $id = "s"; }
$o = new N2();
$p = new ReflectionProperty("N2", "id");
echo $p->getValue($o); echo ";";
$p->setValue($o, "w");
echo $p->getValue($o); echo ";";
echo $o->id; echo ";";
$o->id = "z";
echo $p->getValue($o); echo ";";"#
        ),
        "s;w;w;z;",
    );
}

/// Verifies a private typed property reads back its default and reports itself initialized.
///
/// `php -n` 8.5.6 prints `4;init;`. A private property is stored under a mangled name, so this
/// pins that the store change kept the name mangling that already worked.
#[test]
fn reflection_reads_a_private_typed_default_and_its_initialized_state() {
    assert_eq!(
        out(
            br#"class P1 { private int $n = 4; }
$q = new P1();
$r = new ReflectionProperty("P1", "n");
echo $r->getValue($q); echo ";";
echo $r->isInitialized($q) ? "init" : "uninit"; echo ";";"#
        ),
        "4;init;",
    );
}

/// Verifies an inherited protected property is read through a child instance.
///
/// `php -n` 8.5.6 prints `base;`. The declaring class and the object's class differ here, which
/// is the shape a container reflecting on a service hierarchy actually produces.
#[test]
fn reflection_reads_an_inherited_property_through_a_child_instance() {
    assert_eq!(
        out(
            br#"class Q1 { protected $slot = "base"; }
class Q2 extends Q1 {}
$o = new Q2();
$p = new ReflectionProperty("Q1", "slot");
echo $p->getValue($o); echo ";";"#
        ),
        "base;",
    );
}

/// Verifies the raw-value APIs read the same store as the ordinary ones.
///
/// `php -n` 8.5.6 prints `2;9;9;`. `getRawValue()` bypasses a property's hooks but not its
/// storage, so it has to consult the store the value actually lives in.
#[test]
fn reflection_raw_value_apis_use_the_same_property_store() {
    assert_eq!(
        out(
            br#"class R1 { public $raw = 2; }
$o = new R1();
$p = new ReflectionProperty("R1", "raw");
echo $p->getRawValue($o); echo ";";
$p->setRawValue($o, 9);
echo $p->getRawValue($o); echo ";";
echo $o->raw; echo ";";"#
        ),
        "2;9;9;",
    );
}
