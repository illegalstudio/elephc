//! Purpose:
//! Coverage for the eval demand that sizes the eval-only builtin surfaces: literal fragments
//! lower exactly the Reflection and DateTime bodies they name, runtime-sourced evals lower all.
//!
//! Called from:
//! - `crate::ir_lower::tests` through Rust's test harness.
//!
//! Key details:
//! - Every fixture here needs the magician bridge (a class declaration or a `throw` inside the
//!   fragment keeps the EIR AOT planner out), so the demand, not the planner, decides the surface.

use std::collections::HashSet;

use super::lower_source;
use crate::ir_lower::program::{eval_demand, EvalDemand};

/// Collects the lowered class method names of a module.
fn lowered_methods(module: &crate::ir::Module) -> HashSet<String> {
    module
        .class_methods
        .iter()
        .map(|function| function.name.clone())
        .collect()
}

/// A literal fragment that names nothing date- or reflection-shaped lowers neither surface.
#[test]
fn literal_eval_without_date_or_reflection_names_lowers_neither_surface() {
    let module = lower_source(
        "<?php eval('class EvalDemandPlain { public int $n = 1; } echo (new EvalDemandPlain())->n;');",
    );
    assert!(module.required_runtime_features.eval_bridge);
    let EvalDemand::Names(names) = eval_demand(&module) else {
        panic!("a literal fragment must produce a named demand");
    };
    assert!(names.classes.contains("evaldemandplain"));
    let lowered = lowered_methods(&module);
    assert!(!lowered.contains("DateTime::createFromFormat"), "{lowered:?}");
    assert!(!lowered.contains("ReflectionClass::getName"), "{lowered:?}");
}

/// Naming a date alias function inside the fragment lowers the DateTime alias surface.
#[test]
fn literal_eval_naming_a_date_function_lowers_the_date_surface() {
    let module = lower_source(
        "<?php eval('class EvalDemandDates {} echo date_create(\"2024-01-01\")->format(\"Y\");');",
    );
    let lowered = lowered_methods(&module);
    assert!(lowered.contains("DateTime::createFromFormat"), "{lowered:?}");
    assert!(!lowered.contains("ReflectionClass::getName"), "{lowered:?}");
}

/// Reflection over a named class is a dynamic lookup inline (the reachability scanner widens
/// every hazard for it), and a fragment is read exactly like inline code, so a fragment that
/// reflects demands everything and lowers the Reflection surface with the rest.
#[test]
fn literal_eval_reflecting_a_class_demands_everything_like_inline_reflection() {
    let module = lower_source(
        "<?php class EvalDemandSubject { public function run(): int { return 1; } } eval('class EvalDemandProbe {} echo (new ReflectionClass(\"EvalDemandSubject\"))->getName();');",
    );
    assert_eq!(eval_demand(&module), EvalDemand::Everything);
    let lowered = lowered_methods(&module);
    assert!(lowered.contains("ReflectionClass::getName"), "{lowered:?}");
    assert!(lowered.contains("ReflectionMethod::getName"), "{lowered:?}");
}

/// A runtime-sourced eval, or a literal with a dynamic hazard inside, demands everything.
#[test]
fn opaque_evals_demand_everything() {
    for source in [
        "<?php eval($argv[1]);",
        "<?php eval('class EvalDemandDyn {} $f = $argv[1]; $f();');",
        "<?php $d = new DateTime(\"2024-01-01\"); eval('class EvalDemandNullsafe {} $m = $argv[1]; $d?->$m(2025, 1, 1);');",
        "<?php eval('include $argv[1];');",
    ] {
        let module = lower_source(source);
        assert_eq!(eval_demand(&module), EvalDemand::Everything, "{source}");
        let lowered = lowered_methods(&module);
        assert!(lowered.contains("DateTime::createFromFormat"), "{source}");
        assert!(lowered.contains("ReflectionClass::getName"), "{source}");
    }
}

/// A fragment calling a date method on an object the outer program built lowers that method
/// through the alias surface, while the same call in a program without any date object does not.
#[test]
fn literal_eval_calling_a_date_method_on_an_outer_object_lowers_the_date_surface() {
    let module = lower_source(
        "<?php $d = new DateTime(\"2024-01-01\"); eval('class EvalDemandOuterDate {} $d->setDate(2025, 1, 1); echo $d->format(\"Y-m-d\");');",
    );
    let EvalDemand::Names(names) = eval_demand(&module) else {
        panic!("a literal fragment must produce a named demand");
    };
    assert!(names.wildcard_methods.contains("setdate"));
    let lowered = lowered_methods(&module);
    assert!(lowered.contains("DateTime::setDate"), "{lowered:?}");
    let module = lower_source(
        "<?php eval('class EvalDemandNoDate { public function setDate(int $y): int { return $y; } } echo (new EvalDemandNoDate())->setDate(1);');",
    );
    let lowered = lowered_methods(&module);
    assert!(!lowered.contains("DateTime::setDate"), "{lowered:?}");
}

/// A program without eval has no demand and lowers no eval-only surface.
#[test]
fn programs_without_eval_have_no_demand() {
    let module = lower_source("<?php echo strlen(\"abc\");");
    assert_eq!(eval_demand(&module), EvalDemand::None);
    assert!(!module.required_runtime_features.eval_bridge);
}
