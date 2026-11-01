//! Purpose:
//! Lowers shallow object clones and runtime-managed SPL constructors.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Dynamic hashes, callbacks, and runtime-managed payloads preserve ownership rules.

use super::*;

/// `SplDoublyLinkedList::IT_MODE_LIFO`: the LIFO/FIFO iterator-mode bit.
const SPL_ITER_MODE_LIFO: i64 = 2;
/// `SPL_DLLIST_IT_FIX`: the internal flag PHP sets on `SplStack`/`SplQueue`, which freezes the
/// LIFO/FIFO bit and appears in `getIteratorMode()` (a fresh `SplStack` reports 6, `SplQueue` 4).
const SPL_ITER_MODE_FIX: i64 = 4;

/// Lowers PHP object cloning for fixed-class receivers.
pub(in crate::codegen::lower_inst) fn lower_object_clone_shallow(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_object_clone_shallow_layout(ctx, inst)?;
    emit_eval_clone_adoption(ctx, inst)
}

/// Hands a finished compiled clone to the eval bridge, when it is linked, so a clone of an
/// eval-declared object becomes an object of that same class.
///
/// The compiled clone copies the native layout, which for an eval-declared class is its compiled
/// ancestor's, so the copy was that ancestor: Symfony's `Request::duplicate()` does
/// `clone $this`, and Laravel's `Request::duplicate(): static` then failed its own return type.
/// The bridge leaves a source no eval context owns alone.
fn emit_eval_clone_adoption(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if !ctx.module.required_runtime_features.eval_bridge {
        return Ok(());
    }
    let source = expect_operand(inst, 0)?;
    let Some(result) = inst.result else {
        return Ok(());
    };
    if !matches!(ctx.value_php_type(source)?, PhpType::Object(_))
        || !matches!(ctx.value_php_type(result)?, PhpType::Object(_))
    {
        return Ok(());
    }
    let done_label = ctx.next_label("clone_adopt_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x12", "_elephc_eval_object_clone_adopt_fn", 0);
            ctx.emitter.instruction(&format!("cbz x12, {}", done_label)); // no bridge hook: the clone is already complete
            ctx.load_value_to_reg(source, "x0")?;
            ctx.load_value_to_reg(result, "x1")?;
            abi::emit_load_symbol_to_reg(ctx.emitter, "x12", "_elephc_eval_object_clone_adopt_fn", 0);
            ctx.emitter.instruction("blr x12"); // register the clone with the source's eval class
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r11", "_elephc_eval_object_clone_adopt_fn", 0);
            ctx.emitter.instruction("test r11, r11"); // is the bridge hook installed?
            ctx.emitter.instruction(&format!("jz {}", done_label)); // no bridge hook: the clone is already complete
            ctx.load_value_to_reg(source, "rdi")?;
            ctx.load_value_to_reg(result, "rsi")?;
            abi::emit_load_symbol_to_reg(ctx.emitter, "r11", "_elephc_eval_object_clone_adopt_fn", 0);
            ctx.emitter.instruction("call r11"); // register the clone with the source's eval class
        }
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers the layout copy of a compiled `clone`.
fn lower_object_clone_shallow_layout(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    let class_name = class_name_immediate(ctx, inst)?.to_string();
    if !ctx.module.class_infos.contains_key(&class_name) {
        return lower_runtime_class_object_clone(ctx, inst, source, &class_name, CloneMiss::Fatal);
    }
    // PHP's `clone` copies the object's RUNTIME class, and a method that clones `$this` is
    // routinely inherited. Symfony's `ServiceLocator::withContext()` is exactly that shape, and
    // it is reached on an `Argument\ServiceLocator`, whose `get()` override is the whole reason
    // the subclass exists: cloning through the declaring class's layout answered a base object,
    // so the override was lost -- and with it every property the subclass declares.
    if class_has_compiled_subclass(ctx, &class_name)
        && !is_builtin_stdclass(&class_name)
        && !is_runtime_managed_object_clone_class(&class_name)
    {
        return lower_runtime_class_object_clone(
            ctx,
            inst,
            source,
            &class_name,
            CloneMiss::StaticLayout,
        );
    }
    lower_known_class_object_clone(ctx, inst, source, &class_name)
}

/// What a runtime-dispatched clone does for a source object no compiled class id matches.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CloneMiss {
    /// The static type names no concrete layout to fall back on, so the program cannot continue.
    Fatal,
    /// The static type IS a concrete class: clone through its layout, exactly as this lowering
    /// behaved before the dispatch existed. An eval-declared subclass reaching an inherited
    /// `clone $this` carries no compiled class id to match, and answering its compiled base is
    /// the same answer as before rather than a new abort.
    StaticLayout,
}

/// Returns true when another compiled class names this one as its direct parent.
///
/// Direct parentage is the whole question: a class with any descendant at all has a direct child,
/// so this settles "is this class subclassed" without walking an ancestry chain per candidate.
fn class_has_compiled_subclass(ctx: &FunctionContext<'_>, class_name: &str) -> bool {
    let key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.module.class_infos.values().any(|class_info| {
        class_info
            .parent
            .as_deref()
            .is_some_and(|parent| php_symbol_key(parent.trim_start_matches('\\')) == key)
    })
}

/// Lowers a clone whose static receiver is an interface or generic object type.
fn lower_runtime_class_object_clone(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: ValueId,
    static_type: &str,
    miss: CloneMiss,
) -> Result<()> {
    let normalized_static_type = static_type.trim_start_matches('\\');
    let static_type_resolves_clone_hook = ctx
        .module
        .class_infos
        .get(static_type)
        .is_some_and(|class_info| {
            let clone_key = php_symbol_key("__clone");
            class_info.methods.contains_key(&clone_key)
                || class_info.method_impl_classes.contains_key(&clone_key)
        });
    let mut candidates = ctx
        .module
        .class_infos
        .iter()
        .filter(|(class_name, _)| {
            !is_runtime_managed_object_clone_class(class_name)
                && (normalized_static_type.eq_ignore_ascii_case("object")
                    || reflection::reflection_class_matches_object_type(
                        ctx,
                        class_name,
                        normalized_static_type,
                    ))
        })
        .map(|(class_name, class_info)| {
            let clone_key = php_symbol_key("__clone");
            // A `__clone()` the STATIC type already resolves was emitted as an ordinary method
            // call by the IR lowering, which dispatches on the cloned object. Emitting the hook
            // again here would run it twice.
            let clone_impl = (!static_type_resolves_clone_hook)
                .then(|| class_info.method_impl_classes.get(&clone_key).cloned())
                .flatten();
            (class_name.clone(), class_info.class_id, clone_impl)
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(_, class_id, _)| *class_id);

    let done_label = ctx.next_label("object_clone_runtime_done");
    let miss_label = ctx.next_label("object_clone_runtime_miss");
    let match_labels = candidates
        .iter()
        .map(|(class_name, _, _)| {
            ctx.next_label(&format!("object_clone_runtime_{}", label_fragment(class_name)))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(source, abi::int_result_reg(ctx.emitter))?;
    emit_runtime_clone_class_dispatch(ctx, &candidates, &match_labels, &miss_label);

    for ((class_name, _, clone_impl), label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        lower_known_class_object_clone(ctx, inst, source, class_name)?;
        if let Some(impl_class) = clone_impl {
            emit_runtime_clone_method_call(ctx, inst, impl_class)?;
        }
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    match miss {
        CloneMiss::Fatal => {
            let message = format!(
                "Fatal error: Cannot clone runtime class through static type {}\n",
                normalized_static_type
            );
            emit_fatal_message(ctx, message.as_bytes());
        }
        CloneMiss::StaticLayout => {
            lower_known_class_object_clone(ctx, inst, source, static_type)?;
        }
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Branches to the concrete clone lowering selected by the source object's runtime class id.
fn emit_runtime_clone_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    candidates: &[(String, u64, Option<String>)],
    match_labels: &[String],
    miss_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the source object's runtime class id
            for ((_, class_id, _), label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", *class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare against this cloneable concrete class
                ctx.emitter.instruction(&format!("b.eq {}", label));            // clone with the matching concrete object layout
            }
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r11, QWORD PTR [rax]");                // load the source object's runtime class id
            for ((_, class_id, _), label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", *class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare against this cloneable concrete class
                ctx.emitter.instruction(&format!("je {}", label));              // clone with the matching concrete object layout
            }
        }
    }
    abi::emit_jump(ctx.emitter, miss_label);
}

/// Invokes the concrete class's zero-argument `__clone` hook on the stored clone result.
fn emit_runtime_clone_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    impl_class: &str,
) -> Result<()> {
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("object_clone_shallow missing result value"))?;
    let receiver_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(result, receiver_reg)?;
    abi::emit_call_label(ctx.emitter, &method_symbol(impl_class, &php_symbol_key("__clone")));
    Ok(())
}

/// Lowers one clone using a statically selected concrete object layout.
fn lower_known_class_object_clone(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: ValueId,
    class_name: &str,
) -> Result<()> {
    if is_builtin_stdclass(class_name) {
        return lower_stdclass_clone(ctx, inst, source);
    }
    if is_runtime_managed_object_clone_class(class_name) {
        return Err(CodegenIrError::unsupported(format!(
            "clone for runtime-managed class {}",
            class_name
        )));
    }
    let (
        class_id,
        property_count,
        allow_dynamic_properties,
        retained_offsets,
        persisted_offsets,
        owned_reference_property_offsets,
    ) = {
        let class_info =
            ctx.module.class_infos.get(class_name).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", class_name))
            })?;
        let retained_offsets = cloned_property_retain_offsets(class_info);
        let persisted_offsets = cloned_property_persist_offsets(class_info);
        let owned_reference_property_offsets = owned_reference_property_offsets(class_info);
        (
            class_info.class_id,
            class_info.properties.len(),
            class_info.has_property_hash_storage(),
            retained_offsets,
            persisted_offsets,
            owned_reference_property_offsets,
        )
    };
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("object_clone_shallow missing result value"))?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(source, result_reg)?;
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        allow_dynamic_properties,
        &[],
        &[],
    )?;
    ctx.store_result_value(result)?;
    let source_reg = abi::secondary_scratch_reg(ctx.emitter);
    let dest_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, source_reg);
    ctx.load_value_to_reg(result, dest_reg)?;
    if let Some(offset) = crate::codegen_support::throwable_layout::ordinary_creation_line_offset_by_id(
        class_id, &ctx.module.class_infos,
    ) {
        let scratch = abi::tertiary_scratch_reg(ctx.emitter);
        let stem = ctx.next_label("throwable_clone_line");
        crate::codegen_support::runtime::emit_creation_line_load(
            ctx.emitter, source_reg, result_reg, scratch, &stem,
        );
        abi::emit_store_to_address(ctx.emitter, result_reg, dest_reg, offset);
    }
    emit_clone_declared_property_slots(
        ctx,
        source_reg,
        dest_reg,
        property_count,
        &retained_offsets,
        &persisted_offsets,
        &owned_reference_property_offsets,
    );
    if allow_dynamic_properties {
        emit_clone_dynamic_property_hash(
            ctx,
            source_reg,
            dest_reg,
            dynamic_property_hash_offset(property_count),
        );
    }
    Ok(())
}

/// Lowers `clone` for `stdClass`, whose payload is just class id plus dynamic-property hash.
pub(super) fn lower_stdclass_clone(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: ValueId,
) -> Result<()> {
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("stdClass clone missing result value"))?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(source, result_reg)?;
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_new");
    ctx.store_result_value(result)?;
    let source_reg = abi::secondary_scratch_reg(ctx.emitter);
    let dest_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, source_reg);
    ctx.load_value_to_reg(result, dest_reg)?;
    emit_clone_dynamic_property_hash(ctx, source_reg, dest_reg, 8);
    Ok(())
}

/// Lowers `new stdClass()` through the runtime helper that seeds its dynamic-property hash.
pub(super) fn lower_stdclass_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "stdClass constructor with {} EIR operands",
            inst.operands.len()
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_new");
    store_if_result(ctx, inst)
}

/// Returns true when the class uses the runtime-managed SPL doubly-linked-list payload.
pub(super) fn is_spl_doubly_linked_list_family(class_name: &str) -> bool {
    matches!(class_name, "SplDoublyLinkedList" | "SplStack" | "SplQueue")
}

/// Returns true for object classes whose payload is not the generic declared-property layout.
pub(super) fn is_runtime_managed_object_clone_class(class_name: &str) -> bool {
    let class_name = class_name.trim_start_matches('\\');
    is_fiber_class(class_name)
        || class_name == "Generator"
        || reflection::is_reflection_owner_class(class_name)
        || class_name == "CallbackFilterIterator"
        || class_name == "RecursiveCallbackFilterIterator"
        || class_name == "IteratorIterator"
        || is_spl_doubly_linked_list_family(class_name)
        || class_name == "SplFixedArray"
}

/// Lowers `new SplDoublyLinkedList`, `new SplStack`, and `new SplQueue`.
pub(super) fn lower_spl_doubly_linked_list_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} constructor with {} EIR operands",
            class_name,
            inst.operands.len()
        )));
    }
    let class_id = ctx
        .module
        .class_infos
        .get(class_name)
        .map(|info| info.class_id)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
    let default_mode = match class_name {
        "SplStack" => SPL_ITER_MODE_LIFO | SPL_ITER_MODE_FIX,
        "SplQueue" => SPL_ITER_MODE_FIX,
        _ => 0,
    };
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        class_id as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        default_mode,
    );
    abi::emit_call_label(ctx.emitter, "__rt_spl_dll_new");
    store_if_result(ctx, inst)
}

/// Lowers `new SplFixedArray($size = 0)` through the runtime-backed payload allocator.
pub(super) fn lower_spl_fixed_array_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() > 1 {
        return Err(CodegenIrError::unsupported(format!(
            "SplFixedArray constructor with {} EIR operands",
            inst.operands.len()
        )));
    }
    let class_id = ctx
        .module
        .class_infos
        .get("SplFixedArray")
        .map(|info| info.class_id)
        .ok_or_else(|| CodegenIrError::unsupported("unknown class SplFixedArray"))?;
    if let Some(size) = inst.operands.first().copied() {
        ctx.load_value_to_result(size)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            class_id as i64,
        );
        abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1));
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            class_id as i64,
        );
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 0);
    }
    abi::emit_call_label(ctx.emitter, "__rt_spl_fixed_new");
    store_if_result(ctx, inst)
}

/// Lowers `new CallbackFilterIterator($iterator, $callback)` with callable-array capture.
pub(super) fn lower_callback_filter_iterator_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::unsupported(format!(
            "{} constructor with {} EIR operands",
            class_name,
            inst.operands.len()
        )));
    }
    let source = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let (
        class_id,
        property_count,
        uninitialized_marker_offsets,
        property_initialization,
        callback_env_offset,
    ) = {
        let class_info =
            ctx.module.class_infos.get(class_name).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", class_name))
            })?;
        if class_info.has_property_hash_storage() {
            return Err(CodegenIrError::unsupported(format!(
                "object allocation requiring dynamic properties for {}",
                class_name
            )));
        }
        if class_interfaces_require_missing_method_symbols(ctx, class_name, class_info) {
            return Err(CodegenIrError::unsupported(format!(
                "object allocation requiring interface method symbols not emitted by EIR for {}",
                class_name
            )));
        }
        (
            class_info.class_id,
            class_info.properties.len(),
            uninitialized_property_marker_offsets(class_info),
            collect_property_initialization(ctx.module, class_name, class_info, inst),
            class_info.property_offsets.get("callbackEnv").copied(),
        )
    };
    let inner_slot = resolve_property_slot_for_class(ctx, class_name, "inner", inst)?;
    let callback_slot = resolve_property_slot_for_class(ctx, class_name, "callback", inst)?;
    // `$callback` is declared untyped, so its schema default is a boxed null, but its storage is
    // a callable descriptor (`runtime_property_type_override`). The constructor store below
    // releases the slot's previous value AS a descriptor, so a boxed null left there was read
    // past its end and freed through the descriptor path: a crash whenever the bytes after the
    // box happened to look like a capture table. The allocation's zero word is the descriptor
    // slot's null, and the store overwrites it at once.
    // (A class whose defaults need the init thunk keeps the thunk: it assigns `$callback` too.)
    let property_initialization = match property_initialization {
        PropertyInitialization::Inline(defaults) => PropertyInitialization::Inline(
            defaults
                .into_iter()
                .filter(|default| default.offset != callback_slot.offset)
                .collect::<Vec<_>>(),
        ),
        thunk => thunk,
    };
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
        &[],
    )?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("object_new missing result value"))?;
    ctx.store_result_value(result)?;
    emit_property_initialization(ctx, result, &property_initialization)?;
    if let Some(offset) = callback_env_offset {
        emit_zero_pointer_property(ctx, result, offset)?;
    }
    emit_callback_filter_source_property(ctx, source, result, &inner_slot, inst)?;
    emit_callback_filter_callback_property(ctx, callback, result, &callback_slot, inst)
}

/// Stores CallbackFilterIterator::$inner from a constructor source operand.
pub(super) fn emit_callback_filter_source_property(
    ctx: &mut FunctionContext<'_>,
    source: ValueId,
    target: ValueId,
    slot: &PropertySlot,
    inst: &Instruction,
) -> Result<()> {
    let value_ty = ctx.value_php_type(source)?;
    ensure_property_value_supported(ctx, slot, source, &value_ty, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(target, base_reg)?;
    emit_property_store(ctx, source, slot, base_reg)
}

/// Stores CallbackFilterIterator::$callback, converting callable arrays to descriptors.
pub(super) fn emit_callback_filter_callback_property(
    ctx: &mut FunctionContext<'_>,
    callback: ValueId,
    target: ValueId,
    slot: &PropertySlot,
    inst: &Instruction,
) -> Result<()> {
    let value_ty = ctx.value_php_type(callback)?;
    match value_ty.codegen_repr() {
        PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Str) => {
            callables::emit_runtime_callable_array_descriptor_value(
                ctx,
                callback,
                "callback_filter_constructor",
            )?;
            emit_store_result_to_pointer_property(ctx, target, slot.offset)
        }
        _ => {
            ensure_property_value_supported(ctx, slot, callback, &value_ty, inst)?;
            let base_reg = abi::symbol_scratch_reg(ctx.emitter);
            ctx.load_value_to_reg(target, base_reg)?;
            emit_property_store(ctx, callback, slot, base_reg)
        }
    }
}

/// Stores the current single-register result into one pointer-sized object property.
pub(super) fn emit_store_result_to_pointer_property(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    offset: usize,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    ctx.load_value_to_reg(target, base_reg)?;
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, base_reg, offset);
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, offset + 8);
    Ok(())
}

/// Initializes one pointer-sized object property to null.
pub(super) fn emit_zero_pointer_property(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    offset: usize,
) -> Result<()> {
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(target, base_reg)?;
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, offset);
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, offset + 8);
    Ok(())
}
