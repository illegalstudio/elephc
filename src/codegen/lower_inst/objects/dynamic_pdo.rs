//! Purpose:
//! Lowers dynamic PDO class classification, statement construction, and initialization opcodes.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` through builtin PDO hooks.
//!
//! Key details:
//! - Runtime class names are matched against AOT metadata in stable class-id order.
//! - PDO statement constructors may be non-public and receive named/spread argument containers.

use super::*;

/// Lowers the internal class-name predicate used to gate PDO's post-hydration constructor call.
pub(in crate::codegen::lower_inst) fn lower_dynamic_class_has_constructor(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let class_name_value = expect_operand(inst, 0)?;
    let false_label = ctx.next_label("dynamic_has_ctor_false");
    let done_label = ctx.next_label("dynamic_has_ctor_done");
    if !emit_generic_dynamic_new_class_string(ctx, class_name_value, &false_label)? {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        return store_if_result(ctx, inst);
    }
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);

    let constructor_key = php_symbol_key("__construct");
    let mut classes = ctx
        .module
        .class_infos
        .iter()
        .filter(|(_, info)| info.methods.contains_key(&constructor_key))
        .collect::<Vec<_>>();
    classes.sort_by_key(|(_, info)| info.class_id);
    let matched_labels = classes
        .iter()
        .map(|(class_name, _)| {
            let label = ctx.next_label("dynamic_has_ctor_match");
            emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, class_name, &label);
            label
        })
        .collect::<Vec<_>>();

    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_jump(ctx.emitter, &done_label);
    for label in matched_labels {
        ctx.emitter.label(&label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        abi::emit_jump(ctx.emitter, &done_label);
    }
    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Classifies a runtime class name for PDO's `ATTR_STATEMENT_CLASS` contract.
///
/// Status 0 is an unknown class, 1 is a known class outside the PDOStatement hierarchy,
/// 2 is a PDOStatement subclass with a public user constructor, 3/4 are valid concrete/abstract
/// classes without a user constructor, and 5/6 are valid concrete/abstract classes with a
/// non-public user constructor. PHP accepts abstract statuses when setting the attribute and
/// rejects them only when `prepare()` attempts instantiation.
pub(in crate::codegen::lower_inst) fn lower_dynamic_pdo_statement_class_status(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let class_name_value = expect_operand(inst, 0)?;
    let unknown_label = ctx.next_label("pdo_statement_class_unknown");
    let done_label = ctx.next_label("pdo_statement_class_done");
    if !emit_generic_dynamic_new_class_string(ctx, class_name_value, &unknown_label)? {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        return store_if_result(ctx, inst);
    }
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);

    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, info)| info.class_id);
    let classified = classes
        .into_iter()
        .map(|(class_name, info)| {
            let user_constructor = pdo_statement_user_constructor(ctx, class_name);
            let has_user_constructor = user_constructor.is_some();
            let status = if !class_extends_class(ctx, class_name, "PDOStatement") {
                1
            } else if has_user_constructor
                && user_constructor
                    .as_ref()
                    .is_some_and(|(_, visibility, _)| visibility == &Visibility::Public)
            {
                2
            } else if info.is_abstract {
                if has_user_constructor { 6 } else { 4 }
            } else if has_user_constructor {
                5
            } else {
                3
            };
            let label = ctx.next_label("pdo_statement_class_match");
            emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, class_name, &label);
            (label, status)
        })
        .collect::<Vec<_>>();

    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_jump(ctx.emitter, &done_label);
    for (label, status) in classified {
        ctx.emitter.label(&label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), status);
        abi::emit_jump(ctx.emitter, &done_label);
    }
    ctx.emitter.label(&unknown_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Classifies a runtime class name for PHP 8.4+'s late-static `PDO::connect()`.
///
/// Status 0 is exactly PDO, 1..=7 are the SQLite/MySQL/PostgreSQL/DBLIB/
/// Firebird/ODBC/IBM driver hierarchies, 8 is a generic PDO subclass, and 9 is unknown.
pub(in crate::codegen::lower_inst) fn lower_dynamic_pdo_called_class_status(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let class_name_value = expect_operand(inst, 0)?;
    let unknown_label = ctx.next_label("pdo_called_class_unknown");
    let done_label = ctx.next_label("pdo_called_class_done");
    if !emit_generic_dynamic_new_class_string(ctx, class_name_value, &unknown_label)? {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 9);
        return store_if_result(ctx, inst);
    }
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);

    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, info)| info.class_id);
    let classified = classes
        .into_iter()
        .map(|(class_name, _)| {
            let status = if same_php_type_name(class_name, "PDO") {
                0
            } else if class_extends_class(ctx, class_name, "Pdo\\Sqlite") {
                1
            } else if class_extends_class(ctx, class_name, "Pdo\\Mysql") {
                2
            } else if class_extends_class(ctx, class_name, "Pdo\\Pgsql") {
                3
            } else if class_extends_class(ctx, class_name, "Pdo\\Dblib") {
                4
            } else if class_extends_class(ctx, class_name, "Pdo\\Firebird") {
                5
            } else if class_extends_class(ctx, class_name, "Pdo\\Odbc") {
                6
            } else if class_extends_class(ctx, class_name, "Pdo\\Ibm") {
                7
            } else if class_extends_class(ctx, class_name, "PDO") {
                8
            } else {
                9
            };
            let label = ctx.next_label("pdo_called_class_match");
            emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, class_name, &label);
            (label, status)
        })
        .collect::<Vec<_>>();

    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 9);
    abi::emit_jump(ctx.emitter, &done_label);
    for (label, status) in classified {
        ctx.emitter.label(&label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), status);
        abi::emit_jump(ctx.emitter, &done_label);
    }
    ctx.emitter.label(&unknown_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 9);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Invokes the protected/private constructor selected by PDO statement-class metadata.
pub(in crate::codegen::lower_inst) fn lower_dynamic_pdo_statement_constructor_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let class_name_value = expect_operand(inst, 0)?;
    let statement_value = expect_operand(inst, 1)?;
    let argument_container = expect_operand(inst, 2)?;
    let unmatched_label = ctx.next_label("pdo_statement_constructor_unmatched");
    let done_label = ctx.next_label("pdo_statement_constructor_done");
    if !emit_generic_dynamic_new_class_string(ctx, class_name_value, &unmatched_label)? {
        emit_void_sentinel(ctx);
        return store_if_result(ctx, inst);
    }
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);

    let mut candidates = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, info)| info.class_id);
    for (class_name, info) in classes {
        let Some((owner, visibility, signature)) =
            pdo_statement_user_constructor(ctx, class_name)
        else {
            continue;
        };
        if !class_extends_class(ctx, class_name, "PDOStatement")
            || info.is_abstract
            || visibility == Visibility::Public
        {
            continue;
        }
        let Some(mut candidate) = dynamic_new_candidate(ctx, class_name, info, None, inst)? else {
            continue;
        };
        candidate.constructor_impl = Some(ConstructorCallTarget {
            impl_class: owner,
            param_types: signature
                .params
                .iter()
                .map(|(_, ty)| ty.codegen_repr())
                .collect(),
            ref_params: signature.ref_params.clone(),
            sig: signature,
            padding_thunk: None,
        });
        let label = ctx.next_label("pdo_statement_constructor_match");
        emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, class_name, &label);
        candidates.push((candidate, label));
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_jump(ctx.emitter, &done_label);

    for (candidate, label) in candidates {
        ctx.emitter.label(&label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        ctx.load_value_to_reg(statement_value, abi::int_result_reg(ctx.emitter))?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        move_mixed_unboxed_object_payload(ctx, abi::int_result_reg(ctx.emitter));
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        let constructor = candidate.constructor_impl.as_ref().ok_or_else(|| {
            CodegenIrError::invalid_module("PDO statement constructor candidate has no constructor")
        })?;
        emit_dynamic_new_mixed_constructor_container_call(
            ctx,
            &candidate,
            constructor,
            argument_container,
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&unmatched_label);
    ctx.emitter.label(&done_label);
    emit_void_sentinel(ctx);
    store_if_result(ctx, inst)
}

/// Materializes the EIR void sentinel after an internal side-effect-only constructor call.
fn emit_void_sentinel(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        RUNTIME_NULL_SENTINEL,
    );
}

/// Resolves PDO's effective user constructor, including private ancestor constructors retained by
/// php-src for `ATTR_STATEMENT_CLASS` even though ordinary PHP inheritance omits them.
fn pdo_statement_user_constructor(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> Option<(String, Visibility, crate::types::FunctionSig)> {
    let constructor_key = php_symbol_key("__construct");
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        let info = class_info_by_name(ctx, &name)?;
        if let Some(signature) = info.methods.get(&constructor_key) {
            let owner = info
                .method_impl_classes
                .get(&constructor_key)
                .cloned()
                .unwrap_or_else(|| name.clone());
            if same_php_type_name(&owner, "PDOStatement") {
                return None;
            }
            let visibility = info
                .method_visibilities
                .get(&constructor_key)
                .cloned()
                .unwrap_or(Visibility::Public);
            return Some((owner, visibility, signature.clone()));
        }
        current = info.parent.clone();
    }
    None
}

/// Calls PDOStatement's private base initializer on an already allocated subclass object.
pub(in crate::codegen::lower_inst) fn lower_dynamic_pdo_statement_initialize(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let statement = expect_operand(inst, 0)?;
    ctx.load_value_to_reg(statement, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    move_mixed_unboxed_object_payload(ctx, abi::int_result_reg(ctx.emitter));
    let receiver_reg = abi::int_result_reg(ctx.emitter).to_string();
    let params = [
        PhpType::Object("PDOStatement".to_string()),
        PhpType::Int,
        PhpType::Int,
        PhpType::Int,
        PhpType::Str,
    ];
    let refs = [false, false, false, false, false];
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        &receiver_reg,
        &params[0],
        &inst.operands,
        &params,
        &refs,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(
        ctx.emitter,
        &method_symbol("PDOStatement", &php_symbol_key("__elephcInitialize")),
    );
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    emit_ref_arg_writebacks(ctx, &call_args)?;
    emit_void_sentinel(ctx);
    store_if_result(ctx, inst)
}
