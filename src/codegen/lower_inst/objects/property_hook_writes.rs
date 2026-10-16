//! Purpose:
//! Preserves inherited abstract setter dispatch over readonly backing storage.
//!
//! Called from:
//! - Ordinary named, runtime-name and reflected EIR property-write dispatch.
//!
//! Key details:
//! - Initializers and eval's explicit raw Reflection stores keep physical property storage.
//! - Setter argument guards run, but an abstract setter never initializes the backing slot.

use super::*;

/// Validates and retires an abstract setter argument without publishing it into storage.
pub(super) fn emit_hook_aware_property_store(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    let abstract_setter = ctx.module.class_infos.get(&slot.class_name)
        .and_then(|info| info.retained_abstract_setter(&slot.property))
        .and_then(|contract| contract.set_type.clone().map(|ty| (contract.declaring_type.clone(), ty)));
    let Some((owner, parameter_type)) = abstract_setter else {
        return emit_property_store(ctx, value, slot, base_reg);
    };
    let mut parameter = slot.clone();
    parameter.php_type = parameter_type;
    parameter.abstract_setter_type_error_prefix = Some(format!(
        "{owner}::${}::set(): Argument #1 ($value) must be of type {}, ",
        slot.property, mixed_property_type_guard::php_type_name(&parameter.php_type),
    ));
    // Materialization creates the owner a physical slot would have adopted.
    // Retire that owner here; EIR still owns the original evaluated argument.
    load_property_store_value_to_result(ctx, value, &parameter)?;
    match parameter.php_type.codegen_repr() {
        PhpType::Str => abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe"),
        ty => abi::emit_decref_if_refcounted(ctx.emitter, &ty),
    }
    Ok(())
}
