//! Purpose:
//! Builds push/unshift replacement arrays with balanced operand owners and retained references.
//!
//! Called from:
//! - The shared direct/dynamic push and unshift replacement adapter.
//!
//! Key details:
//! - Push retains existing keys and host append history; unshift reindexes only integer keys.
//! - Fresh output and extracted keys/elements remain rooted through fallible runtime calls.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Copies entries around prepared inserted values without publishing a partial replacement.
pub(super) fn build(name: &str, source: RuntimeCellHandle, inserted: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let prepend = name == "array_unshift";
    let len = values.array_len(source)?;
    let mut associative = values.type_tag(source)? == EVAL_TAG_ASSOC && !prepend;
    if prepend {
        for position in 0..len {
            let key = RuntimeOwnerSlot::new(values.array_iter_key(source, position)?);
            let tag = with_runtime_owners(&key, || {
                let tag = values.type_tag(key.borrowed());
                finish_read(tag, [key.take()], context, values)
            })?;
            associative |= tag == EVAL_TAG_STRING;
        }
    }
    let capacity = len.checked_add(inserted.len()).ok_or(EvalStatus::RuntimeFatal)?;
    let output = RuntimeOwnerSlot::new(if associative { values.assoc_new(capacity)? } else { values.array_new(capacity)? });
    with_runtime_owners(&output, || {
        let populated = (|| {
            let mut next_int = 0_i64;
            if prepend {
                for value in inserted { insert_numeric(output.borrowed(), next_int, *value, context, values)?;
                    next_int = next_int.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?; }
            }
            for position in 0..len {
                copy_entry(source, position, output.borrowed(), prepend, &mut next_int, context, values)?;
            }
            if !prepend {
                let mut next = values.array_next_index(source)?;
                for value in inserted {
                    let Some(index) = next else {
                        return eval_throw_error("Cannot add element to the array as the next element is already occupied", context, values);
                    };
                    insert_numeric(output.borrowed(), index, *value, context, values)?;
                    next = values.array_next_index(output.borrowed())?;
                }
            }
            Ok(())
        })();
        match populated { Ok(()) => Ok(output.take()),
            Err(status) => finish_eval_argument_values(Err(status), [output.take()], context, values) }
    })
}

/// Copies one owned element read and its alias metadata under the selected final key.
fn copy_entry(source: RuntimeCellHandle, position: usize, output: RuntimeCellHandle, reindex: bool,
    next_int: &mut i64, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let key = RuntimeOwnerSlot::new(values.array_iter_key(source, position)?);
    with_runtime_owners(&key, || {
        let result = (|| {
            let target_key = if reindex && values.type_tag(key.borrowed())? == EVAL_TAG_INT {
                let index = *next_int; *next_int = next_int.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
                values.int(index)?
            } else { values.retain(key.borrowed())? };
            let target_key = RuntimeOwnerSlot::new(target_key);
            with_runtime_owners(&target_key, || {
                let result = array_entry_copies::copy_entry(source, key.borrowed(), output,
                    target_key.borrowed(), context, values);
                finish_read(result, [target_key.take()], context, values)
            })
        })();
        finish_read(result, [key.take()], context, values)
    })
}

/// Inserts one borrowed value and immediately retires the generated key owner.
fn insert_numeric(output: RuntimeCellHandle, index: i64, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let key = RuntimeOwnerSlot::new(values.int(index)?);
    with_runtime_owners(&key, || {
        let result = values.array_set(output, key.borrowed(), value).map(|_| ());
        finish_read(result, [key.take()], context, values)
    })
}

/// Retires read temporaries while preserving the original evaluation or insertion failure.
fn finish_read<T>(result: Result<T, EvalStatus>, owners: impl IntoIterator<Item = RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let mut cleanup = Ok(());
    for owner in owners { let released = release_expr_result(owner, context, values);
        if cleanup.is_ok() { cleanup = released; } }
    result.and_then(|result| cleanup.map(|()| result))
}
