//! Purpose:
//! Decodes the stable runtime callable-descriptor ABI into eval-native metadata.
//!
//! Called from:
//! - Runtime consumers that receive a boxed callable descriptor.
//!
//! Key details:
//! - Pointer reads are restricted to descriptors produced by the generated runtime.
//! - Signature names, flags, scalar defaults, and the uniform invoker stay aligned.

use super::*;
use crate::eval_ir::EvalSourceLocation;

const DESCRIPTOR_PHP_NAME_WORD: usize = 2;
const DESCRIPTOR_PHP_NAME_LEN_WORD: usize = 3;
const DESCRIPTOR_SIGNATURE_WORD: usize = 4;
const DESCRIPTOR_ENVIRONMENT_WORD: usize = 5;
const DESCRIPTOR_INVOCATION_WORD: usize = 6;
const DESCRIPTOR_INVOKER_WORD: usize = 7;
/// Invocation-record words: shape, receiver (ptr, len), method (ptr, len), aux (ptr, len).
const INVOCATION_SHAPE_WORD: usize = 0;
const INVOCATION_AUX_WORD: usize = 5;
const INVOCATION_AUX_LEN_WORD: usize = 6;
/// `CallableDescriptorShape::Closure` on the compiler side.
const INVOCATION_SHAPE_CLOSURE: u64 = 1;

const ENVIRONMENT_CAPTURE_COUNT_WORD: usize = 0;
const ENVIRONMENT_CAPTURE_TABLE_WORD: usize = 2;
const BINDING_TYPE_WORD: usize = 2;
const BINDING_BY_REF_WORD: usize = 3;
const BINDING_WORDS: usize = 4;
const DESCRIPTOR_HEADER_WORDS: usize = 8;
const CAPTURE_WORDS: usize = 2;
const CALLBACK_ADAPTER_KIND: u64 = 3;
const CALLBACK_ADAPTER_CONTEXT_CAPTURE_WORD: usize = DESCRIPTOR_HEADER_WORDS;
const CALLBACK_ADAPTER_VALUE_CAPTURE_WORD: usize = DESCRIPTOR_HEADER_WORDS + CAPTURE_WORDS;

const SIGNATURE_VISIBLE_PARAM_COUNT_WORD: usize = 0;
const SIGNATURE_REQUIRED_PARAM_COUNT_WORD: usize = 1;
const SIGNATURE_VARIADIC_INDEX_WORD: usize = 3;
const SIGNATURE_RETURN_TYPE_WORD: usize = 4;
const SIGNATURE_FLAGS_WORD: usize = 6;
const SIGNATURE_PARAM_NAMES_WORD: usize = 7;
const SIGNATURE_PARAM_TYPES_WORD: usize = 8;
const SIGNATURE_DEFAULTS_WORD: usize = 9;
const SIGNATURE_REF_FLAGS_WORD: usize = 10;
const SIGNATURE_DECLARED_FLAGS_WORD: usize = 11;
/// `(pointer, length)` of each declared parameter's type-spec string, or `(0, 0)`.
const SIGNATURE_PARAM_TYPE_SPECS_WORD: usize = 12;

const SIGNATURE_FLAG_DECLARED_RETURN: u64 = 1;
const VARIADIC_INDEX_NONE: u64 = u64::MAX;
const MAX_DESCRIPTOR_PARAMS: usize = 4096;

const DEFAULT_NONE: u64 = 0;
const DEFAULT_INT: u64 = 1;
const DEFAULT_STRING: u64 = 2;
const DEFAULT_FLOAT: u64 = 3;
const DEFAULT_BOOL: u64 = 4;
const DEFAULT_NULL: u64 = 5;
const DEFAULT_EMPTY_ARRAY: u64 = 6;

/// Native callback metadata recovered from one live callable descriptor.
pub(crate) struct DecodedCallableDescriptor {
    pub(crate) display_name: String,
    pub(crate) function: NativeFunction,
    pub(crate) bound_this: Option<DecodedCallableCapture>,
}

/// Raw descriptor capture metadata needed to materialize a reflected bound receiver.
pub(crate) struct DecodedCallableCapture {
    pub(crate) type_tag: u64,
    pub(crate) value_word: u64,
}

/// Decodes a generated callable descriptor referenced by a runtime callable value.
///
/// # Safety
/// `descriptor` must point to a live descriptor produced by Elephc's callable ABI.
pub(crate) unsafe fn decode_callable_descriptor(
    descriptor: *mut c_void,
) -> Option<DecodedCallableDescriptor> {
    if descriptor.is_null() {
        return None;
    }
    let words = descriptor.cast::<u64>();
    let signature = read_word(words, DESCRIPTOR_SIGNATURE_WORD)? as *const u64;
    let invoker_word = read_word(words, DESCRIPTOR_INVOKER_WORD)?;
    if signature.is_null() || invoker_word == 0 {
        return None;
    }
    let param_count = usize::try_from(read_word(signature, SIGNATURE_VISIBLE_PARAM_COUNT_WORD)?)
        .ok()?;
    if param_count > MAX_DESCRIPTOR_PARAMS {
        return None;
    }
    let invoker: NativeFunctionInvoker = std::mem::transmute(invoker_word as usize);
    let mut function = NativeFunction::new(descriptor, invoker, param_count);
    decode_required_param_count(signature, &mut function)?;
    decode_parameter_names(signature, &mut function)?;
    decode_parameter_types(signature, &mut function)?;
    decode_parameter_defaults(signature, &mut function)?;
    decode_parameter_flags(signature, &mut function)?;
    decode_return_type(signature, &mut function)?;
    mark_hidden_func_args_collector(&mut function);
    if let Some((file, location)) = decode_closure_source(words) {
        function.set_source(file, location);
    }
    let display_name = decode_descriptor_string(
        read_word(words, DESCRIPTOR_PHP_NAME_WORD)?,
        read_word(words, DESCRIPTOR_PHP_NAME_LEN_WORD)?,
    )
    .unwrap_or_else(|| String::from("{closure}"));
    let bound_this = decode_bound_this_capture(words)?;
    Some(DecodedCallableDescriptor {
        display_name,
        function,
        bound_this,
    })
}

/// Reads a compiled closure literal's `START:END:FILE` source record from its invocation record.
///
/// The compiler writes it as the closure shape's aux string; `ReflectionFunction` answers
/// `getStartLine()`, `getEndLine()` and `getFileName()` from it.
unsafe fn decode_closure_source(descriptor: *const u64) -> Option<(String, EvalSourceLocation)> {
    let invocation = read_word(descriptor, DESCRIPTOR_INVOCATION_WORD)? as *const u64;
    if (invocation as usize) < 4096
        || read_word(invocation, INVOCATION_SHAPE_WORD)? != INVOCATION_SHAPE_CLOSURE
    {
        return None;
    }
    let length = read_word(invocation, INVOCATION_AUX_LEN_WORD)?;
    if length == 0 || length > 1 << 20 {
        return None;
    }
    let aux = decode_descriptor_string(read_word(invocation, INVOCATION_AUX_WORD)?, length)?;
    let mut parts = aux.splitn(3, ':');
    let start = parts.next()?.parse::<i64>().ok()?;
    let end = parts.next()?.parse::<i64>().ok()?;
    let file = parts.next()?.to_string();
    Some((file, EvalSourceLocation::new(start, end)))
}

/// Name the compiler gives the hidden surplus-argument collector of a `func_get_args()` body
/// (`crate::func_args::HIDDEN_ARGS_PARAM` on the compiler side).
const HIDDEN_FUNC_ARGS_PARAM: &str = "__elephc_func_args#gen";

/// Declares a descriptor's trailing hidden `func_get_args()` collector as compiler-internal.
///
/// A descriptor's signature table lists every physical slot, and a closure whose body calls
/// `func_get_args()` carries the hidden variadic collector as its last one. Without a shape the
/// frame took it for a PHP variadic, so `new ReflectionFunction($closure)` reported the extra
/// `$__elephc_func_args#gen` parameter and `isVariadic()`. The binding still fills the collector:
/// only the visible/hidden partition changes.
fn mark_hidden_func_args_collector(function: &mut NativeFunction) {
    let names = function.param_names();
    let Some(last) = names.len().checked_sub(1) else {
        return;
    };
    if names[last] != HIDDEN_FUNC_ARGS_PARAM || !function.param_variadic(last) {
        return;
    }
    let required = function.required_param_count().min(last);
    function.set_param_name(last, "");
    function.set_shape(NativeCallableShape::new(last, required, false, false));
}

/// Finds the non-reference `$this` binding in a descriptor environment, when present.
unsafe fn decode_bound_this_capture(
    descriptor: *const u64,
) -> Option<Option<DecodedCallableCapture>> {
    decode_named_callable_descriptor_capture(descriptor, "this")
}

/// Finds one non-reference descriptor capture by its exact binding name.
pub(crate) unsafe fn decode_named_callable_descriptor_capture(
    descriptor: *const u64,
    expected_name: &str,
) -> Option<Option<DecodedCallableCapture>> {
    if (descriptor as usize) < 4096 {
        return None;
    }
    let environment = read_word(descriptor, DESCRIPTOR_ENVIRONMENT_WORD)? as *const u64;
    if environment.is_null() {
        return Some(None);
    }
    let capture_count = usize::try_from(read_word(
        environment,
        ENVIRONMENT_CAPTURE_COUNT_WORD,
    )?)
    .ok()?;
    if capture_count > MAX_DESCRIPTOR_PARAMS {
        return None;
    }
    let bindings = read_word(environment, ENVIRONMENT_CAPTURE_TABLE_WORD)? as *const u64;
    if bindings.is_null() {
        return Some(None);
    }
    for index in 0..capture_count {
        let binding_base = index.checked_mul(BINDING_WORDS)?;
        let name = decode_descriptor_string(
            read_word(bindings, binding_base)?,
            read_word(bindings, binding_base.checked_add(1)?)?,
        )?;
        if name != expected_name
            || read_word(
                bindings,
                binding_base.checked_add(BINDING_BY_REF_WORD)?,
            )? != 0
        {
            continue;
        }
        let capture_base = DESCRIPTOR_HEADER_WORDS
            .checked_add(index.checked_mul(CAPTURE_WORDS)?)?;
        return Some(Some(DecodedCallableCapture {
            type_tag: read_word(
                bindings,
                binding_base.checked_add(BINDING_TYPE_WORD)?,
            )?,
            value_word: read_word(descriptor, capture_base)?,
        }));
    }
    Some(None)
}

/// Decodes the inline boxed callback captured by a generated eval callback adapter.
pub(crate) unsafe fn decode_eval_callback_adapter_capture(
    descriptor: *const u64,
) -> Option<(u64, DecodedCallableCapture)> {
    if (descriptor as usize) < 4096 || read_word(descriptor, 0)? != CALLBACK_ADAPTER_KIND {
        return None;
    }
    Some((
        read_word(descriptor, CALLBACK_ADAPTER_CONTEXT_CAPTURE_WORD)?,
        DecodedCallableCapture {
            type_tag: 7,
            value_word: read_word(descriptor, CALLBACK_ADAPTER_VALUE_CAPTURE_WORD)?,
        },
    ))
}

/// Reads the exact required arity stored in the signature record.
unsafe fn decode_required_param_count(
    signature: *const u64,
    function: &mut NativeFunction,
) -> Option<()> {
    let required = usize::try_from(read_word(
        signature,
        SIGNATURE_REQUIRED_PARAM_COUNT_WORD,
    )?)
    .ok()?;
    function.set_required_param_count(required).then_some(())
}

/// Copies PHP-visible parameter names from the descriptor table.
unsafe fn decode_parameter_names(
    signature: *const u64,
    function: &mut NativeFunction,
) -> Option<()> {
    let table = read_word(signature, SIGNATURE_PARAM_NAMES_WORD)? as *const u64;
    if table.is_null() {
        return Some(());
    }
    for index in 0..function.param_count() {
        let name = decode_descriptor_string(
            read_word(table, index.checked_mul(2)?)?,
            read_word(table, index.checked_mul(2)?.checked_add(1)?)?,
        )?;
        if !function.set_param_name(index, name) {
            return None;
        }
    }
    Some(())
}

/// Copies declared parameter types from the compact type table.
unsafe fn decode_parameter_types(
    signature: *const u64,
    function: &mut NativeFunction,
) -> Option<()> {
    let types = read_word(signature, SIGNATURE_PARAM_TYPES_WORD)? as *const u64;
    let declared = read_word(signature, SIGNATURE_DECLARED_FLAGS_WORD)? as *const u64;
    if types.is_null() || declared.is_null() {
        return Some(());
    }
    for index in 0..function.param_count() {
        if read_word(declared, index)? == 0 {
            continue;
        }
        // The declared type as written wins; the representation tag is the fallback for a
        // record without one (a union of classes has only the `Mixed` tag of its slot).
        let param_type = declared_param_type_spec(signature, index)
            .or_else(|| callable_type_from_tag(read_word(types, index.checked_mul(3)?)?, false));
        if let Some(param_type) = param_type {
            if !function.set_param_type(index, param_type) {
                return None;
            }
        }
    }
    Some(())
}

/// Reads one parameter's declared type-spec string from the signature record, when present.
unsafe fn declared_param_type_spec(signature: *const u64, index: usize) -> Option<EvalParameterType> {
    let specs = read_word(signature, SIGNATURE_PARAM_TYPE_SPECS_WORD)? as *const u64;
    if specs.is_null() {
        return None;
    }
    let base = index.checked_mul(2)?;
    let ptr = read_word(specs, base)? as *const u8;
    let len = read_word(specs, base.checked_add(1)?)?;
    if ptr.is_null() || len == 0 {
        return None;
    }
    crate::ffi::native_methods::native_callable_param_type_from_abi(ptr, len)
}

/// Copies scalar and empty-array defaults from the descriptor table.
unsafe fn decode_parameter_defaults(
    signature: *const u64,
    function: &mut NativeFunction,
) -> Option<()> {
    let defaults = read_word(signature, SIGNATURE_DEFAULTS_WORD)? as *const u64;
    if defaults.is_null() {
        return Some(());
    }
    for index in 0..function.param_count() {
        let base = index.checked_mul(3)?;
        let kind = read_word(defaults, base)?;
        let lo = read_word(defaults, base.checked_add(1)?)?;
        let hi = read_word(defaults, base.checked_add(2)?)?;
        let Some(default) = callable_default_from_words(kind, lo, hi) else {
            continue;
        };
        if !function.set_param_default(index, default) {
            return None;
        }
    }
    Some(())
}

/// Copies by-reference and variadic flags from the descriptor signature.
unsafe fn decode_parameter_flags(
    signature: *const u64,
    function: &mut NativeFunction,
) -> Option<()> {
    let ref_flags = read_word(signature, SIGNATURE_REF_FLAGS_WORD)? as *const u64;
    if !ref_flags.is_null() {
        for index in 0..function.param_count() {
            if !function.set_param_by_ref(index, read_word(ref_flags, index)? != 0) {
                return None;
            }
        }
    }
    let variadic = read_word(signature, SIGNATURE_VARIADIC_INDEX_WORD)?;
    if variadic != VARIADIC_INDEX_NONE {
        let index = usize::try_from(variadic).ok()?;
        if !function.set_variadic_index(index) {
            return None;
        }
    }
    Some(())
}

/// Copies a declared return type from the descriptor signature.
unsafe fn decode_return_type(
    signature: *const u64,
    function: &mut NativeFunction,
) -> Option<()> {
    let flags = read_word(signature, SIGNATURE_FLAGS_WORD)?;
    if flags & SIGNATURE_FLAG_DECLARED_RETURN == 0 {
        return Some(());
    }
    let type_word = read_word(signature, SIGNATURE_RETURN_TYPE_WORD)?;
    if let Some(return_type) = callable_type_from_tag(type_word, true) {
        function.set_return_type(return_type);
    }
    Some(())
}

/// Converts one descriptor type tag into eval Reflection type metadata.
fn callable_type_from_tag(tag: u64, is_return: bool) -> Option<EvalParameterType> {
    let variant = match tag {
        0 => EvalParameterTypeVariant::Int,
        1 => EvalParameterTypeVariant::String,
        2 => EvalParameterTypeVariant::Float,
        3 => EvalParameterTypeVariant::Bool,
        4 | 5 | 14 => EvalParameterTypeVariant::Array,
        6 => EvalParameterTypeVariant::Object,
        7 => EvalParameterTypeVariant::Mixed,
        8 if is_return => EvalParameterTypeVariant::Void,
        10 => EvalParameterTypeVariant::Callable,
        12 => EvalParameterTypeVariant::Iterable,
        15 if is_return => EvalParameterTypeVariant::Never,
        _ => return None,
    };
    Some(EvalParameterType::new(vec![variant], false))
}

/// Decodes one supported scalar default record.
unsafe fn callable_default_from_words(
    kind: u64,
    lo: u64,
    hi: u64,
) -> Option<NativeCallableDefault> {
    match kind {
        DEFAULT_NONE => None,
        DEFAULT_INT => Some(NativeCallableDefault::Int(lo as i64)),
        DEFAULT_STRING => decode_descriptor_string(lo, hi).map(NativeCallableDefault::String),
        DEFAULT_FLOAT => Some(NativeCallableDefault::Float(f64::from_bits(lo))),
        DEFAULT_BOOL => Some(NativeCallableDefault::Bool(lo != 0)),
        DEFAULT_NULL => Some(NativeCallableDefault::Null),
        DEFAULT_EMPTY_ARRAY => Some(NativeCallableDefault::EmptyArray),
        _ => None,
    }
}

/// Copies one pointer/length string from generated read-only descriptor data.
unsafe fn decode_descriptor_string(pointer: u64, length: u64) -> Option<String> {
    let length = usize::try_from(length).ok()?;
    if length == 0 {
        return Some(String::new());
    }
    let pointer = pointer as *const u8;
    if pointer.is_null() {
        return None;
    }
    std::str::from_utf8(std::slice::from_raw_parts(pointer, length))
        .ok()
        .map(str::to_string)
}

/// Reads one machine word from a generated descriptor table.
unsafe fn read_word(base: *const u64, index: usize) -> Option<u64> {
    if base.is_null() {
        return None;
    }
    Some(base.add(index).read_unaligned())
}
