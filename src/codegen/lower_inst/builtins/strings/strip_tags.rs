//! Purpose:
//! Lowers strip_tags subjects and string, null, array, or weak scalar allow-lists.
//!
//! Called from:
//! - The typed string runtime-function dispatcher.
//!
//! Key details:
//! - Boxed and concrete arrays share checked dense-value normalization.
//! - Temporary array owners are retired after joining; all targets share ABI helpers.
//! - Strict callers reject scalar allow-lists before the strip helper is invoked.

use super::*;

/// Lowers `strip_tags($string, $allowed_tags = null)` to `__rt_strip_tags`.
///
/// The runtime helper takes a subject string plus an already-normalized allow
/// string (`"<p><a>"`). A missing, null, or empty allow-list becomes a zero
/// length pair. Array allow-lists are joined with `><` and wrapped in `<>`,
/// including `<>` for a single empty element, matching PHP 8.5's array form.
/// Proven-invalid allow types raise PHP's `TypeError`; boxed Mixed/Union values
/// are dispatched at runtime.
pub(crate) fn lower_strip_tags(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_strip_tags_impl(ctx, inst, "__rt_strip_tags", &PhpType::Str)?;
    store_if_result(ctx, inst)
}

/// Lowers the allocation-free `strlen(strip_tags(...))` helper.
///
/// The scanner runs in a count-only mode and returns the retained byte count in
/// the integer result register; it never publishes or persists the output.
pub(crate) fn lower_strip_tags_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_strip_tags_impl(ctx, inst, "__rt_strip_tags", &PhpType::Int)?;
    store_if_result(ctx, inst)
}

/// Emits the shared strip-tags argument materialization and call for one result shape.
fn lower_strip_tags_impl(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    label: &str,
    result_ty: &PhpType,
) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "strip_tags expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    let owns_allow = if let Some(allowed) = inst.operands.get(1) {
        matches!(ctx.value_php_type(*allowed)?.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_))
    } else {
        false
    };
    if owns_allow {
        abi::emit_reserve_temporary_stack(ctx.emitter, 16);
        let owner_address = abi::tertiary_scratch_reg(ctx.emitter);
        abi::emit_temporary_stack_address(ctx.emitter, owner_address, 0);
        abi::emit_store_zero_to_address(ctx.emitter, owner_address, 0);
        abi::emit_push_call_operand_owner(ctx.emitter, owner_address, false);
    }
    let subject = expect_operand(inst, 0)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_value_as_string_to_regs(ctx, subject, "strip_tags", "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the subject string while materializing $allowed_tags
            lower_strip_tags_allow(ctx, inst)?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the allow-list pointer as the secondary string argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the allow-list length as the secondary string argument
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the subject string into the primary argument pair
        }
        Arch::X86_64 => {
            load_value_as_string_to_regs(ctx, subject, "strip_tags", "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            lower_strip_tags_allow(ctx, inst)?;
            ctx.emitter.instruction("mov rcx, rdx");                            // pass the allow-list length as the fourth SysV string argument
            ctx.emitter.instruction("mov rdx, rax");                            // pass the allow-list pointer as the third SysV string argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    match (ctx.emitter.target.arch, result_ty.codegen_repr() == PhpType::Int) {
        (Arch::AArch64, true) => {
            ctx.emitter.instruction("mov x5, #1");                              // count bytes without publishing a string
        }
        (Arch::AArch64, false) => {
            ctx.emitter.instruction("mov x5, #0");                              // publish the stripped string
        }
        (Arch::X86_64, true) => {
            ctx.emitter.instruction("mov r8d, 1");                              // count bytes without publishing a string
        }
        (Arch::X86_64, false) => {
            ctx.emitter.instruction("xor r8d, r8d");                            // publish the stripped string
        }
    }
    abi::emit_call_label(ctx.emitter, label);
    if owns_allow {
        abi::emit_pop_call_operand_owner(ctx.emitter);
        abi::emit_push_result_value(ctx.emitter, result_ty);
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
        abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        if result_ty.codegen_repr() == PhpType::Str {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
        } else {
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        }
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
    Ok(())
}

/// Materializes `$allowed_tags` into the target string-result registers.
///
/// A missing or null argument becomes a zero-length pair so `__rt_strip_tags`
/// strips every tag. Arrays are rendered through implode with glue `><` and
/// then wrapped as `<...>` when the array has any elements, even an empty string.
fn lower_strip_tags_allow(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 {
        emit_empty_allow_string(ctx);
        return Ok(());
    }
    let allowed = expect_operand(inst, 1)?;
    match ctx.value_php_type(allowed)?.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            emit_empty_allow_string(ctx);
            Ok(())
        }
        PhpType::Str => {
            load_value_as_string_to_regs(
                ctx,
                allowed,
                "strip_tags",
                string_ptr_reg(ctx),
                string_len_reg(ctx),
            )
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            lower_strip_tags_allow_from_array(ctx, inst, allowed)
        }
        PhpType::Mixed | PhpType::Union(_) => lower_strip_tags_allow_mixed(ctx, inst, allowed),
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False
            if !strip_tags_strict_types(inst) =>
        {
            // Weak scalar conversion cannot produce a <tag>, so its allow-list is empty.
            emit_empty_allow_string(ctx);
            Ok(())
        }
        PhpType::Bool | PhpType::False => {
            ctx.load_value_to_result(allowed)?;
            let register = abi::int_result_reg(ctx.emitter);
            emit_strip_tags_bool_type_error(ctx, register);
            Ok(())
        }
        other => {
            super::super::super::exceptions::emit_type_error(
                ctx,
                &strip_tags_allow_type_error(strip_tags_type_name(&other)),
            );
            Ok(())
        }
    }
}

/// Retains the caller's per-file scalar binding mode at this builtin boundary.
fn strip_tags_strict_types(inst: &Instruction) -> bool {
    matches!(inst.immediate, Some(Immediate::RuntimeCall(
        crate::ir::RuntimeCallTarget::ProfiledFunction { strict_types: Some(true), .. }
    )))
}

/// Names PHP's true/false argument kinds when a strict union rejects a boolean.
fn emit_strip_tags_bool_type_error(ctx: &mut FunctionContext<'_>, register: &str) {
    let from_true = ctx.next_label("strip_tags_allow_true_error");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz {}, {}", register, from_true)); // select PHP's true kind without losing the boolean payload
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", register, register)); // inspect the rejected boolean value, not its Mixed tag
            ctx.emitter.instruction(&format!("jne {}", from_true));             // distinguish true from false in the union TypeError
        }
    }
    super::super::super::exceptions::emit_type_error(ctx, &strip_tags_allow_type_error("false"));
    ctx.emitter.label(&from_true);
    super::super::super::exceptions::emit_type_error(ctx, &strip_tags_allow_type_error("true"));
}

/// Writes a zero-length allow-list into the string-result registers.
fn emit_empty_allow_string(ctx: &mut FunctionContext<'_>) {
    let ptr_reg = string_ptr_reg(ctx);
    let len_reg = string_len_reg(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, xzr", ptr_reg));          // empty allow-list pointer
            ctx.emitter.instruction(&format!("mov {}, xzr", len_reg));          // empty allow-list length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("xor {}, {}", ptr_reg, ptr_reg));  // empty allow-list pointer
            ctx.emitter.instruction(&format!("xor {}, {}", len_reg, len_reg));  // empty allow-list length
        }
    }
}

/// Joins an array allow-list into PHP's `<tag><tag>` string form.
fn lower_strip_tags_allow_from_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    allowed: ValueId,
) -> Result<()> {
    let runtime_label = super::split::implode_runtime_label(ctx, inst, 1)?;
    let hash_copy = super::split::implode_normalized_value_type(ctx, inst, 1)?;
    let (glue_label, glue_len) = ctx.data.add_string(b"><");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &glue_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", glue_len as i64);
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve `><` glue while loading the allow array
            super::split::load_implode_array(ctx, allowed)?;
            ctx.emitter.instruction("mov x3, x0");                              // pass the indexed allow-array pointer to implode
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the `><` glue into implode's string argument
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &glue_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", glue_len as i64);
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            super::split::load_implode_array(ctx, allowed)?;
            ctx.emitter.instruction("mov rdx, rax");                            // pass the indexed allow-array pointer to implode
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    let (array_reg, count_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x3", "x5"),
        Arch::X86_64 => ("rdx", "r8"),
    };
    // The normalized dense payload starts with its element count. Empty rendered
    // strings still contribute a tag, so preserve this independently of join length.
    abi::emit_load_from_address(ctx.emitter, count_reg, array_reg, 0);
    abi::emit_push_reg(ctx.emitter, count_reg);
    super::implode_owners::emit_owned_implode(ctx, runtime_label, hash_copy);
    abi::emit_pop_reg(ctx.emitter, count_reg);
    emit_wrap_allow_string(ctx);
    // The caller parked the subject above its cleanup record and owner slot.
    let owner_offset = 16 + abi::CALL_OPERAND_OWNER_RECORD_BYTES;
    abi::emit_store_to_sp(ctx.emitter, string_ptr_reg(ctx), owner_offset);
    Ok(())
}

/// Wraps a non-empty array's join, even an empty string, using its saved count.
/// The count is in `x5`/`r8`; the joined string remains in the string-result pair.
fn emit_wrap_allow_string(ctx: &mut FunctionContext<'_>) {
    let done = ctx.next_label("strip_tags_allow_wrap_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x5, {}", done));              // only an empty array has no allowed tags
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test r8, r8");                             // distinguish an empty array from one empty rendered element
            ctx.emitter.instruction(&format!("jz {}", done));                   // skip wrapping only when the source array has no elements
        }
    }
    super::strip_tags_owners::emit_owned_allow_concat(ctx, b"<", true);
    super::strip_tags_owners::emit_owned_allow_concat(ctx, b">", false);
    ctx.emitter.label(&done);
}

/// Dispatches a boxed Mixed/Union `$allowed_tags` by runtime tag.
fn lower_strip_tags_allow_mixed(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    allowed: ValueId,
) -> Result<()> {
    let done = ctx.next_label("strip_tags_allow_mixed_done");
    let from_null = ctx.next_label("strip_tags_allow_mixed_null");
    let from_string = ctx.next_label("strip_tags_allow_mixed_string");
    let from_array = ctx.next_label("strip_tags_allow_mixed_array");
    let from_bad = ctx.next_label("strip_tags_allow_mixed_bad");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(allowed, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp x0, #8");                              // Mixed tag 8 is null
            ctx.emitter.instruction(&format!("b.eq {}", from_null));            // null allow-list strips every tag
            ctx.emitter.instruction("cmp x0, #1");                              // Mixed tag 1 is string
            ctx.emitter.instruction(&format!("b.eq {}", from_string));          // borrow the string payload as the allow-list
            if !strip_tags_strict_types(inst) {
                ctx.emitter.instruction("cmp x0, #0");                          // weak integer strings cannot contain an allowed tag
                ctx.emitter.instruction(&format!("b.eq {}", from_null));        // render the accepted numeric allow-list as empty
                ctx.emitter.instruction("cmp x0, #2");                          // weak float strings cannot contain an allowed tag
                ctx.emitter.instruction(&format!("b.eq {}", from_null));        // render the accepted numeric allow-list as empty
                ctx.emitter.instruction("cmp x0, #3");                          // weak boolean strings cannot contain an allowed tag
                ctx.emitter.instruction(&format!("b.eq {}", from_null));        // render the accepted boolean allow-list as empty
            }
            ctx.emitter.instruction("cmp x0, #4");                              // Mixed tag 4 is an indexed array
            ctx.emitter.instruction(&format!("b.eq {}", from_array));           // join array values into `<tag><tag>`
            ctx.emitter.instruction("cmp x0, #5");                              // Mixed tag 5 is an associative array
            ctx.emitter.instruction(&format!("b.eq {}", from_array));           // join hash values into `<tag><tag>`
            ctx.emitter.instruction(&format!("b {}", from_bad));                // any other tag is PHP's TypeError
            ctx.emitter.label(&from_null);
            emit_empty_allow_string(ctx);
            ctx.emitter.instruction(&format!("b {}", done));                    // finish with the empty allow-list
            ctx.emitter.label(&from_string);
            ctx.emitter.instruction(&format!("b {}", done));                    // x1/x2 already hold the borrowed string
            ctx.emitter.label(&from_array);
            lower_strip_tags_allow_from_array(ctx, inst, allowed)?;
            ctx.emitter.instruction(&format!("b {}", done));                    // finish with the wrapped allow string
            ctx.emitter.label(&from_bad);
            emit_strip_tags_mixed_type_error(ctx);
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(allowed, "rax")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp rax, 8");                              // Mixed tag 8 is null
            ctx.emitter.instruction(&format!("je {}", from_null));              // null allow-list strips every tag
            ctx.emitter.instruction("cmp rax, 1");                              // Mixed tag 1 is string
            ctx.emitter.instruction(&format!("je {}", from_string));            // borrow the string payload as the allow-list
            if !strip_tags_strict_types(inst) {
                ctx.emitter.instruction("cmp rax, 0");                          // weak integer strings cannot contain an allowed tag
                ctx.emitter.instruction(&format!("je {}", from_null));          // render the accepted numeric allow-list as empty
                ctx.emitter.instruction("cmp rax, 2");                          // weak float strings cannot contain an allowed tag
                ctx.emitter.instruction(&format!("je {}", from_null));          // render the accepted numeric allow-list as empty
                ctx.emitter.instruction("cmp rax, 3");                          // weak boolean strings cannot contain an allowed tag
                ctx.emitter.instruction(&format!("je {}", from_null));          // render the accepted boolean allow-list as empty
            }
            ctx.emitter.instruction("cmp rax, 4");                              // Mixed tag 4 is an indexed array
            ctx.emitter.instruction(&format!("je {}", from_array));             // join array values into `<tag><tag>`
            ctx.emitter.instruction("cmp rax, 5");                              // Mixed tag 5 is an associative array
            ctx.emitter.instruction(&format!("je {}", from_array));             // join hash values into `<tag><tag>`
            ctx.emitter.instruction(&format!("jmp {}", from_bad));              // any other tag is PHP's TypeError
            ctx.emitter.label(&from_null);
            emit_empty_allow_string(ctx);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // finish with the empty allow-list
            ctx.emitter.label(&from_string);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed string pointer into the result pointer register
            ctx.emitter.instruction(&format!("jmp {}", done));                  // rdx already holds the unboxed string length
            ctx.emitter.label(&from_array);
            lower_strip_tags_allow_from_array(ctx, inst, allowed)?;
            ctx.emitter.instruction(&format!("jmp {}", done));                  // finish with the wrapped allow string
            ctx.emitter.label(&from_bad);
            emit_strip_tags_mixed_type_error(ctx);
            ctx.emitter.label(&done);
        }
    }
    Ok(())
}

/// Throws PHP's `$allowed_tags` TypeError for an unexpected Mixed tag in `x0`/`rax`.
fn emit_strip_tags_mixed_type_error(ctx: &mut FunctionContext<'_>) {
    let prefix = "strip_tags(): Argument #2 ($allowed_tags) must be of type array|string|null, ";
    let suffix = " given";
    let int_l = ctx.next_label("strip_tags_allow_ty_int");
    let float_l = ctx.next_label("strip_tags_allow_ty_float");
    let bool_l = ctx.next_label("strip_tags_allow_ty_bool");
    let object_l = ctx.next_label("strip_tags_allow_ty_object");
    let resource_l = ctx.next_label("strip_tags_allow_ty_resource");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // Mixed tag 0 is int
            ctx.emitter.instruction(&format!("b.eq {}", int_l));                // name the TypeError as `int`
            ctx.emitter.instruction("cmp x0, #2");                              // Mixed tag 2 is float
            ctx.emitter.instruction(&format!("b.eq {}", float_l));              // name the TypeError as `float`
            ctx.emitter.instruction("cmp x0, #3");                              // Mixed tag 3 is bool
            ctx.emitter.instruction(&format!("b.eq {}", bool_l));               // name the TypeError as `bool`
            ctx.emitter.instruction("cmp x0, #6");                              // Mixed tag 6 is object
            ctx.emitter.instruction(&format!("b.eq {}", object_l));             // name the TypeError as `object`
            ctx.emitter.instruction("cmp x0, #9");                              // Mixed tag 9 is resource
            ctx.emitter.instruction(&format!("b.eq {}", resource_l));           // name the TypeError as `resource`
            super::super::super::exceptions::emit_type_error(
                ctx,
                &strip_tags_allow_type_error("mixed"),
            );
            ctx.emitter.label(&int_l);
            super::super::super::exceptions::emit_type_error(
                ctx,
                &strip_tags_allow_type_error("int"),
            );
            ctx.emitter.label(&float_l);
            super::super::super::exceptions::emit_type_error(
                ctx,
                &strip_tags_allow_type_error("float"),
            );
            ctx.emitter.label(&bool_l);
            emit_strip_tags_bool_type_error(ctx, "x1");
            ctx.emitter.label(&object_l);
            super::super::super::exceptions::emit_type_error(
                ctx,
                &strip_tags_allow_type_error("object"),
            );
            ctx.emitter.label(&resource_l);
            super::super::super::exceptions::emit_type_error(
                ctx,
                &strip_tags_allow_type_error("resource"),
            );
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // Mixed tag 0 is int
            ctx.emitter.instruction(&format!("je {}", int_l));                  // name the TypeError as `int`
            ctx.emitter.instruction("cmp rax, 2");                              // Mixed tag 2 is float
            ctx.emitter.instruction(&format!("je {}", float_l));                // name the TypeError as `float`
            ctx.emitter.instruction("cmp rax, 3");                              // Mixed tag 3 is bool
            ctx.emitter.instruction(&format!("je {}", bool_l));                 // name the TypeError as `bool`
            ctx.emitter.instruction("cmp rax, 6");                              // Mixed tag 6 is object
            ctx.emitter.instruction(&format!("je {}", object_l));               // name the TypeError as `object`
            ctx.emitter.instruction("cmp rax, 9");                              // Mixed tag 9 is resource
            ctx.emitter.instruction(&format!("je {}", resource_l));             // name the TypeError as `resource`
            super::super::super::exceptions::emit_type_error(
                ctx,
                &format!("{prefix}mixed{suffix}"),
            );
            ctx.emitter.label(&int_l);
            super::super::super::exceptions::emit_type_error(
                ctx,
                &format!("{prefix}int{suffix}"),
            );
            ctx.emitter.label(&float_l);
            super::super::super::exceptions::emit_type_error(
                ctx,
                &format!("{prefix}float{suffix}"),
            );
            ctx.emitter.label(&bool_l);
            emit_strip_tags_bool_type_error(ctx, "rdi");
            ctx.emitter.label(&object_l);
            super::super::super::exceptions::emit_type_error(
                ctx,
                &format!("{prefix}object{suffix}"),
            );
            ctx.emitter.label(&resource_l);
            super::super::super::exceptions::emit_type_error(
                ctx,
                &format!("{prefix}resource{suffix}"),
            );
        }
    }
}

/// Builds PHP's `strip_tags()` Argument #2 TypeError message for `type_name`.
fn strip_tags_allow_type_error(type_name: &str) -> String {
    format!(
        "strip_tags(): Argument #2 ($allowed_tags) must be of type array|string|null, {type_name} given"
    )
}

/// Returns PHP's TypeError type label for a proven-invalid allow-list type.
fn strip_tags_type_name(ty: &PhpType) -> &'static str {
    match ty {
        PhpType::Int => "int",
        PhpType::Float => "float",
        PhpType::Bool | PhpType::False => "bool",
        PhpType::Object(_) | PhpType::Packed(_) => "object",
        PhpType::Resource(_) => "resource",
        PhpType::Callable => "callable",
        _ => "mixed",
    }
}
