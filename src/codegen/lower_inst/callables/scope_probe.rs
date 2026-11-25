//! Purpose:
//! Answers `is_callable()` for callable arrays naming methods only the calling class can call.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::member_queries::lower_is_callable()`, after the
//!   public runtime lookup, which also serves the entry check of a declared `callable` parameter.
//!
//! Key details:
//! - PHP checks callability from the calling scope: inside a class, `[$this, 'privateMethod']`,
//!   a protected method of a related class and `[self::class, 'privateStatic']` are callable.
//!   The runtime `__rt_is_callable_*` helpers only know public methods.
//! - The candidates are the scope-only ones the callable factories and invocations already bind
//!   (`shared_selector::scope_only_*`). A class with none emits nothing, so public-only code and
//!   every function outside a class keep the public lookup unchanged.
//! - The probe runs only after the public lookup answered false, never raises (a malformed
//!   array is simply not callable) and allocates nothing: a receiver row is matched, not bound.

use super::*;

/// Indexed-array value-type stamp of slots holding boxed `Mixed` cells.
const ARRAY_STAMP_MIXED: i64 = 7;
/// Indexed-array value-type stamp of slots holding string pointer/length pairs.
const ARRAY_STAMP_STRING: i64 = 1;

/// Turns a false public `is_callable()` verdict in the integer result register into true when
/// `value` is a callable array naming a method only the lexical class can call.
pub(in crate::codegen) fn emit_scope_is_callable_probe(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    boxed: bool,
) -> Result<()> {
    if ctx.lexical_class().is_none() {
        return Ok(());
    }
    let instance_targets = shared_selector::scope_only_instance_targets(ctx);
    let static_cases = shared_selector::scope_only_static_cases(ctx, None);
    if instance_targets.is_empty() && static_cases.is_empty() {
        return Ok(());
    }
    ctx.emitter.comment("is_callable: methods only the calling class scope can call");
    let done = ctx.next_label("scope_is_callable_done");
    let miss = ctx.next_label("scope_is_callable_miss");
    let shaped = ctx.next_label("scope_is_callable_shaped");
    let static_label = ctx.next_label("scope_is_callable_static");
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &done);
    let source = if boxed {
        CallableArraySource::BoxedArray(value)
    } else {
        CallableArraySource::RawArray(value)
    };
    if boxed {
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_INDEXED_ARRAY)); // only an indexed array can name a method here
                ctx.emitter.instruction(&format!("b.ne {}", miss));             // any other boxed value keeps the public verdict
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_INDEXED_ARRAY)); // only an indexed array can name a method here
                ctx.emitter.instruction(&format!("jne {}", miss));              // any other boxed value keeps the public verdict
            }
        }
    }
    // -- a malformed array is not callable: test the shape the selector slots would require --
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_load_callable_array_base(ctx, &source, array_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", array_reg, miss));   // a null array names no method
            ctx.emitter.instruction(&format!("ldr x10, [{}]", array_reg));      // load the array element count
            ctx.emitter.instruction("cmp x10, #2");                             // only [receiver, method] can be callable
            ctx.emitter.instruction(&format!("b.ne {}", miss));                 // another length is not callable
            elephc_memoir::values::array_header::emit_load_value_type_tag(
                ctx.emitter,
                array_reg,
                "x10",
            );
            ctx.emitter.instruction(&format!("cmp x10, #{}", ARRAY_STAMP_MIXED)); // do the slots hold boxed Mixed cells?
            ctx.emitter.instruction(&format!("b.eq {}", shaped));               // read them as Mixed selectors
            ctx.emitter.instruction(&format!("cmp x10, #{}", ARRAY_STAMP_STRING)); // do the slots hold string pairs?
            ctx.emitter.instruction(&format!("b.ne {}", miss));                 // no other slot type can name a method
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", array_reg, array_reg)); // a null array names no method
            ctx.emitter.instruction(&format!("je {}", miss));                   // leave before reading its header
            ctx.emitter.instruction(&format!("cmp QWORD PTR [{}], 2", array_reg)); // only [receiver, method] can be callable
            ctx.emitter.instruction(&format!("jne {}", miss));                  // another length is not callable
            elephc_memoir::values::array_header::emit_load_value_type_tag(
                ctx.emitter,
                array_reg,
                "r10",
            );
            ctx.emitter.instruction(&format!("cmp r10, {}", ARRAY_STAMP_MIXED)); // do the slots hold boxed Mixed cells?
            ctx.emitter.instruction(&format!("je {}", shaped));                 // read them as Mixed selectors
            ctx.emitter.instruction(&format!("cmp r10, {}", ARRAY_STAMP_STRING)); // do the slots hold string pairs?
            ctx.emitter.instruction(&format!("jne {}", miss));                  // no other slot type can name a method
        }
    }
    ctx.emitter.label(&shaped);
    emit_boxed_callable_array_selector_slots(ctx, &source, None)?;
    let matched = ctx.next_label("scope_is_callable_matched");
    selector_tables::emit_instance_target_table_match(
        ctx,
        &instance_targets,
        selector_tables::ReceiverSelector::CallableArray,
        &matched,
        &static_label,
    )?;
    ctx.emitter.label(&static_label);
    for case in &static_cases {
        let next = ctx.next_label("scope_is_callable_static_next");
        emit_branch_if_mixed_static_case_mismatch(ctx, case, &next);
        abi::emit_jump(ctx.emitter, &matched);
        ctx.emitter.label(&next);
    }
    abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
    abi::emit_jump(ctx.emitter, &miss);
    ctx.emitter.label(&matched);
    abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
    abi::emit_load_int_immediate(ctx.emitter, result, 1);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&miss);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    ctx.emitter.label(&done);
    Ok(())
}
