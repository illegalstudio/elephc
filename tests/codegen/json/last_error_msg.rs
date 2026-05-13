use super::*;

#[test]
fn test_json_last_error_msg_initial() {
    let out = compile_and_run("<?php echo json_last_error_msg();");
    assert_eq!(out, "No error");
}

#[test]
fn test_json_last_error_msg_after_successful_call() {
    let out = compile_and_run(
        "<?php json_encode([1, 2, 3]); echo json_last_error_msg();",
    );
    assert_eq!(out, "No error");
}

#[test]
fn test_json_last_error_msg_returns_string_type() {
    let out = compile_and_run(
        "<?php $msg = json_last_error_msg(); echo gettype($msg);",
    );
    assert_eq!(out, "string");
}

#[test]
fn test_json_last_error_msg_concat() {
    let out = compile_and_run(
        r#"<?php echo "msg=" . json_last_error_msg() . ";";"#,
    );
    assert_eq!(out, "msg=No error;");
}
