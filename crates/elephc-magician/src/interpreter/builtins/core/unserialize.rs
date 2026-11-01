//! Purpose:
//! Declarative eval registry entry and implementation for `unserialize`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - Parses php's own storable-representation grammar: `N;`, `b:0|1;`, `i:<int>;`, `d:<float>;`,
//!   `s:<byte-len>:"<bytes>";`, `a:<count>:{<key><value>...}`,
//!   `O:<name-len>:"<name>":<count>:{<key><value>...}`.
//! - Object hydration for a class-name-allowed payload writes each DECLARED property through the
//!   SAME storage-name + overlay + slot-mirror machinery `eval_dynamic_class_allocate_object()`
//!   uses when it seeds default property values (`crate::interpreter::statements::
//!   class_resolution`), so a private property from an UNMANGLED payload key
//!   (`s:1:"a";s:2:"AA";`) lands in the DECLARED private slot -- `"\0ClassName\0a"` -- rather than
//!   creating a dynamic public property beside a still-default private one. This is deliberately
//!   the FIRST thing this file's tests assert (see `builtins_unserialize.rs`): PLAN.md notes this
//!   exact shape is the literal content of a Symfony cache-freshness payload, so a wrong binding
//!   here is a wrong cache decision with no crash, not a test failure anyone would notice by
//!   running the app.
//! - `r:N;` / `R:N;` back-references resolve against the values parsed so far, numbered the way
//!   php numbers them: every value except an `R:` takes the next 1-based index, an `r:` takes one
//!   too (it aliases its target), and array/object KEYS take none. `r:` needs an object target and
//!   shares its identity; `R:` copies the target (an object keeps its identity), because the
//!   interpreter cannot bind two slots to one PHP reference here. A target still being built (an
//!   object naming itself from inside its own properties, an enclosing array) is not yet
//!   resolvable and fails the parse like an out-of-range index. The registry is only kept when
//!   the payload contains an `r:`/`R:` marker at all.
//! - NOT implemented (measured absent from the real 404 payloads; each is a separate, larger
//!   ticket): `E:` enum payloads, `C:` `Serializable` payloads,
//!   `unserialize_callback_func` (the ini key still round-trips through `ini_get`/`ini_set`
//!   independently of this file), and `max_depth`/`unserialize_max_depth` (no depth guard at
//!   all -- a malicious/corrupt deeply-nested payload can blow the Rust stack, matching php's own
//!   guard being the thing missing, not present-but-wrong). Byte-exact warning OFFSETS are
//!   best-effort (the offset is wherever this scanner's cursor sat when it gave up), not a
//!   guaranteed match to php's own scanner position for every malformed shape.

use super::super::super::*;

eval_builtin! {
    contract: "unserialize",
    area: Core,
    direct: Core,
    values: Core,
}

/// Dispatches a direct eval `unserialize()` call, preserving PHP source argument order.
pub(in crate::interpreter) fn eval_builtin_unserialize(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_throw_argument_count_error(
            "unserialize() expects at least 1 argument, 0 given",
            context,
            values,
        ),
        [data] => {
            let data = eval_expr(data, context, scope, values)?;
            eval_unserialize_call(data, None, context, values)
        }
        [data, options] => {
            let data = eval_expr(data, context, scope, values)?;
            let options = eval_expr(options, context, scope, values)?;
            eval_unserialize_call(data, Some(options), context, values)
        }
        _ => eval_throw_argument_count_error(
            &format!("unserialize() expects at most 2 arguments, {} given", args.len()),
            context,
            values,
        ),
    }
}

/// Dispatches an evaluated-argument `unserialize()` call (named args, spread, callable dispatch).
pub(in crate::interpreter) fn eval_unserialize_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => eval_throw_argument_count_error(
            "unserialize() expects at least 1 argument, 0 given",
            context,
            values,
        ),
        [data] => eval_unserialize_call(*data, None, context, values),
        [data, options] => eval_unserialize_call(*data, Some(*options), context, values),
        _ => eval_throw_argument_count_error(
            &format!(
                "unserialize() expects at most 2 arguments, {} given",
                evaluated_args.len()
            ),
            context,
            values,
        ),
    }
}

/// Which class names `unserialize()` may hydrate as real objects.
enum EvalUnserializeAllowedClasses {
    All,
    None,
    List(Vec<String>),
}

impl EvalUnserializeAllowedClasses {
    fn allows(&self, class_name: &str) -> bool {
        match self {
            Self::All => true,
            Self::None => false,
            Self::List(names) => names.iter().any(|name| name.eq_ignore_ascii_case(class_name)),
        }
    }
}

/// Validates arguments, reads the `allowed_classes` option, and drives the parse.
fn eval_unserialize_call(
    data: RuntimeCellHandle,
    options: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(data)? != EVAL_TAG_STRING {
        let given = eval_given_type_spelling(data, values)?;
        return eval_throw_type_error(
            &format!("unserialize(): Argument #1 ($data) must be of type string, {given} given"),
            context,
            values,
        );
    }
    let allowed_classes = match options {
        None => EvalUnserializeAllowedClasses::All,
        Some(options) => {
            if !matches!(values.type_tag(options)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
                let given = eval_given_type_spelling(options, values)?;
                return eval_throw_type_error(
                    &format!(
                        "unserialize(): Argument #2 ($options) must be of type array, {given} given"
                    ),
                    context,
                    values,
                );
            }
            eval_unserialize_allowed_classes_option(options, context, values)?
        }
    };

    let bytes = values.string_bytes(data)?;
    if bytes.is_empty() {
        // Measured: `unserialize('')` is `false` with NO warning -- Symfony reads possibly-empty
        // cache files, and this silence is load-bearing.
        return values.bool_value(false);
    }

    let mut cursor = EvalUnserializeCursor { data: &bytes, pos: 0 };
    let mut refs = EvalUnserializeRefs {
        enabled: bytes.windows(2).any(|pair| pair == b"r:" || pair == b"R:"),
        slots: Vec::new(),
    };
    let parsed = eval_unserialize_parse_value(&mut cursor, &allowed_classes, &mut refs, context, values);
    refs.release_all(values);
    match parsed {
        Ok(value) => {
            if cursor.pos < bytes.len() {
                values.warning(&format!(
                    "unserialize(): Extra data starting at offset {} of {} bytes",
                    cursor.pos,
                    bytes.len()
                ))?;
            }
            Ok(value)
        }
        Err(EvalUnserializeFailure::AlreadyWarned) => values.bool_value(false),
        Err(EvalUnserializeFailure::Generic) => {
            values.warning(&format!(
                "unserialize(): Error at offset {} of {} bytes",
                cursor.pos,
                bytes.len()
            ))?;
            values.bool_value(false)
        }
    }
}

/// Reads `$options['allowed_classes']`, defaulting to `All` when the key is absent.
fn eval_unserialize_allowed_classes_option(
    options: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalUnserializeAllowedClasses, EvalStatus> {
    let key = values.string("allowed_classes")?;
    let exists = values.array_key_exists(key, options)?;
    let present = values.truthy(exists)?;
    if !present {
        return Ok(EvalUnserializeAllowedClasses::All);
    }
    let key = values.string("allowed_classes")?;
    let value = values.array_get(options, key)?;
    match values.type_tag(value)? {
        EVAL_TAG_BOOL => Ok(if values.truthy(value)? {
            EvalUnserializeAllowedClasses::All
        } else {
            EvalUnserializeAllowedClasses::None
        }),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let len = values.array_len(value)?;
            let mut names = Vec::with_capacity(len);
            for position in 0..len {
                let key = values.array_iter_key(value, position)?;
                let element = values.array_get(value, key)?;
                let bytes = values.string_bytes(element)?;
                names.push(String::from_utf8_lossy(&bytes).into_owned());
            }
            Ok(EvalUnserializeAllowedClasses::List(names))
        }
        _ => {
            let given = eval_given_type_spelling(value, values)?;
            eval_throw_type_error(
                &format!(
                    "unserialize(): Option \"allowed_classes\" must be of type array|bool, {given} given"
                ),
                context,
                values,
            )
        }
    }
}

/// A parse failure: either a specific warning was already emitted (a warning shape more precise
/// than the generic wrapper), or the caller must emit php's generic `Error at offset` wrapper.
enum EvalUnserializeFailure {
    AlreadyWarned,
    Generic,
}

/// The values parsed so far, indexed the way php numbers them for `r:`/`R:` back-references.
///
/// A slot is reserved when a value starts and filled when it completes, so a container's own
/// index stays reserved (and unresolvable) while its children parse. Entries are owned retains
/// released by `release_all` once the call ends. A disabled registry (no marker in the payload,
/// or an array/object key) reserves and stores nothing.
struct EvalUnserializeRefs {
    enabled: bool,
    slots: Vec<Option<RuntimeCellHandle>>,
}

impl EvalUnserializeRefs {
    /// A registry that records nothing, for payloads without back-references and for keys.
    fn off() -> Self {
        EvalUnserializeRefs { enabled: false, slots: Vec::new() }
    }

    /// Reserves the next index for a value that is starting to parse.
    fn reserve(&mut self) -> Option<usize> {
        if !self.enabled {
            return None;
        }
        self.slots.push(None);
        Some(self.slots.len() - 1)
    }

    /// Publishes a completed value under its reserved index.
    fn publish(
        &mut self,
        slot: Option<usize>,
        value: RuntimeCellHandle,
        values: &mut impl RuntimeValueOps,
    ) -> Result<(), EvalStatus> {
        if let Some(slot) = slot {
            self.slots[slot] = Some(values.retain(value)?);
        }
        Ok(())
    }

    /// Releases every retained entry.
    fn release_all(&mut self, values: &mut impl RuntimeValueOps) {
        for entry in self.slots.drain(..).flatten() {
            let _ = values.release(entry);
        }
    }
}

/// A byte-position cursor over the serialized payload.
struct EvalUnserializeCursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> EvalUnserializeCursor<'a> {
    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn take_byte(&mut self, byte: u8) -> Result<(), EvalUnserializeFailure> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(EvalUnserializeFailure::Generic)
        }
    }

    /// Reads bytes up to (not including) the next occurrence of `delim`, advancing past it.
    fn take_until(&mut self, delim: u8) -> Result<&'a [u8], EvalUnserializeFailure> {
        let start = self.pos;
        while self.peek() != Some(delim) {
            if self.peek().is_none() {
                return Err(EvalUnserializeFailure::Generic);
            }
            self.pos += 1;
        }
        let slice = &self.data[start..self.pos];
        self.pos += 1;
        Ok(slice)
    }

    /// Reads exactly `len` bytes without any delimiter.
    fn take_exact(&mut self, len: usize) -> Result<&'a [u8], EvalUnserializeFailure> {
        if self.pos + len > self.data.len() {
            return Err(EvalUnserializeFailure::Generic);
        }
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }
}

/// Parses one serialized value at the cursor's current position, numbering it for `r:`/`R:`.
fn eval_unserialize_parse_value(
    cursor: &mut EvalUnserializeCursor<'_>,
    allowed_classes: &EvalUnserializeAllowedClasses,
    refs: &mut EvalUnserializeRefs,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalUnserializeFailure> {
    if matches!(cursor.peek(), Some(b'r' | b'R')) {
        return eval_unserialize_parse_back_reference(cursor, refs, values);
    }
    let slot = refs.reserve();
    let value = eval_unserialize_parse_plain_value(cursor, allowed_classes, refs, context, values)?;
    refs.publish(slot, value, values).map_err(|_| EvalUnserializeFailure::Generic)?;
    Ok(value)
}

/// Resolves `r:<n>;` (object identity) or `R:<n>;` (php reference) against the values parsed so
/// far.
///
/// `r:` takes an index of its own that aliases its target; `R:` takes none. Only an object can be
/// an `r:` target. `R:` hands back a copy of a non-object target (php would bind both slots to
/// one reference) and the same object for an object target.
fn eval_unserialize_parse_back_reference(
    cursor: &mut EvalUnserializeCursor<'_>,
    refs: &mut EvalUnserializeRefs,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalUnserializeFailure> {
    let marker = cursor.peek().ok_or(EvalUnserializeFailure::Generic)?;
    cursor.pos += 1;
    cursor.take_byte(b':')?;
    let digits = cursor.take_until(b';')?;
    let index: usize = std::str::from_utf8(digits)
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or(EvalUnserializeFailure::Generic)?;
    let target = index
        .checked_sub(1)
        .and_then(|slot| refs.slots.get(slot).copied().flatten())
        .ok_or(EvalUnserializeFailure::Generic)?;
    let is_object = values.type_tag(target).map_err(|_| EvalUnserializeFailure::Generic)? == EVAL_TAG_OBJECT;
    if marker == b'r' && !is_object {
        return Err(EvalUnserializeFailure::Generic);
    }
    let result = if is_object { values.retain(target) } else { values.copy_value(target) }
        .map_err(|_| EvalUnserializeFailure::Generic)?;
    if marker == b'r' {
        refs.slots.push(Some(values.retain(result).map_err(|_| EvalUnserializeFailure::Generic)?));
    }
    Ok(result)
}

/// Parses one non-back-reference serialized value at the cursor's current position.
fn eval_unserialize_parse_plain_value(
    cursor: &mut EvalUnserializeCursor<'_>,
    allowed_classes: &EvalUnserializeAllowedClasses,
    refs: &mut EvalUnserializeRefs,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalUnserializeFailure> {
    match cursor.peek() {
        Some(b'N') => {
            cursor.pos += 1;
            cursor.take_byte(b';')?;
            values.null().map_err(|_| EvalUnserializeFailure::Generic)
        }
        Some(b'b') => {
            cursor.pos += 1;
            cursor.take_byte(b':')?;
            let digit = cursor.take_until(b';')?;
            let flag = match digit {
                b"1" => true,
                b"0" => false,
                _ => return Err(EvalUnserializeFailure::Generic),
            };
            values.bool_value(flag).map_err(|_| EvalUnserializeFailure::Generic)
        }
        Some(b'i') => {
            cursor.pos += 1;
            cursor.take_byte(b':')?;
            let digits = cursor.take_until(b';')?;
            let text = std::str::from_utf8(digits).map_err(|_| EvalUnserializeFailure::Generic)?;
            match text.parse::<i64>() {
                Ok(n) => values.int(n).map_err(|_| EvalUnserializeFailure::Generic),
                Err(_) => {
                    if text.is_empty() || !text.trim_start_matches('-').bytes().all(|b| b.is_ascii_digit())
                    {
                        return Err(EvalUnserializeFailure::Generic);
                    }
                    // Digit string is well-formed but out of i64 range: php clamps to the
                    // PHP_INT_MAX/PHP_INT_MIN boundary with a recoverable warning, not a failure.
                    values
                        .warning("unserialize(): Numerical result out of range")
                        .map_err(|_| EvalUnserializeFailure::Generic)?;
                    let clamped = if text.starts_with('-') { i64::MIN } else { i64::MAX };
                    values.int(clamped).map_err(|_| EvalUnserializeFailure::Generic)
                }
            }
        }
        Some(b'd') => {
            cursor.pos += 1;
            cursor.take_byte(b':')?;
            let digits = cursor.take_until(b';')?;
            let text = std::str::from_utf8(digits).map_err(|_| EvalUnserializeFailure::Generic)?;
            let parsed = match text {
                "NAN" => f64::NAN,
                "INF" => f64::INFINITY,
                "-INF" => f64::NEG_INFINITY,
                other => other.parse::<f64>().map_err(|_| EvalUnserializeFailure::Generic)?,
            };
            values.float(parsed).map_err(|_| EvalUnserializeFailure::Generic)
        }
        Some(b's') => {
            let bytes = eval_unserialize_parse_raw_string(cursor)?;
            values
                .string_bytes_value(&bytes)
                .map_err(|_| EvalUnserializeFailure::Generic)
        }
        Some(b'a') => eval_unserialize_parse_array(cursor, allowed_classes, refs, context, values),
        Some(b'O') => eval_unserialize_parse_object(cursor, allowed_classes, refs, context, values),
        _ => Err(EvalUnserializeFailure::Generic),
    }
}

/// Parses the raw byte payload of one `s:<len>:"<bytes>";` string, WITHOUT constructing a cell --
/// shared by value parsing and by array/object key parsing, which both need the same grammar.
fn eval_unserialize_parse_raw_string(
    cursor: &mut EvalUnserializeCursor<'_>,
) -> Result<Vec<u8>, EvalUnserializeFailure> {
    cursor.take_byte(b's')?;
    cursor.take_byte(b':')?;
    let len_digits = cursor.take_until(b':')?;
    let len: usize = std::str::from_utf8(len_digits)
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or(EvalUnserializeFailure::Generic)?;
    cursor.take_byte(b'"')?;
    let bytes = cursor.take_exact(len)?.to_vec();
    cursor.take_byte(b'"')?;
    cursor.take_byte(b';')?;
    Ok(bytes)
}

/// Parses an `i:<int>;` object-payload key.
fn eval_unserialize_parse_int_key(
    cursor: &mut EvalUnserializeCursor<'_>,
) -> Result<i64, EvalUnserializeFailure> {
    cursor.take_byte(b'i')?;
    cursor.take_byte(b':')?;
    // `take_until` consumes the `;` too.
    let digits = cursor.take_until(b';')?;
    std::str::from_utf8(digits)
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or(EvalUnserializeFailure::Generic)
}

/// Builds the array a `__unserialize()` hook receives from a payload, turning the decimal
/// spelling of an integer key back into the integer key php's array has.
fn eval_unserialize_payload_array(
    payload: Vec<(String, RuntimeCellHandle)>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut array = values.assoc_new(payload.len())?;
    for (name, value) in payload {
        let key = match name.parse::<i64>() {
            Ok(index) if index.to_string() == name => values.int(index)?,
            _ => values.string(&name)?,
        };
        array = values.array_set(array, key, value)?;
    }
    Ok(array)
}

/// Parses `a:<count>:{<key><value>...}` into a fresh eval array.
fn eval_unserialize_parse_array(
    cursor: &mut EvalUnserializeCursor<'_>,
    allowed_classes: &EvalUnserializeAllowedClasses,
    refs: &mut EvalUnserializeRefs,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalUnserializeFailure> {
    cursor.take_byte(b'a')?;
    cursor.take_byte(b':')?;
    let count_digits = cursor.take_until(b':')?;
    let count: usize = std::str::from_utf8(count_digits)
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or(EvalUnserializeFailure::Generic)?;
    cursor.take_byte(b'{')?;
    let mut result = values.assoc_new(count).map_err(|_| EvalUnserializeFailure::Generic)?;
    for _ in 0..count {
        if cursor.peek().is_none() {
            values
                .warning("unserialize(): Unexpected end of serialized data")
                .map_err(|_| EvalUnserializeFailure::Generic)?;
            return Err(EvalUnserializeFailure::AlreadyWarned);
        }
        // Keys are written straight onto the wire by php and take no back-reference index.
        let key = eval_unserialize_parse_value(
            cursor,
            allowed_classes,
            &mut EvalUnserializeRefs::off(),
            context,
            values,
        )?;
        if cursor.peek().is_none() {
            values
                .warning("unserialize(): Unexpected end of serialized data")
                .map_err(|_| EvalUnserializeFailure::Generic)?;
            return Err(EvalUnserializeFailure::AlreadyWarned);
        }
        let element = eval_unserialize_parse_value(cursor, allowed_classes, refs, context, values)?;
        result = values
            .array_set(result, key, element)
            .map_err(|_| EvalUnserializeFailure::Generic)?;
    }
    cursor.take_byte(b'}')?;
    eval_unserialize_retag_as_list_if_packed(result, values)
}

/// Re-tags a fold result as a packed list when its key sequence is exactly `0..len`, matching
/// `array_replace`'s own rule: the tag must follow the keys, not the builder's internal choice.
fn eval_unserialize_retag_as_list_if_packed(
    assoc: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalUnserializeFailure> {
    let len = values.array_len(assoc).map_err(|_| EvalUnserializeFailure::Generic)?;
    for position in 0..len {
        let key = values
            .array_iter_key(assoc, position)
            .map_err(|_| EvalUnserializeFailure::Generic)?;
        let is_sequential_int = values.type_tag(key).map_err(|_| EvalUnserializeFailure::Generic)?
            == EVAL_TAG_INT
            && values.raw_value_word(key).map_err(|_| EvalUnserializeFailure::Generic)? as i64
                == position as i64;
        if !is_sequential_int {
            return Ok(assoc);
        }
    }
    let mut list = values.array_new(len).map_err(|_| EvalUnserializeFailure::Generic)?;
    for position in 0..len {
        let key = values
            .array_iter_key(assoc, position)
            .map_err(|_| EvalUnserializeFailure::Generic)?;
        let value = values
            .array_get(assoc, key)
            .map_err(|_| EvalUnserializeFailure::Generic)?;
        list = values
            .array_set(list, key, value)
            .map_err(|_| EvalUnserializeFailure::Generic)?;
    }
    values.release(assoc).map_err(|_| EvalUnserializeFailure::Generic)?;
    Ok(list)
}

/// Parses `O:<name-len>:"<name>":<count>:{<key><value>...}` and hydrates an object.
fn eval_unserialize_parse_object(
    cursor: &mut EvalUnserializeCursor<'_>,
    allowed_classes: &EvalUnserializeAllowedClasses,
    refs: &mut EvalUnserializeRefs,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalUnserializeFailure> {
    cursor.take_byte(b'O')?;
    cursor.take_byte(b':')?;
    let name_len_digits = cursor.take_until(b':')?;
    let name_len: usize = std::str::from_utf8(name_len_digits)
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or(EvalUnserializeFailure::Generic)?;
    cursor.take_byte(b'"')?;
    let name_bytes = cursor.take_exact(name_len)?.to_vec();
    cursor.take_byte(b'"')?;
    cursor.take_byte(b':')?;
    let class_name =
        String::from_utf8(name_bytes).map_err(|_| EvalUnserializeFailure::Generic)?;
    let count_digits = cursor.take_until(b':')?;
    let count: usize = std::str::from_utf8(count_digits)
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or(EvalUnserializeFailure::Generic)?;
    cursor.take_byte(b'{')?;

    let mut payload: Vec<(String, RuntimeCellHandle)> = Vec::with_capacity(count);
    for _ in 0..count {
        if cursor.peek().is_none() {
            values
                .warning("unserialize(): Unexpected end of serialized data")
                .map_err(|_| EvalUnserializeFailure::Generic)?;
            return Err(EvalUnserializeFailure::AlreadyWarned);
        }
        // A `__serialize()` payload keeps its array's own keys, so an integer key (`i:0;`) is as
        // valid here as a property name; it travels as its decimal spelling.
        let key = if cursor.peek() == Some(b'i') {
            eval_unserialize_parse_int_key(cursor)?.to_string()
        } else {
            let key_bytes = eval_unserialize_parse_raw_string(cursor)?;
            String::from_utf8_lossy(&key_bytes).into_owned()
        };
        if cursor.peek().is_none() {
            values
                .warning("unserialize(): Unexpected end of serialized data")
                .map_err(|_| EvalUnserializeFailure::Generic)?;
            return Err(EvalUnserializeFailure::AlreadyWarned);
        }
        let value = eval_unserialize_parse_value(cursor, allowed_classes, refs, context, values)?;
        payload.push((key, value));
    }
    cursor.take_byte(b'}')?;

    eval_unserialize_hydrate_object(&class_name, payload, allowed_classes, context, values)
        .map_err(|_| EvalUnserializeFailure::Generic)
}

/// Hydrates one EVAL-DECLARED class from a property hash the COMPILED decoder already parsed.
///
/// The native `unserialize` decoder builds objects from the AOT class table, and an
/// eval-declared class has no entry there: `__rt_new_by_name` answers 0 and the decoder falls
/// through to its `__PHP_Incomplete_Class` stand-in (class id -2). Symfony reaches that on every
/// request, because its compiled DI container lives in `var/cache` behind a computed `require`
/// and is therefore interpreted — so every class the container names is eval-declared.
///
/// OWNERSHIP: `payload` values arrive OWNED (`array_get` hands back a fresh cell) and
/// `eval_unserialize_hydrate_object` moves them into the object, which is the same contract the
/// interpreter's own parser uses. The decoder's hash keeps its own references and is freed with
/// the stand-in object it belongs to, so nothing is shared between the two.
///
/// The allow-list has already been applied by the decoder before it got this far, so this asks
/// only whether the class is one the INTERPRETER declared.
#[cfg(not(test))]
pub(crate) fn eval_unserialize_declared_object_from_hash(
    class_name: &str,
    payload_hash: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if context.class(class_name).is_none() {
        return Ok(None);
    }
    let mut payload = Vec::new();
    if !payload_hash.is_null() {
        let len = values.array_len(payload_hash)?;
        for position in 0..len {
            let key = values.array_iter_key(payload_hash, position)?;
            let name = values.string_bytes(key)?;
            let value = values.array_get(payload_hash, key);
            values.release(key)?;
            let value = value?;
            match String::from_utf8(name) {
                Ok(name) => payload.push((name, value)),
                Err(_) => values.release(value)?,
            }
        }
    }
    eval_unserialize_hydrate_object(
        class_name,
        payload,
        &EvalUnserializeAllowedClasses::All,
        context,
        values,
    )
    .map(Some)
}

/// Builds the actual object once a class name and its property payload have been parsed --
/// separated from the byte-cursor parser above so its error type is the ordinary `EvalStatus`
/// every other object-construction helper in this crate already uses.
fn eval_unserialize_hydrate_object(
    class_name: &str,
    payload: Vec<(String, RuntimeCellHandle)>,
    allowed_classes: &EvalUnserializeAllowedClasses,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // PHP resolves an unknown class here through `unserialize_callback_func`, whose whole point
    // is to give the autoloader a chance before the value degrades to
    // `__PHP_Incomplete_Class` — Symfony points it at its own autoloader, so its `.meta` files
    // deserialize into real `FileResource`/`GlobResource` objects. Without that chance the
    // stand-in object's class cannot be resolved at all, and the first typed parameter it
    // reaches refuses to bind: traced as `arg_tags=["6:<id>:<unresolved>"]` against a
    // `ResourceInterface` parameter. Only classes the allow-list already permits are offered to
    // the autoloader, so `unserialize($s, ['allowed_classes' => false])` still loads nothing.
    let mut class_is_known =
        context.class(class_name).is_some() || values.class_exists(class_name)?;
    if !class_is_known && allowed_classes.allows(class_name) {
        class_is_known = crate::interpreter::eval_spl_autoload_class_bridge(
            class_name, context, values,
        )
        .unwrap_or(false);
    }
    if !class_is_known || !allowed_classes.allows(class_name) {
        return eval_unserialize_incomplete_class(class_name, payload, values);
    }
    if let Some(class) = context.class(class_name).cloned() {
        let mut scope = ElephcEvalScope::new();
        let object = eval_dynamic_class_allocate_object(&class, context, &mut scope, values)?;
        let identity = values.object_identity(object)?;
        if let Some((declaring_class, method)) = context.class_method(class.name(), "__unserialize") {
            // php hands a `__unserialize()` class the payload and writes no property itself:
            // the keys are whatever `__serialize()` chose (`0`, `1`, ...), not property names,
            // and writing them first left `0`/`1`/`2` dynamic properties on the object.
            let payload_array = eval_unserialize_payload_array(payload, values)?;
            let result = eval_dynamic_method_with_values(
                &declaring_class,
                class.name(),
                &method,
                object,
                vec![EvaluatedCallArg {
                    name: None,
                    value: payload_array,
                    ref_target: None,
                }],
                context,
                values,
            )?;
            values.release(result)?;
            return Ok(object);
        }
        for &(ref name, value) in &payload {
            let declared_name = eval_unserialize_declared_property_name(name);
            if let Some((declaring_class, property)) =
                context.class_property(class.name(), declared_name)
            {
                let storage_name = eval_instance_property_storage_name(&declaring_class, &property);
                if property.visibility() == EvalVisibility::Public {
                    let _ = values.property_set(object, &storage_name, value);
                }
                eval_store_dynamic_property_value(identity, &storage_name, value, context, values)?;
                context.mark_dynamic_property_initialized(identity, &storage_name);
            } else {
                let _ = values.property_set(object, name, value);
                eval_store_dynamic_property_value(identity, name, value, context, values)?;
                context.mark_dynamic_property_initialized(identity, name);
            }
        }
        return Ok(object);
    }
    // A native/AOT class with no eval-declared metadata (e.g. a genuine built-in): there is no
    // declared-property table to consult, so every payload property lands as a plain dynamic
    // one. Known limitation: a native class with real private state would mis-bind here exactly
    // as an undeclared eval property would -- out of scope, no such class is on the 404 path.
    // A compiled class with `__unserialize()` does not need that table at all: php hands the
    // hook the payload, which is how Symfony's session reads back the compiled `App\Entity\User`
    // inside a security token.
    let object = values.new_object(class_name)?;
    // A compiled class that restores itself through `__unserialize()` gets the payload handed to
    // that hook, as php does, instead of having its keys written as properties from outside its
    // scope: those writes land nowhere for a protected property, and Laravel's
    // `UnsignedSerializableClosure` came back with a null `$serializable` -- every cached closure
    // route then died with "Call to a member function getClosure() on null".
    if eval_unserialize_native_class_has_hook(class_name, context, values)? {
        let mut payload_array = values.assoc_new(payload.len())?;
        for (name, value) in payload {
            let key = values.string(&name)?;
            payload_array = values.array_set(payload_array, key, value)?;
        }
        let result = crate::interpreter::eval_method_call_result(
            object,
            "__unserialize",
            vec![payload_array],
            context,
            values,
        );
        values.release(payload_array)?;
        values.release(result?)?;
        return Ok(object);
    }
    for (name, value) in payload {
        // A payload written by PHP mangles a private/protected property as
        // `"\0Class\0name"` / `"\0*\0name"`. The interpreter's ordinary `property_set` applies
        // the CALLING scope's visibility, so a protected key became a dynamic property and the
        // typed slot stayed uninitialized: Symfony's `ContainerBuilder::$parameterBag` came back
        // uninitialized once AOT code had touched the class. Hydration takes the declared-slot
        // path instead, the same matcher the compiled decoder uses.
        values.hydrate_declared_property(object, &name, value)?;
    }
    Ok(object)
}

/// Whether a class the interpreter did not declare has an `__unserialize()` hook.
fn eval_unserialize_native_class_has_hook(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_cell = values.string(class_name)?;
    let method_cell = values.string("__unserialize")?;
    let exists = crate::interpreter::eval_member_exists_result(
        "method_exists",
        &[class_cell, method_cell],
        context,
        values,
    );
    values.release(class_cell)?;
    values.release(method_cell)?;
    let exists = exists?;
    let answer = values.truthy(exists)?;
    values.release(exists)?;
    Ok(answer)
}

/// Returns the DECLARED property name a serialized payload key stands for.
///
/// `serialize()` mangles a private property as `"\0ClassName\0name"` and a protected one as
/// `"\0*\0name"`, and PHP resolves both back onto the declared property. A class with a
/// `__serialize()` picks its own keys instead, which are normally the declared names already —
/// so the rule is simply "whatever follows the last NUL", which leaves an ordinary public key
/// untouched. Without this, a payload written by real PHP (every Symfony `.meta` file) set a
/// dynamic property beside a still-uninitialized typed one, and the first read of it raised
/// `must not be accessed before initialization`.
fn eval_unserialize_declared_property_name(name: &str) -> &str {
    match name.rfind('\0') {
        Some(separator) => &name[separator + 1..],
        None => name,
    }
}

/// Builds a `__PHP_Incomplete_Class` object for a disallowed or unresolvable class name, with
/// `__PHP_Incomplete_Class_Name` set FIRST and every payload property carried through, unmangled,
/// in payload order -- no declared defaults, matching php's own (measured) shape exactly.
fn eval_unserialize_incomplete_class(
    class_name: &str,
    payload: Vec<(String, RuntimeCellHandle)>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // MEASURED via the compiled probe (scratchpad/r200/gate.php lotc_unser): the production
    // runtime has no native `__PHP_Incomplete_Class` registered at all (`values.new_object`
    // fails outright, where the FakeOps unit fixture is permissive about any class name and
    // never caught this). Falling back to `stdClass` avoids turning a disallowed/unknown class
    // name into a hard interpreter failure -- the object still carries
    // `__PHP_Incomplete_Class_Name` and every payload property -- but `get_class()` on it then
    // answers `"stdClass"`, not `"__PHP_Incomplete_Class"`, and none of php's own
    // "tried to access a property on an incomplete object" warning machinery applies. Fixing
    // this fully means registering a real native `__PHP_Incomplete_Class` class, which is a
    // separate, larger ticket than this wire-up.
    let object = values
        .new_object("__PHP_Incomplete_Class")
        .or_else(|_| values.new_object("stdClass"))?;
    let marker_name = values.string(class_name)?;
    values.property_set(object, "__PHP_Incomplete_Class_Name", marker_name)?;
    for (name, value) in payload {
        values.property_set(object, &name, value)?;
    }
    Ok(object)
}
