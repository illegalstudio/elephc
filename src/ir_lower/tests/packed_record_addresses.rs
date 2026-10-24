//! Purpose:
//! Regression coverage for sink-only EIR retyping of packed record addresses.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Packed address producers retain `Packed(T)` metadata for field-access optimization.
//! - Pointer sinks receive a value-preserving `Move` retyped to pointer-shaped `I64` storage.

use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{IrHeapKind, IrType, Op};
use crate::types::PhpType;

/// Verifies packed producers remain record-shaped while all four pointer sinks receive pointers.
#[test]
fn packed_record_addresses_retype_only_at_pointer_sinks() {
    let module = super::lower_source(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
packed class Body { public Vec2 $pos; }
packed class Holder { public ptr<Vec2> $value; }
extern function consume(ptr<Vec2> $value): void;
function first(buffer<Vec2> $values): ptr<Vec2> { return $values[0]; }
$values = buffer_new<Vec2>(1);
$holders = buffer_new<Holder>(1);
$holders[0]->value = $values[0];
consume($values[0]);
$bodies = buffer_new<Body>(1);
echo ptr_is_null($bodies[0]->pos), ptr_is_null(first(buffer_new<Vec2>(1)));
"#,
    );
    let first = module
        .functions
        .iter()
        .find(|function| function.name == "first")
        .expect("first function");
    let buffer_address = first
        .instructions
        .iter()
        .find(|instruction| {
            instruction.op == Op::BufferGet
                && instruction.result_php_type == PhpType::Packed("Vec2".to_string())
        })
        .expect("packed buffer address");
    assert_eq!(buffer_address.result_type, IrType::Heap(IrHeapKind::Object));
    let buffer_value = buffer_address.result.expect("buffer address result");
    let return_retype = first.instructions.iter().find(|instruction| {
        instruction.op == Op::Move
            && instruction.operands.first() == Some(&buffer_value)
            && instruction.result_php_type == PhpType::Pointer(Some("Vec2".to_string()))
    });
    assert_eq!(
        return_retype.expect("pointer return retype").result_type,
        IrType::I64,
    );

    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    let nested_address = main
        .instructions
        .iter()
        .find(|instruction| {
            instruction.op == Op::PropGet
                && instruction.result_php_type == PhpType::Packed("Vec2".to_string())
        })
        .expect("nested packed address");
    assert_eq!(nested_address.result_type, IrType::Heap(IrHeapKind::Object));
    let nested_value = nested_address.result.expect("nested address result");
    let builtin_retype = main.instructions.iter().find(|instruction| {
        instruction.op == Op::Move
            && instruction.operands.first() == Some(&nested_value)
            && instruction.result_php_type == PhpType::Pointer(None)
    });
    assert_eq!(
        builtin_retype.expect("pointer builtin retype").result_type,
        IrType::I64,
    );
    let property_operand = main
        .instructions
        .iter()
        .find(|instruction| instruction.op == Op::PropSet)
        .and_then(|instruction| instruction.operands.get(1))
        .copied()
        .expect("packed pointer field operand");
    let property_retype = main
        .instructions
        .iter()
        .find(|instruction| instruction.result == Some(property_operand))
        .expect("packed pointer field retype");
    assert_eq!(property_retype.op, Op::Move);
    assert_eq!(
        property_retype.result_php_type,
        PhpType::Pointer(Some("Vec2".to_string())),
    );
    assert_eq!(property_retype.result_type, IrType::I64);

    let extern_operand = main
        .instructions
        .iter()
        .find(|instruction| instruction.op == Op::ExternCall)
        .and_then(|instruction| instruction.operands.first())
        .copied()
        .expect("extern pointer operand");
    let extern_retype = main
        .instructions
        .iter()
        .find(|instruction| instruction.result == Some(extern_operand))
        .expect("extern pointer retype");
    assert_eq!(extern_retype.op, Op::Move);
    assert_eq!(
        extern_retype.result_php_type,
        PhpType::Pointer(Some("Vec2".to_string())),
    );
    assert_eq!(extern_retype.result_type, IrType::I64);
}

/// Verifies optimized packed buffer loops keep packed field classification and no pointer guard.
#[test]
fn packed_buffer_loop_emits_no_pointer_nonnull_call_on_linux_targets() {
    let source = r#"<?php
packed class Body { public float $x; public float $vx; }
function step(buffer<Body> $b, int $n, float $dt): void {
    for ($i = 0; $i < $n; $i++) {
        $b[$i]->x = $b[$i]->x + $b[$i]->vx * $dt;
    }
}
"#;
    for name in ["linux-x86_64", "linux-aarch64"] {
        let mut module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).expect("supported Linux target"),
        );
        crate::ir_passes::optimize_module(&mut module);
        let step = module
            .functions
            .iter()
            .find(|function| function.name == "step")
            .expect("step function");
        assert!(
            step.instructions.iter().any(|instruction| {
                instruction.op == Op::BufferGetResolved
                    && instruction.result_php_type == PhpType::Packed("Body".to_string())
            }),
            "{name}: packed buffer accesses should remain optimized",
        );
        assert!(
            step.instructions
                .iter()
                .any(|instruction| step.is_packed_field_access(instruction)),
            "{name}: packed field accesses should retain packed receivers",
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(
            !assembly.contains("call __rt_ptr_check_nonnull")
                && !assembly.contains("bl __rt_ptr_check_nonnull"),
            "{name}: packed receivers must not emit pointer nonnull calls",
        );
    }
}

/// Verifies a local that holds `null` and packed record addresses gets one pointer-word frame
/// slot, in sequence and around a loop that starts from `null` (task 1af74866: both used to widen
/// to boxed `mixed`), while a record-only local keeps its record storage.
#[test]
fn null_and_record_locals_get_pointer_word_slots() {
    let module = super::lower_source(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function seq(buffer<Vec2> $v): int { $x = null; $x = $v[0]; $y = $v[1]; return $x->x + $y->x; }
function search(buffer<Vec2> $v): int {
    $best = null;
    for ($i = 0; $i < 2; $i++) {
        if ($best === null || $v[$i]->x > $best->x) { $best = $v[$i]; }
    }
    return $best === null ? 0 : $best->x;
}
$v = buffer_new<Vec2>(2);
echo seq($v), search($v);
"#,
    );
    let slot = |function: &str, name: &str| {
        module
            .functions
            .iter()
            .find(|candidate| candidate.name == function)
            .unwrap_or_else(|| panic!("{function} function"))
            .locals
            .iter()
            .find(|local| local.name.as_deref() == Some(name))
            .map(|local| (local.php_type.clone(), local.ir_type))
            .unwrap_or_else(|| panic!("{function} local {name}"))
    };
    let pointer = PhpType::Pointer(Some("Vec2".to_string()));
    assert_eq!(slot("seq", "x"), (pointer.clone(), IrType::I64));
    assert_eq!(
        slot("seq", "y"),
        (PhpType::Packed("Vec2".to_string()), IrType::Heap(IrHeapKind::Object)),
    );
    assert_eq!(slot("search", "best"), (pointer, IrType::I64));
}
