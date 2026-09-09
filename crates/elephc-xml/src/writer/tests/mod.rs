//! Purpose:
//! Unit tests for the `xmlTextWriter` bridge, replaying the PHP 8.5.10 / libxml2 2.15.3
//! probe corpus call by call against the linked libxml2 and comparing the exact bytes.
//!
//! Called from:
//! - `cargo test -p elephc-xml --lib writer` through Rust's test harness, only with
//!   `ELEPHC_XML_LIBXML2_LIB_DIR` set (`cfg(elephc_xml_native)`).
//!
//! Key details:
//! - Expected strings are the `outputMemory()` results PHP printed for the same sequence;
//!   PHP-layer behavior (name `ValueError`s, NUL truncation, return shapes) is factored out.
//! - `take()` mirrors `outputMemory(true)`: flush, then drain.

mod corpus_basic;
mod corpus_dtd;
mod corpus_encoding;
mod corpus_namespaces;
mod corpus_states;
mod names;

use super::Writer;

/// A fresh memory writer.
fn writer() -> Writer {
    Writer::new()
}

/// A fresh memory writer with indentation enabled.
fn indented() -> Writer {
    let mut w = Writer::new();
    w.set_indent(true);
    w
}

/// `outputMemory(true)` as a lossy string for readable assertions.
fn take(w: &mut Writer) -> String {
    String::from_utf8_lossy(&w.take_output()).into_owned()
}

/// `outputMemory(true)` as raw bytes.
fn take_bytes(w: &mut Writer) -> Vec<u8> {
    w.take_output()
}
