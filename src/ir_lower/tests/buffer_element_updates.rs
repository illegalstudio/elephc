//! Purpose:
//! Pins the typed lowering of scalar `buffer<T>` element `++`/`--` on every supported target.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Statement and expression updates of local and property buffers lower to a typed element
//!   read, a checked integer or float step and a typed store, with no `StrIncDec`, `MixedClone`
//!   or `MixedBox` staging the value.
//! - The checks run on the lowered EIR and again after the complete EIR optimizer, and every
//!   target must still validate and emit assembly.
//! - A PHP array element keeps the generic path, which promotes an overflow to float.

use crate::codegen::platform::Target;
use crate::ir::{Function, Immediate, Module, Op, ValueDef};
use std::path::Path;

/// Local, property, width and expression forms of buffer element `++`/`--`.
const SOURCE: &str = r#"<?php
class CellGrid {
    public buffer<int> $cellStart;
    public buffer<f32> $weights;
    public function __construct(int $cells) {
        $this->cellStart = buffer_new<int>($cells);
        $this->weights = buffer_new<f32>($cells);
    }
    public function enter(int $cell): void { $this->cellStart[$cell]++; }
    public function leave(int $cell): void { --$this->cellStart[$cell]; }
    public function weigh(int $cell): void { $this->weights[$cell]--; }
    public function claim(int $cell): int { return $this->cellStart[$cell]--; }
}
function countLocal(buffer<int> $counts, int $cell): void { $counts[$cell]++; --$counts[$cell ^ 1]; }
function stepWidths(buffer<u8> $bytes, buffer<i16> $words, buffer<float> $values, buffer<f32> $singles, int $i): void {
    $bytes[$i]++;
    --$words[$i];
    $values[$i]--;
    ++$singles[$i];
}
function postfixValues(buffer<int> $counts, buffer<float> $values, int $i): float {
    return $counts[$i]++ + $values[$i]--;
}
function prefixFloat(buffer<f32> $singles, int $i): float { return ++$singles[$i]; }
function prefixInt(buffer<int> $counts, int $i): mixed { return ++$counts[$i]; }
$grid = new CellGrid(8);
$grid->enter($argc);
$grid->leave($argc);
$grid->weigh($argc);
echo $grid->claim($argc);
$counts = buffer_new<int>(8);
$bytes = buffer_new<u8>(4);
$words = buffer_new<i16>(4);
$values = buffer_new<float>(4);
$singles = buffer_new<f32>(4);
countLocal($counts, $argc);
stepWidths($bytes, $words, $values, $singles, $argc);
echo postfixValues($counts, $values, $argc), prefixFloat($singles, $argc), prefixInt($counts, $argc);
"#;

/// Every function and method of the fixture whose buffer updates must be typed.
const UPDATED: [&str; 9] = [
    "CellGrid::enter", "CellGrid::leave", "CellGrid::weigh", "CellGrid::claim", "countLocal",
    "stepWidths", "postfixValues", "prefixFloat", "prefixInt",
];

/// Finds a top-level function or class method by its EIR name.
fn named<'m>(module: &'m Module, name: &str) -> &'m Function {
    module.functions.iter().chain(&module.class_methods)
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing function {name}"))
}

/// Counts the instructions of one opcode, optionally only those with the given immediate.
fn count(function: &Function, op: Op, immediate: Option<Immediate>) -> usize {
    function.instructions.iter().filter(|inst| {
        inst.op == op && (immediate.is_none() || inst.immediate == immediate)
    }).count()
}

/// Asserts that a function stores its updated elements without staging them in Mixed cells.
fn assert_unstaged(function: &Function, context: &str) {
    for op in [Op::StrIncDec, Op::MixedClone, Op::MixedBox] {
        assert_eq!(count(function, op, None), 0, "{context} {}: {op:?}\n{}",
            function.name, crate::ir::print_function(function));
    }
    assert!(function.instructions.iter().any(|inst| {
        matches!(inst.op, Op::BufferSet | Op::BufferSetResolved)
    }), "{context} {}: typed element store", function.name);
}

/// Returns the arena position of the local load the function's typed store derives its handle from.
///
/// The handle of a local store is that load; a property store's handle is the property read of
/// the write receiver, whose own receiver is the load.
fn store_receiver_load(function: &Function) -> usize {
    let store = function.instructions.iter().find(|inst| inst.op == Op::BufferSet)
        .unwrap_or_else(|| panic!("{} has no buffer_set", function.name));
    let mut value = store.operands[0];
    loop {
        let ValueDef::Instruction { inst, .. } = function.value(value).expect("store operand").def
        else {
            panic!("{}: the store handle is not an instruction result", function.name);
        };
        let definition = &function.instructions[inst.as_raw() as usize];
        if definition.op == Op::LoadLocal {
            return inst.as_raw() as usize;
        }
        value = definition.operands[0];
    }
}

/// Lowered updates use one typed step per update and box only a prefix int expression's value.
#[test]
fn buffer_element_incdec_lowers_typed_steps_on_every_target() {
    let cast = Some(Immediate::Bool(true));
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = super::lower_source_at_for_target(
            SOURCE, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        for name in UPDATED {
            assert_unstaged(named(&module, name), target);
        }
        let steps = |name: &str| {
            let function = named(&module, name);
            (
                count(function, Op::ICheckedAddToInt, cast.clone()),
                count(function, Op::ICheckedSubToInt, cast.clone()),
                count(function, Op::ICheckedAdd, None) + count(function, Op::ICheckedSub, None),
                count(function, Op::FAdd, None),
                count(function, Op::FSub, None),
            )
        };
        assert_eq!(steps("CellGrid::enter"), (1, 0, 0, 0, 0), "{target}");
        assert_eq!(steps("CellGrid::leave"), (0, 1, 0, 0, 0), "{target}");
        assert_eq!(steps("CellGrid::weigh"), (0, 0, 0, 0, 1), "{target}");
        assert_eq!(steps("CellGrid::claim"), (0, 1, 0, 0, 0), "{target}");
        assert_eq!(steps("countLocal"), (1, 1, 0, 0, 0), "{target}");
        assert_eq!(steps("stepWidths"), (1, 1, 0, 1, 1), "{target}");
        assert_eq!(steps("prefixFloat"), (0, 0, 0, 1, 0), "{target}");
        // PHP's prefix value is the sum itself, a float past PHP_INT_MAX, so only that value
        // is a checked `mixed`; the store beside it is typed.
        assert_eq!(steps("prefixInt"), (1, 0, 1, 0, 0), "{target}");
        let postfix = named(&module, "postfixValues");
        assert_eq!(count(postfix, Op::ICheckedAddToInt, cast.clone()), 1, "{target}");
        assert_eq!(count(postfix, Op::FSub, None), 1, "{target}");
        assert_eq!(count(postfix, Op::ICheckedAdd, None), 0, "{target}");
        // An expression reads the element where its prelude did, before the write loads its
        // handle or receiver, so user code the read runs is seen by the write as before.
        for name in ["CellGrid::claim", "prefixFloat", "prefixInt"] {
            let function = named(&module, name);
            let read = function.instructions.iter().position(|inst| inst.op == Op::BufferGet)
                .unwrap_or_else(|| panic!("{target} {name}: element read"));
            assert!(store_receiver_load(function) > read,
                "{target} {name}: the write's receiver is loaded after the read\n{}",
                crate::ir::print_function(function));
        }

        crate::ir_passes::optimize_module(&mut module);
        crate::ir::validate_module(&module).unwrap();
        for name in UPDATED {
            assert_unstaged(named(&module, name), &format!("{target} optimized"));
        }
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
        assert!(assembly.contains("__rt_php_float_to_int"),
            "{target}: an overflowing int step converts the promoted float back to int");
    }
}

/// A PHP array element keeps the generic update, whose overflow stores a float in the array.
#[test]
fn array_element_incdec_keeps_the_generic_update() {
    let source = r#"<?php
function countArray(array $counts, int $cell): array { $counts[$cell]++; return $counts; }
echo count(countArray([1, 2, 3], $argc));
"#;
    let module = super::lower_source(source);
    let function = named(&module, "countArray");
    assert_eq!(count(function, Op::ICheckedAddToInt, None), 0,
        "{}", crate::ir::print_function(function));
    assert_eq!(count(function, Op::ICheckedSubToInt, None), 0);
}
