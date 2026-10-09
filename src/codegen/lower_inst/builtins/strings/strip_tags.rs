//! Purpose:
//! Lowers strip_tags subjects and string, null or array allow-lists.
//!
//! Called from:
//! - The typed string runtime-function dispatcher.
//!
//! Key details:
//! - Boxed and concrete arrays share checked dense-value normalization.
//! - Temporary array owners are retired after joining; all targets share ABI helpers.

use super::*;

/// Lowers `strip_tags($string, $allowed_tags = null)` to `__rt_strip_tags`.
///
/// The runtime helper takes a subject string plus an already-normalized allow
/// string (`"<p><a>"`). A missing, null, or empty allow-list becomes a zero
/// length pair. Array allow-lists are joined with `><` and wrapped in `<>`,
/// matching PHP 8.5's array form. Proven-invalid allow types raise PHP's
/// `TypeError`; boxed Mixed/Union values are dispatched at runtime.
pub(crate) fn lower_strip_tags(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "strip_tags expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
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
    abi::emit_call_label(ctx.emitter, "__rt_strip_tags");
    store_if_result(ctx, inst)
}

/// Materializes `$allowed_tags` into the target string-result registers.
///
/// A missing or null argument becomes a zero-length pair so `__rt_strip_tags`
/// strips every tag. Arrays are rendered through implode with glue `><` and
/// then wrapped as `<...>` when the join is non-empty.
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
        other => {
            super::super::super::exceptions::emit_type_error(
                ctx,
                &strip_tags_allow_type_error(strip_tags_type_name(&other)),
            );
            Ok(())
        }
    }
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
    if let Some(value_ty) = hash_copy {
        let array_reg = match ctx.emitter.target.arch {
            Arch::AArch64 => "x3",
            Arch::X86_64 => "rdx",
        };
        abi::emit_push_reg(ctx.emitter, array_reg);
        abi::emit_call_label(ctx.emitter, runtime_label);
        abi::emit_push_result_value(ctx.emitter, &PhpType::Str);
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(value_ty)));
        let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
        abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    } else {
        abi::emit_call_label(ctx.emitter, runtime_label);
    }
    emit_wrap_allow_string(ctx);
    Ok(())
}

/// Wraps a non-empty implode result as `<joined>` so array allow-lists match PHP.
fn emit_wrap_allow_string(ctx: &mut FunctionContext<'_>) {
    let done = ctx.next_label("strip_tags_allow_wrap_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x2, {}", done));              // empty joins stay an empty allow-list
            let (lt_label, lt_len) = ctx.data.add_string(b"<");
            ctx.emitter.instruction("mov x3, x1");                              // pass the joined tags as concat's right operand
            ctx.emitter.instruction("mov x4, x2");                              // pass the joined length as concat's right operand
            abi::emit_symbol_address(ctx.emitter, "x1", &lt_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", lt_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
            let (gt_label, gt_len) = ctx.data.add_string(b">");
            abi::emit_symbol_address(ctx.emitter, "x3", &gt_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", gt_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test rdx, rdx"));                 // empty joins stay an empty allow-list
            ctx.emitter.instruction(&format!("jz {}", done));                   // skip wrapping when implode produced no tags
            let (lt_label, lt_len) = ctx.data.add_string(b"<");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the joined tags as concat's right operand
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the joined length as concat's right operand
            abi::emit_symbol_address(ctx.emitter, "rax", &lt_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", lt_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
            let (gt_label, gt_len) = ctx.data.add_string(b">");
            abi::emit_symbol_address(ctx.emitter, "rdi", &gt_label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", gt_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
        }
    }
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
            super::super::super::exceptions::emit_type_error(
                ctx,
                &strip_tags_allow_type_error("bool"),
            );
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
            super::super::super::exceptions::emit_type_error(
                ctx,
                &format!("{prefix}bool{suffix}"),
            );
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
