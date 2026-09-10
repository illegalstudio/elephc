//! Purpose:
//! Forwards callable arguments to the shared mbstring invocation coordinator.
//!
//! Called from:
//! - The descriptor invoker after classifying its normalized argument container.
//!
//! Key details:
//! - Borrowed cells preserve actual arity, omitted defaults, and original PHP types.
//! - The shared coordinator owns conversion and returns a single owned Mixed result.

use super::*;
use elephc_builtin_contract::RuntimeBuiltinId;

/// Borrows named values into stack cells and includes defaults only before the last supplied slot.
pub(super) fn emit_assoc(
    emitter: &mut Emitter, ctx: &mut InvokerEmitContext, data: &mut DataSection,
    source: LoadedArraySource, operation: RuntimeBuiltinId,
) {
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).expect("mbstring contract");
    let capacity = contract.params.len();
    let pointers = (capacity * 8 + 15) & !15;
    let count_slot = pointers + capacity * 24;
    let frame = (count_slot + 8 + 15) & !15;
    let target = emitter.target;
    let (hash, scratch) = match target.arch { Arch::AArch64 => ("x20", "x9"), Arch::X86_64 => ("r13", "r10") };
    emit_loaded_array_source_to_reg(source, hash, emitter);
    abi::emit_reserve_temporary_stack(emitter, frame);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    abi::emit_store_to_sp(emitter, scratch, count_slot);
    for (index, parameter) in contract.params.iter().enumerate() {
        let record = pointers + index * 24;
        let missing = ctx.next_label("mbstring_named_default");
        let ready = ctx.next_label("mbstring_named_value_ready");
        emit_hash_lookup_for_param_or_index(hash, Some(parameter.name), index, emitter, ctx, data);
        abi::emit_branch_if_int_result_zero(emitter, &missing);
        let (low, high, tag) = raw_hash_value_regs(emitter);
        abi::emit_store_to_sp(emitter, tag, record);
        abi::emit_store_to_sp(emitter, low, record + 8);
        abi::emit_store_to_sp(emitter, high, record + 16);
        abi::emit_load_int_immediate(emitter, scratch, index as i64 + 1);
        abi::emit_store_to_sp(emitter, scratch, count_slot);
        abi::emit_jump(emitter, &ready);
        emitter.label(&missing);
        stage_default(emitter, data, parameter.default, record);
        emitter.label(&ready);
        abi::emit_temporary_stack_address(emitter, scratch, record);
        abi::emit_store_to_sp(emitter, scratch, index * 8);
    }
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 0), operation.as_u32() as i64);
    abi::emit_temporary_stack_address(emitter, abi::int_arg_reg_name(target, 1), 0);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(target, 2), count_slot);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 3), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 4), 0);
    abi::emit_call_label(emitter, "__rt_mbstring_native");
    abi::emit_release_temporary_stack(emitter, frame);
    abi::emit_call_label(emitter, "__rt_mbstring_box_result");
}

/// Writes an immutable scalar default using neutral metadata without allocating temporary owners.
fn stage_default(
    emitter: &mut Emitter, data: &mut DataSection,
    default: Option<elephc_builtin_contract::DefaultSpec>, record: usize,
) {
    use elephc_builtin_contract::DefaultSpec;
    let (ty, low) = match default {
        None | Some(DefaultSpec::Null) => (PhpType::Void, 0),
        Some(DefaultSpec::Int(value)) => (PhpType::Int, value),
        Some(DefaultSpec::Bool(value)) => (PhpType::Bool, i64::from(value)),
        Some(DefaultSpec::Str(_)) => (PhpType::Str, 0),
        _ => unreachable!("mbstring contracts use scalar literal defaults"),
    };
    let scratch = match emitter.target.arch { Arch::AArch64 => "x9", Arch::X86_64 => "r10" };
    abi::emit_load_int_immediate(emitter, scratch, crate::codegen::runtime_value_tag(&ty) as i64);
    abi::emit_store_to_sp(emitter, scratch, record);
    if let Some(DefaultSpec::Str(value)) = default {
        let (label, length) = data.add_string(value.as_bytes());
        abi::emit_symbol_address(emitter, scratch, &label);
        abi::emit_store_to_sp(emitter, scratch, record + 8);
        abi::emit_load_int_immediate(emitter, scratch, length as i64);
    } else {
        abi::emit_load_int_immediate(emitter, scratch, low);
        abi::emit_store_to_sp(emitter, scratch, record + 8);
        abi::emit_load_int_immediate(emitter, scratch, 0);
    }
    abi::emit_store_to_sp(emitter, scratch, record + 16);
}

/// Stages aligned borrowed pointers without retaining arguments or copying an owned string return.
pub(super) fn emit_indexed(
    emitter: &mut Emitter, ctx: &mut InvokerEmitContext,
    source: LoadedArraySource, operation: RuntimeBuiltinId,
) {
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).expect("mbstring contract");
    let capacity = contract.max_args.unwrap_or(contract.params.len());
    let bytes = (capacity * 8 + 15) & !15;
    let target = emitter.target;
    let array_reg = match target.arch { Arch::AArch64 => "x9", Arch::X86_64 => "r10" };
    emit_loaded_array_source_to_reg(source, array_reg, emitter);
    abi::emit_reserve_temporary_stack(emitter, bytes);
    abi::emit_load_from_address(emitter, abi::int_arg_reg_name(target, 2), array_reg, 0);
    let ready = ctx.next_label("mbstring_pointers_ready");
    for index in 0..capacity {
        match target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp x2, #{index}"));              // preserve the actual argument count before bounded pointer staging
                emitter.instruction(&format!("b.ls {ready}"));                  // leave omitted arguments absent for the shared coordinator
                emitter.instruction(&format!("ldr x10, [x9, #{}]", 24 + index * 8)); // read one borrowed Mixed cell from the native array
                emitter.instruction(&format!("str x10, [sp, #{}]", index * 8)); // provide an aligned pointer array for the Rust boundary
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp rdx, {index}"));              // retain actual arity independently of the bounded staging area
                emitter.instruction(&format!("jbe {ready}"));                   // skip absent arguments without filling defaults
                emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {}]", 24 + index * 8)); // borrow the original cell without an extra owner
                emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", index * 8)); // align the pointer slice consumed by Rust
            }
        }
    }
    emitter.label(&ready);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 0), operation.as_u32() as i64);
    abi::emit_temporary_stack_address(emitter, abi::int_arg_reg_name(target, 1), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 3), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 4), 0);
    abi::emit_call_label(emitter, "__rt_mbstring_native");
    abi::emit_release_temporary_stack(emitter, bytes);
    abi::emit_call_label(emitter, "__rt_mbstring_box_result");
}
