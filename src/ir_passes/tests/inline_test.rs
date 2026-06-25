//! Purpose:
//! Hand-built EIR unit tests for the small-function inliner pass.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (ir_passes).
//!
//! Key details:
//! - Constructs minimal Module + Function bodies directly with Builder for callees
//!   and manual call emission for callers (Data immediate + function_names).
//! - Verifies change flag, Call removal, result flow via continuation block param,
//!   validator cleanliness, and non-inlining of large/recursive/try/gen cases.
//! - Also covers the ownership/termination guards: mutual recursion must not hang
//!   or inline, and by-ref/refcounted callees must be refused (the splice bypasses
//!   the callee epilogue, so inlining owned state would leak).
//! - Uses $argc-style "runtime unknown" motivation only at e2e layer.

use crate::ir::{
    validate_module, Builder, Function, FunctionParam, Immediate, IrHeapKind, IrType, LocalKind,
    Module, Op, Terminator,
};
use crate::codegen::platform::{Arch, Platform, Target};
use crate::ir::Ownership;
use crate::ir_passes::inline::inline_small_functions;
use crate::types::PhpType;

/// Helper: build a tiny const-returning function "fortytwo" (entry has 0 params per EIR rule).
fn make_fortytwo_callee() -> Function {
    let mut f = Function::new("fortytwo".to_string(), IrType::I64, PhpType::Int);
    let mut b = Builder::new(&mut f);
    let entry = b.create_named_block("entry", vec![]);
    b.set_entry(entry);
    b.position_at_end(entry);
    let res = b.emit_const_i64(42);
    b.terminate(Terminator::Return { value: Some(res) });
    f
}

/// Helper: build a void callee "do_nothing" with a couple of pure instrs.
fn make_void_callee() -> Function {
    let mut f = Function::new("do_nothing".to_string(), IrType::Void, PhpType::Void);
    let mut b = Builder::new(&mut f);
    let entry = b.create_named_block("entry", vec![]);
    b.set_entry(entry);
    b.position_at_end(entry);
    let _ = b.emit_const_i64(42); // dead const ok for test
    b.terminate(Terminator::Return { value: None });
    f
}

/// Build a minimal module with one caller calling a small callee by Data name.
fn make_simple_caller_callee_module() -> (Module, String /*caller name*/) {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));

    // callee first so index stable
    let ft = make_fortytwo_callee();
    module.add_function(ft);

    // caller: main that calls fortytwo() (no params) and returns the result
    let mut caller = Function::new("main_inline".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut caller);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        // Emit Call with Data immediate pointing to "fortytwo" in function_names. The
        // callee takes no parameters, so the call passes no operands (operand count must
        // match the parameter count for the inliner to bind arguments directly).
        let data_id = module.data.intern_function_name("fortytwo");
        let call_res = b
            .emit(
                Op::Call,
                vec![],
                Some(Immediate::Data(data_id)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        b.terminate(Terminator::Return {
            value: Some(call_res),
        });
    }
    let caller_name = caller.name.clone();
    module.add_function(caller);
    (module, caller_name)
}

#[test]
fn inliner_inlines_small_returning_function_and_removes_call() {
    let (mut module, caller_name) = make_simple_caller_callee_module();
    let changed = inline_small_functions(&mut module);
    assert!(changed, "should report change for eligible inline");

    // Find the inlined caller (now has no Call)
    let caller = module
        .functions
        .iter()
        .find(|f| f.name == caller_name)
        .expect("caller present");

    let has_call = caller
        .instructions
        .iter()
        .any(|i| matches!(i.op, Op::Call | Op::FunctionVariantCall));
    assert!(!has_call, "original Call should be gone after inlining");

    // Must still be validator clean
    if let Err(e) = validate_module(&module) {
        panic!("module invalid after inlining: {:?}", e);
    }

    // Result value produced by the inlined const
    let has_const42 = caller.instructions.iter().any(|i| matches!(i.immediate, Some(Immediate::I64(42))));
    assert!(has_const42, "inlined body should contribute the const 42 result");
}

#[test]
fn inliner_supports_function_variant_call_sites() {
    // FVC immediate + label in strings so extract_target_name resolves a name.
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));
    let ft = make_fortytwo_callee();
    module.add_function(ft);
    // Use distinct group name in label so collect includes the group (real usage: public group vs concrete variant names).
    // variant at index 0 will resolve to the concrete "fortytwo".
    let _ = module.data.intern_function_name("fortytwo");
    let _ = module.data.intern_string("fgroup:fortytwo");

    let mut host = Function::new("h_fvc".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut host);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        // FVC immediate (group/variant are opaque ids; name comes from string label)
        let r = b.emit(
            Op::FunctionVariantCall,
            vec![],
            Some(Immediate::FunctionVariantRef { group: 0, variant: 0 }),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        ).unwrap();
        b.terminate(Terminator::Return { value: Some(r) });
    }
    module.add_function(host);

    let changed = inline_small_functions(&mut module);
    assert!(changed, "FVC to small fn should inline when name resolves via label");
    let hf = module.functions.iter().find(|f| f.name == "h_fvc").unwrap();
    let has_fvc_or_call = hf.instructions.iter().any(|i| matches!(i.op, Op::Call | Op::FunctionVariantCall));
    assert!(!has_fvc_or_call, "no FVC/Call remains after inlining at FVC site");
    assert!(validate_module(&module).is_ok());
}

/// Unit test the resolver (per strategy): builds real Module (functions + label in strings),
/// asserts collect_dispatch_groups order and resolve_variant_callee_name returns the concrete.
#[test]
fn resolver_collect_and_resolve_fvc_uses_canonical_logic() {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));
    let mut ft = Function::new("fortytwo".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut ft);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let v = b.emit_const_i64(42);
        b.terminate(Terminator::Return { value: Some(v) });
    }
    module.add_function(ft);
    let _ = module.data.intern_function_name("fortytwo");
    let _ = module.data.intern_string("fgroup:fortytwo");

    let groups = crate::ir::function_variants::collect_dispatch_groups(&module);
    assert!(!groups.is_empty());
    let g0 = &groups[0];
    assert_eq!(g0.name, "fgroup");
    assert_eq!(g0.variants, vec!["fortytwo".to_string()]);

    let name = crate::ir::function_variants::resolve_variant_callee_name(&module, 0, 0);
    assert_eq!(name.as_deref(), Some("fortytwo"));

    let callee = crate::ir::function_variants::resolve_variant_callee(&module, 0, 0);
    assert!(callee.is_some());
    assert_eq!(callee.unwrap().name, "fortytwo");
}

#[test]
fn inliner_inlines_void_small_function() {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));
    let void_c = make_void_callee();
    module.add_function(void_c);

    let mut caller = Function::new("caller_void".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut caller);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let data_id = module.data.intern_function_name("do_nothing");
        // void call (no result)
        b.emit(
            Op::Call,
            vec![],
            Some(Immediate::Data(data_id)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let seven = b.emit_const_i64(7);
        b.terminate(Terminator::Return { value: Some(seven) });
    }
    module.add_function(caller);

    let changed = inline_small_functions(&mut module);
    assert!(changed);

    // No Call left in any
    for f in &module.functions {
        assert!(!f.instructions.iter().any(|i| i.op == Op::Call));
    }
    assert!(validate_module(&module).is_ok());
}

#[test]
fn inliner_respects_size_threshold_and_non_recursive() {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));

    // Build a callee larger than 24 non-nops: 30 const+iadd chain
    let mut big = Function::new("big".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut big);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let mut cur = b.emit_const_i64(0);
        for i in 1..31 {
            let k = b.emit_const_i64(i);
            cur = b
                .emit(
                    Op::IAdd,
                    vec![cur, k],
                    None,
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .unwrap();
        }
        b.terminate(Terminator::Return { value: Some(cur) });
    }
    module.add_function(big);

    let mut caller = Function::new("c".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut caller);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let data = module.data.intern_function_name("big");
        let r = b
            .emit(
                Op::Call,
                vec![],
                Some(Immediate::Data(data)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        b.terminate(Terminator::Return { value: Some(r) });
    }
    module.add_function(caller);

    let changed = inline_small_functions(&mut module);
    // big >24 non-nop, should not inline
    assert!(!changed, "oversized callee must not be inlined");

    // Still has the Call
    let c = module.functions.iter().find(|f| f.name == "c").unwrap();
    assert!(c.instructions.iter().any(|i| i.op == Op::Call));
}

#[test]
fn inliner_skips_recursive_and_generator_and_try() {
    // Recursive: callee that calls self
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));
    let mut rec = Function::new("rec".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut rec);
        let entry = b.create_named_block("entry", vec![(IrType::I64, PhpType::Int)]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let p = b.block_param(entry, 0);
        let data = module.data.intern_function_name("rec");
        let r = b
            .emit(
                Op::Call,
                vec![p],
                Some(Immediate::Data(data)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        b.terminate(Terminator::Return { value: Some(r) });
    }
    module.add_function(rec);

    let mut c = Function::new("c".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut c);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let data = module.data.intern_function_name("rec");
        let one = b.emit_const_i64(1);
        let r = b
            .emit(
                Op::Call,
                vec![one],
                Some(Immediate::Data(data)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        b.terminate(Terminator::Return { value: Some(r) });
    }
    module.add_function(c);

    let changed = inline_small_functions(&mut module);
    assert!(!changed, "recursive must be refused");

    // Generator flag
    let mut g = Function::new("g".to_string(), IrType::I64, PhpType::Int);
    g.flags.is_generator = true;
    // minimal body
    {
        let mut b = Builder::new(&mut g);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let v = b.emit_const_i64(9);
        b.terminate(Terminator::Return { value: Some(v) });
    }
    module.add_function(g);

    let mut c2 = Function::new("c2".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut c2);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let data = module.data.intern_function_name("g");
        let r = b
            .emit(
                Op::Call,
                vec![],
                Some(Immediate::Data(data)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        b.terminate(Terminator::Return { value: Some(r) });
    }
    module.add_function(c2);

    let changed2 = inline_small_functions(&mut module);
    assert!(!changed2, "generator callee must be refused");

    // Real try/catch: callee with TryPushHandler (has_exception_handlers must catch it)
    let mut t = Function::new("has_try".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut t);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        // Emit a try handler op (any of the set makes has_exception_handlers true)
        b.emit(
            Op::TryPushHandler,
            vec![],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let seven = b.emit_const_i64(7);
        b.terminate(Terminator::Return { value: Some(seven) });
    }
    module.add_function(t);

    let mut c3 = Function::new("c3".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut c3);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let data = module.data.intern_function_name("has_try");
        let r = b
            .emit(
                Op::Call,
                vec![],
                Some(Immediate::Data(data)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        b.terminate(Terminator::Return { value: Some(r) });
    }
    module.add_function(c3);

    let changed3 = inline_small_functions(&mut module);
    assert!(!changed3, "try/catch callee must be refused by has_exception_handlers");
    let c3f = module.functions.iter().find(|f| f.name == "c3").unwrap();
    assert!(c3f.instructions.iter().any(|i| i.op == Op::Call), "Call to has_try site must remain");
}

#[test]
fn inliner_handles_multi_block_callee() {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));

    // callee: multi-block, returns 42 (entry 0-param per rule, internal choice)
    let mut mb = Function::new("mb".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut mb);
        let entry = b.create_named_block("entry", vec![]);
        let thenb = b.create_named_block("then", vec![]);
        let elseb = b.create_named_block("else", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        // always go then for test simplicity
        b.terminate(Terminator::Br {
            target: thenb,
            args: vec![],
        });
        b.position_at_end(thenb);
        let v42 = b.emit_const_i64(42);
        b.terminate(Terminator::Return { value: Some(v42) });
        b.position_at_end(elseb);
        let v7 = b.emit_const_i64(7);
        b.terminate(Terminator::Return { value: Some(v7) });
    }
    module.add_function(mb);

    let mut host = Function::new("h".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut host);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let data = module.data.intern_function_name("mb");
        let r = b
            .emit(
                Op::Call,
                vec![],
                Some(Immediate::Data(data)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        b.terminate(Terminator::Return { value: Some(r) });
    }
    module.add_function(host);

    let changed = inline_small_functions(&mut module);
    assert!(changed);
    let h = module.functions.iter().find(|f| f.name == "h").unwrap();
    // After inlining we expect the constants 42/7 present and no Call
    assert!(!h.instructions.iter().any(|i| i.op == Op::Call));
    assert!(h.instructions.iter().any(|i| matches!(i.immediate, Some(Immediate::I64(42))) || matches!(i.immediate, Some(Immediate::I64(7)))));
    assert!(validate_module(&module).is_ok());
}

/// Structural test: FVC call-site name resolution must use the canonical
/// `resolve_variant_callee_name` over `module.functions`, and inlining at that
/// site must remove the `FunctionVariantCall` opcode (using the shipped search
/// path, whose snapshot resolver mirrors the same canonical logic).
#[test]
fn resolve_call_target_is_canonical_and_inliner_removes_fvc() {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));
    let ft = make_fortytwo_callee();
    module.add_function(ft);
    let _ = module.data.intern_function_name("fortytwo");
    let _ = module.data.intern_string("fgroup:fortytwo");

    // Manually construct host with FVC immediate (as would appear for variant dispatch).
    let mut host = Function::new("h_fvc2".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut host);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let r = b.emit(
            Op::FunctionVariantCall,
            vec![],
            Some(Immediate::FunctionVariantRef { group: 0, variant: 0 }),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        ).unwrap();
        b.terminate(Terminator::Return { value: Some(r) });
    }
    module.add_function(host);

    // Canonical FVC name resolution must yield the concrete variant callee.
    let via_direct = crate::ir::function_variants::resolve_variant_callee_name(&module, 0, 0);
    assert_eq!(via_direct.as_deref(), Some("fortytwo"));

    // Inliner (whose snapshot resolver mirrors the canonical name) must remove the FVC site.
    let changed = inline_small_functions(&mut module);
    assert!(changed, "FVC site to small eligible callee must inline via canonical name");
    let hf = module.functions.iter().find(|f| f.name == "h_fvc2").unwrap();
    assert!(!hf.instructions.iter().any(|i| matches!(i.op, Op::Call | Op::FunctionVariantCall)),
            "FVC opcode must be gone after inlining the canonical variant callee");
    assert!(validate_module(&module).is_ok());
}

/// Builds a small, scalar, 0-param function whose body is a single `Call` (by Data
/// name) to `callee_name` and returns its result. Used to wire mutual-recursion cycles.
fn make_caller_of(module: &mut Module, name: &str, callee_name: &str) -> Function {
    let mut f = Function::new(name.to_string(), IrType::I64, PhpType::Int);
    let data_id = module.data.intern_function_name(callee_name);
    let mut b = Builder::new(&mut f);
    let entry = b.create_named_block("entry", vec![]);
    b.set_entry(entry);
    b.position_at_end(entry);
    let r = b
        .emit(
            Op::Call,
            vec![],
            Some(Immediate::Data(data_id)),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .unwrap();
    b.terminate(Terminator::Return { value: Some(r) });
    f
}

/// Regression for the mutual-recursion hang: two small functions that call each other
/// must be detected as recursive by the call-graph cycle analysis, so neither is
/// inlined and the pass terminates (reaching this assertion at all proves no hang).
#[test]
fn inliner_does_not_inline_or_hang_on_mutual_recursion() {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));
    let f = make_caller_of(&mut module, "mr_f", "mr_g");
    module.add_function(f);
    let g = make_caller_of(&mut module, "mr_g", "mr_f");
    module.add_function(g);

    // Must return (no infinite expansion) and must not inline either mutual callee.
    let changed = inline_small_functions(&mut module);
    assert!(!changed, "mutually recursive small functions must not be inlined");
    for name in ["mr_f", "mr_g"] {
        let func = module.functions.iter().find(|f| f.name == name).unwrap();
        assert!(
            func.instructions.iter().any(|i| i.op == Op::Call),
            "call in {} must remain (recursive cycle excluded from inlining)",
            name
        );
    }
    assert!(validate_module(&module).is_ok());
}

/// By-ref parameters need the caller's storage; the value-store splice cannot model
/// that, so such callees must be refused.
#[test]
fn inliner_skips_by_ref_param_callee() {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));

    // Callee `inc` with a by-ref int param; trivial scalar body (0-param entry).
    let mut inc = Function::new("inc".to_string(), IrType::Void, PhpType::Void);
    inc.params.push(FunctionParam {
        name: "x".to_string(),
        ir_type: IrType::I64,
        php_type: PhpType::Int,
        by_ref: true,
        variadic: false,
    });
    {
        let mut b = Builder::new(&mut inc);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        b.terminate(Terminator::Return { value: None });
    }
    module.add_function(inc);

    let mut caller = Function::new("c_byref".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut caller);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let data = module.data.intern_function_name("inc");
        let arg = b.emit_const_i64(1);
        b.emit(
            Op::Call,
            vec![arg],
            Some(Immediate::Data(data)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let seven = b.emit_const_i64(7);
        b.terminate(Terminator::Return { value: Some(seven) });
    }
    module.add_function(caller);

    let changed = inline_small_functions(&mut module);
    assert!(!changed, "by-ref param callee must not be inlined");
    let c = module.functions.iter().find(|f| f.name == "c_byref").unwrap();
    assert!(c.instructions.iter().any(|i| i.op == Op::Call));
}

/// A callee owning an object-typed local can run a `__destruct` when the local is
/// released; the splice defers that release to the host epilogue, changing observable
/// destructor timing, so object-holding callees must be refused.
#[test]
fn inliner_skips_object_local_callee() {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));

    // Callee with an Object local (destructor-capable) but a plain I64 return.
    let mut f = Function::new("has_obj_local".to_string(), IrType::I64, PhpType::Int);
    f.add_local(
        Some("o".to_string()),
        IrType::Heap(IrHeapKind::Object),
        PhpType::Object("Foo".to_string()),
        LocalKind::PhpLocal,
    );
    {
        let mut b = Builder::new(&mut f);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let v = b.emit_const_i64(42);
        b.terminate(Terminator::Return { value: Some(v) });
    }
    module.add_function(f);

    let mut caller = Function::new("c_objlocal".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut caller);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let data = module.data.intern_function_name("has_obj_local");
        let r = b
            .emit(
                Op::Call,
                vec![],
                Some(Immediate::Data(data)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        b.terminate(Terminator::Return { value: Some(r) });
    }
    module.add_function(caller);

    let changed = inline_small_functions(&mut module);
    assert!(!changed, "callee owning an object local must not be inlined");
    let c = module.functions.iter().find(|f| f.name == "c_objlocal").unwrap();
    assert!(c.instructions.iter().any(|i| i.op == Op::Call));
}

/// A callee returning an object transfers a destructor-capable value across the boundary;
/// it must be refused (destructor timing is observable, unlike destructor-free returns).
#[test]
fn inliner_skips_object_return_callee() {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));

    // Object-returning callee; eligibility refuses it before any transform, so a minimal
    // (untransformed) body is fine for this check.
    let mut f = Function::new(
        "ret_obj".to_string(),
        IrType::Heap(IrHeapKind::Object),
        PhpType::Object("Foo".to_string()),
    );
    {
        let mut b = Builder::new(&mut f);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        b.terminate(Terminator::Unreachable);
    }
    module.add_function(f);

    let mut caller = Function::new("c_retobj".to_string(), IrType::I64, PhpType::Int);
    {
        let mut b = Builder::new(&mut caller);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let data = module.data.intern_function_name("ret_obj");
        b.emit(
            Op::Call,
            vec![],
            Some(Immediate::Data(data)),
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object("Foo".to_string()),
            Ownership::Owned,
        );
        let seven = b.emit_const_i64(7);
        b.terminate(Terminator::Return { value: Some(seven) });
    }
    module.add_function(caller);

    let changed = inline_small_functions(&mut module);
    assert!(!changed, "callee returning an object must not be inlined");
    let c = module.functions.iter().find(|f| f.name == "c_retobj").unwrap();
    assert!(c.instructions.iter().any(|i| i.op == Op::Call));
}

/// A small string helper (destructor-free refcounted boundary) IS inlined: the string
/// param slot is directly returned, so it is transplanted as a cleanup-excluded slot and
/// the result flows through the continuation parameter. Validates IR stays well-formed.
#[test]
fn inliner_inlines_destructor_free_string_helper() {
    let mut module = Module::new(Target::new(Platform::MacOS, Arch::AArch64));

    // `id(string $s): string { return $s; }` — param in slot[0], returned directly.
    let mut f = Function::new("id_str".to_string(), IrType::Str, PhpType::Str);
    f.params.push(FunctionParam {
        name: "s".to_string(),
        ir_type: IrType::Str,
        php_type: PhpType::Str,
        by_ref: false,
        variadic: false,
    });
    f.add_local(Some("s".to_string()), IrType::Str, PhpType::Str, LocalKind::PhpLocal);
    {
        let mut b = Builder::new(&mut f);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let loaded = b
            .emit(
                Op::LoadLocal,
                vec![],
                Some(Immediate::LocalSlot(crate::ir::LocalSlotId::from_raw(0))),
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .unwrap();
        b.terminate(Terminator::Return { value: Some(loaded) });
    }
    module.add_function(f);

    let mut caller = Function::new("c_idstr".to_string(), IrType::Str, PhpType::Str);
    {
        let mut b = Builder::new(&mut caller);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let arg_data = module.data.intern_string("hi");
        let arg = b.emit_const_str(arg_data);
        let data = module.data.intern_function_name("id_str");
        let r = b
            .emit(
                Op::Call,
                vec![arg],
                Some(Immediate::Data(data)),
                IrType::Str,
                PhpType::Str,
                Ownership::Owned,
            )
            .unwrap();
        b.terminate(Terminator::Return { value: Some(r) });
    }
    module.add_function(caller);

    let changed = inline_small_functions(&mut module);
    assert!(changed, "destructor-free string helper must be inlined");
    let c = module.functions.iter().find(|f| f.name == "c_idstr").unwrap();
    assert!(
        !c.instructions.iter().any(|i| i.op == Op::Call),
        "string helper call must be gone after inlining"
    );
    assert!(validate_module(&module).is_ok());
}
