//! Purpose:
//! Defines versioned read-only views of AOT callable metadata exposed to hosted eval.
//!
//! Called from:
//! - Compiler-generated projection code and Magician's runtime adapter.
//!
//! Key details:
//! - Pointers are borrowed while the originating callable remains live.
//! - The opaque parameter table is consumed only by the matching native accessor.

use std::ffi::c_void;

/// Sentinel for a signature without a visible variadic parameter.
pub const NO_VARIADIC_PARAMETER: u64 = u64::MAX;

/// Visible callable metadata returned by the V1 native signature accessor.
#[derive(Default)]
#[repr(C)]
pub struct CallableSignatureViewV1 {
    /// Opaque origin used by the V1 parameter accessor.
    pub parameters: *const c_void,
    /// Borrowed display-name bytes.
    pub name: *const u8,
    /// Display-name byte count.
    pub name_length: usize,
    /// Number of visible parameters.
    pub parameter_count: usize,
    /// Number of regular parameters.
    pub regular_count: usize,
    /// Visible variadic parameter index or the sentinel.
    pub variadic: u64,
}

/// One visible parameter returned by the V1 native parameter accessor.
#[derive(Default)]
#[repr(C)]
pub struct CallableParameterViewV1 {
    /// Borrowed parameter-name bytes.
    pub name: *const u8,
    /// Parameter-name byte count.
    pub name_length: usize,
    /// Native representation tag.
    pub type_tag: u64,
    /// Nonzero when the parameter takes a reference.
    pub by_ref: u64,
}
