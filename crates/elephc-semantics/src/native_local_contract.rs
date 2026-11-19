//! Purpose:
//! Shares versioned native local-origin metadata layouts across compiler and eval boundaries.
//!
//! Called from:
//! - Generated local descriptors and the no-PHP scope contract-copy ABI.
//!
//! Key details:
//! - Records borrow immutable UTF-8 ranges and contain no runtime value owners.
//! - Local descriptors preserve the existing four-word declaration callback prefix.

pub const ABI_VERSION: usize = 1;

/// Immutable generated local descriptor following the shared declaration prefix.
#[repr(C)]
pub struct LocalTypeDescriptorV1 {
    pub callback: usize,
    pub class_id: usize,
    pub property_slot: usize,
    pub is_static: usize,
    pub version: usize,
    pub type_ptr: *const u8,
    pub type_len: usize,
}

/// Borrowed source/target names and the selected immutable producer descriptor.
#[repr(C)]
pub struct BindingCopyV1 {
    pub version: usize,
    pub source_ptr: *const u8,
    pub source_len: usize,
    pub target_ptr: *const u8,
    pub target_len: usize,
    pub descriptor: *const LocalTypeDescriptorV1,
}

/// Reserved generated type atom, distinct from PHP class names and source type hints.
pub const RESOURCE_TYPE_SPEC: &str = "@resource";

#[cfg(test)]
mod tests {
    use super::*;

    /// Every supported target uses the same eight-byte words and four-word callback prefix.
    #[test]
    fn native_local_origin_metadata_layout_preserves_callback_prefix() {
        assert_eq!(std::mem::size_of::<LocalTypeDescriptorV1>(), 56);
        assert_eq!(std::mem::offset_of!(LocalTypeDescriptorV1, version), 32);
        assert_eq!(std::mem::offset_of!(LocalTypeDescriptorV1, type_ptr), 40);
        assert_eq!(std::mem::offset_of!(LocalTypeDescriptorV1, type_len), 48);
        assert_eq!(std::mem::size_of::<BindingCopyV1>(), 48);
        assert_eq!(std::mem::offset_of!(BindingCopyV1, descriptor), 40);
    }
}

/// Reserved generated prefix for an array-content constraint that permits other value kinds.
pub const ARRAY_CONTENTS_TYPE_SPEC_PREFIX: &str = "@array-contents:";

/// Internal generic term used after decoding an array-content source constraint.
pub const ARRAY_CONTENTS_TYPE_NAME: &str = "@array-contents";
