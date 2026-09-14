//! Purpose:
//! Exercises nullable lengths, scalar result kinds, and shared mbstring settings.
//!
//! Called from:
//! - The codegen test binary's string module.
//!
//! Key details:
//! - The same PHP fixture runs natively and through opaque eval.
//! - Expected output was cross-checked against the PHP 8.5 mbstring baseline.

use crate::support::*;

const SCALAR_PROGRAM: &str = r#"function length_option(bool $all): ?int { if ($all) { return null; } return 2; }
echo mb_substr("Aé日本B", 1, length_option($argc > 0)), ":";
echo mb_substr("Aé日本B", 1, length_option($argc < 0)), ":";
echo mb_strcut("Aé日本B", 2, 5), ":";
echo mb_substr(encoding: null, length: null, start: -2, string: "Aé日本B"), ":";
echo mb_scrub(chr(255) . "é"), ":";
echo mb_trim("　 猫　"), ":", mb_ltrim("xx猫xx", "x"), ":", mb_rtrim("xx猫xx", "x"), ":";
echo mb_trim(" x ", ""), ":";
echo mb_str_pad("猫", 4, "犬", STR_PAD_BOTH), ":", mb_str_pad("猫", 3), ":";
echo mb_convert_kana("ﾊﾟﾋﾟﾌﾟ"), ":", mb_convert_kana("カタカナ", "c"), ":";
echo mb_substr_count("é猫é猫", "é猫"), "\n";
var_dump(mb_ord("\0"), mb_ord(chr(255)), mb_chr(-1), mb_chr(128512), mb_chr(8364, "ASCII"));
var_dump(mb_strpos("猫é猫É", "猫"), mb_strpos("猫é猫É", "犬"), mb_stripos("猫é猫É", "É"), mb_strrpos("猫é猫É", "猫"), mb_strripos("猫é猫É", "é", -2));
var_dump(mb_strstr("猫é猫É", "é", true), mb_strstr("猫é猫É", "犬"), mb_stristr("猫é猫É", "É"), mb_strrchr("猫é猫É", "猫"), mb_strrichr("猫é猫É", "é"), mb_strstr("", ""));
"#;
const SCALAR_OUTPUT: &str = r#"é日本B:é日:é日:本B:?é:猫:猫xx:xx猫: x :犬猫犬犬:猫  :パピプ:かたかな:2
int(0)
bool(false)
bool(false)
string(4) "😀"
bool(false)
int(0)
bool(false)
int(1)
int(2)
int(1)
string(3) "猫"
bool(false)
string(7) "é猫É"
string(5) "猫É"
string(2) "É"
string(0) ""
"#;

/// Verifies native substring, trimming, kana, search, and ordinal result semantics.
#[test]
fn test_mbstring_scalar_native_operations() {
    assert_eq!(compile_and_run(&format!("<?php namespace ScalarText; {SCALAR_PROGRAM}")), SCALAR_OUTPUT);
}

/// Verifies eval uses the same nullable lengths, boolean arguments, and scalar result tags.
#[test]
fn test_mbstring_scalar_eval_operations() {
    assert_eq!(compile_and_run(&opaque_eval(SCALAR_PROGRAM)), SCALAR_OUTPUT);
}

/// Wraps a PHP body in runtime-unknown eval source without changing its literal bytes.
fn opaque_eval(body: &str) -> String {
    let literal = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{literal}' : ''; eval($source);")
}

/// Verifies language and encoding settings are shared in both directions across AOT and eval.
#[test]
fn test_mbstring_scalar_settings_shared_with_eval() {
    let out = compile_and_run(r#"<?php
var_dump(Mb_Internal_Encoding(), Mb_Language(), Mb_Http_Output());
var_dump(mb_internal_encoding("ISO-8859-1"), mb_language("Japanese"), mb_http_output("pass"));
$source = $argc > 0 ? '
echo mb_internal_encoding(), ":", mb_language(), ":", mb_http_output(), ":";
echo mb_strlen("é"), ":";
mb_internal_encoding("UTF-8"); mb_language("neutral"); mb_http_output("UTF-16LE");
' : '';
eval($source);
echo mb_internal_encoding(null), ":", mb_language(null), ":", mb_http_output(null), ":", mb_strlen("é"), ":";
try { mb_language("invalid"); } catch (ValueError) { echo mb_language(), ":"; }
try { mb_internal_encoding("invalid"); } catch (ValueError) { echo mb_internal_encoding(), ":"; }
try { mb_http_output("pass\0tail"); } catch (ValueError) { echo mb_http_output(); }
"#);
    assert_eq!(out, "string(5) \"UTF-8\"\nstring(7) \"neutral\"\nstring(5) \"UTF-8\"\nbool(true)\nbool(true)\nbool(true)\nISO-8859-1:Japanese:pass:2:UTF-8:neutral:UTF-16LE:1:neutral:UTF-8:UTF-16LE");
}

/// Verifies named/spread and first-class calls preserve null, boolean, and integer arguments.
#[test]
fn test_mbstring_scalar_callable_contracts() {
    let out = compile_and_run(r#"<?php
namespace Text;
function length_option(bool $all): ?int { if ($all) { return null; } return 1; }
$cut = mb_substr(...);
echo $cut(...["encoding" => null, "length" => null, "string" => "a猫b", "start" => 1]), ":";
echo Mb_SuBsTr("a猫b", 1, length_option($argc > 0)), ":";
$find = mb_strstr(...);
var_dump($find(before_needle: true, needle: "猫", haystack: "a猫b", encoding: null));
$character = mb_chr(...);
var_dump($character(0));
$name = $argc > 0 ? "mb_chr" : "mb_ord";
var_dump(call_user_func($name, 29483));
$get = mb_internal_encoding(...);
var_dump($get(), $get(null));
"#);
    assert_eq!(out, "猫b:猫b:string(1) \"a\"\nstring(1) \"\0\"\nstring(3) \"猫\"\nstring(5) \"UTF-8\"\nstring(5) \"UTF-8\"\n");
}

/// Verifies PHP prechecks, post-encoding errors, and padding no-op validation order.
#[test]
fn test_mbstring_scalar_validation_order() {
    let body = r#"
try { mb_ord("", "invalid"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_substr_count("a", "", "invalid"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_substr("a", PHP_INT_MIN, null, "invalid"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_convert_kana("a", "!", "invalid"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_strpos("abc", "a", 9, "invalid"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_strpos("abc", "a", 9); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_str_pad("a", 3, "", 9); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
echo mb_str_pad("abc", 1, "", 9), ":", mb_trim(" x ", "");
"#;
    let expected = concat!(
        "mb_ord(): Argument #1 ($string) must not be empty\n",
        "mb_substr_count(): Argument #2 ($needle) must not be empty\n",
        "mb_substr(): Argument #2 ($start) must be between -9223372036854775807 and 9223372036854775807\n",
        "mb_convert_kana(): Argument #2 ($mode) contains invalid flag: '!'\n",
        "mb_strpos(): Argument #4 ($encoding) must be a valid encoding, \"invalid\" given\n",
        "mb_strpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n",
        "mb_str_pad(): Argument #3 ($pad_string) must not be empty\n",
        "abc: x "
    );
    assert_eq!(compile_and_run(&format!("<?php {body}")), expected);
    assert_eq!(compile_and_run(&opaque_eval(body)), expected);
}

/// Verifies declared union returns remain usable through typed functions, narrowing, and callables.
#[test]
fn test_mbstring_union_result_contracts() {
    let out = compile_and_run(r#"<?php
namespace TypedText;
function position(string $needle): int|false { return mb_strpos("a猫b", $needle); }
function ordinal(string $text): int|false { return mb_ord($text); }
function character(int $point): string|false { return mb_chr($point); }
function suffix(string $needle): string|false { return mb_strstr("a猫b", $needle); }
function encoding(): string|bool { return mb_internal_encoding(); }
$needle = $argc > 0 ? "猫" : "missing";
$position = position($needle);
if ($position !== false) { echo $position + 1, ":"; }
$tail = suffix($needle);
if ($tail !== false) { echo mb_strtoupper($tail), ":"; }
$character = character(29483);
if ($character !== false) { echo mb_strlen($character), ":"; }
$ordinal = ordinal("猫");
if ($ordinal !== false) { echo $ordinal + 1, ":"; }
$find = mb_strpos(...);
$found = $find("a猫b", $needle);
if ($found !== false) { echo $found + 2, ":"; }
var_dump(position("missing"), character(-1), suffix("missing"), encoding());
"#);
    assert_eq!(out, "2:猫B:1:29484:3:bool(false)\nbool(false)\nbool(false)\nstring(5) \"UTF-8\"\n");
}
