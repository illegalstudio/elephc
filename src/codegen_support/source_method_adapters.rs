//! Purpose:
//! Adapts source-visible method calls to physical methods carrying generated `func_args` slots.
//! Owns planning, symbols, and target-aware wrapper assembly for these boundaries.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` and runtime method-table emission.
//!
//! Key details:
//! - Only one trailing generated collector can be synthesized because its actual count is known.
//! - Source variadics and hidden argc slots are rejected instead of entering an unsafe ABI.
//! - The wrapper publishes its collector owner before entering PHP and preserves every return word.

use std::collections::{HashMap, HashSet};

use crate::codegen_support::emit::Emitter;
use crate::names::{join_php_symbol, method_symbol, static_method_symbol};
use crate::types::{ClassInfo, FunctionSig, PhpType};

use super::{abi, emit_array_value_type_stamp};

/// Whether the physical method receives an object or the late-static class id first.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum MethodKind {
    Instance,
    Static,
}

/// A validated source-to-physical method ABI relationship.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MethodAbiPlan {
    Direct,
    AppendEmptyCollector,
}

/// Plans the safe adaptation between one caller contract and its physical implementation.
pub(crate) fn plan_method_abi(
    caller: &FunctionSig,
    physical: &FunctionSig,
) -> Result<MethodAbiPlan, String> {
    if caller.by_ref_return != physical.by_ref_return {
        return Err("source method ABI adaptation cannot bridge a by-reference return mismatch".to_string());
    }
    if same_parameter_abi(caller, physical) {
        return Ok(MethodAbiPlan::Direct);
    }
    if crate::func_args::sig_has_hidden_argc_param(caller)
        || crate::func_args::sig_has_hidden_argc_param(physical)
    {
        return Err("source method ABI adaptation cannot synthesize the hidden actual-count parameter".to_string());
    }
    if caller.variadic.is_some() {
        return Err("source method ABI adaptation cannot forward a source variadic with an unknown actual count".to_string());
    }
    if !crate::func_args::sig_collects_surplus_args(physical) {
        return Err("source method ABI adaptation found an unsupported physical signature difference".to_string());
    }
    let projected = source_visible_signature(physical)?;
    if !same_parameter_abi(caller, &projected) {
        return Err("source method ABI adaptation found a visible parameter mismatch".to_string());
    }
    Ok(MethodAbiPlan::AppendEmptyCollector)
}

/// Selects a source-vtable entry, rejecting source variadics that need hidden actual count.
pub(crate) fn source_vtable_entry_symbol(
    class_name: &str,
    method_name: &str,
    physical: &FunctionSig,
    kind: MethodKind,
) -> Result<String, String> {
    if physical.variadic.is_some()
        && !crate::func_args::sig_collects_surplus_args(physical)
    {
        if crate::func_args::sig_has_hidden_argc_param(physical) {
            return Err(
                "source method vtable cannot omit a source variadic hidden actual-count parameter"
                    .to_string(),
            );
        }
        return Ok(physical_method_symbol(class_name, method_name, kind));
    }
    source_method_entry_symbol(class_name, method_name, physical, kind)
}

/// Projects a collector-bearing physical signature back to the declaration visible in PHP.
pub(crate) fn source_visible_signature(physical: &FunctionSig) -> Result<FunctionSig, String> {
    if crate::func_args::sig_has_hidden_argc_param(physical) {
        return Err("source method ABI adaptation cannot project a source variadic hidden argc".to_string());
    }
    let mut source = physical.clone();
    if crate::func_args::sig_collects_surplus_args(physical) {
        let Some((name, _)) = source.params.last() else {
            return Err("generated method collector is missing its physical parameter".to_string());
        };
        if name != crate::func_args::HIDDEN_ARGS_PARAM {
            return Err("generated method collector is not the final physical parameter".to_string());
        }
        source.params.pop();
        source.param_type_exprs.pop();
        source.param_attributes.pop();
        source.defaults.pop();
        source.ref_params.pop();
        source.declared_params.pop();
        source.variadic = None;
    }
    if source.variadic.is_some() {
        return Err("source method ABI adaptation cannot project a source variadic".to_string());
    }
    Ok(source)
}

/// Returns true when a call signature already carries generated physical `func_args` slots.
pub(crate) fn uses_physical_func_args_abi(sig: &FunctionSig) -> bool {
    crate::func_args::sig_collects_surplus_args(sig)
        || crate::func_args::sig_has_hidden_argc_param(sig)
}

/// Returns the shared source-boundary symbol for one physical method implementation.
pub(crate) fn source_method_adapter_symbol(
    class_name: &str,
    method_name: &str,
    kind: MethodKind,
) -> String {
    let prefix = match kind {
        MethodKind::Instance => "_method_source_abi",
        MethodKind::Static => "_static_source_abi",
    };
    join_php_symbol(prefix, &[class_name, method_name])
}

/// Selects a raw method symbol or its source-boundary adapter after validating the ABI.
pub(crate) fn source_method_entry_symbol(
    class_name: &str,
    method_name: &str,
    physical: &FunctionSig,
    kind: MethodKind,
) -> Result<String, String> {
    let source = source_visible_signature(physical)?;
    match plan_method_abi(&source, physical)? {
        MethodAbiPlan::Direct => Ok(physical_method_symbol(class_name, method_name, kind)),
        MethodAbiPlan::AppendEmptyCollector => Ok(source_method_adapter_symbol(
            class_name,
            method_name,
            kind,
        )),
    }
}

/// Emits one deduplicated source adapter for every collector-bearing implementation in scope.
pub(crate) fn emit_source_method_adapters(
    emitter: &mut Emitter,
    classes: &HashMap<String, ClassInfo>,
    emitted_class_names: Option<&HashSet<String>>,
) -> Result<(), String> {
    let mut specs = Vec::new();
    for (class_name, class_info) in classes {
        if emitted_class_names.is_some_and(|names| !names.contains(class_name)) {
            continue;
        }
        for (method_name, sig) in &class_info.methods {
            if crate::func_args::sig_collects_surplus_args(sig)
                && class_info
                    .method_impl_classes
                    .get(method_name)
                    .is_some_and(|owner| owner == class_name)
            {
                specs.push((class_name.clone(), method_name.clone(), MethodKind::Instance));
            }
        }
        for (method_name, sig) in &class_info.static_methods {
            if crate::func_args::sig_collects_surplus_args(sig)
                && class_info
                    .static_method_impl_classes
                    .get(method_name)
                    .is_some_and(|owner| owner == class_name)
            {
                specs.push((class_name.clone(), method_name.clone(), MethodKind::Static));
            }
        }
    }
    specs.sort();
    specs.dedup();
    for (class_name, method_name, kind) in specs {
        let class_info = classes
            .get(&class_name)
            .ok_or_else(|| format!("missing class metadata for source adapter {class_name}"))?;
        let physical = match kind {
            MethodKind::Instance => class_info.methods.get(&method_name),
            MethodKind::Static => class_info.static_methods.get(&method_name),
        }
        .ok_or_else(|| format!("missing physical signature for {class_name}::{method_name}"))?;
        let source = source_visible_signature(physical)?;
        let symbol = source_method_adapter_symbol(&class_name, &method_name, kind);
        let implementation = physical_method_symbol(&class_name, &method_name, kind);
        emit_method_adapter(
            emitter,
            &symbol,
            &implementation,
            kind,
            &source,
            physical,
            false,
        )?;
    }
    Ok(())
}

/// Emits one wrapper, optionally boxing its concrete return for an interface slot.
pub(crate) fn emit_method_adapter(
    emitter: &mut Emitter,
    wrapper: &str,
    implementation: &str,
    kind: MethodKind,
    caller: &FunctionSig,
    physical: &FunctionSig,
    box_return_as_mixed: bool,
) -> Result<(), String> {
    let plan = plan_method_abi(caller, physical)?;
    if box_return_as_mixed && physical.by_ref_return {
        return Err("source method ABI adapter cannot value-box a by-reference return".to_string());
    }
    let implicit_ty = match kind {
        MethodKind::Instance => PhpType::Object("__source_method_receiver".to_string()),
        MethodKind::Static => PhpType::Int,
    };
    let mut incoming_types = vec![implicit_ty];
    incoming_types.extend(caller.params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut incoming_refs = vec![false];
    incoming_refs.extend(caller.ref_params.iter().copied());
    let frame_size = (incoming_types.len() + 3) * 16;
    let owner_offset = (incoming_types.len() + 1) * 16;
    let return_offset = owner_offset + 16;

    emitter.raw(".align 2");
    emitter.label_global(wrapper);
    emitter.comment("adapt source-visible method ABI to physical method ABI");
    abi::emit_frame_prologue(emitter, frame_size);
    let mut cursor = abi::IncomingArgCursor::for_target(emitter.target, 0);
    for (index, (ty, by_ref)) in incoming_types.iter().zip(&incoming_refs).enumerate() {
        abi::emit_store_incoming_param(
            emitter,
            "source_method_arg",
            ty,
            (index + 1) * 16,
            *by_ref,
            &mut cursor,
        );
    }
    if plan == MethodAbiPlan::AppendEmptyCollector {
        abi::emit_store_zero_to_local_slot(emitter, owner_offset);
        emit_empty_collector_owner(emitter, owner_offset);
    }

    let mut outgoing_types = incoming_types.clone();
    let mut outgoing_refs = incoming_refs.clone();
    if plan == MethodAbiPlan::AppendEmptyCollector {
        outgoing_types.push(PhpType::Array(Box::new(PhpType::Mixed)));
        outgoing_refs.push(false);
    }
    let abi_types = outgoing_types
        .iter()
        .zip(&outgoing_refs)
        .map(|(ty, by_ref)| if *by_ref { PhpType::Int } else { ty.codegen_repr() })
        .collect::<Vec<_>>();
    for (index, ty) in abi_types.iter().enumerate() {
        let offset = if index < incoming_types.len() {
            (index + 1) * 16
        } else {
            owner_offset
        };
        push_frame_value(emitter, ty, offset);
    }
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &abi_types, 0);
    let overflow = abi::materialize_outgoing_args(emitter, &assignments);
    let pad = abi::outgoing_call_stack_pad_bytes(emitter.target, overflow);
    abi::emit_reserve_temporary_stack(emitter, pad);
    abi::emit_call_label(emitter, implementation);
    abi::emit_release_temporary_stack(emitter, pad);
    abi::emit_release_temporary_stack(emitter, overflow);

    let physical_return = if physical.by_ref_return {
        PhpType::Int
    } else {
        physical.return_type.codegen_repr()
    };
    preserve_return_value(emitter, &physical_return, return_offset);
    if plan == MethodAbiPlan::AppendEmptyCollector {
        abi::emit_pop_call_operand_owner(emitter);
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), owner_offset);
        abi::emit_store_zero_to_local_slot(emitter, owner_offset);
        abi::emit_call_label(emitter, "__rt_decref_any");
    }
    restore_return_value(emitter, &physical_return, return_offset);
    if box_return_as_mixed {
        super::value_boxing::emit_box_current_value_as_mixed(emitter, &physical_return);
    }
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
    Ok(())
}

fn same_parameter_abi(left: &FunctionSig, right: &FunctionSig) -> bool {
    left.params
        .iter()
        .map(|(_, ty)| ty.codegen_repr())
        .eq(right.params.iter().map(|(_, ty)| ty.codegen_repr()))
        && left.ref_params == right.ref_params
        && left.variadic == right.variadic
}

fn physical_method_symbol(class_name: &str, method_name: &str, kind: MethodKind) -> String {
    match kind {
        MethodKind::Instance => method_symbol(class_name, method_name),
        MethodKind::Static => static_method_symbol(class_name, method_name),
    }
}

fn emit_empty_collector_owner(emitter: &mut Emitter, owner_offset: usize) {
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 0);
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        PhpType::Mixed.stack_size() as i64,
    );
    abi::emit_call_label(emitter, "__rt_array_new");
    emit_array_value_type_stamp(emitter, abi::int_result_reg(emitter), &PhpType::Mixed);
    abi::store_at_offset(emitter, abi::int_result_reg(emitter), owner_offset);
    let owner_address = abi::tertiary_scratch_reg(emitter);
    abi::emit_frame_slot_address(emitter, owner_address, owner_offset);
    abi::emit_push_call_operand_owner(emitter, owner_address, false);
}

fn push_frame_value(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty.codegen_repr() {
        PhpType::Float => {
            let reg = abi::float_result_reg(emitter);
            abi::load_at_offset(emitter, reg, offset);
            abi::emit_push_float_reg(emitter, reg);
        }
        PhpType::Str => {
            let (lo, hi) = abi::string_result_regs(emitter);
            abi::load_at_offset(emitter, lo, offset);
            abi::load_at_offset(emitter, hi, offset - 8);
            abi::emit_push_reg_pair(emitter, lo, hi);
        }
        PhpType::TaggedScalar => {
            let lo = abi::int_result_reg(emitter);
            let hi = crate::codegen_support::sentinels::tagged_scalar_tag_reg(emitter);
            abi::load_at_offset(emitter, lo, offset);
            abi::load_at_offset(emitter, hi, offset - 8);
            abi::emit_push_reg_pair(emitter, lo, hi);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            let reg = abi::int_result_reg(emitter);
            abi::load_at_offset(emitter, reg, offset);
            abi::emit_push_reg(emitter, reg);
        }
    }
}

fn preserve_return_value(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty.codegen_repr() {
        PhpType::Float => abi::store_at_offset(emitter, abi::float_result_reg(emitter), offset),
        PhpType::Str => {
            let (lo, hi) = abi::string_result_regs(emitter);
            abi::store_at_offset(emitter, lo, offset);
            abi::store_at_offset(emitter, hi, offset - 8);
        }
        PhpType::TaggedScalar => {
            let lo = abi::int_result_reg(emitter);
            let hi = crate::codegen_support::sentinels::tagged_scalar_tag_reg(emitter);
            abi::store_at_offset(emitter, lo, offset);
            abi::store_at_offset(emitter, hi, offset - 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => abi::store_at_offset(emitter, abi::int_result_reg(emitter), offset),
    }
}

fn restore_return_value(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty.codegen_repr() {
        PhpType::Float => abi::load_at_offset(emitter, abi::float_result_reg(emitter), offset),
        PhpType::Str => {
            let (lo, hi) = abi::string_result_regs(emitter);
            abi::load_at_offset(emitter, lo, offset);
            abi::load_at_offset(emitter, hi, offset - 8);
        }
        PhpType::TaggedScalar => {
            let lo = abi::int_result_reg(emitter);
            let hi = crate::codegen_support::sentinels::tagged_scalar_tag_reg(emitter);
            abi::load_at_offset(emitter, lo, offset);
            abi::load_at_offset(emitter, hi, offset - 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature(params: Vec<(String, PhpType)>, variadic: Option<String>) -> FunctionSig {
        let len = params.len();
        FunctionSig {
            params,
            param_type_exprs: vec![None; len],
            param_attributes: vec![Vec::new(); len],
            defaults: vec![None; len],
            return_type: PhpType::Str,
            declared_return: true,
            by_ref_return: false,
            ref_params: vec![false; len],
            declared_params: vec![true; len],
            variadic,
            deprecation: None,
        }
    }

    #[test]
    fn planner_accepts_only_one_generated_collector_difference() {
        let source = signature(vec![("name".to_string(), PhpType::Str)], None);
        let mut physical = source.clone();
        physical.params.push((
            crate::func_args::HIDDEN_ARGS_PARAM.to_string(),
            PhpType::Array(Box::new(PhpType::Mixed)),
        ));
        physical.param_type_exprs.push(None);
        physical.param_attributes.push(Vec::new());
        physical.defaults.push(None);
        physical.ref_params.push(false);
        physical.declared_params.push(false);
        physical.variadic = Some(crate::func_args::HIDDEN_ARGS_PARAM.to_string());
        assert_eq!(
            plan_method_abi(&source, &physical).unwrap(),
            MethodAbiPlan::AppendEmptyCollector
        );
    }

    #[test]
    fn planner_rejects_source_variadic_actual_count_adaptation() {
        let source = signature(
            vec![("values".to_string(), PhpType::Mixed)],
            Some("values".to_string()),
        );
        let mut physical = source.clone();
        physical.params.insert(
            0,
            (crate::func_args::HIDDEN_ARGC_PARAM.to_string(), PhpType::Int),
        );
        physical.param_type_exprs.insert(0, None);
        physical.param_attributes.insert(0, Vec::new());
        physical.defaults.insert(0, None);
        physical.ref_params.insert(0, false);
        physical.declared_params.insert(0, false);
        assert!(plan_method_abi(&source, &physical)
            .unwrap_err()
            .contains("actual-count"));
    }

    #[test]
    fn source_vtable_rejects_a_source_variadic_hidden_actual_count() {
        let mut physical = signature(
            vec![("values".to_string(), PhpType::Mixed)],
            Some("values".to_string()),
        );
        assert_eq!(
            source_vtable_entry_symbol("C", "sort", &physical, MethodKind::Static).unwrap(),
            static_method_symbol("C", "sort")
        );
        physical.params.insert(
            0,
            (crate::func_args::HIDDEN_ARGC_PARAM.to_string(), PhpType::Int),
        );
        physical.param_type_exprs.insert(0, None);
        physical.param_attributes.insert(0, Vec::new());
        physical.defaults.insert(0, None);
        physical.ref_params.insert(0, false);
        physical.declared_params.insert(0, false);
        assert!(source_vtable_entry_symbol("C", "sort", &physical, MethodKind::Static)
            .unwrap_err()
            .contains("actual-count"));
    }

    #[test]
    fn by_reference_return_is_preserved_as_one_pointer_and_never_value_boxed() {
        let mut caller = signature(Vec::new(), None);
        caller.return_type = PhpType::Mixed;
        caller.by_ref_return = true;
        let mut physical = caller.clone();
        physical.return_type = PhpType::Str;
        physical.params.push((
            crate::func_args::HIDDEN_ARGS_PARAM.to_string(),
            PhpType::Array(Box::new(PhpType::Mixed)),
        ));
        physical.param_type_exprs.push(None);
        physical.param_attributes.push(Vec::new());
        physical.defaults.push(None);
        physical.ref_params.push(false);
        physical.declared_params.push(false);
        physical.variadic = Some(crate::func_args::HIDDEN_ARGS_PARAM.to_string());

        let mut mismatched_caller = caller.clone();
        mismatched_caller.by_ref_return = false;
        assert!(plan_method_abi(&mismatched_caller, &physical)
            .unwrap_err()
            .contains("by-reference return mismatch"));

        let mut emitter = Emitter::new(
            crate::codegen_support::platform::Target::parse("linux-x86_64").unwrap(),
        );
        emit_method_adapter(
            &mut emitter,
            "_test_by_ref_adapter",
            "_test_by_ref_physical",
            MethodKind::Instance,
            &caller,
            &physical,
            false,
        )
        .unwrap();
        assert!(!emitter.output().contains("__rt_mixed_from_value"));
        assert!(emit_method_adapter(
            &mut Emitter::new(
                crate::codegen_support::platform::Target::parse("linux-x86_64").unwrap(),
            ),
            "_test_invalid_by_ref_box",
            "_test_by_ref_physical",
            MethodKind::Instance,
            &caller,
            &physical,
            true,
        )
        .unwrap_err()
        .contains("by-reference"));
    }

    #[test]
    fn emitter_spills_before_allocating_and_balances_collector_ownership_on_all_targets() {
        let source = signature(vec![("name".to_string(), PhpType::Str)], None);
        let mut physical = source.clone();
        physical.params.push((
            crate::func_args::HIDDEN_ARGS_PARAM.to_string(),
            PhpType::Array(Box::new(PhpType::Mixed)),
        ));
        physical.param_type_exprs.push(None);
        physical.param_attributes.push(Vec::new());
        physical.defaults.push(None);
        physical.ref_params.push(false);
        physical.declared_params.push(false);
        physical.variadic = Some(crate::func_args::HIDDEN_ARGS_PARAM.to_string());

        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(
                crate::codegen_support::platform::Target::parse(target).unwrap(),
            );
            emit_method_adapter(
                &mut emitter,
                "_test_source_adapter",
                "_test_physical_method",
                MethodKind::Instance,
                &source,
                &physical,
                false,
            )
            .unwrap();
            let asm = emitter.output();
            let spill = asm.find("param $source_method_arg").unwrap();
            let allocation = asm.find("__rt_array_new").unwrap();
            let publish = asm.find("__rt_cleanup_call_operand_owner").unwrap();
            let call = asm.find("_test_physical_method").unwrap();
            let detach = asm.rfind("_exc_call_frame_top").unwrap();
            let release = asm.find("__rt_decref_any").unwrap();
            assert!(spill < allocation, "{target}: inputs must be spilled before allocation");
            assert!(allocation < publish, "{target}: allocation must precede publication");
            assert!(publish < call, "{target}: owner must be visible before entering PHP");
            assert!(call < detach, "{target}: normal return must detach the cleanup record");
            assert!(detach < release, "{target}: detach must precede normal decref");
            let stamp = if target == "linux-x86_64" {
                "mov r12, 7"
            } else {
                "mov x11, #7"
            };
            assert!(asm.contains(stamp), "{target}: {asm}");

            let mut wide_source = signature(
                (0..10)
                    .map(|index| (format!("arg{index}"), PhpType::Int))
                    .collect(),
                None,
            );
            wide_source.return_type = PhpType::Int;
            let mut wide_physical = wide_source.clone();
            wide_physical.params.push((
                crate::func_args::HIDDEN_ARGS_PARAM.to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            ));
            wide_physical.param_type_exprs.push(None);
            wide_physical.param_attributes.push(Vec::new());
            wide_physical.defaults.push(None);
            wide_physical.ref_params.push(false);
            wide_physical.declared_params.push(false);
            wide_physical.variadic = Some(crate::func_args::HIDDEN_ARGS_PARAM.to_string());
            let mut wide_emitter = Emitter::new(
                crate::codegen_support::platform::Target::parse(target).unwrap(),
            );
            emit_method_adapter(
                &mut wide_emitter,
                "_test_static_source_adapter",
                "_test_static_physical",
                MethodKind::Static,
                &wide_source,
                &wide_physical,
                false,
            )
            .unwrap();
            let wide_asm = wide_emitter.output();
            assert!(wide_asm.contains("from caller stack"), "{target}: {wide_asm}");
            assert!(wide_asm.contains("_test_static_physical"), "{target}: {wide_asm}");
        }
    }
}
