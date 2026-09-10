//! Purpose:
//! Defines the opaque Oniguruma provider shared by native packaging and the mbstring engine.
//!
//! Called from:
//! - Native provider registration, regex compilation, matching, and capture extraction.
//!
//! Key details:
//! - Callback code has process lifetime, never calls PHP, and must not unwind through Rust.
//! - Handles use paired provider frees; names borrow regex storage until explicitly copied.

use std::ffi::c_void;

/// Reviewed Oniguruma version shared by the native package catalog and PHP's version constant.
pub const ONIGURUMA_VERSION: &str = "6.9.10";

/// Readable zero bytes required after a subject when a caller supplies its own native lookahead padding.
pub const SUBJECT_PADDING: usize = 32;
/// The request carries stack and retry limits in bits zero/one and initialized subject padding in bit two.
pub const SUBJECT_PADDED: u32 = 4;

/// Borrows pattern bytes with stable encoding/syntax IDs and the six mbregex option bits.
#[repr(C)]
pub struct MbRegexCompileV1 {
    pub pattern: *const u8,
    pub length: u64,
    pub encoding: u32,
    pub options: u32,
    pub syntax: u32,
    pub reserved: u32,
}

/// Borrows a complete subject and selects a byte offset, matching mode, and per-call limits.
#[repr(C)]
pub struct MbRegexSearchV1 {
    pub subject: *const u8,
    pub length: u64,
    pub offset: u64,
    pub anchored: u32,
    pub flags: u32,
    pub stack_limit: u64,
    pub retry_limit: u64,
}

/// Borrows one named capture and reports its last participating duplicate group number.
#[repr(C)]
pub struct MbRegexNameV1 {
    pub bytes: *const u8,
    pub length: u64,
    pub group: i64,
}

/// Initializes the pinned library once, returning zero on success.
pub type Initialize = unsafe extern "C" fn() -> i32;
/// Compiles one pattern, publishing a handle only on success and copying any compile diagnostic.
pub type Compile = unsafe extern "C" fn(*const MbRegexCompileV1, *mut *mut c_void, *mut u8, u64) -> i32;
/// Frees one handle allocated by the corresponding provider operation; null is accepted.
pub type Free = unsafe extern "C" fn(*mut c_void);
/// Returns match length in anchored mode or match position in search mode, -1 for no match, or an error.
pub type Search = unsafe extern "C" fn(*mut c_void, *const MbRegexSearchV1, *mut *mut c_void) -> i64;
/// Returns the number of numeric captures in a region, or named entries in a compiled regex.
pub type Count = unsafe extern "C" fn(*mut c_void) -> u64;
/// Reads a numeric capture's byte bounds; unmatched groups have (-1, -1).
pub type RegionGet = unsafe extern "C" fn(*mut c_void, u64, *mut i64, *mut i64) -> i32;
/// Reads a named capture in native enumeration order, borrowing its name from the regex owner.
pub type NameGet = unsafe extern "C" fn(*mut c_void, *mut c_void, u64, *mut MbRegexNameV1) -> i32;
/// Resolves a replacement backreference name against the current matched duplicate groups.
pub type NameLookup = unsafe extern "C" fn(*mut c_void, *mut c_void, *const u8, u64) -> i32;
/// Reports whether PHP may expand numbered replacement backreferences for this syntax and pattern.
pub type NumberedBackrefs = unsafe extern "C" fn(*mut c_void) -> i32;
/// Returns the compiled option mask after native syntax defaults, or UINT32_MAX for an invalid handle.
pub type CompiledOptions = unsafe extern "C" fn(*mut c_void) -> u32;
/// Copies one native error as NUL-terminated bytes, returning its length or a negative failure.
pub type Error = unsafe extern "C" fn(i32, *mut u8, u64) -> i32;

/// Immutable, complete function table whose allocation and release functions must stay paired.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MbRegexProviderV1 {
    pub version: u32,
    pub size: u32,
    pub initialize: Option<Initialize>,
    pub compile: Option<Compile>,
    pub free_regex: Option<Free>,
    pub search: Option<Search>,
    pub free_region: Option<Free>,
    pub region_count: Option<Count>,
    pub region_get: Option<RegionGet>,
    pub name_count: Option<Count>,
    pub name_get: Option<NameGet>,
    pub name_lookup: Option<NameLookup>,
    pub numbered_backrefs: Option<NumberedBackrefs>,
    pub error: Option<Error>,
    pub compiled_options: Option<CompiledOptions>,
}

/// Rejects partial providers before any native callback can be invoked.
impl MbRegexProviderV1 {
    /// Checks the complete version-one shape without calling external code.
    pub fn is_complete(&self) -> bool {
        self.version == 1 && self.size as usize == std::mem::size_of::<Self>()
            && self.initialize.is_some() && self.compile.is_some() && self.free_regex.is_some()
            && self.search.is_some() && self.free_region.is_some() && self.region_count.is_some()
            && self.region_get.is_some() && self.name_count.is_some() && self.name_get.is_some()
            && self.name_lookup.is_some() && self.numbered_backrefs.is_some() && self.error.is_some()
            && self.compiled_options.is_some()
    }
}
