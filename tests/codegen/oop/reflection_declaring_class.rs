//! Purpose:
//! Integration tests for `ReflectionMethod::getDeclaringClass()` and
//! `ReflectionProperty::getDeclaringClass()`, which a compiled reflector builds on demand.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A compiled member reflector records the address of its declaring class's shared
//!   materializer, and `getDeclaringClass()` calls it; building the object with every reflector
//!   made each one a graph that a `--web` request then tore down.
//! - Every call returns a fresh `ReflectionClass`, as php's does.
//! - A reflector the eval interpreter built still carries the object itself.
//! - A temporary `ReflectionClass` whose member list is answered at compile time is released.
//! - Expected output is php's own (`php -n`) for the same program.

use super::*;

/// Verifies the declaring class of every listed and constructed member, and that each call
/// returns a fresh object as php's does (`===` false, `==` true).
#[test]
fn test_reflection_declaring_class_is_built_per_call() {
    let out = compile_and_run(
        r###"<?php
class Base { public $a = 1; protected static $s = 2; public function f() {} private function g() {} }
class Kid extends Base { public $b = 3; public function h() {} }
$rc = new ReflectionClass(Kid::class);
foreach ($rc->getProperties() as $p) {
    echo $p->getName(), " ", $p->class, " ", $p->getDeclaringClass()->getName(), "\n";
}
foreach ($rc->getMethods() as $m) {
    echo $m->getName(), " ", $m->class, " ", $m->getDeclaringClass()->getName(), "\n";
}
$p = new ReflectionProperty(Kid::class, 'a');
echo $p->class, " ", $p->getDeclaringClass()->getName(), "\n";
$m = new ReflectionMethod(Kid::class, 'f');
echo $m->class, " ", $m->getDeclaringClass()->getName(), "\n";
$first = $p->getDeclaringClass();
$second = $p->getDeclaringClass();
var_dump($first === $second);
var_dump($first == $second);
echo implode(",", array_map(fn($x) => $x->name, $m->getDeclaringClass()->getMethods())), "\n";
"###,
    );
    assert_eq!(
        out,
        "b Kid Kid\na Base Base\ns Base Base\nh Kid Kid\nf Base Base\nBase Base\nBase Base\n\
         bool(false)\nbool(true)\nf,g\n"
    );
}

/// Verifies a static property read through `ReflectionProperty` sees the live value: the
/// static branches of `getValue()` and `isInitialized()` go through `getDeclaringClass()`.
#[test]
fn test_reflection_static_property_reads_through_the_declaring_class() {
    let out = compile_and_run(
        r###"<?php
class Cfg { public static $level = 1; protected static ?array $cache = null; public static int $typed; }
$p = new ReflectionProperty(Cfg::class, 'level');
Cfg::$level = 5;
var_dump($p->getValue());
var_dump($p->isInitialized());
$t = new ReflectionProperty(Cfg::class, 'typed');
var_dump($t->isInitialized());
Cfg::$typed = 3;
var_dump($t->isInitialized());
foreach ((new ReflectionClass(Cfg::class))->getProperties() as $prop) {
    if ($prop->isStatic() && $prop->isInitialized()) {
        echo $prop->getName(), '=', var_export($prop->getValue(), true), "\n";
    }
}
"###,
    );
    assert_eq!(
        out,
        "int(5)\nbool(true)\nbool(false)\nbool(true)\nlevel=5\ncache=NULL\ntyped=3\n"
    );
}

/// Verifies compiled and interpreter-built reflectors both answer across the eval boundary:
/// eval code reads a compiled reflector's declaring class and invokes through it, and compiled
/// code reads the declaring class of a reflector the interpreter built.
#[test]
fn test_reflection_declaring_class_across_the_eval_boundary() {
    let out = compile_and_run(
        r###"<?php
class Base { public $a = 1; public static $s = 7; public function f() { return 'f:' . static::class; } }
class Kid extends Base {}
$p = new ReflectionProperty(Kid::class, 'a');
$m = new ReflectionMethod(Kid::class, 'f');
echo eval('return $p->getDeclaringClass()->getName();'), "\n";
echo eval('return $m->class . "/" . $m->getDeclaringClass()->name;'), "\n";
echo eval('return $m->invoke(new Kid());'), "\n";
$rm = eval('return new ReflectionMethod("Kid", "f");');
echo $rm->getDeclaringClass()->getName(), " ", $rm->class, "\n";
$rp = eval('return new ReflectionProperty("Kid", "s");');
echo $rp->getDeclaringClass()->getName(), " ", $rp->getValue(), "\n";
"###,
    );
    assert_eq!(out, "Base\nBase/Base\nf:Kid\nBase Base\nBase 7\n");
}

/// Reads `live_blocks=<n>` from a heap-debug leak summary.
fn live_blocks(stderr: &str) -> usize {
    let summary = stderr
        .lines()
        .find(|line| line.contains("leak summary"))
        .unwrap_or_else(|| panic!("no leak summary in: {stderr}"));
    if summary.contains("clean") {
        return 0;
    }
    summary
        .split("live_blocks=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("unreadable leak summary: {summary}"))
}

/// Verifies building, reading and dropping member reflectors and their declaring classes holds
/// no memory per iteration: ten and forty iterations end with the same live blocks.
///
/// The count is compared between two runs rather than required to be clean, because blocks
/// that live for the whole process are not a leak; a per-iteration leak grows with the count.
/// The eagerly built declaring classes held 163 blocks per iteration of this loop.
#[test]
fn test_reflection_declaring_class_holds_no_memory_per_call() {
    let program = |iterations: usize| {
        format!(
            r###"<?php
class Base {{ public $a = 1; protected static $s = 2; public function f() {{}} }}
class Kid extends Base {{ public $b = 3; }}
function once(): string {{
    $p = new ReflectionProperty(Kid::class, 'a');
    $names = [$p->getDeclaringClass()->getName()];
    $s = new ReflectionProperty(Base::class, 's');
    $names[] = (string) $s->getValue();
    $names[] = $s->isInitialized() ? 'y' : 'n';
    $m = new ReflectionMethod(Kid::class, 'f');
    $names[] = $m->getDeclaringClass()->name;
    $rc = new ReflectionClass(Kid::class);
    foreach ($rc->getProperties() as $prop) {{
        $names[] = $prop->getDeclaringClass()->getName();
    }}
    return implode(',', $names);
}}
$last = '';
for ($i = 0; $i < {iterations}; $i++) {{
    $last = once();
}}
echo $last, "\n";
"###
        )
    };
    let few = compile_and_run_with_heap_debug(&program(10));
    let many = compile_and_run_with_heap_debug(&program(40));
    assert_eq!(few.stdout, "Base,2,y,Base,Kid,Base,Base\n");
    assert_eq!(many.stdout, "Base,2,y,Base,Kid,Base,Base\n");
    assert_eq!(
        live_blocks(&few.stderr),
        live_blocks(&many.stderr),
        "live blocks grew with the iteration count:\n10: {}\n40: {}",
        few.stderr,
        many.stderr
    );
}

/// Verifies a temporary `ReflectionClass` receiver is released when its member list or static
/// property values are answered from compile-time metadata: ten and forty iterations end with
/// the same live blocks.
///
/// Those calls never read the receiver, and the interception returned without releasing it, so
/// `(new ReflectionClass(X::class))->getMethods()` kept the whole class graph alive per call;
/// through a local the same calls were already flat.
#[test]
fn test_reflection_member_lists_release_a_temporary_receiver() {
    let program = |iterations: usize| {
        format!(
            r###"<?php
class Base {{ public $a = 1; protected static $s = 2; public function f() {{}} }}
class Kid extends Base {{ public $b = 3; }}
function once(): string {{
    $names = [];
    foreach ((new ReflectionClass(Kid::class))->getProperties() as $p) {{
        $names[] = $p->name;
    }}
    foreach ((new ReflectionClass(Kid::class))->getMethods() as $m) {{
        $names[] = $m->name;
    }}
    $names[] = (string) count((new ReflectionClass(Base::class))->getStaticProperties());
    $names[] = (string) (new ReflectionClass(Base::class))->getStaticPropertyValue('s');
    return implode(',', $names);
}}
$last = '';
for ($i = 0; $i < {iterations}; $i++) {{
    $last = once();
}}
echo $last, "\n";
"###
        )
    };
    let few = compile_and_run_with_heap_debug(&program(10));
    let many = compile_and_run_with_heap_debug(&program(40));
    assert_eq!(few.stdout, "b,a,s,f,1,2\n");
    assert_eq!(many.stdout, "b,a,s,f,1,2\n");
    assert_eq!(
        live_blocks(&few.stderr),
        live_blocks(&many.stderr),
        "live blocks grew with the iteration count:\n10: {}\n40: {}",
        few.stderr,
        many.stderr
    );
}

/// Verifies `newInstance()`, `newInstanceArgs()` and `newInstanceWithoutConstructor()` release
/// the class name they read from the reflector: ten and forty iterations end with the same live
/// blocks. The name kept its own reference and nothing consumed it, so each call leaked it.
#[test]
fn test_reflection_new_instance_releases_the_class_name() {
    let program = |iterations: usize| {
        format!(
            r###"<?php
class Kid {{ public $b = 3; public function __construct(public int $n = 1) {{}} }}
function once(): string {{
    $rc = new ReflectionClass(Kid::class);
    $one = $rc->newInstance(2);
    $two = $rc->newInstanceArgs([4]);
    $bare = $rc->newInstanceWithoutConstructor();
    $rc->newInstance();
    return $one->n . ',' . $two->n . ',' . $bare->b;
}}
$last = '';
for ($i = 0; $i < {iterations}; $i++) {{
    $last = once();
}}
echo $last, "\n";
"###
        )
    };
    let few = compile_and_run_with_heap_debug(&program(10));
    let many = compile_and_run_with_heap_debug(&program(40));
    assert_eq!(few.stdout, "2,4,3\n");
    assert_eq!(many.stdout, "2,4,3\n");
    assert_eq!(
        live_blocks(&few.stderr),
        live_blocks(&many.stderr),
        "live blocks grew with the iteration count:\n10: {}\n40: {}",
        few.stderr,
        many.stderr
    );
}
