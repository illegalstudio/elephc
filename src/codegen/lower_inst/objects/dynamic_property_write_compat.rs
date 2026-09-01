//! Purpose:
//! Dynamic-property write compatibility, indirect mutation, and diagnostics.
//!
//! Called from:
//! - Object property write entry and fetch-for-write lowering.
//!
//! Key details:
//! - Emits creation deprecations only for new keys and respects error_reporting.

use super::*;

/// Fetches an allow-dynamic property cell for an indirect array write, creating boxed null on miss.
pub(in crate::codegen::lower_inst) fn lower_dynamic_property_fetch_for_write(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    key: ValueId,
) -> Result<()> {
    let property = const_string_operand(ctx, key)?
        .map(str::to_owned)
        .ok_or_else(|| {
            CodegenIrError::unsupported(
                "dynamic-property fetch-for-write with non-constant property name",
            )
        })?;
    let hash_offset = dynamic_property_hash_offset_for_object(ctx, object, &property)?
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "array fetch-for-write on non-dynamic object property ${}",
                property
            ))
        })?;
    emit_dynamic_property_creation_deprecation_if_missing(
        ctx,
        object,
        &property,
        hash_offset,
        inst.span,
    )?;

    let (key_label, key_len) = ctx.data.add_string(property.as_bytes());
    let miss_label = ctx.next_label("dynamic_prop_fetch_for_write_miss");
    let done_label = ctx.next_label("dynamic_prop_fetch_for_write_done");
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x0, [{}, #{}]", object_reg, hash_offset)); // load the dynamic-property side table
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction(&format!("cbz x0, {}", miss_label));       // create a persistent null cell when the key is absent
            ctx.emitter.instruction("mov x0, x1");                             // return the stored boxed Mixed cell for in-place mutation
            abi::emit_jump(ctx.emitter, &done_label);

            ctx.emitter.label(&miss_label);
            emit_boxed_null(ctx);
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.load_value_to_reg(object, object_reg)?;
            ctx.emitter
                .instruction(&format!("ldr x0, [{}, #{}]", object_reg, hash_offset)); // pass the current side table to hash_set
            abi::emit_push_reg(ctx.emitter, object_reg);
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x3", 16);
            ctx.emitter.instruction("mov x4, xzr");                            // boxed Mixed entries use only the low payload word
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x5",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "x0", object_reg, hash_offset);
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "mov rdi, QWORD PTR [{} + {}]",
                object_reg, hash_offset
            )); // load the dynamic-property side table
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction("test rax, rax");                          // check whether the property cell already exists
            ctx.emitter.instruction(&format!("jz {}", miss_label));            // create a persistent null cell on miss
            ctx.emitter.instruction("mov rax, rdi");                           // return the stored boxed Mixed cell for in-place mutation
            abi::emit_jump(ctx.emitter, &done_label);

            ctx.emitter.label(&miss_label);
            emit_boxed_null(ctx);
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.load_value_to_reg(object, object_reg)?;
            ctx.emitter.instruction(&format!(
                "mov rdi, QWORD PTR [{} + {}]",
                object_reg, hash_offset
            )); // pass the current side table to hash_set
            abi::emit_push_reg(ctx.emitter, object_reg);
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rcx", 16);
            ctx.emitter.instruction("xor r8, r8");                             // boxed Mixed entries use only the low payload word
            abi::emit_load_int_immediate(
                ctx.emitter,
                "r9",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "rax", object_reg, hash_offset);
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Emits ext/date's PHP 8.2+ deprecation only when a dynamic-property key is first created.
pub(super) fn emit_dynamic_property_creation_deprecation_if_missing(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    hash_offset: usize,
    span: Option<crate::span::Span>,
) -> Result<()> {
    let PhpType::Object(class_name) = ctx.value_php_type(object)?.codegen_repr() else {
        return Ok(());
    };
    let normalized = class_name.trim_start_matches('\\');
    if !ctx
        .module
        .class_infos
        .get(normalized)
        .is_some_and(|info| info.dynamic_properties_deprecated)
    {
        return Ok(());
    }
    let present_label = ctx.next_label("dynamic_prop_deprecation_present");
    let (key_label, key_len) = ctx.data.add_string(property.as_bytes());
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x0, [{}, #{}]", object_reg, hash_offset)); // inspect the existing dynamic-property side table before storing
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction(&format!("cbnz x0, {}", present_label));   // reassignment of an existing dynamic property is not deprecated again
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "mov rdi, QWORD PTR [{} + {}]",
                object_reg, hash_offset
            )); // inspect the existing dynamic-property side table before storing
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction("test rax, rax");                          // did the property already exist?
            ctx.emitter.instruction(&format!("jnz {}", present_label));       // reassignments do not emit the creation deprecation
        }
    }
    let display_class = dynamic_property_diagnostic_class_name(ctx, normalized);
    emit_dynamic_property_deprecation(ctx, &display_class, property, span);
    ctx.emitter.label(&present_label);
    Ok(())
}

/// Reconstructs php-src's user-facing anonymous descendant name for diagnostics.
pub(super) fn dynamic_property_diagnostic_class_name(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> String {
    if !class_name.starts_with("class@anonymous#") {
        return class_name.to_string();
    }
    ctx.module
        .class_infos
        .get(class_name)
        .and_then(|info| info.parent.as_deref())
        .map_or_else(
            || "class@anonymous".to_string(),
            |parent| format!("{}@anonymous", parent.trim_start_matches('\\')),
        )
}

/// Writes one suppression- and error-reporting-aware dynamic-property deprecation line.
pub(super) fn emit_dynamic_property_deprecation(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
    span: Option<crate::span::Span>,
) {
    let line = span.map_or(0, |span| span.line);
    let source = ctx.module.source_path.as_deref().unwrap_or("Unknown");
    let message = format!(
        "\nDeprecated: Creation of dynamic property {}::${} is deprecated in {} on line {}\n",
        class_name, property, source, line
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    let masked_label = ctx.next_label("dynamic_prop_deprecation_masked");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_rt_error_reporting", 0);
            abi::emit_load_int_immediate(ctx.emitter, "x10", 8192);
            ctx.emitter.instruction("tst x9, x10");                            // is E_DEPRECATED enabled by error_reporting()?
            ctx.emitter.instruction(&format!("b.eq {}", masked_label));       // skip the diagnostic when E_DEPRECATED is masked
            abi::emit_symbol_address(ctx.emitter, "x1", &message_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", message_len as i64);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r11", "_rt_error_reporting", 0);
            ctx.emitter.instruction("test r11, 8192");                         // is E_DEPRECATED enabled by error_reporting()?
            ctx.emitter.instruction(&format!("jz {}", masked_label));         // skip the diagnostic when E_DEPRECATED is masked
            abi::emit_symbol_address(ctx.emitter, "rdi", &message_label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", message_len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_write");
    ctx.emitter.label(&masked_label);
}

/// Lowers a dynamic stdClass write through a nullable object receiver.
pub(super) fn lower_nullable_stdclass_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
) -> Result<()> {
    let null_label = ctx.next_label("nullable_stdclass_set_null");
    let done_label = ctx.next_label("nullable_stdclass_set_done");
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_nullable_receiver_object_payload(ctx, object, &null_label, object_reg)?;
    abi::emit_push_reg(ctx.emitter, object_reg);
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    let (key_label, key_len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the owned boxed property value
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the owned boxed property value
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_property_assign_on_null_fatal(ctx, property);

    ctx.emitter.label(&done_label);
    Ok(())
}
