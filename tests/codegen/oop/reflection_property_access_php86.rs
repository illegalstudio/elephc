//! Purpose:
//! Runs php-src 8.6's ReflectionProperty::isReadable()/isWritable() fixtures.
//!
//! Called from:
//! - `cargo test --test codegen_tests reflection_property_access_php86`.
//!
//! Key details:
//! - Sources are transcribed from `ext/reflection/tests/ReflectionProperty_is*.phpt`.
//! - The readonly fixture replaces `new ReflectionClass($instance)` with a literal class name
//!   because dynamic ReflectionClass construction is not supported, and it omits the
//!   `$this->a = 43` write because readonly reinitialization inside `__clone` is not yet
//!   implemented. The visibility-window assertions are unchanged.

use crate::support::*;

/// php-src `ReflectionProperty_is_readable_visibility`.
#[test]
fn test_reflection_property_is_readable_visibility_php86() {
    let out = compile_and_run(r#"<?php

class A {}

class B extends A {
    public $a;
    protected $b;
    private $c;
    public protected(set) int $d = 42;
}

class C extends B {}

class D extends C {
    public function __get($name) {}
}

class E extends D {
    private $f;

    public function __isset($name) {
        return $name === 'f';
    }
}

function test($scope) {
    $rc = new ReflectionClass(B::class);
    foreach ($rc->getProperties() as $rp) {
        echo $rp->getName() . ' from ' . ($scope ?? 'global') . ': ';
        var_dump($rp->isReadable($scope, $scope && $scope !== 'A' ? new $scope : null));
    }
}

foreach (['A', 'B', 'C', 'D', 'E'] as $scope) {
    test($scope);
}
test(null);"#);
    assert_eq!(out.trim(), r#"a from A: bool(true)
b from A: bool(true)
c from A: bool(false)
d from A: bool(true)
a from B: bool(true)
b from B: bool(true)
c from B: bool(true)
d from B: bool(true)
a from C: bool(true)
b from C: bool(true)
c from C: bool(false)
d from C: bool(true)
a from D: bool(true)
b from D: bool(true)
c from D: bool(true)
d from D: bool(true)
a from E: bool(true)
b from E: bool(true)
c from E: bool(false)
d from E: bool(true)
a from global: bool(true)
b from global: bool(false)
c from global: bool(false)
d from global: bool(true)"#.trim());
}

/// php-src `ReflectionProperty_is_readable_static`.
#[test]
fn test_reflection_property_is_readable_static_php86() {
    let out = compile_and_run(r#"<?php

class A {
    public static $a;
    public static int $b;
    public static int $c = 42;
    protected static int $d = 42;
    private static int $e = 42;
    public private(set) static int $f = 42;
}

$r = new ReflectionProperty('A', 'a');
try {
    $r->isReadable(null, new A);
} catch (Exception $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
}

$rc = new ReflectionClass('A');
foreach ($rc->getProperties() as $rp) {
    echo $rp->getName() . ' from global: ';
    var_dump($rp->isReadable(null));
}"#);
    assert_eq!(out.trim(), r#"ReflectionException: null is expected as object argument for static properties
a from global: bool(true)
b from global: bool(false)
c from global: bool(true)
d from global: bool(false)
e from global: bool(false)
f from global: bool(true)"#.trim());
}

/// php-src `ReflectionProperty_is_readable_hooks`.
#[test]
fn test_reflection_property_is_readable_hooks_php86() {
    let out = compile_and_run(r#"<?php

class A {
    public $a { get => $this->a; }
    public $b { get => 42; }
    public $c { set => $value; }
    public $d { set {} }
    public $e { get => $this->e; set => $value; }
    public $f { get {} set {} }
}

function test($scope) {
    $rc = new ReflectionClass(A::class);
    foreach ($rc->getProperties() as $rp) {
        echo $rp->getName() . ' from ' . ($scope ?? 'global') . ': ';
        var_dump($rp->isReadable($scope, null));
    }
}

test('A');
test(null);"#);
    assert_eq!(out.trim(), r#"a from A: bool(true)
b from A: bool(true)
c from A: bool(true)
d from A: bool(false)
e from A: bool(true)
f from A: bool(true)
a from global: bool(true)
b from global: bool(true)
c from global: bool(true)
d from global: bool(false)
e from global: bool(true)
f from global: bool(true)"#.trim());
}

/// php-src `ReflectionProperty_is_writable_visibility`.
#[test]
fn test_reflection_property_is_writable_visibility_php86() {
    let out = compile_and_run(r#"<?php

class A {}

class B extends A {
    public $a;
    protected $b;
    private $c;
    public private(set) int $d;
    public protected(set) int $e;
    public readonly int $f;
}

class C extends B {}

class D extends C {
    public function __set($name, $value) {}
}

function test($scope) {
    $rc = new ReflectionClass(B::class);
    foreach ($rc->getProperties() as $rp) {
        echo $rp->getName() . ' from ' . ($scope ?? 'global') . ': ';
        var_dump($rp->isWritable($scope, $scope && $scope !== 'A' ? new $scope : null));
    }
}

test('A');
test('B');
test('C');
test('D');
test(null);"#);
    assert_eq!(out.trim(), r#"a from A: bool(true)
b from A: bool(true)
c from A: bool(false)
d from A: bool(false)
e from A: bool(true)
f from A: bool(true)
a from B: bool(true)
b from B: bool(true)
c from B: bool(true)
d from B: bool(true)
e from B: bool(true)
f from B: bool(true)
a from C: bool(true)
b from C: bool(true)
c from C: bool(false)
d from C: bool(false)
e from C: bool(true)
f from C: bool(true)
a from D: bool(true)
b from D: bool(true)
c from D: bool(true)
d from D: bool(false)
e from D: bool(true)
f from D: bool(true)
a from global: bool(true)
b from global: bool(false)
c from global: bool(false)
d from global: bool(false)
e from global: bool(false)
f from global: bool(false)"#.trim());
}

/// php-src `ReflectionProperty_is_writable_static`.
#[test]
fn test_reflection_property_is_writable_static_php86() {
    let out = compile_and_run(r#"<?php

class A {
    public static $a;
    public static int $b;
    public static int $c = 42;
    protected static int $d = 42;
    private static int $e = 42;
    public private(set) static int $f = 42;
}

$r = new ReflectionProperty('A', 'a');
try {
    $r->isWritable(null, new A);
} catch (Exception $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
}

$rc = new ReflectionClass('A');
foreach ($rc->getProperties() as $rp) {
    echo $rp->getName() . ' from global: ';
    var_dump($rp->isWritable(null));
}"#);
    assert_eq!(out.trim(), r#"ReflectionException: null is expected as object argument for static properties
a from global: bool(true)
b from global: bool(true)
c from global: bool(true)
d from global: bool(false)
e from global: bool(false)
f from global: bool(false)"#.trim());
}

/// php-src `ReflectionProperty_is_writable_hooks`.
#[test]
fn test_reflection_property_is_writable_hooks_php86() {
    let out = compile_and_run(r#"<?php

class A {
    public $a { get => $this->a; }
    public $b { get => 42; }
    public $c { set => $value; }
    public $d { set {} }
    public $e { get => $this->e; set => $value; }
    public $f { get {} set {} }
}

function test($scope) {
    $rc = new ReflectionClass(A::class);
    foreach ($rc->getProperties() as $rp) {
        echo $rp->getName() . ' from ' . ($scope ?? 'global') . ': ';
        var_dump($rp->isWritable($scope, null));
    }
}

test('A');
test(null);"#);
    assert_eq!(out.trim(), r#"a from A: bool(true)
b from A: bool(false)
c from A: bool(true)
d from A: bool(true)
e from A: bool(true)
f from A: bool(true)
a from global: bool(true)
b from global: bool(false)
c from global: bool(true)
d from global: bool(true)
e from global: bool(true)
f from global: bool(true)"#.trim());
}

/// php-src `ReflectionProperty_iswritable_readonly`.
#[test]
fn test_reflection_property_iswritable_readonly_php86() {
    let out = compile_and_run(r#"<?php
class A {
    public readonly int $a;
    public readonly int $b;

    public function __construct() {
        $this->a = 42;
    }

    public function __clone() {
        test($this);
    }
}

function test($instance) {
    $rc = new ReflectionClass('A');
    foreach ($rc->getProperties() as $rp) {
        echo $rp->getName() . ' from A: ';
        var_dump($rp->isWritable('A', $instance));
    }
}

test(new A);
$a = clone new A;
test($a);"#);
    assert_eq!(out.trim(), r#"a from A: bool(false)
b from A: bool(true)
a from A: bool(true)
b from A: bool(true)
a from A: bool(false)
b from A: bool(true)"#.trim());
}

