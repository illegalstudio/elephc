//! Purpose:
//! Normalizes dynamic instanceof values and target metadata.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Class/interface matcher kinds and invalid-target diagnostics remain target-aware.

use super::*;

/// Tests PHP's built-in `Closure` class against Elephc's callable descriptor representation.
///
/// Direct callable values are non-null descriptor pointers. Boxed gradual values carry runtime
/// tag 10 when their payload is such a descriptor; strings, arrays, invokable objects, and other
/// callable forms intentionally remain false because PHP's `Closure` class is narrower than
/// PHP's general callable set.
pub(super) fn emit_closure_instanceof(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match value_ty.codegen_repr() {
        PhpType::Callable => {
            ctx.load_value_to_reg(value, result_reg)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #0");                      // null descriptors are not Closure objects
                    ctx.emitter.instruction("cset x0, ne");                     // return true for a live callable descriptor
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("test rax, rax");                   // null descriptors are not Closure objects
                    ctx.emitter.instruction("setne al");                        // materialize the non-null descriptor predicate
                    ctx.emitter.instruction("movzx rax, al");                   // widen the boolean result to the PHP integer register
                }
            }
        }
        PhpType::Mixed => {
            ctx.load_value_to_reg(value, result_reg)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #10");                     // runtime tag 10 identifies a boxed callable descriptor
                    ctx.emitter.instruction("cset x0, eq");                     // return whether the boxed value is a Closure descriptor
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 10");                     // runtime tag 10 identifies a boxed callable descriptor
                    ctx.emitter.instruction("sete al");                         // materialize the callable-tag predicate
                    ctx.emitter.instruction("movzx rax, al");                   // widen the boolean result to the PHP integer register
                }
            }
        }
        _ => emit_false(ctx),
    }
    Ok(())
}

/// Normalizes the tested value into an object pointer or null for dynamic `instanceof`.
pub(super) fn emit_normalized_dynamic_instanceof_value(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    match value_ty {
        // `Iterable` joins `Object` for the same reason as the named path: the slot holds the
        // unboxed payload, so the raw pointer is already the matcher's input.
        PhpType::Object(_) | PhpType::Iterable => {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
        }
        PhpType::Callable => {
            emit_callable_object_capture_or_null(ctx, value)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
            emit_mixed_instanceof_value_normalization(ctx);
        }
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    Ok(())
}

/// Extracts an invokable-object receiver from a callable descriptor, or returns null.
pub(super) fn emit_callable_object_capture_or_null(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
) -> Result<()> {
    let descriptor_reg = abi::int_result_reg(ctx.emitter).to_string();
    let kind_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let false_label = ctx.next_label("callable_instanceof_not_object");
    let done_label = ctx.next_label("callable_instanceof_object_done");
    ctx.load_value_to_reg(value, &descriptor_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", descriptor_reg, false_label)); // null descriptors are not object values
            abi::emit_load_from_address(ctx.emitter, &kind_reg, &descriptor_reg, 0);
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::symbol_scratch_reg(ctx.emitter),
                callable_descriptor::CALLABLE_DESC_KIND_OBJECT_INVOKE as i64,
            );
            ctx.emitter.instruction(&format!(
                "cmp {}, {}",
                kind_reg,
                abi::symbol_scratch_reg(ctx.emitter)
            )); // identify descriptors created from invokable objects
            ctx.emitter
                .instruction(&format!("b.ne {}", false_label));               // non-object callable forms fail instanceof
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "test {}, {}",
                descriptor_reg, descriptor_reg
            )); // null descriptors are not object values
            ctx.emitter
                .instruction(&format!("jz {}", false_label));                  // skip descriptor metadata loads for null
            abi::emit_load_from_address(ctx.emitter, &kind_reg, &descriptor_reg, 0);
            ctx.emitter.instruction(&format!(
                "cmp {}, {}",
                kind_reg,
                callable_descriptor::CALLABLE_DESC_KIND_OBJECT_INVOKE
            )); // identify descriptors created from invokable objects
            ctx.emitter
                .instruction(&format!("jne {}", false_label));                 // non-object callable forms fail instanceof
        }
    }
    callable_descriptor::emit_load_runtime_capture_to_result(
        ctx.emitter,
        &descriptor_reg,
        0,
        &PhpType::Object(String::new()),
    );
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, &descriptor_reg, 0);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Normalizes an unboxed Mixed object pointer into the integer result register for `instanceof`.
pub(super) fn emit_mixed_instanceof_value_normalization(ctx: &mut FunctionContext<'_>) {
    let object_label = ctx.next_label("instanceof_dynamic_value_object");
    let done = ctx.next_label("instanceof_dynamic_value_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the tested mixed payload is an object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // object payloads can be matched after dynamic target resolution
            ctx.emitter.instruction("mov x0, #0");                              // scalar mixed payloads become null so the matcher returns false
            ctx.emitter.instruction(&format!("b {}", done));                    // skip object-payload promotion for scalar payloads
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov x0, x1");                              // promote the unboxed object pointer into the normal result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the tested mixed payload is an object
            ctx.emitter.instruction(&format!("je {}", object_label));           // object payloads can be matched after dynamic target resolution
            ctx.emitter.instruction("xor eax, eax");                            // scalar mixed payloads become null so the matcher returns false
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip object-payload promotion for scalar payloads
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov rax, rdi");                            // promote the unboxed object pointer into the normal result register
        }
    }
    ctx.emitter.label(&done);
}

/// Resolves the dynamic `instanceof` target into matcher id/kind registers.
pub(super) fn emit_dynamic_target_metadata(
    ctx: &mut FunctionContext<'_>,
    target: crate::ir::ValueId,
    target_ty: &PhpType,
    false_label: &str,
) -> Result<()> {
    match target_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(target, ptr_reg, len_reg)?;
            emit_lookup_string_target(ctx, false_label);
        }
        PhpType::Object(_) => {
            ctx.load_value_to_reg(target, abi::int_result_reg(ctx.emitter))?;
            emit_object_target_metadata(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(target, abi::int_result_reg(ctx.emitter))?;
            emit_mixed_target_metadata(ctx, false_label);
        }
        _ => emit_invalid_dynamic_target_fatal(ctx),
    }
    Ok(())
}

/// Looks up a string dynamic target in the runtime class/interface name table.
pub(super) fn emit_lookup_string_target(ctx: &mut FunctionContext<'_>, false_label: &str) {
    abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the dynamic string resolve to a known class or interface?
            ctx.emitter.instruction(&format!("b.eq {}", false_label));          // unresolved class-string targets make instanceof false
            ctx.emitter.instruction("mov x0, x1");                              // move the resolved target id into the matcher target-id register
            ctx.emitter.instruction("mov x1, x2");                              // move the resolved target kind into the matcher target-kind register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the dynamic string resolve to a known class or interface?
            ctx.emitter.instruction(&format!("je {}", false_label));            // unresolved class-string targets make instanceof false
            ctx.emitter.instruction("mov rax, rdi");                            // move the resolved target id into the matcher target-id register
        }
    }
}

/// Extracts matcher metadata from an object-typed dynamic target.
pub(super) fn emit_object_target_metadata(ctx: &mut FunctionContext<'_>) {
    let ok_label = ctx.next_label("instanceof_dynamic_object_target_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", ok_label));         // non-null object targets can provide runtime class metadata
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&ok_label);
            ctx.emitter.instruction("ldr x0, [x0]");                            // load the runtime class id from the target object header
            ctx.emitter.instruction("mov x1, #0");                              // object targets always resolve to class target kind
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // null object targets are not valid dynamic instanceof targets
            ctx.emitter.instruction(&format!("jne {}", ok_label));              // non-null object targets can provide runtime class metadata
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&ok_label);
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // load the runtime class id from the target object header
            ctx.emitter.instruction("xor edx, edx");                            // object targets always resolve to class target kind
        }
    }
}

/// Unboxes a Mixed/Union target and routes strings or objects to the matching target resolver.
pub(super) fn emit_mixed_target_metadata(ctx: &mut FunctionContext<'_>, false_label: &str) {
    let string_label = ctx.next_label("instanceof_dynamic_target_string");
    let object_label = ctx.next_label("instanceof_dynamic_target_object");
    let done = ctx.next_label("instanceof_dynamic_target_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("b.eq {}", string_label));         // resolve boxed string targets through class-string lookup
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // resolve boxed object targets through their runtime class id
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&string_label);
            emit_lookup_string_target(ctx, false_label);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov x0, x1");                              // move the unboxed target object pointer into the result register
            emit_object_target_metadata(ctx);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("je {}", string_label));           // resolve boxed string targets through class-string lookup
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("je {}", object_label));           // resolve boxed object targets through their runtime class id
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&string_label);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed target string pointer into the lookup input register
            emit_lookup_string_target(ctx, false_label);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed target object pointer into the result register
            emit_object_target_metadata(ctx);
        }
    }
    ctx.emitter.label(&done);
}

/// Emits a dynamic `instanceof` matcher call after target id/kind have been resolved.
pub(super) fn emit_dynamic_match_call(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x1");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_push_reg(ctx.emitter, "rdx");
        }
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2));
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1));
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0));
    abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
}

/// Emits the runtime fatal for invalid dynamic `instanceof` targets.
pub(super) fn emit_invalid_dynamic_target_fatal(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_instanceof_invalid_target");
}

/// Emits the metadata matcher call with object-or-mixed input already in argument 0.
pub(in crate::codegen::lower_inst) fn emit_match_call(
    ctx: &mut FunctionContext<'_>,
    target_id: u64,
    target_kind: i64,
    helper: &str,
) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        target_id as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        target_kind,
    );
    abi::emit_call_label(ctx.emitter, helper);
}

/// Classifies a named target as a class `(kind 0)` or interface `(kind 1)`.
pub(in crate::codegen::lower_inst) fn classify_named_target(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> Option<(u64, i64)> {
    if let Some(class_info) = class_info_by_name(ctx, class_name) {
        return Some((class_info.class_id, 0));
    }
    interface_info_by_name(ctx, class_name)
        .map(|interface_info| (interface_info.interface_id, 1))
}

/// Emits a boolean false result for non-object values or unresolved targets.
pub(super) fn emit_false(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
}

/// Resolves an instruction property-name immediate into the module data pool.
pub(super) fn property_name_immediate<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Resolves an instruction class-name immediate into the module data pool.
pub(in crate::codegen::lower_inst) fn class_name_immediate<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("class data", data.as_raw()))
}

/// Returns true when a class name refers to PHP's built-in `Fiber` type.
pub(super) fn is_fiber_class(class_name: &str) -> bool {
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
}
