//! Purpose:
//! Regression tests for the declaring-class reflectors of eval-built member reflectors.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval::declaring_class_reflectors` through Rust's test
//!   harness.
//!
//! Key details:
//! - Eval-built members store their declaring class's name; the compiled getters build a
//!   fresh, complete `ReflectionClass` per call through the eval bridge, as php does.
//! - Fixtures run under heap debug inside a function so every reflector is released before
//!   the leak summary. Expected output is php 8.5's.

use crate::support::*;

/// `getDeclaringClass()` and `ReflectionParameter::getClass()` on reflectors built inside
/// eval return a new, complete `ReflectionClass` per call: its own methods, properties,
/// constants and nested `getDeclaringClass()` work, two calls never share an object, and
/// nothing outlives the function.
#[test]
fn test_eval_declaring_class_reflectors_are_complete_and_fresh() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function probe(): void {
    eval('interface EvalDeclShape { public function area(): float; }
class EvalDeclBase implements EvalDeclShape { public int $a = 1; const K = 3; public function area(): float { return 1.0; } public function f(int $x, EvalDeclBase $b): int { return $x; } }
class EvalDeclChild extends EvalDeclBase { public string $c = "c"; public function g(?EvalDeclChild $y = null): void {} }
function evalDeclNames(array $items): string { $out = []; foreach ($items as $item) { $out[] = $item->getName(); } return implode(",", $out); }
$r = new ReflectionClass("EvalDeclChild");
$g = $r->getMethod("g");
$f = $r->getMethod("f");
$dg = $g->getDeclaringClass(); $df = $f->getDeclaringClass();
echo $dg->getName(), ":", evalDeclNames($dg->getMethods()), "\n";
echo $df->getName(), ":", evalDeclNames($df->getMethods()), "\n";
echo $df->hasConstant("K") ? $df->getConstant("K") : "none", "|", evalDeclNames($df->getProperties()), "\n";
echo $df->getMethod("f")->getDeclaringClass()->getName(), "|", $df->getMethod("f")->getDeclaringClass()->getMethod("area")->getName(), "\n";
echo $r->getParentClass()->getName(), ":", count($r->getParentClass()->getMethods()), "|", $dg === $g->getDeclaringClass() ? "same" : "fresh", "\n";
$a = $r->getProperty("a"); echo $a->getDeclaringClass()->getName(), ":", count($a->getDeclaringClass()->getProperties()), "|", $a->getDeclaringClass() === $a->getDeclaringClass() ? "same" : "fresh", "\n";
$k = $r->getReflectionConstant("K"); echo $k->getDeclaringClass()->getName(), ":", $k->getDeclaringClass() === $k->getDeclaringClass() ? "same" : "fresh", "\n";
$p = $f->getParameters()[1]; echo $p->getDeclaringClass()->getName(), ":", count($p->getDeclaringClass()->getMethods()), "|", $p->getDeclaringClass() === $p->getDeclaringClass() ? "same" : "fresh", "\n";
echo $p->getClass()->getName(), ":", count($p->getClass()->getMethods()), "|", $p->getClass() === $p->getClass() ? "same" : "fresh", "\n";');
}
probe();
"#,
    );
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "EvalDeclChild:g,area,f\n",
            "EvalDeclBase:area,f\n",
            "3|a\n",
            "EvalDeclBase|area\n",
            "EvalDeclBase:2|fresh\n",
            "EvalDeclBase:1|fresh\n",
            "EvalDeclBase:fresh\n",
            "EvalDeclBase:2|fresh\n",
            "EvalDeclBase:2|fresh\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Compiled code calling `getDeclaringClass()` on an eval-built reflector still gets a fresh,
/// complete reflector after the eval context that built it has ended (the reflector escaped
/// the function whose eval declared its class), and for a compiled class reflected by eval.
#[test]
fn test_eval_reflector_builds_declaring_class_after_its_eval_context_ends() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class AotDeclTarget { public function run(): void {} }
function makeEvalReflector() {
    return eval('class EvalEscapedDecl { public function m(): void {} public function n(): void {} }
return new ReflectionMethod("EvalEscapedDecl", "m");');
}
function probe(): void {
    $m = makeEvalReflector();
    $d = $m->getDeclaringClass();
    echo $m->class, ":", $d->getName(), ":", count($d->getMethods()), ":", $d === $m->getDeclaringClass() ? "same" : "fresh", "\n";
    $aot = eval('return new ReflectionMethod("AotDeclTarget", "run");');
    $ad = $aot->getDeclaringClass();
    echo $aot->class, ":", $ad->getName(), ":", count($ad->getMethods()), ":", $ad === $aot->getDeclaringClass() ? "same" : "fresh", "\n";
}
probe();
"#,
    );
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "EvalEscapedDecl:EvalEscapedDecl:2:fresh\nAotDeclTarget:AotDeclTarget:1:fresh\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
