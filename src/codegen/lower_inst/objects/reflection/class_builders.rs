//! Purpose:
//! Emits the module-level builders that materialize a fresh, complete `ReflectionClass`
//! for one class, called through `__elephc_reflection_class_build()`.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()`, after the module's bodies (and again after
//!   `main`), through `emit_pending_reflection_class_builders`.
//!
//! Key details:
//! - Member reflectors request a builder by storing its address in their declaring-class or
//!   class-type slot (`member_properties_emit::emit_reflection_class_builder_address_as_mixed`).
//! - Emitting a builder can request more builders (the declaring classes of its own members),
//!   so the queue is drained to a fixed point; it is bounded by the module's class count.
//! - Each builder is a global shared helper: it leaves an OWNED `ReflectionClass` in the
//!   integer result register and holds no reference afterwards, so no reflector is ever
//!   retained by a static slot or by its own members.
//! - An enum's builder materializes a complete `ReflectionEnum`, as php's class factory does
//!   for every getter that hands back a class (`zend_reflection_class_factory`).

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::shared_helper::emit_global_shared_helper;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::Module;

use super::*;

/// Emits every requested `ReflectionClass` builder that has not been emitted yet.
pub(in crate::codegen) fn emit_pending_reflection_class_builders(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    while let Some((class_name, label)) = shared.next_pending_reflection_class_builder() {
        let comment = format!("--- ReflectionClass builder: {} ---", class_name);
        emit_global_shared_helper(
            module,
            emitter,
            data,
            shared,
            regalloc_linear,
            &label,
            PhpType::Object("ReflectionClass".to_string()),
            &comment,
            |ctx: &mut FunctionContext<'_>| {
                if is_reflection_enum(ctx, &class_name) {
                    let metadata = reflection_enum_class_metadata_for_name(ctx, &class_name)?;
                    return emit_reflection_owner_object(ctx, "ReflectionEnum", &metadata);
                }
                let metadata = reflection_class_metadata_for_name(ctx, &class_name)?;
                emit_reflection_owner_object(ctx, "ReflectionClass", &metadata)
            },
        )?;
    }
    Ok(())
}
