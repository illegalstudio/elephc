//! Purpose:
//! Backend-neutral EIR graphs used by compositional builtin semantic hooks.
//!
//! Called from:
//! - `crate::ir_lower::context` through `BuiltinLoweringContext`.
//!
//! Key details:
//! - Object projection, array-tail reads, and dynamic constant lookup preserve ownership and CFG joins.

use super::*;
use crate::ir::{RuntimeCallTarget, RuntimeFnId};

/// Materializes the declared properties visible to `get_object_vars()` in the current PHP scope.
pub(crate) fn lower_get_object_vars_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: ValueId,
    span: Span,
) -> Result<
    crate::builtins::semantics::LoweredBuiltinValue,
    crate::builtins::semantics::BuiltinLoweringError,
> {
    let object_type = ctx.builder.value_php_type(object);
    let is_static_date_object = singular_object_class(&object_type).is_some_and(
        |(class_name, _)| date_object_uses_virtual_property_shape(ctx, class_name),
    );
    if is_static_date_object {
        return lower_object_property_hash(ctx, object, span, false);
    }
    let result_type = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let value = ctx.emit_value(
        Op::RuntimeCall,
        vec![object],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::GetObjectVars,
        ))),
        result_type,
        RuntimeFnId::GetObjectVars.effects(),
        Some(span),
    );
    Ok(crate::builtins::semantics::LoweredBuiltinValue { value: value.value })
}

/// Materializes a date object's JSON-visible shape, recursively projecting nested date objects.
pub(super) fn lower_json_date_object_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: ValueId,
    span: Span,
) -> Result<
    crate::builtins::semantics::LoweredBuiltinValue,
    crate::builtins::semantics::BuiltinLoweringError,
> {
    let object_type = ctx.builder.value_php_type(object);
    let Some((class_name, _)) = singular_object_class(&object_type) else {
        return Err(crate::builtins::semantics::BuiltinLoweringError::new(
            "JSON date projection requires a statically known object class",
        ));
    };
    if class_extends_class(ctx, class_name, "DatePeriod") {
        lower_object_property_hash(ctx, object, span, true)
    } else {
        Ok(crate::builtins::semantics::LoweredBuiltinValue {
            value: lower_date_serialize_hash_from_object(ctx, object, span).value,
        })
    }
}

/// Calls the ext/date object's `__serialize()` hook to obtain its public JSON property hash.
pub(super) fn lower_date_serialize_hash_from_object(
    ctx: &mut LoweringContext<'_, '_>,
    object: ValueId,
    span: Span,
) -> LoweredValue {
    let hash_type = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let method = ctx.intern_string("__serialize");
    ctx.emit_value(
        Op::MethodCall,
        vec![object],
        Some(Immediate::Data(method)),
        hash_type,
        Op::MethodCall.default_effects(),
        Some(span),
    )
}

/// Builds a visible property hash, optionally replacing nested date objects with JSON hashes.
pub(super) fn lower_object_property_hash(
    ctx: &mut LoweringContext<'_, '_>,
    object: ValueId,
    span: Span,
    project_nested_date_objects: bool,
) -> Result<
    crate::builtins::semantics::LoweredBuiltinValue,
    crate::builtins::semantics::BuiltinLoweringError,
> {
    let object_type = ctx.builder.value_php_type(object);
    let Some((class_name, _)) = singular_object_class(&object_type) else {
        return Err(crate::builtins::semantics::BuiltinLoweringError::new(
            "get_object_vars() requires a statically known object class in AOT mode",
        ));
    };
    let normalized = class_name.trim_start_matches('\\');
    let Some(class_info) = ctx.classes.get(normalized) else {
        return Err(crate::builtins::semantics::BuiltinLoweringError::new(format!(
            "get_object_vars() cannot resolve class {normalized} in AOT mode",
        )));
    };
    let mut properties = class_info
        .properties
        .iter()
        .enumerate()
        .filter(|(index, (property, _))| {
            let is_date_period_virtual = class_extends_class(ctx, normalized, "DatePeriod")
                && crate::types::php_src_date_property_names("DatePeriod")
                    .is_some_and(|names| names.contains(&property.as_str()));
            is_date_period_virtual
                || (class_info.visible_property_index(property) == Some(*index)
                    && property_is_accessible_for_ir(ctx, normalized, class_info, property))
        })
        .map(|(_, (property, property_type))| {
            let declaring_class = class_info
                .property_declaring_classes
                .get(property)
                .cloned()
                .unwrap_or_else(|| normalized.to_string());
            let accessor = property_hook_get_method(property);
            let has_getter = class_info
                .methods
                .contains_key(&php_symbol_key(&accessor));
            (
                property.clone(),
                normalize_value_php_type(property_type.codegen_repr()),
                declaring_class,
                has_getter,
                accessor,
            )
        })
        .collect::<Vec<_>>();
    // php-src lists ordinary subclass properties before the virtual properties exposed by
    // internal date handlers. Stable sorting preserves declaration order within both groups.
    properties.sort_by_key(|(_, _, _, has_getter, _)| *has_getter);
    let hash_type = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(properties.len() as u32)),
        hash_type,
        Op::HashNew.default_effects(),
        Some(span),
    );
    for (property, property_type, _declaring_class, has_getter, accessor) in properties {
        let key_data = ctx.intern_string(&property);
        let key = ctx.emit_value(
            Op::ConstStr,
            Vec::new(),
            Some(Immediate::Data(key_data)),
            PhpType::Str,
            Op::ConstStr.default_effects(),
            Some(span),
        );
        let member = ctx.intern_string(if has_getter { &accessor } else { &property });
        let value = ctx.emit_value(
            if has_getter {
                Op::MethodCall
            } else {
                Op::PropGet
            },
            vec![object],
            Some(Immediate::Data(member)),
            property_type,
            if has_getter {
                Op::MethodCall.default_effects()
            } else {
                Op::PropGet.default_effects()
            },
            Some(span),
        );
        let date_period_nested_class = if project_nested_date_objects
            && class_extends_class(ctx, normalized, "DatePeriod")
        {
            match property.as_str() {
                "start" => Some("DateTimeImmutable"),
                "interval" => Some("DateInterval"),
                _ => None,
            }
        } else {
            None
        };
        let value = if let Some(nested_class) = date_period_nested_class {
            let unboxed = ctx.emit_value(
                Op::RuntimeCall,
                vec![value.value],
                None,
                PhpType::Object(nested_class.to_string()),
                effects_lookup::runtime_effects(),
                Some(span),
            );
            if ctx.value_is_owning_temporary(value) {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
            }
            let nested = lower_date_serialize_hash_from_object(ctx, unboxed.value, span);
            if ctx.value_is_owning_temporary(unboxed) {
                crate::ir_lower::ownership::release_if_owned(ctx, unboxed, Some(span));
            }
            LoweredValue {
                value: nested.value,
                ir_type: nested.ir_type,
            }
        } else if project_nested_date_objects
            && singular_object_class(&ctx.builder.value_php_type(value.value)).is_some_and(
                |(class_name, _)| date_object_uses_virtual_property_shape(ctx, class_name),
            )
        {
            let release_source = ctx.value_is_owning_temporary(value)
                && !ctx.value_is_owned_unboxed_local_load(value.value);
            let nested = lower_object_property_hash(ctx, value.value, span, true)?;
            if release_source {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
            }
            LoweredValue {
                value: nested.value,
                ir_type: ctx.builder.value_type(nested.value),
            }
        } else {
            value
        };
        let value = ensure_boxed_mixed(ctx, value, span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(span),
        );
    }
    Ok(crate::builtins::semantics::LoweredBuiltinValue {
        value: hash.value,
    })
}

/// Returns whether php-src exposes the class through ext/date virtual object properties.
pub(super) fn date_object_uses_virtual_property_shape(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> bool {
    [
        "DateTime",
        "DateTimeImmutable",
        "DateTimeZone",
        "DateInterval",
        "DatePeriod",
    ]
    .iter()
    .any(|base| class_extends_class(ctx, class_name, base))
}

/// Reads the final value of a PHP array and returns boxed `false` when empty.
pub(crate) fn lower_array_end_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    array: ValueId,
    span: Span,
) -> Result<
    crate::builtins::semantics::LoweredBuiltinValue,
    crate::builtins::semantics::BuiltinLoweringError,
> {
    let array_type = ctx.builder.value_php_type(array).codegen_repr();
    if matches!(array_type, PhpType::Mixed | PhpType::Union(_)) {
        return lower_dynamic_array_end_from_value(ctx, array, span);
    }
    let (len_op, value_type) = match &array_type {
        PhpType::Array(element) => (
            Op::ArrayLen,
            array_access_element_result_type(normalize_value_php_type(element.codegen_repr())),
        ),
        PhpType::AssocArray { value, .. } => (
            Op::HashLen,
            array_access_element_result_type(normalize_value_php_type(value.codegen_repr())),
        ),
        _ => {
            return Err(crate::builtins::semantics::BuiltinLoweringError::new(
                "end() requires a statically typed array in AOT mode",
            ));
        }
    };
    let len = ctx.emit_value(
        len_op,
        vec![array],
        None,
        PhpType::Int,
        len_op.default_effects(),
        Some(span),
    );
    let zero = emit_i64_at_span(ctx, 0, span);
    let nonempty = ctx.emit_value(
        Op::ICmp,
        vec![len.value, zero.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sgt)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let value_block = ctx.builder.create_named_block("end.value", Vec::new());
    let empty_block = ctx.builder.create_named_block("end.empty", Vec::new());
    let mixed_type = PhpType::Mixed;
    let merge = ctx.builder.create_named_block(
        "end.merge",
        vec![(value_ir_type(&mixed_type), mixed_type.clone())],
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: nonempty.value,
        then_target: value_block,
        then_args: Vec::new(),
        else_target: empty_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(value_block);
    let value = match array_type {
        PhpType::Array(_) => {
            let one = emit_i64_at_span(ctx, 1, span);
            let index = ctx.emit_value(
                Op::ISub,
                vec![len.value, one.value],
                None,
                PhpType::Int,
                Op::ISub.default_effects(),
                Some(span),
            );
            ctx.emit_value(
                Op::ArrayGet,
                vec![array, index.value],
                None,
                value_type,
                Op::ArrayGet.default_effects(),
                Some(span),
            )
        }
        PhpType::AssocArray { .. } => {
            let key = ctx.emit_value(
                Op::RuntimeCall,
                vec![array],
                Some(Immediate::RuntimeCall(
                    crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ArrayKeyLast),
                )),
                PhpType::Mixed,
                crate::ir::RuntimeFnId::ArrayKeyLast.effects(),
                Some(span),
            );
            let value = ctx.emit_value(
                Op::HashGet,
                vec![array, key.value],
                None,
                value_type,
                Op::HashGet.default_effects(),
                Some(span),
            );
            crate::ir_lower::ownership::release_if_owned(ctx, key, Some(span));
            value
        }
        _ => unreachable!("end() array representation was checked before graph construction"),
    };
    let value = ensure_boxed_mixed(ctx, value, span);
    ctx.builder.terminate(Terminator::Br {
        target: merge,
        args: vec![value.value],
    });

    ctx.builder.position_at_end(empty_block);
    let false_value = ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(false)),
        PhpType::False,
        Op::ConstBool.default_effects(),
        Some(span),
    );
    let false_value = ensure_boxed_mixed(ctx, false_value, span);
    ctx.builder.terminate(Terminator::Br {
        target: merge,
        args: vec![false_value.value],
    });

    ctx.builder.position_at_end(merge);
    Ok(crate::builtins::semantics::LoweredBuiltinValue {
        value: ctx.builder.block_param(merge, 0),
    })
}

/// Selects the final element when an array-producing expression is represented as boxed Mixed.
fn lower_dynamic_array_end_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    array: ValueId,
    span: Span,
) -> Result<
    crate::builtins::semantics::LoweredBuiltinValue,
    crate::builtins::semantics::BuiltinLoweringError,
> {
    let key = ctx.emit_value(
        Op::RuntimeCall,
        vec![array],
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ArrayKeyLast),
        )),
        PhpType::Mixed,
        crate::ir::RuntimeFnId::ArrayKeyLast.effects(),
        Some(span),
    );
    let empty = ctx.emit_value(
        Op::IsNull,
        vec![key.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(span),
    );
    let value_block = ctx
        .builder
        .create_named_block("end.dynamic.value", Vec::new());
    let empty_block = ctx
        .builder
        .create_named_block("end.dynamic.empty", Vec::new());
    let mixed_type = PhpType::Mixed;
    let merge = ctx.builder.create_named_block(
        "end.dynamic.merge",
        vec![(value_ir_type(&mixed_type), mixed_type.clone())],
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: empty.value,
        then_target: empty_block,
        then_args: Vec::new(),
        else_target: value_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(value_block);
    let warn_on_missing = emit_bool_literal(ctx, true, Some(span));
    let value = ctx.emit_value(
        Op::RuntimeCall,
        vec![array, key.value, warn_on_missing.value],
        None,
        PhpType::Mixed,
        Op::RuntimeCall.default_effects(),
        Some(span),
    );
    crate::ir_lower::ownership::release_if_owned(ctx, key, Some(span));
    ctx.builder.terminate(Terminator::Br {
        target: merge,
        args: vec![value.value],
    });

    ctx.builder.position_at_end(empty_block);
    crate::ir_lower::ownership::release_if_owned(ctx, key, Some(span));
    let false_value = ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(false)),
        PhpType::False,
        Op::ConstBool.default_effects(),
        Some(span),
    );
    let false_value = ensure_boxed_mixed(ctx, false_value, span);
    ctx.builder.terminate(Terminator::Br {
        target: merge,
        args: vec![false_value.value],
    });

    ctx.builder.position_at_end(merge);
    Ok(crate::builtins::semantics::LoweredBuiltinValue {
        value: ctx.builder.block_param(merge, 0),
    })
}

/// Reuses an existing Mixed cell or boxes a concrete value with balanced source ownership.
pub(super) fn ensure_boxed_mixed(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if value.ir_type == IrType::Heap(IrHeapKind::Mixed) {
        value
    } else {
        ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
    }
}

/// Selects a prescanned global constant by runtime string without introducing a backend builtin.
pub(crate) fn lower_constant_from_name_value(
    ctx: &mut LoweringContext<'_, '_>,
    name: ValueId,
    span: Span,
) -> Result<
    crate::builtins::semantics::LoweredBuiltinValue,
    crate::builtins::semantics::BuiltinLoweringError,
> {
    if ctx.builder.value_php_type(name).codegen_repr() != PhpType::Str {
        return Err(crate::builtins::semantics::BuiltinLoweringError::new(
            "constant() requires a string operand after call normalization",
        ));
    }
    let mut constants = ctx
        .constants
        .iter()
        .map(|(constant_name, (value, php_type))| {
            (constant_name.clone(), value.clone(), php_type.clone())
        })
        .collect::<Vec<_>>();
    constants.sort_by(|left, right| left.0.cmp(&right.0));
    let mixed_type = PhpType::Mixed;
    let merge = ctx.builder.create_named_block(
        "constant.merge",
        vec![(value_ir_type(&mixed_type), mixed_type.clone())],
    );
    for (constant_name, value, php_type) in constants {
        let matched = ctx
            .builder
            .create_named_block("constant.match", Vec::new());
        let next = ctx
            .builder
            .create_named_block("constant.next", Vec::new());
        let candidate_data = ctx.intern_string(&constant_name);
        let candidate = ctx.emit_value(
            Op::ConstStr,
            Vec::new(),
            Some(Immediate::Data(candidate_data)),
            PhpType::Str,
            Op::ConstStr.default_effects(),
            Some(span),
        );
        let equal = ctx.emit_value(
            Op::StrictEq,
            vec![name, candidate.value],
            None,
            PhpType::Bool,
            Op::StrictEq.default_effects(),
            Some(span),
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: equal.value,
            then_target: matched,
            then_args: Vec::new(),
            else_target: next,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(matched);
        let source = Expr::new(value.clone(), span);
        let value = constants::lower_constant_value(ctx, value, php_type, &source);
        let value = ctx.box_value_as_mixed(value, mixed_type.clone(), Some(span));
        ctx.builder.terminate(Terminator::Br {
            target: merge,
            args: vec![value.value],
        });
        ctx.builder.position_at_end(next);
    }
    let message = ctx.intern_string("Fatal error: Undefined constant name passed to constant()\n");
    ctx.builder.terminate(Terminator::Fatal { message });
    ctx.builder.position_at_end(merge);
    let value = ctx.builder.block_param(merge, 0);
    Ok(crate::builtins::semantics::LoweredBuiltinValue { value })
}
