//! Purpose:
//! Defines the on-target word layout of callable descriptors and their metadata records.
//!
//! Called from:
//! - Compiler data emission, native callable introspection, and callable heap traversal.
//!
//! Key details:
//! - Data writers may use symbolic words. Native readers use the same fields as 64-bit words.

use std::mem::{offset_of, size_of};

use crate::{Arch, Emitter};

/// Named word fields of the native header record.
#[repr(C)]
pub struct DescriptorHeader<T> {
    /// Callable dispatch shape.
    pub kind: T,
    /// Native entry symbol.
    pub entry: T,
    /// Display-name pointer.
    pub name: T,
    /// Display-name byte count.
    pub name_len: T,
    /// Signature record pointer.
    pub signature: T,
    /// Capture and hidden-argument metadata pointer.
    pub environment: T,
    /// Invocation metadata pointer.
    pub invocation: T,
    /// Uniform boxed invocation entry.
    pub invoker: T,
}

impl<T> DescriptorHeader<T> {
    /// Emits the fields in their native word order.
    pub fn into_words(self) -> [T; 8] {
        [
            self.kind, self.entry, self.name, self.name_len,
            self.signature, self.environment, self.invocation, self.invoker,
        ]
    }
}

/// Named word fields of the native signature record.
#[repr(C)]
pub struct DescriptorSignature<T> {
    /// Number of visible source parameters.
    pub param_count: T,
    /// Number of required parameters.
    pub required_count: T,
    /// Number of regular parameters.
    pub regular_count: T,
    /// Source variadic index or its sentinel.
    pub variadic_index: T,
    /// Return representation tag.
    pub return_tag: T,
    /// Return register count.
    pub return_words: T,
    /// Return declaration flags.
    pub return_flags: T,
    /// Parameter name table pointer.
    pub param_names: T,
    /// Parameter representation table pointer.
    pub param_types: T,
    /// Default-value table pointer.
    pub defaults: T,
    /// By-reference flag table pointer.
    pub ref_flags: T,
    /// Parameter declaration flags pointer.
    pub declared_flags: T,
}

impl<T> DescriptorSignature<T> {
    /// Emits the fields in their native word order.
    pub fn into_words(self) -> [T; 12] {
        [
            self.param_count, self.required_count, self.regular_count, self.variadic_index,
            self.return_tag, self.return_words, self.return_flags, self.param_names,
            self.param_types, self.defaults, self.ref_flags, self.declared_flags,
        ]
    }
}

/// Named word fields of the native environment record.
#[repr(C)]
pub struct DescriptorEnvironment<T> {
    /// Number of runtime captures.
    pub capture_count: T,
    /// Number of hidden parameters.
    pub hidden_count: T,
    /// Capture binding metadata table.
    pub captures: T,
    /// Hidden-parameter binding metadata table.
    pub hidden: T,
}

impl<T> DescriptorEnvironment<T> {
    /// Emits the fields in their native word order.
    pub fn into_words(self) -> [T; 4] {
        [
            self.capture_count, self.hidden_count, self.captures, self.hidden,
        ]
    }
}

/// Named word fields of the native binding record.
#[repr(C)]
pub struct DescriptorBinding<T> {
    /// Binding name pointer.
    pub name: T,
    /// Binding name byte count.
    pub name_len: T,
    /// Storage representation tag.
    pub tag: T,
    /// Whether the binding owns a reference cell.
    pub by_ref: T,
}

/// One native callable-name selector row.
#[repr(C)]
pub struct NameSelectorRow<T> {
    /// Callable name pointer.
    pub name: T,
    /// Callable name byte count.
    pub name_len: T,
    /// Callable descriptor pointer.
    pub descriptor: T,
}

impl<T> NameSelectorRow<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 3] {
        [self.name, self.name_len, self.descriptor]
    }
}

/// One native receiver-bound callable selector row.
#[repr(C)]
pub struct InstanceSelectorRow<T> {
    /// Exact runtime receiver class identifier.
    pub class_id: T,
    /// Method name pointer.
    pub method_name: T,
    /// Method name byte count.
    pub method_name_len: T,
    /// Callable descriptor template pointer.
    pub template: T,
}

/// One public instance-method name row used for runtime callable lookup.
#[repr(C)]
pub struct MethodNameRow<T> {
    /// Method name pointer.
    pub name: T,
    /// Method name byte count.
    pub name_len: T,
}

/// One user-function name row used by dynamic `function_exists` lookup.
#[repr(C)]
pub struct FunctionNameRow<T> {
    /// Function name pointer.
    pub name: T,
    /// Function name byte count.
    pub name_len: T,
    /// Optional include-variant active-symbol address.
    pub active_symbol: T,
}

impl<T> FunctionNameRow<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 3] {
        [self.name, self.name_len, self.active_symbol]
    }
}

impl<T> MethodNameRow<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 2] {
        [self.name, self.name_len]
    }
}

/// One public static-method name row used for runtime callable lookup.
#[repr(C)]
pub struct StaticMethodNameRow<T> {
    /// Class name pointer.
    pub class_name: T,
    /// Class name byte count.
    pub class_name_len: T,
    /// Static method name pointer.
    pub method_name: T,
    /// Static method name byte count.
    pub method_name_len: T,
}

impl<T> StaticMethodNameRow<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 4] {
        [
            self.class_name,
            self.class_name_len,
            self.method_name,
            self.method_name_len,
        ]
    }
}

impl<T> InstanceSelectorRow<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 4] {
        [
            self.class_id,
            self.method_name,
            self.method_name_len,
            self.template,
        ]
    }
}

impl<T> DescriptorBinding<T> {
    /// Emits the fields in their native word order.
    pub fn into_words(self) -> [T; 4] {
        [
            self.name, self.name_len, self.tag, self.by_ref,
        ]
    }
}

/// Selects one word from a callable descriptor header.
#[derive(Clone, Copy)]
pub enum DescriptorField {
    /// Callable dispatch shape.
    Kind,
    /// Native entry symbol.
    Entry,
    /// Display-name pointer.
    Name,
    /// Display-name byte count.
    NameLength,
    /// Signature record pointer.
    Signature,
    /// Capture and hidden-argument metadata pointer.
    Environment,
    /// Invocation metadata pointer.
    Invocation,
    /// Uniform boxed invocation entry.
    Invoker,
}

impl DescriptorField {
    /// Returns the field displacement inside one native header.
    const fn offset(self) -> usize {
        match self {
            Self::Kind => offset_of!(DescriptorHeader<u64>, kind),
            Self::Entry => offset_of!(DescriptorHeader<u64>, entry),
            Self::Name => offset_of!(DescriptorHeader<u64>, name),
            Self::NameLength => offset_of!(DescriptorHeader<u64>, name_len),
            Self::Signature => offset_of!(DescriptorHeader<u64>, signature),
            Self::Environment => offset_of!(DescriptorHeader<u64>, environment),
            Self::Invocation => offset_of!(DescriptorHeader<u64>, invocation),
            Self::Invoker => offset_of!(DescriptorHeader<u64>, invoker),
        }
    }
}

/// Selects one word from a callable environment record.
#[derive(Clone, Copy)]
pub enum EnvironmentField {
    /// Number of runtime captures.
    CaptureCount,
    /// Number of hidden parameters.
    HiddenCount,
    /// Capture binding metadata table.
    Captures,
    /// Hidden-parameter binding metadata table.
    Hidden,
}

impl EnvironmentField {
    /// Returns the field displacement inside one native environment record.
    const fn offset(self) -> usize {
        match self {
            Self::CaptureCount => offset_of!(DescriptorEnvironment<u64>, capture_count),
            Self::HiddenCount => offset_of!(DescriptorEnvironment<u64>, hidden_count),
            Self::Captures => offset_of!(DescriptorEnvironment<u64>, captures),
            Self::Hidden => offset_of!(DescriptorEnvironment<u64>, hidden),
        }
    }
}

/// Selects one word from a callable binding record.
#[derive(Clone, Copy)]
pub enum BindingField {
    /// Binding name pointer.
    Name,
    /// Binding name byte count.
    NameLength,
    /// Storage representation tag.
    Tag,
    /// Whether the binding owns a reference cell.
    ByRef,
}

impl BindingField {
    /// Returns the field displacement inside one native binding record.
    const fn offset(self) -> usize {
        match self {
            Self::Name => offset_of!(DescriptorBinding<u64>, name),
            Self::NameLength => offset_of!(DescriptorBinding<u64>, name_len),
            Self::Tag => offset_of!(DescriptorBinding<u64>, tag),
            Self::ByRef => offset_of!(DescriptorBinding<u64>, by_ref),
        }
    }
}

/// Selects one word from a callable-name selector row.
#[derive(Clone, Copy)]
pub enum NameSelectorField {
    /// Callable name pointer.
    Name,
    /// Callable name byte count.
    NameLength,
    /// Callable descriptor pointer.
    Descriptor,
}

impl NameSelectorField {
    /// Returns the field displacement inside one native row.
    const fn offset(self) -> usize {
        match self {
            Self::Name => offset_of!(NameSelectorRow<u64>, name),
            Self::NameLength => offset_of!(NameSelectorRow<u64>, name_len),
            Self::Descriptor => offset_of!(NameSelectorRow<u64>, descriptor),
        }
    }
}

/// Selects one word from a receiver-bound callable selector row.
#[derive(Clone, Copy)]
pub enum InstanceSelectorField {
    /// Exact runtime receiver class identifier.
    ClassId,
    /// Method name pointer.
    MethodName,
    /// Method name byte count.
    MethodNameLength,
    /// Callable descriptor template pointer.
    Template,
}

/// Selects one word from a public instance-method name row.
#[derive(Clone, Copy)]
pub enum MethodNameField {
    /// Method name pointer.
    Name,
    /// Method name byte count.
    NameLength,
}

/// Selects one word from a dynamic user-function name row.
#[derive(Clone, Copy)]
pub enum FunctionNameField {
    /// Function name pointer.
    Name,
    /// Function name byte count.
    NameLength,
    /// Optional include-variant active-symbol address.
    ActiveSymbol,
}

impl FunctionNameField {
    /// Returns the field displacement inside one native row.
    const fn offset(self) -> usize {
        match self {
            Self::Name => offset_of!(FunctionNameRow<u64>, name),
            Self::NameLength => offset_of!(FunctionNameRow<u64>, name_len),
            Self::ActiveSymbol => offset_of!(FunctionNameRow<u64>, active_symbol),
        }
    }
}

impl MethodNameField {
    /// Returns the field displacement inside one native row.
    const fn offset(self) -> usize {
        match self {
            Self::Name => offset_of!(MethodNameRow<u64>, name),
            Self::NameLength => offset_of!(MethodNameRow<u64>, name_len),
        }
    }
}

/// Selects one word from a public static-method name row.
#[derive(Clone, Copy)]
pub enum StaticMethodNameField {
    /// Class name pointer.
    ClassName,
    /// Class name byte count.
    ClassNameLength,
    /// Static method name pointer.
    MethodName,
    /// Static method name byte count.
    MethodNameLength,
}

impl StaticMethodNameField {
    /// Returns the field displacement inside one native row.
    const fn offset(self) -> usize {
        match self {
            Self::ClassName => offset_of!(StaticMethodNameRow<u64>, class_name),
            Self::ClassNameLength => offset_of!(StaticMethodNameRow<u64>, class_name_len),
            Self::MethodName => offset_of!(StaticMethodNameRow<u64>, method_name),
            Self::MethodNameLength => offset_of!(StaticMethodNameRow<u64>, method_name_len),
        }
    }
}

impl InstanceSelectorField {
    /// Returns the field displacement inside one native row.
    const fn offset(self) -> usize {
        match self {
            Self::ClassId => offset_of!(InstanceSelectorRow<u64>, class_id),
            Self::MethodName => offset_of!(InstanceSelectorRow<u64>, method_name),
            Self::MethodNameLength => offset_of!(InstanceSelectorRow<u64>, method_name_len),
            Self::Template => offset_of!(InstanceSelectorRow<u64>, template),
        }
    }
}

/// Byte offset of the native entry word.
pub const ENTRY_OFFSET: usize = offset_of!(DescriptorHeader<u64>, entry);
/// Byte offset of the native kind word.
pub const KIND_OFFSET: usize = offset_of!(DescriptorHeader<u64>, kind);
/// Byte offset of the native name word.
pub const NAME_OFFSET: usize = offset_of!(DescriptorHeader<u64>, name);
/// Byte offset of the native name len word.
pub const NAME_LENGTH_OFFSET: usize = offset_of!(DescriptorHeader<u64>, name_len);
/// Byte offset of the native signature word.
pub const SIGNATURE_OFFSET: usize = offset_of!(DescriptorHeader<u64>, signature);
/// Byte offset of the native environment word.
pub const ENVIRONMENT_OFFSET: usize = offset_of!(DescriptorHeader<u64>, environment);
/// Byte offset of the native invoker word.
pub const INVOKER_OFFSET: usize = offset_of!(DescriptorHeader<u64>, invoker);
/// Byte offset of the native param count word.
pub const SIGNATURE_COUNT_OFFSET: usize = offset_of!(DescriptorSignature<u64>, param_count);
/// Byte offset of the native regular count word.
pub const SIGNATURE_REGULAR_OFFSET: usize = offset_of!(DescriptorSignature<u64>, regular_count);
/// Byte offset of the native variadic index word.
pub const SIGNATURE_VARIADIC_OFFSET: usize = offset_of!(DescriptorSignature<u64>, variadic_index);
/// Byte offset of the native param names word.
pub const SIGNATURE_NAMES_OFFSET: usize = offset_of!(DescriptorSignature<u64>, param_names);
/// Byte offset of the native param types word.
pub const SIGNATURE_TYPES_OFFSET: usize = offset_of!(DescriptorSignature<u64>, param_types);
/// Byte offset of the native ref flags word.
pub const SIGNATURE_REF_FLAGS_OFFSET: usize = offset_of!(DescriptorSignature<u64>, ref_flags);
/// Byte offset of the native captures word.
pub const CAPTURE_TABLE_OFFSET: usize = offset_of!(DescriptorEnvironment<u64>, captures);
/// Byte offset of the native capture count word.
pub const CAPTURE_COUNT_OFFSET: usize = offset_of!(DescriptorEnvironment<u64>, capture_count);
// Collector scans use the first word directly and preserve their instruction comments.
const _: () = assert!(CAPTURE_COUNT_OFFSET == 0);
/// Byte offset of the native tag word.
pub const BINDING_TAG_OFFSET: usize = offset_of!(DescriptorBinding<u64>, tag);
/// Byte offset of the native by ref word.
pub const BINDING_REF_OFFSET: usize = offset_of!(DescriptorBinding<u64>, by_ref);

/// Fixed bytes in a native callable descriptor before its captures.
pub const HEADER_BYTES: usize = size_of::<DescriptorHeader<u64>>();
/// Byte offset of the first runtime capture following the fixed descriptor header.
pub const RUNTIME_CAPTURE_OFFSET: usize = HEADER_BYTES;
/// Words in a parameter-name row.
pub const PARAM_NAME_WORDS: usize = 2;
/// Words in a parameter type, size and register-count row.
pub const PARAM_TYPE_WORDS: usize = 3;
/// Words consumed by one capture or hidden-parameter binding.
pub const BINDING_WORDS: usize = BINDING_BYTES / size_of::<u64>();
/// Bytes in the native binding row.
pub const BINDING_BYTES: usize = size_of::<DescriptorBinding<u64>>();
/// Bytes in one callable-name selector row.
pub const NAME_SELECTOR_ROW_BYTES: usize = size_of::<NameSelectorRow<u64>>();
/// Bytes in one receiver-bound callable selector row.
pub const INSTANCE_SELECTOR_ROW_BYTES: usize = size_of::<InstanceSelectorRow<u64>>();
/// Bytes occupied by the leading row-count word of an instance-method name table.
pub const METHOD_NAME_COUNT_BYTES: usize = size_of::<u64>();
/// Bytes in one public instance-method name row.
pub const METHOD_NAME_ROW_BYTES: usize = size_of::<MethodNameRow<u64>>();
/// Bytes in one public static-method name row.
pub const STATIC_METHOD_NAME_ROW_BYTES: usize = size_of::<StaticMethodNameRow<u64>>();
/// Bytes in one dynamic user-function name row.
pub const FUNCTION_NAME_ROW_BYTES: usize = size_of::<FunctionNameRow<u64>>();
/// Shift from logical binding index to its byte displacement.
pub const BINDING_SHIFT: u32 = BINDING_BYTES.trailing_zeros();
/// Signature marker for a nonvariadic callable.
pub const NO_VARIADIC_PARAMETER: u64 = u64::MAX;

const _: () = assert!(NAME_SELECTOR_ROW_BYTES == 24);
const _: () = assert!(INSTANCE_SELECTOR_ROW_BYTES == 32);
const _: () = assert!(METHOD_NAME_COUNT_BYTES == 8);
const _: () = assert!(METHOD_NAME_ROW_BYTES == 16);
const _: () = assert!(STATIC_METHOD_NAME_ROW_BYTES == 32);
const _: () = assert!(FUNCTION_NAME_ROW_BYTES == 24);

/// Loads the dispatch kind from an already validated callable descriptor.
pub fn emit_load_kind(
    emitter: &mut impl crate::TargetAbi,
    destination: &str,
    descriptor: &str,
) {
    emit_load_descriptor_field(emitter, destination, descriptor, DescriptorField::Kind);
}

/// Loads one typed word from a callable descriptor header.
pub fn emit_load_descriptor_field(
    emitter: &mut impl crate::TargetAbi,
    destination: &str,
    descriptor: &str,
    field: DescriptorField,
) {
    emitter.emit_load_from_address(destination, descriptor, field.offset());
}

/// Loads one typed word from a callable environment record.
pub fn emit_load_environment_field(
    emitter: &mut impl crate::TargetAbi,
    destination: &str,
    environment: &str,
    field: EnvironmentField,
) {
    emitter.emit_load_from_address(destination, environment, field.offset());
}

/// Forms the address of one callable binding row.
pub fn emit_binding_row_address(
    emitter: &mut impl Emitter,
    destination: &str,
    table: &str,
    index: &str,
    scratch: &str,
) {
    emit_indexed_row_address(
        emitter,
        destination,
        table,
        index,
        scratch,
        0,
        BINDING_BYTES,
    );
}

/// Loads one typed word from a callable binding row.
pub fn emit_load_binding_field(
    emitter: &mut impl crate::TargetAbi,
    destination: &str,
    binding: &str,
    field: BindingField,
) {
    emitter.emit_load_from_address(destination, binding, field.offset());
}

/// Loads one typed word from a callable-name selector row.
pub fn emit_load_name_selector_field(
    emitter: &mut impl crate::TargetAbi,
    destination: &str,
    row: &str,
    field: NameSelectorField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Loads one typed word from a receiver-bound callable selector row.
pub fn emit_load_instance_selector_field(
    emitter: &mut impl crate::TargetAbi,
    destination: &str,
    row: &str,
    field: InstanceSelectorField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Loads the leading row count from a public instance-method name table.
pub fn emit_load_method_name_count(
    emitter: &mut impl crate::TargetAbi,
    destination: &str,
    table: &str,
) {
    emitter.emit_load_from_address(destination, table, 0);
}

/// Forms the address of one public instance-method name row after its count word.
pub fn emit_method_name_row_address(
    emitter: &mut impl Emitter,
    destination: &str,
    table: &str,
    index: &str,
    scratch: &str,
) {
    emit_indexed_row_address(
        emitter,
        destination,
        table,
        index,
        scratch,
        METHOD_NAME_COUNT_BYTES,
        METHOD_NAME_ROW_BYTES,
    );
}

/// Loads one typed word from a public instance-method name row.
pub fn emit_load_method_name_field(
    emitter: &mut impl crate::TargetAbi,
    destination: &str,
    row: &str,
    field: MethodNameField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Forms the address of one public static-method name row.
pub fn emit_static_method_name_row_address(
    emitter: &mut impl Emitter,
    destination: &str,
    table: &str,
    index: &str,
    scratch: &str,
) {
    emit_indexed_row_address(
        emitter,
        destination,
        table,
        index,
        scratch,
        0,
        STATIC_METHOD_NAME_ROW_BYTES,
    );
}

/// Loads one typed word from a public static-method name row.
pub fn emit_load_static_method_name_field(
    emitter: &mut impl crate::TargetAbi,
    destination: &str,
    row: &str,
    field: StaticMethodNameField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Forms the address of one dynamic user-function name row.
pub fn emit_function_name_row_address(
    emitter: &mut impl Emitter,
    destination: &str,
    table: &str,
    index: &str,
    scratch: &str,
) {
    emit_indexed_row_address(
        emitter,
        destination,
        table,
        index,
        scratch,
        0,
        FUNCTION_NAME_ROW_BYTES,
    );
}

/// Loads one typed word from a dynamic user-function name row.
pub fn emit_load_function_name_field(
    emitter: &mut impl crate::TargetAbi,
    destination: &str,
    row: &str,
    field: FunctionNameField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Forms an address inside a table of uniform native rows.
fn emit_indexed_row_address(
    emitter: &mut impl Emitter,
    destination: &str,
    table: &str,
    index: &str,
    scratch: &str,
    prefix_bytes: usize,
    row_bytes: usize,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.emit_load_int_immediate(scratch, row_bytes as i64);
            emitter.instruction(&format!("mul {scratch}, {index}, {scratch}"));
            emitter.instruction(&format!("add {destination}, {table}, {scratch}"));
            if prefix_bytes != 0 {
                emitter.instruction(&format!("add {destination}, {destination}, #{prefix_bytes}"));
            }
        }
        Arch::X86_64 => {
            let offset = if destination == table { scratch } else { destination };
            emitter.emit_reg_move(offset, index);
            emitter.instruction(&format!("imul {offset}, {offset}, {row_bytes}"));
            if destination == table {
                emitter.instruction(&format!("add {destination}, {offset}"));
            } else {
                emitter.instruction(&format!("add {destination}, {table}"));
            }
            if prefix_bytes != 0 {
                emitter.instruction(&format!("add {destination}, {prefix_bytes}"));
            }
        }
    }
}
