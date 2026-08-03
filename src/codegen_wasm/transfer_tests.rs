//! Purpose:
//! Regression tests for the wasm32-wasi type-aware value transfer layer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Exercises concrete-to-Mixed boxing, void-call null-sentinel boxing,
//!   checked Mixed-to-concrete unboxing, block-argument transfers, and the
//!   main-function `$argc`/`$argv` prologue. Each generated module is assembled
//!   and validated with `wasmparser`; behavioral tests run under `wasmer` when
//!   installed.

use super::generate_module as generate;
use crate::codegen::Emit;
use crate::codegen::platform::Target;
use crate::ir::{
    Builder, Function, FunctionParam, Immediate, IrHeapKind, IrType, LocalKind, Module, Op,
    Ownership, Terminator,
};
use crate::types::PhpType;

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// Per-process counter for unique temp directories used by parallel wasmer runs.
static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

/// Verifies equal-width i64 values do not bypass PHP-type validation at a call
/// boundary, where treating an integer as a callable would corrupt dispatch.
#[test]
fn equal_width_scalar_transfer_still_requires_php_type_identity() {
    let error = super::transfer::classify_transfer(
        IrType::I64,
        PhpType::Int,
        IrType::I64,
        PhpType::Callable,
    )
    .expect_err("int must not bit-copy into callable storage");

    assert!(error.to_string().contains("unsupported wasm value transfer"));
}

/// Rejects malformed source metadata before any boxing branch can reinterpret
/// same-width values as a different PHP runtime kind.
#[test]
fn malformed_source_storage_pairs_never_box_as_mixed() {
    let malformed = [
        (IrType::I64, PhpType::Object("C".to_string())),
        (IrType::F64, PhpType::Int),
        (IrType::Str, PhpType::Int),
        (
            IrType::Heap(IrHeapKind::Array),
            PhpType::Object("C".to_string()),
        ),
    ];

    for (source_ir, source_php) in malformed {
        let error = super::transfer::classify_transfer(
            source_ir,
            source_php.clone(),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        )
        .expect_err("malformed source pair must be rejected");
        assert!(
            error.to_string().contains("invalid source"),
            "unexpected error for {source_ir:?}/{source_php:?}: {error}"
        );
    }
}

/// Rejects malformed destination metadata before Mixed unboxing selects a cast
/// solely from the destination's WASM width.
#[test]
fn malformed_destination_storage_pairs_never_unbox_mixed() {
    let malformed = [
        (IrType::I64, PhpType::Object("C".to_string())),
        (IrType::F64, PhpType::Int),
        (IrType::Str, PhpType::Int),
        (
            IrType::Heap(IrHeapKind::Array),
            PhpType::Object("C".to_string()),
        ),
    ];

    for (dest_ir, dest_php) in malformed {
        let error = super::transfer::classify_transfer(
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            dest_ir,
            dest_php.clone(),
        )
        .expect_err("malformed destination pair must be rejected");
        assert!(
            error.to_string().contains("invalid destination"),
            "unexpected error for {dest_ir:?}/{dest_php:?}: {error}"
        );
    }
}

/// Rejects canonical i64 destinations whose semantics need a specialized
/// unboxer instead of the generic integer-cast transfer path.
#[test]
fn mixed_to_callable_or_pointer_requires_specialized_unboxing() {
    for php_type in [PhpType::Callable, PhpType::Pointer(None)] {
        let error = super::transfer::classify_transfer(
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            IrType::I64,
            php_type.clone(),
        )
        .expect_err("generic Mixed unboxing must not reinterpret callable/pointer payloads");
        assert!(
            error.to_string().contains("unboxing a Mixed cell"),
            "unexpected error for {php_type:?}: {error}"
        );
    }
}

/// Covers every canonical EIR storage family accepted by the shared pair
/// validator, including the explicit null-sentinel exception.
#[test]
fn canonical_storage_pair_matrix_is_exhaustive() {
    let canonical = [
        (IrType::I64, PhpType::Int),
        (IrType::I64, PhpType::Bool),
        (IrType::I64, PhpType::Callable),
        (IrType::I64, PhpType::Pointer(None)),
        (IrType::F64, PhpType::Float),
        (IrType::Str, PhpType::Str),
        (IrType::TaggedScalar, PhpType::TaggedScalar),
        (
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
        ),
        (
            IrType::Heap(IrHeapKind::Hash),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Int),
            },
        ),
        (
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object("C".to_string()),
        ),
        (IrType::Heap(IrHeapKind::Iterable), PhpType::Iterable),
        (
            IrType::Heap(IrHeapKind::Buffer),
            PhpType::Buffer(Box::new(PhpType::Int)),
        ),
        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed),
        (IrType::Void, PhpType::Void),
        (IrType::I64, PhpType::Void),
    ];

    for (ir_type, php_type) in canonical {
        super::transfer::validate_storage_pair(ir_type, &php_type)
            .unwrap_or_else(|error| panic!("{ir_type:?}/{php_type:?}: {error}"));
    }
}

/// Returns a fresh temp directory path so concurrent wasmer runs cannot collide.
fn unique_tmp_dir(tag: &str) -> std::path::PathBuf {
    let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "elephc_wasm_transfer_{}_{}_{}",
        tag,
        std::process::id(),
        n
    ))
}

/// Assembles WAT to wasm bytes and validates them, returning the bytes.
///
/// Panics with the WAT text if assembly or validation fails, so a structural or
/// typing defect in the transfer lowering is reported legibly.
fn assemble_and_validate(wat: &str) -> Vec<u8> {
    let bytes = ::wat::parse_str(wat).unwrap_or_else(|e| panic!("WAT did not assemble: {e}\n{wat}"));
    wasmparser::validate(&bytes)
        .unwrap_or_else(|e| panic!("wasm did not validate: {e}\n{wat}"));
    bytes
}

/// Returns true when the `wasmer` CLI is available.
fn wasmer_available() -> bool {
    Command::new("wasmer")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Runs a generated command module under `wasmer` with the given CLI arguments
/// and returns its trimmed stdout.  Skips and returns `None` when wasmer is absent.
fn run_main_with_args(module: &Module, args: &[&str]) -> Option<String> {
    let wat = generate(module, Emit::Executable).expect("module should lower");
    let _bytes = assemble_and_validate(&wat);
    if !wasmer_available() {
        return None;
    }
    let dir = unique_tmp_dir("run");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("m.wasm");
    std::fs::write(&path, &_bytes).expect("write wasm");
    let mut cmd = Command::new("wasmer");
    cmd.arg("run").arg(&path);
    if !args.is_empty() {
        cmd.arg("--");
        cmd.args(args);
    }
    let out = cmd.output().expect("run wasmer");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "wasmer run failed: {}\n{}",
        String::from_utf8_lossy(&out.stderr),
        wat
    );
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Builds a module whose single non-main function receives an i64 block argument
/// and forwards it to a second block whose parameter is a Mixed cell.  The body
/// echoes the Mixed cell.
fn module_i64_to_mixed_block_arg() -> Module {
    let mut module = Module::new(Target::wasm());
    let mut function = Function::new("thread".to_string(), IrType::Void, PhpType::Void);
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![(IrType::I64, PhpType::Int)]);
        let body = b.create_named_block(
            "body",
            vec![(IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)],
        );
        b.set_entry(entry);
        let x = b.block_param(entry, 0);
        let _y = b.block_param(body, 0);
        b.position_at_end(entry);
        b.terminate(Terminator::Br {
            target: body,
            args: vec![x],
        });
        b.position_at_end(body);
        // The destination block only needs to receive the boxed Mixed cell;
        // returning immediately keeps the module free of command-runtime imports.
        b.terminate(Terminator::Return { value: None });
    }
    module.add_function(function);
    module
}

/// Builds a module with a void callee and a `main` that observes its result
/// through a Mixed value.
fn module_void_call_result_to_mixed() -> Module {
    let mut module = Module::new(Target::wasm());

    let mut callee = Function::new("side_effect".to_string(), IrType::Void, PhpType::Void);
    {
        let mut b = Builder::new(&mut callee);
        let entry = b.create_named_block("entry", Vec::new());
        b.set_entry(entry);
        b.position_at_end(entry);
        b.terminate(Terminator::Return { value: None });
    }
    module.add_function(callee);
    let side_effect_name = module.data.intern_function_name("side_effect");

    let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    main.flags.is_main = true;
    {
        let mut b = Builder::new(&mut main);
        let entry = b.create_named_block("entry", Vec::new());
        b.set_entry(entry);
        b.position_at_end(entry);
        // The expression-facing result is Mixed even though the callee's actual
        // ABI result is void. Lowering must synthesize a null Mixed cell instead
        // of reading from an empty WASM operand stack.
        let result = b
            .emit(
                Op::Call,
                Vec::new(),
                Some(Immediate::Data(side_effect_name)),
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .unwrap();
        b.emit(
            Op::EchoValue,
            vec![result],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        b.terminate(Terminator::Return { value: None });
    }
    module.add_function(main);
    module
}

/// Builds a module with an integer callee and a `main` that calls it, storing the
/// concrete return into a Mixed local and echoing it.
fn module_int_call_result_to_mixed() -> Module {
    let mut module = Module::new(Target::wasm());

    let mut callee = Function::new("answer".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut callee);
        let entry = b.create_named_block("entry", Vec::new());
        b.set_entry(entry);
        b.position_at_end(entry);
        let v = b.emit_const_i64(42);
        b.terminate(Terminator::Return { value: Some(v) });
    }
    module.add_function(callee);
    let answer_name = module.data.intern_function_name("answer");

    let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    main.flags.is_main = true;
    let mixed_slot = main.add_local(
        Some("mixed_result".to_string()),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        LocalKind::HiddenTemp,
    );
    {
        let mut b = Builder::new(&mut main);
        let entry = b.create_named_block("entry", Vec::new());
        b.set_entry(entry);
        b.position_at_end(entry);
        let call_result = b
            .emit(
                Op::Call,
                Vec::new(),
                Some(Immediate::Data(answer_name)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        b.emit_store_local(mixed_slot, call_result);
        let loaded = b.emit_load_local(
            mixed_slot,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        b.emit(
            Op::EchoValue,
            vec![loaded],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        b.terminate(Terminator::Return { value: None });
    }
    module.add_function(main);
    module
}

/// Builds a `main` function with an `$argc` local and echoes it.
fn module_main_argc_local() -> Module {
    let mut module = Module::new(Target::wasm());
    let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    function.flags.is_main = true;
    let argc_slot = function.add_local(
        Some("argc".to_string()),
        IrType::I64,
        PhpType::Int,
        LocalKind::PhpLocal,
    );
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", Vec::new());
        b.set_entry(entry);
        b.position_at_end(entry);
        let v = b.emit_load_local(argc_slot, IrType::I64, PhpType::Int);
        b.emit(
            Op::EchoValue,
            vec![v],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        b.terminate(Terminator::Return { value: None });
    }
    module.add_function(function);
    module
}

/// Builds a `main` function that stores an i64 local into a Mixed local, loads
/// it back, and echoes it.  This reproduces the while-loop regression where an
/// i64 increment was stored into an i32 Mixed slot.
fn module_i64_local_store_into_mixed() -> Module {
    let mut module = Module::new(Target::wasm());
    let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    main.flags.is_main = true;
    let i64_slot = main.add_local(
        Some("counter".to_string()),
        IrType::I64,
        PhpType::Int,
        LocalKind::PhpLocal,
    );
    let mixed_slot = main.add_local(
        Some("mixed_result".to_string()),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        LocalKind::HiddenTemp,
    );
    {
        let mut b = Builder::new(&mut main);
        let entry = b.create_named_block("entry", Vec::new());
        b.set_entry(entry);
        b.position_at_end(entry);
        let zero = b.emit_const_i64(3);
        b.emit_store_local(i64_slot, zero);
        let loaded = b.emit_load_local(i64_slot, IrType::I64, PhpType::Int);
        b.emit_store_local(mixed_slot, loaded);
        let boxed = b.emit_load_local(mixed_slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        b.emit(Op::EchoValue, vec![boxed], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        b.terminate(Terminator::Return { value: None });
    }
    module.add_function(main);
    module
}

/// Builds a non-main function that receives a Mixed cell and stores it into a
/// concrete heap slot, exercising runtime tag validation and payload extraction.
fn module_mixed_param_to_heap(
    function_name: &str,
    dest_ir: IrType,
    dest_php: PhpType,
) -> Module {
    let mut module = Module::new(Target::wasm());
    let mut function =
        Function::new(function_name.to_string(), IrType::Void, PhpType::Void);
    function.params.push(FunctionParam {
        name: "value".to_string(),
        ir_type: IrType::Heap(IrHeapKind::Mixed),
        php_type: PhpType::Mixed,
        by_ref: false,
        variadic: false,
    });
    let mixed_slot = function.add_local(
        Some("value".to_string()),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        LocalKind::PhpLocal,
    );
    let dest_slot = function.add_local(
        Some("destination".to_string()),
        dest_ir,
        dest_php.clone(),
        LocalKind::HiddenTemp,
    );
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", Vec::new());
        b.set_entry(entry);
        b.position_at_end(entry);
        let mixed = b.emit_load_local(
            mixed_slot,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        b.emit_store_local(dest_slot, mixed);
        b.terminate(Terminator::Return { value: None });
    }
    module.add_function(function);
    module
}

/// Builds a command that round-trips an empty array through a Mixed cell and
/// prints its length after checked unboxing.
fn module_array_round_trip_through_mixed() -> Module {
    let mut module = Module::new(Target::wasm());
    let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    main.flags.is_main = true;
    let mixed_slot = main.add_local(
        Some("mixed".to_string()),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        LocalKind::HiddenTemp,
    );
    let array_php = PhpType::Array(Box::new(PhpType::Mixed));
    let array_slot = main.add_local(
        Some("array".to_string()),
        IrType::Heap(IrHeapKind::Array),
        array_php.clone(),
        LocalKind::HiddenTemp,
    );
    {
        let mut b = Builder::new(&mut main);
        let entry = b.create_named_block("entry", Vec::new());
        b.set_entry(entry);
        b.position_at_end(entry);
        let array = b
            .emit(
                Op::ArrayNew,
                Vec::new(),
                Some(Immediate::Capacity(0)),
                IrType::Heap(IrHeapKind::Array),
                array_php.clone(),
                Ownership::Owned,
            )
            .unwrap();
        b.emit_store_local(mixed_slot, array);
        let mixed = b.emit_load_local(
            mixed_slot,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        b.emit_store_local(array_slot, mixed);
        let restored = b.emit_load_local(
            array_slot,
            IrType::Heap(IrHeapKind::Array),
            array_php,
        );
        let len = b
            .emit(
                Op::ArrayLen,
                vec![restored],
                None,
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        b.emit(
            Op::EchoValue,
            vec![len],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        b.terminate(Terminator::Return { value: None });
    }
    module.add_function(main);
    module
}

/// Verifies that an i64 block argument is boxed into a Mixed param and the
/// generated module validates.
#[test]
fn i64_block_arg_boxes_into_mixed_param() {
    let wat = generate(&module_i64_to_mixed_block_arg(), Emit::Executable)
        .expect("i64-to-mixed block arg should lower");
    assert!(wat.contains("(func $fn_u_thread"), "{wat}");
    assemble_and_validate(&wat);
}

/// Verifies that a void callee's result materializes as a null Mixed cell,
/// producing a valid module.
#[test]
fn void_call_result_boxes_null_for_mixed_destination() {
    let wat = generate(&module_void_call_result_to_mixed(), Emit::Executable)
        .expect("void call result should lower");
    assert!(wat.contains("(func $_entry (export \"_start\")"), "{wat}");
    assert!(wat.contains("call $fn_u_side_u_effect"), "{wat}");
    assert!(wat.contains("Elephc null sentinel for void callee result"), "{wat}");
    assert!(wat.contains("call $__rt_mixed_from_value"), "{wat}");
    assemble_and_validate(&wat);
}

/// Verifies that a concrete int return is boxed for a Mixed result slot and
/// that the program prints the expected value under wasmer.
#[test]
fn int_call_result_boxes_into_mixed_slot() {
    let out = run_main_with_args(&module_int_call_result_to_mixed(), &[]);
    if let Some(out) = out {
        assert_eq!(out, "42");
    }
}

/// Verifies that the main-function prologue initializes the source `$argc`
/// local from `__rt_argc` and that `echo $argc` observes the host argument count.
#[test]
fn main_argc_local_is_initialized_from_wasi() {
    let out = run_main_with_args(&module_main_argc_local(), &["a", "b", "c"]);
    if let Some(out) = out {
        // argv[0] is the wasm file itself plus the three explicit args.
        assert_eq!(out, "4");
    }
}

/// Verifies that storing an i64 local into a Mixed slot boxes the integer and
/// that the program prints the expected value under wasmer.
#[test]
fn i64_local_store_boxes_into_mixed_slot() {
    let out = run_main_with_args(&module_i64_local_store_into_mixed(), &[]);
    if let Some(out) = out {
        assert_eq!(out, "3");
    }
}

/// Verifies that Mixed heap payloads are checked against the destination kind
/// before their low word is exposed as a pointer.
#[test]
fn mixed_heap_unboxing_validates_array_hash_object_and_iterable_tags() {
    let cases = [
        (
            "mixed_to_array",
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Mixed)),
            &["i64.const 4"][..],
        ),
        (
            "mixed_to_hash",
            IrType::Heap(IrHeapKind::Hash),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            },
            &["i64.const 5"][..],
        ),
        (
            "mixed_to_object",
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object("stdClass".to_string()),
            &["i64.const 6"][..],
        ),
        (
            "mixed_to_iterable",
            IrType::Heap(IrHeapKind::Iterable),
            PhpType::Iterable,
            &["i64.const 4", "i64.const 5", "i64.const 6"][..],
        ),
    ];

    for (name, ir, php, expected_tags) in cases {
        let wat = generate(
            &module_mixed_param_to_heap(name, ir, php),
            Emit::Executable,
        )
        .unwrap_or_else(|error| panic!("{name} should lower: {error}"));
        assert!(wat.contains("call $__rt_mixed_unbox"), "{name}\n{wat}");
        assert!(
            wat.contains(
                "elephc-trap:non-public:reactor-mixed-heap-mismatch"
            ),
            "{name}\n{wat}"
        );
        for tag in expected_tags {
            assert!(wat.contains(tag), "{name} missing {tag}\n{wat}");
        }
        assemble_and_validate(&wat);
    }
}

/// Verifies the mixed-cell payload word order by round-tripping a real array and
/// observing its length after checked unboxing.
#[test]
fn array_round_trip_through_mixed_preserves_pointer_payload() {
    let out = run_main_with_args(&module_array_round_trip_through_mixed(), &[]);
    if let Some(out) = out {
        assert_eq!(out, "0");
    }
}
