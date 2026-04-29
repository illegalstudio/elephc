use super::*;

#[test]
fn test_error_control_suppresses_runtime_warning() {
    let out = compile_and_run_capture(
        r#"<?php
echo @file_get_contents("missing.txt");
echo "after";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "after");
    assert_eq!(out.stderr, "");
}

#[test]
fn test_readline() {
    let out = compile_and_run_with_stdin(
        r#"<?php
$line = readline();
echo "read: " . trim($line);
"#,
        "world\n",
    );
    assert_eq!(out, "read: world");
}
