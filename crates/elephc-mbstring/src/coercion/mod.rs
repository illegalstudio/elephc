//! Purpose:
//! Plans PHP internal-parameter coercion before mbstring request-state dispatch.
//!
//! Called from:
//! - Shared host argument adapters and focused PHP compatibility tests.
//!
//! Key details:
//! - Decisions derive from the neutral parameter contract and caller strictness.
//! - Stringable and float formatting are explicit host actions, never callbacks inside Rust.
//! - Hosts must deliver diagnostics in order and stop if an error handler throws.

mod numeric;
pub(crate) mod entity_map;
mod messages;

use std::borrow::Cow;
use elephc_builtin_contract::{BuiltinContract, RuntimeBuiltinId, TypeSpec};
use numeric::numeric_integer;

/// A borrowed concrete PHP input, without interpreting host object or array memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Float(u64),
    String(&'a [u8]),
    Array,
    Object { class: &'a [u8], stringable: bool },
    Resource { closed: bool },
}

/// A ready scalar or a host action that must finish before the mbstring operation runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prepared<'a> {
    Null,
    Bool(bool),
    Int(i64),
    String(Cow<'a, [u8]>),
    /// Retain the original array and snapshot it after all outer parameter coercions.
    Array,
    /// Format these bits using the host's current PHP float precision, without extra warnings.
    FormatFloat(u64),
    /// Invoke the original receiver's string conversion through the host's protected call boundary.
    InvokeStringable,
    /// Validate and retain the original callback through the host's callable resolver.
    ResolveCallable,
}

/// One PHP diagnostic to emit before consuming the prepared value or invoking later callbacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic { pub level: u32, pub message: Vec<u8> }

/// Prepared ownership/action plus ordered diagnostics, or a complete binary PHP TypeError message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preparation<'a> {
    pub value: Result<Prepared<'a>, Vec<u8>>,
    pub diagnostics: Vec<Diagnostic>,
}

const NULL: u8 = 1;
const BOOL: u8 = 2;
const INT: u8 = 4;
const STRING: u8 = 8;
const ARRAY: u8 = 16;
const CALLABLE: u8 = 32;

/// Plans one registered mbstring parameter; unsupported contracts or indexes return None.
pub fn prepare(operation: RuntimeBuiltinId, index: usize, input: Input<'_>, strict: bool) -> Option<Preparation<'_>> {
    if !operation.is_mbstring() { return None; }
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id())?;
    prepare_contract(contract, index, input, strict)
}

/// Applies the same parameter planner to a shared contract with host-owned reference parameters.
pub(crate) fn prepare_contract<'a>(contract: &BuiltinContract, index: usize, input: Input<'a>, strict: bool) -> Option<Preparation<'a>> {
    let parameter = contract.params.get(index)?;
    // PHP's MIME encoder parses supplied charset/transfer values as nonnullable strings,
    // although reflection and null-deprecation messages expose their nullable declarations.
    let parsed_type = if contract.id == RuntimeBuiltinId::MbEncodeMimeheader.builtin_id() && matches!(index, 1 | 2) {
        TypeSpec::Str
    } else { parameter.ty };
    let mask = parameter_mask(parsed_type)?;
    let mut result = Preparation { value: Err(messages::type_error(contract, index, parsed_type, input)), diagnostics: Vec::new() };
    let identity = match input {
        Input::Null if mask & NULL != 0 => Some(Prepared::Null),
        Input::Bool(value) if mask & BOOL != 0 => Some(Prepared::Bool(value)),
        Input::Int(value) if mask & INT != 0 => Some(Prepared::Int(value)),
        Input::String(value) if mask & STRING != 0 => Some(Prepared::String(Cow::Borrowed(value))),
        Input::Array if mask & ARRAY != 0 => Some(Prepared::Array),
        _ => None,
    };
    if let Some(value) = identity { result.value = Ok(value); return Some(result); }
    if mask & CALLABLE != 0 { result.value = Ok(Prepared::ResolveCallable); return Some(result); }
    if strict { return Some(result); }
    if input == Input::Null && mask & (INT | STRING | BOOL) != 0 {
        result.diagnostics.push(messages::null_argument(contract, index));
    }
    // PHP's string/integer union keeps actual strings above and otherwise tries integer first.
    if mask & INT != 0 {
        let integer = match input {
            Input::Null => Some((0, None)),
            Input::Bool(value) => Some((value as i64, None)),
            Input::Float(bits) => numeric::float_integer(bits).map(|value| (value,
                (f64::from_bits(bits) != value as f64).then(|| messages::lossy_float(bits)))),
            Input::String(bytes) => numeric_integer(bytes).map(|(value, lossy)| (value,
                lossy.then(|| messages::lossy_string(bytes)))),
            _ => None,
        };
        if let Some((value, diagnostic)) = integer {
            result.value = Ok(Prepared::Int(value));
            result.diagnostics.extend(diagnostic);
            return Some(result);
        }
    }
    if mask & STRING != 0 {
        let value = match input {
            Input::Null | Input::Bool(false) => Some(Prepared::String(Cow::Borrowed(b""))),
            Input::Bool(true) => Some(Prepared::String(Cow::Borrowed(b"1"))),
            Input::Int(value) => Some(Prepared::String(Cow::Owned(value.to_string().into_bytes()))),
            Input::Float(bits) => {
                if f64::from_bits(bits).is_nan() { result.diagnostics.push(messages::nan("string")); }
                Some(Prepared::FormatFloat(bits))
            }
            Input::Object { stringable: true, .. } => Some(Prepared::InvokeStringable),
            _ => None,
        };
        if let Some(value) = value { result.value = Ok(value); return Some(result); }
    }
    if mask & BOOL != 0 {
        let value = match input {
            Input::Null => Some(false),
            Input::Int(value) => Some(value != 0),
            Input::Float(bits) => {
                if f64::from_bits(bits).is_nan() { result.diagnostics.push(messages::nan("bool")); }
                Some(f64::from_bits(bits) != 0.0)
            }
            Input::String(bytes) => Some(!bytes.is_empty() && bytes != b"0"),
            _ => None,
        };
        if let Some(value) = value { result.value = Ok(Prepared::Bool(value)); }
    }
    Some(result)
}

/// Recognizes the scalar/array parameter shapes whose PHP outer parsing this planner implements.
pub fn parameter_mask(ty: TypeSpec) -> Option<u8> {
    match ty {
        TypeSpec::Null => Some(NULL), TypeSpec::Bool => Some(BOOL), TypeSpec::Int => Some(INT),
        TypeSpec::Str => Some(STRING), TypeSpec::Array => Some(ARRAY),
        TypeSpec::Callable => Some(CALLABLE),
        TypeSpec::Nullable(inner) => Some(parameter_mask(*inner)? | NULL),
        TypeSpec::Union(members) => members.iter().try_fold(0, |mask, &member| Some(mask | parameter_mask(member)?)),
        _ => None,
    }
}

/// Validates the outer PHP argument count before any parameter coercion or host callback runs.
pub fn arity_error(operation: RuntimeBuiltinId, count: usize) -> Option<Vec<u8>> {
    if !operation.is_mbstring() { return None; }
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id())?;
    if operation.supports_arity(count) { return None; }
    arity_error_contract(contract, count)
}

/// Formats a PHP argument-count error directly from the neutral contract before host access.
pub(crate) fn arity_error_contract(contract: &BuiltinContract, count: usize) -> Option<Vec<u8>> {
    let minimum = contract.min_args.unwrap_or_else(|| contract.params.iter().take_while(|parameter| parameter.default.is_none()).count());
    let maximum = contract.max_args.unwrap_or(contract.params.len());
    if (minimum..=maximum).contains(&count) { return None; }
    let (bound, number) = if minimum == maximum { ("exactly", maximum) }
        else if count < minimum { ("at least", minimum) } else { ("at most", maximum) };
    Some(format!("{}() expects {bound} {number} argument{}, {count} given", contract.name,
        if number == 1 { "" } else { "s" }).into_bytes())
}
