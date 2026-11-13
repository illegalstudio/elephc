//! Purpose:
//! Tests runtime class-string subjects against AOT and eval inheritance metadata.
//!
//! Called from:
//! - The typed is_a/is_subclass_of builtin lowerer.
//!
//! Key details:
//! - The source string and optional runtime flag survive every comparison call.
//! - Candidate names include classes and interfaces, with PHP case-insensitive matching.
//! - A versioned query follows dynamic inheritance through the compiled positive candidates.

use super::*;

/// Emits a relation test for a dynamic string subject and a literal target.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, subject: ValueId,
    target: &str, exclude_self: bool,
) -> Result<()> {
    let flag = inst.operands.get(2).copied();
    let literal_flag = match flag {
        Some(value) => const_bool_operand(ctx, value)?,
        None => Some(exclude_self),
    };
    if literal_flag == Some(false) {
        emit_bool_result(ctx, false);
        return store_if_result(ctx, inst);
    }
    let target_key = php_symbol_key(target.trim_start_matches('\\'));
    let mut candidates = std::collections::BTreeSet::new();
    for name in ctx.module.class_infos.keys() {
        let same = php_symbol_key(name.trim_start_matches('\\')) == target_key;
        if (!exclude_self && same) || parent_chain_contains(ctx, name, &target_key)
            || class_interfaces_contain(ctx, name, &target_key)
        { candidates.insert(name.trim_start_matches('\\').to_string()); }
    }
    for name in ctx.module.interface_infos.keys() {
        let same = php_symbol_key(name.trim_start_matches('\\')) == target_key;
        let mut parents = Vec::new();
        super::super::class_relations::collect_interface_parents(ctx, name, &mut parents);
        if (!exclude_self && same) || parents.iter().any(|name|
            php_symbol_key(name.trim_start_matches('\\')) == target_key)
        { candidates.insert(name.trim_start_matches('\\').to_string()); }
    }
    let matched = ctx.next_label("class_string_relation_match");
    let done = ctx.next_label("class_string_relation_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(subject, ptr, len)?;
    abi::emit_store_to_sp(ctx.emitter, ptr, 0);
    abi::emit_store_to_sp(ctx.emitter, len, 8);
    if literal_flag.is_none() {
        predicates::emit_value_truthiness(ctx, flag.expect("runtime flag exists"), "is_a")?;
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    }
    for candidate in &candidates {
        emit_match(ctx, candidate, &matched);
        emit_match(ctx, &format!("\\{candidate}"), &matched);
    }
    if ctx.module.required_runtime_features.eval_bridge {
        let mut related = vec![target.trim_start_matches('\\').to_string()];
        // Dynamic classes can inherit a compiled class or interface related to the target.
        // Include the target itself even for is_subclass_of; the ABI excludes only exact self.
        related.extend(candidates);
        let related = related.join("\0");
        let (label, len) = ctx.data.add_string(related.as_bytes());
        for index in 0..2 {
            abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, index), index * 8);
        }
        abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), &label);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), len as i64);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 4), i64::from(exclude_self));
        let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_class_string_relation_v1");
        abi::emit_call_label(ctx.emitter, &symbol);
        if literal_flag.is_none() {
            abi::emit_branch_if_int_result_nonzero(ctx.emitter, &matched);
        }
    } else {
        emit_bool_result(ctx, false);
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&matched);
    if literal_flag.is_none() {
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    } else { emit_bool_result(ctx, true); }
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    store_if_result(ctx, inst)
}

/// Compares one saved class string using the target's string-comparison ABI.
fn emit_match(ctx: &mut FunctionContext<'_>, candidate: &str, matched: &str) {
    let (label, len) = ctx.data.add_string(candidate.as_bytes());
    let regs = match ctx.emitter.target.arch {
        Arch::AArch64 => ["x1", "x2", "x3", "x4"],
        Arch::X86_64 => ["rdi", "rsi", "rdx", "rcx"],
    };
    abi::emit_load_temporary_stack_slot(ctx.emitter, regs[0], 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, regs[1], 8);
    abi::emit_symbol_address(ctx.emitter, regs[2], &label);
    abi::emit_load_int_immediate(ctx.emitter, regs[3], len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test the PHP class-name comparison result
            ctx.emitter.instruction(&format!("b.eq {matched}"));                // accept this related class or interface
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test the PHP class-name comparison result
            ctx.emitter.instruction(&format!("je {matched}"));                  // accept this related class or interface
        }
    }
}
