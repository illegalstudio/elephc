//! Purpose:
//! Tests shared mbstring runtime semantics through native and dynamic eval calls.
//!
//! Called from:
//! - The codegen test binary's string module.
//!
//! Key details:
//! - Dynamic source depends on argc so eval cannot be replaced by a static fragment.
//! - Error and cache cases verify one state owner across the AOT/eval boundary.

use crate::support::*;

/// Verifies native counting uses mbstring codecs, including mobile, transfer, and composite mappings.
#[test]
fn test_mb_strlen_shared_codec_catalog() {
    let out = compile_and_run(r#"<?php
echo mb_strlen("\x82\xA0", "SJIS-mac"), ":";
echo mb_strlen("\xA4\xF7", "EUC-JP-2004"), ":";
echo mb_strlen("\xF8\x9F", "SJIS-Mobile#DOCOMO"), ":";
echo mb_strlen("\x80\xFF", "CP1252"), ":";
echo @mb_strlen("YQ==", "BASE64"), ":";
echo mb_strlen("abc", "UTF-8\0ignored"), ":";
try { mb_strlen("a", "UTF-8//IGNORE"); }
catch (ValueError $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "1:2:1:2:1:3:mb_strlen(): Argument #2 ($encoding) must be a valid encoding, \"UTF-8//IGNORE\" given");
}

/// Verifies opaque eval and callable dispatch retain catchable binary encoding errors.
#[test]
fn test_mb_strlen_dynamic_eval_errors_and_callables() {
    let out = compile_and_run(r#"<?php
$source = $argc > 0 ? '
echo mb_strlen("日本語", null), ":";
echo call_user_func("MB_STRLEN", "héllo", "8bit"), ":";
try { mb_strlen("a", "invalid-encoding"); }
catch (ValueError $error) { echo $error->getMessage(), ":"; }
try { call_user_func("mb_strlen", "a", "another-invalid"); }
catch (ValueError $error) { echo "callable-caught"; }
' : '';
eval($source);
echo ":", mb_strlen("é");
"#);
    assert_eq!(out, "3:6:mb_strlen(): Argument #2 ($encoding) must be a valid encoding, \"invalid-encoding\" given:callable-caught:1");
}

/// Verifies native and eval calls share the same explicit-encoding deprecation cache.
#[test]
fn test_mb_strlen_aot_eval_share_cache() {
    let out = compile_and_run_capture(r#"<?php
echo mb_strlen("YQ==", "BASE64"), ":";
$source = $argc > 0 ? 'echo mb_strlen("Yg==", "BASE64"), ":";' : '';
eval($source);
echo mb_strlen("Yw==", "BASE64");
"#);
    assert_eq!(out.stdout, "1:1:1");
    assert_eq!(out.stderr.matches("Handling Base64 via mbstring is deprecated").count(), 1);
}

/// Verifies `mb_strlen()` counts valid UTF-8 across ASCII, multibyte, and empty strings.
#[test]
fn test_mb_strlen_codepoint_count() {
    let out = compile_and_run(
        "<?php echo mb_strlen('abc'), ':', mb_strlen('héllo wörld'), ':', mb_strlen(''), ':', mb_strlen('日本語');",
    );
    assert_eq!(out, "3:11:0:3");
}

/// Verifies `mb_strlen()` accepts PHP's optional nullable encoding and byte-count aliases.
#[test]
fn test_mb_strlen_encoding_argument() {
    let out = compile_and_run(
        r#"<?php
echo mb_strlen("héllo", "UTF-8"), ":";
echo mb_strlen("héllo", "8bit"), ":";
echo mb_strlen(string: "日本語", encoding: null), ":";
$encoding = $argc > 0 ? "binary" : "UTF-8";
echo mb_strlen("héllo", $encoding), ":";
echo mb_strlen("\x68\x00\xE9\x00", "UTF-16LE"), ":";
$length = mb_strlen(...);
echo $length("héllo", "8bit");"#,
    );
    assert_eq!(out, "5:6:3:6:2:6");
}

/// Verifies malformed and truncated UTF-8 follows PHP mbstring substitution boundaries.
#[test]
fn test_mb_strlen_malformed_utf8() {
    let out = compile_and_run(
        r#"<?php
echo mb_strlen("\x80", "UTF-8"), ":";
echo mb_strlen("\xC0\xAF", "UTF-8"), ":";
echo mb_strlen("\xE2\x82", "UTF-8"), ":";
echo mb_strlen("\xED\xA0\x80", "UTF-8"), ":";
echo mb_strlen("\xF4\x90\x80\x80", "UTF-8"), ":";
echo mb_strlen("\xE2\x28\xA1", "UTF-8");"#,
    );
    assert_eq!(out, "1:2:1:3:4:3");
}

/// Verifies namespaced/case-insensitive lookup and unknown-encoding `ValueError` behavior.
#[test]
fn test_mb_strlen_namespace_and_invalid_encoding() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
echo Mb_StRlEn("日本語"), ":";
$encoding = $argc > 0 ? "definitely-not-an-encoding" : "UTF-8";
try {
    mb_strlen("abc", $encoding);
} catch (\ValueError $error) {
    echo "caught";
}"#,
    );
    assert_eq!(out, "3:caught");
}

/// Verifies nullable function results use the internal encoding while empty names still throw.
#[test]
fn test_mb_strlen_nullable_and_empty_encoding() {
    let out = compile_and_run(r#"<?php
function encoding(bool $nullable): ?string {
    if ($nullable) { return null; }
    return "8bit";
}
echo mb_strlen("é", encoding($argc > 0)), ":";
echo mb_strlen("é", encoding($argc < 0)), ":";
try { echo mb_strlen("abc", ""); }
catch (ValueError $error) { echo "empty-caught:"; }
$empty = substr("a", 1);
try { echo mb_strlen("abc", $empty); }
catch (ValueError $error) { echo "dynamic-empty-caught"; }
"#);
    assert_eq!(out, "1:2:empty-caught:dynamic-empty-caught");
}

/// Verifies full/simple Unicode case modes, contextual sigma, and non-UTF encodings.
#[test]
fn test_mbstring_case_modes_and_encodings() {
    let out = compile_and_run(r#"<?php
echo mb_strtoupper("Straße"), ":", mb_strtolower("ΟΔΟΣ"), ":";
echo mb_convert_case("Straße", MB_CASE_UPPER), ":";
echo mb_convert_case("Straße", MB_CASE_LOWER), ":";
echo mb_convert_case("Straße", MB_CASE_TITLE), ":";
echo mb_convert_case("Straße", MB_CASE_FOLD), ":";
echo mb_convert_case("Straße", MB_CASE_UPPER_SIMPLE), ":";
echo mb_convert_case("Straße", MB_CASE_LOWER_SIMPLE), ":";
echo mb_convert_case("Straße", MB_CASE_TITLE_SIMPLE), ":";
echo mb_convert_case("Straße", MB_CASE_FOLD_SIMPLE), ":";
echo bin2hex(mb_strtoupper("i\xfd", "ISO-8859-9")), ":";
echo mb_ucfirst("ßeta"), ":", mb_lcfirst("İstanbul"), ":";
echo mb_ucfirst(""), mb_strtolower("");
"#);
    assert_eq!(out, "STRASSE:οδος:STRASSE:straße:Straße:strasse:STRAßE:straße:Straße:straße:dd49:Sseta:i\u{307}stanbul:");
}

/// Verifies display widths, marker placement, signed bounds, and all optional named arguments.
#[test]
fn test_mbstring_width_and_trimming() {
    let out = compile_and_run(r#"<?php
namespace Labels;
echo Mb_StRwIdTh("漢字abc"), ":";
echo mb_strwidth("\xA4\xF7", "EUC-JP-2004"), ":";
echo mb_strimwidth("漢字abc", 0, 5, ".."), ":";
echo mb_strimwidth(encoding: null, trim_marker: "!", width: 4, start: 1, string: "漢字abc"), ":";
echo @mb_strimwidth("abcdef", -4, -1, "!"), ":";
echo mb_strimwidth("abc", 0, 3), ":";
$upper = mb_strtoupper(...);
echo $upper(...["string" => "é", "encoding" => "UTF-8"]);
"#);
    assert_eq!(out, "7:4:漢..:字a!:cd!:abc:É");
}

/// Verifies the same case/width operations and constants are available in opaque eval and callables.
#[test]
fn test_mbstring_dynamic_eval_text_operations() {
    let out = compile_and_run(r#"<?php
$source = $argc > 0 ? '
echo mb_strwidth("漢字abc"), ":";
echo mb_strtoupper("Straße"), ":", mb_strtolower("ΟΔΟΣ"), ":";
echo mb_convert_case("Straße", MB_CASE_FOLD), ":";
echo mb_ucfirst("ßeta"), ":", mb_lcfirst("İstanbul"), ":";
echo mb_strimwidth("漢字abc", 1, 4, "!"), ":";
echo call_user_func("MB_STRToupper", "é", null), ":";
try { mb_convert_case("abc", 8); }
catch (ValueError $error) { echo $error->getMessage(); }
' : '';
eval($source);
"#);
    assert_eq!(out, "7:STRASSE:οδος:strasse:Sseta:i\u{307}stanbul:字a!:É:mb_convert_case(): Argument #2 ($mode) must be one of the MB_CASE_* constants");
}

/// Verifies encoding lookup precedes operation validation and native errors remain catchable.
#[test]
fn test_mbstring_text_operation_errors() {
    let out = compile_and_run(r#"<?php
try { mb_convert_case("abc", 8); }
catch (ValueError $error) { echo $error->getMessage(), ":"; }
try { mb_convert_case("abc", 8, "invalid"); }
catch (ValueError $error) { echo $error->getMessage(), ":"; }
try { mb_strimwidth("abc", 4, 1); }
catch (ValueError $error) { echo $error->getMessage(), ":"; }
try { mb_strwidth("abc", ""); }
catch (ValueError $error) { echo "empty-caught"; }
"#);
    assert_eq!(out, "mb_convert_case(): Argument #2 ($mode) must be one of the MB_CASE_* constants:mb_convert_case(): Argument #3 ($encoding) must be a valid encoding, \"invalid\" given:mb_strimwidth(): Argument #2 ($start) is out of range:empty-caught");
}

/// Verifies runtime-selected callable names and nullable encoding results retain shared semantics.
#[test]
fn test_mbstring_callable_names_and_nullable_encodings() {
    let out = compile_and_run(r#"<?php
function selected_encoding(bool $default): ?string {
    if ($default) { return null; }
    return "UTF-8";
}
$name = $argc > 0 ? "mb_strtoupper" : "mb_strtolower";
echo call_user_func($name, "é", selected_encoding($argc > 0)), ":";
$upper = mb_strtoupper(...);
echo $upper("é", selected_encoding($argc > 0)), ":";
echo mb_convert_case(encoding: selected_encoding($argc > 0), mode: MB_CASE_FOLD, string: "Straße");
"#);
    assert_eq!(out, "É:É:strasse");
}

/// Shared source for each supported-target mbstring lowering check.
const MBSTRING_SUPPORTED_TARGET_SOURCE: &str = r#"<?php
function selected_encoding(bool $default): ?string {
    if ($default) { return null; }
    return "UTF-8";
}
function substitution_value(string|int|null $value): string|int|bool { return mb_substitute_character($value); }
function checked_input(array|string|null $value): bool { return mb_check_encoding($value); }
function selected_length(bool $default): ?int { if ($default) { return null; } return 2; }
#[Export]
function mbstring_fixture(int $count): int {
$value = $count > 0 ? "Straße 東京" : "other";
$encoding = selected_encoding($count > 0);
echo mb_strlen($value, $encoding), mb_strwidth($value, $encoding);
echo mb_strtoupper($value, $encoding), mb_strtolower($value, $encoding);
echo mb_convert_case($value, MB_CASE_FOLD, $encoding);
echo mb_ucfirst($value, $encoding), mb_lcfirst($value, $encoding);
echo mb_strimwidth($value, 0, 7, "..", $encoding);
echo mb_substr($value, 1, selected_length($count > 0), $encoding);
echo mb_strcut($value, 1, selected_length($count < 0), $encoding);
echo mb_scrub($value, $encoding), mb_trim($value, null, $encoding);
echo mb_ltrim($value, "S", $encoding), mb_rtrim($value, "京", $encoding);
echo mb_str_pad($value, 15, " ", STR_PAD_BOTH, $encoding);
echo mb_convert_kana($value, "KV", $encoding), mb_substr_count($value, "東", $encoding);
var_dump(mb_chr($count, $encoding), mb_ord($value, $encoding));
var_dump(mb_strpos($value, "東", 0, $encoding), mb_stripos($value, "s", 0, $encoding));
var_dump(mb_strrpos($value, "東", 0, $encoding), mb_strripos($value, "s", 0, $encoding));
var_dump(mb_strstr($value, "東", $count > 0, $encoding), mb_stristr($value, "s", false, $encoding));
var_dump(mb_strrchr($value, "東", false, $encoding), mb_strrichr($value, "s", false, $encoding));
var_dump(mb_language(), mb_internal_encoding($encoding), mb_http_output(null));
var_dump(mb_str_split($value, 2, $encoding), mb_encoding_aliases("ASCII"), mb_preferred_mime_name("utf8"));
var_dump(mb_check_encoding([$value], $encoding), mb_check_encoding(["label" => [$value]]));
var_dump(checked_input([$value]), checked_input($value), checked_input(null));
var_dump(substitution_value($count), substitution_value("none"), substitution_value(null));
$catalog = mb_list_encodings();
var_dump(mb_detect_encoding($value, $catalog), mb_detect_encoding($value, ["UTF-8", "ASCII"], true));
echo mb_decode_mimeheader("=?UTF-8?Q?caf=C3=A9?="), "\n";
echo mb_encode_mimeheader($value, "UTF-8", "Q", "\r\n", $count);
var_dump(mb_get_info(), mb_get_info("http_input"), mb_get_info($value));
var_dump(mb_http_input(), mb_http_input(null), mb_http_input("I"), mb_http_input($value));
var_dump(mb_regex_encoding(), mb_regex_encoding($encoding));
echo mb_regex_set_options(), mb_regex_set_options($count > 0 ? "ip" : "r");
var_dump(mb_ereg_match(pattern: ".", string: $value, options: $count > 0 ? null : "i"));
var_dump(mb_ereg_search_init(string: $value, pattern: ".", options: null));
var_dump(mb_ereg_search(), mb_ereg_search_pos(pattern: null), mb_ereg_search_regs(options: null));
var_dump(mb_ereg_search_getpos(), mb_ereg_search_getregs(), mb_ereg_search_setpos(offset: 0));
var_dump(mb_split(pattern: ",", string: $value, limit: $count));
var_dump(mb_ereg_replace(pattern: "(.)", replacement: "[\\0]", string: $value));
var_dump(mb_eregi_replace("a", $value, $value, $count > 0 ? null : ""));
$replace_arguments = $count > 0 ? ["a", "X"] : ["a", "X", $value];
try { mb_ereg_replace(...$replace_arguments); } catch (Throwable $error) { echo get_class($error); }
echo MB_ONIGURUMA_VERSION;
$current = mb_ereg_search_regs();
while (is_array($current)) {
    echo (string)$current[0];
    $current = mb_ereg_search_regs();
}
return 0;
}
function mbstring_mime_callback_fixture(int $count): void {
$encode = $count > 0 ? "mb_encode_mimeheader" : "mb_strtoupper";
echo $encode(string: "café"), $encode(charset: "UTF-8", string: "café");
try { call_user_func_array($encode, ["string" => "header", "charset" => mbstring_failed_argument()]); } catch (Exception) {}
$decode = $count > 0 ? "mb_decode_mimeheader" : "mb_strlen";
try { $decode(); } catch (ArgumentCountError $error) { echo $error->getMessage(); }
try { $decode([]); } catch (TypeError $error) { echo $error->getMessage(); }
try { call_user_func($decode, "header", mbstring_failed_argument()); } catch (Exception) {}
try { call_user_func_array($decode, [$count, "header", mbstring_failed_argument()]); } catch (Exception) {}
}
function mbstring_failed_argument(): string { throw new Exception("argument failed"); }
mbstring_fixture($argc);
mbstring_mime_callback_fixture($argc);
$mbstring_dynamic = $argc > 0 ? 'try { mb_ereg_search(options: "Q"); } catch (Throwable $error) { var_dump($error->getPrevious()); }' : '';
eval($mbstring_dynamic);
"#;

/// Verifies one supported target lowers shared text calls and nullable parameters through the bridge.
fn check_mbstring_supported_target_lowering(target: &str) {
    let dir = make_cli_test_dir("elephc_mbstring_target");
    let php = dir.join("main.php");
    std::fs::write(&php, MBSTRING_SUPPORTED_TARGET_SOURCE).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--emit-asm", "--target", target]);
    if target.starts_with("ios-") { command.args(["--emit", "staticlib"]); }
    let output = command.arg(&php).output().unwrap();
    assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
    let assembly = std::fs::read_to_string(php.with_extension("s")).unwrap();
    assert!(assembly.contains("__rt_mbstring_native"), "{target}");
    assert!(assembly.contains("mbstring_pointers_ready"), "{target}");
    assert!(assembly.contains("__elephc_eval_builtin_throwable_getprevious"), "{target}");
    assert!(assembly.contains("__rt_throwable_previous"), "{target}");
    if !target.starts_with("ios-") {
        assert!(assembly.contains("__rt_mbstring_request_reset"), "{target}");
        assert!(assembly.contains("__rt_mbstring_release_catalog"), "{target}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

macro_rules! supported_target_lowering_test {
    ($name:ident, $target:literal) => {
        /// Checks one supported target so its compile has an independent CI timeout.
        #[test]
        fn $name() {
            check_mbstring_supported_target_lowering($target);
        }
    };
}

supported_target_lowering_test!(test_mbstring_supported_target_lowering_macos_aarch64, "macos-aarch64");
supported_target_lowering_test!(test_mbstring_supported_target_lowering_ios_arm64, "ios-arm64");
supported_target_lowering_test!(test_mbstring_supported_target_lowering_ios_sim_arm64, "ios-sim-arm64");
supported_target_lowering_test!(test_mbstring_supported_target_lowering_linux_aarch64, "linux-aarch64");
supported_target_lowering_test!(test_mbstring_supported_target_lowering_linux_x86_64, "linux-x86_64");
