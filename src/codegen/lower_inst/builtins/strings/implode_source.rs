//! Purpose:
//! Materializes the dense payload `implode()` joins when its array operand arrived boxed —
//! a `mixed` or union slot such as `?array`, whose real shape is only knowable at run time.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::strings::split`, the `implode()`/`join()` lowering.
//!
//! Key details:
//! - The answer is always an OWNED indexed array, so the join site's release is unconditional:
//!   hashes become dense value copies and packed arrays widen to boxed Mixed slots.
//! - Every non-array payload raises PHP's own `TypeError` instead of being read through the
//!   indexed-array layout. Reference PHP words null differently from the other types, and
//!   names a boolean by its value.

use super::*;

use crate::codegen::lower_inst::builtins::arrays::values::emit_loaded_boxed_array_values;
use crate::codegen::lower_inst::builtins::scalar_metadata::emit_branch_on_gettype_mixed_tag;

/// Reference PHP's wording when the payload is null: the separator's type is named too,
/// because that is the overload PHP reports against.
const IMPLODE_NULL_TYPE_ERROR: &str = "If argument #1 ($separator) is of type string, \
argument #2 ($array) must be of type array, null given";

/// Reference PHP's wording for every other non-array payload, completed by the type name.
const IMPLODE_TYPE_ERROR_PREFIX: &str =
    "Argument #2 ($array) must be of type ?array, ";

/// The first parameter accepts arrays or separators before overload selection.
const IMPLODE_SINGLE_TYPE_ERROR_PREFIX: &str =
    "Argument #1 ($separator) must be of type array|string, ";

/// Raises PHP's `TypeError` unless the boxed operand really holds an array, and returns to its
/// caller when it does.
///
/// Emitted BEFORE the join site materializes anything, and deliberately separate from
/// [`emit_boxed_implode_array_source`], which does the second unbox once the glue is already
/// parked on the stack. A throw from there would unwind past the parked glue with an unbalanced
/// stack and past an array this lowering has already taken a reference to; from here the frame
/// is exactly as the surrounding `try` left it, so the `TypeError` is catchable and leaks
/// nothing. The extra unbox is a tag read.
///
/// Reading a non-array through the indexed-array layout is what made `implode(',', $a)` on a
/// null `?array` segfault, and made an object print its header words (issue #689).
pub(super) fn emit_boxed_implode_array_type_guard(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    name: &str,
) -> Result<()> {
    let is_array = ctx.next_label("implode_guard_array");
    let int_case = ctx.next_label("implode_guard_int");
    let string_case = ctx.next_label("implode_guard_string");
    let float_case = ctx.next_label("implode_guard_float");
    let bool_case = ctx.next_label("implode_guard_bool");
    let true_case = ctx.next_label("implode_guard_true");
    let object_case = ctx.next_label("implode_guard_object");
    let resource_case = ctx.next_label("implode_guard_resource");
    let callable_case = ctx.next_label("implode_guard_callable");
    let single = inst.operands.len() == 1;
    let strict = matches!(inst.immediate, Some(Immediate::RuntimeCall(
        crate::ir::RuntimeCallTarget::ProfiledFunction { strict_types: Some(true), .. }
    )));
    let prefix = if single { IMPLODE_SINGLE_TYPE_ERROR_PREFIX } else { IMPLODE_TYPE_ERROR_PREFIX };
    let overload_error = format!("{name}(): {IMPLODE_NULL_TYPE_ERROR}");

    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_on_gettype_mixed_tag(ctx, 4, &is_array);
    emit_branch_on_gettype_mixed_tag(ctx, 5, &is_array);
    if matches!(inst.immediate, Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::PositionalImplode))) {
        super::super::exceptions::emit_type_error(ctx, &overload_error);
        ctx.emitter.label(&is_array);
        return Ok(());
    }
    emit_branch_on_gettype_mixed_tag(ctx, 0, &int_case);
    emit_branch_on_gettype_mixed_tag(ctx, 1, &string_case);
    emit_branch_on_gettype_mixed_tag(ctx, 2, &float_case);
    emit_branch_on_gettype_mixed_tag(ctx, 3, &bool_case);
    emit_branch_on_gettype_mixed_tag(ctx, 6, &object_case);
    emit_branch_on_gettype_mixed_tag(ctx, 9, &resource_case);
    emit_branch_on_gettype_mixed_tag(ctx, 10, &callable_case);
    // Mixed unboxing canonicalizes null container sentinels to PHP's null tag.
    if single && strict {
        emit_implode_type_error(ctx, name, prefix, "null");
    } else {
        super::super::exceptions::emit_type_error(ctx, &overload_error);
    }

    ctx.emitter.label(&int_case);
    emit_implode_scalar_type_error(ctx, name, prefix, "int", single && !strict);
    ctx.emitter.label(&string_case);
    emit_implode_scalar_type_error(ctx, name, prefix, "string", single);
    ctx.emitter.label(&float_case);
    emit_implode_scalar_type_error(ctx, name, prefix, "float", single && !strict);

    // PHP names a boolean by value: "true given" / "false given", never "bool given".
    ctx.emitter.label(&bool_case);
    let payload = unboxed_payload_reg(ctx);
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), payload);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &true_case);
    emit_implode_scalar_type_error(ctx, name, prefix, "false", single && !strict);
    ctx.emitter.label(&true_case);
    emit_implode_scalar_type_error(ctx, name, prefix, "true", single && !strict);

    ctx.emitter.label(&object_case);
    if single && !strict {
        emit_implode_stringable_separator(ctx, array, name)?;
    }
    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_implode_object_type_error(ctx, name, prefix);

    ctx.emitter.label(&callable_case);
    emit_implode_type_error(ctx, name, prefix, "Closure");
    ctx.emitter.label(&resource_case);
    emit_implode_resource_type_error(ctx, name, prefix);

    ctx.emitter.label(&is_array);
    Ok(())
}

/// Leaves an owned dense Mixed array for the join runtime in the first integer argument register.
///
/// The shared boxed-array normalizer converts hash values to dense storage and widens packed
/// arrays according to their runtime element tag. The outer Mixed cell describes only that the
/// operand is an array; its payload may still contain raw ints, objects, or strings, so retaining
/// it as `Array<Mixed>` without checking the packed header would make the renderer read the wrong
/// slot layout.
///
/// The type guard runs before the glue is staged, making the normalizer's invalid-tag path
/// unreachable for a well-formed value while keeping any actual TypeError catchable.
pub(super) fn emit_boxed_implode_array_source(ctx: &mut FunctionContext<'_>) -> Result<()> {
    emit_loaded_boxed_array_values(
        ctx,
        "implode(): Argument #2 ($array) must be of type array",
    )
}

/// The register `__rt_mixed_unbox` leaves the unboxed payload (`value_lo`) in.
fn unboxed_payload_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    }
}

/// Raises `implode()`'s `TypeError` naming `type_name`, exactly as php-src words it.
fn emit_implode_type_error(ctx: &mut FunctionContext<'_>, name: &str, prefix: &str, type_name: &str) {
    super::super::exceptions::emit_type_error(
        ctx,
        &format!("{name}(): {prefix}{type_name} given"),
    );
}

/// Reports the missing array after a weak scalar separator has been accepted.
fn emit_implode_scalar_type_error(
    ctx: &mut FunctionContext<'_>, name: &str, prefix: &str, type_name: &str, separator: bool,
) {
    if separator {
        super::super::exceptions::emit_type_error(ctx, &format!("{name}(): {IMPLODE_NULL_TYPE_ERROR}"));
    } else {
        emit_implode_type_error(ctx, name, prefix, type_name);
    }
}

/// Native extension handles use resource storage but expose PHP object class names.
fn emit_implode_resource_type_error(ctx: &mut FunctionContext<'_>, name: &str, prefix: &str) {
    let cases: Vec<_> = [(2, "HashContext"), (5, "HashContext"), (6, "CurlHandle"),
        (7, "CurlMultiHandle"), (8, "CurlShareHandle")].into_iter()
        .map(|(kind, class)| (kind, class, ctx.next_label("implode_guard_handle"))).collect();
    for (kind, _, label) in &cases {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp x2, #{}", kind));         // distinguish PHP object handles from genuine resources
                ctx.emitter.instruction(&format!("b.eq {}", label));            // name the extension object's PHP class
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp rdx, {}", kind));         // distinguish PHP object handles from genuine resources
                ctx.emitter.instruction(&format!("je {}", label));              // name the extension object's PHP class
            }
        }
    }
    emit_implode_type_error(ctx, name, prefix, "resource");
    for (_, class, label) in cases {
        ctx.emitter.label(&label);
        emit_implode_type_error(ctx, name, prefix, class);
    }
}

/// A weak one-argument Stringable runs its conversion before the missing-array error.
fn emit_implode_stringable_separator(ctx: &mut FunctionContext<'_>, array: ValueId, name: &str) -> Result<()> {
    let missing = ctx.next_label("implode_separator_not_stringable");
    let native = ctx.next_label("implode_separator_native_stringable");
    let intrinsic = ctx.next_label("implode_separator_intrinsic_stringable");
    let accepted = ctx.next_label("implode_separator_accepted");
    if ctx.module.required_runtime_features.eval_bridge {
        ctx.load_value_to_result(array)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), unboxed_payload_reg(ctx));
        let probe = ctx.emitter.target.extern_symbol("__elephc_eval_dynamic_object_has_to_string");
        abi::emit_call_label(ctx.emitter, &probe);
        abi::emit_branch_if_int_result_zero(ctx.emitter, &native);
        super::super::eval::emit_discarded_object_string_context(ctx, array)?;
        abi::emit_jump(ctx.emitter, &accepted);
    }
    ctx.emitter.label(&native);
    // The eval predicate is a C call and may clobber the unboxed payload registers.
    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x10", "_class_tostring_count", 0);
            ctx.emitter.instruction("ldr x9, [x1]");                            // read the class id after the symbol helper has used x9 as scratch
            ctx.emitter.instruction("cmp x9, x10");                             // reject synthetic and out-of-range class ids before indexing
            ctx.emitter.instruction(&format!("b.hs {}", intrinsic));            // an unknown class has no callable native hook
            abi::emit_symbol_address(ctx.emitter, "x10", "_class_tostring_ptrs");
            ctx.emitter.instruction("ldr x10, [x10, x9, lsl #3]");              // find the concrete or inherited conversion hook
            ctx.emitter.instruction(&format!("cbz x10, {}", intrinsic));        // builtin Throwable conversion has no source-method table entry
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rdi]");                 // inspect the object's concrete class without consuming it
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_class_tostring_count", 0);
            ctx.emitter.instruction("cmp r9, r10");                             // reject synthetic and out-of-range class ids before indexing
            ctx.emitter.instruction(&format!("jae {}", intrinsic));             // an unknown class has no callable native hook
            abi::emit_symbol_address(ctx.emitter, "r10", "_class_tostring_ptrs");
            ctx.emitter.instruction("mov r10, QWORD PTR [r10 + r9*8]");         // find the concrete or inherited conversion hook
            ctx.emitter.instruction("test r10, r10");                           // test whether the receiver has a native string conversion hook
            ctx.emitter.instruction(&format!("jz {}", intrinsic));              // builtin Throwable conversion has no source-method table entry
        }
    }
    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_implode_cast_string");
    let (ptr, _) = abi::string_result_regs(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), ptr);
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    abi::emit_jump(ctx.emitter, &accepted);

    ctx.emitter.label(&intrinsic);
    if let Some(throwable) = ctx.module.interface_infos.get("Throwable") {
        let interface_id = throwable.interface_id;
        ctx.load_value_to_result(array)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x0, [x1]");                        // test the receiver's actual class for intrinsic Throwable conversion
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, QWORD PTR [rdi]");            // test the receiver's actual class for intrinsic Throwable conversion
            }
        }
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), interface_id as i64);
        abi::emit_call_label(ctx.emitter, "__rt_class_implements_interface");
        abi::emit_branch_if_int_result_zero(ctx.emitter, &missing);
        // The remaining Throwable hook is builtin and side-effect free; its text is discarded.
        // User overrides already took the eval or native hook path above.
    } else {
        abi::emit_jump(ctx.emitter, &missing);
    }
    ctx.emitter.label(&accepted);
    super::super::exceptions::emit_type_error(ctx, &format!("{name}(): {IMPLODE_NULL_TYPE_ERROR}"));
    ctx.emitter.label(&missing);
    Ok(())
}

/// Raises `implode()`'s `TypeError` for an object, naming its class.
///
/// PHP prints the CLASS, not the word "object", and the class is only known at run time here —
/// the operand is a `mixed` slot. The name comes from the same dense metadata table
/// `get_class()` reads, so the message is php-src's wording verbatim.
fn emit_implode_object_type_error(ctx: &mut FunctionContext<'_>, name: &str, prefix: &str) {
    let object = unboxed_payload_reg(ctx);
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, result, object);
    super::super::types::emit_dynamic_object_class_name(ctx, "get_class");
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, ptr, len);
    emit_implode_error_concat_prefix(ctx, &format!("{name}(): {prefix}"));
    emit_implode_error_concat_suffix(ctx, " given");
    abi::emit_push_reg_pair(ctx.emitter, ptr, len);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    abi::emit_pop_reg_pair(ctx.emitter, ptr, len);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    super::super::exceptions::emit_type_error_from_string_result(ctx);
}

/// Prepends a static fragment to the message held in the string-result registers.
fn emit_implode_error_concat_prefix(ctx: &mut FunctionContext<'_>, prefix: &str) {
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    let (right_ptr, right_len) = implode_concat_right_operand_regs(ctx);
    let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
    ctx.emitter
        .instruction(&format!("mov {}, {}", right_ptr, text_ptr));              // move the built text into the concat right operand
    ctx.emitter
        .instruction(&format!("mov {}, {}", right_len, text_len));              // move its length into the concat right operand
    abi::emit_symbol_address(ctx.emitter, text_ptr, &prefix_label);
    abi::emit_load_int_immediate(ctx.emitter, text_len, prefix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// Appends a static fragment to the message held in the string-result registers.
fn emit_implode_error_concat_suffix(ctx: &mut FunctionContext<'_>, suffix: &str) {
    let (right_ptr, right_len) = implode_concat_right_operand_regs(ctx);
    let (suffix_label, suffix_len) = ctx.data.add_string(suffix.as_bytes());
    abi::emit_symbol_address(ctx.emitter, right_ptr, &suffix_label);
    abi::emit_load_int_immediate(ctx.emitter, right_len, suffix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// The register pair `__rt_concat` reads its right operand from.
fn implode_concat_right_operand_regs(ctx: &FunctionContext<'_>) -> (&'static str, &'static str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ("x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi"),
    }
}
