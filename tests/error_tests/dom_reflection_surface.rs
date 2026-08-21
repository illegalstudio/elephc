//! Purpose:
//! Frontend TDD coverage for public DOM/libxml/SimpleXML call signatures exposed
//! through PHP reflection.
//!
//! Called from:
//! - `cargo test --test error_tests dom_reflection_surface`.
//!
//! Key details:
//! - Positive rows prevent the checker from rejecting valid named PHP 8.5.8 calls.
//! - Negative rows keep static argument planning aligned with the native runtime's
//!   catchable arity and named-argument contract.

use super::{expect_error, expect_no_error};

/// Verifies public DOM/libxml/SimpleXML call signatures accept their PHP 8.5.8 named forms.
#[test]
fn dom_reflection_surface_accepts_public_named_call_shapes() {
    expect_no_error(
        r#"<?php
function inspect(DOMDocument $legacy, DOMXPath $xpath): void {
    $legacy->loadXML(source: '<root/>', options: 0);
    $legacy->saveXML(node: null, options: 0);
    $xpath->query(expression: '//root', contextNode: null, registerNodeNS: true);
    Dom\XMLDocument::createFromString(source: '<root/>', options: 0, overrideEncoding: null);
    libxml_use_internal_errors(use_errors: null);
    libxml_set_external_entity_loader(resolver_function: null);
    simplexml_load_string(
        data: '<root/>',
        class_name: null,
        options: 0,
        namespace_or_prefix: '',
        is_prefix: false,
    );
    simplexml_import_dom(node: $legacy, class_name: null);
}
"#,
    );
}

/// Verifies static named-argument planning rejects unsupported names before bridge lowering.
#[test]
fn dom_reflection_surface_rejects_unknown_named_parameter() {
    expect_error(
        r#"<?php
Dom\XMLDocument::createFromString(source: '<root/>', unexpected: true);
"#,
        "Unknown named parameter $unexpected",
    );
}

/// Verifies required public function arguments remain mandatory in the checker-visible API surface.
#[test]
fn dom_reflection_surface_rejects_missing_required_function_argument() {
    expect_error(
        "<?php libxml_set_external_entity_loader();",
        "libxml_set_external_entity_loader() expects exactly 1 argument, 0 given",
    );
}
