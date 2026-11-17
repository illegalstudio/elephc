//! Purpose:
//! Regression tests for the `ReflectionEnum` php's class factory hands back for enum members.
//!
//! Called from:
//! - `cargo test --test codegen_tests reflection_enum_declaring_class` through Rust's test
//!   harness.
//!
//! Key details:
//! - `getDeclaringClass()` of an enum case, constant or method and `ReflectionParameter::
//!   getClass()` of an enum-typed parameter reflect the enum as `ReflectionEnum`, both from
//!   the compiled class builders and from the eval bridge (`__elephc_eval_reflection_class_build`).
//! - Enum case getters are typed `ReflectionEnum`, so `isBacked()` and `getBackingType()`
//!   type-check on their result. Expected output is php 8.5's.
//! - No heap assertion: the reflected enum cases are process-lifetime singletons.

use crate::support::*;

/// Every class getter reflects an enum as a `ReflectionEnum` with its backing metadata, fresh
/// per call, for compiled enums and for enums declared inside eval, whether the reflector is
/// used inside the fragment or handed to compiled code; `new ReflectionClass(Enum)` stays a
/// `ReflectionClass`, eval's `getCases()` lists the cases, and both enum reflectors answer
/// php's empty `getDefaultProperties()`.
#[test]
fn test_enum_members_reflect_their_declaring_class_as_reflection_enum() {
    let out = compile_and_run_capture(
        r#"<?php
enum Suit: string { case Hearts = "H"; case Spades = "S"; const Wild = self::Spades; public function label(): string { return $this->name; } }
enum Plain { case One; case Two; }
function takes(Suit $s): void {}
function describe(ReflectionClass $c): string { return get_class($c) . ":" . $c->getName() . ":" . ($c->isEnum() ? "enum" : "class") . ":" . count($c->getConstants()); }
function probe(): void {
    $hearts = new ReflectionEnumBackedCase("Suit", "Hearts");
    $decl = $hearts->getDeclaringClass();
    echo describe($decl), "|", $decl->isBacked() ? "backed" : "unit", "|", $decl->getBackingType()->getName(), "|", $decl === $hearts->getDeclaringClass() ? "same" : "fresh", "\n";
    $one = new ReflectionEnumUnitCase("Plain", "One");
    echo describe($one->getDeclaringClass()), "|", $one->getDeclaringClass()->isBacked() ? "backed" : "unit", "\n";
    echo describe((new ReflectionClassConstant("Suit", "Wild"))->getDeclaringClass()), "\n";
    echo describe((new ReflectionMethod("Suit", "label"))->getDeclaringClass()), "\n";
    echo describe((new ReflectionFunction("takes"))->getParameters()[0]->getClass()), "\n";
    $evalCase = eval('enum EvalSuit: int { case A = 1; case B = 2; public function twice(): int { return $this->value * 2; } }
$cases = (new ReflectionEnum("EvalSuit"))->getCases();
echo get_class((new ReflectionEnumBackedCase("EvalSuit", "A"))->getDeclaringClass()), "|", get_class((new ReflectionMethod("EvalSuit", "twice"))->getDeclaringClass()), "|", get_class(new ReflectionClass("EvalSuit")), "|", count($cases), ":", $cases[1]->getName(), "\n";
return new ReflectionEnumBackedCase("EvalSuit", "B");');
    $evalDecl = $evalCase->getDeclaringClass();
    echo describe($evalDecl), "|", $evalDecl->isBacked() ? "backed" : "unit", "|", $evalDecl->getBackingType()->getName(), "|", $evalDecl === $evalCase->getDeclaringClass() ? "same" : "fresh", "|", json_encode($evalDecl->getDefaultProperties()), "|", json_encode($decl->getDefaultProperties()), "\n";
}
probe();
"#,
    );
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "ReflectionEnum:Suit:enum:3|backed|string|fresh\n",
            "ReflectionEnum:Plain:enum:2|unit\n",
            "ReflectionEnum:Suit:enum:3\n",
            "ReflectionEnum:Suit:enum:3\n",
            "ReflectionEnum:Suit:enum:3\n",
            "ReflectionEnum|ReflectionEnum|ReflectionClass|2:B\n",
            "ReflectionEnum:EvalSuit:enum:2|backed|int|fresh|[]|[]\n",
        )
    );
}
