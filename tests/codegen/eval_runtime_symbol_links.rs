//! Purpose:
//! Regression coverage for eval bridge runtime symbols in minimal programs.
//!
//! Called from:
//! - `tests/codegen/mod.rs` through the codegen integration suite.

use crate::support::*;

/// The eval interface probe links even when the AOT program declares no
/// user-defined interfaces, and answers false for an absent name.
#[test]
fn test_eval_interface_probe_links_without_static_interface_inventory() {
    let out = compile_and_run(
        r#"<?php
$function = "interface_exists";
$source = 'return '.$function.'("MissingEvalInterface") ? "Y" : "N";';
echo eval($source);
"#,
    );
    assert_eq!(out, "N");
}
