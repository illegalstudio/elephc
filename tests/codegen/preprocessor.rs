//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of preprocessor, including ifdef selects then branch when symbol is defined, ifdef selects else branch when symbol is missing, and ifdef without else can erase statement.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

use crate::support::*;
/// Verifies that when a symbol is defined via CLI flags, the then-branch of
/// `ifdef SYMBOL { ... } else { ... }` is selected and executed.
#[test]
fn test_ifdef_selects_then_branch_when_symbol_is_defined() {
    let out = compile_and_run_with_defines(
        r#"<?php
ifdef DEBUG {
    echo "debug";
} else {
    echo "release";
}
"#,
        &["DEBUG"],
    );
    assert_eq!(out, "debug");
}

/// Verifies that when a symbol is not defined, the else-branch is selected.
#[test]
fn test_ifdef_selects_else_branch_when_symbol_is_missing() {
    let out = compile_and_run_with_defines(
        r#"<?php
ifdef DEBUG {
    echo "debug";
} else {
    echo "release";
}
"#,
        &[],
    );
    assert_eq!(out, "release");
}

/// Verifies that an ifdef without an else branch erases the entire block
/// (no trailing else implied) when the symbol is not defined.
#[test]
fn test_ifdef_without_else_can_erase_statement() {
    let out = compile_and_run_with_defines(
        r#"<?php
echo "a";
ifdef DEBUG {
    echo "b";
}
echo "c";
"#,
        &[],
    );
    assert_eq!(out, "ac");
}

/// Verifies that ifdef branches can be nested, and that inner/outer
/// symbol presence correctly selects the appropriate branch at each level.
#[test]
fn test_ifdef_supports_nested_branches() {
    let out = compile_and_run_with_defines(
        r#"<?php
ifdef OUTER {
    ifdef INNER {
        echo "both";
    } else {
        echo "outer";
    }
} else {
    echo "none";
}
"#,
        &["OUTER"],
    );
    assert_eq!(out, "outer");
}
/// Verifies that the same source file compiled with different CLI define
/// flags produces branch-appropriate output in both cases.
#[test]
fn test_ifdef_cli_define_flag_controls_branch_selection() {
    let source = r#"<?php
ifdef DEBUG {
    echo "debug";
} else {
    echo "release";
}
"#;

    let release_out = compile_cli_file_and_run(source, &[]);
    assert_eq!(release_out, "release");

    let debug_out = compile_cli_file_and_run(source, &["DEBUG"]);
    assert_eq!(debug_out, "debug");
}

/// Verifies that when an ifdef branch is active (symbol defined), any
/// require/include inside that branch is resolved and loaded.
#[test]
fn test_ifdef_active_branch_resolves_includes() {
    let out = compile_and_run_files_with_defines(
        &[
            (
                "main.php",
                r#"<?php
ifdef FEATURE {
    require "part.php";
}
"#,
            ),
            ("part.php", "<?php echo \"ok\";"),
        ],
        "main.php",
        &["FEATURE"],
    );
    assert_eq!(out, "ok");
}

/// Verifies that when an ifdef branch is inactive (symbol not defined),
/// includes inside that branch are not resolved, so a missing file does
/// not cause a compile error.
#[test]
fn test_ifdef_inactive_branch_skips_missing_include_resolution() {
    let out = compile_and_run_files_with_defines(
        &[(
            "main.php",
            r#"<?php
ifdef FEATURE {
    require "missing.php";
}
echo "safe";
"#,
        )],
        "main.php",
        &[],
    );
    assert_eq!(out, "safe");
}
