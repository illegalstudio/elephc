//! Purpose:
//! Lowers boxed Mixed array reads, writes, and fetch-for-write operations.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;
use crate::ir::RuntimeCallTarget;

/// Lowers binary runtime fallbacks that Phase 04 can identify by operand type.
pub(super) fn lower_binary_runtime_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let receiver_ty = ctx.value_php_type(receiver)?.codegen_repr();
    let result_ty = inst.result_php_type.codegen_repr();
    match (receiver_ty, &result_ty) {
        (PhpType::Mixed | PhpType::Union(_), PhpType::Void) => {
            lower_mixed_cell_runtime_assign(ctx, inst)
        }
        (PhpType::Mixed | PhpType::Union(_), _) => {
            lower_mixed_array_runtime_get(ctx, inst, false, false)
        }
        (PhpType::AssocArray { .. }, PhpType::Void) => hashes::lower_hash_append(ctx, inst),
        (other, _) => Err(CodegenIrError::unsupported(format!(
            "runtime_call with receiver PHP type {:?} returning PHP type {:?}",
            other, inst.result_php_type
        ))),
    }
}

/// Lowers `$mixed[$key]` through the shared boxed Mixed array/hash/stdClass reader.
pub(super) fn lower_mixed_array_runtime_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    for_write: bool,
    pending_append: bool,
) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let warn_on_missing = expect_operand(inst, 2)?;
    if !for_write && ctx.module.required_runtime_features.eval_bridge {
        // An object the interpreter declared has no compiled class id, so the runtime reader
        // cannot find its `ArrayAccess` methods and answered "Cannot use object of type stdClass
        // as array". Ask the bridge first; a native receiver falls straight through.
        let native_label = ctx.next_label("mixed_array_get_native");
        let done_label = ctx.next_label("mixed_array_get_done");
        lower_eval_owned_array_get(ctx, inst, receiver, key, warn_on_missing, &native_label, &done_label)?;
        ctx.emitter.label(&native_label);
        lower_mixed_array_runtime_get_native(ctx, inst, receiver, key, warn_on_missing, for_write, pending_append)?;
        ctx.emitter.label(&done_label);
        return Ok(());
    }
    lower_mixed_array_runtime_get_native(ctx, inst, receiver, key, warn_on_missing, for_write, pending_append)
}

/// Reads `$receiver[$key]` from an object the interpreter owns through its `ArrayAccess` methods.
///
/// A quiet read (`isset()`, `??`) asks `offsetExists()` first and answers null without calling
/// `offsetGet()` when it says no, as php does; a normal read calls `offsetGet()` directly. A
/// receiver the interpreter does not own branches to `native_label` untouched.
fn lower_eval_owned_array_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver: ValueId,
    key: ValueId,
    warn_on_missing: ValueId,
    native_label: &str,
    done_label: &str,
) -> Result<()> {
    let mut call = inst.clone();
    call.operands = vec![receiver, key];
    let quiet = value_is_const_int(ctx, warn_on_missing, 0)?;
    if !quiet {
        return builtins::lower_eval_owned_method_call(
            ctx,
            &call,
            receiver,
            "offsetGet",
            native_label,
            done_label,
        );
    }
    let exists_label = ctx.next_label("mixed_array_get_exists");
    let absent_label = ctx.next_label("mixed_array_get_absent");
    builtins::lower_eval_owned_method_call_with_result(
        ctx,
        &call,
        receiver,
        "offsetExists",
        native_label,
        &exists_label,
        &PhpType::Bool,
        false,
    )?;
    ctx.emitter.label(&exists_label);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &absent_label);
    builtins::lower_eval_owned_method_call(ctx, &call, receiver, "offsetGet", native_label, done_label)?;
    ctx.emitter.label(&absent_label);
    objects::emit_boxed_null(ctx);
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done_label);
    Ok(())
}

/// The compiled `__rt_mixed_array_get` call for `$mixed[$key]`.
fn lower_mixed_array_runtime_get_native(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver: ValueId,
    key: ValueId,
    warn_on_missing: ValueId,
    for_write: bool,
    pending_append: bool,
) -> Result<()> {
    if !for_write {
        if let Some(kind) = object_key_kind(ctx, key)? {
            return lower_mixed_array_object_key_get(ctx, inst, receiver, key, warn_on_missing, kind);
        }
    }
    lower_mixed_array_runtime_get_plain(ctx, inst, receiver, key, warn_on_missing, for_write, pending_append)
}

/// The hash-keyed `__rt_mixed_array_get` read, for every key that is not an object.
fn lower_mixed_array_runtime_get_plain(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver: ValueId,
    key: ValueId,
    warn_on_missing: ValueId,
    for_write: bool,
    pending_append: bool,
) -> Result<()> {
    if let Some(slot) = callable_array_slot(ctx, key, for_write)? {
        return lower_mixed_callable_slot_or_array_get(
            ctx,
            inst,
            receiver,
            key,
            warn_on_missing,
            slot,
        );
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            hashes::materialize_hash_key_aarch64(ctx, key)?;
            if for_write {
                abi::emit_load_int_immediate(ctx.emitter, "x3", i64::from(pending_append));
            } else {
                ctx.load_value_to_reg(warn_on_missing, "x3")?;
            }
            ctx.load_value_to_reg(receiver, "x0")?;
        }
        Arch::X86_64 => {
            hashes::materialize_hash_key_x86_64(ctx, key)?;
            if for_write {
                abi::emit_load_int_immediate(ctx.emitter, "rcx", i64::from(pending_append));
            } else {
                ctx.load_value_to_reg(warn_on_missing, "rcx")?;
            }
            ctx.load_value_to_reg(receiver, "rdi")?;
        }
    }
    abi::emit_call_label(
        ctx.emitter,
        if for_write {
            "__rt_mixed_array_get_for_write"
        } else {
            "__rt_mixed_array_get"
        },
    );
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// How a subscript key can reach `ArrayAccess::offset*()` with an OBJECT as the offset.
#[derive(Clone)]
enum ObjectKeyKind {
    /// The key is statically an object (`Throwable $e`, `object $o`): never a valid array key, so
    /// only an `ArrayAccess` receiver can accept it. Carries the class name php prints in its
    /// TypeError.
    Static(String),
    /// The key is a boxed `mixed`: it is an object offset only when its runtime tag says so.
    Dynamic,
}

/// Classifies a key for object-offset dispatch, or `None` when it can never be an object.
///
/// A `mixed` container may hold a `WeakMap`, `SplObjectStorage` or a user `ArrayAccess` indexed
/// by objects. The hash-key path normalizes every key to an int or a string, so an object key was
/// either refused at compile time (static object) or read back as null (boxed object): Laravel's
/// exception handler writes `$this->reportedExceptionMap[$e] = true` and `?? false` over an
/// untyped property holding a `WeakMap`.
fn object_key_kind(ctx: &FunctionContext<'_>, key: ValueId) -> Result<Option<ObjectKeyKind>> {
    Ok(match ctx.value_php_type(key)?.codegen_repr() {
        PhpType::Object(name) => {
            let name = name.trim_start_matches('\\');
            Some(ObjectKeyKind::Static(if name.is_empty() {
                "object".to_string()
            } else {
                name.to_string()
            }))
        }
        // A closure is a `Closure` object key (`$scope[$closure]` after `instanceof Closure`).
        PhpType::Callable => Some(ObjectKeyKind::Static("Closure".to_string())),
        PhpType::Mixed | PhpType::Union(_) => Some(ObjectKeyKind::Dynamic),
        _ => None,
    })
}

/// Branches to `label` unless `value` is a boxed Mixed cell whose runtime tag is `tag`.
fn emit_branch_unless_mixed_tag(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    tag: i64,
    label: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(value, "x0")?;
            ctx.emitter.instruction(&format!("cbz x0, {}", label));             // an absent cell is no object
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed runtime payload tag
            ctx.emitter.instruction(&format!("cmp x9, #{}", tag));              // compare against the wanted tag
            ctx.emitter.instruction(&format!("b.ne {}", label));                // any other payload leaves the object path
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(value, "rax")?;
            ctx.emitter.instruction("test rax, rax");                           // an absent cell is no object
            ctx.emitter.instruction(&format!("je {}", label));
            ctx.emitter.instruction(&format!("cmp QWORD PTR [rax], {}", tag));  // compare the boxed tag against the wanted tag
            ctx.emitter.instruction(&format!("jne {}", label));                 // any other payload leaves the object path
        }
    }
    Ok(())
}

/// Branches to `label` unless `key` is a boxed object or closure: a `Closure` is an object key
/// too, boxed with the callable-descriptor tag (10). laravel/serializable-closure keys its
/// `ClosureScope` (an `SplObjectStorage`) by closures, and `isset($this->scope[$closure])` read
/// the hash-key path, which answers null for a descriptor.
fn emit_branch_unless_object_key(ctx: &mut FunctionContext<'_>, key: ValueId, label: &str) -> Result<()> {
    let object = ctx.next_label("mixed_object_key_is_object");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(key, "x0")?;
            ctx.emitter.instruction(&format!("cbz x0, {}", label));             // an absent cell is no object
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed runtime payload tag
            ctx.emitter.instruction("cmp x9, #6");                              // runtime tag 6 = object
            ctx.emitter.instruction(&format!("b.eq {}", object));
            ctx.emitter.instruction("cmp x9, #10");                             // runtime tag 10 = closure descriptor
            ctx.emitter.instruction(&format!("b.ne {}", label));                // any other payload leaves the object path
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(key, "rax")?;
            ctx.emitter.instruction("test rax, rax");                           // an absent cell is no object
            ctx.emitter.instruction(&format!("je {}", label));
            ctx.emitter.instruction("cmp QWORD PTR [rax], 6");                  // runtime tag 6 = object
            ctx.emitter.instruction(&format!("je {}", object));
            ctx.emitter.instruction("cmp QWORD PTR [rax], 10");                 // runtime tag 10 = closure descriptor
            ctx.emitter.instruction(&format!("jne {}", label));                 // any other payload leaves the object path
        }
    }
    ctx.emitter.label(&object);
    Ok(())
}

/// Throws php's TypeError for an object offset used on something that is not `ArrayAccess`.
fn emit_object_offset_type_error(ctx: &mut FunctionContext<'_>, class_name: &str, quiet: bool) {
    let message = if quiet {
        format!("Cannot access offset of type {} in isset or empty", class_name)
    } else {
        format!("Cannot access offset of type {} on array", class_name)
    };
    exceptions::emit_type_error(ctx, &message);
}

/// Reads `$mixed[$objectKey]`: an `ArrayAccess` receiver is asked through `offsetGet()`, a quiet
/// read (`??`, `isset()`, `empty()`) through `offsetExists()` first.
///
/// The `offsetExists()` answer is boxed into the instruction's own result cell and read back
/// from there, because the argument cleanups that follow the call overwrite the result registers.
fn lower_mixed_array_object_key_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver: ValueId,
    key: ValueId,
    warn_on_missing: ValueId,
    kind: ObjectKeyKind,
) -> Result<()> {
    let result = inst.result;
    let result_ty = inst.result_php_type.codegen_repr();
    if !matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "object key read with result PHP type {:?}",
            inst.result_php_type
        )));
    }
    let quiet = value_is_const_int(ctx, warn_on_missing, 0)?;
    let other_label = ctx.next_label("mixed_object_key_other");
    let done_label = ctx.next_label("mixed_object_key_done");
    emit_branch_unless_mixed_tag(ctx, receiver, 6, &other_label)?;
    if matches!(kind, ObjectKeyKind::Dynamic) {
        emit_branch_unless_object_key(ctx, key, &other_label)?;
    }
    let mut call = inst.clone();
    call.operands = vec![receiver, key];
    if quiet {
        let absent_label = ctx.next_label("mixed_object_key_absent");
        array_access_runtime::lower_boxed_array_access_interface_call(ctx, &call, "offsetExists")?;
        if let Some(result) = result {
            let result_reg = abi::int_result_reg(ctx.emitter).to_string();
            ctx.load_value_to_result(result)?;
            abi::emit_push_reg(ctx.emitter, &result_reg);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            abi::emit_push_reg(ctx.emitter, mixed_unbox_low_payload_reg(ctx));
            abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, 16);
            abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
            abi::emit_pop_reg(ctx.emitter, &result_reg);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
        abi::emit_branch_if_int_result_zero(ctx.emitter, &absent_label);
        array_access_runtime::lower_boxed_array_access_interface_call(ctx, &call, "offsetGet")?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&absent_label);
        objects::emit_boxed_null(ctx);
        cast_loaded_mixed_pointer_to_result(ctx, &result_ty)?;
        store_if_result(ctx, inst)?;
    } else {
        array_access_runtime::lower_boxed_array_access_interface_call(ctx, &call, "offsetGet")?;
    }
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&other_label);
    match &kind {
        ObjectKeyKind::Dynamic => {
            lower_mixed_array_runtime_get_plain(ctx, inst, receiver, key, warn_on_missing, false, false)?;
        }
        ObjectKeyKind::Static(class_name) => {
            // A null receiver answers null, as php does for `null[$k]`; anything else cannot take
            // an object offset.
            let not_null_label = ctx.next_label("mixed_object_key_not_null");
            let null_label = ctx.next_label("mixed_object_key_null");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.load_value_to_reg(receiver, "x0")?;
                    ctx.emitter.instruction(&format!("cbz x0, {}", null_label)); // an absent cell reads as null
                    ctx.emitter.instruction("ldr x9, [x0]");                    // inspect the boxed runtime payload tag
                    ctx.emitter.instruction("cmp x9, #8");                      // tag 8 is php null
                    ctx.emitter.instruction(&format!("b.ne {}", not_null_label)); // anything else is a type error
                }
                Arch::X86_64 => {
                    ctx.load_value_to_reg(receiver, "rax")?;
                    ctx.emitter.instruction("test rax, rax");                   // an absent cell reads as null
                    ctx.emitter.instruction(&format!("je {}", null_label));
                    ctx.emitter.instruction("cmp QWORD PTR [rax], 8");          // tag 8 is php null
                    ctx.emitter.instruction(&format!("jne {}", not_null_label)); // anything else is a type error
                }
            }
            ctx.emitter.label(&null_label);
            objects::emit_boxed_null(ctx);
            cast_loaded_mixed_pointer_to_result(ctx, &result_ty)?;
            store_if_result(ctx, inst)?;
            abi::emit_jump(ctx.emitter, &done_label);
            ctx.emitter.label(&not_null_label);
            emit_object_offset_type_error(ctx, class_name, quiet);
        }
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Which slot of PHP's two-element callable array a constant index is asking for.
///
/// A `callable` slot holds a DESCRIPTOR, not the `[$object, 'method']` array the caller wrote, so
/// indexing one has to rebuild the array slot by slot. The descriptor keeps both halves: the
/// receiver as its first runtime capture, and the method name inside its qualified PHP name.
#[derive(Clone, Copy)]
enum CallableArraySlot {
    /// Index 0 — the bound receiver, returned as the SAME object so identity is preserved.
    Receiver,
    /// Index 1 — the bare method name, split off the descriptor's qualified `"Class::method"`.
    MethodName,
}

/// Recognizes a constant read of index 0 or 1, the only two slots a callable array has.
///
/// Reads for write are excluded: `$callable[0] = ...` is not something a descriptor can model,
/// and the ordinary boxed-array writer already handles that path.
fn callable_array_slot(
    ctx: &FunctionContext<'_>,
    key: ValueId,
    for_write: bool,
) -> Result<Option<CallableArraySlot>> {
    if for_write {
        return Ok(None);
    }
    if value_is_const_int(ctx, key, 0)? {
        return Ok(Some(CallableArraySlot::Receiver));
    }
    if value_is_const_int(ctx, key, 1)? {
        return Ok(Some(CallableArraySlot::MethodName));
    }
    Ok(None)
}

/// Reads one slot of a boxed callable-array descriptor, otherwise delegates to the ordinary boxed
/// array/hash reader.
///
/// Index 0 preserves object identity for APIs returning callable arrays. Index 1 used to fall
/// through to `__rt_mixed_array_get`, which answers null for the callable descriptor tag, so
/// `$callable[1]` read back EMPTY while `is_array($callable)` on the same value answered true —
/// the descriptor impersonated an array for the predicate but not for the read. Symfony's
/// `ControllerEvent::getAttributes` is the caller that needs both halves: it reaches
/// `method_exists($this->controller[0], $this->controller[1])` under an `\is_array` guard, and
/// with an empty second argument no `match` arm matched and the request died on
/// `unhandled match case`.
fn lower_mixed_callable_slot_or_array_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver: ValueId,
    key: ValueId,
    warn_on_missing: ValueId,
    slot: CallableArraySlot,
) -> Result<()> {
    let callable_label = ctx.next_label("mixed_callable_array_slot");
    let native_label = ctx.next_label("mixed_callable_array_native");
    let done_label = ctx.next_label("mixed_callable_array_done");
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(receiver, "x0")?;
            ctx.emitter.instruction(&format!("cbz x0, {}", native_label));
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed runtime tag
            ctx.emitter.instruction("cmp x9, #10");                            // callable descriptor tag
            ctx.emitter.instruction(&format!("b.ne {}", native_label));
            ctx.emitter.instruction("ldr x9, [x0, #8]");                       // load the callable descriptor payload
            ctx.emitter.instruction("ldr x10, [x9]");                          // inspect the descriptor source-shape kind
            ctx.emitter.instruction(&format!(
                "cmp x10, #{}",
                callable_descriptor::CALLABLE_DESC_KIND_ARRAY
            ));
            ctx.emitter.instruction(&format!("b.eq {}", callable_label));
            ctx.emitter.instruction(&format!("b {}", native_label));
            ctx.emitter.label(&callable_label);
            match slot {
                CallableArraySlot::Receiver => {
                    ctx.emitter
                        .instruction(&format!("mov {}, x9", descriptor_reg));
                    abi::emit_load_from_address(
                        ctx.emitter,
                        "x0",
                        descriptor_reg,
                        callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET,
                    );
                }
                CallableArraySlot::MethodName => {
                    ctx.emitter.instruction("mov x0, x9");                     // pass the descriptor to the method-name reader
                }
            }
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(receiver, "rax")?;
            ctx.emitter.instruction("test rax, rax");                          // null Mixed cells use the ordinary reader
            ctx.emitter.instruction(&format!("je {}", native_label));
            ctx.emitter.instruction("cmp QWORD PTR [rax], 10");                // callable descriptor tag
            ctx.emitter.instruction(&format!("jne {}", native_label));
            ctx.emitter.instruction("mov r10, QWORD PTR [rax + 8]");            // load the callable descriptor payload
            ctx.emitter.instruction(&format!(
                "cmp QWORD PTR [r10], {}",
                callable_descriptor::CALLABLE_DESC_KIND_ARRAY
            ));
            ctx.emitter.instruction(&format!("je {}", callable_label));
            ctx.emitter.instruction(&format!("jmp {}", native_label));
            ctx.emitter.label(&callable_label);
            match slot {
                CallableArraySlot::Receiver => {
                    ctx.emitter
                        .instruction(&format!("mov {}, r10", descriptor_reg));
                    abi::emit_load_from_address(
                        ctx.emitter,
                        "rax",
                        descriptor_reg,
                        callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET,
                    );
                }
                CallableArraySlot::MethodName => {
                    ctx.emitter.instruction("mov rax, r10");                   // pass the descriptor to the method-name reader
                }
            }
        }
    }
    match slot {
        // The receiver capture is a raw object pointer and still needs a Mixed box; the
        // method-name helper already returns one.
        CallableArraySlot::Receiver => {
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Object(String::new()));
        }
        CallableArraySlot::MethodName => {
            abi::emit_call_label(ctx.emitter, "__rt_callable_descriptor_method_name");
        }
    }
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&native_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            hashes::materialize_hash_key_aarch64(ctx, key)?;
            ctx.load_value_to_reg(warn_on_missing, "x3")?;
            ctx.load_value_to_reg(receiver, "x0")?;
        }
        Arch::X86_64 => {
            hashes::materialize_hash_key_x86_64(ctx, key)?;
            ctx.load_value_to_reg(warn_on_missing, "rcx")?;
            ctx.load_value_to_reg(receiver, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_get");
    ctx.emitter.label(&done_label);
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Returns whether one EIR operand is the requested integer literal.
fn value_is_const_int(ctx: &FunctionContext<'_>, value: ValueId, expected: i64) -> Result<bool> {
    let value = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value.def else {
        return Ok(false);
    };
    let inst = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(inst.op == Op::ConstI64 && inst.immediate == Some(Immediate::I64(expected)))
}

/// Lowers typed fetch-for-write parent reads of nested array writes (issue #555).
///
/// Two receiver shapes share the `ArrayFetchForWrite` runtime target:
/// - boxed `Mixed` receiver → `__rt_mixed_array_get_for_write(cell, key)`
///   autovivifies missing/null elements inside the receiver cell and returns
///   an owned boxed cell (the STORED one whenever storage is boxed);
/// - concrete `Array`/`AssocArray` receiver → `__rt_array_ensure_elem_for_write
///   (container, tag, key)` autovivifies the element and returns the possibly
///   promoted/reallocated container pointer for the local storeback.
pub(super) fn lower_array_fetch_for_write_runtime_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    pending_append: bool,
) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let receiver_ty = ctx.value_php_type(receiver)?.codegen_repr();
    let normalization = if matches!(inst.immediate, Some(Immediate::RuntimeCall(
        RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed
            | RuntimeCallTarget::PendingAppendFetchForWriteAlreadyDiagnosed,
    ))) {
        hashes::HashKeyNormalization::PhpAlreadyDiagnosed
    } else { hashes::HashKeyNormalization::Php };
    match receiver_ty {
        PhpType::Mixed | PhpType::Union(_) => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    hashes::materialize_hash_key_aarch64_with(ctx, key, normalization)?;
                    abi::emit_load_int_immediate(ctx.emitter, "x3", i64::from(pending_append));
                    ctx.load_value_to_reg(receiver, "x0")?;
                }
                Arch::X86_64 => {
                    hashes::materialize_hash_key_x86_64_with(ctx, key, normalization)?;
                    abi::emit_load_int_immediate(ctx.emitter, "rcx", i64::from(pending_append));
                    ctx.load_value_to_reg(receiver, "rdi")?;
                }
            }
            abi::emit_call_label(ctx.emitter, "__rt_mixed_array_get_for_write");
            cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
            store_if_result(ctx, inst)
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            if let Some(slot) = super::receiver_place::ReceiverPlace::resolve(ctx, receiver)?.slot() {
                ctx.prepare_mutated_source_local_owner(slot, receiver)?;
            }
            let indexed = matches!(receiver_ty, PhpType::Array(_));
            let tag: i64 = if indexed {
                4
            } else {
                5
            };
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    hashes::materialize_hash_key_aarch64_with(ctx, key, normalization)?;
                    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
                    ctx.load_value_to_reg(receiver, "x0")?;
                    if indexed {
                        abi::emit_push_reg(ctx.emitter, "x0");
                        abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
                        ctx.emitter.instruction("cmp x0, #3");                  // a runtime-promoted array must be fetched through associative storage
                        ctx.emitter.instruction("mov x1, #4");                  // indexed payloads keep their normal fetch tag
                        ctx.emitter.instruction("mov x9, #5");                  // hashes use the associative payload tag
                        ctx.emitter.instruction("csel x1, x9, x1, eq");         // select the actual container shape instead of its static Array type
                        abi::emit_pop_reg(ctx.emitter, "x0");
                    } else {
                        abi::emit_load_int_immediate(ctx.emitter, "x1", tag);
                    }
                    abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");
                }
                Arch::X86_64 => {
                    hashes::materialize_hash_key_x86_64_with(ctx, key, normalization)?;
                    abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
                    ctx.load_value_to_reg(receiver, "rdi")?;
                    if indexed {
                        abi::emit_push_reg(ctx.emitter, "rdi");
                        ctx.emitter.instruction("mov rax, rdi");                // heap-kind queries take their payload in the integer result register
                        abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
                        ctx.emitter.instruction("cmp rax, 3");                  // a runtime-promoted array must be fetched through associative storage
                        ctx.emitter.instruction("mov rsi, 4");                  // indexed payloads keep their normal fetch tag
                        ctx.emitter.instruction("mov r10, 5");                  // hashes use the associative payload tag
                        ctx.emitter.instruction("cmove rsi, r10");              // select the actual container shape instead of its static Array type
                        abi::emit_pop_reg(ctx.emitter, "rdi");
                    } else {
                        abi::emit_load_int_immediate(ctx.emitter, "rsi", tag);
                    }
                    abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
                }
            }
            abi::emit_call_label(ctx.emitter, "__rt_array_ensure_elem_for_write");
            store_if_result(ctx, inst)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "fetch-for-write runtime_call with receiver PHP type {:?}",
            other
        ))),
    }
}

/// Lowers `$mixed[$key] = $value` through the shared boxed Mixed array/hash/stdClass writer.
///
/// An `Immediate::Bool(true)` marks the write half of a compound update whose read already
/// reported a float key's conversion, so the key is rebuilt without a second deprecation.
pub(super) fn lower_mixed_array_runtime_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    match ctx.value_php_type(receiver)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {}
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "runtime_call array set with receiver PHP type {:?}",
                other
            )))
        }
    }
    let normalization = if matches!(inst.immediate, Some(Immediate::Bool(true))) {
        hashes::HashKeyNormalization::PhpAlreadyDiagnosed
    } else {
        hashes::HashKeyNormalization::Php
    };
    let object_label = ctx.next_label("mixed_array_set_array_access");
    let native_label = ctx.next_label("mixed_array_set_native");
    let done_label = ctx.next_label("mixed_array_set_done");
    emit_mixed_array_set_object_guard(ctx, receiver, &object_label, &native_label)?;

    ctx.emitter.label(&native_label);
    // A closure key keeps the native writer's own handling (and its operand releases) here.
    let closure_key = matches!(ctx.value_php_type(key)?.codegen_repr(), PhpType::Callable);
    if let Some(ObjectKeyKind::Static(class_name)) =
        object_key_kind(ctx, key)?.filter(|_| !closure_key)
    {
        // An object offset is never a valid array key: only the `ArrayAccess` arm below takes it.
        emit_object_offset_type_error(ctx, &class_name, false);
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                lower_mixed_array_runtime_set_aarch64(ctx, receiver, key, value, normalization)?
            }
            Arch::X86_64 => {
                lower_mixed_array_runtime_set_x86_64(ctx, receiver, key, value, normalization)?
            }
        }
    }
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&object_label);
    if ctx.module.required_runtime_features.eval_bridge {
        // An object the interpreter declared has no compiled interface table: the dispatch below
        // found nothing to call and the write silently vanished. Ask the bridge first.
        let compiled_label = ctx.next_label("mixed_array_set_compiled_array_access");
        builtins::lower_eval_owned_method_call(
            ctx,
            inst,
            receiver,
            "offsetSet",
            &compiled_label,
            &done_label,
        )?;
        ctx.emitter.label(&compiled_label);
    }
    array_access_runtime::lower_boxed_array_access_interface_call(ctx, inst, "offsetSet")?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Branches boxed object payloads to module-aware ArrayAccess dispatch before the shared helper.
fn emit_mixed_array_set_object_guard(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    object_label: &str,
    native_label: &str,
) -> Result<()> {
    // The runtime's own SPL containers have no compiled `offsetSet`: the interface dispatch wrote
    // nowhere (`$fixed[0] = "x"` through a `Mixed` read back garbage, with a bad refcount), while
    // `__rt_mixed_array_set` stores into their runtime storage. They take the native path.
    const RUNTIME_SPL_CLASS_IDS: &[&str] = &[
        "_spl_fixed_array_class_id",
        "_spl_dll_class_id",
        "_spl_stack_class_id",
        "_spl_queue_class_id",
    ];
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(receiver, "x0")?;
            ctx.emitter.instruction(&format!("cbz x0, {}", native_label));      // absent Mixed cells stay on the null-safe runtime path
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed runtime payload tag
            ctx.emitter.instruction("cmp x9, #6");                              // object payloads require module-aware interface dispatch
            ctx.emitter.instruction(&format!("b.ne {}", native_label));         // arrays, hashes and scalars use the runtime writer
            ctx.emitter.instruction("ldr x10, [x0, #8]");                       // the object payload
            ctx.emitter.instruction("ldr x10, [x10]");                          // its class id
            for symbol in RUNTIME_SPL_CLASS_IDS {
                abi::emit_symbol_address(ctx.emitter, "x11", symbol);
                ctx.emitter.instruction("ldr x11, [x11]");                      // a runtime SPL container class id
                ctx.emitter.instruction("cmp x10, x11");                        // is the receiver that container?
                ctx.emitter.instruction(&format!("b.eq {}", native_label));     // its runtime storage takes the write
            }
            abi::emit_jump(ctx.emitter, object_label);                          // call the concrete ArrayAccess::offsetSet implementation
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(receiver, "rax")?;
            ctx.emitter.instruction("test rax, rax");                           // absent Mixed cells stay on the null-safe runtime path
            ctx.emitter.instruction(&format!("je {}", native_label));
            ctx.emitter.instruction("cmp QWORD PTR [rax], 6");                  // object payloads require module-aware interface dispatch
            ctx.emitter.instruction(&format!("jne {}", native_label));          // arrays, hashes and scalars use the runtime writer
            ctx.emitter.instruction("mov r10, QWORD PTR [rax + 8]");            // the object payload
            ctx.emitter.instruction("mov r10, QWORD PTR [r10]");                // its class id
            for symbol in RUNTIME_SPL_CLASS_IDS {
                ctx.emitter.instruction(&format!("cmp r10, QWORD PTR [rip + {symbol}]")); // is the receiver that runtime SPL container?
                ctx.emitter.instruction(&format!("je {}", native_label));       // its runtime storage takes the write
            }
            abi::emit_jump(ctx.emitter, object_label);                          // call the concrete ArrayAccess::offsetSet implementation
        }
    }
    Ok(())
}

/// Materializes AArch64 operands for the boxed Mixed array/hash writer.
pub(super) fn lower_mixed_array_runtime_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    key: ValueId,
    value: ValueId,
    normalization: hashes::HashKeyNormalization,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        if !ctx.value_can_transfer_ownership_to_consumer(value)? {
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
        }
    } else {
        // The declared type for a `?ptr<T>`, so address 0 is stored as PHP null.
        emit_box_current_value_as_mixed(ctx.emitter, &ctx.mixed_box_type(value)?);
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    hashes::materialize_hash_key_aarch64_with(ctx, key, normalization)?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    ctx.load_value_to_reg(receiver, "x0")?;
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    abi::emit_pop_reg(ctx.emitter, "x3");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
    Ok(())
}

/// Materializes x86_64 operands for the boxed Mixed array/hash writer.
pub(super) fn lower_mixed_array_runtime_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    key: ValueId,
    value: ValueId,
    normalization: hashes::HashKeyNormalization,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        if !ctx.value_can_transfer_ownership_to_consumer(value)? {
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
        }
    } else {
        // The declared type for a `?ptr<T>`, so address 0 is stored as PHP null.
        emit_box_current_value_as_mixed(ctx.emitter, &ctx.mixed_box_type(value)?);
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    hashes::materialize_hash_key_x86_64_with(ctx, key, normalization)?;
    abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
    ctx.load_value_to_reg(receiver, "rdi")?;
    abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
    abi::emit_pop_reg(ctx.emitter, "rcx");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
    Ok(())
}

/// Lowers the spread-source materialization: a boxed value in, an indexed array out.
///
/// The whole body is one call. `__rt_mixed_spread_array` passes an indexed array through and
/// rebuilds the two elements of a callable descriptor, so the spread's length guard and element
/// reads see an ordinary array either way.
pub(super) fn lower_mixed_spread_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    ctx.load_value_to_result(source)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_spread_array");
    store_if_result(ctx, inst)
}
