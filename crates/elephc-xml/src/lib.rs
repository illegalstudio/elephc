//! Purpose:
//! The `elephc_xml` bridge: PHP's `ext/xml` SAX parser and `ext/xmlwriter` writer as thin
//! FFI layers over libxml2 2.15.3 (the C library from the native catalog's `libxml2`
//! package) behind a panic-free C ABI, linked into compiled programs that use the
//! `xml_*` / `xmlwriter_*` surface.
//!
//! Called from:
//! - Compiled PHP programs through the `elephc_xml_*` entry points (`crate::abi`), declared
//!   by the compiler's xml prelude as an `extern "elephc_xml"` block.
//! - `cargo test -p elephc-xml` for the engine unit tests (the libxml2-calling ones need
//!   `ELEPHC_XML_LIBXML2_LIB_DIR`, see `build.rs`).
//!
//! Key details:
//! - `parser` drives libxml2's push parser through the Elephc-owned C shim
//!   (`src/native_deps/recipes/libxml2_shim.c`) and reproduces php-src's `ext/xml/compat.c`
//!   SAX handler set: event shapes, positions, entity routing and the numeric
//!   `xmlParserErrors` codes PHP reports verbatim.
//! - `writer` is `xmlTextWriter` over an in-memory `xmlOutputBufferCreateIO` sink, exactly
//!   how `ext/xmlwriter` builds its memory writers.
//! - `ffi` holds every `extern "C"` declaration; no libxml2 struct layout is declared in
//!   Rust, the shim owns all internal field access.
//! - Nothing here touches PHP values: handles are integers, strings cross as bytes.

pub mod abi;
mod ffi;
pub mod parser;
pub mod writer;

#[cfg(all(test, elephc_xml_native))]
mod native_tests {
    //! Purpose:
    //! Sanity checks that the linked native archives are the pinned libxml2 release.
    //!
    //! Called from:
    //! - `cargo test -p elephc-xml` with `ELEPHC_XML_LIBXML2_LIB_DIR` set.
    //!
    //! Key details:
    //! - `LIBXML_VERSION` is `MAJOR * 10000 + MINOR * 100 + MICRO`, so 2.15.3 is 21503.

    /// The shim was compiled against, and the archive is, libxml2 2.15.3.
    #[test]
    fn libxml2_version_matches_the_pinned_catalog_release() {
        assert_eq!(unsafe { crate::ffi::elephc_libxml2_v1_version() }, 21503);
    }
}

#[cfg(all(test, not(elephc_xml_native)))]
mod skipped_tests {
    //! Purpose:
    //! The single test that runs when no native libxml2 artifact is configured, so a
    //! plain `cargo test -p elephc-xml` reports why the engine corpora did not run.
    //!
    //! Called from:
    //! - `cargo test -p elephc-xml` without `ELEPHC_XML_LIBXML2_LIB_DIR`.
    //!
    //! Key details:
    //! - Mirrors `crates/elephc-curl`'s skip path.

    /// Prints the skip instruction instead of silently passing an empty suite.
    #[test]
    fn native_libxml2_tests_are_skipped() {
        eprintln!(
            "SKIP: ELEPHC_XML_LIBXML2_LIB_DIR is not set; run elephc native add libxml2 and point it at the artifact's lib/ directory"
        );
    }
}
