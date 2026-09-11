//! Purpose:
//! Interpreter tests for string splitting, replacing, regex, entity, URL, ctype, and hash builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover byte-string and encoded string builtin behavior.

use super::super::*;
use super::support::*;

/// Keeps invalid UTF-8 literal bytes intact through ASCII case conversion and retained function defaults.
#[test]
fn string_literal_bytes_survive_case_conversion_and_defaults() {
    let program = parse_fragment(br#"function bytes_default(string $value = "\xFFAZ") { return $value; }
return strtolower(bytes_default());"#).unwrap();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).unwrap();
    assert_eq!(values.get(result), FakeValue::Bytes(vec![0xff, b'a', b'z']));
}

/// Verifies eval `mb_strlen()` matches encoding, malformed-byte, callable, and error behavior.
#[test]
fn execute_program_dispatches_mb_strlen_builtin() {
    let program = parse_fragment(
        r#"echo mb_strlen("abc"); echo ":";
	echo mb_strlen(string: "héllo", encoding: "8bit"); echo ":";
	echo mb_strlen("", null); echo ":";
	echo call_user_func("mb_strlen", "日本語"); echo ":";
	echo call_user_func_array("mb_strlen", ["string" => "héllo", "encoding" => "UTF-8"]); echo ":";
	echo mb_strlen(chr(128), "UTF-8"); echo ":";
	echo mb_strlen(chr(104) . chr(0) . chr(233) . chr(0), "UTF-16LE"); echo ":";
	try {
	    mb_strlen("abc", "definitely-not-an-encoding");
	} catch (ValueError $error) {
	    echo "caught";
	}
	echo ":";
	return function_exists("mb_strlen") && is_callable("mb_strlen");"#
            .as_bytes(),
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:6:0:3:5:1:2:caught:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies shared text dispatch uses the real engine with integer arguments and owned strings.
#[test]
fn execute_program_dispatches_mbstring_text_builtins() {
    let program = parse_fragment(r#"
echo mb_strtoupper("Straße"), ":", mb_strtolower("ΟΔΟΣ"), ":";
echo mb_convert_case("Straße", MB_CASE_FOLD), ":";
echo mb_strwidth("漢字abc"), ":";
echo mb_ucfirst("ßeta"), ":", mb_lcfirst("Éloïse"), ":";
echo mb_strimwidth("漢字abc", 1, 4, "!"), ":";
try { mb_convert_case("a", 9); }
catch (ValueError) { echo "caught"; }
"#.as_bytes()).expect("parse mbstring fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute mbstring fragment");
    assert_eq!(values.output, "STRASSE:οδος:strasse:7:Sseta:éloïse:字a!:caught");
}

/// Verifies eval `explode()` and `implode()` bridge byte strings and arrays.
#[test]
fn execute_program_dispatches_explode_implode_builtins() {
    let program = parse_fragment(
        br#"$parts = explode(",", "a,b,");
echo count($parts); echo ":" . $parts[0] . ":" . $parts[1] . ":" . $parts[2];
echo ":" . implode("|", $parts);
echo ":" . implode(separator: "-", array: ["x", 2, true, null]);
$call_parts = call_user_func("explode", ":", "m:n");
echo ":" . $call_parts[1];
echo ":" . call_user_func_array("implode", ["separator" => "/", "array" => ["p", "q"]]);
echo ":"; echo function_exists("explode");
return function_exists("implode");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:a:b::a|b|:x-2-1-:n:p/q:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `str_split()` builds indexed arrays of fixed-width chunks.
#[test]
fn execute_program_dispatches_str_split_builtin() {
    let program = parse_fragment(
        br#"$letters = str_split("abc");
echo count($letters) . ":" . $letters[0] . $letters[1] . $letters[2]; echo ":";
$pairs = str_split(string: "abcd", length: 2);
echo $pairs[0] . "-" . $pairs[1]; echo ":";
$empty = str_split("");
echo count($empty); echo ":";
$call = call_user_func("str_split", "xyz", 2);
echo $call[0] . "-" . $call[1]; echo ":";
$named = call_user_func_array("str_split", ["string" => "pqrs", "length" => 3]);
echo $named[0] . "-" . $named[1]; echo ":";
return function_exists("str_split");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:abc:ab-cd:0:xy-z:pqr-s:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `str_pad()` supports PHP left, right, and both-side padding modes.
#[test]
fn execute_program_dispatches_str_pad_builtin() {
    let program = parse_fragment(
            br#"echo "[" . str_pad("hi", 5) . "]"; echo ":";
echo "[" . str_pad(string: "hi", length: 5, pad_string: "_", pad_type: 0) . "]"; echo ":";
echo "[" . str_pad("x", 6, "ab", 2) . "]"; echo ":";
echo call_user_func("str_pad", "42", 5, "0", 0); echo ":";
echo call_user_func_array("str_pad", ["string" => "x", "length" => 3, "pad_string" => "."]); echo ":";
return function_exists("str_pad");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "[hi   ]:[___hi]:[abxaba]:00042:x..:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval string replacement builtins support direct, named, and callable dispatch.
#[test]
fn execute_program_dispatches_string_replace_builtins() {
    let program = parse_fragment(
            br#"echo str_replace("o", "0", "Hello World"); echo ":";
echo str_replace(search: "aa", replace: "b", subject: "aaaa"); echo ":";
echo str_replace("", "x", "abc"); echo ":";
echo str_ireplace("HE", "ye", "Hello he"); echo ":";
echo call_user_func("str_replace", "l", "L", "hello"); echo ":";
echo call_user_func_array("str_ireplace", ["search" => "x", "replace" => "Y", "subject" => "xX"]); echo ":";
echo function_exists("str_replace");
return function_exists("str_ireplace");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Hell0 W0rld:bb:abc:yello ye:heLLo:YY:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval regex builtins handle captures, replacement, callbacks, and splitting.
#[test]
fn execute_program_dispatches_preg_builtins() {
    let program = parse_fragment(
            br#"$ok = preg_match("/([a-z]+)([0-9]+)/", "id42", $matches);
echo $ok . ":" . count($matches) . ":" . $matches[0] . ":" . $matches[1] . ":" . $matches[2] . ":";
echo preg_match("/xyz/", "id42") . ":";
echo preg_match_all("/[0-9]+/", "a1b22c333") . ":";
$allCount = preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $all);
echo $allCount . ":" . count($all) . ":" . $all[0][1] . ":" . $all[1][0] . ":" . $all[2][1] . ":";
$setCount = preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $set, PREG_SET_ORDER);
echo $setCount . ":" . count($set) . ":" . $set[0][0] . ":" . $set[0][1] . ":" . $set[1][2] . ":";
preg_match("/(a)?(b)/", "b", $offsetOne, PREG_OFFSET_CAPTURE);
echo $offsetOne[0][0] . ":" . $offsetOne[0][1] . ":" . $offsetOne[1][0] . ":" . $offsetOne[1][1] . ":" . $offsetOne[2][0] . ":" . $offsetOne[2][1] . ":";
preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $offsetAll, PREG_OFFSET_CAPTURE);
echo $offsetAll[0][1][0] . ":" . $offsetAll[0][1][1] . ":" . $offsetAll[1][0][1] . ":" . $offsetAll[2][1][1] . ":";
preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $offsetSet, PREG_SET_ORDER | PREG_OFFSET_CAPTURE);
echo $offsetSet[1][0][0] . ":" . $offsetSet[1][0][1] . ":" . $offsetSet[0][2][1] . ":";
preg_match("/(a)?(b)(c)?/", "b", $nullOne, PREG_UNMATCHED_AS_NULL);
echo count($nullOne) . ":" . ($nullOne[1] === null ? "n" : "bad") . ":" . $nullOne[2] . ":" . ($nullOne[3] === null ? "n" : "bad") . ":";
preg_match("/(a)?(b)(c)?/", "b", $nullOffset, PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE);
echo ($nullOffset[1][0] === null ? "n" : "bad") . ":" . $nullOffset[1][1] . ":" . ($nullOffset[3][0] === null ? "n" : "bad") . ":" . $nullOffset[3][1] . ":";
preg_match_all("/(a)?(b)(c)?/", "b", $nullAll, PREG_UNMATCHED_AS_NULL);
echo ($nullAll[1][0] === null ? "n" : "bad") . ":" . $nullAll[2][0] . ":" . ($nullAll[3][0] === null ? "n" : "bad") . ":";
preg_match_all("/(a)?(b)(c)?/", "b", $nullSet, PREG_SET_ORDER | PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE);
echo ($nullSet[0][1][0] === null ? "n" : "bad") . ":" . $nullSet[0][1][1] . ":" . ($nullSet[0][3][0] === null ? "n" : "bad") . ":" . $nullSet[0][3][1] . ":";
preg_match_all("/(x)(y)/", "abc", $none);
echo count($none) . ":" . count($none[0]) . ":" . count($none[1]) . ":" . count($none[2]) . ":";
echo preg_replace("/([a-z])([0-9])/", "$2-$1", "a1 b2") . ":";
function eval_regex_wrap($matches) { return "[" . $matches[0] . "]"; }
echo preg_replace_callback("/[A-Z]/", "eval_regex_wrap", "AB") . ":";
$limited = preg_split("/,/", "a,b,c", 2);
echo count($limited) . ":" . $limited[0] . ":" . $limited[1] . ":";
$kept = preg_split("/,/", "a,,b", 0, PREG_SPLIT_NO_EMPTY);
echo count($kept) . ":" . $kept[1] . ":";
echo call_user_func("preg_match", "/x/", "x") . ":";
$replaced = call_user_func_array("preg_replace", ["pattern" => "/[0-9]+/", "replacement" => "N", "subject" => "a12"]);
echo $replaced . ":";
$captured = preg_split("/(,)/", "a,b", 0, PREG_SPLIT_DELIM_CAPTURE);
echo count($captured) . ":" . $captured[1] . ":";
$splitOffsets = preg_split("/,/", "a,b,c", 2, PREG_SPLIT_OFFSET_CAPTURE);
echo $splitOffsets[0][0] . ":" . $splitOffsets[0][1] . ":" . $splitOffsets[1][0] . ":" . $splitOffsets[1][1] . ":";
$splitBoth = preg_split("/(,)/", "a,b", 0, PREG_SPLIT_DELIM_CAPTURE | PREG_SPLIT_OFFSET_CAPTURE);
echo count($splitBoth) . ":" . $splitBoth[1][0] . ":" . $splitBoth[1][1] . ":";
$splitNoEmpty = preg_split("/,/", "a,,b", 0, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_OFFSET_CAPTURE);
echo $splitNoEmpty[1][0] . ":" . $splitNoEmpty[1][1] . ":";
class EvalPregMatchesBox {
    public array $matches = [];
    public static array $staticMatches = [];
}
$box = new EvalPregMatchesBox();
preg_match("/([a-z]+)([0-9]+)/", "ab12", $box->matches);
echo $box->matches[0] . ":" . $box->matches[1] . ":" . $box->matches[2] . ":";
$name = "matches";
preg_match_all("/([a-z])([0-9])/", "a1 b2", $box->{$name}, PREG_SET_ORDER);
echo count($box->matches) . ":" . $box->matches[1][0] . ":" . $box->matches[1][2] . ":";
preg_match("/([A-Z]+)/", "ID", EvalPregMatchesBox::$staticMatches);
echo EvalPregMatchesBox::$staticMatches[0] . ":";
$class = "EvalPregMatchesBox";
$staticName = "staticMatches";
preg_match_all("/([a-z])/", "xy", $class::${$staticName});
echo count(EvalPregMatchesBox::$staticMatches[0]) . ":" . EvalPregMatchesBox::$staticMatches[0][1] . ":";
return function_exists("preg_match") && function_exists("preg_match_all") && function_exists("preg_replace") && function_exists("preg_replace_callback") && function_exists("preg_split") && defined("PREG_SPLIT_NO_EMPTY") && defined("PREG_SET_ORDER") && defined("PREG_OFFSET_CAPTURE") && defined("PREG_SPLIT_OFFSET_CAPTURE") && defined("PREG_UNMATCHED_AS_NULL");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "1:3:id42:id:42:0:3:2:3:b22:a:22:2:2:a1:a:22:b:0::-1:b:0:b22:3:0:4:b22:3:1:4:n:b:n:n:-1:n:-1:n:b:n:n:-1:n:-1:3:0:0:0:1-a 2-b:[A][B]:2:a:b,c:2:b:1:aN:3:,:a:0:b,c:2:3:,:1:b:3:ab12:ab:12:2:b2:2:ID:2:y:"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies `preg_replace_callback()` accepts the same callable forms as other callback builtins.
#[test]
fn execute_program_preg_replace_callback_accepts_general_callables() {
    let program = parse_fragment(
        br#"class EvalPregCallbackBox {
    public $prefix = "";
    public function __construct($prefix) { $this->prefix = $prefix; }
    public function wrap($matches) { return $this->prefix . $matches[0]; }
    public static function wrapStatic($matches) { return "S" . $matches[0]; }
}
$box = new EvalPregCallbackBox("O");
echo preg_replace_callback("/[A-Z]/", [$box, "wrap"], "AB") . ":";
echo preg_replace_callback("/[0-9]/", "EvalPregCallbackBox::wrapStatic", "12") . ":";
echo preg_replace_callback("/[C]/", ["EvalPregCallbackBox", "wrapStatic"], "CC") . ":";
$first = $box->wrap(...);
echo preg_replace_callback("/[a-z]/", $first, "xy") . ":";
$static = EvalPregCallbackBox::wrapStatic(...);
return preg_replace_callback("/[m]/", $static, "mm");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "OAOB:S1S2:SCSC:OxOy:");
    assert_eq!(values.get(result), FakeValue::String("SmSm".to_string()));
}

/// Verifies dynamic preg callables write by-reference `$matches` targets.
#[test]
fn execute_program_dispatches_dynamic_preg_match_ref_targets() {
    let program = parse_fragment(
        br#"$match = "preg_match";
$ok = $match("/([a-z]+)([0-9]+)/", "id42", $matches);
echo $ok . ":" . $matches[0] . ":" . $matches[1] . ":" . $matches[2] . ":";
$matchAll = "preg_match_all";
$count = $matchAll("/([a-z])([0-9])/", "a1 b2", $all, PREG_SET_ORDER);
echo $count . ":" . $all[1][0] . ":" . $all[1][2] . ":";
$firstClass = preg_match(...);
$okAgain = $firstClass("/([A-Z]+)/", "ID", $firstClassMatches);
return $okAgain . ":" . $firstClassMatches[0];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:id42:id:42:2:b2:2:");
    assert_eq!(values.get(result), FakeValue::String("1:ID".to_string()));
}

/// Verifies named direct preg calls write by-reference `$matches` targets.
#[test]
fn execute_program_dispatches_named_preg_match_ref_targets() {
    let program = parse_fragment(
        br#"$named = [];
$ok = preg_match(pattern: "/([a-z]+)([0-9]+)/", subject: "id42", matches: $named);
echo $ok . ":" . $named[0] . ":" . $named[1] . ":" . $named[2] . ":";
$all = [];
$count = preg_match_all(pattern: "/([a-z])([0-9])/", subject: "a1 b2", matches: $all, flags: PREG_SET_ORDER);
echo $count . ":" . $all[1][0] . ":" . $all[1][2] . ":";
return preg_match(pattern: "/x/", subject: "x", flags: PREG_OFFSET_CAPTURE);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:id42:id:42:2:b2:2:");
    assert_eq!(values.get(result), FakeValue::Int(1));
}

/// Verifies eval HTML entity builtins encode, decode, and dispatch as callables.
#[test]
fn execute_program_dispatches_html_entity_builtins() {
    let program = parse_fragment(
        br#"echo htmlspecialchars("<b>\"Hi\" & 'bye'</b>"); echo ":";
echo htmlentities(string: "<a>"); echo ":";
echo html_entity_decode("&lt;b&gt;hi&lt;/b&gt;"); echo ":";
echo call_user_func("htmlspecialchars", "<x>"); echo ":";
echo call_user_func_array("html_entity_decode", ["string" => "&quot;q&quot;"]); echo ":";
echo function_exists("htmlspecialchars"); echo function_exists("htmlentities");
return function_exists("html_entity_decode");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "&lt;b&gt;&quot;Hi&quot; &amp; &#039;bye&#039;&lt;/b&gt;:&lt;a&gt;:<b>hi</b>:&lt;x&gt;:\"q\":11"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval URL codec builtins dispatch through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_url_codec_builtins() {
    let program = parse_fragment(
        br#"echo urlencode("a b&=~"); echo ":";
echo rawurlencode(string: "a b&=~"); echo ":";
echo urldecode("a+b%26%3D%7E"); echo ":";
echo rawurldecode("a+b%26%3D%7E"); echo ":";
echo call_user_func("urlencode", "%zz"); echo ":";
echo call_user_func_array("rawurldecode", ["string" => "x%2By%zz"]); echo ":";
echo function_exists("urlencode"); echo function_exists("rawurlencode");
echo function_exists("urldecode");
return function_exists("rawurldecode");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "a+b%26%3D%7E:a%20b%26%3D~:a b&=~:a+b&=~:%25zz:x+y%zz:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `parse_url()` arrays, component constants, callables, and `ValueError` behavior.
#[test]
fn execute_program_dispatches_parse_url_builtin() {
    let program = parse_fragment(
        br#"$all = parse_url("https://user:pass@example.com:8080/path?q=1#frag");
echo $all["scheme"] . ":" . $all["host"] . ":" . $all["port"] . ":";
echo $all["user"] . ":" . $all["pass"] . ":" . $all["path"] . ":";
echo $all["query"] . ":" . $all["fragment"] . ":";
echo parse_url("http://[::1]:80/", PHP_URL_HOST) . ":";
echo parse_url(url: "http://host", component: PHP_URL_PORT) === null ? "missing" : "bad"; echo ":";
echo parse_url("http://") === false ? "false" : "bad"; echo ":";
echo count(parse_url("/path", -2)); echo ":";
echo call_user_func("parse_url", "//callable/path", PHP_URL_HOST); echo ":";
echo call_user_func_array("parse_url", ["url" => "mailto:a@b", "component" => PHP_URL_PATH]); echo ":";
try {
    parse_url("x", 8);
} catch (ValueError $error) {
    echo $error->getMessage();
}
echo ":"; echo function_exists("parse_url"); echo defined("PHP_URL_FRAGMENT");
return PHP_URL_FRAGMENT;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "https:example.com:8080:user:pass:/path:q=1:frag:[::1]:missing:false:1:callable:a@b:\
parse_url(): Argument #2 ($component) must be a valid URL component identifier, 8 given:11"
    );
    assert_eq!(values.get(result), FakeValue::Int(EVAL_PHP_URL_FRAGMENT));
}

/// Verifies eval incremental hash context builtins use elephc-crypto state.
#[test]
fn execute_program_dispatches_hash_context_builtins() {
    let program = parse_fragment(
        br#"$ctx = hash_init("sha256");
echo is_resource($ctx) ? "ctx" : "bad"; echo ":";
echo get_resource_type($ctx) === "stream" ? "rtype" : "bad"; echo ":";
echo hash_update($ctx, "ab") ? "up1" : "bad"; echo ":";
$copy = hash_copy($ctx);
echo hash_update($ctx, "c") ? "up2" : "bad"; echo ":";
echo hash_update($copy, "d") ? "upcopy" : "bad"; echo ":";
echo hash_final($ctx); echo ":";
echo hash_final($copy); echo ":";
$raw = call_user_func("hash_init", "md5");
hash_update(context: $raw, data: "abc");
echo bin2hex(call_user_func("hash_final", $raw, true)); echo ":";
$named = call_user_func_array("hash_init", ["algo" => "sha1"]);
call_user_func_array("hash_update", ["context" => $named, "data" => "abc"]);
echo call_user_func_array("hash_final", ["context" => $named]); echo ":";
echo function_exists("hash_init"); echo function_exists("hash_update");
echo function_exists("hash_final"); echo function_exists("hash_copy");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "ctx:rtype:up1:up2:upcopy:",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad:",
            "a52d159f262b2c6ddb724a61840befc36eb30c88877a4030b65cbe86298449c9:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "a9993e364706816aba3e25717850c26c9cd0d89d:",
            "1111"
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval hash contexts consume NO PHP resource id, so later resources keep theirs.
///
/// PHP 8's `hash_init()`/`hash_copy()` return `HashContext` OBJECTS. Objects and resources
/// are unrelated numbering spaces in php-src, so a hash context takes nothing from the
/// counter `get_resource_id()` reports:
///
/// ```text
/// eval('hash_init("md5"); hash_copy($h); $x = fopen(...);') -> get_resource_id($x) === 5
/// ```
///
/// The interpreter used to box its context with `RuntimeValueOps::resource`, which binds an
/// id, so each context stole one and this program reported 7 instead of 5. `hash_context()`
/// boxes resource kind 5 instead — id-less and destructor-less.
///
/// `stream_context_create()` supplies the trailing GENUINE resource because it needs no
/// filesystem, and it doubles as the control: if the fix had disabled id binding wholesale
/// rather than only for hash contexts, `$sc` would report nothing sensible here.
///
/// NOTE FOR THE NEXT READER: this assertion is only meaningful because `FakeOps` models
/// inert payloads (`inert_resources`). The fake binds an id for EVERY `FakeValue::Resource`
/// in `alloc`, so a `hash_context()` that merely forwarded to `runtime_resource` would keep
/// this test — and every other magician test — structurally unable to see the defect.
#[test]
fn execute_program_hash_contexts_consume_no_resource_id() {
    let program = parse_fragment(
        br#"$ctx = hash_init("md5");
$copy = hash_copy($ctx);
$again = hash_init("sha1");
$sc = stream_context_create();
echo get_resource_id($sc);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert_eq!(
        values.resource_ids.len(),
        1,
        "only the stream context may hold a PHP resource id: {:?}",
        values.resource_ids
    );
    assert_eq!(
        values.inert_resources.len(),
        3,
        "both hash_init() calls and the hash_copy() must be inert: {:?}",
        values.inert_resources
    );
}

/// Verifies eval hash contexts stay usable after being boxed as inert kind-5 cells.
///
/// Guards the other half of the change. The id fix must not be bought by degrading the
/// context: the low payload word is still the `EvalStreamResources` key, so
/// `eval_resource_payload` must resolve it and the digests must stay exact. A `hash_copy()`
/// that shared state with its source, or a key that resolved to the wrong slot, would show
/// up as a changed digest rather than a changed id.
#[test]
fn execute_program_inert_hash_contexts_still_hash_correctly() {
    let program = parse_fragment(
        br#"$a = hash_init("md5");
hash_update($a, "abc");
$b = hash_copy($a);
hash_update($b, "def");
echo hash_final($a); echo ":";
echo hash_final($b);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "900150983cd24fb0d6963f7d28e17f72:e80b5017098950fc58aad83c8c14978e"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `ctype_*` predicates dispatch through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_ctype_builtins() {
    let program = parse_fragment(
        br#"echo ctype_alpha("abc") ? "A" : "-"; echo ":";
echo ctype_digit(text: "123") ? "D" : "-"; echo ":";
echo ctype_alnum("a1") ? "N" : "-"; echo ":";
echo ctype_space(" \t\n" . chr(11) . chr(12) . "\r") ? "S" : "-"; echo ":";
echo ctype_alpha("") ? "bad" : "empty"; echo ":";
echo call_user_func("ctype_digit", "12x") ? "bad" : "not-digit"; echo ":";
echo call_user_func_array("ctype_space", ["text" => " x"]) ? "bad" : "not-space"; echo ":";
echo function_exists("ctype_alpha"); echo function_exists("ctype_digit");
echo function_exists("ctype_alnum");
return function_exists("ctype_space");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:D:N:S:empty:not-digit:not-space:111");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `crc32()` returns PHP-compatible non-negative checksums.
#[test]
fn execute_program_dispatches_crc32_builtin() {
    let program = parse_fragment(
            br#"echo crc32(""); echo ":";
echo crc32(string: "123456789"); echo ":";
echo call_user_func("crc32", "hello"); echo ":";
echo call_user_func_array("crc32", ["string" => "The quick brown fox jumps over the lazy dog"]); echo ":";
return function_exists("crc32");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "0:3421780262:907060870:1095738169:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `hash_algos()` returns supported hash names through callable dispatch too.
#[test]
fn execute_program_dispatches_hash_algos_builtin() {
    let program = parse_fragment(
        br#"$algos = hash_algos();
echo count($algos) . ":" . $algos[0] . ":" . $algos[5] . ":";
echo in_array("crc32c", $algos) ? "crc" : "bad";
$call = call_user_func("hash_algos");
echo ":" . $call[18];
$spread = call_user_func_array("hash_algos", []);
echo ":" . $spread[27] . ":";
echo function_exists("hash_algos") ? "exists" : "missing";
return count($algos);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "28:md2:sha256:crc:whirlpool:joaat:exists");
    assert_eq!(values.get(result), FakeValue::Int(28));
}
/// Verifies eval one-shot hash digest builtins use the crypto bridge and dispatch dynamically.
#[test]
fn execute_program_dispatches_hash_digest_builtins() {
    let filename = format!("elephc_magician_hash_file_{}.txt", std::process::id());
    let source = format!(
        r#"echo md5("abc"); echo ":";
echo sha1(string: "abc"); echo ":";
echo hash("sha256", "abc"); echo ":";
echo hash_hmac(algo: "sha256", data: "data", key: "key"); echo ":";
echo bin2hex(md5("abc", true)); echo ":";
echo bin2hex(call_user_func("sha1", "abc", true)); echo ":";
echo call_user_func_array("hash", ["algo" => "md5", "data" => "abc"]); echo ":";
echo call_user_func_array("hash_hmac", ["algo" => "sha256", "data" => "data", "key" => "key"]); echo ":";
file_put_contents("{filename}", "abc");
echo hash_file("sha256", "{filename}"); echo ":";
echo bin2hex(hash_file(algo: "md5", filename: "{filename}", binary: true)); echo ":";
echo call_user_func_array("hash_file", ["algo" => "md5", "filename" => "{filename}"]); echo ":";
echo hash_file("sha256", "{filename}.missing") === false ? "missing" : "bad"; echo ":";
unlink("{filename}");
echo function_exists("md5"); echo function_exists("sha1"); echo function_exists("hash"); echo function_exists("hash_file");
return function_exists("hash_hmac");"#,
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "900150983cd24fb0d6963f7d28e17f72:",
            "a9993e364706816aba3e25717850c26c9cd0d89d:",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad:",
            "5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "a9993e364706816aba3e25717850c26c9cd0d89d:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0:",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "missing:",
            "1111"
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies scalar bridge defaults, nullable lengths, booleans, and request-setting mutations.
#[test]
fn execute_program_dispatches_mbstring_scalar_builtins() {
    elephc_mbstring::abi::elephc_mbstring_reset_v1();
    let program = parse_fragment(r#"
echo mb_substr("a猫b", 1, null), ":", mb_strcut("a猫b", 2, 3), ":";
echo mb_strstr("a猫b", "猫", true), ":", mb_convert_kana("ﾊﾟ"), ":";
echo mb_ord(chr(255)) === false ? "invalid:" : "bad:";
echo mb_chr(29483), ":", mb_str_pad("猫", 2), ":";
echo mb_internal_encoding("ISO-8859-1") ? "set:" : "bad:";
echo mb_strlen("é"), ":", mb_internal_encoding(null), ":";
mb_internal_encoding("UTF-8");
return mb_strpos("a猫b", "missing") === false;
"#.as_bytes()).expect("parse mbstring scalar fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute mbstring scalar fragment");
    assert_eq!(values.output, "猫b:猫:a:パ:invalid:猫:猫 :set:2:ISO-8859-1:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies the fake adapter materializes the real engine's binary array results and metadata.
#[test]
fn execute_program_dispatches_mbstring_array_builtins() {
    let program = parse_fragment(r#"
$parts = mb_str_split(chr(0) . chr(255) . chr(128), 2, "8bit");
echo count($parts), ":", bin2hex($parts[0]), ":", bin2hex($parts[1]), ":";
$copy = $parts; $copy[0] = "changed";
echo bin2hex($parts[0]), ":";
$aliases = mb_encoding_aliases("ASCII");
echo count($aliases), ":", $aliases[0], ":", mb_preferred_mime_name("ASCII");
return mb_str_split("");
"#.as_bytes()).expect("parse mbstring array fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute mbstring array fragment");
    assert_eq!(values.output, "2:00ff:80:00ff:11:ANSI_X3.4-1968:US-ASCII");
    assert_eq!(values.get(result), FakeValue::Array(Vec::new()));
}

/// Verifies eval array inputs reach the real engine without conversion into the string Array.
#[test]
fn execute_program_dispatches_mbstring_check_encoding() {
    let program = parse_fragment(r#"
$valid = ["names" => ["猫", "é"]];
$invalid = [chr(255) => "ok"];
echo (int)mb_check_encoding($valid), ":", (int)mb_check_encoding($invalid), ":";
echo (int)mb_check_encoding([true, null, 42]), ":";
return mb_check_encoding([chr(255)], "8bit");
"#.as_bytes()).expect("parse mbstring array-check fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute mbstring array-check fragment");
    assert_eq!(values.output, "1:0:1:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Preserves invalid UTF-8 keys across fake array updates, regular sorting, and iteration.
#[test]
fn execute_program_preserves_binary_array_keys() {
    let program = parse_fragment(r#"
$items = [chr(255) => 1, chr(254) => 2];
$items[chr(255)] = 3;
ksort($items);
echo count($items), ":";
foreach ($items as $key => $value) { echo bin2hex($key), "=", $value, ":"; }
return mb_check_encoding($items);
"#.as_bytes()).expect("parse binary array-key fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute binary array-key fragment");
    assert_eq!(values.output, "2:fe=2:ff=3:");
    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// Sends real integer and string union arguments through the shared substitution C dispatch.
#[test]
fn execute_program_dispatches_mbstring_substitution() {
    let program = parse_fragment(r#"
mb_substitute_character(33);
echo mb_substitute_character(), ":", bin2hex(mb_scrub(chr(255))), ":";
mb_substitute_character("none");
echo mb_substitute_character(), ":", bin2hex(mb_scrub(chr(255))), ":";
mb_substitute_character("long");
echo bin2hex(mb_scrub(chr(255)));
return mb_substitute_character(63);
"#.as_bytes()).expect("parse substitution-setting fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute substitution-setting fragment");
    assert_eq!(values.output, "33:21:none::21");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
