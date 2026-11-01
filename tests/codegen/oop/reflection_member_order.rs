//! Purpose:
//! Integration tests for the members php's reflection lists, and their order.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `getProperties()`: own declarations as written (static and instance interleaved,
//!   promoted properties at the constructor), trait properties after them, then what the
//!   parent lists minus its privates and minus redeclared names.
//! - `getMethods()`: the same shape for methods. The signature maps are hashed, so an order
//!   read from them is the hasher's, not php's.
//! - An ancestor's private property is not reflectable through a descendant.
//! - Expected output is php's own (`php -n`) for the same program.

use super::*;

/// Verifies `new ReflectionProperty`, `getProperty`, `hasProperty` and `getProperties` agree with
/// php about an ancestor's private properties: a descendant does not have them.
///
/// `new ReflectionProperty(Child::class, 'secret')` used to succeed and name `Base`, where php
/// throws `ReflectionException`.
#[test]
fn test_reflection_property_refuses_an_ancestors_private_property() {
    let out = compile_and_run(
        r###"<?php
class Base {
    private string $secret = 'base';
    protected int $shared = 1;
    private static int $hidden = 2;
    public int $open = 3;
}
class Child extends Base {
    private string $own = 'child';
}
class GrandChild extends Child {}

function attempt(string $class, string $name): string {
    try {
        $r = new ReflectionProperty($class, $name);
        return $r->getName() . '@' . $r->class;
    } catch (ReflectionException $e) {
        return 'E:' . $e->getMessage();
    }
}

foreach (['Base', 'Child', 'GrandChild'] as $class) {
    foreach (['secret', 'shared', 'hidden', 'open', 'own'] as $name) {
        echo $class, '::', $name, ' ', attempt($class, $name), "\n";
    }
}
echo "literal: ";
try {
    $r = new ReflectionProperty(Child::class, 'secret');
    echo $r->class, "\n";
} catch (ReflectionException $e) {
    echo 'E:', $e->getMessage(), "\n";
}
foreach (['Base', 'Child', 'GrandChild'] as $class) {
    $rc = new ReflectionClass($class);
    $names = array_map(fn ($p) => $p->getName() . '@' . $p->class, $rc->getProperties());
    echo $class, ' props: ', implode(',', $names), "\n";
    echo $class, ' has: ', json_encode([
        $rc->hasProperty('secret'), $rc->hasProperty('hidden'), $rc->hasProperty('shared'), $rc->hasProperty('own'),
    ]), "\n";
    try {
        echo $class, ' get: ', $rc->getProperty('secret')->class, "\n";
    } catch (ReflectionException $e) {
        echo $class, ' get: E:', $e->getMessage(), "\n";
    }
}
"###,
    );
    assert_eq!(
        out,
        "Base::secret secret@Base\nBase::shared shared@Base\nBase::hidden hidden@Base\nBase::open open@Base\nBase::own E:Property Base::$own does not exist\nChild::secret E:Property Child::$secret does not exist\nChild::shared shared@Base\nChild::hidden E:Property Child::$hidden does not exist\nChild::open open@Base\nChild::own own@Child\nGrandChild::secret E:Property GrandChild::$secret does not exist\nGrandChild::shared shared@Base\nGrandChild::hidden E:Property GrandChild::$hidden does not exist\nGrandChild::open open@Base\nGrandChild::own E:Property GrandChild::$own does not exist\nliteral: E:Property Child::$secret does not exist\nBase props: secret@Base,shared@Base,hidden@Base,open@Base\nBase has: [true,true,true,false]\nBase get: Base\nChild props: own@Child,shared@Base,open@Base\nChild has: [false,false,true,true]\nChild get: Child get: E:Property Child::$secret does not exist\nGrandChild props: shared@Base,open@Base\nGrandChild has: [false,false,true,false]\nGrandChild get: GrandChild get: E:Property GrandChild::$secret does not exist\n"
    );
}

/// Verifies `getProperties()` order with a trait, statics, a promoted property, a redeclared
/// public property and a shadowed private one, across three levels.
#[test]
fn test_reflection_get_properties_lists_php_order() {
    let out = compile_and_run(
        r###"<?php
trait T { public $t = 0; private $tp = 0; }
class A { public $x = 1; public $y = 2; private $p = 0; protected static $s = 0; }
class B extends A {
    use T;
    public $y = 3;
    public static $bs = 0;
    private $p = 5;
    public function __construct(public int $promo = 0) {}
    public $z = 4;
}
class C extends B {}
foreach (['A', 'B', 'C'] as $c) {
    echo $c, ': ', implode(',', array_map(fn ($p) => $p->getName() . '@' . $p->class, (new ReflectionClass($c))->getProperties())), "\n";
}
"###,
    );
    assert_eq!(
        out,
        "A: x@A,y@A,p@A,s@A\nB: y@B,bs@B,p@B,promo@B,z@B,t@B,tp@B,x@A,s@A\nC: y@B,bs@B,promo@B,z@B,t@B,x@A,s@A\n"
    );
}

/// Verifies the same rules through `ReflectionObject`, a static filter and `getProperty()`.
#[test]
fn test_reflection_private_properties_through_objects_and_filters() {
    let out = compile_and_run(
        r###"<?php
class Base {
    private string $secret = 'base';
    protected int $shared = 1;
    private static int $hidden = 2;
    public int $open = 3;
}
class Child extends Base {
    private string $own = 'child';
}
class GrandChild extends Child {}

$rc = new ReflectionClass(Child::class);
echo 'Child props: ', implode(',', array_map(fn ($p) => $p->getName() . '@' . $p->class, $rc->getProperties())), "\n";
echo 'Child has: ', json_encode([$rc->hasProperty('secret'), $rc->hasProperty('hidden'), $rc->hasProperty('shared')]), "\n";
$rg = new ReflectionClass(GrandChild::class);
echo 'GrandChild props: ', implode(',', array_map(fn ($p) => $p->getName() . '@' . $p->class, $rg->getProperties())), "\n";
$rb = new ReflectionClass(Base::class);
echo 'Base props: ', implode(',', array_map(fn ($p) => $p->getName() . '@' . $p->class, $rb->getProperties())), "\n";
echo 'Base static: ', implode(',', array_map(fn ($p) => $p->getName(), $rb->getProperties(ReflectionProperty::IS_STATIC))), "\n";
$ro = new ReflectionObject(new GrandChild());
echo 'object props: ', implode(',', array_map(fn ($p) => $p->getName() . '@' . $p->class, $ro->getProperties())), "\n";
try {
    $r = new ReflectionProperty(GrandChild::class, 'own');
    echo 'literal own: ', $r->class, "\n";
} catch (ReflectionException $e) {
    echo 'literal own: E:', $e->getMessage(), "\n";
}
try {
    echo 'getProperty: ', $rg->getProperty('secret')->class, "\n";
} catch (ReflectionException $e) {
    echo 'E:', $e->getMessage(), "\n";
}
"###,
    );
    assert_eq!(
        out,
        "Child props: own@Child,shared@Base,open@Base\nChild has: [false,false,true]\nGrandChild props: shared@Base,open@Base\nBase props: secret@Base,shared@Base,hidden@Base,open@Base\nBase static: hidden\nobject props: shared@Base,open@Base\nliteral own: E:Property GrandChild::$own does not exist\ngetProperty: E:Property GrandChild::$secret does not exist\n"
    );
}

/// Verifies `getMethods()` order with a trait, statics, abstract and private methods, and an
/// overridden parent method, across three levels, plus `hasMethod()` on an ancestor's private.
#[test]
fn test_reflection_get_methods_lists_php_order() {
    let out = compile_and_run(
        r###"<?php
trait T { public function tm() {} private function tp() {} }
abstract class A {
    public function a1() {}
    private function ap() {}
    protected static function as1() {}
    public function shared() {}
    abstract public function abs();
}
class B extends A {
    use T;
    public function shared() {}
    public static function bs() {}
    public function abs() {}
    private function bp() {}
    public function __construct() {}
}
class C extends B {}
foreach (['A', 'B', 'C'] as $c) {
    echo $c, ': ', implode(',', array_map(fn ($m) => $m->getName() . '@' . $m->class, (new ReflectionClass($c))->getMethods())), "\n";
    echo $c, ' static: ', implode(',', array_map(fn ($m) => $m->getName(), (new ReflectionClass($c))->getMethods(ReflectionMethod::IS_STATIC))), "\n";
}
$rc = new ReflectionClass(C::class);
echo json_encode([$rc->hasMethod('ap'), $rc->hasMethod('bp'), $rc->hasMethod('tp'), $rc->hasMethod('A1')]), "\n";
"###,
    );
    assert_eq!(
        out,
        "A: a1@A,ap@A,as1@A,shared@A,abs@A\nA static: as1\nB: shared@B,bs@B,abs@B,bp@B,__construct@B,tm@B,tp@B,a1@A,as1@A\nB static: bs,as1\nC: shared@B,bs@B,abs@B,__construct@B,tm@B,a1@A,as1@A\nC static: bs,as1\n[true,true,true,true]\n"
    );
}

/// Verifies `getMethods()` returns the declared spelling in declaration order.
#[test]
fn test_reflection_get_methods_keeps_declared_case_and_order() {
    let out = compile_and_run(
        r###"<?php
class ReflectCasePreservedTarget {
    public function CamelCaseOne(): string { return "one"; }
    public static function StaticTwo(): string { return "two"; }
    public function lower(): string { return "l"; }
}

$class = new ReflectionClass(ReflectCasePreservedTarget::class);
$names = [];
foreach ($class->getMethods() as $method) {
    $names[] = $method->getName();
}
echo implode(",", $names) . "|";
sort($names);
echo implode(",", $names) . ":";
echo $class->getMethod("camelcaseone")->getName() . ":";
echo $class->getMethod("CamelCaseOne")->getName() . ":";
echo $class->getMethod("statictwo")->getName() . ":";
echo (new ReflectionMethod(ReflectCasePreservedTarget::class, "camelcaseone"))->getName() . ":";
echo (new ReflectionMethod(ReflectCasePreservedTarget::class, "CamelCaseOne"))->getName() . ":";
echo $class->hasMethod("CAMELCASEONE") ? "H" : "h";
echo "\n";
"###,
    );
    assert_eq!(
        out,
        "CamelCaseOne,StaticTwo,lower|CamelCaseOne,StaticTwo,lower:CamelCaseOne:CamelCaseOne:StaticTwo:CamelCaseOne:CamelCaseOne:H\n"
    );
}
