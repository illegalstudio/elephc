//! Native storage names for compiled physical sources.
//!
//! Used by inclusion lowering and web reset. Physical path bytes, not module-local
//! ID numbers or lossy path hashes, disambiguate symbols across compiled modules.

use std::fmt::Write;
use std::path::Path;
mod bridge;
mod closed_include;
pub(super) use bridge::{deferred_class_symbol, emit_state_helpers, emit_state_install, PRIMED};
pub(in crate::codegen) use bridge::{emit_lookup, OnMatch};
pub(in crate::codegen) use closed_include::{CLOSED_INCLUDE, FLAG_DISCARDED, FLAG_ONCE, FLAG_REQUIRED};
pub(in crate::codegen) use bridge::{
    INCLUDE_UNIT_CANDIDATE, INCLUDE_UNIT_FUNCTION_OFFSET, INCLUDE_UNIT_GUARD_OFFSET,
    INCLUDE_UNIT_LOOKUP,
};

/// Encodes the exact canonical path into an assembly-safe, collision-free symbol.
/// Identity lookup still uses SourceId/catalog metadata, never decoding this name.
pub(super) fn include_guard_symbol(path: &Path) -> String {
    let mut symbol = String::from("_include_once_");
    for byte in path.as_os_str().as_encoded_bytes() {
        let _ = write!(symbol, "{byte:02x}");
    }
    symbol
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_guard_symbols_encode_bytes_without_separator_collisions() {
        assert_eq!(include_guard_symbol(Path::new("/a")), "_include_once_2f61");
        assert_ne!(include_guard_symbol(Path::new("/a-b")), include_guard_symbol(Path::new("/a_b")));
    }

    #[cfg(unix)]
    #[test]
    fn source_guard_symbols_distinguish_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        let a = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/a\xff".to_vec()));
        let b = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/a\xfe".to_vec()));
        assert_eq!(a.to_string_lossy(), b.to_string_lossy());
        assert_ne!(include_guard_symbol(&a), include_guard_symbol(&b));
    }
}
