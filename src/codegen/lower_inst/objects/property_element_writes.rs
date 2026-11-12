//! Purpose:
//! Republishes a typed array or hash property that an element write mutated in place.
//!
//! Called from:
//! - `crate::codegen::lower_inst::arrays` (`array_set`, `array_push`).
//! - `crate::codegen::lower_inst::hashes` (`hash_set`, `hash_append`).
//!
//! Key details:
//! - The receiver is the result of `PropGetForWrite`: the property's own container, already
//!   separated, published and BORROWED. The write helpers split under the `ensure_unique`
//!   convention, so the property's reference is the one a later split or growth consumes and the
//!   pointer they hand back is only ever PUBLISHED, never retained or released.
//! - Publication is skipped when the helper returns the pointer it was given. An overwrite
//!   releases the replaced value last, and that release can run a destructor which writes or
//!   replaces this same property; storing the helper's pointer afterwards would discard it.
//!   A relocation only happens before any value is released, so a changed pointer is safe.

use super::*;

/// A declared array or hash property whose container an element write mutates in place.
#[derive(Clone)]
pub(in crate::codegen::lower_inst) struct InPlacePropertyContainer {
    /// The receiver object the property belongs to.
    object: ValueId,
    /// The fixed slot holding the container pointer, or the reference cell that holds it.
    slot: PropertySlot,
}

/// Resolves `value` to the property whose container `PropGetForWrite` lent it from.
///
/// `Borrow` and `Move` keep the same container identity and are peeled. Every other producer,
/// including the owning `PropGet` + `Acquire` read the mutating builtins use, answers `None` and
/// keeps its existing write-back.
pub(in crate::codegen::lower_inst) fn in_place_property_container(
    ctx: &FunctionContext<'_>,
    mut value: ValueId,
) -> Result<Option<InPlacePropertyContainer>> {
    loop {
        let Some(value_ref) = ctx.function.value(value) else {
            return Err(CodegenIrError::missing_entry("value", value.as_raw()));
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else {
            return Ok(None);
        };
        let Some(inst_ref) = ctx.function.instruction(inst) else {
            return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
        };
        match inst_ref.op {
            Op::Borrow | Op::Move => {
                let Some(source) = inst_ref.operands.first().copied() else {
                    return Ok(None);
                };
                value = source;
            }
            Op::PropGetForWrite => {
                let Some(object) = inst_ref.operands.first().copied() else {
                    return Ok(None);
                };
                let Ok(property) = property_name_immediate(ctx, inst_ref) else {
                    return Ok(None);
                };
                let Ok(slot) = resolve_property_slot(ctx, object, property, inst_ref) else {
                    return Ok(None);
                };
                let raw_container = matches!(
                    slot.php_type.codegen_repr(),
                    PhpType::Array(_) | PhpType::AssocArray { .. }
                );
                if slot.is_packed || !raw_container {
                    return Ok(None);
                }
                return Ok(Some(InPlacePropertyContainer { object, slot }));
            }
            _ => return Ok(None),
        }
    }
}

impl InPlacePropertyContainer {
    /// Reloads the container from the property into `value`'s storage.
    ///
    /// A hash key conversion can run a warning handler between the fetch and the write, and
    /// that handler may replace the property. The write must then target the container the
    /// property holds now, not the one the fetch lent out. Sharing needs no step of its own:
    /// the write helper separates a shared table itself.
    pub(in crate::codegen::lower_inst) fn reload(
        &self,
        ctx: &mut FunctionContext<'_>,
        value: ValueId,
    ) -> Result<()> {
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        let result_reg = abi::int_result_reg(ctx.emitter);
        ctx.load_value_to_reg(self.object, base_reg)?;
        if self.slot.is_reference {
            let cell_reg = reference_pointer_reg(ctx, base_reg);
            abi::emit_load_from_address(ctx.emitter, cell_reg, base_reg, self.slot.offset);
            abi::emit_load_from_address(ctx.emitter, result_reg, cell_reg, 0);
        } else {
            abi::emit_load_from_address(ctx.emitter, result_reg, base_reg, self.slot.offset);
        }
        ctx.store_result_value(value)
    }

    /// Publishes the container a write helper returned, when it differs from the one passed in.
    ///
    /// Expects the helper's pointer in the integer result register and the pointer it was
    /// given still in `value`'s storage. A changed pointer is stored into the slot (or through
    /// the reference cell) without touching any reference count, and `value` then tracks it.
    pub(in crate::codegen::lower_inst) fn publish_relocated(
        &self,
        ctx: &mut FunctionContext<'_>,
        value: ValueId,
    ) -> Result<()> {
        let unchanged = ctx.next_label("prop_element_write_unchanged");
        let result_reg = abi::int_result_reg(ctx.emitter);
        let previous_reg = abi::secondary_scratch_reg(ctx.emitter);
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        // -- publish the container only when the write relocated it --
        ctx.load_value_to_reg(value, previous_reg)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {result_reg}, {previous_reg}")); // did the write hand back the container it was given?
                ctx.emitter.instruction(&format!("b.eq {unchanged}"));          // same container: the property already holds it
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {result_reg}, {previous_reg}")); // did the write hand back the container it was given?
                ctx.emitter.instruction(&format!("je {unchanged}"));            // same container: the property already holds it
            }
        }
        ctx.load_value_to_reg(self.object, base_reg)?;
        if self.slot.is_reference {
            let cell_reg = reference_pointer_reg(ctx, base_reg);
            abi::emit_load_from_address(ctx.emitter, cell_reg, base_reg, self.slot.offset);
            abi::emit_store_to_address(ctx.emitter, result_reg, cell_reg, 0);
        } else {
            abi::emit_store_to_address(ctx.emitter, result_reg, base_reg, self.slot.offset);
        }
        ctx.emitter.label(&unchanged);
        ctx.store_result_value(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::Target;
    use crate::ir::Op;

    /// Element writes into typed properties fetch for write, store nothing back in EIR, and
    /// publish a relocated container from the backend on every supported target.
    #[test]
    fn typed_property_element_writes_stay_in_place_on_every_target() {
        let source = "<?php final class InPlaceStore { \
            public array<int, int> $map = []; public array<int> $list = []; } \
            function putInPlace(InPlaceStore $store, int $key, int $value): void { \
                $store->map[$key] = $value; $store->list[] = $value; $store->list[$key] = $value; } \
            putInPlace(new InPlaceStore(), 1, 2);";
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = crate::ir_lower::tests::lower_source_at_for_target(source,
                std::path::Path::new("main.php"), std::path::Path::new("."), Target::parse(name).unwrap());
            let function = module.functions.iter()
                .find(|function| function.name == "putInPlace").unwrap();
            for write in [Op::HashSet, Op::ArrayPush, Op::ArraySet] {
                // The lazy-initialization arm stores a fresh container; the write's own block
                // must not store the fetched one back.
                let ops: Vec<Op> = function.blocks.iter()
                    .map(|block| block.instructions.iter()
                        .map(|id| function.instruction(*id).unwrap().op).collect::<Vec<_>>())
                    .find(|ops| ops.contains(&write))
                    .unwrap_or_else(|| panic!("{name}: missing {write:?}"));
                assert!(!ops.contains(&Op::PropSet), "{name}: {write:?} never stores the container back");
                let index = ops.iter().position(|op| *op == write).unwrap();
                assert_eq!(ops[index - 1], Op::PropGetForWrite,
                    "{name}: {write:?} takes the container fetched for write, with nothing between");
            }
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            let body = assembly.split_once("@fn name=putInPlace symbol=").unwrap().1
                .split_once("@endfn name=putInPlace").unwrap().0;
            let published = body.matches("prop_element_write_unchanged").count();
            assert!(published >= 6, "{name}: each write compares and publishes its container");
        }
    }

    /// A boxed value stored into concrete element storage is converted inside the write, where
    /// user code can run, so that write keeps the private-copy shape on every target.
    #[test]
    fn boxed_values_into_typed_elements_write_a_private_copy_on_every_target() {
        let source = "<?php final class CopyStore { public array<string, string> $names = []; \
            public array<string, mixed> $any = []; } \
            function putBoxed(CopyStore $store, mixed $value): void { \
                $store->names['a'] = $value; } \
            function putAny(CopyStore $store, mixed $value): void { \
                $store->any['a'] = $value; } \
            putBoxed(new CopyStore(), 'x'); putAny(new CopyStore(), 'x');";
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = crate::ir_lower::tests::lower_source_at_for_target(source,
                std::path::Path::new("main.php"), std::path::Path::new("."), Target::parse(name).unwrap());
            let block_ops = |function: &str| {
                let function = module.functions.iter().find(|f| f.name == function).unwrap();
                function.blocks.iter()
                    .map(|block| block.instructions.iter()
                        .map(|id| function.instruction(*id).unwrap().op).collect::<Vec<_>>())
                    .find(|ops| ops.contains(&Op::HashSet))
                    .unwrap_or_else(|| panic!("{name}: missing hash_set"))
            };
            let boxed = block_ops("putBoxed");
            assert!(boxed.contains(&Op::PropSet) && !boxed.contains(&Op::PropGetForWrite),
                "{name}: a converting write stores a private copy back");
            let any = block_ops("putAny");
            assert!(any.contains(&Op::PropGetForWrite) && !any.contains(&Op::PropSet),
                "{name}: a boxed value into boxed storage needs no conversion and stays in place");
        }
    }
}
