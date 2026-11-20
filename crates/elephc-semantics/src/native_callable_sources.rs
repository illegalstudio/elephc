//! Purpose:
//! Defines the versioned logical source side record of native callable signatures.
//!
//! Called from:
//! - Compiler descriptor metadata emission and Magician descriptor decoding.
//!
//! Key details:
//! - Existing physical signature offsets remain unchanged.
//! - Legacy records without the presence flag have no logical source evidence.

/// Signature flags word in the existing fixed header.
pub const FLAGS_WORD: usize = 6;
/// Presence of the extended logical declaration record.
pub const SOURCE_FLAG: u64 = 1 << 1;
/// Appended source record pointer, after every legacy signature word.
pub const SOURCE_RECORD_WORD: usize = 12;
/// Exact neutral Type JSON record version supported by both producers and consumers.
pub const SOURCE_VERSION: u64 = 1;
/// Bound for one trusted static declaration payload.
pub const MAX_SOURCE_BYTES: usize = 65536;

/// Trusted generated signature spelling containing JSON of the shared structural Type vocabulary.
pub const TYPE_SPEC_PREFIX: &str = "@logical-type:";
