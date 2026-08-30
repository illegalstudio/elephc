//! Purpose:
//! Detects eval-owned PCNTL callables nested in values crossing back into AOT storage.
//!
//! Called from:
//! - Eval execution-result validation and the generated scope-reload ABI validator.
//!
//! Key details:
//! - Arrays and objects are walked transitively with identity guards for cyclic graphs.
//! - Container reads return owned cells, which the scan releases on every exit path.

use std::collections::HashSet;

use super::*;

/// Returns whether a value graph contains a callable whose metadata belongs to another eval context.
pub(crate) fn value_contains_foreign_pcntl_callable(
    root: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let mut pending = vec![(root, false)];
    let mut visited = HashSet::new();
    while let Some((value, owned)) = pending.pop() {
        if context.pcntl_foreign_callable_owner(value).is_some() {
            release_walk_values(value, owned, &mut pending, values);
            return Ok(true);
        }
        let tag = match values.type_tag(value) {
            Ok(tag) => tag,
            Err(status) => {
                release_walk_values(value, owned, &mut pending, values);
                return Err(status);
            }
        };
        match tag {
            EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
                let identity = match values.raw_value_word(value) {
                    Ok(identity) => identity,
                    Err(status) => {
                        release_walk_values(value, owned, &mut pending, values);
                        return Err(status);
                    }
                };
                if !visited.insert((tag, identity)) {
                    release_walk_value(value, owned, values);
                    continue;
                }
                let len = match values.array_len(value) {
                    Ok(len) => len,
                    Err(status) => {
                        release_walk_values(value, owned, &mut pending, values);
                        return Err(status);
                    }
                };
                for position in 0..len {
                    let key = match values.array_iter_key(value, position) {
                        Ok(key) => key,
                        Err(status) => {
                            release_walk_values(value, owned, &mut pending, values);
                            return Err(status);
                        }
                    };
                    let nested = match values.array_get(value, key) {
                        Ok(nested) => nested,
                        Err(status) => {
                            let _ = values.release(key);
                            release_walk_values(value, owned, &mut pending, values);
                            return Err(status);
                        }
                    };
                    if let Err(status) = values.release(key) {
                        let _ = values.release(nested);
                        release_walk_values(value, owned, &mut pending, values);
                        return Err(status);
                    }
                    pending.push((nested, true));
                }
            }
            EVAL_TAG_OBJECT => {
                let identity = match values.object_identity(value) {
                    Ok(identity) => identity,
                    Err(status) => {
                        release_walk_values(value, owned, &mut pending, values);
                        return Err(status);
                    }
                };
                if context
                    .closure_object_target(identity)
                    .is_some_and(|target| target.contains_foreign_context())
                {
                    release_walk_values(value, owned, &mut pending, values);
                    return Ok(true);
                }
                if !visited.insert((tag, identity)) {
                    release_walk_value(value, owned, values);
                    continue;
                }
                let class_name = match eval_debug_object_class_name(
                    value,
                    Some(identity),
                    context,
                    values,
                ) {
                    Ok(class_name) => class_name,
                    Err(status) => {
                        release_walk_values(value, owned, &mut pending, values);
                        return Err(status);
                    }
                };
                let properties = match eval_debug_object_properties(
                    value,
                    Some(identity),
                    &class_name,
                    context,
                    values,
                ) {
                    Ok(properties) => properties,
                    Err(status) => {
                        release_walk_values(value, owned, &mut pending, values);
                        return Err(status);
                    }
                };
                pending.extend(
                    properties
                        .into_iter()
                        .map(|property| (property.value, true)),
                );
            }
            _ => {}
        }
        release_walk_value(value, owned, values);
    }
    Ok(false)
}

/// Releases one container-read result when the graph walker owns it.
fn release_walk_value(
    value: RuntimeCellHandle,
    owned: bool,
    values: &mut impl RuntimeValueOps,
) {
    if owned {
        let _ = values.release(value);
    }
}

/// Releases the current value and all queued container-read results after an early exit.
fn release_walk_values(
    value: RuntimeCellHandle,
    owned: bool,
    pending: &mut Vec<(RuntimeCellHandle, bool)>,
    values: &mut impl RuntimeValueOps,
) {
    release_walk_value(value, owned, values);
    for (pending_value, pending_owned) in pending.drain(..) {
        release_walk_value(pending_value, pending_owned, values);
    }
}
