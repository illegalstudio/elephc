//! Purpose:
//! Lowers named and dynamic instanceof entry points.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Eval-aware and runtime metadata paths keep their existing precedence.

use super::*;

/// Lowers named `instanceof` using runtime class/interface metadata.
pub(in crate::codegen::lower_inst) fn lower_instanceof(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?;
    let class_name = class_name_immediate(ctx, inst)?.to_string();
    // `instanceof Closure` has a direct answer from the value's own storage — a native callable
    // descriptor IS a Closure — but only when every closure in the program is native.
    //
    // A PROGRAM WITH AN EVAL BRIDGE ALSO HAS INTERPRETED CLOSURES, and those are not callable
    // descriptors: the interpreter hands back its generic object wrapper, so the tag test below
    // answers `false` for a value PHP calls a Closure. Symfony's event dispatcher is exactly
    // that shape — `$listener[0] instanceof Closure` guards the conversion of a lazily built
    // listener — and a false answer there leaves the array unconverted until `$listener(...)`
    // fatals. So the fast path is taken only when the answer cannot come from the bridge, and
    // everything else falls through to the eval-aware matcher below, which asks the bridge's
    // identity table first and the native matcher (descriptors included) second.
    let closure_probe = class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("Closure");
    if closure_probe
        && (matches!(value_ty, PhpType::Callable)
            || !ctx.module.required_runtime_features.eval_bridge)
    {
        emit_closure_instanceof(ctx, value, &value_ty)?;
        return store_if_result(ctx, inst);
    }
    if !matches!(
        value_ty,
        PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Iterable
    ) {
        emit_false(ctx);
        return store_if_result(ctx, inst);
    }
    if class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("__PHP_Incomplete_Class")
    {
        emit_incomplete_class_instanceof(ctx, value, &value_ty)?;
        return store_if_result(ctx, inst);
    }
    // A receiver whose STATIC class already satisfies the target answers from the declared type
    // alone, with no metadata lookup and no bridge crossing. `PhpType::Object(name)` is an upper
    // bound -- the value is `name` or a subclass -- and PHP's hierarchy only grows downward, so
    // every value that can arrive satisfies the target too.
    //
    // This is what the eval bridge cannot improve on: an object an eval context created carries
    // the generic wrapper as its native header, so the ordinary matcher would see `stdClass`, but
    // the DECLARED type is still true of it. Deciding from the declaration is therefore both
    // cheaper and more accurate than asking the bridge.
    //
    // The answer is `receiver != null`, not a constant `true`: PHP's `null instanceof X` is false,
    // and a declared object type still reads null from an uninitialized slot.
    //
    // Measured on Symfony's `--web` request: `object_is_a` was 229 of 387 eval-bridge entries,
    // the single largest remaining category, against targets like `Request`, `RequestStack` and
    // `ContainerInterface` that the receiver's own declaration already answers.
    if static_type_satisfies_instanceof(ctx, &value_ty, &class_name) {
        emit_receiver_is_non_null(ctx, value)?;
        return store_if_result(ctx, inst);
    }
    // Not for `Closure`: the bridge answers from objects it can see, and a native closure is a
    // callable descriptor (runtime tag 10), not an object, so the bridge-only answer was `false`
    // for every compiled closure in a function that happens to own an eval context. Laravel's
    // `Container::build()` owns one (`new ReflectionClass($concrete)`), read its own resolver
    // closure as a class name, and every binding failed with "Target class [] does not exist".
    // The path below asks the bridge first and then the native closure test.
    if builtins::has_eval_context(ctx) && !closure_probe {
        return builtins::lower_eval_object_is_a(ctx, inst, value, &class_name, false);
    }
    // An AOT method can receive an object that was created by a request-global eval context.
    // Its native storage header is deliberately the generic object wrapper, so the ordinary
    // matcher would only see `stdClass`. Ask the bridge-owned identity table first, then retain
    // the native matcher as the fallback for ordinary AOT objects and callable descriptors.
    let eval_fallback_done = if ctx.module.required_runtime_features.eval_bridge
        && matches!(value_ty, PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_))
    {
        let eval_matched = ctx.next_label("instanceof_eval_owner_matched");
        let eval_definite_no = ctx.next_label("instanceof_eval_owner_definite_no");
        let native_fallback = ctx.next_label("instanceof_eval_owner_native_fallback");
        let done = ctx.next_label("instanceof_eval_owner_done");
        builtins::emit_eval_object_is_a_named_fallback(
            ctx,
            value,
            &class_name,
            &eval_matched,
            &native_fallback,
            &eval_definite_no,
        )?;
        ctx.emitter.label(&eval_matched);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&eval_definite_no);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&native_fallback);
        Some(done)
    } else {
        None
    };
    emit_native_instanceof(ctx, value, &value_ty, &class_name)?;
    if let Some(done) = &eval_fallback_done {
        abi::emit_jump(ctx.emitter, done);
        ctx.emitter.label(done);
    }
    store_if_result(ctx, inst)
}

/// Emits the ordinary class-id answer for `instanceof`, leaving it in the result register.
///
/// Lifted out of `lower_instanceof` so the eval-context path can reach it too: that path used to
/// `return` straight into the bridge, which meant a function holding an eval context crossed the
/// FFI boundary for EVERY `instanceof`, including ones over an ordinary compiled object.
fn emit_native_instanceof(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
    class_name: &str,
) -> Result<()> {
    if class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("Closure")
    {
        return emit_closure_instanceof(ctx, value, value_ty);
    }
    let Some((target_id, target_kind)) = classify_named_target(ctx, class_name) else {
        emit_false(ctx);
        return Ok(());
    };
    match value_ty {
        PhpType::Callable => {
            emit_callable_object_capture_or_null(ctx, value)?;
            emit_match_call(ctx, target_id, target_kind, "__rt_exception_matches");
        }
        // `Iterable` joins `Object`, not `Mixed`: an `iterable` slot holds the UNBOXED payload
        // (coercing a gradual value to `iterable` emits `Op::MixedUnbox`, see
        // src/ir_lower/gradual_coercions.rs:104), so the raw pointer is already what the object
        // matcher wants -- routing it to `__rt_mixed_instanceof` double-unboxes and answers
        // false. Its array half stays safe because the matcher range-checks the class id against
        // the emitted table; empty, list, assoc and nested arrays all answer false, matching
        // php -n 8.5.10. Omitting `Iterable` made every `instanceof` on an `iterable` operand a
        // hardcoded false -- a WRONG answer, not a conservative one, which is what sent Symfony's
        // `ResourceCheckerConfigCache::isFresh()` past its `iterator_to_array()` conversion.
        PhpType::Object(_) | PhpType::Iterable => {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            emit_match_call(ctx, target_id, target_kind, "__rt_exception_matches");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            emit_match_call(ctx, target_id, target_kind, "__rt_mixed_instanceof");
        }
        _ => emit_false(ctx),
    }
    Ok(())
}

/// Tests the synthetic `__PHP_Incomplete_Class` identity carried by class id `-2`.
///
/// `unserialize(..., ['allowed_classes' => false])` stores refused objects with a reserved
/// class id and the original class name; PHP still reports them as instances of the internal
/// `__PHP_Incomplete_Class`. No declared class-info entry exists for that name, so the normal
/// target classifier would answer false before looking at the object.
fn emit_incomplete_class_instanceof(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    match value_ty {
        PhpType::Object(_) => {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("ldr x0, [x0]");                    // object header class id
                    ctx.emitter.instruction("cmn x0, #2");                      // class id -2 marks an incomplete object
                    ctx.emitter.instruction("cset x0, eq");                     // return whether this is the synthetic class
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rax, QWORD PTR [rax]");        // object header class id
                    ctx.emitter.instruction("cmp rax, -2");                     // class id -2 marks an incomplete object
                    ctx.emitter.instruction("sete al");                         // materialize the identity comparison
                    ctx.emitter.instruction("movzx rax, al");                   // widen the boolean result to the integer ABI
                }
            }
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            let false_label = ctx.next_label("incomplete_instanceof_false");
            let done = ctx.next_label("incomplete_instanceof_done");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #6");                      // runtime tag 6 = object payload
                    ctx.emitter.instruction(&format!("b.ne {}", false_label));  // non-objects are not instances
                    ctx.emitter.instruction("ldr x1, [x1]");                    // object header class id
                    ctx.emitter.instruction("cmn x1, #2");                      // class id -2 marks an incomplete object
                    ctx.emitter.instruction("cset x0, eq");                     // return the synthetic-class identity
                    ctx.emitter.instruction(&format!("b {}", done));            // skip the false arm
                    ctx.emitter.label(&false_label);
                    ctx.emitter.instruction("mov x0, #0");                      // scalar payloads are not instances
                    ctx.emitter.label(&done);
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 6");                      // runtime tag 6 = object payload
                    ctx.emitter.instruction(&format!("jne {}", false_label));   // non-objects are not instances
                    ctx.emitter.instruction("mov rdi, QWORD PTR [rdi]");        // object header class id
                    ctx.emitter.instruction("cmp rdi, -2");                     // class id -2 marks an incomplete object
                    ctx.emitter.instruction("sete al");                         // materialize the identity comparison
                    ctx.emitter.instruction("movzx rax, al");                   // widen the boolean result to the integer ABI
                    ctx.emitter.instruction(&format!("jmp {}", done));          // skip the false arm
                    ctx.emitter.label(&false_label);
                    ctx.emitter.instruction("xor eax, eax");                    // scalar payloads are not instances
                    ctx.emitter.label(&done);
                }
            }
        }
        _ => emit_false(ctx),
    }
    Ok(())
}

/// Lowers dynamic `instanceof` where the target is resolved from a runtime string or object.
pub(in crate::codegen::lower_inst) fn lower_instanceof_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let target = expect_operand(inst, 1)?;
    if builtins::has_eval_context(ctx) {
        return builtins::lower_eval_object_is_a_dynamic(ctx, inst, value, target, false);
    }
    let value_ty = ctx.value_php_type(value)?;
    let target_ty = ctx.value_php_type(target)?;
    // A class the interpreter declared is unknown to the compile-time class table, so a string
    // target naming one never resolves below. Ask the bridge's identity table first, with no
    // local context, exactly as the named `instanceof` does; a miss falls through to the native
    // matcher for ordinary compiled objects.
    let eval_fallback_done = if ctx.module.required_runtime_features.eval_bridge
        && matches!(value_ty, PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_))
        && matches!(target_ty, PhpType::Str | PhpType::Mixed | PhpType::Union(_))
    {
        let eval_matched = ctx.next_label("instanceof_dynamic_eval_owner_matched");
        let native_fallback = ctx.next_label("instanceof_dynamic_eval_owner_native");
        let eval_done = ctx.next_label("instanceof_dynamic_eval_owner_done");
        builtins::emit_eval_object_is_a_dynamic_name_fallback(
            ctx,
            value,
            target,
            &target_ty,
            &eval_matched,
            &native_fallback,
        )?;
        ctx.emitter.label(&eval_matched);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        abi::emit_jump(ctx.emitter, &eval_done);
        ctx.emitter.label(&native_fallback);
        Some(eval_done)
    } else {
        None
    };
    let target_false = ctx.next_label("instanceof_dynamic_target_false");
    let done = ctx.next_label("instanceof_dynamic_done");
    emit_normalized_dynamic_instanceof_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_dynamic_target_metadata(ctx, target, &target_ty, &target_false)?;
    emit_dynamic_match_call(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&target_false);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_false(ctx);
    ctx.emitter.label(&done);
    if let Some(eval_done) = &eval_fallback_done {
        abi::emit_jump(ctx.emitter, eval_done);
        ctx.emitter.label(eval_done);
    }
    store_if_result(ctx, inst)
}

/// Whether a receiver's statically known class already satisfies an `instanceof` target.
///
/// Answers only for an exact `PhpType::Object(name)` with a name the module knows. A bare
/// `object`, a gradual value, or a class the closed world has never seen cannot prove anything,
/// and the caller falls through to the runtime paths.
fn static_type_satisfies_instanceof(
    ctx: &FunctionContext<'_>,
    value_ty: &PhpType,
    target: &str,
) -> bool {
    let PhpType::Object(class_name) = value_ty else {
        return false;
    };
    let class_name = class_name.trim_start_matches('\\');
    let target = target.trim_start_matches('\\');
    if class_name.is_empty() || target.is_empty() {
        return false;
    }
    let target_key = php_symbol_key(target);
    let mut current = Some(class_name);
    while let Some(candidate) = current {
        if php_symbol_key(candidate) == target_key {
            return true;
        }
        let Some(info) = ctx.module.class_infos.get(candidate) else {
            break;
        };
        current = info.parent.as_deref().map(|parent| parent.trim_start_matches('\\'));
    }
    crate::codegen::lower_inst::array_access_runtime::class_implements_interface(
        ctx, class_name, target,
    )
}

/// Materializes `receiver != null` as the `instanceof` result.
///
/// The receiver is a raw object pointer at this point, so the whole answer is one compare: PHP
/// says false for null and, given the caller already proved the declared class satisfies the
/// target, true for every other value that can arrive.
fn emit_receiver_is_non_null(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    ctx.load_value_to_result(value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                               // is the receiver a live object pointer?
            ctx.emitter.instruction("cset x0, ne");                              // null answers false, anything else true
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                            // is the receiver a live object pointer?
            ctx.emitter.instruction("setne al");                                 // null answers false, anything else true
            ctx.emitter.instruction("movzx rax, al");                            // widen the boolean byte into the result register
        }
    }
    Ok(())
}
