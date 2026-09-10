//! Purpose:
//! Regression coverage for the deprecated libxml external-entity-loader toggle.
//!
//! Called from:
//! - `cargo test --test codegen_tests libxml_entity_loader` through Rust's test harness.
//!
//! Key details:
//! - PHP preserves the previous toggle state while emitting one deprecation per call.
//! - The exact diagnostic and state sequence are pinned against the PHP 8.5.8/libxml2 2.15.3 oracle.

use crate::support::compile_and_run;

/// Verifies the entity-loader toggle preserves state and PHP's deprecation ordering.
#[test]
fn libxml_disable_entity_loader_preserves_state_and_deprecation_order() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $severity, string $message): bool {
    echo $severity . ":" . $message . "|";
    return true;
});
echo libxml_disable_entity_loader() ? "T" : "F";
echo libxml_disable_entity_loader(true) ? "T" : "F";
echo libxml_disable_entity_loader(false) ? "T" : "F";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "8192:Function libxml_disable_entity_loader() is deprecated since 8.0, ",
            "as external entity loading is disabled by default|F",
            "8192:Function libxml_disable_entity_loader() is deprecated since 8.0, ",
            "as external entity loading is disabled by default|T",
            "8192:Function libxml_disable_entity_loader() is deprecated since 8.0, ",
            "as external entity loading is disabled by default|T",
        ),
    );
}
