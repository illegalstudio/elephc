//! Purpose:
//! Tests for the must-defined local availability analysis used by optimized
//! debug builds to report `<optimized out>` locals.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are hand-built with `crate::ir::Builder`, so each case isolates
//!   straight-line stores, branch joins, parameters and a promoted slot.

use crate::ir::{
    Function, FunctionParam, Immediate, IrType, LocalKind, LocalSlotId, Op, Ownership, Terminator,
};
use crate::ir::Builder;
use crate::ir_passes::compute_debug_availability;
use crate::ir_passes::mem2reg::Mem2Reg;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Declares one named integer PHP local and returns its slot.
fn add_int_local(builder: &mut Builder<'_>, name: &str) -> LocalSlotId {
    builder.add_local(
        Some(name.to_string()),
        IrType::I64,
        PhpType::Int,
        LocalKind::PhpLocal,
    )
}

/// Emits an `UnsetLocal` for one slot.
fn emit_unset(builder: &mut Builder<'_>, slot: LocalSlotId) {
    builder.emit(
        Op::UnsetLocal,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
}

/// A local is dead before its first store, live after it, and dead again after
/// `unset()`. Parameters enter the function available.
#[test]
fn stores_and_unset_track_straight_line_availability() {
    let mut function = Function::new("straight".to_string(), IrType::I64, PhpType::Int);
    let slot;
    {
        let mut builder = Builder::new(&mut function);
        let parameter = add_int_local(&mut builder, "n");
        let local = add_int_local(&mut builder, "x");
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let _before_store = builder.emit_const_i64(1);
        let value = builder.emit_const_i64(2);
        builder.emit_store_local(local, value);
        let _ = builder.emit_load_local(local, IrType::I64, PhpType::Int);
        emit_unset(&mut builder, local);
        let after_unset = builder.emit_const_i64(3);
        builder.terminate(Terminator::Return { value: Some(after_unset) });
        slot = local;
        assert_eq!(parameter, LocalSlotId::from_raw(0));
    }
    function.params.push(FunctionParam {
        name: "n".to_string(),
        ir_type: IrType::I64,
        php_type: PhpType::Int,
        by_ref: false,
        variadic: false,
    });
    let availability = compute_debug_availability(
        &function,
        &[LocalSlotId::from_raw(0), slot],
    );
    let index = availability.index_of(slot).expect("debug index");
    let instructions = &function.blocks[0].instructions;
    assert_eq!(availability.dead_indices(instructions[0]), vec![index]);
    assert_eq!(availability.dead_indices(instructions[2]), vec![index], "before the store");
    assert!(availability.dead_indices(instructions[3]).is_empty(), "after the store");
    assert!(availability.dead_indices(instructions[4]).is_empty(), "before unset");
    assert_eq!(availability.dead_indices(instructions[5]), vec![index], "after unset");
    assert!(
        !availability.dead_indices(instructions[0]).contains(&0),
        "the parameter is available on entry"
    );
}

/// A local assigned in only one branch is dead at the join; assigning it in
/// both branches makes it available.
#[test]
fn branch_joins_intersect_defined_locals() {
    for store_in_both in [false, true] {
        let mut function = Function::new("join".to_string(), IrType::I64, PhpType::Int);
        let slot;
        let join_block;
        {
            let mut builder = Builder::new(&mut function);
            let local = add_int_local(&mut builder, "x");
            slot = Some(local);
            let entry = builder.create_named_block("entry", vec![]);
            let then_block = builder.create_named_block("then", vec![]);
            let else_block = builder.create_named_block("else", vec![]);
            let join = builder.create_named_block("join", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let cond = builder.emit_const_bool(true);
            builder.terminate(Terminator::CondBr {
                cond,
                then_target: then_block,
                then_args: Vec::new(),
                else_target: else_block,
                else_args: Vec::new(),
            });
            builder.position_at_end(then_block);
            let one = builder.emit_const_i64(1);
            builder.emit_store_local(local, one);
            builder.terminate(Terminator::Br { target: join, args: Vec::new() });
            builder.position_at_end(else_block);
            if store_in_both {
                let two = builder.emit_const_i64(2);
                builder.emit_store_local(local, two);
            }
            builder.terminate(Terminator::Br { target: join, args: Vec::new() });
            builder.position_at_end(join);
            let loaded = builder.emit_load_local(local, IrType::I64, PhpType::Int);
            join_block = Some(join);
            builder.terminate(Terminator::Return { value: Some(loaded) });
        }
        let availability = compute_debug_availability(&function, &[slot.unwrap()]);
        let index = availability.index_of(slot.unwrap()).expect("debug index");
        let join = join_block.expect("join block");
        let join_load = function.blocks[join.as_raw() as usize].instructions[0];
        let dead = availability.dead_indices(join_load);
        if store_in_both {
            assert!(dead.is_empty(), "both predecessors stored the local");
        } else {
            assert_eq!(dead, vec![index], "one predecessor skipped the store");
        }
    }
}

/// A slot promoted by mem2reg is no longer backed by its frame slot, so every
/// site reports it as optimized out.
#[test]
fn promoted_slots_are_optimized_out_everywhere() {
    let mut function = Function::new("promoted".to_string(), IrType::I64, PhpType::Int);
    let slot;
    {
        let mut builder = Builder::new(&mut function);
        let local = add_int_local(&mut builder, "x");
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let value = builder.emit_const_i64(7);
        builder.emit_store_local(local, value);
        let loaded = builder.emit_load_local(local, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(loaded) });
        slot = local;
    }
    Mem2Reg
        .run(&mut function, &mut crate::ir::DataPool::default())
        .then_some(())
        .expect("mem2reg should promote the slot");
    let availability = compute_debug_availability(&function, &[slot]);
    let index = availability.index_of(slot).expect("debug index");
    for block in &function.blocks {
        for instruction in &block.instructions {
            assert!(
                availability.dead_indices(*instruction).contains(&index),
                "promoted local must be optimized out"
            );
        }
    }
}

/// Dead lists compress to ranges so one source site stays small.
#[test]
fn dead_ranges_merge_consecutive_indices() {
    use crate::ir_passes::format_dead_ranges;
    assert_eq!(format_dead_ranges(&[]), "");
    assert_eq!(format_dead_ranges(&[0]), "0");
    assert_eq!(format_dead_ranges(&[0, 1, 2, 4, 6, 7]), "0-2,4,6-7");
}

/// A loop header keeps parameters and locals that the body never rewrites:
/// the optimistic back-edge state must not intersect the header down to zero.
#[test]
fn loop_headers_keep_locals_the_body_does_not_rewrite() {
    let mut function = Function::new("loop".to_string(), IrType::I64, PhpType::Int);
    let local;
    let header_block;
    let exit_block;
    {
        let mut builder = Builder::new(&mut function);
        local = add_int_local(&mut builder, "seen");
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("header", vec![]);
        let body = builder.create_named_block("body", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        header_block = header;
        exit_block = exit;
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Br { target: header, args: Vec::new() });
        builder.position_at_end(header);
        let header_condition = builder.emit_const_bool(true);
        builder.terminate(Terminator::CondBr {
            cond: header_condition,
            then_target: body,
            then_args: Vec::new(),
            else_target: exit,
            else_args: Vec::new(),
        });
        builder.position_at_end(body);
        let value = builder.emit_const_i64(1);
        builder.emit_store_local(local, value);
        builder.terminate(Terminator::Br { target: header, args: Vec::new() });
        builder.position_at_end(exit);
        let exit_value = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(exit_value) });
    }
    let header_instruction =
        function.blocks[header_block.as_raw() as usize].instructions[0];
    let exit_instruction = function.blocks[exit_block.as_raw() as usize].instructions[0];
    function.params.push(FunctionParam {
        name: "n".to_string(),
        ir_type: IrType::I64,
        php_type: PhpType::Int,
        by_ref: false,
        variadic: false,
    });
    let availability = compute_debug_availability(
        &function,
        &[LocalSlotId::from_raw(0), local],
    );
    let parameter = availability.index_of(LocalSlotId::from_raw(0)).expect("parameter index");
    let local_index = availability.index_of(local).expect("local index");
    for (instruction, why) in [
        (header_instruction, "loop header"),
        (exit_instruction, "loop exit"),
    ] {
        let dead = availability.dead_indices(instruction);
        assert!(!dead.contains(&parameter), "parameter dead at {why}: {dead:?}");
    }
    let dead = availability.dead_indices(header_instruction);
    assert!(
        !dead.contains(&local_index),
        "the back edge defines the body local at the header: {dead:?}"
    );
}

/// Optimized debug builds keep dead stores so a later stop cannot report the
/// previous value through a stale frame slot.
#[test]
fn optimized_debug_builds_keep_dead_stores() {
    use crate::codegen::platform::Target;
    use crate::ir::Module;
    let passes = |optimized| {
        let mut module = Module::new(Target::detect_host());
        module.debug_optimized = optimized;
        crate::ir_passes::driver::default_passes(&module)
            .into_iter()
            .any(|pass| pass.name() == "dead-store")
    };
    assert!(passes(false), "ordinary builds run dead-store elimination");
    assert!(!passes(true), "optimized debug builds must keep dead stores");
}
