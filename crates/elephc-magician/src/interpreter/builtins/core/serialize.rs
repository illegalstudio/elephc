//! Purpose:
//! Declarative eval registry entry and implementation for `serialize`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - The wire format is php's own `var_export`-adjacent serialization grammar
//!   (`N;`, `b:0|1;`, `i:<int>;`, `d:<float>;`, `s:<byte-len>:"<bytes>";`,
//!   `a:<count>:{<key><value>...}`, `O:<name-len>:"<name>":<count>:{<key><value>...}`).
//! - Object property enumeration and visibility reuse the SAME machinery `print_r()`/
//!   `var_dump()` use (`eval_debug_object_properties`, `crate::interpreter::builtins::core::
//!   print_r`), so a private/protected property gets the correct NUL-mangled key
//!   (`"\0ClassName\0prop"` / `"\0*\0prop"`) instead of a plain (public) one -- the exact bug
//!   PLAN.md warns about: a wrong mangling here corrupts the app's own cache-freshness metadata
//!   with no crash, just a wrong decision.
//! - An object that appears again in the same structure is written as `r:<n>;`, where `n` is the
//!   1-based index of its first occurrence. php numbers every value it writes (an `r:` back-edge
//!   included) but never an array/object KEY, so the counter below advances once per value and
//!   not for keys; a self-referencing structure therefore ends in a back-edge instead of looping.
//!   `R:` (a PHP reference shared by two slots) is not written: the interpreter cannot tell two
//!   by-value copies from one reference here, so a shared scalar serializes as two plain values.
//! - NOT implemented: `Serializable` payloads.

use super::super::super::*;

eval_builtin! {
    contract: "serialize",
    area: Core,
    direct: Core,
    values: Core,
}

/// Dispatches a direct eval `serialize()` call, preserving PHP source argument order.
pub(in crate::interpreter) fn eval_builtin_serialize(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return eval_throw_argument_count_error(
            &format!(
                "serialize() expects exactly 1 argument, {} given",
                args.len()
            ),
            context,
            values,
        );
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_serialize_result(value, context, values)
}

/// Dispatches an evaluated-argument `serialize()` call (named args, spread, callable dispatch).
pub(in crate::interpreter) fn eval_serialize_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = evaluated_args else {
        return eval_throw_argument_count_error(
            &format!(
                "serialize() expects exactly 1 argument, {} given",
                evaluated_args.len()
            ),
            context,
            values,
        );
    };
    eval_serialize_result(*value, context, values)
}

/// Serializes ONE eval-owned object into its `O:len:"Class":n:{…}` fragment.
///
/// The generated serializer reads an object's class from its header, which for every object the
/// interpreter builds says `stdClass`, and its properties from the runtime hash, which is empty
/// because the overlay lives in the eval context. It wrote `O:8:"stdClass":0:{}` for a resource
/// Symfony's `ConfigCache::write()` handed it, and the next request read that back as a bare
/// stdClass. This renders the fragment the object really has.
#[cfg(not(test))]
pub(crate) fn eval_serialize_object_fragment(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_serialize_result(object, context, values)
}

/// php's value numbering for `r:` back-edges while one `serialize()` call writes its output.
struct EvalSerializeState {
    /// Index the next written value takes (1-based, as `r:<n>;` counts).
    next_index: u64,
    /// Objects already written, with the index of their first occurrence.
    objects: Vec<(usize, u64)>,
}

impl EvalSerializeState {
    /// Takes the next value index.
    fn take_index(&mut self) -> u64 {
        let index = self.next_index;
        self.next_index += 1;
        index
    }
}

/// Appends an array/object key, which php writes directly and does not number.
fn eval_serialize_append_key(
    key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    buf: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    if values.type_tag(key)? == EVAL_TAG_INT {
        let n = eval_int_value(key, values)?;
        buf.extend_from_slice(format!("i:{n};").as_bytes());
    } else {
        let bytes = values.string_bytes(key)?;
        buf.extend_from_slice(format!("s:{}:\"", bytes.len()).as_bytes());
        buf.extend_from_slice(&bytes);
        buf.extend_from_slice(b"\";");
    }
    Ok(())
}

/// Serializes one eval value into php's storable-representation byte string.
fn eval_serialize_result(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut buf = Vec::new();
    let mut state = EvalSerializeState { next_index: 1, objects: Vec::new() };
    eval_serialize_append_value(value, context, values, &mut state, &mut buf)?;
    values.string_bytes_value(&buf)
}

/// Appends one value's serialized byte representation, recursing into arrays and objects.
fn eval_serialize_append_value(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    state: &mut EvalSerializeState,
    buf: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    // Every value takes an index, the `r:` back-edge below included.
    let index = state.take_index();
    match values.type_tag(value)? {
        EVAL_TAG_NULL => buf.extend_from_slice(b"N;"),
        EVAL_TAG_BOOL => {
            buf.extend_from_slice(if values.truthy(value)? { b"b:1;" } else { b"b:0;" });
        }
        EVAL_TAG_INT => {
            let n = eval_int_value(value, values)?;
            buf.extend_from_slice(format!("i:{n};").as_bytes());
        }
        EVAL_TAG_FLOAT => {
            // MEASURED via the compiled `--web`-shaped probe (scratchpad/r200/gate.php
            // lotc_ser), not just the FakeOps unit fixture: `values.string_bytes()` on a float
            // cell answers php's `precision=14` echo/string-cast rule (`serialize(1/3)` ->
            // `0.33333333333333`, 14 threes), NOT `serialize_precision=-1`'s shortest
            // round-trip rule php's real `serialize()` uses (`0.3333333333333333`, 16 threes) --
            // reusing it here would have shipped a byte-for-byte wrong serialization for any
            // non-terminating fraction. `f64::to_string()` on the RAW bit pattern (read via
            // `raw_value_word`, bypassing that lossy string conversion entirely) already
            // implements shortest round-trip and was verified against `php -n` 8.5.6 for both
            // this repeating fraction and every terminating case (`1.0` -> `"1"`, `1.5` ->
            // `"1.5"`) via a standalone `rustc` check before this was written.
            let bits = values.raw_value_word(value)?;
            let float = f64::from_bits(bits);
            let wire = if float.is_nan() {
                "NAN".to_string()
            } else if float.is_infinite() {
                if float.is_sign_negative() { "-INF".to_string() } else { "INF".to_string() }
            } else {
                float.to_string()
            };
            buf.extend_from_slice(format!("d:{wire};").as_bytes());
        }
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(value)?;
            buf.extend_from_slice(format!("s:{}:\"", bytes.len()).as_bytes());
            buf.extend_from_slice(&bytes);
            buf.extend_from_slice(b"\";");
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let len = values.array_len(value)?;
            buf.extend_from_slice(format!("a:{len}:{{").as_bytes());
            for position in 0..len {
                let key = values.array_iter_key(value, position)?;
                let element = values.array_get(value, key)?;
                eval_serialize_append_key(key, values, buf)?;
                eval_serialize_append_value(element, context, values, state, buf)?;
            }
            buf.push(b'}');
        }
        EVAL_TAG_OBJECT => {
            let identity = eval_debug_object_identity(value, values);
            let object_key = identity.unwrap_or(value.as_ptr() as usize as u64) as usize;
            if let Some((_, first)) = state.objects.iter().find(|(seen, _)| *seen == object_key) {
                buf.extend_from_slice(format!("r:{first};").as_bytes());
                return Ok(());
            }
            state.objects.push((object_key, index));
            let class_name = eval_debug_object_class_name(value, identity, context, values)?;
            if eval_serialize_object_has_method(identity, &class_name, "__serialize", context, values)? {
                eval_serialize_append_magic_serialize(
                    value, &class_name, context, values, state, buf,
                )?;
                return Ok(());
            }
            let mut entries = eval_serialize_object_entries(value, identity, &class_name, context, values)?;
            if eval_serialize_object_has_method(identity, &class_name, "__sleep", context, values)? {
                match eval_serialize_sleep_names(value, &class_name, context, values)? {
                    Some(names) => {
                        entries = eval_serialize_sleep_entries(entries, &names, &class_name, values)?;
                    }
                    None => {
                        buf.extend_from_slice(b"N;");
                        return Ok(());
                    }
                }
            }
            buf.extend_from_slice(
                format!("O:{}:\"{}\":{}:{{", class_name.len(), class_name, entries.len())
                    .as_bytes(),
            );
            for (key, element) in &entries {
                buf.extend_from_slice(format!("s:{}:\"", key.len()).as_bytes());
                buf.extend_from_slice(key);
                buf.extend_from_slice(b"\";");
                eval_serialize_append_value(*element, context, values, state, buf)?;
            }
            buf.push(b'}');
        }
        // A resource carries no storable state; php serializes it as `i:0;` with no warning
        // (measured: `serialize(fopen(...))` -> `i:0;`, open or closed).
        _ => buf.extend_from_slice(b"i:0;"),
    }
    Ok(())
}

/// Whether the object's class declares `method`, in the interpreter or in the compiled program.
fn eval_serialize_object_has_method(
    identity: Option<u64>,
    class_name: &str,
    method: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let Some(class) = identity.and_then(|identity| context.dynamic_object_class(identity)) {
        let name = class.name().to_string();
        if context.class_method(&name, method).is_some() {
            return Ok(true);
        }
    }
    Ok(eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method, context, values)?.is_some())
}

/// Appends an object whose class defines `__serialize()`: its returned array becomes the body,
/// with the array's own keys, as php's `php_var_serialize_intern` writes it.
fn eval_serialize_append_magic_serialize(
    object: RuntimeCellHandle,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    state: &mut EvalSerializeState,
    buf: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let data = eval_method_call_result_with_evaluated_args(
        object,
        "__serialize",
        Vec::new(),
        context,
        values,
    )?;
    if !matches!(values.type_tag(data)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        values.release(data)?;
        return eval_throw_type_error(
            &format!("{class_name}::__serialize() must return an array"),
            context,
            values,
        );
    }
    let len = values.array_len(data)?;
    buf.extend_from_slice(format!("O:{}:\"{}\":{}:{{", class_name.len(), class_name, len).as_bytes());
    for position in 0..len {
        let key = values.array_iter_key(data, position)?;
        let element = values.array_get(data, key)?;
        eval_serialize_append_key(key, values, buf)?;
        eval_serialize_append_value(element, context, values, state, buf)?;
    }
    buf.push(b'}');
    values.release(data)
}

/// The object's properties as `(serialized key, value)` pairs, private and protected names
/// mangled the way php's wire format writes them.
///
/// An interpreter-declared object reads its overlay; a compiled object reads the compiled
/// program's `(array)` cast, which already carries php's mangled keys. Without that cast a
/// compiled object's private and protected state was invisible here, and it serialized as
/// `O:5:"Token":0:{}`.
fn eval_serialize_object_entries(
    object: RuntimeCellHandle,
    identity: Option<u64>,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(Vec<u8>, RuntimeCellHandle)>, EvalStatus> {
    let eval_owned =
        identity.is_some_and(|identity| context.dynamic_object_declaring_class(identity).is_some());
    if !eval_owned {
        if let Some(array) = values.compiled_object_to_array(object)? {
            let len = values.array_len(array)?;
            let mut entries = Vec::with_capacity(len);
            for position in 0..len {
                let key = values.array_iter_key(array, position)?;
                let element = values.array_get(array, key)?;
                let key_bytes = values.string_bytes(key)?;
                values.release(key)?;
                entries.push((key_bytes, element));
            }
            return Ok(entries);
        }
    }
    Ok(eval_debug_object_properties(object, identity, class_name, context, values)?
        .iter()
        .map(|property| (eval_serialize_mangled_property_key(property).into_bytes(), property.value))
        .collect())
}

/// Calls `__sleep()` and returns the property names it lists, or None when it did not return
/// an array (php warns and serializes the object as `N;`).
fn eval_serialize_sleep_names(
    object: RuntimeCellHandle,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Vec<Vec<u8>>>, EvalStatus> {
    let names = eval_method_call_result_with_evaluated_args(
        object,
        "__sleep",
        Vec::new(),
        context,
        values,
    )?;
    let warning = format!(
        "Warning: serialize(): {class_name}::__sleep() should return an array only containing the names of instance-variables to serialize\n"
    );
    if !matches!(values.type_tag(names)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        values.release(names)?;
        values.warning(&warning)?;
        return Ok(None);
    }
    let len = values.array_len(names)?;
    let mut listed = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(names, position)?;
        let name = values.array_get(names, key)?;
        values.release(key)?;
        if values.type_tag(name)? != EVAL_TAG_STRING {
            values.warning(&warning)?;
        }
        listed.push(values.string_bytes(name)?);
    }
    values.release(names)?;
    Ok(Some(listed))
}

/// Keeps the properties `__sleep()` named, in its order, trying each name as php does: as a
/// public name, then private to the object's class, then protected.
fn eval_serialize_sleep_entries(
    entries: Vec<(Vec<u8>, RuntimeCellHandle)>,
    names: &[Vec<u8>],
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(Vec<u8>, RuntimeCellHandle)>, EvalStatus> {
    let mut kept: Vec<(Vec<u8>, RuntimeCellHandle)> = Vec::with_capacity(names.len());
    for name in names {
        let mut private = vec![0u8];
        private.extend_from_slice(class_name.as_bytes());
        private.push(0);
        private.extend_from_slice(name);
        let mut protected = b"\0*\0".to_vec();
        protected.extend_from_slice(name);
        let found = [name.clone(), private, protected].into_iter().find_map(|candidate| {
            entries
                .iter()
                .find(|(key, _)| *key == candidate)
                .map(|(key, value)| (key.clone(), *value))
        });
        match found {
            Some(entry) if kept.iter().any(|(key, _)| *key == entry.0) => {
                values.warning(&format!(
                    "Warning: serialize(): \"{}\" is returned from __sleep() multiple times\n",
                    String::from_utf8_lossy(name)
                ))?;
            }
            Some(entry) => kept.push(entry),
            None => values.warning(&format!(
                "Warning: serialize(): \"{}\" returned as member variable from __sleep() but does not exist\n",
                String::from_utf8_lossy(name)
            ))?,
        }
    }
    Ok(kept)
}

/// Computes one object property's serialized key, NUL-mangling it for protected/private the
/// same way php's own wire format does: `"\0*\0name"` for protected, `"\0Class\0name"` for
/// private, and the plain name for public.
pub(in crate::interpreter) fn eval_serialize_mangled_property_key(property: &EvalDebugObjectProperty) -> String {
    match &property.visibility.kind {
        EvalDebugPropertyVisibilityKind::Public => property.name.clone(),
        EvalDebugPropertyVisibilityKind::Protected => format!("\0*\0{}", property.name),
        EvalDebugPropertyVisibilityKind::Private(class_name) => {
            format!("\0{class_name}\0{}", property.name)
        }
    }
}
