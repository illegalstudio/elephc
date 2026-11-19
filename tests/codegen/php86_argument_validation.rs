//! Purpose:
//! PHP 8.6 argument-validation checks that live in injected preludes.
//!
//! Called from:
//! - `cargo test --test codegen_tests php86_argument_validation`.
//!
//! Key details:
//! - The checks are compile-profile gated inside the prelude builders, so these fixtures
//!   compile with `--php-version=8.6`.
//! - The SimpleXML case needs the managed libxml2 package and is skipped when it is absent.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// `setlocale()` rejects an array `$locales` followed by extra args and NUL locale names.
#[test]
fn test_setlocale_php86_argument_validation() {
    let out = compile_and_run_with_php_version(
        r#"<?php
try { setlocale(LC_ALL, ['C'], 'extra'); } catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), "\n"; }
try { setlocale(LC_ALL, "C\0"); } catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), "\n"; }
"#,
        PhpVersion::Php86,
    );
    assert_eq!(
        out,
        "ArgumentCountError:setlocale() expects exactly 2 arguments when argument #2 ($locales) is an array\n\
ValueError:setlocale(): Argument #2 ($locales) must not contain any null bytes\n"
    );
}

/// `SimpleXMLElement::__construct()` rejects NUL bytes in the XML data.
#[test]
fn test_simplexml_construct_rejects_nul_php86() {
    if skip_without_xml_native("test_simplexml_construct_rejects_nul_php86") {
        return;
    }
    let out = compile_and_run_with_php_version(
        r#"<?php
try { new SimpleXMLElement("a\0"); } catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), "\n"; }
"#,
        PhpVersion::Php86,
    );
    assert_eq!(
        out,
        "ValueError:SimpleXMLElement::__construct(): Argument #1 ($data) must not contain any null bytes\n"
    );
}
