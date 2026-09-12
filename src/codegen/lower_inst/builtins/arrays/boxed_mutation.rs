//! Purpose:
//! Separates boxed PHP array receivers before edge mutations and key-ordering operations.
//!
//! Called from:
//! - Boxed array mutation and sorting backend paths.
//!
//! Key details:
//! - The outer cell is separated and published before its packed/hash payload is mutated.
//! - Pop/shift return an independently owned Mixed value; key sorts preserve key/value ownership.

use super::*;

/// Separates the caller's boxed cell, publishes it, then removes the selected edge value.
pub(super) fn lower_boxed_array_take(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    shift: bool,
) -> Result<()> {
    require_array_pop_result_type(&inst.result_php_type.codegen_repr())?;
    let name = if shift { "array_shift" } else { "array_pop" };
    prepare_boxed_array_receiver(ctx, array, name)?;
    ctx.load_value_to_reg(array, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), i64::from(shift));
    abi::emit_call_label(ctx.emitter, "__rt_array_take_boxed");
    store_if_result(ctx, inst)
}

/// Validates a boxed array and publishes a unique cell before any payload mutation.
pub(super) fn prepare_boxed_array_receiver(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    name: &str,
) -> Result<()> {
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    receiver.require_writable(name)?;
    ctx.load_value_to_reg(array, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_call_label(ctx.emitter, "__rt_array_cell_ensure_unique");
    require_valid_array_result(ctx, name);
    ctx.store_result_value(array)?;
    receiver.store_back_value(ctx, array)
}

/// Transfers the owned payload at the stack top into a unique cell, then retires its old payload.
/// The new container must already retain every old value needed after publication.
pub(super) fn install_boxed_array_payload(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    tag: u8,
) -> Result<()> {
    debug_assert!(matches!(tag, 4 | 5));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("ldr x0, [x9, #8]");                        // take the unique cell's previous payload owner
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", 0);
            ctx.emitter.instruction("str x10, [x9, #8]");                       // transfer the new container into the published cell
            abi::emit_load_int_immediate(ctx.emitter, "x10", i64::from(tag));
            ctx.emitter.instruction("str x10, [x9]");                           // install the layout tag before retiring old storage
            ctx.emitter.instruction("str xzr, [x9, #16]");                      // array payloads have no high word
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "r10")?;
            ctx.emitter.instruction("mov rax, QWORD PTR [r10 + 8]");            // take the unique cell's previous payload owner
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", 0);
            ctx.emitter.instruction("mov QWORD PTR [r10 + 8], r11");            // transfer the new container into the published cell
            ctx.emitter.instruction(&format!("mov QWORD PTR [r10], {tag}"));    // publish the new packed or associative layout
            ctx.emitter.instruction("mov QWORD PTR [r10 + 16], 0");             // array payloads have no high word
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    Ok(())
}

/// Reindexes and separates a boxed array before the shared scalar comparator mutates it.
pub(super) fn lower_boxed_array_sort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    name: &str,
) -> Result<()> {
    prepare_boxed_array_receiver(ctx, array, name)?;
    ctx.load_value_to_result(array)?;
    super::values::emit_loaded_boxed_array_values(
        ctx, &format!("{name}(): Argument #1 ($array) must be of type array"),
    )?;
    let result = abi::int_result_reg(ctx.emitter);
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    // Normalization can retain an already dense Mixed array. Consume that
    // independent owner in COW before sorting, leaving value aliases intact.
    abi::emit_reg_move(ctx.emitter, arg0, result);
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    abi::emit_push_reg(ctx.emitter, result);
    install_boxed_array_payload(ctx, array, 4)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg0, 0);
    super::sort_dispatch::emit_mixed_slot_sort(ctx, name)?;
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_load_int_immediate(ctx.emitter, result, 0x7fff_ffff_ffff_fffe);
    store_if_result(ctx, inst)
}

/// Promotes the published unique cell to hash storage and relinks keys without moving values.
pub(super) fn lower_boxed_array_key_sort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    name: &str,
    order: KeySortOrder,
) -> Result<()> {
    prepare_boxed_array_receiver(ctx, array, name)?;
    let arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(array, arg)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_cell_promote_to_hash");
    require_valid_array_result(ctx, name);
    // Promotion installs a unique hash in the cell. The sorter only relinks
    // entries, so the borrowed payload must not be split or released again.
    abi::emit_reg_move(ctx.emitter, arg, abi::int_result_reg(ctx.emitter));
    let helper = match order {
        KeySortOrder::Ascending => "__rt_hash_ksort",
        KeySortOrder::Descending => "__rt_hash_krsort",
    };
    abi::emit_call_label(ctx.emitter, helper);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    store_if_result(ctx, inst)
}

/// Throws before consuming an invalid cell or payload returned by an array runtime helper.
fn require_valid_array_result(ctx: &mut FunctionContext<'_>, name: &str) {
    let valid = ctx.next_label("array_mutation_valid");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {valid}"));              // reject non-array cells before publishing any receiver change
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // valid empty arrays still have a nonzero boxed cell
            ctx.emitter.instruction(&format!("jnz {valid}"));                   // publish only a validated array cell
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx, &format!("{name}(): Argument #1 ($array) must be of type array"),
    );
    ctx.emitter.label(&valid);
}
