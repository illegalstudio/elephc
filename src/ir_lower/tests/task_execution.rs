//! Purpose:
//! Verifies unified execution-mode enum lowering on every supported target.
//!
//! Called from:
//! - The `crate::ir_lower::tests` unit-test harness.
//!
//! Key details:
//! - Runtime-unknown input keeps both mode-selection paths present during lowering.
//! - These are EIR validation checks, not target executable or scheduler acceptance.

use super::*;
use crate::ir::Op;

/// Named fixed fields and static unpacks retain native worker lowering on every target.
#[test]
fn task_named_and_unpacked_submissions_lower_on_all_supported_targets() {
    let source = r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
function chooseExecution(bool $parallel): Execution {
    return $parallel ? Execution::Parallel : Execution::Async;
}
run(static function (TaskGroup $tasks): void {
    $task = $tasks->spawn(value: 41, task: static fn(int $value): int => $value + 1,
        execution: chooseExecution(true));
    echo $task->await();
    echo $tasks->spawn(...[Execution::Parallel, static fn(int $value): int => $value, 42])->await();
    $tasks->every(value: 42, seconds: 0.001,
        task: static function (PeriodicControl $control, int $value): void {
            echo $value; $control->stop();
        }, execution: chooseExecution(true));
});
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        assert!(module.functions.iter().chain(&module.closures).any(|function| {
            function.instructions.iter().any(|inst| inst.op == Op::ParallelSpawn)
        }), "{name}: named/unpacked worker branch remains reachable");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// A runtime enum keeps both task engines reachable and produces valid assembly on every target.
#[test]
fn task_runtime_execution_dispatches_on_all_supported_targets() {
    let source = r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
function chooseExecution(bool $parallel): Execution {
    return $parallel ? Execution::Parallel : Execution::Async;
}
run(static function (TaskGroup $tasks): void {
    $task = $tasks->spawn(chooseExecution(true), static fn(int $value): int => $value + 1, 41);
    echo $task->await();
});
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        assert!(module.functions.iter().chain(&module.closures).any(|function| {
            function.instructions.iter().any(|inst| inst.op == Op::ParallelSpawn)
        }), "{name}: runtime-selected worker branch remains reachable");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// A nullable owned receiver is published before a throwing call on every supported ABI.
#[test]
fn task_throwing_owned_receiver_is_unwind_rooted_on_all_targets() {
    use crate::ir::Immediate;
    use crate::types::PhpType;

    let source = r#"<?php
class Item { public function fail(): void { throw new RuntimeException("boom"); } }
class Owner {
    private array $items = [];
    public function create(): Item { $item = new Item(); $this->items[] = $item; return $item; }
    public function retire(Item $item): void { $removed = array_splice($this->items, 0, 1); unset($removed); }
}
function execute(Owner $owner): void {
    $item = null;
    try { $item = $owner->create(); $item->fail(); }
    finally { if ($item instanceof Item) { $owner->retire($item); } unset($item); }
}
$owner = new Owner();
try { execute($owner); } catch (Throwable $error) { echo $error->getMessage(); unset($error); }
unset($owner);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "execute").unwrap();
        let (call_index, call) = function.instructions.iter().enumerate().find(|(_, inst)| {
            inst.op == Op::MethodCall && inst.operands.len() == 1
                && function.value(inst.operands[0]).unwrap().php_type == PhpType::Object("Item".into())
        }).expect("nullable Item fail call remains reachable");
        let receiver = call.operands[0];
        let store = function.instructions[..call_index].iter().rev().find(|inst| {
            inst.op == Op::StoreLocal && inst.operands == [receiver]
        }).expect("receiver owner stored before invoking the method");
        let Some(Immediate::LocalSlot(slot)) = store.immediate else { panic!("receiver owner slot"); };
        assert!(function.instructions[..call_index].iter().any(|inst| {
            inst.op == Op::PushCallOperandOwner && inst.immediate == Some(Immediate::LocalSlot(slot))
        }), "{name}: owned receiver must be visible to exception cleanup");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Worker envelope string reads retire their detached copies after argument snapshots on all targets.
#[test]
fn task_failure_envelope_snapshot_retires_widened_string_load_on_all_targets() {
    use crate::ir::Immediate;
    use crate::types::PhpType;

    let source = r#"<?php
use Elephc\Task\{TaskGroup, PeriodicControl, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    $tasks->every(Execution::Parallel, 0.001, static function (PeriodicControl $control): void {
        throw new RuntimeException("worker failure");
    });
});
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let decoder = module.class_methods.iter().find(|function| {
            function.name == "Elephc\\Parallel\\Future::__takePreparedValue"
        }).expect("worker envelope decoder is present");
        let load = decoder.instructions.iter().find(|inst| {
            if inst.op != Op::LoadLocal { return false; }
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return false; };
            let Some(value) = inst.result.and_then(|value| decoder.value(value)) else { return false; };
            decoder.locals[slot.as_raw() as usize].php_type.codegen_repr() == PhpType::Mixed
                && value.php_type.codegen_repr() == PhpType::Str
        }).expect("later unset widens the decoder string slot");
        let projected = load.result.unwrap();
        assert_eq!(decoder.instructions.iter().filter(|inst| {
            inst.op == Op::Release && inst.operands == [projected]
        }).count(), 1, "{name}: detached string retires exactly once after copying");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Execution modes preserve their enum return type through dynamic selection on every target.
#[test]
fn task_execution_enum_lowers_on_all_supported_targets() {
    let source = r#"<?php
use Elephc\Task\Execution;
function chooseMode(bool $parallel): Execution {
    if ($parallel) { return Execution::Parallel; }
    return Execution::Async;
}
echo chooseMode($argc > 1)->name;
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        assert!(module.functions.iter().any(|function| function.name == "chooseMode"),
            "{name}: runtime mode-selection function must survive lowering");
    }
}

/// Constructor argument retirement must not consume Task's stored engine owners twice.
#[test]
fn task_parallel_constructor_arguments_retire_once_on_all_supported_targets() {
    let source = r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
run(static function (TaskGroup $tasks): void {
    $tasks->spawn(Execution::Parallel, static fn(): int => 42);
});
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let mut checked = 0;
        for function in module.functions.iter().chain(&module.closures) {
            for inst in &function.instructions {
                if inst.op != Op::ObjectNew || inst.operands.len() != 3 { continue; }
                let Some(value) = inst.result.and_then(|value| function.value(value)) else { continue; };
                if value.php_type != crate::types::PhpType::Object("Elephc\\Task\\Task".into()) { continue; }
                checked += 1;
                for argument in [inst.operands[0], inst.operands[2]] {
                    let releases = function.instructions.iter().filter(|candidate| {
                        candidate.op == Op::Release && candidate.operands == [argument]
                    }).count();
                    assert_eq!(releases, 1, "{name}: each constructor argument retires once");
                }
            }
        }
        assert_eq!(checked, 1, "{name}: unified worker Task constructor must be present");
    }
}

/// Opaque group dispatch lowers finitely with dummy prelude spans on every target.
#[test]
fn task_closed_group_dispatch_emits_on_all_supported_targets() {
    let source = r#"<?php
use Elephc\Task\{TaskGroup, Execution};
use function Elephc\Task\run;
$closed = run(static fn(TaskGroup $tasks): TaskGroup => $tasks);
try { $closed->spawn(Execution::Async, static fn(): int => 1); }
catch (Error $error) { echo $error->getMessage(); }
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let main = module.functions.iter().find(|function| function.name == "main").unwrap();
        // The variadic group method binds either through main's exact-class selection (a
        // runtime `get_class` choosing the typed collector arm) or the runtime descriptor.
        let selected = main.instructions.iter().any(|inst| inst.op == Op::RuntimeCall
            && inst.immediate == Some(crate::ir::Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::GetClass))));
        assert!(selected || main.instructions.iter().any(|inst| inst.op == Op::CallableDescriptorInvoke),
            "{name}: ambiguous group method binds against the selected runtime class or descriptor");
        // The descriptor path guards null explicitly; the selected path resolves the receiver's
        // class first, so a null receiver reaches the ordinary fallback and throws "Call to a
        // member function spawn() on null" before any argument is evaluated, as PHP does.
        assert!(selected || main.instructions.iter().any(|inst| inst.op == Op::IsNull),
            "{name}: null receiver guard precedes argument evaluation");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
