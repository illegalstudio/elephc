//! Purpose:
//! Regression tests for whole-module EIR call and property effect refinement.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Tests inspect pre-optimization EIR so assertions cover lowering metadata directly.
//! - Dynamic instance assertions use checked class hierarchies and inherited implementations.

use super::*;
use crate::ir::{Effects, Immediate, Op};

/// Returns the named direct-call instruction from one lowered function.
fn named_call<'a>(
    function: &'a crate::ir::Function,
    module: &'a crate::ir::Module,
    name: &str,
) -> &'a crate::ir::Instruction {
    function
        .instructions
        .iter()
        .find(|instruction| {
            if instruction.op != Op::Call {
                return false;
            }
            let Some(Immediate::Data(data_id)) = instruction.immediate.as_ref() else {
                return false;
            };
            module
                .data
                .function_names
                .get(data_id.as_raw() as usize)
                .is_some_and(|candidate| candidate == name)
        })
        .unwrap_or_else(|| panic!("missing call to {name} in {}", function.name))
}

/// Returns the named property-read instruction from one lowered function.
fn named_property_read<'a>(
    function: &'a crate::ir::Function,
    module: &'a crate::ir::Module,
    property: &str,
) -> &'a crate::ir::Instruction {
    function
        .instructions
        .iter()
        .find(|instruction| {
            if !matches!(instruction.op, Op::PropGet | Op::NullsafePropGet) {
                return false;
            }
            let Some(Immediate::Data(data_id)) = instruction.immediate.as_ref() else {
                return false;
            };
            module
                .data
                .strings
                .get(data_id.as_raw() as usize)
                .is_some_and(|candidate| candidate == property)
        })
        .unwrap_or_else(|| panic!("missing property read {property} in {}", function.name))
}

/// Verifies direct user calls inherit fixed-point effects from their lowered bodies.
#[test]
fn direct_calls_receive_callee_effect_summaries() {
    let module = lower_source(
        r#"<?php
function pure_len(string $value): int {
    return strlen($value);
}
function noisy_len(string $value): int {
    echo $value;
    return strlen($value);
}
echo pure_len("a");
echo noisy_len("b");
"#,
    );
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("missing main EIR");

    let pure = named_call(main, &module, "pure_len");
    let noisy = named_call(main, &module, "noisy_len");

    assert_eq!(pure.effects, Effects::WRITES_GLOBAL);
    assert_eq!(
        noisy.effects,
        Effects::WRITES_GLOBAL | Effects::OUTPUT
    );
    assert!(!pure.effects.contains(Effects::READS_FS));
    assert!(!pure.effects.contains(Effects::MAY_THROW));
}

/// Verifies virtual method calls union effects from every concrete checked override.
#[test]
fn instance_calls_union_closed_world_override_effects() {
    let module = lower_source(
        r#"<?php
class Base {
    public function value(): int { return 1; }
    public function relay(): int { return $this->value(); }
}
final class Child extends Base {
    public function value(): int { echo "child"; return 2; }
}
function run(Base $value): int { return $value->relay(); }
function exact(): int { return (new Base())->value(); }
echo run(new Base());
"#,
    );
    let relay = module
        .class_methods
        .iter()
        .find(|function| function.name == "Base::relay")
        .expect("missing Base::relay EIR");
    let method_call = relay
        .instructions
        .iter()
        .find(|instruction| instruction.op == Op::MethodCall)
        .expect("missing virtual value() call");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("missing main EIR");
    let run = named_call(main, &module, "run");
    let exact = module
        .functions
        .iter()
        .find(|function| function.name == "exact")
        .expect("missing exact EIR");
    let exact_call = exact
        .instructions
        .iter()
        .find(|instruction| instruction.op == Op::MethodCall)
        .expect("missing exact value() call");

    assert!(method_call.effects.contains(Effects::READS_HEAP));
    assert!(method_call.effects.contains(Effects::OUTPUT));
    assert!(run.effects.contains(Effects::READS_HEAP));
    assert!(run.effects.contains(Effects::OUTPUT));
    assert!(!run.effects.contains(Effects::READS_FS));
    assert!(!run.effects.contains(Effects::WRITES_PROCESS));
    assert!(exact_call.effects.contains(Effects::READS_HEAP));
    assert!(!exact_call.effects.contains(Effects::OUTPUT));
}

/// Verifies a runtime eval bridge blocks virtual hierarchy refinement but not exact construction.
#[test]
fn eval_bridge_keeps_virtual_dispatch_conservative() {
    let module = lower_source(
        r#"<?php
class EvalBase {
    public function value(): int { return 1; }
    public function relay(): int { return $this->value(); }
}
function exact(): int { return (new EvalBase())->value(); }
function install(string $source): void { eval($source); }
"#,
    );
    let relay = module
        .class_methods
        .iter()
        .find(|function| function.name == "EvalBase::relay")
        .expect("missing EvalBase::relay EIR");
    let virtual_call = relay
        .instructions
        .iter()
        .find(|instruction| instruction.op == Op::MethodCall)
        .expect("missing virtual value() call");
    let exact = module
        .functions
        .iter()
        .find(|function| function.name == "exact")
        .expect("missing exact EIR");
    let exact_call = exact
        .instructions
        .iter()
        .find(|instruction| instruction.op == Op::MethodCall)
        .expect("missing exact value() call");

    assert!(virtual_call.effects.contains(Effects::MAY_DEOPT));
    assert!(virtual_call.effects.contains(Effects::MAY_THROW));
    assert!(exact_call.effects.contains(Effects::READS_HEAP));
    assert!(exact_call.effects.contains(Effects::WRITES_GLOBAL));
    assert!(!exact_call.effects.contains(Effects::MAY_THROW));
    assert!(!exact_call.effects.contains(Effects::MAY_DEOPT));
}

/// Verifies declared slots and magic getters receive distinct property-read effects.
#[test]
fn property_reads_refine_typed_slots_and_magic_getters() {
    let module = lower_source(
        r#"<?php
final class Box {
    public $safe;
    public int $risky;
    public function safeRead() { return $this->safe; }
    public function riskyRead() { return $this->risky; }
}
final class MagicBox {
    public function __get(string $name) { echo $name; return 1; }
    public function read() { return $this->missing; }
}
final class HookBox {
    public int $computed { get { echo "hook"; return 1; } }
    public function read() { return $this->computed; }
}
"#,
    );
    let safe = module
        .class_methods
        .iter()
        .find(|function| function.name == "Box::safeRead")
        .expect("missing Box::safeRead EIR");
    let risky = module
        .class_methods
        .iter()
        .find(|function| function.name == "Box::riskyRead")
        .expect("missing Box::riskyRead EIR");
    let magic = module
        .class_methods
        .iter()
        .find(|function| function.name == "MagicBox::read")
        .expect("missing MagicBox::read EIR");
    let hook = module
        .class_methods
        .iter()
        .find(|function| function.name == "HookBox::read")
        .expect("missing HookBox::read EIR");

    assert_eq!(
        named_property_read(safe, &module, "safe").effects,
        Effects::READS_HEAP
    );
    assert_eq!(
        named_property_read(risky, &module, "risky").effects,
        Effects::READS_HEAP | Effects::MAY_THROW
    );
    let magic_effects = named_property_read(magic, &module, "missing").effects;
    assert!(magic_effects.contains(Effects::READS_HEAP));
    assert!(magic_effects.contains(Effects::OUTPUT));
    assert!(!magic_effects.contains(Effects::MAY_DEOPT));
    let hook_effects = hook
        .instructions
        .iter()
        .find(|instruction| instruction.op == Op::MethodCall)
        .expect("missing HookBox property accessor call")
        .effects;
    assert!(hook_effects.contains(Effects::OUTPUT));
    assert!(!hook_effects.contains(Effects::MAY_DEOPT));
}

/// Verifies inaccessible private calls retain their catchable error effect.
#[test]
fn inaccessible_instance_calls_remain_throwing() {
    let module = lower_source(
        r#"<?php
final class LockedBox {
    private function secret(): int { return 1; }
}
function reveal(LockedBox $box): int { return $box->secret(); }
function relay(LockedBox $box): int { return reveal($box); }
"#,
    );
    let reveal = module
        .functions
        .iter()
        .find(|function| function.name == "reveal")
        .expect("missing reveal EIR");
    let relay = module
        .functions
        .iter()
        .find(|function| function.name == "relay")
        .expect("missing relay EIR");
    let reveal_call = named_call(relay, &module, "reveal");

    assert!(
        reveal
            .instructions
            .iter()
            .any(|instruction| instruction.op == Op::ThrowException)
    );
    assert!(reveal_call.effects.contains(Effects::MAY_THROW));
}
