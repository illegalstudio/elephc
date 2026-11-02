//! Purpose:
//! Lowers IteratorIterator construction and Traversable normalization.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Optional downcast checks remain target-aware and PHP-compatible.

use super::*;

/// Lowers `new IteratorIterator($iterator)` by normalizing a Traversable source to an Iterator.
pub(super) fn lower_iterator_iterator_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    let source_ty = ctx.value_php_type(source)?.codegen_repr();
    let PhpType::Object(source_name) = &source_ty else {
        // php's answer for a SCALAR argument is a TypeError, and emitting it is both what php does
        // and the only honest lowering: the constructor has no Traversable to wrap. Refusing the
        // whole program instead is what a fabricated scalar parameter type turned into a build
        // failure -- `twig/twig`'s `CoreExtension::filter($env, $isSandboxed, $array, $arrow)` has
        // no compiled caller, its untyped parameters keep the `int` inference seed, and
        // `new \IteratorIterator($array)` is reached only past `if (!is_iterable($array)) throw`,
        // so the throw this emits stands where the guard already made the code unreachable.
        if let Some(given) = scalar_type_error_name(&source_ty) {
            exceptions::emit_type_error(
                ctx,
                &format!(
                    "IteratorIterator::__construct(): Argument #1 ($iterator) must be of type Traversable, {} given",
                    given
                ),
            );
            return store_if_result(ctx, inst);
        }
        return Err(CodegenIrError::unsupported(format!(
            "IteratorIterator source PHP type {:?}",
            source_ty
        )));
    };
    if !object_type_is_a(ctx, source_name, "Traversable") {
        return Err(CodegenIrError::unsupported(format!(
            "IteratorIterator Traversable normalization for PHP type {:?}",
            source_ty
        )));
    }
    let class_info = ctx
        .module
        .class_infos
        .get("IteratorIterator")
        .ok_or_else(|| CodegenIrError::unsupported("unknown class IteratorIterator"))?;
    if class_info.has_property_hash_storage() {
        return Err(CodegenIrError::unsupported(
            "object allocation requiring dynamic properties for IteratorIterator",
        ));
    }
    if class_interfaces_require_missing_method_symbols(ctx, "IteratorIterator", class_info) {
        return Err(CodegenIrError::unsupported(
            "object allocation requiring interface method symbols not emitted by EIR for IteratorIterator",
        ));
    }
    let inner_offset = class_info
        .property_offsets
        .get("inner")
        .copied()
        .ok_or_else(|| CodegenIrError::missing_entry("property offset", 0))?;
    let inner_ty = class_info
        .properties
        .iter()
        .find(|(name, _)| name == "inner")
        .map(|(_, ty)| ty.clone())
        .ok_or_else(|| CodegenIrError::missing_entry("property inner", 0))?;
    let class_id = class_info.class_id;
    let property_count = class_info.properties.len();
    let uninitialized_marker_offsets = uninitialized_property_marker_offsets(class_info);
    let slot = PropertySlot {
        class_name: "IteratorIterator".to_string(),
        property: "inner".to_string(),
        php_type: inner_ty,
        offset: inner_offset,
        is_declared: true,
        is_packed: false,
        width: None,
        is_reference: false,
        abstract_setter_type_error_prefix: None,
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
    emit_iterator_iterator_inner_from_traversable(
        ctx,
        source,
        inst.operands.get(1).copied(),
        result,
        &slot,
    )
}

/// Returns php's name for a scalar type in a TypeError message, or `None` for anything else.
fn scalar_type_error_name(php_type: &PhpType) -> Option<&'static str> {
    match php_type {
        PhpType::Int | PhpType::TaggedScalar => Some("int"),
        PhpType::Float => Some("float"),
        PhpType::Bool | PhpType::False => Some("bool"),
        PhpType::Str => Some("string"),
        PhpType::Void => Some("null"),
        _ => None,
    }
}

/// Stores IteratorIterator::$inner after converting IteratorAggregate inputs through getIterator().
pub(super) fn emit_iterator_iterator_inner_from_traversable(
    ctx: &mut FunctionContext<'_>,
    source: ValueId,
    downcast: Option<ValueId>,
    target: ValueId,
    slot: &PropertySlot,
) -> Result<()> {
    emit_push_iterator_iterator_downcast_status(ctx, downcast)?;
    ctx.load_value_to_result(source)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let direct_case = ctx.next_label("iterator_iterator_source_iterator");
    let aggregate_case = ctx.next_label("iterator_iterator_source_aggregate");
    let done = ctx.next_label("iterator_iterator_source_done");
    emit_branch_if_saved_traversable_implements(ctx, "Iterator", &direct_case)?;
    emit_branch_if_saved_traversable_implements(ctx, "IteratorAggregate", &aggregate_case)?;
    let owned_case = ctx.next_label("iterator_iterator_source_unwrapped");
    let bridge = ctx.module.required_runtime_features.eval_bridge;
    if bridge {
        // A class the interpreter declared has no compiled class id, so neither check above can
        // see its interfaces. The interpreter already refused a non-Traversable at the parameter
        // boundary, so an object it owns is an IteratorAggregate (the bridge hands over what
        // `getIterator()` returns, which `$inner` wraps) or an Iterator (kept as `$inner`, whose
        // methods are dispatched through the bridge like every other call on such an object).
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            0,
        );
        let unwrap_symbol = ctx
            .emitter
            .target
            .extern_symbol("__elephc_eval_unwrap_aggregate");
        abi::emit_call_label(ctx.emitter, &unwrap_symbol);
        abi::emit_branch_if_int_result_nonzero(ctx.emitter, &owned_case);
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            0,
        );
        let owner_probe = ctx
            .emitter
            .target
            .extern_symbol("__elephc_eval_object_has_dynamic_owner");
        abi::emit_call_label(ctx.emitter, &owner_probe);
        abi::emit_branch_if_int_result_nonzero(ctx.emitter, &direct_case);
    }
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&direct_case);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_incref_if_refcounted(ctx.emitter, &slot.php_type.codegen_repr());
    abi::emit_jump(ctx.emitter, &done);

    if bridge {
        // The bridge returned the aggregate's traversable with a reference this object now owns.
        ctx.emitter.label(&owned_case);
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        abi::emit_jump(ctx.emitter, &done);
    }

    ctx.emitter.label(&aggregate_case);
    emit_validate_iterator_iterator_aggregate_downcast(ctx)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    move_result_to_receiver_arg(ctx);
    iterators::emit_interface_dispatch_call(ctx, "IteratorAggregate", "getiterator", None)?;

    ctx.emitter.label(&done);
    emit_iterator_inner_property_from_result(ctx, target, slot.offset)
}

/// Pushes `[status, class_id]` metadata for IteratorIterator's optional downcast argument.
pub(super) fn emit_push_iterator_iterator_downcast_status(
    ctx: &mut FunctionContext<'_>,
    downcast: Option<ValueId>,
) -> Result<()> {
    let Some(value) = downcast else {
        emit_push_iterator_iterator_downcast_status_pair(ctx, 0, 0);
        return Ok(());
    };
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
            abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
            emit_push_iterator_iterator_downcast_status_from_lookup(ctx);
        }
        PhpType::Void | PhpType::Never => {
            emit_push_iterator_iterator_downcast_status_pair(ctx, 0, 0);
        }
        // The omitted optional `?string $class` reaches the constructor as a BOXED null, so the
        // static type says nothing: only a null at run time means "no downcast requested". Taking
        // every boxed value for an invalid downcast made `new IteratorIterator($aggregate)` throw
        // "Class to downcast to not found..." for every IteratorAggregate source.
        // A given class name arrives boxed the same way, so a string tag is resolved exactly like
        // a `string` operand; taking every non-null box for an invalid name made
        // `new IteratorIterator(new ArrayObject(...), "ArrayObject")` throw.
        PhpType::Mixed | PhpType::Union(_) => {
            let null_label = ctx.next_label("iterator_iterator_downcast_null");
            let string_label = ctx.next_label("iterator_iterator_downcast_string");
            let done_label = ctx.next_label("iterator_iterator_downcast_done");
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #8");                      // runtime tag 8 is null: no downcast class was given
                    ctx.emitter.instruction(&format!("b.eq {}", null_label));   // omitted or null class: no downcast constraint
                    ctx.emitter.instruction("cmp x0, #1");                      // runtime tag 1 is a class-name string to resolve
                    ctx.emitter.instruction(&format!("b.eq {}", string_label)); // resolve the boxed class name like a string operand
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 8");                      // runtime tag 8 is null: no downcast class was given
                    ctx.emitter.instruction(&format!("je {}", null_label));     // omitted or null class: no downcast constraint
                    ctx.emitter.instruction("cmp rax, 1");                      // runtime tag 1 is a class-name string to resolve
                    ctx.emitter.instruction(&format!("je {}", string_label));   // resolve the boxed class name like a string operand
                }
            }
            emit_push_iterator_iterator_downcast_status_pair(ctx, 2, 0);
            abi::emit_jump(ctx.emitter, &done_label);
            ctx.emitter.label(&string_label);
            if ctx.emitter.target.arch == Arch::X86_64 {
                ctx.emitter.instruction("mov rax, rdi");                        // the unboxed string pointer becomes the lookup's string pointer
            }
            abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
            emit_push_iterator_iterator_downcast_status_from_lookup(ctx);
            abi::emit_jump(ctx.emitter, &done_label);
            ctx.emitter.label(&null_label);
            emit_push_iterator_iterator_downcast_status_pair(ctx, 0, 0);
            ctx.emitter.label(&done_label);
        }
        _ => {
            emit_push_iterator_iterator_downcast_status_pair(ctx, 2, 0);
        }
    }
    Ok(())
}

/// Pushes downcast status metadata after `__rt_instanceof_lookup` returned target metadata.
pub(super) fn emit_push_iterator_iterator_downcast_status_from_lookup(ctx: &mut FunctionContext<'_>) {
    let invalid = ctx.next_label("iterator_iterator_downcast_lookup_invalid");
    let done = ctx.next_label("iterator_iterator_downcast_lookup_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the downcast class-string resolve to metadata?
            ctx.emitter.instruction(&format!("b.eq {}", invalid));              // invalid downcast names throw for IteratorAggregate inputs
            ctx.emitter.instruction("cmp x2, #0");                              // target kind 0 means a concrete class, not an interface
            ctx.emitter.instruction(&format!("b.ne {}", invalid));              // interface names are invalid downcast targets
            ctx.emitter.instruction("mov x0, #1");                              // status 1 means x1 carries a concrete downcast class id
            ctx.emitter.instruction(&format!("b {}", done));                    // preserve the resolved class id for later validation

            ctx.emitter.label(&invalid);
            ctx.emitter.instruction("mov x0, #2");                              // status 2 means the downcast must throw for aggregates
            ctx.emitter.instruction("mov x1, #0");                              // invalid downcast targets have no usable class id
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the downcast class-string resolve to metadata?
            ctx.emitter.instruction(&format!("je {}", invalid));                // invalid downcast names throw for IteratorAggregate inputs
            ctx.emitter.instruction("test rdx, rdx");                           // target kind 0 means a concrete class, not an interface
            ctx.emitter.instruction(&format!("jne {}", invalid));               // interface names are invalid downcast targets
            ctx.emitter.instruction("mov rax, 1");                              // status 1 means rdi carries a concrete downcast class id
            ctx.emitter.instruction(&format!("jmp {}", done));                  // preserve the resolved class id for later validation

            ctx.emitter.label(&invalid);
            ctx.emitter.instruction("mov rax, 2");                              // status 2 means the downcast must throw for aggregates
            ctx.emitter.instruction("xor edi, edi");                            // invalid downcast targets have no usable class id
        }
    }
    ctx.emitter.label(&done);
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg_pair(ctx.emitter, "x0", "x1"),
        Arch::X86_64 => abi::emit_push_reg_pair(ctx.emitter, "rax", "rdi"),
    }
}

/// Pushes a literal IteratorIterator downcast status pair.
pub(super) fn emit_push_iterator_iterator_downcast_status_pair(
    ctx: &mut FunctionContext<'_>,
    status: i64,
    class_id: i64,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", status);
            abi::emit_load_int_immediate(ctx.emitter, "x1", class_id);
            abi::emit_push_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rax", status);
            abi::emit_load_int_immediate(ctx.emitter, "rdi", class_id);
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdi");
        }
    }
}

/// Validates the optional downcast metadata before wrapping an IteratorAggregate source.
pub(super) fn emit_validate_iterator_iterator_aggregate_downcast(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let aggregate_interface_id = interface_info_by_name(ctx, "IteratorAggregate")
        .ok_or_else(|| CodegenIrError::unsupported("missing interface IteratorAggregate"))?
        .interface_id as i64;
    let skip = ctx.next_label("iterator_iterator_downcast_skip");
    let throw = ctx.next_label("iterator_iterator_downcast_throw");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #16]");                       // load downcast status: 0 omitted/null, 1 class id, 2 invalid
            ctx.emitter.instruction(&format!("cbz x9, {}", skip));              // omitted/null class arguments do not constrain aggregates
            ctx.emitter.instruction("cmp x9, #1");                              // only status 1 carries a valid concrete class id
            ctx.emitter.instruction(&format!("b.ne {}", throw));                // invalid names and interfaces throw for aggregates
            ctx.emitter.instruction("ldr x0, [sp]");                            // pass the saved IteratorAggregate object to the class matcher
            ctx.emitter.instruction("ldr x1, [sp, #24]");                       // pass the requested downcast class id to the class matcher
            abi::emit_load_int_immediate(ctx.emitter, "x2", 0);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            ctx.emitter.instruction("cmp x0, #0");                              // did the aggregate object match the requested class?
            ctx.emitter.instruction(&format!("b.eq {}", throw));                // non-base downcast classes are rejected like PHP
            ctx.emitter.instruction("ldr x0, [sp, #24]");                       // pass the requested class id to the interface checker
            abi::emit_load_int_immediate(ctx.emitter, "x1", aggregate_interface_id);
            abi::emit_call_label(ctx.emitter, "__rt_class_implements_interface");
            ctx.emitter.instruction("cmp x0, #0");                              // did the downcast class implement IteratorAggregate?
            ctx.emitter.instruction(&format!("b.eq {}", throw));                // non-Traversable base classes are rejected like PHP
            ctx.emitter.instruction(&format!("b {}", skip));                    // the aggregate downcast class is valid
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 16]");           // load downcast status: 0 omitted/null, 1 class id, 2 invalid
            ctx.emitter.instruction("test r10, r10");                           // is there an explicit downcast class to validate?
            ctx.emitter.instruction(&format!("je {}", skip));                   // omitted/null class arguments do not constrain aggregates
            ctx.emitter.instruction("cmp r10, 1");                              // only status 1 carries a valid concrete class id
            ctx.emitter.instruction(&format!("jne {}", throw));                 // invalid names and interfaces throw for aggregates
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // pass the saved IteratorAggregate object to the class matcher
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");           // pass the requested downcast class id to the class matcher
            abi::emit_load_int_immediate(ctx.emitter, "rdx", 0);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            ctx.emitter.instruction("test rax, rax");                           // did the aggregate object match the requested class?
            ctx.emitter.instruction(&format!("je {}", throw));                  // non-base downcast classes are rejected like PHP
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");           // pass the requested class id to the interface checker
            abi::emit_load_int_immediate(ctx.emitter, "rsi", aggregate_interface_id);
            abi::emit_call_label(ctx.emitter, "__rt_class_implements_interface");
            ctx.emitter.instruction("test rax, rax");                           // did the downcast class implement IteratorAggregate?
            ctx.emitter.instruction(&format!("je {}", throw));                  // non-Traversable base classes are rejected like PHP
            ctx.emitter.instruction(&format!("jmp {}", skip));                  // the aggregate downcast class is valid
        }
    }

    ctx.emitter.label(&throw);
    emit_throw_iterator_iterator_downcast_logic_exception(ctx);
    ctx.emitter.label(&skip);
    Ok(())
}

/// Throws the LogicException required for invalid IteratorIterator aggregate downcasts.
pub(super) fn emit_throw_iterator_iterator_downcast_logic_exception(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            crate::codegen_support::throwable_layout::emit_allocate(ctx.emitter, crate::codegen_support::throwable_layout::PAYLOAD_SIZE);
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 marks object instances
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp allocation as a runtime object
            ctx.emitter.instruction("bl __rt_object_handle_acquire");           // bind the new object to its PHP object handle
            abi::emit_symbol_address(ctx.emitter, "x9", "_spl_logic_exception_class_id");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load LogicException's runtime class id
            ctx.emitter.instruction("str x9, [x0]");                            // store the class id at object header
            abi::emit_symbol_address(ctx.emitter, "x9", "_iterator_iterator_downcast_msg");
            ctx.emitter.instruction("str x9, [x0, #8]");                        // store static exception message pointer
            ctx.emitter.instruction(&format!(                                   // materialize the exception message length
                "mov x9, #{}",
                ITERATOR_ITERATOR_DOWNCAST_MESSAGE.len()
            ));                                                                 // load static exception message length
            ctx.emitter.instruction("str x9, [x0, #16]");                       // store static exception message length
            ctx.emitter.instruction("str xzr, [x0, #24]");                      // exception code defaults to zero
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(ctx.emitter, "x0");
            ctx.emitter.instruction(&format!("str xzr, [x0, #{}]", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET)); // previous defaults to null
            abi::emit_symbol_address(ctx.emitter, "x9", "_exc_value");
            ctx.emitter.instruction("str x0, [x9]");                            // publish the active exception object
            ctx.emitter.instruction("b __rt_throw_current");                    // enter the standard exception unwinder
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rbp");                                // preserve caller frame pointer for exception allocation
            ctx.emitter.instruction("mov rbp, rsp");                            // establish an aligned helper frame
            ctx.emitter.instruction("sub rsp, 16");                             // keep the nested heap allocation call aligned
            crate::codegen_support::throwable_layout::emit_allocate(ctx.emitter, crate::codegen_support::throwable_layout::PAYLOAD_SIZE);
            ctx.emitter.instruction(
                &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))
            );                                                                  // stamp the canonical x86_64 heap-kind word (magic + kind 6 throwable)
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp allocation as a runtime object
            ctx.emitter.instruction("call __rt_object_handle_acquire");         // bind the new object to its PHP object handle
            ctx.emitter
                .instruction("mov r10, QWORD PTR [rip + _spl_logic_exception_class_id]"); // load LogicException's runtime class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the class id at object header
            ctx.emitter
                .instruction("lea r10, [rip + _iterator_iterator_downcast_msg]"); // materialize static exception message pointer
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store static exception message pointer
            ctx.emitter.instruction(&format!(                                   // store the exception message length
                "mov QWORD PTR [rax + 16], {}",
                ITERATOR_ITERATOR_DOWNCAST_MESSAGE.len()
            ));                                                                 // store static exception message length
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");             // exception code defaults to zero
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(ctx.emitter, "rax");
            ctx.emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET)); // previous defaults to null
            ctx.emitter
                .instruction("mov QWORD PTR [rip + _exc_value], rax"); // publish the active exception object
            ctx.emitter.instruction("mov rsp, rbp");                            // release helper frame before throwing
            ctx.emitter.instruction("pop rbp");                                 // restore caller frame pointer before throwing
            ctx.emitter.instruction("jmp __rt_throw_current");                  // enter the standard exception unwinder
        }
    }
}

/// Branches when the saved Traversable candidate implements the requested interface.
pub(super) fn emit_branch_if_saved_traversable_implements(
    ctx: &mut FunctionContext<'_>,
    interface_name: &str,
    target_label: &str,
) -> Result<()> {
    let interface_id = interface_info_by_name(ctx, interface_name)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!("missing interface {}", interface_name))
        })?
        .interface_id as i64;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
            abi::emit_load_int_immediate(ctx.emitter, "x1", interface_id);
            abi::emit_load_int_immediate(ctx.emitter, "x2", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the saved Traversable matches this interface
            ctx.emitter.instruction(&format!("b.ne {}", target_label));         // select the matching IteratorIterator normalization path
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", interface_id);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            ctx.emitter.instruction("test rax, rax");                           // test whether the saved Traversable matches this interface
            ctx.emitter.instruction(&format!("jne {}", target_label));          // select the matching IteratorIterator normalization path
        }
    }
    Ok(())
}

/// Moves the object result into the receiver ABI slot before an interface method call.
pub(super) fn move_result_to_receiver_arg(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the normalized object result as the method receiver
    }
}

/// Writes the normalized Iterator pointer into IteratorIterator::$inner.
pub(super) fn emit_iterator_inner_property_from_result(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    inner_offset: usize,
) -> Result<()> {
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    let tag_reg = abi::secondary_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(target, base_reg)?;
    abi::emit_store_to_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        base_reg,
        inner_offset,
    );
    abi::emit_load_int_immediate(ctx.emitter, tag_reg, 6);
    abi::emit_store_to_address(ctx.emitter, tag_reg, base_reg, inner_offset + 8);
    Ok(())
}
