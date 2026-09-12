//! Purpose:
//! Static and instance method callback target resolution.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Resolved static-method callback metadata for a small runtime helper wrapper.
pub(super) struct StaticMethodCallbackTarget {
    pub(super) entry_label: String,
    pub(super) called_class: StaticCallbackCalledClass,
    pub(super) dynamic_slot: Option<usize>,
    pub(super) env_source: Option<StaticCallbackEnvSource>,
    pub(super) param_types: Vec<PhpType>,
    pub(super) return_ty: PhpType,
}

/// Resolved instance-method callback metadata for sort runtime wrappers.
pub(super) struct InstanceMethodCallbackTarget {
    pub(super) entry_label: String,
    pub(super) receiver: ValueId,
    pub(super) param_types: Vec<PhpType>,
    pub(super) return_ty: PhpType,
}

/// Source used by a callback wrapper to materialize the hidden called-class id.
pub(super) enum StaticCallbackCalledClass {
    Immediate(u64),
    Env,
}

/// Source used by the sort call site to build the callback environment.
#[derive(Clone)]
pub(super) enum StaticCallbackEnvSource {
    Local(LocalSlotId),
    ThisObject(LocalSlotId),
    Value(ValueId),
    FunctionLabel(String),
}

/// Resolves a sort static-method callback, allowing `static::` with an environment.
pub(super) fn static_method_sort_callback_target(
    ctx: &FunctionContext<'_>,
    callback_name: &str,
    owner: &str,
    visible_arg_types: Option<&[PhpType]>,
) -> Result<Option<StaticMethodCallbackTarget>> {
    static_method_callback_target_inner(ctx, callback_name, owner, visible_arg_types, true)
}

/// Resolves static-method callback metadata and optionally supports late-static env dispatch.
pub(super) fn static_method_callback_target_inner(
    ctx: &FunctionContext<'_>,
    callback_name: &str,
    owner: &str,
    visible_arg_types: Option<&[PhpType]>,
    allow_static_env: bool,
) -> Result<Option<StaticMethodCallbackTarget>> {
    let Some((receiver, method)) = callback_name.rsplit_once("::") else {
        return Ok(None);
    };
    let receiver = receiver.trim_start_matches('\\');
    if receiver == "static" && allow_static_env {
        return static_late_bound_method_callback_target(ctx, method, owner, visible_arg_types);
    }
    if matches!(receiver, "self" | "parent" | "static" | "object") {
        return Err(CodegenIrError::unsupported(format!(
            "{} with lexical or receiver-bound static method callback '{}'",
            owner, callback_name
        )));
    }
    let visible_arg_types = visible_arg_types.ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} '{}' with dynamic callback argument shape",
            owner, callback_name
        ))
    })?;
    require_static_method_callback_arg_types(owner, callback_name, visible_arg_types)?;
    let receiver_info = ctx.module.class_infos.get(receiver).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} with unknown static method callback class '{}'",
            owner, receiver
        ))
    })?;
    let method_key = php_symbol_key(method);
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(receiver);
    let impl_info = ctx.module.class_infos.get(impl_class).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} with unknown static method implementation class '{}'",
            owner, impl_class
        ))
    })?;
    let sig = impl_info.static_methods.get(&method_key).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} with unknown static method callback '{}'",
            owner, callback_name
        ))
    })?;
    let source_sig = crate::codegen_support::source_method_adapters::source_visible_signature(sig)
        .map_err(CodegenIrError::unsupported)?;
    if source_sig.params.len() != visible_arg_types.len() {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with {} visible args for {} params",
            owner,
            callback_name,
            visible_arg_types.len(),
            source_sig.params.len()
        )));
    }
    require_static_method_callback_param_types(owner, callback_name, &source_sig, visible_arg_types)?;
    Ok(Some(StaticMethodCallbackTarget {
        entry_label: crate::codegen_support::source_method_adapters::source_method_entry_symbol(
            impl_class,
            &method_key,
            sig,
            crate::codegen_support::source_method_adapters::MethodKind::Static,
        )
        .map_err(CodegenIrError::unsupported)?,
        called_class: StaticCallbackCalledClass::Immediate(receiver_info.class_id),
        dynamic_slot: None,
        env_source: None,
        param_types: source_sig
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect(),
        return_ty: sig.return_type.codegen_repr(),
    }))
}

/// Resolves a late-bound `static::method(...)` callback target for sort runtime wrappers.
pub(super) fn static_late_bound_method_callback_target(
    ctx: &FunctionContext<'_>,
    method: &str,
    owner: &str,
    visible_arg_types: Option<&[PhpType]>,
) -> Result<Option<StaticMethodCallbackTarget>> {
    let receiver = current_callback_class(ctx)?;
    let callback_name = format!("static::{}", method);
    let visible_arg_types = visible_arg_types.ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} '{}' with dynamic callback argument shape",
            owner, callback_name
        ))
    })?;
    require_static_method_callback_arg_types(owner, &callback_name, visible_arg_types)?;
    let receiver_info = ctx.module.class_infos.get(receiver).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} with unknown static callback receiver class '{}'",
            owner, receiver
        ))
    })?;
    let method_key = php_symbol_key(method);
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(receiver);
    let impl_info = ctx.module.class_infos.get(impl_class).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} with unknown static method implementation class '{}'",
            owner, impl_class
        ))
    })?;
    let sig = impl_info.static_methods.get(&method_key).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} with unknown static method callback '{}'",
            owner, callback_name
        ))
    })?;
    let source_sig = crate::codegen_support::source_method_adapters::source_visible_signature(sig)
        .map_err(CodegenIrError::unsupported)?;
    if source_sig.params.len() != visible_arg_types.len() {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with {} visible args for {} params",
            owner,
            callback_name,
            visible_arg_types.len(),
            source_sig.params.len()
        )));
    }
    require_static_method_callback_param_types(
        owner,
        &callback_name,
        &source_sig,
        visible_arg_types,
    )?;
    Ok(Some(StaticMethodCallbackTarget {
        entry_label: crate::codegen_support::source_method_adapters::source_method_entry_symbol(
            impl_class,
            &method_key,
            sig,
            crate::codegen_support::source_method_adapters::MethodKind::Static,
        )
        .map_err(CodegenIrError::unsupported)?,
        called_class: StaticCallbackCalledClass::Env,
        dynamic_slot: receiver_info.static_vtable_slots.get(&method_key).copied(),
        env_source: Some(static_callback_env_source(ctx)?),
        param_types: source_sig
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect(),
        return_ty: sig.return_type.codegen_repr(),
    }))
}

/// Returns the lexical class for the current EIR class method.
pub(super) fn current_callback_class<'a>(ctx: &'a FunctionContext<'_>) -> Result<&'a str> {
    ctx.function
        .name
        .rsplit_once("::")
        .map(|(class_name, _)| class_name)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "static callback outside class method {}",
                ctx.function.name
            ))
        })
}

/// Returns the current called-class id source available to a late-static callback.
pub(super) fn static_callback_env_source(ctx: &FunctionContext<'_>) -> Result<StaticCallbackEnvSource> {
    if let Some(slot) = ctx.local_slot_by_name(crate::names::CALLED_CLASS_ID_LOCAL) {
        return Ok(StaticCallbackEnvSource::Local(slot));
    }
    if let Some(slot) = ctx.local_slot_by_name("this") {
        return Ok(StaticCallbackEnvSource::ThisObject(slot));
    }
    Err(CodegenIrError::unsupported(format!(
        "static callback without called-class context in {}",
        ctx.function.name
    )))
}

/// Resolves an `object::method(...)` callback target and its captured receiver for sort helpers.
pub(super) fn instance_method_sort_callback_target(
    ctx: &FunctionContext<'_>,
    callback: &StaticCallbackName,
    owner: &str,
    visible_arg_types: Option<&[PhpType]>,
) -> Result<Option<InstanceMethodCallbackTarget>> {
    let Some((receiver_label, method)) = callback.name.rsplit_once("::") else {
        return Ok(None);
    };
    if receiver_label.trim_start_matches('\\') != "object" {
        return Ok(None);
    }
    let Some(receiver) = callback.receiver else {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' without captured receiver operand",
            owner, callback.name
        )));
    };
    let visible_arg_types = visible_arg_types.ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} '{}' with dynamic callback argument shape",
            owner, callback.name
        ))
    })?;
    require_static_method_callback_arg_types(owner, &callback.name, visible_arg_types)?;
    let receiver_ty = ctx.value_php_type(receiver)?.codegen_repr();
    let PhpType::Object(class_name) = receiver_ty else {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with receiver PHP type {:?}",
            owner, callback.name, receiver_ty
        )));
    };
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx.module.class_infos.get(normalized).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} with unknown instance callback class '{}'",
            owner, normalized
        ))
    })?;
    let method_key = php_symbol_key(method);
    let sig = class_info.methods.get(&method_key).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} with unknown instance method callback '{}'",
            owner, callback.name
        ))
    })?;
    let source_sig = crate::codegen_support::source_method_adapters::source_visible_signature(sig)
        .map_err(CodegenIrError::unsupported)?;
    if source_sig.params.len() != visible_arg_types.len() {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with {} visible args for {} params",
            owner,
            callback.name,
            visible_arg_types.len(),
            source_sig.params.len()
        )));
    }
    require_static_method_callback_param_types(
        owner,
        &callback.name,
        &source_sig,
        visible_arg_types,
    )?;
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(normalized);
    if !instance_method_already_emitted(ctx, impl_class, &method_key) {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' without emitted EIR method body",
            owner, callback.name
        )));
    }
    Ok(Some(InstanceMethodCallbackTarget {
        entry_label: crate::codegen_support::source_method_adapters::source_method_entry_symbol(
            impl_class,
            &method_key,
            sig,
            crate::codegen_support::source_method_adapters::MethodKind::Instance,
        )
        .map_err(CodegenIrError::unsupported)?,
        receiver,
        param_types: source_sig
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect(),
        return_ty: sig.return_type.codegen_repr(),
    }))
}

/// Returns true when the instance callback target has a generated EIR method body.
pub(super) fn instance_method_already_emitted(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
) -> bool {
    ctx.module.class_methods.iter().any(|function| {
        !function.flags.is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(class, method)| {
                    class == class_name && php_symbol_key(method) == method_key
                })
    })
}

/// Verifies the wrapper can forward the callback argument ABI without boxing or shuffling pairs.
///
/// String arguments occupy two integer ABI slots, so they are only forwarded when every
/// visible argument is a string: that is the shape the runtime callback helpers actually
/// produce (a single-string element callback, or the two-string `usort()` comparator).
/// A mixed string/scalar list would need a register-shuffling wrapper no runtime helper
/// currently calls, so it stays a diagnosed unsupported feature.
pub(super) fn require_static_method_callback_arg_types(
    owner: &str,
    callback_name: &str,
    visible_arg_types: &[PhpType],
) -> Result<()> {
    if !(1..=2).contains(&visible_arg_types.len()) {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with {} visible callback args",
            owner,
            callback_name,
            visible_arg_types.len()
        )));
    }
    if visible_arg_types
        .iter()
        .any(|ty| matches!(ty.codegen_repr(), PhpType::Str))
        && !(visible_arg_types.len() == 1
            && matches!(visible_arg_types[0].codegen_repr(), PhpType::Str))
        && !visible_arg_types
            .iter()
            .all(|ty| matches!(ty.codegen_repr(), PhpType::Str))
    {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with mixed string and scalar callback args",
            owner, callback_name
        )));
    }
    for ty in visible_arg_types {
        if !matches!(
            ty.codegen_repr(),
            PhpType::Int | PhpType::Bool | PhpType::Str | PhpType::Void | PhpType::Never
        ) {
            return Err(CodegenIrError::unsupported(format!(
                "{} '{}' with unsupported callback arg type {:?}",
                owner,
                callback_name,
                ty.codegen_repr()
            )));
        }
    }
    Ok(())
}

/// Verifies the target method can consume the wrapper's runtime callback ABI values.
pub(super) fn require_static_method_callback_param_types(
    owner: &str,
    callback_name: &str,
    sig: &crate::types::FunctionSig,
    visible_arg_types: &[PhpType],
) -> Result<()> {
    for ((_, param_ty), visible_ty) in sig.params.iter().zip(visible_arg_types.iter()) {
        let param_ty = param_ty.codegen_repr();
        let visible_ty = visible_ty.codegen_repr();
        if matches!(visible_ty, PhpType::Void | PhpType::Never) {
            continue;
        }
        if param_ty == PhpType::Mixed {
            continue;
        }
        if matches!((&param_ty, &visible_ty), (PhpType::Int | PhpType::Bool, PhpType::Int | PhpType::Bool)) {
            continue;
        }
        if matches!((&param_ty, &visible_ty), (PhpType::Str, PhpType::Str)) {
            continue;
        }
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with callback param type {:?} for runtime arg type {:?}",
            owner, callback_name, param_ty, visible_ty
        )));
    }
    Ok(())
}

pub(super) const NO_CALLBACK_BOX_OFFSET: usize = usize::MAX;
