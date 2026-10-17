//! Purpose:
//! Structural and supported-target tests for direct EIR tail recursion elimination.
//!
//! Called from:
//! - `cargo test -p elephc --lib tail_call`.
//!
//! Key details:
//! - Hand-built IR pins frame safety, exact binding, recursive components and fresh IDs.
//! - Runtime PHP tests separately prove the lowering is selected for real source programs.

use crate::codegen::platform::{Arch, Platform, Target};
use crate::ir::{validate_module, Builder, Effects, Function, FunctionParam, Immediate, IrType, LocalKind, Module, Op, Ownership, Terminator};
use crate::ir_passes::tail_call::eliminate_tail_recursion;
use crate::types::PhpType;

/// Builds one scalar tail-recursive body with a base case and an optional second parameter.
fn tail_function(module: &mut Module, name: &str, callee: &str, params: usize) -> Function {
    let mut function = Function::new(name.into(), IrType::I64, PhpType::Int);
    let slots: Vec<_> = (0..params).map(|index| {
        let name = format!("p{index}");
        function.params.push(FunctionParam { name: name.clone(), ir_type: IrType::I64,
            php_type: PhpType::Int, by_ref: false, variadic: false });
        function.add_local(Some(name), IrType::I64, PhpType::Int, LocalKind::PhpLocal)
    }).collect();
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    let base = builder.create_named_block("base", vec![]);
    let recurse = builder.create_named_block("recurse", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let n = builder.emit_load_local(slots[0], IrType::I64, PhpType::Int);
    builder.terminate(Terminator::CondBr { cond: n, then_target: recurse, then_args: vec![],
        else_target: base, else_args: vec![] });
    builder.position_at_end(base);
    let zero = builder.emit_const_i64(0);
    builder.terminate(Terminator::Return { value: Some(zero) });
    builder.position_at_end(recurse);
    let one = builder.emit_const_i64(1);
    let next = builder.emit(Op::ISub, vec![n, one], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
    let mut args = vec![next];
    for &slot in &slots[1..] { args.push(builder.emit_load_local(slot, IrType::I64, PhpType::Int)); }
    let result = builder.emit(Op::Call, args, Some(Immediate::Data(module.data.intern_function_name(callee))),
        IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
    builder.terminate(Terminator::Return { value: Some(result) });
    function
}

/// Creates a two-function mutual cycle with directly returned scalar calls.
fn mutual_module() -> Module {
    let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let a = tail_function(&mut module, "a", "b", 1);
    let b = tail_function(&mut module, "b", "a", 1);
    module.add_function(a);
    module.add_function(b);
    module
}

/// Self recursion becomes a back edge, preserves validator invariants and is idempotent.
#[test]
fn tail_call_self_rebinds_parameters() {
    let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let function = tail_function(&mut module, "count", "count", 2);
    module.add_function(function);
    assert!(eliminate_tail_recursion(&mut module));
    validate_module(&module).unwrap();
    let function = &module.functions[0];
    assert!(function.instructions.iter().all(|inst| inst.op != Op::Call));
    assert_eq!(function.blocks[function.entry.as_raw() as usize].name, "tail_initial");
    assert!(function.blocks.iter().any(|block| block.name == "tail_rebind_count" && block.params.len() == 2));
    assert!(!eliminate_tail_recursion(&mut module));
}

/// Every mutual ABI entry contains both bodies, with no recursive call and fresh IDs.
#[test]
fn tail_call_mutual_clones_each_body_once_per_entry() {
    let mut module = mutual_module();
    assert!(eliminate_tail_recursion(&mut module));
    validate_module(&module).unwrap();
    for function in &module.functions {
        assert!(function.instructions.iter().all(|inst| inst.op != Op::Call));
        assert_eq!(function.blocks.iter().filter(|block| block.name.starts_with("tail_rebind_")).count(), 2);
    }
}

/// A non-tail add after the call must keep its ordinary recursive activation.
#[test]
fn tail_call_rejects_non_tail_work() {
    let mut module = mutual_module();
    let function = &mut module.functions[0];
    let recurse = function.blocks[2].id;
    let Some(Terminator::Return { value: Some(value) }) = function.blocks[2].terminator.take() else { panic!() };
    let mut builder = Builder::new(function);
    builder.position_at_end(recurse);
    let one = builder.emit_const_i64(1);
    let result = builder.emit_iadd(value, one);
    builder.terminate(Terminator::Return { value: Some(result) });
    assert!(!eliminate_tail_recursion(&mut module));
    validate_module(&module).unwrap();
}

/// Reference, variadic, owning, exceptional and representation-changing frames retain calls.
#[test]
fn tail_call_rejects_unsafe_frames_and_coercions() {
    for guard in 0..7 {
        let mut module = mutual_module();
        let function = &mut module.functions[0];
        match guard {
            0 => function.params[0].by_ref = true,
            1 => function.params[0].variadic = true,
            2 => { function.add_local(Some("s".into()), IrType::Str, PhpType::Str, LocalKind::PhpLocal); }
            3 => function.locals[0].kind = LocalKind::StaticLocal,
            4 => function.flags.is_generator = true,
            5 => function.flags.by_ref_return = true,
            _ => function.params[0].php_type = PhpType::Bool,
        }
        assert!(!eliminate_tail_recursion(&mut module), "guard {guard}");
    }
}

/// Every parameter read regains local effects after rebinding makes it mutable.
#[test]
fn tail_call_restores_immutable_parameter_load_effects() {
    let mut module = mutual_module();
    for function in &mut module.functions {
        for inst in &mut function.instructions {
            if inst.op == Op::LoadLocal { inst.effects = Effects::PURE; }
        }
    }
    assert!(eliminate_tail_recursion(&mut module));
    for function in &module.functions {
        assert!(function.instructions.iter().filter(|inst| inst.op == Op::LoadLocal)
            .all(|inst| inst.effects.contains(Effects::READS_LOCAL)));
    }
}

/// Backtrace/eval support must preserve the original physical activation chain.
#[test]
fn tail_call_preserves_observable_activation_chains() {
    let mut module = mutual_module();
    module.required_runtime_features.eval_bridge = true;
    assert!(!eliminate_tail_recursion(&mut module));
}

/// All supported targets emit both mutual ABI entries and start at the declared initial block.
#[test]
fn tail_call_emits_for_every_supported_target() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = mutual_module();
        module.target = Target::parse(target).unwrap();
        let mut main = Function::new("main".into(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        let mut builder = Builder::new(&mut main);
        let entry = builder.create_named_block("main", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Return { value: None });
        module.add_function(main);
        assert!(eliminate_tail_recursion(&mut module));
        validate_module(&module).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let initial = assembly.find("@block name=tail_initial").expect("initial entry emitted");
        let body = assembly.find("@block name=entry").expect("original body emitted");
        assert!(initial < body, "{target}: ABI prologue must enter tail_initial first");
    }
}

/// Builds a numeric argument owner across blocks, optionally exposing an unbalanced return path.
fn rooted_numeric_module(unbalanced: bool, numeric: bool) -> Module {
    use crate::ir::{IrHeapKind, LocalSlotId};
    let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut function = Function::new("rooted".into(), IrType::I64, PhpType::Int);
    function.params.push(FunctionParam { name: "n".into(), ir_type: IrType::I64,
        php_type: PhpType::Int, by_ref: false, variadic: false });
    let n = function.add_local(Some("n".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    let root = function.add_local(None, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, LocalKind::HiddenTemp);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    let tail = builder.create_named_block("tail", vec![]);
    let other = builder.create_named_block("other", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let n_value = builder.emit_load_local(n, IrType::I64, PhpType::Int);
    let boxed = builder.emit(if numeric { Op::MixedBox } else { Op::Call },
        if numeric { vec![n_value] } else { vec![] },
        if numeric { None } else { Some(Immediate::Data(module.data.intern_function_name("unknown"))) },
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    builder.emit_store_local(root, boxed);
    builder.emit(Op::PushCallOperandOwner, vec![], Some(Immediate::LocalSlot(root)),
        IrType::Void, PhpType::Void, Ownership::NonHeap);
    if unbalanced {
        builder.terminate(Terminator::CondBr { cond: n_value, then_target: tail, then_args: vec![],
            else_target: other, else_args: vec![] });
    } else {
        builder.terminate(Terminator::Br { target: tail, args: vec![] });
    }
    builder.position_at_end(other);
    let zero = builder.emit_const_i64(0);
    builder.terminate(Terminator::Return { value: Some(zero) });
    builder.position_at_end(tail);
    let result = builder.emit(Op::Call, vec![boxed],
        Some(Immediate::Data(module.data.intern_function_name("rooted"))),
        IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
    for op in [Op::PopCallOperandOwner, Op::ReleaseLocalSlot] {
        builder.emit(op, vec![], Some(Immediate::LocalSlot(root)), IrType::Void, PhpType::Void, Ownership::NonHeap);
    }
    builder.terminate(Terminator::Return { value: Some(result) });
    module.add_function(function);
    assert_eq!(root, LocalSlotId::from_raw(1));
    module
}

/// Numeric roots spanning inlined argument blocks retire before the scalar branch.
#[test]
fn tail_call_numeric_roots_balance_across_blocks() {
    let mut module = rooted_numeric_module(false, true);
    assert!(eliminate_tail_recursion(&mut module));
    validate_module(&module).unwrap();
    let function = &module.functions[0];
    assert!(function.instructions.iter().any(|inst| inst.op == Op::Cast));
    assert!(function.instructions.iter().all(|inst| inst.op != Op::Call));
}

/// A numeric root still live on any normal return prevents frame reuse.
#[test]
fn tail_call_rejects_root_live_on_another_return() {
    let mut module = rooted_numeric_module(true, true);
    assert!(!eliminate_tail_recursion(&mut module));
}

/// A balanced root carrying an unknown Mixed result has no numeric cleanup proof.
#[test]
fn tail_call_rejects_arbitrary_mixed_roots() {
    let mut module = rooted_numeric_module(false, false);
    assert!(!eliminate_tail_recursion(&mut module));
}

/// A shared local-slot return join keeps the base result while tail stores are erased.
#[test]
fn tail_call_forwards_scalar_local_return_joins() {
    let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut function = tail_function(&mut module, "joined", "joined", 1);
    let slot = function.add_local(None, IrType::I64, PhpType::Int, LocalKind::OwnedTemp);
    let mut builder = Builder::new(&mut function);
    let join = builder.create_named_block("return_join", vec![]);
    for index in [1, 2] {
        let Some(Terminator::Return { value: Some(value) }) = function.blocks[index].terminator.take() else { panic!() };
        let block = function.blocks[index].id;
        let mut builder = Builder::new(&mut function);
        builder.position_at_end(block);
        builder.emit_store_local(slot, value);
        builder.terminate(Terminator::Br { target: join, args: vec![] });
    }
    let mut builder = Builder::new(&mut function);
    builder.position_at_end(join);
    let value = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
    builder.emit(Op::UnsetLocal, vec![], Some(Immediate::LocalSlot(slot)),
        IrType::Void, PhpType::Void, Ownership::NonHeap);
    builder.terminate(Terminator::Return { value: Some(value) });
    module.add_function(function);
    validate_module(&module).unwrap();
    assert!(eliminate_tail_recursion(&mut module));
    validate_module(&module).unwrap();
    assert!(module.functions[0].instructions.iter().all(|inst| inst.op != Op::Call));
}

/// A tail edge must never bypass numeric owner cleanup in a shared return block.
#[test]
fn tail_call_keeps_cleanup_in_forwarded_blocks() {
    let mut module = rooted_numeric_module(false, true);
    let function = &mut module.functions[0];
    let block = function.blocks[1].id;
    let mut builder = Builder::new(function);
    let join = builder.create_named_block("cleanup_join", vec![]);
    let function = &mut module.functions[0];
    let cleanup = function.blocks[1].instructions.split_off(1);
    let returned = function.blocks[1].terminator.take();
    function.blocks[1].terminator = Some(Terminator::Br { target: join, args: vec![] });
    function.blocks[join.as_raw() as usize].instructions = cleanup;
    function.blocks[join.as_raw() as usize].terminator = returned;
    for (index, &id) in function.blocks[join.as_raw() as usize].instructions.iter().enumerate() {
        if let Some(value) = function.instructions[id.as_raw() as usize].result {
            function.values[value.as_raw() as usize].def = crate::ir::ValueDef::Instruction {
                block: join, index: index as u32, inst: id,
            };
        }
    }
    assert_eq!(block.as_raw(), 1);
    validate_module(&module).unwrap();
    assert!(!eliminate_tail_recursion(&mut module));
}
