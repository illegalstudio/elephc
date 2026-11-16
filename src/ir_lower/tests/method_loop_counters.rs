//! Purpose:
//! Guards scalar counter recovery across resolved by-value instance calls and CFG joins.
//!
//! Called from:
//! - The AST-to-EIR library regression suite.
//!
//! Key details:
//! - Counter storage is checked before optional optimization on every supported target.
//! - An independently unbounded accumulator and overflowing arguments must remain Mixed.

use crate::ir::{Function, Immediate, IrHeapKind, IrType, Op};

/// Collects the physical load representations for one named PHP binding.
fn loads(function: &Function, name: &str) -> Vec<IrType> {
    let slot = function.locals.iter().find(|local| local.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("missing local {name}")).id;
    function.instructions.iter().filter(|inst| {
        inst.op == Op::LoadLocal && inst.immediate == Some(Immediate::LocalSlot(slot))
    }).map(|inst| inst.result_type).collect()
}

/// A method-observed inner counter and an invariant outer counter both survive conditional joins.
#[test]
fn method_loop_counters_narrow_after_conditional_joins_all_targets() {
    let source = include_str!("../../../examples/method-loop-counter/main.php");
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function| function.name == "countHits").unwrap();
        for name in ["round", "index"] {
            let types = loads(function, name);
            assert!(!types.is_empty(), "{target} {name}");
            assert!(types.iter().all(|ty| *ty == IrType::I64),
                "{target} {name}: {}", crate::ir::print_function(function));
        }
        assert!(loads(function, "hits").contains(&IrType::Heap(IrHeapKind::Mixed)),
            "a bounded loop counter does not bound the independently accumulated hit count");
    }
}

/// A method parameter cannot turn an unguarded increment at PHP_INT_MAX into a scalar counter.
#[test]
fn method_loop_counters_keep_overflowing_method_arguments_mixed() {
    let source = r#"<?php
final class Observer { public function read(int $value): int { return $value; } }
function observe(Observer $box, int $count): void {
    $value=PHP_INT_MAX;
    for($index=0;$index<$count;$index++){ $box->read($value); $value++; }
}
try { observe(new Observer(),$argc+1); } catch(TypeError $error) { echo 'caught'; }
"#;
    let module = super::lower_source(source);
    let function = module.functions.iter().find(|function| function.name == "observe").unwrap();
    assert!(loads(function, "value").contains(&IrType::Heap(IrHeapKind::Mixed)));
}

/// Unchanged scalar values flowing through nested joins need no instance-call metadata.
#[test]
fn method_loop_counters_recover_nested_branch_invariants() {
    let source = r#"<?php
function nestedCounters(int $rounds, int $size): int {
    $sum=0;
    for($round=0;$round<$rounds;$round++){
        for($index=0;$index<$size;$index++){
            if($index&1){ $sum=$sum^$index; }
        }
    }
    return $sum;
}
echo nestedCounters($argc+1,$argc+3);
"#;
    let module = super::lower_source(source);
    let function = module.functions.iter().find(|function| function.name == "nestedCounters").unwrap();
    for name in ["round", "index"] {
        assert!(loads(function, name).iter().all(|ty| *ty == IrType::I64),
            "{}", crate::ir::print_function(function));
    }
}
