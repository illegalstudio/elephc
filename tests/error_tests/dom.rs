//! Purpose:
//! Compile-time diagnostics for DOM classes whose construction is abstract or private.
//!
//! Called from:
//! - `cargo test --test error_tests dom` through Rust's test harness.
//!
//! Key details:
//! - These compiler-resident guards prevent constructor opcodes from reaching the native bridge.

use super::expect_error;

/// Verifies the modern node base exposes only its private internal constructor.
#[test]
fn modern_dom_node_constructor_is_private() {
    expect_error(
        "<?php $node = new Dom\\Node();",
        "Cannot access private constructor: Dom\\Node::__construct",
    );
}

/// Verifies namespace-info values can only be produced by modern element queries.
#[test]
fn modern_dom_namespace_info_constructor_is_private() {
    expect_error(
        "<?php $info = new Dom\\NamespaceInfo();",
        "Cannot access private constructor: Dom\\NamespaceInfo::__construct",
    );
}

/// Verifies token lists can only be produced by modern element properties.
#[test]
fn modern_dom_token_list_constructor_is_private() {
    expect_error(
        "<?php $tokens = new Dom\\TokenList();",
        "Cannot access private constructor: Dom\\TokenList::__construct",
    );
}
