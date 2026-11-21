//! Purpose:
//! Defines the on-target word layout of callable descriptors and their metadata records.
//!
//! Called from:
//! - Compiler data emission, native callable introspection, and callable heap traversal.
//!
//! Key details:
//! - Data writers may use symbolic words. Native readers use the same fields as 64-bit words.

use std::mem::{offset_of, size_of};

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

impl<T> DescriptorBinding<T> {
    /// Emits the fields in their native word order.
    pub fn into_words(self) -> [T; 4] {
        [
            self.name, self.name_len, self.tag, self.by_ref,
        ]
    }
}

/// Byte offset of the native entry word.
pub const ENTRY_OFFSET: usize = offset_of!(DescriptorHeader<u64>, entry);
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
/// Words in a parameter-name row.
pub const PARAM_NAME_WORDS: usize = 2;
/// Words in a parameter type, size and register-count row.
pub const PARAM_TYPE_WORDS: usize = 3;
/// Words consumed by one capture or hidden-parameter binding.
pub const BINDING_WORDS: usize = BINDING_BYTES / size_of::<u64>();
/// Bytes in the native binding row.
pub const BINDING_BYTES: usize = size_of::<DescriptorBinding<u64>>();
/// Shift from logical binding index to its byte displacement.
pub const BINDING_SHIFT: u32 = BINDING_BYTES.trailing_zeros();
/// Signature marker for a nonvariadic callable.
pub const NO_VARIADIC_PARAMETER: u64 = u64::MAX;
