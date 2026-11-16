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
    }
    std::fs::remove_dir_all(dir).unwrap();
}


/// Verifies the ASCII case fast paths, including Unicode fallbacks and title word rules.
#[test]
fn test_mb_ascii_case_fast_paths() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$ascii = "hello_world-foo bar";
$accented = "héllo wörld";
echo mb_strtoupper($ascii, "UTF-8"), "|";
echo mb_strtolower("HELLO", "UTF-8"), "|";
echo mb_convert_case($ascii, MB_CASE_TITLE, "UTF-8"), "|";
echo mb_convert_case("don't stop", MB_CASE_TITLE, "UTF-8"), "|";
echo mb_convert_case("ABC", MB_CASE_FOLD, "UTF-8"), "|";
echo mb_ucfirst("hello", "UTF-8"), "|";
echo mb_lcfirst("Hello", "UTF-8"), "|";
echo mb_strtoupper($accented, "UTF-8"), "|";
echo mb_strtolower("HÉLLO", "UTF-8"), "|";
echo mb_convert_case($accented, MB_CASE_TITLE, "UTF-8"), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "HELLO_WORLD-FOO BAR|hello|Hello_World-Foo Bar|Don't Stop|abc|Hello|hello|\
         HÉLLO WÖRLD|héllo|Héllo Wörld|\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies ASCII width, encoding validation, slicing, and padding fast paths.
#[test]
fn test_mb_ascii_width_slice_and_pad_fast_paths() {
    let out = compile_and_run_with_heap_debug(r#"<?php
echo mb_strwidth("hello", "UTF-8"), "|";
echo mb_strwidth("héllo", "UTF-8"), "|";
echo mb_check_encoding("héllo", "UTF-8") ? "1" : "0", "|";
echo mb_check_encoding("\x80", "UTF-8") ? "1" : "0", "|";
echo mb_substr("abcdefghij", 2, 3, "UTF-8"), "|";
echo mb_substr("abcdefghij", -3, 2, "UTF-8"), "|";
echo mb_strcut("abcdefghij", 2, 3, "UTF-8"), "|";
echo mb_str_pad("x", 5, "-", STR_PAD_BOTH, "UTF-8"), "|";
echo mb_ord("A", "UTF-8"), "|";
try { mb_ord("", "UTF-8"); } catch (ValueError $e) { echo "empty|"; }
echo mb_ord("é", "UTF-8"), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "5|5|1|0|cde|hi|cde|--x--|65|empty|233|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies the x86_64 ASCII predicate reports through rcx so `mb_ord` keeps the
/// subject pointer in rax for its first-byte load.
#[test]
fn test_mb_ord_x86_64_ascii_predicate_preserves_the_subject_pointer() {
    let dir = make_cli_test_dir("elephc_mb_ord_x86_ascii");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo mb_ord(\"A\", \"UTF-8\");").unwrap();
    let output = elephc_cli_command(&dir)
        .args(["--target", "linux-x86_64", "--emit-asm"])
        .arg(&php_path)
        .output()
        .expect("failed to emit x86_64 assembly");
    assert!(
        output.status.success(),
        "cross-target --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(dir.join("main.s")).expect("expected x86_64 assembly");
    let call = asm
        .lines()
        .position(|line| line.trim() == "call __rt_mb_is_ascii")
        .expect("mb_ord must check the UTF-8 subject for ASCII");
    let after: Vec<&str> = asm.lines().map(str::trim).skip(call + 1).take(8).collect();
    assert!(
        after[0].starts_with("test rcx, rcx"),
        "the x86_64 predicate must report through rcx, got: {}",
        after[0]
    );
    assert!(
        after.iter().any(|line| line.starts_with("movzx edi, BYTE PTR [rax]")),
        "mb_ord must load the first subject byte through the preserved rax pointer"
    );

    let target = Target::parse("linux-x86_64").expect("linux-x86_64 is supported");
    let runtime = elephc::codegen::generate_runtime(8_388_608, target);
    let helper = runtime
        .split("__rt_mb_is_ascii:")
        .nth(1)
        .expect("the runtime must emit the ASCII predicate")
        .split("--- runtime:")
        .next()
        .unwrap();
    assert!(helper.contains("mov ecx, 1"), "the predicate must report through rcx");
    assert!(helper.contains("xor ecx, ecx"), "the predicate must reject through rcx");
    assert!(!helper.contains("mov eax, 1"), "the predicate must not clobber rax");
    assert!(!helper.contains("xor eax, eax"), "the predicate must not clobber rax");
    let _ = fs::remove_dir_all(dir);
}

/// Verifies the x86_64 digit-run census copies the subject pointer before reusing rax
/// for the replaced-length accumulator.
#[test]
fn test_mb_ereg_replace_len_x86_64_keeps_the_subject_cursor() {
    let dir = make_cli_test_dir("elephc_mb_ereg_replace_len_x86");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php echo strlen(mb_ereg_replace(\"[0-9]+\", \"#\", \"a1b22c333\"));",
    )
    .unwrap();
    let output = elephc_cli_command(&dir)
        .args(["--target", "linux-x86_64", "--emit-asm"])
        .arg(&php_path)
        .output()
        .expect("failed to emit x86_64 assembly");
    assert!(
        output.status.success(),
        "cross-target --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(dir.join("main.s")).expect("expected x86_64 assembly");
    let lines: Vec<&str> = asm.lines().map(str::trim).collect();
    let call = lines
        .iter()
        .position(|line| *line == "call __rt_mb_is_ascii")
        .expect("the digit-run census must check the subject for ASCII");
    assert!(lines[call + 1].starts_with("test rcx, rcx"), "got: {}", lines[call + 1]);
    let reset = lines[call..]
        .iter()
        .position(|line| line.starts_with("xor eax, eax"))
        .expect("the census must start its replaced-length accumulator at zero")
        + call;
    assert_eq!(
        lines[reset - 1], "mov r8, rax",
        "the census must copy the subject pointer into r8 before rax becomes the count"
    );
    let fallback = lines
        .iter()
        .position(|line| line.contains("mb_ereg_replace_len_fallback"))
        .expect("the non-fast pattern must emit the bridge fallback");
    let unbox = lines[fallback..]
        .iter()
        .position(|line| *line == "call __rt_mixed_unbox")
        .expect("the non-fast pattern must unbox the bridge result")
        + fallback;
    assert!(
        lines[unbox + 2].starts_with("mov QWORD PTR [rsp + 16], rdx"),
        "the unboxed string length must come from the high payload register, got: {}",
        lines[unbox + 2]
    );
    let _ = fs::remove_dir_all(dir);
}

/// Verifies ASCII trim, substring counting, and a non-ASCII trim fallback.
#[test]
fn test_mb_ascii_trim_and_substr_count_fast_paths() {
    let out = compile_and_run_with_heap_debug(r#"<?php
echo mb_trim("  hi  ", " ", "UTF-8"), "|";
echo mb_ltrim("  hi  ", " ", "UTF-8"), "|";
echo mb_rtrim("  hi  ", " ", "UTF-8"), "|";
echo mb_substr_count("banana", "ana", "UTF-8"), "|";
echo mb_substr_count("héllo", "l", "UTF-8"), "|";
echo mb_trim("héllo", " ", "UTF-8"), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "hi|hi  |  hi|1|2|héllo|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Verifies the allocation-free codepoint, detection, width, split, scrub, and search sinks.
#[test]
fn test_mb_ascii_search_and_length_sink_fast_paths() {
    let out = compile_and_run_with_heap_debug(r#"<?php
echo (int)mb_ord("A", "UTF-8"), "|";
echo strlen(mb_chr(65, "UTF-8")), "|";
echo strlen(mb_chr(0x20AC, "UTF-8")), "|";
echo mb_detect_encoding("héllo", "UTF-8"), "|";
echo strlen(mb_strimwidth("The quick brown fox jumps over the lazy dog", 0, 10, "...", "UTF-8")), "|";
echo count(mb_str_split("abcdefghij", 2, "UTF-8")), "|";
echo strlen(mb_scrub("héllo", "UTF-8")), "|";
echo (int)mb_strpos("abcdef", "cd", 0, "UTF-8"), "|";
echo (int)mb_strripos("abcABC", "a", 0, "UTF-8"), "|";
echo strlen(mb_strstr("abcdef", "cd", false, "UTF-8")), "|";
echo strlen(mb_strrichr("abcABC", "a", false, "UTF-8")), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "65|1|3|UTF-8|10|5|6|2|3|4|3|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Verifies the numeric-entity, literal-split, regex-replace, and raw-position fast paths.
#[test]
fn test_mb_numericentity_and_regex_fast_paths() {
    let out = compile_and_run_with_heap_debug(r##"<?php
$map = [0x80, 0x10FFFF, 0, 0x1FFFFF];
echo strlen(mb_encode_numericentity("héllo wörld", $map, "UTF-8")), "|";
echo mb_encode_numericentity("€", $map, "UTF-8"), "|";
echo strlen(mb_encode_numericentity("a\x80b", $map, "UTF-8")), "|";
echo strlen(mb_decode_numericentity("h&#233;llo w&#246;rld", $map, "UTF-8")), "|";
echo mb_decode_numericentity("h&#233;llo", $map, "UTF-8"), "|";
echo strlen(mb_decode_numericentity("a&b", $map, "UTF-8")), "|";
echo count(mb_split(",", "1,2,3")), "|";
echo count(mb_split(",", "")), "|";
echo count(mb_split(",", ",a,")), "|";
$regexSplit = mb_split("a+", "baaac");
echo count($regexSplit), "|";
echo strlen(mb_ereg_replace("[0-9]+", "#", "abc123def456")), "|";
echo mb_ereg_replace("[0-9]+", "#", "abc123def456"), "|";
echo strlen(mb_ereg_replace("[0-9]+", "#", "a1b22c333")), "|";
echo strlen(mb_ereg_replace("[a-z]+", "#", "abc123")), "|";
echo (int)mb_strpos("abcdef", "cd", 0, "UTF-8"), "|";
echo (int)mb_strrpos("abcdef", "cd", 0, "UTF-8"), "|";
echo count(mb_encoding_aliases("UTF-8")), "|";
echo count(mb_list_encodings()), "|";
echo strlen(mb_preferred_mime_name("UTF-8")), "|";
echo strlen(mb_get_info("internal_encoding")), "|";
echo "\n";
"##);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "21|&#8364;|3|13|héllo|3|3|1|3|2|8|abc#def#|6|4|2|2|1|79|5|5|\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Verifies the allocation-free mbstring setting fast paths and boxed boolean results.
#[test]
fn test_mb_setting_fast_paths() {
    let out = compile_and_run_with_heap_debug(r#"<?php
echo (int)mb_internal_encoding("UTF-8"), "|";
echo (int)mb_language("neutral"), "|";
echo (int)mb_http_output("pass"), "|";
echo (int)mb_regex_encoding("UTF-8"), "|";
echo (int)mb_substitute_character(0xFFFD), "|";
echo mb_internal_encoding(), "|";
echo mb_http_output(), "|";
echo mb_regex_encoding(), "|";
echo strlen((string)mb_regex_set_options("r")), "|";
echo strlen((string)mb_http_input("G")), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1|1|1|1|1|UTF-8|pass|UTF-8|2|0|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Verifies the MIME base64 fast path, the ASCII passthrough, and the bridge fallback for an
/// ASCII prefix before the first non-ASCII byte.
#[test]
fn test_mb_encode_mimeheader_fast_path() {
    let out = compile_and_run_with_heap_debug(r#"<?php
echo mb_encode_mimeheader("héllo", "UTF-8"), "|";
echo mb_encode_mimeheader("héllo wörld", "UTF-8"), "|";
echo mb_encode_mimeheader("hello", "UTF-8"), "|";
echo mb_encode_mimeheader("a b é", "UTF-8"), "|";
echo mb_encode_mimeheader("日本", "UTF-8"), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "=?UTF-8?B?aMOpbGxv?=|=?UTF-8?B?aMOpbGxvIHfDtnJsZA==?=|hello|\
         a b =?UTF-8?B?w6k=?=|=?UTF-8?B?5pel5pys?=|\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Verifies the `K` half-width katakana converter, including the voiced-mark bridge fallback.
#[test]
fn test_mb_convert_kana_halfwidth_fast_path() {
    let out = compile_and_run_with_heap_debug(r#"<?php
echo mb_convert_kana("ｱｲｳｴｵ", "K", "UTF-8"), "|";
echo mb_convert_kana("ｱｲ abc", "K", "UTF-8"), "|";
echo mb_convert_kana("アイウエオ", "K", "UTF-8"), "|";
echo mb_convert_kana("aｱb", "K", "UTF-8"), "|";
echo mb_convert_kana("ｱﾞ", "K", "UTF-8"), "|";
echo mb_convert_kana("｡｢｣､･ｦｰ", "K", "UTF-8"), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "アイウエオ|アイ abc|アイウエオ|aアb|ア゛|。「」、・ヲー|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}



/// Repeats the explicit UTF-8 internal-encoding setter so the no-op fast path is exercised.
#[test]
fn test_mb_internal_encoding_repeated_setter() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$ok = true;
for ($i = 0; $i < 100; $i++) { $ok = $ok && mb_internal_encoding("UTF-8"); }
echo $ok ? "ok" : "fail", "|", mb_internal_encoding(), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ok|UTF-8\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies the UTF-8-to-UTF-8 output-handler passthrough retains its subject without copying.
#[test]
fn test_mb_output_handler_utf8_passthrough() {
    let out = compile_and_run_with_heap_debug(r#"<?php
echo mb_output_handler("héllo", 0), "|";
echo bin2hex(mb_output_handler("\x80", 0)), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "héllo|80|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Verifies the literal UTF-8 detection-order specialization, the getter, and the invalid fallback.
#[test]
fn test_mb_detect_order_literal_fast_path() {
    let out = compile_and_run_with_heap_debug(r#"<?php
var_dump(mb_detect_order(["UTF-8"]));
var_dump(mb_detect_order());
try { mb_detect_order(["NOPE"]); } catch (ValueError $e) { echo "error\n"; }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\narray(1) {\n  [0]=>\n  string(5) \"UTF-8\"\n}\nerror\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Verifies the dedicated progressive-search init/step/position path and invalid-subject fallback.
#[test]
fn test_mb_ereg_search_native_state_path() {
    let out = compile_and_run_with_heap_debug(r#"<?php
var_dump(mb_ereg_search_init("abc123", "[0-9]+"));
var_dump(mb_ereg_search());
var_dump(mb_ereg_search_getpos());
var_dump(mb_ereg_search_setpos(3));
var_dump(mb_ereg_search_getpos());
var_dump(mb_ereg_search());
var_dump(mb_ereg_search_getpos());
var_dump(mb_ereg_search_init("\x80", "[0-9]+"));
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\nbool(true)\nint(6)\nbool(true)\nint(3)\nbool(true)\nint(6)\nbool(false)\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Verifies the constant-local length folds and the non-ASCII fallbacks they must leave alone.
#[test]
fn test_mb_constant_local_length_folds() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$s = str_repeat("abcdefgh", 16);
$t = "   hello world   ";
$w = "The quick brown fox";
$e = "héllo wörld";
$n = str_repeat("a", 100);
$p = "abcdefghijklmnopqrst";
echo mb_strlen($s, "UTF-8"), "|";
echo strlen(mb_strtoupper($s, "UTF-8")), "|";
echo strlen(mb_convert_case($s, MB_CASE_TITLE, "UTF-8")), "|";
echo strlen(mb_trim($t, " \n\r\t\v\0", "UTF-8")), "|";
echo mb_strwidth($w, "UTF-8"), "|";
echo mb_check_encoding($e, "UTF-8") ? "1" : "0", "|";
echo mb_substr_count($n, "a", "UTF-8"), "|";
echo count(mb_str_split($p, 2, "UTF-8")), "|";
echo strlen(mb_str_pad("x", 32, "-", STR_PAD_RIGHT, "UTF-8")), "|";
echo mb_substr($p, 10, 5, "UTF-8"), "|";
echo strlen(mb_strcut($s, 10, 20, "UTF-8")), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "128|128|128|11|19|1|100|10|32|klmno|20|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Verifies that omitted optional mbstring arguments never take the constant-local fold.
#[test]
fn test_mb_constant_local_fold_optional_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$s = "abcdef";
echo strlen(mb_substr($s, 1, 2)), "|";
echo strlen(mb_trim("  hi  ")), "|";
echo strlen(mb_ltrim("  hi  ")), "|";
echo strlen(mb_str_pad("x", 3)), "|";
echo strlen(mb_convert_case($s, MB_CASE_TITLE)), "|";
echo strlen(mb_ucfirst($s)), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2|2|4|3|6|6|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Verifies that escape-generated byte strings keep their exact bytes through the folds.
#[test]
fn test_mb_constant_folds_preserve_escape_bytes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$bad = "\x80";
$valid = "héllo";
$escaped = "\xC3\xA9";
echo mb_check_encoding($bad, "UTF-8") ? "1" : "0", "|";
echo mb_strlen($bad, "UTF-8"), "|";
echo mb_strlen($escaped, "UTF-8"), "|";
echo mb_strlen($valid, "UTF-8"), "|";
echo mb_strwidth($valid, "UTF-8"), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "0|1|1|5|5|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Verifies the native array-returning `mb_ereg_search_pos`/`regs`/`getregs` paths.
///
/// The returned arrays must own copies of the session match, so mutating one result never
/// changes what a later `getregs()` reads. The no-match and uninitialized branches keep PHP's
/// `false` result.
#[test]
fn test_mb_ereg_search_native_array_paths() {
    let out = compile_and_run_with_heap_debug(r#"<?php
mb_ereg_search_init("abc123", "[0-9]+");
var_dump(mb_ereg_search_pos());
var_dump(mb_ereg_search_pos());
var_dump(mb_ereg_search_getpos());
var_dump(mb_ereg_search_getregs());
mb_ereg_search_init("abc123", "[0-9]+");
$r = mb_ereg_search_regs();
var_dump($r);
$r[0] = "changed";
var_dump(mb_ereg_search_getregs());
mb_ereg_search_init("abc", "[0-9]+");
var_dump(mb_ereg_search_pos());
var_dump(mb_ereg_search_regs());
var_dump(mb_ereg_search_getregs());
mb_ereg_search_init("abc", "[0-9]*");
var_dump(mb_ereg_search_pos());
var_dump(mb_ereg_search_getregs());
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(2) {\n  [0]=>\n  int(3)\n  [1]=>\n  int(3)\n}\n\
bool(false)\nint(6)\nbool(false)\n\
array(1) {\n  [0]=>\n  string(3) \"123\"\n}\n\
array(1) {\n  [0]=>\n  string(3) \"123\"\n}\n\
bool(false)\nbool(false)\nbool(false)\n\
array(2) {\n  [0]=>\n  int(0)\n  [1]=>\n  int(0)\n}\n\
array(1) {\n  [0]=>\n  string(0) \"\"\n}\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
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
