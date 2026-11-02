//! Purpose:
//! Eval registry entry and implementation for `parse_str`: parses PHP's `&`-separated query
//! grammar into the by-reference `$result` out-parameter.
//!
//! Called from:
//! - `crate::interpreter::expressions::calls::eval_call()`, which dispatches `parse_str` from
//!   its own hard-coded ladder (`calls.rs:138`) BEFORE the registry gate, because `$result` is
//!   a mandatory by-reference out-parameter and `EvalValuesHook` structurally cannot write one.
//!   The registry entry below exists so `function_exists("parse_str")`, `is_callable()`, and the
//!   `EvalDirectHook::ParseStr` arm resolve the name -- the direct hook itself is unreachable in
//!   practice because the ladder line fires first, and simply re-packages its positional
//!   arguments as `EvalCallArg`s and delegates to the same call path.
//!
//! Key details, each backed by a `php -n` 8.5.6 measurement (`scratchpad/verify_ps*.php`):
//! - `&` is the only field separator modeled; `;` is NOT one (php's default
//!   `arg_separator.input` is `"&"`, and the interpreter has no ini storage to read a
//!   configured one -- a documented, deliberate gap).
//! - The WHOLE parse stops at the first NUL byte in the source, not just the current field.
//! - Name and value are `+`/`%XX`-decoded BEFORE the bracket grammar runs, once.
//! - Leading `0x20` bytes are stripped from the front of the (decoded) name; no other whitespace
//!   is touched.
//! - In the root (the part of the name before the first `[`), `' '` and `'.'` become `'_'`;
//!   inside a bracket segment neither is touched.
//! - An empty root name drops the whole field.
//! - Each `[...]` up to the first `]` is one path segment: an EMPTY segment appends, a segment
//!   that is exactly one whitespace byte (space or tab) ALSO appends, anything else is a literal
//!   key, and content after the last closed `]` is silently ignored.
//! - An unterminated `[` has two different outcomes depending on WHICH segment fails: failure in
//!   the FIRST segment flat-mangles the WHOLE name (every `' '`, `'.'`, `'['` becomes `'_'`,
//!   producing one scalar key); failure in a LATER segment drops the unparsed tail and keeps the
//!   path built so far.
//! - Array keys follow PHP's normal canonicalization (a canonical decimal integer string becomes
//!   an int key); `[]` appends at `max(existing int keys) + 1`, INCLUDING negative keys, starting
//!   at `0` with none, and is silently dropped (not wrapped or overwritten) once the existing max
//!   is `PHP_INT_MAX`.
//! - `max_input_vars` (default 1000) caps the number of top-level fields; `max_input_nesting_level`
//!   (default 64) caps bracket depth per field -- both hard-coded to php's own defaults, since
//!   there is nowhere in the interpreter to read a configured ini value from either.

use super::super::super::*;

eval_builtin! {
    contract: "parse_str",
    area: String,
    direct: ParseStr,
    values: none,
}

/// Maximum accepted top-level fields, matching php's default `max_input_vars`.
const MAX_INPUT_VARS: usize = 1000;

/// Maximum accepted bracket segments per field, matching php's default `max_input_nesting_level`.
const MAX_BRACKET_SEGMENTS: usize = 64;

/// Evaluates the `EvalDirectHook::ParseStr` registry entry point.
///
/// Unreachable in practice: `calls.rs:138` intercepts every `parse_str` call before the registry
/// gate is even consulted, because the by-reference out-parameter needs a real reference target,
/// which only that earlier, unevaluated-argument path can capture. This exists so the registry
/// entry is internally consistent (`EvalDirectHook` requires at least one live hook) and so a
/// caller that somehow bypasses the ladder still gets php's own semantics rather than a refusal.
pub(in crate::interpreter) fn eval_builtin_parse_str_direct(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let call_args: Vec<EvalCallArg> = args
        .iter()
        .cloned()
        .map(EvalCallArg::positional)
        .collect();
    eval_builtin_parse_str_call(&call_args, context, scope, values)
}

/// Evaluates PHP 8's mandatory-output `parse_str(string $string, array &$result)` form.
pub(in crate::interpreter) fn eval_builtin_parse_str_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated, context, _, values| {
        if evaluated.len() != 2 {
            return eval_throw_argument_count_error(
                &format!(
                    "parse_str() expects exactly 2 arguments, {} given",
                    evaluated.len()
                ),
                context,
                values,
            );
        }
        let (bound, _) = bind_evaluated_ref_builtin_args(&["string", "result"], &evaluated, false)?;
        let source = required_evaluated_ref_arg(&bound, 0)?.value;
        let output = required_evaluated_ref_arg(&bound, 1)?;
        let target = output.ref_target.clone().ok_or(EvalStatus::RuntimeFatal)?;
        let source_bytes = eval_require_string_arg(source, "parse_str", 1, "string", context, values)?;
        let result = eval_parse_str_result(&source_bytes, values)?;
        eval_write_direct_ref_target(
            &target,
            result,
            context,
            values,
            Some(ScopeCellOwnership::Owned),
        )?;
        values.null()
    })
}

/// Parses the default PHP query-string grammar into a fresh associative result array.
fn eval_parse_str_result(
    source_bytes: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // NUL terminates the WHOLE parse, not just the current field.
    let source = match source_bytes.iter().position(|byte| *byte == 0) {
        Some(index) => &source_bytes[..index],
        None => source_bytes,
    };
    let mut root = QueryNode::Map(Vec::new());
    let mut accepted = 0usize;
    for field in source.split(|byte| *byte == b'&') {
        if field.is_empty() {
            continue;
        }
        let (raw_name, raw_value) = match field.iter().position(|byte| *byte == b'=') {
            Some(index) => (&field[..index], &field[index + 1..]),
            None => (field, &field[field.len()..]),
        };
        let name = decode_form_component(raw_name);
        let name = strip_leading_spaces(&name);
        if name.is_empty() {
            continue;
        }
        if accepted >= MAX_INPUT_VARS {
            // php warns `Input variables exceeded %d. To increase the limit change
            // max_input_vars in php.ini.` here and stops entirely; the diagnostic itself is not
            // wired up in this lot (see the session report), but the stop-and-keep-what-you-have
            // behavior is.
            break;
        }
        let Some(path) = query_key_path(&name) else {
            // Empty root name: the whole field is dropped.
            continue;
        };
        if path.len() > MAX_BRACKET_SEGMENTS + 1 {
            // php silently drops one variable past `max_input_nesting_level`, no warning, and
            // every OTHER field in the same query still lands.
            continue;
        }
        let value = decode_form_component(raw_value);
        root.insert(&path, value);
        accepted += 1;
    }
    root.materialize(values)
}

/// One completed path segment: a literal key, or the `[]` append marker.
#[derive(Clone, PartialEq, Eq)]
enum QueryKey {
    Name(Vec<u8>),
    Append,
}

/// A partially built result tree: either a leaf scalar or a map of path segments to children.
enum QueryNode {
    Scalar(Vec<u8>),
    Map(Vec<(Vec<u8>, QueryNode)>),
}

impl QueryNode {
    /// Writes `value` at `path`, replacing whatever was there before (scalar <-> array, in
    /// either direction, per php's own re-use rule) and silently dropping the write if an `[]`
    /// append has nowhere left to go (the existing integer keys already cover `PHP_INT_MAX`).
    fn insert(&mut self, path: &[QueryKey], value: Vec<u8>) {
        let Some((head, tail)) = path.split_first() else {
            *self = Self::Scalar(value);
            return;
        };
        let Self::Map(entries) = self else {
            *self = Self::Map(Vec::new());
            self.insert(path, value);
            return;
        };
        let key = match head {
            QueryKey::Name(key) => key.clone(),
            QueryKey::Append => {
                let Some(key) = next_append_key(entries) else {
                    return;
                };
                key
            }
        };
        if let Some((_, child)) = entries.iter_mut().find(|(existing, _)| *existing == key) {
            child.insert(tail, value);
        } else {
            let mut child = Self::Map(Vec::new());
            child.insert(tail, value);
            entries.push((key, child));
        }
    }

    /// Builds the final PHP array (or scalar) for this node, delegating php's own array-key
    /// canonicalization (a canonical decimal integer string becomes an int key) to
    /// `RuntimeValueOps::array_set()`, the same path every ordinary PHP array write uses.
    fn materialize(
        &self,
        values: &mut impl RuntimeValueOps,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self {
            Self::Scalar(value) => values.string_bytes_value(value),
            Self::Map(entries) => {
                let mut result = values.assoc_new(entries.len())?;
                for (key, value) in entries {
                    let key = values.string_bytes_value(key)?;
                    let value = value.materialize(values)?;
                    result = values.array_set(result, key, value)?;
                }
                Ok(result)
            }
        }
    }
}

/// Returns the next `[]` append key: `max(existing canonical int keys) + 1`, including negative
/// keys, starting at `0` with none, and `None` once the existing max is already `i64::MAX` --
/// php silently drops that append rather than wrapping or overwriting the existing element.
fn next_append_key(entries: &[(Vec<u8>, QueryNode)]) -> Option<Vec<u8>> {
    let max = entries
        .iter()
        .filter_map(|(key, _)| canonical_int_key(key))
        .max();
    match max {
        None => Some(b"0".to_vec()),
        Some(i64::MAX) => None,
        Some(value) => Some((value + 1).to_string().into_bytes()),
    }
}

/// Returns the int value of `bytes` when it is a PHP canonical decimal integer string (optional
/// leading `-`, no leading zeros except a bare `"0"`, and `"-0"` itself stays a string) -- the
/// same rule PHP applies when deciding whether an array key is an int or a string.
fn canonical_int_key(bytes: &[u8]) -> Option<i64> {
    let text = std::str::from_utf8(bytes).ok()?;
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits.as_bytes()[0] == b'0' {
        return None;
    }
    if negative && digits == "0" {
        return None;
    }
    text.parse::<i64>().ok()
}

/// Strips leading `0x20` bytes from the front of a decoded name -- and ONLY the front, and ONLY
/// that one byte value: a tab or newline at the front is left untouched.
fn strip_leading_spaces(name: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < name.len() && name[start] == b' ' {
        start += 1;
    }
    &name[start..]
}

/// Builds the path a decoded, leading-space-stripped name resolves to, or `None` when the root
/// name (the part before the first `[`) is empty -- php drops such a field entirely.
fn query_key_path(name: &[u8]) -> Option<Vec<QueryKey>> {
    let root_end = name
        .iter()
        .position(|byte| *byte == b'[')
        .unwrap_or(name.len());
    if root_end == 0 {
        return None;
    }
    if root_end == name.len() {
        let mut root = name.to_vec();
        mangle_root_bytes(&mut root);
        return Some(vec![QueryKey::Name(root)]);
    }

    let after_first_open = &name[root_end + 1..];
    if !after_first_open.contains(&b']') {
        // Failure in the FIRST segment: the whole name becomes one flat key.
        return Some(vec![QueryKey::Name(flat_mangle_whole_name(name))]);
    }

    let mut root = name[..root_end].to_vec();
    mangle_root_bytes(&mut root);
    let mut path = vec![QueryKey::Name(root)];
    let mut rest = &name[root_end..];
    while let Some(after_open) = rest.strip_prefix(b"[") {
        let Some(close) = after_open.iter().position(|byte| *byte == b']') else {
            // Failure in a LATER segment: drop the unparsed tail, keep what was built.
            break;
        };
        let segment = &after_open[..close];
        let key = if segment.is_empty()
            || (segment.len() == 1 && matches!(segment[0], b' ' | b'\t'))
        {
            QueryKey::Append
        } else {
            QueryKey::Name(segment.to_vec())
        };
        path.push(key);
        rest = &after_open[close + 1..];
    }
    Some(path)
}

/// Replaces `' '` and `'.'` with `'_'` -- the ROOT-only mangling rule.
fn mangle_root_bytes(root: &mut [u8]) {
    for byte in root.iter_mut() {
        if matches!(*byte, b' ' | b'.') {
            *byte = b'_';
        }
    }
}

/// Replaces `' '`, `'.'` and `'['` with `'_'` across the WHOLE name -- the fallback php applies
/// when the very first `[` in a name never closes.
fn flat_mangle_whole_name(name: &[u8]) -> Vec<u8> {
    name.iter()
        .map(|byte| {
            if matches!(*byte, b' ' | b'.' | b'[') {
                b'_'
            } else {
                *byte
            }
        })
        .collect()
}

fn decode_form_component(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] == b'+' {
            output.push(b' ');
            index += 1;
            continue;
        }
        if input[index] == b'%' && index + 2 < input.len() {
            if let (Some(high), Some(low)) = (hex(input[index + 1]), hex(input[index + 2])) {
                output.push(high << 4 | low);
                index += 3;
                continue;
            }
        }
        output.push(input[index]);
        index += 1;
    }
    output
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
