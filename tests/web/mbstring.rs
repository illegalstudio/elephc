//! Purpose:
//! Verifies mbstring request settings and native catalog ownership across reused web workers.
//!
//! Called from:
//! - The focused web integration harness using one local prefork worker.
//!
//! Key details:
//! - Each response must rebuild catalog identity after the preceding request's arena reset.
//! - Candidate mutations and request-setting changes must not leak into the next response.

use super::*;

#[path = "../support/managed_pcre2.rs"]
mod managed_pcre2;

/// Restores compiled INI defaults after runtime setting mutations in the same prefork worker.
#[test]
fn web_mbstring_startup_configuration_reset() {
    let dir = make_test_dir("web_mbstring_startup");
    let native = managed_pcre2::prepare_managed_pcre2_cli_project(&dir,
        elephc::codegen::platform::Target::detect_host());
    let php = dir.join("app.php");
    fs::write(&php, r#"<?php
echo mb_internal_encoding(), ":", mb_http_input("L"), ":", mb_http_output(), ":";
echo mb_get_info("strict_detection"), ":";
mb_internal_encoding("ASCII");
mb_http_output("UTF-8");
echo mb_internal_encoding(), ":";
echo ini_get("mbstring.language"), ":";
ini_set("mbstring.language", "Japanese");
$all = (array)ini_get_all(null, false);
echo $all["mbstring.language"], ":", $all["session.name"], ":";
ini_restore("mbstring.language");
echo ini_get("mbstring.language");
"#).unwrap();
    let compiled = Command::new(elephc_bin()).current_dir(&dir)
        .env("XDG_CACHE_HOME", dir.join("cache-root")).env("ELEPHC_NATIVE_CACHE", native)
        .args(["--web", "--ini", "default_charset=8bit", "--ini", "mbstring.strict_detection=1"])
        .arg(&php).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let addr = format!("127.0.0.1:{}", free_port());
    let mut child = spawn_server(&php.with_extension(""), &addr, "1");
    let responses: Vec<_> = (0..12).map(|_| http_get(&addr, "/")).collect();
    let _ = child.kill();
    let _ = child.wait();
    for (index, response) in responses.iter().enumerate() {
        assert_eq!(raw_response_body(response), "8bit:8bit:8bit:On:ASCII:neutral:Japanese:PHPSESSID:neutral", "request {index}: {response}");
    }
    fs::remove_dir_all(dir).unwrap();
}

/// Reuses one worker while detecting with shared/detached lists and changing request defaults.
#[test]
fn web_mbstring_catalog_and_settings_reset() {
    let dir = make_test_dir("web_mbstring_catalog");
    let source = r#"<?php
$text = "caff" . chr(168) . chr(168) . " Stra?e";
$order = mb_detect_order();
if (is_array($order)) { echo implode(",", $order), ":"; }
echo mb_internal_encoding(), ":";
echo mb_get_info("illegal_chars"), ":";
echo mb_http_input("L"), ":";
$catalog = mb_list_encodings();
echo mb_detect_encoding($text, $catalog), ":";
$copy = $catalog;
$copy[0] = "ASCII";
$copy[0] = "BASE64";
echo mb_detect_encoding($text, $copy), ":";
mb_detect_order("SJIS");
mb_internal_encoding("8bit");
mb_scrub(chr(255), "UTF-8");
echo mb_get_info("illegal_chars"), ":";
echo "done";
"#;
    let bin = compile_web(&dir, source, "app");
    let addr = format!("127.0.0.1:{}", free_port());
    let mut child = spawn_server(&bin, &addr, "1");
    let responses: Vec<_> = (0..12).map(|_| http_get(&addr, "/")).collect();
    let _ = child.kill();
    let _ = child.wait();
    for (index, response) in responses.iter().enumerate() {
        assert_eq!(raw_response_body(response), "ASCII,UTF-8:UTF-8:0:UTF-8:GB18030:SJIS:1:done", "request {index}: {response}");
    }
    fs::remove_dir_all(dir).unwrap();
}
