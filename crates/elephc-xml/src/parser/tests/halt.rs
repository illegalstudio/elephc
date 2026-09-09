//! Purpose:
//! Unit test for the SAX callbacks' unwind guard: a panic inside a callback must not cross
//! the `extern "C"` boundary (which would abort the process) but halt the parser the way
//! `Parser::stop` does.
//!
//! Called from:
//! - `cargo test -p elephc-xml --lib parser::tests::halt`, only with
//!   `ELEPHC_XML_LIBXML2_LIB_DIR` set (`cfg(elephc_xml_native)`).
//!
//! Key details:
//! - `Parser::inject_callback_panic` is the `cfg(test)` hook that makes `Inner::push`
//!   panic; the default panic hook still prints the message to stderr, which is expected.

use crate::parser::{Parser, Step, PHP_XML_ERROR_NO_MEMORY};

/// A panicking callback halts the parser with `PHP_XML_ERROR_NO_MEMORY`: the chunk is
/// not well-formed, the queue is empty, and every `next()` answers `Failed`.
#[test]
fn callback_panic_halts_the_parser_instead_of_aborting() {
    let mut parser = Parser::new(None);
    parser.feed(b"<root>", false);
    assert!(matches!(parser.next(), Step::Event(_)), "start tag before the injection");
    assert!(matches!(parser.next(), Step::NeedMoreData));
    parser.inject_callback_panic();
    parser.feed(b"<child/>text</root>", true);
    assert!(!parser.is_well_formed());
    assert_eq!(parser.error_code(), PHP_XML_ERROR_NO_MEMORY);
    assert!(matches!(parser.next(), Step::Failed));
    assert!(matches!(parser.next(), Step::Failed), "failure is sticky");
    // Later chunks stay failed too, like a parser stopped by `stop()`.
    parser.feed(b"", true);
    assert!(!parser.is_well_formed());
    assert!(matches!(parser.next(), Step::Failed));
}
