//! Purpose:
//! Checks opaque Task capability reflection lowering on every supported target.
//!
//! Called from:
//! - The compiler's focused AST-to-EIR unit harness.
//!
//! Key details:
//! - Known reflected methods/properties use EIR denials; allocation guards resolve runtime IDs.
//! - This proves valid target emission, not Linux/iOS execution or complete Reflection parity.

use super::*;

/// Eval's generated collectors must not break fixed-argument scheduler guard ABI emission.
#[test]
fn task_reflection_eval_guard_adapters_emit_on_all_supported_targets() {
    let source = r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    $control = $tasks->every(Execution::Async, 0.1, static function (PeriodicControl $current): void {
        $current->stop();
    });
    eval('$factory = new ReflectionMethod("Elephc\\Task\\PeriodicControl", "__fromNativeId");
        try { $factory->invoke(null, 1); } catch (Error $error) { unset($error); }
        unset($factory);');
    $control->stop();
});
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Private factory/storage calls and constructorless allocation emit target-aware refusals.
#[test]
fn task_reflection_capability_guards_emit_on_all_supported_targets() {
    let source = r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    $control = $tasks->every(Execution::Async, 0.1, static function (PeriodicControl $current): void {
        $current->stop();
    });
    $factory = new ReflectionMethod(PeriodicControl::class, "__fromNativeId");
    try { $factory->invoke(null, 1); } catch (Error $error) { unset($error); }
    $property = new ReflectionProperty(PeriodicControl::class, "id");
    try { $property->getValue($control); } catch (Error $error) { unset($error); }
    try { $property->setValue($control, 0); } catch (Error $error) { unset($error); }
    $type = new ReflectionClass(PeriodicControl::class);
    try { $type->newInstanceWithoutConstructor(); } catch (Error $error) { unset($error); }
    $control->stop();
});
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
