//! Purpose:
//! Loads borrowed native reference values from raw or canonical managed cells.
//!
//! Called from:
//! - Native local and object-property reference reads.
//!
//! Key details:
//! - Physical shape is checked on each read because PHP callbacks may promote a cell.
//! - Canonical children retain their actual PHP tag independently of the entry declaration.
//! - Headerless borrowed references retain their existing raw ABI.

use super::*;

/// Distinguishes attached native payloads, boxed values and detached concrete frame storage.
pub(super) fn lower_mutation_layout(
    ctx: &mut FunctionContext<'_>, inst: &Instruction,
) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    let slot = super::reference_arguments::local_slot_for_loaded_value(ctx, source)?;
    let boxed = ctx.next_label("reference_mutation_boxed");
    let detached = ctx.next_label("reference_mutation_detached");
    let done = ctx.next_label("reference_mutation_layout_done");
    let result = abi::int_result_reg(ctx.emitter);
    if !ctx.local_ref_cell_representation_is_definite(slot)
        && !ctx.local_ref_cell_representation_is_dynamic(slot)
    {
        abi::emit_load_int_immediate(ctx.emitter, result, 0);
        return store_if_result(ctx, inst);
    }
    if ctx.local_ref_cell_representation_is_dynamic(slot) {
        let state = ctx.ref_cell_state_offset(slot).ok_or_else(||
            CodegenIrError::invalid_module("native mutation reference has no layout state"))?;
        abi::load_at_offset(ctx.emitter, result, state);
        abi::emit_branch_if_int_result_zero(ctx.emitter, &detached);
    }
    let offset = ctx.local_offset(slot)?;
    abi::load_at_offset(ctx.emitter, result, offset);
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_canonical");
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &boxed);
    abi::emit_load_int_immediate(ctx.emitter, result, 1);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&boxed);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&detached);
    // State zero ends the reference binding. Its occupant follows the final frame layout,
    // which can differ from the entry payload and can widen after this instruction was lowered.
    let detached_layout = if ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed { 0 } else { 2 };
    abi::emit_load_int_immediate(ctx.emitter, result, detached_layout);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Reads the actual canonical value or the legacy payload, then exposes a borrowed logical view.
pub(super) fn load_as(
    ctx: &mut FunctionContext<'_>, pointer: &str, payload_ty: &PhpType, result_ty: &PhpType,
) -> Result<()> {
    load_value(ctx, pointer, payload_ty, result_ty, false)
}

/// Takes one independent value copy while keeping the source cell and its aliases unchanged.
pub(super) fn load_owned_mixed(
    ctx: &mut FunctionContext<'_>, pointer: &str, payload_ty: &PhpType,
) -> Result<()> {
    load_value(ctx, pointer, payload_ty, &PhpType::Mixed, true)
}

/// Selects canonical or legacy storage before applying the requested value ownership.
fn load_value(
    ctx: &mut FunctionContext<'_>, pointer: &str, payload_ty: &PhpType,
    result_ty: &PhpType, owned: bool,
) -> Result<()> {
    let raw = ctx.next_label("reference_raw_payload");
    let done = ctx.next_label("reference_payload_done");
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, pointer);
    abi::emit_reg_move(ctx.emitter, result, pointer);
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_canonical");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &raw);
    // A parameter declaration validates entry, but callbacks can replace its canonical child.
    // Read that child's actual tag rather than projecting bits through the declaration's ABI.
    abi::emit_call_label(ctx.emitter, "__rt_mixed_deref");
    super::local_loads::coerce_borrowed_ref_cell_local(ctx, &PhpType::Mixed, result_ty)?;
    if owned {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&raw);
    abi::emit_load_temporary_stack_slot(ctx.emitter, pointer, 0);
    match payload_ty.codegen_repr() {
        PhpType::Str => {
            let (low, high) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, high, pointer, 8);
            abi::emit_load_from_address(ctx.emitter, low, pointer, 0);
        }
        PhpType::Float => {
            abi::emit_load_from_address(ctx.emitter, abi::float_result_reg(ctx.emitter), pointer, 0);
        }
        PhpType::TaggedScalar => {
            let tag = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, tag, pointer, 8);
            abi::emit_load_from_address(ctx.emitter, result, pointer, 0);
        }
        _ => abi::emit_load_from_address(ctx.emitter, result, pointer, 0),
    }
    if payload_ty.codegen_repr() == PhpType::Mixed {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_deref");
    }
    super::local_loads::coerce_borrowed_ref_cell_local(ctx, payload_ty, result_ty)?;
    if owned && payload_ty.codegen_repr() == PhpType::Mixed {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    }
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::{frame, shared_helper::helper_function};
    use crate::codegen::{data_section::DataSection, emit::Emitter, shared_state::SharedCodegenState};
    use crate::codegen_support::platform::Target;
    use crate::ir::{FunctionParam, IrType};

    /// Detached native parameters select their final frame layout instead of the entry cell ABI.
    #[test]
    fn detached_reference_mutation_layout_uses_final_frame_storage_on_every_target() {
        let source = r#"<?php
function nativeDetachedMutationLayout(string &$text, mixed $replacement): void {
    $text = $replacement; $text[0] = 'Z'; unset($text);
}
"#;
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = crate::ir_lower::tests::lower_source_at_for_target(source,
                std::path::Path::new("main.php"), std::path::Path::new("."), target);
            for (storage, code) in [(PhpType::Str, 2), (PhpType::Mixed, 0)] {
                let mut function = module.functions.iter().find(|function|
                    function.name == "nativeDetachedMutationLayout").unwrap().clone();
                function.locals[0].php_type = storage.clone();
                function.locals[0].ir_type = if storage == PhpType::Str { IrType::Str } else { IrType::Heap(crate::ir::IrHeapKind::Mixed) };
                let query = function.instructions.iter().find(|inst| inst.immediate ==
                    Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ReferenceCellMutationLayout)))
                    .unwrap().clone();
                let layout = frame::layout_for_function(&function, target, false, false, false);
                let mut emitter = Emitter::new(target);
                let mut data = DataSection::new();
                let mut shared = SharedCodegenState::default();
                {
                    let mut context = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                        &mut shared, layout, false, false, false, None);
                    super::lower_mutation_layout(&mut context, &query).unwrap();
                }
                let assembly = emitter.output();
                let detached = assembly.lines().find(|line| line.trim_end().ends_with(':')
                    && line.contains("reference_mutation_detached")).unwrap();
                let arm = assembly.split_once(detached).unwrap().1.split("reference_mutation_layout_done").next().unwrap();
                let expected = if target.arch == Arch::AArch64 {
                    format!("mov x0, #{code}")
                } else { format!("mov rax, {code}") };
                assert!(arm.contains(&expected), "{name}: {storage:?} detached layout: {arm}");
            }
        }
    }

    /// Scalar readers preserve the raw ABI while probing current canonical shape on every target.
    #[test]
    fn scalar_reference_readers_probe_current_physical_layout_on_every_target() {
        let source = "<?php \
            function readNativeIntReference(int &$value): int { return $value; } \
            function readNativeBoolReference(bool &$value): bool { return $value; } \
            function readNativeFloatReference(float &$value): float { return $value; } \
            function readNativeStringReference(string &$value): string { return $value; } \
            function readNativeNullableReference(?int &$value): ?int { return $value; }";
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = crate::ir_lower::tests::lower_source_at_for_target(source,
                std::path::Path::new("main.php"), std::path::Path::new("."), Target::parse(target).unwrap());
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            for name in ["readNativeIntReference", "readNativeBoolReference", "readNativeFloatReference",
                "readNativeStringReference", "readNativeNullableReference"]
            {
                let body = assembly.split_once(&format!("@fn name={name} symbol=")).unwrap().1
                    .split_once(&format!("@endfn name={name}")).unwrap().0;
                let probe = body.find("__rt_reference_cell_canonical").unwrap();
                assert!(body[probe..].contains("__rt_mixed_deref"),
                    "{target} {name}: canonical payloads must expose the actual child");
                assert!(body[probe..].contains("reference_raw_payload"),
                    "{target} {name}: headerless arguments must retain their raw layout");
                if target == "linux-x86_64" && name == "readNativeStringReference" {
                    let unbox = body[probe..].find("__rt_mixed_unbox").unwrap() + probe;
                    let canonical = body[unbox..].lines().take_while(|line|
                        !(line.trim_end().ends_with(':') && line.contains("reference_raw_payload")))
                        .collect::<Vec<_>>().join("\n");
                    assert!(canonical.contains("mov rax, rdi"),
                        "{target}: unbox returns the canonical string pointer in rdi");
                    assert!(!canonical.contains("mov rdx, rsi"),
                        "{target}: unbox already returns the string length in rdx");
                }
            }
        }
    }

    /// A Mixed read of a typed reference keeps the canonical child's tag without reboxing raw bits.
    #[test]
    fn canonical_reference_mixed_views_keep_actual_tags_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            let mut function = helper_function("canonical_mixed_view", PhpType::Void);
            function.params.clear();
            function.params.push(FunctionParam {
                name: "value".into(), ir_type: IrType::I64, php_type: PhpType::Int,
                by_ref: true, variadic: false,
            });
            function.values[0].ir_type = IrType::I64;
            function.values[0].php_type = PhpType::Int;
            let slot = function.add_local(Some("value".into()), IrType::I64, PhpType::Int,
                crate::ir::LocalKind::PhpLocal);
            let layout = frame::layout_for_function(&function, target, false, false, false);
            let mut emitter = Emitter::new(target);
            let mut data = DataSection::new();
            let mut shared = SharedCodegenState::default();
            {
                let mut context = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                    &mut shared, layout, false, false, false, None);
                super::super::local_loads::load_ref_cell_local_to_result_as(
                    &mut context, slot, &PhpType::Mixed).unwrap();
            }
            let assembly = emitter.output();
            let raw_label = assembly.lines().find(|line|
                line.trim_end().ends_with(':') && line.contains("reference_raw_payload"))
                .unwrap_or_else(|| panic!("{name}: missing raw reference branch: {assembly}"));
            let (canonical, raw) = assembly.split_once(raw_label).unwrap();
            assert!(canonical.contains("__rt_reference_cell_canonical"), "{name}");
            assert!(canonical.contains("__rt_mixed_deref"), "{name}");
            assert!(!canonical.contains("__rt_mixed_from_value"),
                "{name}: the real boxed child already carries its tag");
            assert!(raw.contains("__rt_mixed_from_value"),
                "{name}: raw integer references still need their legacy boxing adapter");
        }
    }

    /// A typed reference's by-value copy owns a box on both raw and canonical paths.
    #[test]
    fn owned_reference_value_views_balance_both_layouts_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            let function = helper_function("owned_reference_view", PhpType::Void);
            let layout = frame::layout_for_function(&function, target, false, false, false);
            let mut emitter = Emitter::new(target);
            let mut data = DataSection::new();
            let mut shared = SharedCodegenState::default();
            {
                let mut context = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                    &mut shared, layout, false, false, false, None);
                let pointer = abi::symbol_scratch_reg(context.emitter);
                super::load_owned_mixed(&mut context, pointer, &PhpType::Int).unwrap();
            }
            let assembly = emitter.output();
            let raw_label = assembly.lines().find(|line|
                line.trim_end().ends_with(':') && line.contains("reference_raw_payload")).unwrap();
            let (canonical, raw) = assembly.split_once(raw_label).unwrap();
            assert!(canonical.contains("__rt_mixed_clone"), "{name}: detach the actual canonical child");
            assert!(!canonical.contains("__rt_mixed_from_value"), "{name}: preserve its actual PHP tag");
            assert!(raw.contains("__rt_mixed_from_value"), "{name}: box the legacy integer exactly once");
            assert!(!raw.contains("__rt_mixed_clone"), "{name}: the new raw box already owns its value");
        }
    }
}
