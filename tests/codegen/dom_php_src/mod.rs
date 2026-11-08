//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests rewritten as Elephc codegen tests, one module per
//! php-src folder (the flat legacy folder is split by family).
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - Every test runs from its php-src folder inside a mirrored copy of
//!   `tests/fixtures/dom_php_src/`, so `__DIR__`-relative fixture paths resolve as under
//!   php-src; see `php_src_corpus.rs`.
//! - The tests link the managed libxml2 artifact and skip through
//!   `skip_without_xml_native`; CI sets `ELEPHC_TEST_REQUIRE_XML_NATIVE=1` to turn that
//!   skip into a provisioning failure.

pub(super) use super::php_src_corpus::{assert_exact, assert_fatal_format, assert_format};
pub(super) use crate::support::skip_without_xml_native;

/// Root of the php-src DOM fixture assets.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/dom_php_src");

/// Runs one DOM test program from `folder` with `env` and returns its merged output bytes.
pub(super) fn run_case(folder: &str, env: &[(&str, &str)], source: &str) -> Vec<u8> {
    super::php_src_corpus::run_case_in("dom", FIXTURES, folder, env, source)
}

mod bug79701;
mod compare_document_position;
mod create_attribute_ns_prefix_conflicts;
mod delayed_freeing;
mod gh11830;
mod legacy_bugs;
mod legacy_document;
mod legacy_gh;
mod legacy_misc;
mod legacy_nodes;
mod legacy_numbered;
mod legacy_xpath;
mod manually_call_constructor;
mod modern_common;
mod modern_css_selectors;
mod modern_extensions;
mod modern_html;
mod modern_html_encoding;
mod modern_html_interactions;
mod modern_html_parser;
mod modern_html_serializer;
mod modern_spec;
mod modern_token_list;
mod modern_xml;
