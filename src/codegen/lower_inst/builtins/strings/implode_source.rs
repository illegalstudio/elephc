//! Purpose:
//! Materializes the dense payload `implode()` joins when its array operand arrived boxed —
//! a `mixed` or union slot such as `?array`, whose real shape is only knowable at run time.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::strings::split`, the `implode()`/`join()` lowering.
//!
//! Key details:
//! - The answer is always an OWNED indexed array, so the join site's release is unconditional:
//!   a hash is converted to its values, and an indexed array is retained.
//! - Every non-array payload raises PHP's own `TypeError` instead of being read through the
//!   indexed-array layout. Reference PHP words null differently from the other types, and
//!   names a boolean by its value.

use super::*;

use crate::codegen::lower_inst::builtins::arrays::values::emit_loaded_assoc_array_values;
use crate::codegen::lower_inst::builtins::scalar_metadata::emit_branch_on_gettype_mixed_tag;

/// Reference PHP's wording when the payload is null: the separator's type is named too,
/// because that is the overload PHP reports against.
const IMPLODE_NULL_TYPE_ERROR: &str = "implode(): If argument #1 ($separator) is of type string, \
argument #2 ($array) must be of type array, null given";

/// Reference PHP's wording for every other non-array payload, completed by the type name.
const IMPLODE_TYPE_ERROR_PREFIX: &str =
    "implode(): Argument #2 ($array) must be of type ?array, ";

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
    array: ValueId,
) -> Result<()> {
    let is_array = ctx.next_label("implode_guard_array");
    let int_case = ctx.next_label("implode_guard_int");
    let string_case = ctx.next_label("implode_guard_string");
    let float_case = ctx.next_label("implode_guard_float");
    let bool_case = ctx.next_label("implode_guard_bool");
    let true_case = ctx.next_label("implode_guard_true");
    let object_case = ctx.next_label("implode_guard_object");

    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_on_gettype_mixed_tag(ctx, 4, &is_array);
    emit_branch_on_gettype_mixed_tag(ctx, 5, &is_array);
    emit_branch_on_gettype_mixed_tag(ctx, 0, &int_case);
    emit_branch_on_gettype_mixed_tag(ctx, 1, &string_case);
    emit_branch_on_gettype_mixed_tag(ctx, 2, &float_case);
    emit_branch_on_gettype_mixed_tag(ctx, 3, &bool_case);
    emit_branch_on_gettype_mixed_tag(ctx, 6, &object_case);
    // Every remaining tag is PHP's null, including the legacy sentinel payloads
    // `__rt_mixed_unbox` canonicalizes.
    super::super::exceptions::emit_type_error(ctx, IMPLODE_NULL_TYPE_ERROR);

    ctx.emitter.label(&int_case);
    emit_implode_type_error(ctx, "int");
    ctx.emitter.label(&string_case);
    emit_implode_type_error(ctx, "string");
    ctx.emitter.label(&float_case);
    emit_implode_type_error(ctx, "float");

    // PHP names a boolean by value: "true given" / "false given", never "bool given".
    ctx.emitter.label(&bool_case);
    let payload = unboxed_payload_reg(ctx);
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), payload);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &true_case);
    emit_implode_type_error(ctx, "false");
    ctx.emitter.label(&true_case);
    emit_implode_type_error(ctx, "true");

    ctx.emitter.label(&object_case);
    emit_implode_object_type_error(ctx);

    ctx.emitter.label(&is_array);
    Ok(())
}

/// Leaves the indexed array `implode()` should join in the first integer argument register,
/// given a boxed Mixed operand already loaded there.
///
/// The operand's declared type was `mixed` or a union, so nothing static says whether it holds
/// an indexed array or a hash, and the two need different handling:
///
///   * an indexed array is joined directly, RETAINED so the caller can release unconditionally;
///   * a hash is converted to its values first, exactly as a statically typed one is, because
///     the renderers walk a dense payload and a hash has none (PHP joins values, ignoring keys).
///
/// The result is owned either way — the hash conversion allocates, the indexed branch increfs —
/// so the join site releases it with one unconditional decref rather than a runtime flag.
///
/// A payload that is no array at all cannot reach here: `emit_boxed_implode_array_type_guard`
/// already threw for it, at a point where a throw is safe.
pub(super) fn emit_boxed_implode_array_source(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let assoc = ctx.next_label("implode_src_assoc");
    let done = ctx.next_label("implode_src_done");

    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_on_gettype_mixed_tag(ctx, 5, &assoc);

    emit_move_payload_to_first_argument(ctx);
    // The payload is BORROWED from the Mixed cell. Retaining it here is what lets the join site
    // release its operand unconditionally, the same way it releases the hash conversion.
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&assoc);
    emit_move_payload_to_first_argument(ctx);
    // `Mixed` values: a hash stores boxed cells, and stamping the copy as such is what routes
    // it through `__rt_implode`'s per-element cast rather than the raw-slot arms.
    emit_loaded_assoc_array_values(ctx, &PhpType::Mixed)?;

    ctx.emitter.label(&done);
    Ok(())
}

/// The register `__rt_mixed_unbox` leaves the unboxed payload (`value_lo`) in.
fn unboxed_payload_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    }
}

/// Moves the unboxed payload into the first integer argument register.
fn emit_move_payload_to_first_argument(ctx: &mut FunctionContext<'_>) {
    let payload = unboxed_payload_reg(ctx);
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), payload);
}

/// Raises `implode()`'s `TypeError` naming `type_name`, exactly as php-src words it.
fn emit_implode_type_error(ctx: &mut FunctionContext<'_>, type_name: &str) {
    super::super::exceptions::emit_type_error(
        ctx,
        &format!("{IMPLODE_TYPE_ERROR_PREFIX}{type_name} given"),
    );
}

/// Raises `implode()`'s `TypeError` for an object, naming its class.
///
/// PHP prints the CLASS, not the word "object", and the class is only known at run time here —
/// the operand is a `mixed` slot. The name comes from the same dense metadata table
/// `get_class()` reads, so the message is php-src's wording verbatim.
fn emit_implode_object_type_error(ctx: &mut FunctionContext<'_>) {
    let (name_ptr_reg, name_len_reg) = abi::string_result_regs(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x1]");                            // load the receiver class id from the unboxed object payload
            abi::emit_symbol_address(ctx.emitter, "x10", "_class_name_entries");
            ctx.emitter.instruction("lsl x11, x9, #4");                         // scale the class id to the 16-byte class-name row
            ctx.emitter.instruction("add x10, x10, x11");                       // address the receiver's class-name metadata
            ctx.emitter
                .instruction(&format!("ldr {}, [x10]", name_ptr_reg));          // borrow the class-name pointer
            ctx.emitter
                .instruction(&format!("ldr {}, [x10, #8]", name_len_reg));      // borrow the class-name byte length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rdi]");                 // load the receiver class id from the unboxed object payload
            abi::emit_symbol_address(ctx.emitter, "r10", "_class_name_entries");
            ctx.emitter.instruction("shl r9, 4");                               // scale the class id to the 16-byte class-name row
            ctx.emitter
                .instruction(&format!("mov {}, QWORD PTR [r10 + r9]", name_ptr_reg)); // borrow the class-name pointer
            ctx.emitter
                .instruction(&format!("mov {}, QWORD PTR [r10 + r9 + 8]", name_len_reg)); // borrow the class-name byte length
        }
    }
    emit_implode_error_concat_prefix(ctx, IMPLODE_TYPE_ERROR_PREFIX);
    emit_implode_error_concat_suffix(ctx, " given");
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
