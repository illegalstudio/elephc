//! Purpose:
//! Interpreter tests for binary string conversion and escaping builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover byte-preserving string helpers and base64 conversion.

use super::super::*;
use super::support::*;

/// Verifies eval `strrev()` dispatches through direct and callable paths.
#[test]
fn execute_program_dispatches_strrev_builtin() {
    let program = parse_fragment(
        br#"echo strrev("Hello"); echo ":";
echo strrev(123); echo ":";
echo call_user_func("strrev", "ABC"); echo ":";
echo call_user_func_array("strrev", ["def"]); echo ":";
return function_exists("strrev");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "olleH:321:CBA:fed:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `grapheme_strrev()` reverses UTF-8 grapheme clusters.
#[test]
fn execute_program_dispatches_grapheme_strrev_builtin() {
    let program = parse_fragment(
        br#"echo grapheme_strrev("ABCDE"); echo ":";
echo bin2hex(grapheme_strrev(hex2bin("4165cc8142"))); echo ":";
echo bin2hex(grapheme_strrev(hex2bin("41f09f91a9f09f8fbde2808df09f92bb42"))); echo ":";
echo grapheme_strrev(chr(255)) === false ? "false" : "bad"; echo ":";
echo call_user_func("grapheme_strrev", "xy"); echo ":";
echo call_user_func_array("grapheme_strrev", ["string" => "pq"]); echo ":";
return function_exists("grapheme_strrev") && is_callable("grapheme_strrev");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "EDCBA:4265cc8141:42f09f91a9f09f8fbde2808df09f92bb41:false:yx:qp:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `chr()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_chr_builtin() {
    let program = parse_fragment(
        br#"echo chr(65); echo ":";
echo bin2hex(chr(codepoint: 256)); echo ":";
echo bin2hex(call_user_func("chr", 257)); echo ":";
echo call_user_func_array("chr", ["codepoint" => 321]); echo ":";
return function_exists("chr");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:00:01:A:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval gzip/zlib builtins round-trip strings and return false on invalid data.
#[test]
fn execute_program_dispatches_gzip_builtins() {
    let program = parse_fragment(
        br#"$data = "hello\0world";
$zlib = gzcompress($data);
echo gzuncompress($zlib) === $data ? "zc" : "bad"; echo ":";
$raw = gzdeflate($data);
echo gzinflate($raw) === $data ? "df" : "bad"; echo ":";
echo gzuncompress("not zlib") === false ? "bad-zlib" : "bad"; echo ":";
echo gzinflate("not raw deflate") === false ? "bad-raw" : "bad"; echo ":";
echo gzuncompress(gzcompress(data: "abc", level: 1)); echo ":";
echo call_user_func("gzuncompress", call_user_func("gzcompress", "call")); echo ":";
echo call_user_func_array("gzinflate", ["data" => call_user_func_array("gzdeflate", ["data" => "spread", "level" => 1])]); echo ":";
echo function_exists("gzcompress");
echo function_exists("gzdeflate");
echo function_exists("gzinflate");
return is_callable("gzuncompress");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "zc:df:bad-zlib:bad-raw:abc:call:spread:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `str_repeat()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_str_repeat_builtin() {
    let program = parse_fragment(
        br#"echo str_repeat("ha", 3); echo ":";
echo strlen(str_repeat(string: "x", times: 0)); echo ":";
echo call_user_func("str_repeat", "ab", 2); echo ":";
echo call_user_func_array("str_repeat", ["string" => "z", "times" => 3]); echo ":";
return function_exists("str_repeat");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "hahaha:0:abab:zzz:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `substr()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_substr_builtin() {
    let program = parse_fragment(
            br#"echo substr("abcdef", 2); echo ":";
echo substr(string: "abcdef", offset: 1, length: -1); echo ":";
echo substr("abcdef", -2); echo ":";
echo call_user_func("substr", "abcdef", 2, -2); echo ":";
echo call_user_func_array("substr", ["string" => "abcdef", "offset" => -4, "length" => 2]); echo ":";
return function_exists("substr");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "cdef:bcde:ef:cd:cd:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `substr_replace()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_substr_replace_builtin() {
    let program = parse_fragment(
            br#"echo substr_replace("hello world", "PHP", 6, 5); echo ":";
echo substr_replace(string: "abcdef", replace: "X", offset: 1, length: -1); echo ":";
echo substr_replace("abcdef", "X", -2); echo ":";
echo call_user_func("substr_replace", "abcdef", "X", 99, 1); echo ":";
echo call_user_func_array("substr_replace", ["string" => "abcdef", "replace" => "X", "offset" => -99, "length" => 2]); echo ":";
return function_exists("substr_replace");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "hello PHP:aXf:abcdX:abcdefX:Xcdef:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `nl2br()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_nl2br_builtin() {
    let program = parse_fragment(
        br#"echo bin2hex(nl2br("a\nb")); echo ":";
echo bin2hex(nl2br(string: "a\nb", use_xhtml: false)); echo ":";
echo bin2hex(call_user_func("nl2br", "a\r\nb")); echo ":";
echo bin2hex(call_user_func_array("nl2br", ["string" => "a\n\rb", "use_xhtml" => false])); echo ":";
return function_exists("nl2br");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "613c6272202f3e0a62:613c62723e0a62:613c6272202f3e0d0a62:613c62723e0a0d62:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `bin2hex()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_bin2hex_builtin() {
    let program = parse_fragment(
        br#"echo bin2hex("Az"); echo ":";
echo bin2hex(string: "A\n"); echo ":";
echo call_user_func("bin2hex", "!?"); echo ":";
echo call_user_func_array("bin2hex", ["string" => "ok"]); echo ":";
return function_exists("bin2hex");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "417a:410a:213f:6f6b:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `hex2bin()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_hex2bin_builtin() {
    let program = parse_fragment(
        br#"echo hex2bin("417a"); echo ":";
echo bin2hex(hex2bin(string: "410a")); echo ":";
echo call_user_func("hex2bin", "213f"); echo ":";
echo call_user_func_array("hex2bin", ["string" => "6f6b"]); echo ":";
echo hex2bin("4") ? "bad" : "false"; echo ":";
return function_exists("hex2bin");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Az:410a:!?:ok:false:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert_eq!(
        values.warnings,
        vec![HEX2BIN_ODD_LENGTH_WARNING.to_string()]
    );
}
/// Verifies eval slash escaping builtins use PHP byte-string semantics.
#[test]
fn execute_program_dispatches_slash_escape_builtins() {
    let program = parse_fragment(
        br#"$escaped = addslashes($source);
echo bin2hex($escaped); echo ":";
echo bin2hex(stripslashes($escaped)); echo ":";
echo call_user_func("addslashes", "x\"y"); echo ":";
echo call_user_func_array("stripslashes", [addslashes("o\"k")]); echo ":";
return function_exists("addslashes") && function_exists("stripslashes");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let source = values.string("a\0b\\c\"d'").expect("create source");
    scope.set("source", source, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "615c30625c5c635c22645c27:6100625c63226427:x\\\"y:o\"k:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval shell escaping is registry-visible through direct and callable dispatch.
#[test]
#[cfg(not(windows))]
fn execute_program_dispatches_posix_shell_escape_builtins() {
    let program = parse_fragment(
        br#"echo escapeshellarg("a'b"); echo ":";
echo escapeshellcmd("a&b"); echo ":";
echo call_user_func("escapeshellcmd", "x|y"); echo ":";
return function_exists("escapeshellarg") && function_exists("escapeshellcmd");"#,
    )
    .expect("parse eval shell escaping fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "'a'\\''b':a\\&b:x\\|y:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `base64_encode()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_base64_encode_builtin() {
    let program = parse_fragment(
        br#"echo base64_encode("Hello"); echo ":";
echo base64_encode(string: "Hi"); echo ":";
echo call_user_func("base64_encode", "Test 123!"); echo ":";
echo call_user_func_array("base64_encode", ["string" => ""]); echo ":";
return function_exists("base64_encode");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "SGVsbG8=:SGk=:VGVzdCAxMjMh::");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `base64_decode()` dispatches through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_base64_decode_builtin() {
    let program = parse_fragment(
        br#"echo base64_decode("SGVsbG8="); echo ":";
echo base64_decode(string: "SGk="); echo ":";
echo call_user_func("base64_decode", "VGVzdCAxMjMh"); echo ":";
echo call_user_func_array("base64_decode", ["string" => ""]); echo ":";
return function_exists("base64_decode");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Hello:Hi:Test 123!::");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
