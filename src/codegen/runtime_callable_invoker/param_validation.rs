//! Purpose:
//! Validates descriptor arguments at each declared PHP parameter's binding point.
//!
//! Called from:
//! - The indexed and associative callback invokers in `super`.
//!
//! Key details:
//! - Validation borrows the current argument before conversion/acquisition; pending owners
//!   from preceding parameters are released if a later parameter fails.
//! - Numeric strings use PHP's bounded numeric scanner; invalid values throw catchable TypeError.
//! - ABI v4 carries lexical scope and physical invocation strictness as independent inputs.

use super::*;

mod numeric_bounds;
mod stringable;

/// Resolves the declared PHP contract for a visible parameter, leaving undeclared slots unvalidated.
fn contract(sig: &FunctionSig, index: usize) -> Option<PhpType> {
    sig.param_type_exprs.get(index).and_then(Option::as_ref).map(type_expr_to_php_type)
}

/// Validates one source value at its PHP-observable binding point.
pub(super) fn validate_parameter_result(
    emitter: &mut Emitter, data: &mut DataSection, ctx: &mut InvokerEmitContext,
    sig: &FunctionSig, index: usize, source: &PhpType,
) {
    if let Some(expected) = contract(sig, index) {
        validate_and_restore_result(emitter, data, ctx, source, &expected, &sig.params[index].0);
    }
}

/// Preserves the borrowed raw source across tag/metadata validation.
pub(super) fn validate_and_restore_result(
    emitter: &mut Emitter, data: &mut DataSection, ctx: &mut InvokerEmitContext,
    source: &PhpType, expected: &PhpType, name: &str,
) {
    if *expected == PhpType::Mixed { return; }
    abi::emit_push_result_value(emitter, source);
    validate_result(emitter, data, ctx, source, expected, name);
    match source.codegen_repr() {
        PhpType::Str => {
            let (lo, hi) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, lo, hi);
        }
        PhpType::Float => abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter)),
        PhpType::TaggedScalar => {
            let tag = crate::codegen_support::sentinels::tagged_scalar_tag_reg(emitter);
            abi::emit_pop_reg_pair(emitter, abi::int_result_reg(emitter), tag);
        }
        PhpType::Void | PhpType::Never => {}
        _ => abi::emit_pop_reg(emitter, abi::int_result_reg(emitter)),
    }
}

/// Validates the already-found associative argument without changing its lookup tuple.
pub(super) fn validate_parameter_hash_result(
    emitter: &mut Emitter, data: &mut DataSection, ctx: &mut InvokerEmitContext,
    sig: &FunctionSig, index: usize,
) {
    let Some(expected) = contract(sig, index) else { return; };
    let (lo, hi, tag) = raw_hash_value_regs(emitter);
    let result = abi::int_result_reg(emitter);
    let stack = if emitter.target.arch == Arch::AArch64 { "sp" } else { "rsp" };
    abi::emit_reserve_temporary_stack(emitter, 32);
    for (register, offset) in [(lo, 0), (hi, 8), (tag, 16), (result, 24)] {
        abi::emit_store_to_address(emitter, register, stack, offset);
    }
    validate_raw(emitter, data, ctx, tag, lo, hi, &expected, &sig.params[index].0);
    for (register, offset) in [(lo, 0), (hi, 8), (tag, 16), (result, 24)] {
        abi::emit_load_temporary_stack_slot(emitter, register, offset);
    }
    abi::emit_release_temporary_stack(emitter, 32);
}

/// Applies ordinary physical-invocation binding policy to the current borrowed result registers.
fn validate_result(
    emitter: &mut Emitter, data: &mut DataSection, ctx: &mut InvokerEmitContext,
    source: &PhpType, expected: &PhpType, name: &str,
) {
    validate_result_with_policy(emitter, data, ctx, source, expected, name, false, false);
}

/// Checks a string parameter with explicit strictness before conversion acquires any argument owner.
pub(super) fn validate_string_argument(
    emitter: &mut Emitter, data: &mut DataSection, ctx: &mut InvokerEmitContext,
    source: &PhpType, name: &str, strict: bool,
) {
    validate_result_with_policy(emitter, data, ctx, source, &PhpType::Str, name, strict, false);
}

/// Emits the string return-binding acceptance predicate instead of throwing on a type mismatch.
pub(super) fn string_binding_predicate(
    emitter: &mut Emitter, data: &mut DataSection, ctx: &mut InvokerEmitContext,
    source: &PhpType, strict: bool,
) {
    validate_result_with_policy(emitter, data, ctx, source, &PhpType::Str, "return", strict, true);
}

/// Normalizes native scalar/string registers into a borrowed tag/payload validation tuple.
fn validate_result_with_policy(
    emitter: &mut Emitter, data: &mut DataSection, ctx: &mut InvokerEmitContext,
    source: &PhpType, expected: &PhpType, name: &str, strict_string: bool, predicate: bool,
) {
    let (lo, hi) = abi::string_result_regs(emitter);
    let result = abi::int_result_reg(emitter);
    let tag = abi::secondary_scratch_reg(emitter);
    let source = source.codegen_repr();
    if source == PhpType::Float {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("fmov x0, d0"),                // preserve the floating argument bits for borrowed validation
            Arch::X86_64 => emitter.instruction("movq rax, xmm0"),              // preserve the floating argument bits for borrowed validation
        }
    }
    if source == PhpType::TaggedScalar {
        abi::emit_reg_move(emitter, tag, crate::codegen_support::sentinels::tagged_scalar_tag_reg(emitter));
    } else {
        abi::emit_load_int_immediate(emitter, tag, crate::codegen::runtime_value_tag(&source) as i64);
    }
    let low = if source == PhpType::Str { lo } else { result };
    validate_raw_with_policy(emitter, data, ctx, tag, low, hi, expected, name, strict_string, predicate);
}

/// Validates an associative tag/payload tuple using ordinary invocation-site binding rules.
fn validate_raw(
    emitter: &mut Emitter, data: &mut DataSection, ctx: &mut InvokerEmitContext,
    tag: &str, lo: &str, hi: &str, expected: &PhpType, name: &str,
) {
    validate_raw_with_policy(emitter, data, ctx, tag, lo, hi, expected, name, false, false);
}

/// Peels Mixed/reference markers, then emits policy-dependent acceptance or a catchable TypeError.
fn validate_raw_with_policy(
    emitter: &mut Emitter, data: &mut DataSection, ctx: &mut InvokerEmitContext,
    tag: &str, lo: &str, hi: &str, expected: &PhpType, name: &str, strict_string: bool, predicate: bool,
) {
    if *expected == PhpType::Mixed { return }
    let stack = if emitter.target.arch == Arch::AArch64 { "sp" } else { "rsp" };
    abi::emit_reserve_temporary_stack(emitter, 32);
    abi::emit_store_to_address(emitter, tag, stack, 0);
    abi::emit_store_to_address(emitter, lo, stack, 8);
    abi::emit_store_to_address(emitter, hi, stack, 16);
    let result = abi::int_result_reg(emitter);
    abi::emit_reg_move(emitter, result, stack);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    let (payload, high) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rdx") };
    let ordinary = ctx.next_label("parameter_not_ref_marker");
    let marker = ctx.next_label("parameter_ref_marker");
    branch_equal(emitter, result, INVOKER_ARG_REF_CELL_TAG, &marker);
    abi::emit_jump(emitter, &ordinary);
    emitter.label(&marker);
    abi::emit_reg_move(emitter, result, high);
    abi::emit_load_from_address(emitter, high, payload, 8);
    abi::emit_load_from_address(emitter, payload, payload, 0);
    emitter.label(&ordinary);
    abi::emit_store_to_address(emitter, result, stack, 0);
    abi::emit_store_to_address(emitter, payload, stack, 8);
    abi::emit_store_to_address(emitter, high, stack, 16);
    // A reference to Mixed stores a boxed cell in the referenced low word. Peel that cell too.
    abi::emit_reg_move(emitter, result, stack);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::emit_store_to_address(emitter, result, stack, 0);
    abi::emit_store_to_address(emitter, payload, stack, 8);
    abi::emit_store_to_address(emitter, high, stack, 16);
    let valid = ctx.next_label("parameter_valid");
    let invalid = ctx.next_label("parameter_invalid");
    let done = ctx.next_label("parameter_validation_done");
    if strict_string {
        abi::emit_load_temporary_stack_slot(emitter, result, 0);
        branch_equal(emitter, result, 1, &valid);
    } else if ctx.has_binding_policy && !predicate {
        let weak = ctx.next_label("parameter_weak_binding");
        abi::load_at_offset(emitter, result, INVOKER_BINDING_POLICY_OFFSET);
        branch_equal(emitter, result, 0, &weak);
        accept_strict_type(emitter, data, ctx, expected, &valid);
        abi::emit_jump(emitter, &invalid);
        emitter.label(&weak);
        accept_type(emitter, data, ctx, expected, &valid);
    } else {
        accept_type(emitter, data, ctx, expected, &valid);
    }
    abi::emit_jump(emitter, &invalid);
    emitter.label(&invalid);
    if predicate {
        abi::emit_load_int_immediate(emitter, result, 0);
        abi::emit_jump(emitter, &done);
    } else {
        emit_type_error(emitter, data, &format!("Callback argument ${name} must be of type {expected:?}"));
    }
    emitter.label(&valid);
    if predicate { abi::emit_load_int_immediate(emitter, result, 1); }
    emitter.label(&done);
    abi::emit_release_temporary_stack(emitter, 32);
}

/// Strict scalar verification still permits int-to-float widening and exact union members.
fn accept_strict_type(emitter: &mut Emitter, data: &mut DataSection, ctx: &mut InvokerEmitContext, expected: &PhpType, valid: &str) {
    if let PhpType::Union(members) = expected {
        for member in members { accept_strict_type(emitter, data, ctx, member, valid); }
        return;
    }
    let result = abi::int_result_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, result, 0);
    let tags: &[i64] = match expected {
        PhpType::Int => &[0],
        PhpType::Float => &[0, 2],
        PhpType::Bool => &[3],
        PhpType::Str => &[1],
        _ => { accept_type(emitter, data, ctx, expected, valid); return; }
    };
    for tag in tags { branch_equal(emitter, result, *tag, valid); }
}

/// Emits weak scalar, container-shape, Stringable and object-class acceptance for a borrowed tuple.
fn accept_type(emitter: &mut Emitter, data: &mut DataSection, ctx: &mut InvokerEmitContext, expected: &PhpType, valid: &str) {
    if let PhpType::Union(members) = expected {
        for member in members { accept_type(emitter, data, ctx, member, valid); }
        return;
    }
    let result = abi::int_result_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, result, 0);
    let tags: &[i64] = match expected {
        PhpType::Mixed => { abi::emit_jump(emitter, valid); return; }
        PhpType::Int => &[0, 3],
        PhpType::Float => &[0, 2, 3],
        PhpType::Bool => &[0, 1, 2, 3],
        PhpType::Str => &[0, 1, 2, 3],
        PhpType::Void => &[8],
        PhpType::Array(_) | PhpType::AssocArray { .. } => &[4, 5],
        PhpType::Iterable => &[4, 5, 6],
        PhpType::Callable => &[10],
        PhpType::Object(name) if name.is_empty() => &[6],
        PhpType::False => &[],
        _ => &[],
    };
    for tag in tags { branch_equal(emitter, result, *tag, valid); }
    if *expected == PhpType::Str {
        stringable::accept_stringable_object(emitter, ctx, valid);
    }
    if *expected == PhpType::False {
        let next = ctx.next_label("parameter_false_next");
        let check = ctx.next_label("parameter_false_payload");
        branch_equal(emitter, result, 3, &check);
        abi::emit_jump(emitter, &next);
        emitter.label(&check);
        abi::emit_load_temporary_stack_slot(emitter, result, 8);
        branch_equal(emitter, result, 0, valid);
        emitter.label(&next);
    }
    if matches!(expected, PhpType::Int | PhpType::Float) {
        let string = ctx.next_label("parameter_numeric_string");
        let next = ctx.next_label("parameter_numeric_next");
        branch_equal(emitter, result, 1, &string);
        if *expected == PhpType::Int {
            let float = ctx.next_label("parameter_float_range");
            branch_equal(emitter, result, 2, &float);
            abi::emit_jump(emitter, &next);
            emitter.label(&float);
            abi::emit_load_temporary_stack_slot(emitter, result, 8);
            move_bits_to_float(emitter, result);
            float_fits_int(emitter, &next);
            abi::emit_jump(emitter, valid);
        } else { abi::emit_jump(emitter, &next); }
        emitter.label(&string);
        if *expected == PhpType::Int {
            numeric_bounds::accept_exact_integer_spelling(emitter, ctx, valid);
        }
        let (ptr, len) = abi::string_result_regs(emitter);
        abi::emit_load_temporary_stack_slot(emitter, ptr, 8);
        abi::emit_load_temporary_stack_slot(emitter, len, 16);
        abi::emit_call_label(emitter, if *expected == PhpType::Int { "__rt_str_looks_like_int_for_coercion" } else { "__rt_str_to_number" });
        abi::emit_branch_if_int_result_zero(emitter, &next);
        if *expected == PhpType::Int {
            float_fits_int(emitter, &next);
        }
        abi::emit_jump(emitter, valid);
        emitter.label(&next);
    }
    if let PhpType::Object(name) = expected {
        if !name.is_empty() {
            let next = ctx.next_label("parameter_object_next");
            abi::emit_load_temporary_stack_slot(emitter, result, 0);
            branch_equal(emitter, result, 6, &format!("{next}_check"));
            abi::emit_jump(emitter, &next);
            emitter.label(&format!("{next}_check"));
            let (label, length) = data.add_string(name.trim_start_matches('\\').as_bytes());
            // This runtime helper takes the native PHP string-result ABI, not libc's C ABI.
            let (ptr, len) = abi::string_result_regs(emitter);
            abi::emit_symbol_address(emitter, ptr, &label);
            abi::emit_load_int_immediate(emitter, len, length as i64);
            abi::emit_call_label(emitter, "__rt_instanceof_lookup");
            abi::emit_branch_if_int_result_zero(emitter, &next);
            match emitter.target.arch {
                Arch::AArch64 => abi::emit_load_temporary_stack_slot(emitter, "x0", 8),
                Arch::X86_64 => {
                    abi::emit_reg_move(emitter, "rsi", "rdi");
                    abi::emit_load_temporary_stack_slot(emitter, "rdi", 8);
                }
            }
            abi::emit_call_label(emitter, "__rt_exception_matches");
            abi::emit_branch_if_int_result_nonzero(emitter, valid);
            emitter.label(&next);
        }
    }
}

/// Branches on one tag/value without changing the argument payload register.
fn branch_equal(emitter: &mut Emitter, register: &str, value: i64, label: &str) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, value);
    emitter.instruction(&format!("cmp {register}, {scratch}"));                 // compare a borrowed argument tag without changing its payload
    emitter.instruction(&format!("{} {label}", if emitter.target.arch == Arch::AArch64 { "b.eq" } else { "je" })); // select the matching PHP binding rule
}

/// Reinterprets a borrowed scalar payload as binary64 for PHP integer-bound checks.
fn move_bits_to_float(emitter: &mut Emitter, register: &str) {
    emitter.instruction(&format!("{} {}, {register}", if emitter.target.arch == Arch::AArch64 { "fmov" } else { "movq" }, abi::float_result_reg(emitter))); // reinterpret the borrowed float bits for range checking
}

/// Rejects NaN/infinity and values outside the inclusive-negative/exclusive-positive i64 range.
fn float_fits_int(emitter: &mut Emitter, invalid: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fcmp d0, d0");                                 // reject NaN before integer conversion
            emitter.instruction(&format!("b.vs {invalid}"));                    // unordered values never satisfy the PHP int boundary
            abi::emit_load_int_immediate(emitter, "x9", 0x43e0000000000000);
            emitter.instruction("fmov d1, x9");                                 // materialize the exclusive positive integer limit
            emitter.instruction("fcmp d0, d1");                                 // compare the value with 2^63
            emitter.instruction(&format!("b.ge {invalid}"));                    // reject positive overflow and infinity
            abi::emit_load_int_immediate(emitter, "x9", 0xc3e0000000000000u64 as i64);
            emitter.instruction("fmov d1, x9");                                 // materialize the inclusive negative integer limit
            emitter.instruction("fcmp d0, d1");                                 // compare the value with -2^63
            emitter.instruction(&format!("b.lt {invalid}"));                    // reject negative overflow and infinity
        }
        Arch::X86_64 => {
            emitter.instruction("ucomisd xmm0, xmm0");                          // reject NaN before integer conversion
            emitter.instruction(&format!("jp {invalid}"));                      // unordered values never satisfy the PHP int boundary
            abi::emit_load_int_immediate(emitter, "r10", 0x43e0000000000000);
            emitter.instruction("movq xmm1, r10");                              // materialize the exclusive positive integer limit
            emitter.instruction("ucomisd xmm0, xmm1");                          // compare the value with 2^63
            emitter.instruction(&format!("jae {invalid}"));                     // reject positive overflow and infinity
            abi::emit_load_int_immediate(emitter, "r10", 0xc3e0000000000000u64 as i64);
            emitter.instruction("movq xmm1, r10");                              // materialize the inclusive negative integer limit
            emitter.instruction("ucomisd xmm0, xmm1");                          // compare the value with -2^63
            emitter.instruction(&format!("jb {invalid}"));                      // reject negative overflow and infinity
        }
    }
}

/// Builds a TypeError with the supplied static message and transfers it to the exception unwinder.
fn emit_type_error(emitter: &mut Emitter, data: &mut DataSection, message: &str) {
    let (label, length) = data.add_string(message.as_bytes());
    let result = abi::int_result_reg(emitter);
    abi::emit_load_int_immediate(emitter, result, 56);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    let scratch = abi::secondary_scratch_reg(emitter);
    let kind = if emitter.target.arch == Arch::AArch64 { 6 } else { crate::codegen_support::sentinels::x86_64_heap_kind_word(6) as i64 };
    abi::emit_load_int_immediate(emitter, scratch, kind);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("str {scratch}, [{result}, #-8]")), // stamp the TypeError object heap kind
        Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [{result} - 8], {scratch}")), // stamp the TypeError object heap kind
    }
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");
    abi::emit_load_symbol_to_reg(emitter, scratch, "_spl_type_error_class_id", 0);
    abi::emit_store_to_address(emitter, scratch, result, 0);
    abi::emit_symbol_address(emitter, scratch, &label);
    abi::emit_store_to_address(emitter, scratch, result, 8);
    abi::emit_load_int_immediate(emitter, scratch, length as i64);
    abi::emit_store_to_address(emitter, scratch, result, 16);
    for offset in [24, 32, 40, 48] { abi::emit_store_zero_to_address(emitter, result, offset); }
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_jump(emitter, "__rt_throw_current");
}
