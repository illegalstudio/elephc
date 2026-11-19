//! Purpose:
//! Runs php-src 8.6's readonly-property-default fixtures through the native backend.
//!
//! Called from:
//! - `cargo test --test codegen_tests readonly_property_defaults_php86`.
//!
//! Key details:
//! - Sources and expected outputs are transcribed from
//!   `Zend/tests/readonly_props/readonly_with_default*.phpt` and the defaulted-property
//!   portion of `serialization.phpt`.
//! - php-src's `run-tests.php` trims both sides before comparing.
//! - The dynamic-name write and `unset()` enforcement fixtures stay out: elephc's runtime
//!   property dispatcher does not yet consult readonly state for those two paths.

use crate::support::*;

/// php-src `readonly_with_default.phpt`.
#[test]
fn test_readonly_with_default_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php

enum E {
    case Case;
}

class Test {
    public readonly int $prop = 1;
    public readonly string $className = self::class;
    public readonly ?string $nullable = null;
    public readonly array $array = [1, "two" => 2];
    public readonly E $enum = E::Case;
    public readonly string $enumString = E::Case->name;
}

$test = new Test;
var_dump($test->prop);
var_dump($test->className);
var_dump($test->nullable);
var_dump($test->array);
var_dump($test->enum);
var_dump($test->enumString);
try {
    $test->prop = 2;
} catch (Error $e) {
    echo $e::class, ': ', $e->getMessage(), PHP_EOL;
}

"#);
    assert_eq!(out.trim(), r#"int(1)
string(4) "Test"
NULL
array(2) {
  [0]=>
  int(1)
  ["two"]=>
  int(2)
}
enum(E::Case)
string(4) "Case"
Error: Cannot modify readonly property Test::$prop
"#.trim());
}

/// php-src `readonly_with_default_inheritance.phpt`.
#[test]
fn test_readonly_with_default_inheritance_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php

class ParentDefault {
    public readonly int $prop = 1;
}

class ChildInherits extends ParentDefault {}

class ChildOverrides extends ParentDefault {
    public readonly int $prop = 2;
}

class PrivateParent {
    private readonly int $prop = 3;

    public function getParentProp(): int {
        return $this->prop;
    }
}

class PrivateChild extends PrivateParent {
    public readonly int $prop = 4;
}

var_dump(new ChildInherits()->prop);
var_dump(new ChildOverrides()->prop);

$privateChild = new PrivateChild();
var_dump($privateChild->getParentProp());
var_dump($privateChild->prop);

"#);
    assert_eq!(out.trim(), r#"int(1)
int(2)
int(3)
int(4)
"#.trim());
}

/// php-src `readonly_with_default_interface_get_only.phpt`.
#[test]
fn test_readonly_with_default_interface_get_only_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php

interface I {
    public int $prop { get; }
}

class C implements I {
    public readonly int $prop = 42;
}

var_dump(new C()->prop);
"#);
    assert_eq!(out.trim(), r#"int(42)
"#.trim());
}

/// The defaulted-property part of php-src `serialization.phpt`.
#[test]
fn test_readonly_with_default_serialization_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php
class TestDefault {
    public readonly int $prop = 1;
}

var_dump($s = serialize(new TestDefault));
var_dump(unserialize($s));

var_dump(unserialize("O:11:\"TestDefault\":0:{}"));
var_dump(unserialize("O:11:\"TestDefault\":1:{s:4:\"prop\";i:2;}"));
"#);
    assert_eq!(out.trim(), r#"string(38) "O:11:"TestDefault":1:{s:4:"prop";i:1;}"
object(TestDefault)#1 (1) {
  ["prop"]=>
  int(1)
}
object(TestDefault)#1 (1) {
  ["prop"]=>
  int(1)
}
object(TestDefault)#1 (1) {
  ["prop"]=>
  int(2)
}
"#.trim());
}
