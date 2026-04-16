use crate::support::*;
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
