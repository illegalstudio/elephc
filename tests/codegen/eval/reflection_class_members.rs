//! Purpose:
//! Regression tests for compiled code reading the member lists of eval-built reflectors.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval::reflection_class_members` through Rust's test
//!   harness.
//!
//! Key details:
//! - `__methods`, `__properties`, `__parameters` and `__types` are typed `array<Class>`, whose
//!   compiled readers (`getMethod()`, `getProperty()`, a typed `foreach`) expect one object
//!   pointer per slot. Eval used to store boxed Mixed arrays there, so `getProperty("q")` on an
//!   eval-built `ReflectionClass` read Mixed cells as objects and exhausted the heap.
//! - Member names are sorted before printing: eval lists inherited properties before the
//!   class's own, php the other way round, and the order is not what this test is about.
//! - Fixtures run under heap debug inside a function. Expected output is php 8.5's.

use crate::support::*;

/// Compiled `getMethod()` / `getProperty()` and statically typed iteration over
/// `getMethods()`, `getProperties()`, `getParameters()` and `getTypes()` work on reflectors
/// built inside eval, for a compiled class, an eval-declared one and a `ReflectionObject`,
/// and the heap stays clean.
#[test]
fn test_eval_built_reflector_member_lists_read_as_typed_object_arrays() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class AotMembers { public $p = 1; protected int $q = 2; public function f(int $a, string $b = "x"): void {} public static function g(): int { return 1; } }
function typedIter(ReflectionClass $r): string { $out = []; foreach ($r->getProperties() as $p) { $out[] = "P:" . $p->getName(); } foreach ($r->getMethods() as $m) { $out[] = "M:" . $m->getName(); } sort($out); return implode(",", $out); }
function typedParams(ReflectionMethod $m): string { $out = []; foreach ($m->getParameters() as $p) { $out[] = $p->getName() . "@" . $p->getPosition(); } return implode(",", $out); }
function typedTypes(ReflectionUnionType $u): string { $out = []; foreach ($u->getTypes() as $t) { $out[] = $t->getName(); } return implode(",", $out); }
function probe(): void {
    $aot = eval('return new ReflectionClass("AotMembers");');
    echo typedIter($aot), "|", $aot->getProperty("q")->getName(), "|", $aot->getMethod("F")->getName(), "|", typedParams($aot->getMethod("f")), "\n";
    eval('class EvalBase { public $p = 1; public function f() {} } class EvalChild extends EvalBase { public $q = 2; public function g(int|string $v, ?EvalBase $w = null) {} }');
    $ev = eval('return new ReflectionClass("EvalChild");');
    echo typedIter($ev), "|", $ev->getProperty("q")->getName(), "|", $ev->getMethod("G")->getName(), "|", typedParams($ev->getMethod("g")), "\n";
    try { $ev->getProperty("missing"); } catch (ReflectionException $e) { echo $e->getMessage(), "\n"; }
    try { $ev->getMethod("missing"); } catch (ReflectionException $e) { echo $e->getMessage(), "\n"; }
    $obj = eval('return new ReflectionObject(new EvalChild);');
    echo typedIter($obj), "|", $obj->getMethod("f")->getName(), "\n";
    echo typedTypes(eval('return (new ReflectionMethod("EvalChild", "g"))->getParameters()[0]->getType();')), "\n";
}
probe();
"#,
    );
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "M:f,M:g,P:p,P:q|q|f|a@0,b@1\n",
            "M:f,M:g,P:p,P:q|q|g|v@0,w@1\n",
            "Property EvalChild::$missing does not exist\n",
            "Method EvalChild::missing() does not exist\n",
            "M:f,M:g,P:p,P:q|f\n",
            "string,int\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
