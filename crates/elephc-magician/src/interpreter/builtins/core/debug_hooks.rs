//! Purpose:
//! Invokes user debug hooks before default object projections and materializes their fields.
//!
//! Called from:
//! - The shared print_r/var_dump object-property collector.
//!
//! Key details:
//! - Method declaration lookup does not fall back to __call.
//! - Fetched field cells own references; unclassified hook result owners are not guessed.

use super::super::super::*;
use super::print_r::{EvalDebugObjectProperty, EvalDebugPropertyVisibility, EvalDebugPropertyVisibilityKind};

/// Invokes a declared __debugInfo hook or declines objects with no hook.
pub(in crate::interpreter) fn eval_user_debug_properties(
    object: RuntimeCellHandle, class_name: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<Vec<EvalDebugObjectProperty>>, EvalStatus> {
    let (result, owner) = if let Some(owner) = context.class_method(class_name, "__debugInfo")
        .map(|(owner, _)| owner.to_string())
    {
        context.push_class_scope(owner.clone());
        let mut owned = false;
        let result = eval_method_call_result_with_ownership(object, "__debugInfo", Vec::new(),
            context, values, &mut owned);
        context.pop_class_scope();
        (result.map(|value| EvalExprResult { value, owned }), owner)
    } else if let Some(owner) = values.reflection_method_declaring_class(class_name, "__debugInfo")? {
        let result = eval_native_method_result_unchecked_bridge_scope(object, &owner, "__debugInfo",
            Vec::new(), Some(&owner), Some(class_name), context, values);
        (result, owner)
    } else {
        return Ok(None);
    };
    let result = result?;
    let properties = (|| {
        if values.is_null(result.value)? {
            if crate::eval_php_profile::eval_php_version_id() >= 80_500 {
                values.deprecated(&format!("Returning null from {owner}::__debugInfo() is deprecated, return an empty array instead"))?;
            }
            Ok(Vec::new())
        } else {
            hook_properties(result.value, context, values)
        }
    })();
    let cleanup = if result.owned { eval_release_value(context, values, result.value) } else { Ok(()) };
    match (properties, cleanup) {
        (Ok(properties), Ok(())) => Ok(Some(properties)),
        (Ok(properties), Err(status)) => {
            let _ = release_debug_properties(properties, context, values);
            Err(status)
        }
        (Err(status), _) => Err(status),
    }
}

/// Reads one hook key and releases its temporary owner even if conversion fails.
fn hook_key(key: RuntimeCellHandle, values: &mut impl RuntimeValueOps)
    -> Result<(String, bool), EvalStatus>
{
    let result = (|| {
        let numeric = values.type_tag(key)? == EVAL_TAG_INT;
        let bytes = if numeric {
            let text = values.cast_string(key)?;
            let bytes = values.string_bytes(text);
            let cleanup = values.release(text);
            bytes.and_then(|bytes| cleanup.map(|()| bytes))?
        } else { values.string_bytes(key)? };
        let name = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
        Ok((name, numeric))
    })();
    let cleanup = values.release(key);
    result.and_then(|key| cleanup.map(|()| key))
}

/// Preserves integer keys and PHP's visibility-mangled string keys from a hook array.
fn hook_properties(snapshot: RuntimeCellHandle, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps) -> Result<Vec<EvalDebugObjectProperty>, EvalStatus>
{
    if !values.is_array_like(snapshot)? { return Err(EvalStatus::RuntimeFatal); }
    let mut properties = Vec::new();
    let result = (|| {
        for position in 0..values.array_len(snapshot)? {
            let key = values.array_iter_key(snapshot, position)?;
            let value = values.array_get(snapshot, key);
            let key = hook_key(key, values);
            let (value, (mut name, numeric_name)) = match (value, key) {
                (Ok(value), Ok(key)) => (value, key),
                (Ok(value), Err(status)) => { let _ = values.release(value); return Err(status); }
                (Err(status), _) => return Err(status),
            };
            let mut kind = EvalDebugPropertyVisibilityKind::Public;
            if let Some(rest) = name.strip_prefix('\0') {
                if let Some((scope, field)) = rest.split_once('\0') {
                    kind = if scope == "*" { EvalDebugPropertyVisibilityKind::Protected }
                        else { EvalDebugPropertyVisibilityKind::Private(scope.to_string()) };
                    name = field.to_string();
                }
            }
            properties.push(EvalDebugObjectProperty {
                name, numeric_name, visibility: EvalDebugPropertyVisibility { kind }, value,
                is_reference: false, owned_value: true,
            });
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(properties),
        Err(status) => { let _ = release_debug_properties(properties, context, values); Err(status) }
    }
}
