//! Purpose:
//! Regression coverage for addressable by-reference foreach iterator cleanup.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Cleanup names an iterator-state local instead of carrying an SSA iterator across CFG exits.
//! - Return and repeated outer-loop entry retain the same operand-free cleanup contract.

use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{Function, Immediate, LocalKind, LocalSlotId, Op};

/// Every supported target shares the same iterator-state cleanup representation.
const TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// Lowers one program and returns its named function.
fn lower_function(source: &str, name: &str, target: &str) -> Function {
    super::lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        Target::parse(target).unwrap(),
    )
    .functions
    .into_iter()
    .find(|function| function.name == name)
    .unwrap_or_else(|| panic!("{target}: missing function {name}"))
}

/// Returns the state slot named by the function's only iterator start.
fn only_iterator_state(function: &Function) -> LocalSlotId {
    let states = function
        .instructions
        .iter()
        .filter_map(|inst| match inst.immediate.as_ref() {
            Some(Immediate::IterStart(metadata)) if inst.op == Op::IterStart => {
                Some(metadata.state())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(states.len(), 1);
    let state = states[0];
    assert!(
        function
            .locals
            .get(state.as_raw() as usize)
            .is_some_and(|local| local.id == state && local.kind == LocalKind::IteratorState)
    );
    state
}

/// Returns every iterator cleanup state and asserts that no cleanup carries an SSA operand.
fn iterator_end_states(function: &Function) -> Vec<LocalSlotId> {
    function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::IterEnd)
        .map(|inst| {
            assert!(inst.operands.is_empty(), "IterEnd must not carry an SSA iterator");
            match inst.immediate.as_ref() {
                Some(Immediate::LocalSlot(state)) => *state,
                ref other => panic!("IterEnd must name iterator state, got {other:?}"),
            }
        })
        .collect()
}

/// Return and fallthrough exits both clean the same addressable iterator state.
#[test]
fn by_ref_foreach_return_uses_dominance_safe_iterator_cleanup_on_every_target() {
    let source = r#"<?php
function first(array $a): int {
    foreach ($a as $k => &$v) {
        return $v;
    }
    return 0;
}
echo first(["a" => 1, "b" => 2, "c" => 3]), "\n";
"#;
    for target in TARGETS {
        let function = lower_function(source, "first", target);
        let state = only_iterator_state(&function);
        let ends = iterator_end_states(&function);
        assert_eq!(ends, vec![state, state], "{target}: return and fallthrough cleanup");
    }
}

/// Re-entering an outer loop reinitializes and cleans one stable foreach state each time.
#[test]
fn repeated_foreach_uses_one_addressable_cleanup_state_on_every_target() {
    let source = r#"<?php
function repeat(): void {
    $a = ["a" => 1, "b" => 2, "c" => 3];
    for ($i = 0; $i < 8; $i = $i + 1) {
        foreach ($a as $k => &$v) {
            break;
        }
        unset($v);
    }
}
repeat();
"#;
    for target in TARGETS {
        let function = lower_function(source, "repeat", target);
        let state = only_iterator_state(&function);
        assert_eq!(
            iterator_end_states(&function),
            vec![state],
            "{target}: normal foreach exit owns the cleanup",
        );
    }
}
