//! Purpose:
//! The `elephc_tidy` bridge: PHP's `ext/tidy` document and node API as a thin
//! FFI layer over libtidy 5.8.0, linked into compiled programs that use the
//! `tidy_*` / `tidy` / `tidyNode` surface.
//!
//! Called from:
//! - Compiled PHP programs through the `extern "elephc_tidy"` block declared by
//!   `elephc::tidy_prelude`.
//! - `cargo test -p elephc-tidy` for native tests when
//!   `ELEPHC_TIDY_LIBTIDY_LIB_DIR` points at the managed artifact's `lib/`.
//!
//! Key details:
//! - `document`, `option`, and `node` expose the stable C ABI; `registry` owns
//!   handles and document reference counts; `ffi` is the only place that knows
//!   libtidy's symbols and public buffer layout.
//! - Nothing here touches PHP values: handles are integers, strings cross as
//!   bytes, and every entry is panic-contained.
//! - The staticlib is built with unresolved libtidy symbols. `build.rs` only
//!   links the real archive for this crate's own tests.

mod abi;
mod document;
mod ffi;
mod node;
mod option;
mod registry;

#[cfg(all(test, elephc_tidy_native))]
mod native_tests {
    //! Purpose:
    //! Native smoke tests against the pinned libtidy archive.
    //!
    //! Called from:
    //! - `cargo test -p elephc-tidy` with `ELEPHC_TIDY_LIBTIDY_LIB_DIR` set.
    //!
    //! Key details:
    //! - Exercises the public ABI only; no libtidy FFI type escapes the crate.

    use std::ffi::CStr;

    /// Reads a returned C string.
    fn text(pointer: *const std::ffi::c_char) -> String {
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }

    /// The linked archive reports the pinned 5.8.0 release date.
    #[test]
    fn release_date_matches_the_pinned_catalog_release() {
        assert_eq!(text(crate::document::elephc_tidy_release_date()), "2021/07/10");
    }

    /// A parsed document serializes repaired HTML and exposes its body node.
    #[test]
    fn parse_output_and_body_node_work() {
        let doc = crate::document::elephc_tidy_create();
        assert!(doc > 0);
        let source = b"<HTML><BODY BGCOLOR=#FFFFFF ALINK=#000000></BODY></HTML>";
        assert_eq!(
            unsafe { crate::document::elephc_tidy_parse(doc, source.as_ptr(), source.len() as i64) },
            1
        );
        assert_eq!(crate::document::elephc_tidy_clean_repair(doc), 1);
        let output = text(crate::document::elephc_tidy_output(doc));
        assert!(output.contains("<body bgcolor=\"#FFFFFF\" alink=\"#000000\">"), "{output}");
        let body = crate::node::elephc_tidy_body(doc);
        assert!(body > 0);
        assert_eq!(text(crate::node::elephc_tidy_node_name(body)), "body");
        assert_eq!(crate::node::elephc_tidy_node_attr_count(body), 2);
        assert_eq!(text(crate::node::elephc_tidy_node_attr_name(body, 0)), "bgcolor");
        assert_eq!(text(crate::node::elephc_tidy_node_attr_value(body, 0)), "#FFFFFF");
        assert_eq!(crate::node::elephc_tidy_node_release(body), 1);
        assert_eq!(crate::document::elephc_tidy_release(doc), 1);
    }

    /// Clean repair frees pruned nodes, so wrappers from the previous tree must become
    /// absent handles rather than dereferencing the released `TidyNode*`.
    #[test]
    fn clean_repair_invalidates_previous_nodes() {
        let doc = crate::document::elephc_tidy_create();
        assert!(doc > 0);
        let source = b"<html><body><p>x</p></body></html>";
        assert_eq!(
            unsafe { crate::document::elephc_tidy_parse(doc, source.as_ptr(), source.len() as i64) },
            1
        );
        let body = crate::node::elephc_tidy_body(doc);
        assert!(body > 0);
        let child = crate::node::elephc_tidy_node_child(body, 0);
        assert!(child > 0);
        assert_eq!(crate::document::elephc_tidy_clean_repair(doc), 1);
        assert_eq!(crate::node::elephc_tidy_node_has_children(body), 0);
        assert_eq!(crate::node::elephc_tidy_node_is_text(child), 0);
        assert_eq!(crate::document::elephc_tidy_release(doc), 1);
    }

    /// Configuration enumeration includes `tidy-mark` and reports its forced
    /// false value.
    #[test]
    fn configuration_enumeration_exposes_tidy_mark() {
        let doc = crate::document::elephc_tidy_create();
        assert!(doc > 0);
        let count = crate::option::elephc_tidy_option_count(doc);
        assert!(count > 50, "unexpected option count {count}");
        let index = unsafe { crate::option::elephc_tidy_option_index(doc, c"tidy-mark".as_ptr()) };
        assert!(index >= 0);
        assert_eq!(crate::option::elephc_tidy_option_type(doc, index), 2);
        assert_eq!(crate::option::elephc_tidy_option_value_bool(doc, index), 0);
        // `TidyForceOutput` and `TidyMark` are the two raw enum ids the registry
        // applies at document creation; pin both through libtidy's own table.
        let force = unsafe { crate::option::elephc_tidy_option_index(doc, c"force-output".as_ptr()) };
        assert!(force >= 0);
        assert_eq!(crate::option::elephc_tidy_option_type(doc, force), 2);
        assert_eq!(crate::option::elephc_tidy_option_value_bool(doc, force), 1);
        assert_eq!(crate::document::elephc_tidy_release(doc), 1);
    }
}

#[cfg(all(test, not(elephc_tidy_native)))]
mod skipped_tests {
    //! Purpose:
    //! The single test that runs when no native libtidy artifact is configured,
    //! so a plain `cargo test -p elephc-tidy` reports why native tests did not run.
    //!
    //! Called from:
    //! - `cargo test -p elephc-tidy` without `ELEPHC_TIDY_LIBTIDY_LIB_DIR`.
    //!
    //! Key details:
    //! - Mirrors the xml and fileinfo bridge skip paths.

    /// Prints the skip instruction instead of silently passing an empty suite.
    #[test]
    fn native_libtidy_tests_are_skipped() {
        eprintln!(
            "SKIP: ELEPHC_TIDY_LIBTIDY_LIB_DIR is not set; run elephc native add libtidy and point it at the artifact's lib/ directory"
        );
    }
}
