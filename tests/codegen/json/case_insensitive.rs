use super::*;

// PHP-visible builtins must be reachable through case-insensitive and
// namespaced call syntax (CLAUDE.md mandate). These tests cover the
// JSON public surface to lock the contract in place.

#[test]
fn json_encode_case_insensitive() {
    let out = compile_and_run(r#"<?php echo Json_Encode([1, 2]);"#);
    assert_eq!(out, "[1,2]");
}

#[test]
fn json_encode_uppercase() {
    let out = compile_and_run(r#"<?php echo JSON_ENCODE("hi");"#);
    assert_eq!(out, "\"hi\"");
}

#[test]
fn json_encode_namespaced() {
    let out = compile_and_run(r#"<?php echo \json_encode(true);"#);
    assert_eq!(out, "true");
}

#[test]
fn json_decode_case_insensitive() {
    let out = compile_and_run(r#"<?php echo JSON_DECODE("\"hi\"");"#);
    assert_eq!(out, "hi");
}

#[test]
fn json_decode_namespaced() {
    let out = compile_and_run(r#"<?php echo \json_decode("42");"#);
    assert_eq!(out, "42");
}

#[test]
fn json_validate_case_insensitive() {
    let out = compile_and_run(
        r#"<?php echo Json_Validate("[1,2]") ? "ok" : "no";"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn json_validate_namespaced() {
    let out = compile_and_run(
        r#"<?php echo \json_validate("not json") ? "ok" : "no";"#,
    );
    assert_eq!(out, "no");
}

#[test]
fn json_last_error_case_insensitive() {
    let out = compile_and_run(
        r#"<?php json_decode("not json"); $e = Json_Last_Error(); echo $e > 0 ? "err" : "ok";"#,
    );
    assert_eq!(out, "err");
}

#[test]
fn json_last_error_msg_namespaced() {
    let out = compile_and_run(
        r#"<?php json_decode("not json"); echo strlen(\json_last_error_msg()) > 0 ? "msg" : "empty";"#,
    );
    assert_eq!(out, "msg");
}
