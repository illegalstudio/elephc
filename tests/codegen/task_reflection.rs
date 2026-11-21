//! Purpose:
//! Native and eval regressions for Reflection on scope-owned Task capabilities.
//!
//! Called from:
//! - `cargo test -p elephc --test codegen_tests task_reflection`.
//!
//! Key details:
//! - Denials are catchable, preserve argument effects and keep the original registration usable.
//! - Private Reflection access on ordinary PHP classes must remain unchanged.
//! - Every fixture uses heap debugging; a clean heap alone is not authority isolation.

use crate::support::*;

fn assert_clean(out: ProgramOutput, expected: &str) {
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Private factory calls cannot turn guessed native IDs into controlling PHP proxies.
#[test]
fn task_reflection_private_factory_denies_invoke_and_invoke_args() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
function observedId(): int { echo "I"; return 1; }
run(static function (TaskGroup $tasks): void {
    $control = $tasks->every(Execution::Async, 0.1, static function (PeriodicControl $current): void {
        echo "unexpected"; $current->stop();
    });
    $factory = new ReflectionMethod(PeriodicControl::class, "__fromNativeId");
    try { $factory->invoke(null, observedId()); }
    catch (Error $error) { echo "A"; unset($error); }
    try { $factory->invokeArgs(null, [observedId()]); }
    catch (Error $error) {
        if (!str_contains($error->getMessage(), "Runtime-shaped ReflectionMethod")) { throw $error; }
        echo "U"; unset($error);
    }
    $factory = new ReflectionMethod(PeriodicControl::class, "__fromNativeId");
    try { $factory->invokeArgs(null, [observedId()]); }
    catch (Error $error) { echo "B"; unset($error); }
    $control->stop(); echo "done";
});
"#);
    assert_clean(out, "IAIUIBdone");
}

/// Reading/writing native authority and allocating forged controls all fail before touching state.
#[test]
fn task_reflection_denies_storage_and_allocation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
function replacementId(): int { echo "I"; return 0; }
run(static function (TaskGroup $tasks): void {
    $control = $tasks->every(Execution::Async, 0.1, static function (PeriodicControl $current): void {
        echo "unexpected"; $current->stop();
    });
    $property = new ReflectionProperty(PeriodicControl::class, "id");
    try { $property->getValue($control); }
    catch (Error $error) { echo "G"; unset($error); }
    try { $property->setValue($control, replacementId()); }
    catch (Error $error) { echo "S"; unset($error); }
    $type = new ReflectionClass(PeriodicControl::class);
    try { $type->newInstanceWithoutConstructor(); }
    catch (Error $error) { echo "N"; unset($error); }
    try { $type->newInstance(0.001); }
    catch (Error $error) { echo "A"; unset($error); }
    try { $type->newInstanceArgs([0.001]); }
    catch (Error $error) { echo "B"; unset($error); }
    $control->stop();
});
"#);
    assert_clean(out, "GISNAB");
}

/// Allocation checks the actual runtime selector, including case variants of the class name.
#[test]
fn task_reflection_dynamic_class_selectors_cannot_bypass_allocation_policy() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
function controlClassName(): string { return "Elephc\\Task\\PeriodicControl"; }
run(static function (TaskGroup $tasks): void {
    $control = $tasks->every(Execution::Async, 0.1, static function (PeriodicControl $current): void {
        echo "unexpected"; $current->stop();
    });
    $type = new ReflectionClass(PeriodicControl::class);
    $name = new ReflectionProperty(ReflectionClass::class, "__name");
    $name->setValue($type, controlClassName());
    try { $type->newInstanceWithoutConstructor(); }
    catch (Error $error) {
        if (!str_contains($error->getMessage(), "scope-owned capability")) { throw $error; }
        echo "D"; unset($error);
    }
    $alias = new ReflectionClass(PeriodicControl::class);
    $name = new ReflectionProperty(ReflectionClass::class, "__name");
    $name->setValue($alias, "eLePhC\\tAsK\\pErIoDiCcOnTrOl");
    try { $alias->newInstanceWithoutConstructor(); }
    catch (Error $error) {
        if (!str_contains($error->getMessage(), "scope-owned capability")) { throw $error; }
        echo "A"; unset($error);
    }
    $control->stop();
});
"#);
    assert_clean(out, "DA");
}

/// Opaque storage does not prevent metadata inspection or public stop/setInterval calls.
#[test]
fn task_reflection_public_control_methods_remain_usable() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    $control = $tasks->every(Execution::Async, 0.1, static function (PeriodicControl $current): void {
        echo "unexpected"; $current->stop();
    });
    $change = new ReflectionMethod(PeriodicControl::class, "setInterval");
    $change->invoke($control, 0.2);
    $stop = new ReflectionMethod(PeriodicControl::class, "stop");
    $stop->invoke($control);
    echo $stop->getName();
});
"#);
    assert_clean(out, "stop");
}

/// PHP private reflected methods/storage and constructorless allocation retain their ordinary rules.
#[test]
fn task_reflection_policy_preserves_ordinary_php_private_access() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Ordinary {
    private int $value = 1;
    private function answer(): int { return $this->value; }
    private static function fixed(): int { return 42; }
}
$object = new Ordinary();
$property = new ReflectionProperty(Ordinary::class, "value");
$property->setValue($object, 41);
$answer = new ReflectionMethod(Ordinary::class, "answer");
echo $answer->invoke($object), "|";
$fixed = new ReflectionMethod(Ordinary::class, "fixed");
echo $fixed->invoke(null), "|";
$type = new ReflectionClass(Ordinary::class);
$copy = $type->newInstanceWithoutConstructor();
echo $property->getValue($copy);
unset($copy, $type, $fixed, $answer, $property, $object);
"#);
    assert_clean(out, "41|42|1");
}

/// Eval must enforce the same policy when reflected targets are generated/AOT methods and objects.
#[test]
fn task_reflection_eval_denies_factory_storage_and_allocation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    $control = $tasks->every(Execution::Async, 0.1, static function (PeriodicControl $current): void {
        echo "unexpected"; $current->stop();
    });
    eval('
        $factory = new ReflectionMethod("Elephc\\Task\\PeriodicControl", "__fromNativeId");
        try { $factory->invoke(null, 1); } catch (Error $error) { echo "F"; unset($error); }
        $property = new ReflectionProperty("Elephc\\Task\\PeriodicControl", "id");
        try { $property->getValue($control); } catch (Error $error) { echo "G"; unset($error); }
        try { $property->setValue($control, 0); } catch (Error $error) { echo "S"; unset($error); }
        $type = new ReflectionClass("Elephc\\Task\\PeriodicControl");
        try { $type->newInstanceWithoutConstructor(); } catch (Error $error) { echo "N"; unset($error); }
        unset($type, $property, $factory);
    ');
    $control->stop();
});
"#);
    assert_clean(out, "FGSN");
}
