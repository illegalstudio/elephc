//! Purpose:
//! Checks whole checked-buffer accessor chains after generic specialization.
//!
//! Called from:
//! - The parent method-inlining EIR test module.
//!
//! Key details:
//! - Runtime loop bounds prevent constant folding; every supported target emits assembly.

use super::*;

/// Native-width specializations inline scalar accessors on every supported target.
#[test]
fn generic_native_storage_carriers_inline_on_every_target() {
    let source = r#"<?php
final class WidthCell<T> {
    private buffer<T> $data;
    public function __construct() { $this->data = buffer_new<T>(1); }
    public function get(): T { return $this->data[0]; }
    public function set(T $value): void { $this->data[0] = $value; }
}
function updateWidth(WidthCell<u32> $words, WidthCell<u8> $bytes, WidthCell<f32> $floats, int $n): void {
    for ($i = 0; $i < $n; $i++) {
        $words->set($words->get() + 1);
        $bytes->set($bytes->get() + 1);
        $floats->set($floats->get() + 0.5);
    }
}
updateWidth(new WidthCell<u32>(), new WidthCell<u8>(), new WidthCell<f32>(), $argc);
"#;
    for target in TARGETS {
        let module = optimized(source, target);
        let update = function(&module, "updateWidth");
        assert!(called_methods(&module, update).is_empty(),
            "{target}: width accessor calls survived\n{}", crate::ir::print_function(update));
        assert!(placed(update).any(|inst| matches!(inst.op, Op::BufferGet | Op::BufferGetResolved)));
        assert!(placed(update).any(|inst| matches!(inst.op, Op::BufferSet | Op::BufferSetResolved)));
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Scalar generic accessors and their private guard chain splice into the actual loop.
#[test]
fn checked_managed_buffer_accessors_inline_on_every_target() {
    let source = include_str!("../../../examples/checked-buffer/main.php");
    for target in TARGETS {
        let module = optimized(source, target);
        let update = function(&module, "updateChecked");
        let calls = called_methods(&module, update);
        assert!(calls.is_empty(), "{target}: checked accessor calls survived: {calls:?}\n{}",
            crate::ir::print_function(update));
        assert!(placed(update).any(|inst| matches!(inst.op, Op::BufferGet | Op::BufferGetResolved)));
        assert!(placed(update).any(|inst| matches!(inst.op, Op::BufferSet | Op::BufferSetResolved)));
        let loops = crate::ir_passes::compute_loops(update, &crate::ir_passes::compute_dominance(update));
        let inner = loops.loops().iter().max_by_key(|loop_ref| loop_ref.depth).unwrap();
        for &block in &inner.blocks {
            for &id in &update.blocks[block.as_raw() as usize].instructions {
                let inst = &update.instructions[id.as_raw() as usize];
                assert!(!matches!(inst.op, Op::MethodCall | Op::PropGet | Op::BufferResolve | Op::BufferGet | Op::BufferSet),
                    "{target}: hot-loop {:?} survived\n{}", inst.op, crate::ir::print_function(update));
            }
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Inlined packed getters keep fresh reads after a field write to the same record.
#[test]
fn checked_managed_buffer_packed_reads_survive_writes_on_every_target() {
    let source = r#"<?php
packed class Cell { public int $v; }
final class Grid {
    private buffer<Cell> $cells;
    public function __construct(int $n) { $this->cells = buffer_new<Cell>($n); }
    public function get(int $i): int { return $this->cells[$i]->v; }
    public function put(int $i, int $v): void { $this->cells[$i]->v = $v; }
}
function probeField(Grid $g, int $x): int {
    $before = $g->get(0);
    $g->put(0, $x);
    $after = $g->get(0);
    return $before * 100 + $after;
}
echo probeField(new Grid(1), $argc);
"#;
    for target in TARGETS {
        let module = optimized(source, target);
        let probe = function(&module, "probeField");
        assert!(called_methods(&module, probe).is_empty(), "{target}: accessors survived");
        let reads = placed(probe).filter(|inst| {
            inst.op == Op::PropGet && probe.is_packed_field_access(inst)
        }).count();
        assert_eq!(reads, 2, "{target}: must reload the mutated record\n{}",
            crate::ir::print_function(probe));
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
