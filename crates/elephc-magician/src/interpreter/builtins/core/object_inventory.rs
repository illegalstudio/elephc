//! Purpose:
//! Collects initialized native and eval object slots for `get_mangled_object_vars()`.
//!
//! Called from:
//! - `super::runtime_introspection` for direct and callable Core dispatch.
//!
//! Key details:
//! - Private ancestor slots remain distinct from same-named child slots.
//! - Reads bypass property hooks and visibility without changing the caller's class scope.
//! - Eval storage markers and native storage markers are checked on their respective paths.

use super::super::super::*;
use super::super::collection_builder::EvalArrayBuilder;
use super::super::{eval_runtime_property_access_metadata, eval_runtime_string_array_to_vec};
use std::collections::HashSet;

/// Describes one physical object slot independently of its rendered visibility-mangled key.
struct ObjectPropertyEntry {
    name: String,
    storage: String,
    owner: String,
    visibility: EvalVisibility,
    native: bool,
}

impl ObjectPropertyEntry {
    /// Returns the PHP inventory key for this slot's declaring class and visibility.
    fn key(&self) -> String {
        match self.visibility {
            EvalVisibility::Public => self.name.clone(),
            EvalVisibility::Protected => format!("\0*\0{}", self.name),
            EvalVisibility::Private => format!("\0{}\0{}", self.owner.trim_start_matches('\\'), self.name),
        }
    }
}

/// Returns every initialized stored property, including inaccessible native ancestor slots.
pub(super) fn eval_get_mangled_object_vars(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = args else { return Err(EvalStatus::RuntimeFatal); };
    if values.type_tag(*object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let identity = values.object_identity(*object)?;
    let dynamic_class = context.dynamic_object_class_name(identity);
    let native_class = match &dynamic_class {
        Some(name) => context.class_native_parent_name(name),
        None => Some(runtime_object_class_name(*object, values)?),
    };
    let mut entries = Vec::new();
    if let Some(name) = &native_class {
        collect_native_entries(name, values, &mut entries)?;
    }
    if let Some(name) = &dynamic_class {
        for class in context.class_chain(name) {
            for property in class.properties() {
                if property.is_static() || property.is_virtual() { continue; }
                insert_entry(&mut entries, ObjectPropertyEntry {
                    name: property.name().to_string(),
                    storage: eval_instance_property_storage_name(class.name(), property),
                    owner: class.name().to_string(),
                    visibility: property.visibility(),
                    native: false,
                });
            }
        }
    }
    let mut storage_names = HashSet::new();
    let mut result = EvalArrayBuilder::assoc(values, entries.len())?;
    for entry in entries {
        storage_names.insert(entry.storage.clone());
        let shared_native_owner = if !entry.native {
            match native_class.as_deref() {
                Some(class) => eval_runtime_property_access_metadata(class, &entry.storage, result.values())?
                    .filter(|(_, visibility, is_static)| *visibility != EvalVisibility::Private && !is_static)
                    .map(|(owner, _, _)| owner),
                None => None,
            }
        } else { None };
        let access_scope = shared_native_owner.as_deref().unwrap_or(&entry.owner);
        let initialized = if entry.native || shared_native_owner.is_some() {
            eval_with_native_bridge_scope(access_scope, context, || {
                result.values().property_is_initialized(*object, &entry.storage)
            })?
        } else {
            context.dynamic_property_is_initialized(identity, &entry.storage)
        };
        if !initialized { continue; }
        let reference = (!entry.native).then(|| {
            context.dynamic_property_alias(identity, &entry.storage).cloned()
        }).flatten();
        result.string(&entry.key(), |values| {
            if let Some(reference) = &reference {
                let value = eval_reference_target_value(reference, context, values)?;
                return if value.is_borrowed() { values.retain(value) } else { Ok(value) };
            }
            // Independent child slots must not select a private native ancestor slot
            // just because the inventory's caller happens to be inside that ancestor.
            eval_with_native_bridge_scope(access_scope, context, || {
                values.property_get(*object, &entry.storage)
            })
        })?;
    }
    let property_count = result.values().object_property_len(*object)?;
    for position in 0..property_count {
        let key = result.values().object_property_iter_key(*object, position)?;
        let bytes = result.values().string_bytes(key);
        result.values().release(key)?;
        let name = String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
        if storage_names.insert(name.clone()) {
            result.string(&name, |values| values.property_get(*object, &name))?;
        }
    }
    Ok(result.finish())
}

/// Replaces non-private inherited slots in place without coalescing private declarations.
fn insert_entry(entries: &mut Vec<ObjectPropertyEntry>, entry: ObjectPropertyEntry) {
    if entry.visibility != EvalVisibility::Private {
        if let Some(previous) = entries.iter_mut().find(|previous| {
            previous.visibility != EvalVisibility::Private && previous.name == entry.name
        }) {
            *previous = entry;
            return;
        }
    }
    entries.push(entry);
}

/// Enumerates native declarations from root to child so private ancestors are not lost by lookup.
fn collect_native_entries(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
    entries: &mut Vec<ObjectPropertyEntry>,
) -> Result<(), EvalStatus> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        if !seen.insert(name.to_ascii_lowercase()) { break; }
        current = native_parent_name(&name, values)?;
        chain.push(name);
    }
    for name in chain.into_iter().rev() {
        let names = values.reflection_property_names(&name)?;
        let decoded = eval_runtime_string_array_to_vec(names, values);
        values.release(names)?;
        for property in decoded? {
            let Some((owner, visibility, is_static)) =
                eval_runtime_property_access_metadata(&name, &property, values)?
            else { continue; };
            if is_static || !owner.eq_ignore_ascii_case(&name) { continue; }
            if values.reflection_property_flags(&name, &property)?
                .is_some_and(|flags| flags & EVAL_REFLECTION_MEMBER_FLAG_VIRTUAL != 0)
            { continue; }
            insert_entry(entries, ObjectPropertyEntry {
                storage: property.clone(), name: property, owner, visibility, native: true,
            });
        }
    }
    Ok(())
}

/// Reads a native parent name while releasing the boxed metadata operands and result.
fn native_parent_name(
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let class = values.string(name)?;
    let parent = values.parent_class_name(class);
    values.release(class)?;
    let parent = parent?;
    let bytes = values.string_bytes(parent);
    values.release(parent)?;
    let name = String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok((!name.is_empty()).then_some(name))
}
