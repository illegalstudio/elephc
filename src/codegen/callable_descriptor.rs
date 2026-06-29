//! Purpose:
//! Defines the storage layout for runtime callable descriptor pointers.
//! Centralizes descriptor materialization, signature metadata, environment metadata,
//! invocation shape metadata, and entry loading for indirect calls.
//!
//! Called from:
//! - Closure, first-class callable, callback, Fiber, and SPL callback emitters.
//!
//! Key details:
//! - `PhpType::Callable` remains one pointer-wide, but the pointer now targets a
//!   descriptor whose entry slot is loaded before invoking native code.
//! - Descriptor side records keep callable signature/default/by-ref/variadic and
//!   capture/receiver metadata available to runtime dispatch without changing the
//!   one-word callable ABI.
//! - The optional invoker slot points at a generated uniform runtime adapter whose
//!   ABI is `(descriptor, argument array) -> Mixed`.

use crate::codegen::abi;
use crate::codegen::data_section::{DataSection, DataWord};
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

pub(crate) const CALLABLE_DESC_KIND_CLOSURE: u64 = CallableDescriptorShape::Closure as u64;
pub(crate) const CALLABLE_DESC_KIND_FIRST_CLASS: u64 =
    CallableDescriptorShape::FirstClass as u64;
pub(crate) const CALLABLE_DESC_KIND_CALLBACK_ADAPTER: u64 =
    CallableDescriptorShape::CallbackAdapter as u64;
pub(crate) const CALLABLE_DESC_KIND_FUNCTION: u64 = CallableDescriptorShape::Function as u64;
pub(crate) const CALLABLE_DESC_KIND_BUILTIN: u64 = CallableDescriptorShape::Builtin as u64;
pub(crate) const CALLABLE_DESC_KIND_EXTERN: u64 = CallableDescriptorShape::Extern as u64;
pub(crate) const CALLABLE_DESC_KIND_STATIC_METHOD: u64 =
    CallableDescriptorShape::StaticMethod as u64;
pub(crate) const CALLABLE_DESC_KIND_OBJECT_INVOKE: u64 =
    CallableDescriptorShape::ObjectInvoke as u64;
pub(crate) const CALLABLE_DESC_KIND_INSTANCE_METHOD: u64 =
    CallableDescriptorShape::InstanceMethod as u64;

pub(crate) const CALLABLE_DESC_ENTRY_OFFSET: usize = 8;
#[allow(dead_code)]
pub(crate) const CALLABLE_DESC_SIGNATURE_OFFSET: usize = 32;
#[allow(dead_code)]
pub(crate) const CALLABLE_DESC_ENVIRONMENT_OFFSET: usize = 40;
#[allow(dead_code)]
pub(crate) const CALLABLE_DESC_INVOCATION_OFFSET: usize = 48;
#[allow(dead_code)]
pub(crate) const CALLABLE_DESC_INVOKER_OFFSET: usize = 56;
pub(crate) const CALLABLE_DESC_STATIC_SIZE: usize = 64;
pub(crate) const CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET: usize = CALLABLE_DESC_STATIC_SIZE;

const CALLABLE_DESC_VARIADIC_NONE: u64 = u64::MAX;

const CALLABLE_DESC_DEFAULT_NONE: u64 = 0;
const CALLABLE_DESC_DEFAULT_INT: u64 = 1;
const CALLABLE_DESC_DEFAULT_STRING: u64 = 2;
const CALLABLE_DESC_DEFAULT_FLOAT: u64 = 3;
const CALLABLE_DESC_DEFAULT_BOOL: u64 = 4;
const CALLABLE_DESC_DEFAULT_NULL: u64 = 5;
const CALLABLE_DESC_DEFAULT_EMPTY_ARRAY: u64 = 6;
const CALLABLE_DESC_DEFAULT_COMPLEX: u64 = 255;

#[derive(Clone, Copy, Debug)]
#[repr(u64)]
#[allow(dead_code)]
/// Runtime callable shapes encoded in descriptor metadata.
///
/// The first three values preserve the descriptor kind values that existing
/// callable storage already emits. Later values describe callable forms that
/// can be selected by runtime dispatch or future descriptor materialization.
pub(crate) enum CallableDescriptorShape {
    Closure = 1,
    FirstClass = 2,
    CallbackAdapter = 3,
    String = 4,
    Array = 5,
    ObjectInvoke = 6,
    StaticMethod = 7,
    InstanceMethod = 8,
    Builtin = 9,
    Extern = 10,
    Function = 11,
}

/// Metadata describing how the descriptor should invoke or adapt the target.
#[derive(Clone, Debug)]
pub(crate) struct CallableDescriptorInvocation {
    pub(crate) shape: CallableDescriptorShape,
    pub(crate) receiver_name: Option<String>,
    pub(crate) method_name: Option<String>,
    pub(crate) aux_name: Option<String>,
}

/// Full static callable descriptor input before data-section serialization.
pub(crate) struct CallableDescriptorSpec<'a> {
    pub(crate) entry_label: &'a str,
    pub(crate) php_name: Option<&'a str>,
    pub(crate) kind: u64,
    pub(crate) sig: Option<&'a FunctionSig>,
    pub(crate) captures: &'a [(String, PhpType, bool)],
    pub(crate) hidden_params: &'a [(String, PhpType, bool)],
    pub(crate) invocation: CallableDescriptorInvocation,
    pub(crate) invoker_label: Option<&'a str>,
}

impl CallableDescriptorInvocation {
    /// Creates invocation metadata for a descriptor shape with no receiver or method payload.
    pub(crate) fn new(shape: CallableDescriptorShape) -> Self {
        Self {
            shape,
            receiver_name: None,
            method_name: None,
            aux_name: None,
        }
    }

    /// Creates invocation metadata for a globally named callable target.
    pub(crate) fn named(shape: CallableDescriptorShape, name: impl Into<String>) -> Self {
        Self {
            shape,
            receiver_name: Some(name.into()),
            method_name: None,
            aux_name: None,
        }
    }

    /// Creates invocation metadata for method-like callable targets.
    pub(crate) fn method(
        shape: CallableDescriptorShape,
        receiver_name: Option<String>,
        method_name: impl Into<String>,
    ) -> Self {
        Self {
            shape,
            receiver_name,
            method_name: Some(method_name.into()),
            aux_name: None,
        }
    }
}

/// Emits a descriptor with signature, environment, and invocation side records.
#[allow(clippy::too_many_arguments)]
pub(crate) fn static_descriptor_with_meta(
    data: &mut DataSection,
    entry_label: &str,
    php_name: Option<&str>,
    kind: u64,
    sig: Option<&FunctionSig>,
    captures: &[(String, PhpType, bool)],
    hidden_params: &[(String, PhpType, bool)],
    invocation: CallableDescriptorInvocation,
) -> String {
    static_descriptor_with_optional_invoker_meta(
        data,
        entry_label,
        php_name,
        kind,
        sig,
        captures,
        hidden_params,
        invocation,
        None,
    )
}

/// Emits a descriptor with side records and an optional runtime invoker entry.
#[allow(clippy::too_many_arguments)]
pub(crate) fn static_descriptor_with_optional_invoker_meta(
    data: &mut DataSection,
    entry_label: &str,
    php_name: Option<&str>,
    kind: u64,
    sig: Option<&FunctionSig>,
    captures: &[(String, PhpType, bool)],
    hidden_params: &[(String, PhpType, bool)],
    invocation: CallableDescriptorInvocation,
    invoker_label: Option<&str>,
) -> String {
    let spec = CallableDescriptorSpec {
        entry_label,
        php_name,
        kind,
        sig,
        captures,
        hidden_params,
        invocation,
        invoker_label,
    };
    static_descriptor_from_spec(data, &spec)
}

/// Serializes a descriptor record and returns its data label.
fn static_descriptor_from_spec(
    data: &mut DataSection,
    spec: &CallableDescriptorSpec<'_>,
) -> String {
    let (name_label, name_len) = match spec.php_name {
        Some(name) => {
            let (label, len) = data.add_string(name.as_bytes());
            (Some(label), len as u64)
        }
        None => (None, 0),
    };

    let name_word = name_label
        .map(DataWord::Symbol)
        .unwrap_or(DataWord::U64(0));
    let signature_word = spec
        .sig
        .map(|sig| DataWord::Symbol(signature_record(data, sig)))
        .unwrap_or(DataWord::U64(0));
    let environment_word = if spec.captures.is_empty() && spec.hidden_params.is_empty() {
        DataWord::U64(0)
    } else {
        DataWord::Symbol(environment_record(
            data,
            spec.captures,
            spec.hidden_params,
        ))
    };
    let invocation_word = DataWord::Symbol(invocation_record(data, &spec.invocation));
    let invoker_word = spec
        .invoker_label
        .map(|label| DataWord::Symbol(label.to_string()))
        .unwrap_or(DataWord::U64(0));

    data.add_words(vec![
        DataWord::U64(spec.kind),
        DataWord::Symbol(spec.entry_label.to_string()),
        name_word,
        DataWord::U64(name_len),
        signature_word,
        environment_word,
        invocation_word,
        invoker_word,
    ])
}

/// Emits assembly for loading a descriptor address with side metadata.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_load_descriptor_address_with_meta(
    emitter: &mut Emitter,
    data: &mut DataSection,
    dest_reg: &str,
    entry_label: &str,
    php_name: Option<&str>,
    kind: u64,
    sig: Option<&FunctionSig>,
    captures: &[(String, PhpType, bool)],
    hidden_params: &[(String, PhpType, bool)],
    invocation: CallableDescriptorInvocation,
) {
    let descriptor_label = static_descriptor_with_meta(
        data,
        entry_label,
        php_name,
        kind,
        sig,
        captures,
        hidden_params,
        invocation,
    );
    abi::emit_symbol_address(emitter, dest_reg, &descriptor_label);
}

/// Emits assembly for loading the ABI entry slot from a descriptor.
pub(crate) fn emit_load_entry_from_descriptor(
    emitter: &mut Emitter,
    dest_reg: &str,
    descriptor_reg: &str,
) {
    abi::emit_load_from_address(emitter, dest_reg, descriptor_reg, CALLABLE_DESC_ENTRY_OFFSET);
}

/// Emits assembly for loading the uniform invoker slot from a descriptor.
pub(crate) fn emit_load_invoker_from_descriptor(
    emitter: &mut Emitter,
    dest_reg: &str,
    descriptor_reg: &str,
) {
    abi::emit_load_from_address(emitter, dest_reg, descriptor_reg, CALLABLE_DESC_INVOKER_OFFSET);
}

/// Retains the callable descriptor pointer currently held in the integer result register.
pub(crate) fn emit_retain_current_descriptor(emitter: &mut Emitter) {
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result_reg);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_pop_reg(emitter, result_reg);
}

/// Releases the callable descriptor pointer currently held in the integer result register.
pub(crate) fn emit_release_current_descriptor(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_callable_descriptor_release");
}

/// Copies the fixed static descriptor header into a runtime descriptor allocation.
pub(crate) fn emit_copy_static_descriptor_to_runtime(
    emitter: &mut Emitter,
    dest_reg: &str,
    descriptor_label: &str,
) {
    let source_reg = abi::symbol_scratch_reg(emitter);
    let word_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_symbol_address(emitter, source_reg, descriptor_label);
    for offset in (0..CALLABLE_DESC_STATIC_SIZE).step_by(8) {
        abi::emit_load_from_address(emitter, word_reg, source_reg, offset);
        abi::emit_store_to_address(emitter, word_reg, dest_reg, offset);
    }
}

/// Stores the current result registers into a runtime descriptor capture slot.
pub(crate) fn emit_store_current_result_to_runtime_capture(
    emitter: &mut Emitter,
    descriptor_reg: &str,
    capture_index: usize,
    capture_ty: &PhpType,
) {
    let offset = CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + capture_index * 16;
    match capture_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), descriptor_reg, offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_store_to_address(emitter, ptr_reg, descriptor_reg, offset);
            abi::emit_store_to_address(emitter, len_reg, descriptor_reg, offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), descriptor_reg, offset);
        }
    }
}

/// Loads a runtime descriptor capture slot into the ABI result registers.
pub(crate) fn emit_load_runtime_capture_to_result(
    emitter: &mut Emitter,
    descriptor_reg: &str,
    capture_index: usize,
    capture_ty: &PhpType,
) {
    let offset = CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + capture_index * 16;
    match capture_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), descriptor_reg, offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_load_from_address(emitter, ptr_reg, descriptor_reg, offset);
            abi::emit_load_from_address(emitter, len_reg, descriptor_reg, offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), descriptor_reg, offset);
        }
    }
}

/// Builds the signature side record for runtime arity and argument planning.
fn signature_record(data: &mut DataSection, sig: &FunctionSig) -> String {
    let visible_param_count = sig.params.len();
    let regular_param_count = if sig.variadic.is_some() {
        visible_param_count.saturating_sub(1)
    } else {
        visible_param_count
    };
    let required_count = (0..regular_param_count)
        .filter(|idx| sig.defaults.get(*idx).and_then(|default| default.as_ref()).is_none())
        .map(|idx| idx + 1)
        .max()
        .unwrap_or(0);
    let variadic_index = sig
        .variadic
        .as_ref()
        .and_then(|variadic| sig.params.iter().position(|(name, _)| name == variadic))
        .map(|idx| idx as u64)
        .unwrap_or(CALLABLE_DESC_VARIADIC_NONE);
    let flags = u64::from(sig.declared_return);
    let param_names = optional_symbol(param_name_table(data, &sig.params));
    let param_types = optional_symbol(param_type_table(data, &sig.params));
    let defaults = optional_symbol(default_table(data, &sig.defaults));
    let ref_flags = optional_symbol(flag_table(data, &sig.ref_params));
    let declared_flags = optional_symbol(flag_table(data, &sig.declared_params));

    data.add_words(vec![
        DataWord::U64(visible_param_count as u64),
        DataWord::U64(required_count as u64),
        DataWord::U64(regular_param_count as u64),
        DataWord::U64(variadic_index),
        DataWord::U64(type_tag(&sig.return_type)),
        DataWord::U64(sig.return_type.codegen_repr().register_count() as u64),
        DataWord::U64(flags),
        param_names,
        param_types,
        defaults,
        ref_flags,
        declared_flags,
    ])
}

/// Builds the descriptor environment record for captures and hidden wrapper params.
fn environment_record(
    data: &mut DataSection,
    captures: &[(String, PhpType, bool)],
    hidden_params: &[(String, PhpType, bool)],
) -> String {
    let capture_table = optional_symbol(binding_table(data, captures));
    let hidden_table = optional_symbol(binding_table(data, hidden_params));
    data.add_words(vec![
        DataWord::U64(captures.len() as u64),
        DataWord::U64(hidden_params.len() as u64),
        capture_table,
        hidden_table,
    ])
}

/// Builds the descriptor invocation record for target-shape metadata.
fn invocation_record(
    data: &mut DataSection,
    invocation: &CallableDescriptorInvocation,
) -> String {
    let (receiver_word, receiver_len) = optional_string_word(data, invocation.receiver_name.as_deref());
    let (method_word, method_len) = optional_string_word(data, invocation.method_name.as_deref());
    let (aux_word, aux_len) = optional_string_word(data, invocation.aux_name.as_deref());
    data.add_words(vec![
        DataWord::U64(invocation.shape as u64),
        receiver_word,
        DataWord::U64(receiver_len),
        method_word,
        DataWord::U64(method_len),
        aux_word,
        DataWord::U64(aux_len),
    ])
}

/// Builds the parameter-name metadata table.
fn param_name_table(data: &mut DataSection, params: &[(String, PhpType)]) -> Option<String> {
    if params.is_empty() {
        return None;
    }
    let mut words = Vec::with_capacity(params.len() * 2);
    for (name, _) in params {
        let (name_word, name_len) = optional_string_word(data, Some(name));
        words.push(name_word);
        words.push(DataWord::U64(name_len));
    }
    Some(data.add_words(words))
}

/// Builds the parameter type/size/register metadata table.
fn param_type_table(data: &mut DataSection, params: &[(String, PhpType)]) -> Option<String> {
    if params.is_empty() {
        return None;
    }
    let mut words = Vec::with_capacity(params.len() * 3);
    for (_, ty) in params {
        let repr = ty.codegen_repr();
        words.push(DataWord::U64(type_tag(&repr)));
        words.push(DataWord::U64(repr.stack_size() as u64));
        words.push(DataWord::U64(repr.register_count() as u64));
    }
    Some(data.add_words(words))
}

/// Builds the default-value metadata table.
fn default_table(data: &mut DataSection, defaults: &[Option<Expr>]) -> Option<String> {
    if defaults.is_empty() {
        return None;
    }
    let mut words = Vec::with_capacity(defaults.len() * 3);
    for default in defaults {
        let encoded = encode_default_value(data, default.as_ref());
        words.push(DataWord::U64(encoded.kind));
        words.push(encoded.lo);
        words.push(DataWord::U64(encoded.hi));
    }
    Some(data.add_words(words))
}

/// Builds a compact boolean flag table.
fn flag_table(data: &mut DataSection, flags: &[bool]) -> Option<String> {
    if flags.is_empty() {
        return None;
    }
    Some(data.add_words(
        flags
            .iter()
            .map(|flag| DataWord::U64(u64::from(*flag)))
            .collect(),
    ))
}

/// Builds a named binding table for capture or hidden parameter metadata.
fn binding_table(
    data: &mut DataSection,
    bindings: &[(String, PhpType, bool)],
) -> Option<String> {
    if bindings.is_empty() {
        return None;
    }
    let mut words = Vec::with_capacity(bindings.len() * 4);
    for (name, ty, by_ref) in bindings {
        let (name_word, name_len) = optional_string_word(data, Some(name));
        words.push(name_word);
        words.push(DataWord::U64(name_len));
        words.push(DataWord::U64(type_tag(&ty.codegen_repr())));
        words.push(DataWord::U64(u64::from(*by_ref)));
    }
    Some(data.add_words(words))
}

/// Converts an optional string into a data word pointer and byte length.
fn optional_string_word(data: &mut DataSection, value: Option<&str>) -> (DataWord, u64) {
    match value {
        Some(value) => {
            let (label, len) = data.add_string(value.as_bytes());
            (DataWord::Symbol(label), len as u64)
        }
        None => (DataWord::U64(0), 0),
    }
}

/// Converts an optional table label into a symbol data word.
fn optional_symbol(label: Option<String>) -> DataWord {
    label.map(DataWord::Symbol).unwrap_or(DataWord::U64(0))
}

/// Encoded default literal metadata used by callable signature records.
struct EncodedDefault {
    kind: u64,
    lo: DataWord,
    hi: u64,
}

/// Encodes PHP default expressions that can be represented in static metadata.
fn encode_default_value(data: &mut DataSection, default: Option<&Expr>) -> EncodedDefault {
    let Some(default) = default else {
        return encoded_default(CALLABLE_DESC_DEFAULT_NONE, DataWord::U64(0), 0);
    };

    match &default.kind {
        ExprKind::IntLiteral(value) => {
            encoded_default(CALLABLE_DESC_DEFAULT_INT, DataWord::U64(*value as u64), 0)
        }
        ExprKind::FloatLiteral(value) => {
            encoded_default(CALLABLE_DESC_DEFAULT_FLOAT, DataWord::U64(value.to_bits()), 0)
        }
        ExprKind::StringLiteral(value) => {
            let (label, len) = data.add_string(value.as_bytes());
            encoded_default(CALLABLE_DESC_DEFAULT_STRING, DataWord::Symbol(label), len as u64)
        }
        ExprKind::BoolLiteral(value) => {
            encoded_default(CALLABLE_DESC_DEFAULT_BOOL, DataWord::U64(u64::from(*value)), 0)
        }
        ExprKind::Null => encoded_default(CALLABLE_DESC_DEFAULT_NULL, DataWord::U64(0), 0),
        ExprKind::ArrayLiteral(items) if items.is_empty() => {
            encoded_default(CALLABLE_DESC_DEFAULT_EMPTY_ARRAY, DataWord::U64(0), 0)
        }
        ExprKind::Negate(inner) => encode_negated_default(data, inner),
        _ => encoded_default(CALLABLE_DESC_DEFAULT_COMPLEX, DataWord::U64(0), 0),
    }
}

/// Encodes negated numeric default literals without treating them as complex.
fn encode_negated_default(data: &mut DataSection, expr: &Expr) -> EncodedDefault {
    match &expr.kind {
        ExprKind::IntLiteral(value) => {
            encoded_default(CALLABLE_DESC_DEFAULT_INT, DataWord::U64((-*value) as u64), 0)
        }
        ExprKind::FloatLiteral(value) => {
            encoded_default(CALLABLE_DESC_DEFAULT_FLOAT, DataWord::U64((-*value).to_bits()), 0)
        }
        _ => encode_default_value(data, Some(expr)),
    }
}

/// Creates an encoded default record from its raw metadata fields.
fn encoded_default(kind: u64, lo: DataWord, hi: u64) -> EncodedDefault {
    EncodedDefault { kind, lo, hi }
}

/// Returns the callable descriptor type tag for a PHP codegen type.
fn type_tag(ty: &PhpType) -> u64 {
    match ty.codegen_repr() {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed | PhpType::Union(_) => 7,
        PhpType::Void => 8,
        PhpType::Resource(_) => 9,
        PhpType::Callable => 10,
        PhpType::Pointer(_) => 11,
        PhpType::Iterable => 12,
        PhpType::Buffer(_) => 13,
        PhpType::Packed(_) => 14,
        PhpType::Never => 15,
        PhpType::TaggedScalar => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    /// Verifies that descriptor records contain signature, environment, and invocation pointers.
    #[test]
    fn test_descriptor_serializes_signature_environment_and_invocation_records() {
        let mut data = DataSection::new();
        let sig = FunctionSig {
            params: vec![
                ("value".to_string(), PhpType::Int),
                ("label".to_string(), PhpType::Str),
                ("rest".to_string(), PhpType::Array(Box::new(PhpType::Mixed))),
            ],
            defaults: vec![
                None,
                Some(Expr::new(ExprKind::StringLiteral("fallback".to_string()), Span::dummy())),
                Some(Expr::new(ExprKind::ArrayLiteral(Vec::new()), Span::dummy())),
            ],
            return_type: PhpType::Bool,
            declared_return: true,
            by_ref_return: false,
            ref_params: vec![true, false, false],
            declared_params: vec![true, true, false],
            variadic: Some("rest".to_string()),
            deprecation: None,
        };
        let captures = vec![("offset".to_string(), PhpType::Int, false)];
        let hidden = vec![("receiver".to_string(), PhpType::Object("Box".to_string()), false)];

        let descriptor = static_descriptor_with_meta(
            &mut data,
            "_call_entry",
            Some("Box::call"),
            CALLABLE_DESC_KIND_FIRST_CLASS,
            Some(&sig),
            &captures,
            &hidden,
            CallableDescriptorInvocation::method(
                CallableDescriptorShape::InstanceMethod,
                Some("Box".to_string()),
                "call",
            ),
        );
        let asm = data.emit();

        assert!(asm.contains(&format!(".globl {}\n{}:\n", descriptor, descriptor)));
        assert!(asm.contains("    .quad _call_entry\n"));
        assert!(asm.contains(".ascii \"Box::call\"\n"));
        assert!(asm.contains(".ascii \"fallback\"\n"));
        assert!(asm.contains(".ascii \"offset\"\n"));
        assert!(asm.contains(".ascii \"receiver\"\n"));
        assert!(asm.contains("    .quad 0x0000000000000002\n"));
        assert!(asm.contains("    .quad 0x0000000000000006\n"));
    }

    /// Verifies that descriptors can carry a uniform runtime invoker entry.
    #[test]
    fn test_descriptor_serializes_optional_invoker_slot() {
        let mut data = DataSection::new();
        let sig = FunctionSig {
            params: vec![("value".to_string(), PhpType::Int)],
            defaults: vec![None],
            return_type: PhpType::Int,
            declared_return: false,
            by_ref_return: false,
            ref_params: vec![false],
            declared_params: vec![false],
            variadic: None,
            deprecation: None,
        };

        let descriptor = static_descriptor_with_optional_invoker_meta(
            &mut data,
            "_call_entry",
            Some("demo"),
            CALLABLE_DESC_KIND_FUNCTION,
            Some(&sig),
            &[],
            &[],
            CallableDescriptorInvocation::named(CallableDescriptorShape::Function, "demo"),
            Some("_call_invoker"),
        );
        let asm = data.emit();

        assert!(asm.contains(&format!(".globl {}\n{}:\n", descriptor, descriptor)));
        assert!(asm.contains("    .quad _call_entry\n"));
        assert!(asm.contains("    .quad _call_invoker\n"));
    }
}
