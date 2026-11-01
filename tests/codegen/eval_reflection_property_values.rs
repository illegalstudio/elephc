//! Purpose:
//! End-to-end value tests for `ReflectionProperty::getValue()` and `setValue()` on classes the
//! interpreter declared, where the property store reflection reads must be the store an
//! ordinary `$object->property` access uses.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_reflection_property_values` through Rust's harness.
//!
//! Key details:
//! - An eval-declared object keeps its properties in the interpreter's own per-object store,
//!   filled with every declared default when the object is allocated. Reflection used to read
//!   the runtime object's slots instead, which are empty for such an object, so `getValue()`
//!   answered null until a reflection write happened to populate the slot it read back.
//! - The read and the write are exercised in both directions on purpose: reading one store
//!   while writing the other leaves the two views disagreeing, which is what made a value show
//!   up only on the second read.
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.

use crate::support::*;

/// Verifies a declared default is visible to reflection with no prior write.
///
/// `php -n` 8.5.6 prints `s;`.
#[test]
fn test_eval_reflection_reads_a_declared_default() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class N1 { public $id = "s"; }
$o = new N1();
$p = new ReflectionProperty("N1", "id");
echo $p->getValue($o); echo ";";
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "s;");
}

/// Verifies reflection and ordinary property access agree in both directions.
///
/// `php -n` 8.5.6 prints `s;w;w;z;`: the default, a reflection write read back by reflection,
/// the same write seen by ordinary access, then an ordinary write seen by reflection.
#[test]
fn test_eval_reflection_and_ordinary_access_share_one_store() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class N2 { public $id = "s"; }
$o = new N2();
$p = new ReflectionProperty("N2", "id");
echo $p->getValue($o); echo ";";
$p->setValue($o, "w");
echo $p->getValue($o); echo ";";
echo $o->id; echo ";";
$o->id = "z";
echo $p->getValue($o); echo ";";
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "s;w;w;z;");
}

/// Verifies a private typed property reads its default and reports itself initialized.
///
/// `php -n` 8.5.6 prints `4;init;`. A private property is stored under a mangled name, so this
/// pins that the name mangling survived the store change.
#[test]
fn test_eval_reflection_reads_a_private_typed_default() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class P1 { private int $n = 4; }
$q = new P1();
$r = new ReflectionProperty("P1", "n");
echo $r->getValue($q); echo ";";
echo $r->isInitialized($q) ? "init" : "uninit"; echo ";";
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "4;init;");
}

/// Verifies an inherited property is read through a child instance.
///
/// `php -n` 8.5.6 prints `base;`. The declaring class and the instance's class differ here,
/// which is the shape a container reflecting on a service hierarchy produces.
#[test]
fn test_eval_reflection_reads_an_inherited_property() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class Q1 { protected $slot = "base"; }
class Q2 extends Q1 {}
$o = new Q2();
$p = new ReflectionProperty("Q1", "slot");
echo $p->getValue($o); echo ";";
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "base;");
}

/// Verifies the same holds for a class a file included at RUN TIME declared.
///
/// `php -n` 8.5.6 prints `loaded;s;w;w;`. This is the autoload-shaped path: the class arrives
/// through an include whose path is not a literal, and a container then reflects on it.
#[test]
fn test_runtime_include_reflection_property_values_round_trip() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$name = \"piece\" . \".php\";\ninclude $name;\n",
            ),
            (
                "piece.php",
                r#"<?php
class S2Store { public $id = "s"; }
echo "loaded;";
$o = new S2Store();
$p = new ReflectionProperty("S2Store", "id");
echo $p->getValue($o); echo ";";
$p->setValue($o, "w");
echo $p->getValue($o); echo ";";
echo $o->id; echo ";";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded;s;w;w;");
}
