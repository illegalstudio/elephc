//! Purpose:
//! Passes concrete AOT values to the shared mbstring invocation coordinator.
//!
//! Called from:
//! - Typed mbstring RuntimeFnId dispatch and generated callable wrappers.
//!
//! Key details:
//! - Stack-local invoker descriptors borrow raw SSA values without allocating argument owners.
//! - The coordinator copies values before coercion and owns all conversion/snapshot cleanup.
//! - Physical-source strictness reaches direct calls; callable wrapper inputs retain their existing validation.

use super::*;
use elephc_builtin_contract::{RuntimeBuiltinId, TypeSpec};

mod capture;

/// Stages borrowed value descriptors and invokes shared PHP argument preparation on either architecture.
pub(crate) fn lower_mbstring(ctx: &mut FunctionContext<'_>, inst: &Instruction, operation: RuntimeBuiltinId) -> Result<()> {
    let capture = matches!(operation, RuntimeBuiltinId::MbEreg | RuntimeBuiltinId::MbEregi);
    let query = operation == RuntimeBuiltinId::MbParseStr;
    let output = if query { Some(1) } else if capture { Some(2) } else { None };
    if matches!(inst.immediate, Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ProfiledFunction {
        arguments: crate::ir::RuntimeArgumentLayout::IndexedArray, ..
    }))) {
        if output.is_some() { return Err(CodegenIrError::unsupported("dynamic spread mbstring output references are not yet supported")); }
        return lower_packed(ctx, inst, operation);
    }
    if !operation.supports_arity(inst.operands.len()) {
        return Err(CodegenIrError::invalid_module("invalid mbstring argument count"));
    }
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).expect("mbstring contract");
    let pointers_size = (inst.operands.len() * 8 + 15) & !15;
    let state_offset = pointers_size + inst.operands.len() * 48;
    let size = state_offset + if query { crate::codegen_support::mbstring_query::STATE_BYTES }
        else if capture { 16 } else { 0 };
    let strict = match inst.immediate {
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ProfiledFunction { strict_types: Some(strict), .. })) => strict,
        // Callable wrapper parameters are currently validated by their descriptor invoker.
        // Propagating the invoking source profile remains necessary when that validation moves here.
        _ => false,
    };
    abi::emit_reserve_temporary_stack(ctx.emitter, size);
    for (index, &value) in inst.operands.iter().enumerate() {
        if output == Some(index) {
            capture::stage_reference(ctx, value, index * 8)?;
            continue;
        }
        let record = pointers_size + index * 48;
        let actual = ctx.load_value_to_result(value)?.codegen_repr();
        match ctx.emitter.target.arch {
            Arch::AArch64 => stage_aarch64(ctx, &actual, record, index * 8),
            Arch::X86_64 => stage_x86_64(ctx, &actual, record, index * 8),
        }

    }
    super::load_optional_sprintf_eval_context(ctx, 4)?;
    let target = ctx.emitter.target;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 0), operation.as_u32() as i64);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(target, 1), 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 2), inst.operands.len() as i64);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 3), i64::from(strict));
    if query {
        crate::codegen_support::mbstring_query::stage(ctx.emitter, state_offset);
        abi::emit_call_label(ctx.emitter, "__rt_mbstring_query_native");
    } else if capture {
        capture::stage_state(ctx, state_offset);
        abi::emit_call_label(ctx.emitter, "__rt_mbstring_capture_native");
    } else {
        abi::emit_call_label(ctx.emitter, "__rt_mbstring_native");
    }
    abi::emit_release_temporary_stack(ctx.emitter, size);
    finish_result(ctx, inst, contract.returns)
}

/// Adapts the shared successful result into the PHP type declared by the neutral contract.
fn finish_result(ctx: &mut FunctionContext<'_>, inst: &Instruction, returns: TypeSpec) -> Result<()> {
    let target = ctx.emitter.target;
    if returns == TypeSpec::Str {
        match target.arch {
            Arch::AArch64 => ctx.emitter.instruction("mov x1, x0"),             // adapt the owned native string pointer to the PHP string-result pair
            Arch::X86_64 => ctx.emitter.instruction("mov rdx, rcx"),            // adapt the returned native byte length to the PHP string-result pair
        }
    }
    if matches!(returns, TypeSpec::Mixed | TypeSpec::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mbstring_box_result");
    }
    store_if_result(ctx, inst)
}

/// Invokes the same coordinator over an owned Mixed array without padding or truncating its length.
fn lower_packed(ctx: &mut FunctionContext<'_>, inst: &Instruction, operation: RuntimeBuiltinId) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    if ctx.value_php_type(array)? != PhpType::Array(Box::new(PhpType::Mixed)) {
        return Err(CodegenIrError::invalid_module("packed mbstring arguments require array<mixed>"));
    }
    let strict = matches!(inst.immediate, Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ProfiledFunction {
        strict_types: Some(true), ..
    })));
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).expect("mbstring contract");
    let capacity = contract.max_args.unwrap_or(contract.params.len());
    let size = (capacity * 8 + 15) & !15;
    abi::emit_reserve_temporary_stack(ctx.emitter, size);
    super::load_optional_sprintf_eval_context(ctx, 4)?;
    let target = ctx.emitter.target;
    let scratch = match target.arch { Arch::AArch64 => "x9", Arch::X86_64 => "r10" };
    ctx.load_value_to_reg(array, scratch)?;
    abi::emit_load_from_address(ctx.emitter, abi::int_arg_reg_name(target, 2), scratch, 0);
    // Native heap arrays can be byte-aligned. Rust's pointer slice requires aligned storage.
    // Stage only the bounded accepted argument prefix; the coordinator retains actual arity.
    let ready = ctx.next_label("mbstring_packed_arguments_ready");
    for index in 0..capacity {
        match target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp x2, #{index}"));          // compare the actual argument count with the next pointer slot
                ctx.emitter.instruction(&format!("b.ls {ready}"));              // leave absent arguments unread for shared arity validation
                ctx.emitter.instruction(&format!("ldr x10, [x9, #{}]", 24 + index * 8)); // read the native array's potentially unaligned pointer payload
                ctx.emitter.instruction(&format!("str x10, [sp, #{}]", index * 8)); // copy the borrowed cell pointer into aligned C argument storage
            },
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp rdx, {index}"));          // preserve actual arity independently of the fixed staging capacity
                ctx.emitter.instruction(&format!("jbe {ready}"));               // skip absent pointer slots without padding PHP arguments
                ctx.emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {}]", 24 + index * 8)); // read one possibly unaligned native array element
                ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", index * 8)); // provide Rust with an aligned pointer slice
            },
        }
    }
    ctx.emitter.label(&ready);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(target, 1), 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 0), operation.as_u32() as i64);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 3), i64::from(strict));
    abi::emit_call_label(ctx.emitter, "__rt_mbstring_native");
    abi::emit_release_temporary_stack(ctx.emitter, size);
    finish_result(ctx, inst, contract.returns)
}

/// Stores an AArch64 raw value and a borrowed invoker marker with its concrete runtime tag.
fn stage_aarch64(ctx: &mut FunctionContext<'_>, actual: &PhpType, record: usize, pointer: usize) {
    let raw = record + 24;
    match actual {
        PhpType::Str => {
            ctx.emitter.instruction(&format!("stp x1, x2, [sp, #{raw}]"));      // retain binary string pointer/length without coercion or allocation
        }
        PhpType::Float => {
            ctx.emitter.instruction("fmov x0, d0");                             // preserve all float bits before source-type dispatch
            ctx.emitter.instruction(&format!("stp x0, xzr, [sp, #{raw}]"));     // borrow the raw float bits with no high payload
        }
        _ => ctx.emitter.instruction(&format!("stp x0, xzr, [sp, #{raw}]")),    // borrow the scalar, object, array, or boxed value without transferring ownership
    }
    if *actual == PhpType::TaggedScalar {
        ctx.emitter.instruction("mov x9, x1");                                  // keep the nullable scalar's dynamic tag separate from its payload
    } else if *actual == PhpType::Iterable {
        abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
        ctx.emitter.instruction("add x9, x0, #2");                              // map indexed/hash/object heap kinds to concrete PHP tags
        ctx.emitter.instruction("sub x10, x0, #2");                             // prepare an unsigned range check for the three valid iterable kinds
        ctx.emitter.instruction("cmp x10, #2");                                 // reject unknown or null iterable payload kinds
        ctx.emitter.instruction("mov x10, #8");                                 // prepare the existing boxed-iterable null fallback
        ctx.emitter.instruction("csel x9, x9, x10, ls");                        // keep the concrete kind only for a valid heap-backed iterable
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "x9", crate::codegen::runtime_value_tag(actual) as i64);
    }
    ctx.emitter.instruction("mov x10, #11");                                    // describe borrowed raw storage using the shared invoker marker ABI
    ctx.emitter.instruction(&format!("add x11, sp, #{raw}"));                   // point the marker at this call's stable raw-value storage
    ctx.emitter.instruction(&format!("stp x10, x11, [sp, #{record}]"));         // stage the marker tag and borrowed storage address
    ctx.emitter.instruction(&format!("str x9, [sp, #{}]", record + 16));        // preserve the source tag for coordinator-owned value copying
    ctx.emitter.instruction(&format!("add x10, sp, #{record}"));                // point at the completed borrowed marker
    ctx.emitter.instruction(&format!("str x10, [sp, #{pointer}]"));             // populate the source-order argument-pointer array
}

/// Stores a SysV raw value and the identical invoker marker layout without temporary heap owners.
fn stage_x86_64(ctx: &mut FunctionContext<'_>, actual: &PhpType, record: usize, pointer: usize) {
    let raw = record + 24;
    if *actual == PhpType::Float {
        ctx.emitter.instruction("movq rax, xmm0");                              // preserve all floating-point bits for the shared coercion planner
    }
    ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {raw}], rax"));      // borrow the raw low payload while retaining its original type
    if *actual == PhpType::Str {
        ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", raw + 8)); // preserve binary string length without converting the value
    } else {
        ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", raw + 8)); // scalar and heap-pointer values do not carry a high payload
    }
    if *actual == PhpType::TaggedScalar {
        ctx.emitter.instruction("mov r10, rdx");                                // retain the nullable scalar's runtime tag before any helper call
    } else if *actual == PhpType::Iterable {
        abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
        ctx.emitter.instruction("lea r10, [rax + 2]");                          // map indexed/hash/object heap kinds to their concrete PHP tags
        ctx.emitter.instruction("sub rax, 2");                                  // prepare an unsigned range check covering exactly the valid iterable kinds
        ctx.emitter.instruction("cmp rax, 2");                                  // recognize null or unsupported payload kinds
        ctx.emitter.instruction("mov r11d, 8");                                 // prepare the existing boxed-iterable null fallback
        ctx.emitter.instruction("cmova r10, r11");                              // preserve a concrete tag only for valid heap-backed iterables
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "r10", crate::codegen::runtime_value_tag(actual) as i64);
    }
    ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {record}], 11"));    // use the shared marker for borrowed raw-value storage
    ctx.emitter.instruction(&format!("lea r11, [rsp + {raw}]"));                // address this call's stable raw-value slot
    ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", record + 8)); // publish borrowed storage for coordinator-owned copying
    ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", record + 16)); // publish the authoritative source value tag
    ctx.emitter.instruction(&format!("lea r11, [rsp + {record}]"));             // address the completed argument marker
    ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {pointer}], r11"));  // populate the argument-pointer array without allocating boxes
}
