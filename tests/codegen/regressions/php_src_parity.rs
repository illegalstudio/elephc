//! Purpose:
//! php-src-derived regression coverage for the array, sort, string and JSON paths optimized
//! by the native-speed branch.
//!
//! Called from:
//! - `cargo test` through the codegen test harness.
//!
//! Key details:
//! - Cases are ported from PHP 8.5 php-src `.phpt` files, adapted only where elephc rejects a
//!   pre-existing unsupported form (reference elements in array literals, scalar coercion in
//!   `str_repeat`). Those omissions are called out at the case that needs them.
//! - Each fixture runs with heap debug so ownership regressions fail with a leak report.

use crate::support::*;

/// Ports `ext/standard/tests/array/array_sum*.phpt` and the fused `array_map` path.
///
/// Covers integer/float/mixed arrays, empty arrays, numeric strings, octal/hex literals,
/// duplicates, associative keys, booleans, nulls, a 64-bit-value int array, and positive/negative/
/// overflow-promoted fused map sums. `array_sum_on_reference.phpt` is represented only by
/// its numeric-value behavior: elephc rejects a reference element inside an array literal.
#[test]
fn php_src_array_sum_and_fused_map_match_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$int = [1, 2, 3, 4, 5];
$float = [1.0, 2.2, 3.4, 4.6];
$mixed = [1, 2.3, 4, 0.6, 10];
echo array_sum($int), '|', array_sum($float), '|', array_sum($mixed), '|', array_sum([]), '|';
echo array_sum([3, 2, 100, 150, 25, 350, 0, -3, -1200]), '|';
echo array_sum([056, 023, 00, 015, -045, 01, -07]), '|';
echo array_sum([0xAE, 0x2B, 0X10, -0xCF, 0X12, -0XF2]), '|';
echo array_sum([2.3, 1.9, -4.1, 0.5, 1.9, -4.1, 3.6, 0.5]), '|';
echo array_sum(['a' => 20, "b" => 50, 'c' => 0, 'd' => -30, "e" => 100]), '|';
echo array_sum([true, true, false, true, false]), '|';
echo array_sum([null, null]), '|';
$ints = [];
$strings = [];
for ($i = 1; $i <= 1000; $i++) { $ints[] = $i; $strings[] = (string) $i; }
echo array_sum($ints), '|', array_sum($strings), '|';
echo array_sum([3000000000, 2000000000, 123456789]), '|';
$src = range(1, 5);
echo array_sum(array_map(fn($x) => $x + 1, $src)), '|';
$neg = [1, 2, 3];
echo array_sum(array_map(fn($x) => $x - 10, $neg)), '|';
$ov = range(PHP_INT_MAX, PHP_INT_MAX);
$z = 0;
$z ^= array_sum(array_map(fn($x) => $x + 1, $ov));
echo $z, "|\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "15|11.2|17.9|0|-573|35|-198|2.5|140|3|0|500500|500500|5123456789|20|-24|-9223372036854775808|\n"
    );
    assert!(
        out.stderr
            .contains("The float 9.223372036854776E+18 is not representable as an int"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Ports numeric `sort_basic`/`rsort_basic` cases and the case-conversion/str_repeat variants.
///
/// `strtolower.phpt` and `strtoupper1.phpt` exercise every byte 0..255 plus the documented
/// basic strings; `str_repeat.phpt` and `str_repeat_variation1.phpt` cover zero/positive
/// counts, binary payloads, the negative-count `ValueError`, and the constant length fold.
/// Scalar `str_repeat` coercion is a pre-existing unsupported form and is not asserted here.
#[test]
fn php_src_sort_case_and_repeat_match_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$u = [100, 33, 555, 22];
sort($u);
echo implode(',', $u), '|';
rsort($u);
echo implode(',', $u), '|';
$descending = range(100, 1);
sort($descending);
echo $descending[0], ',', $descending[99], '|';
$ascending = range(1, 100);
rsort($ascending);
echo $ascending[0], ',', $ascending[99], '|';
$duplicates = [5, 5, 3, 3, 1];
sort($duplicates);
echo implode(',', $duplicates), '|';
$bad = 0;
for ($i = 0; $i <= 255; $i++) {
    $char = chr($i);
    $lower = strtolower($char);
    $upper = strtoupper($char);
    if (strlen($lower) !== 1 || strlen($upper) !== 1) { $bad++; }
    if ($i >= 65 && $i <= 90) {
        if ($lower !== chr($i + 32)) { $bad++; }
    } elseif ($i >= 97 && $i <= 122) {
        if ($upper !== chr($i - 32)) { $bad++; }
    } elseif ($lower !== $char || $upper !== $char) {
        $bad++;
    }
}
echo $bad, '|';
echo strtolower("Mary Had A liTTle LAmb and ShE loveD IT So"), '|';
echo bin2hex(strtolower("ABCD\0abcdABCD")), '|';
echo bin2hex(strtoupper("abcd\0ABCD")), '|';
echo strlen(strtolower("Hello Wörld")), '|', strlen(strtoupper("Hello Wörld")), '|';
echo bin2hex(str_repeat("a", 3)), '|';
echo str_repeat("foo", 3), '|';
echo bin2hex(str_repeat("\x00", 3)), '|';
echo bin2hex(str_repeat(chr(0) . chr(128) . chr(129) . chr(234) . chr(235) . chr(254) . chr(255), 3)), '|';
$s = "ab";
echo strlen(str_repeat($s, 32)), '|';
try { str_repeat("x", -1); } catch (ValueError $e) { echo $e->getMessage(), '|'; }
echo "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "22,33,100,555|555,100,33,22|1,100|100,1|1,3,3,5,5|0|\
         mary had a little lamb and she loved it so|\
         61626364006162636461626364|414243440041424344|12|12|\
         616161|foofoofoo|000000|008081eaebfeff008081eaebfeff008081eaebfeff|64|\
         str_repeat(): Argument #2 ($times) must be greater than or equal to 0|\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Ports the count-only `json_decode` observations from php-src JSON tests.
///
/// Covers a normal object root, the default depth-512 rejection, invalid UTF-8, escaped
/// and duplicate keys, numeric-looking keys, and the >64-key decoder fallback.
#[test]
fn php_src_json_decode_count_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$obj = '{"a":1,"b":[1,2,3],"c":"text"}';
echo count(json_decode($obj, true)), '|';
$deep = str_repeat('[', 513) . str_repeat(']', 513);
try { count(json_decode($deep, true)); } catch (TypeError $e) { echo 'deep:', $e->getMessage(), '|'; }
$badUtf8 = "{\"a\":\"\xC3\x28\"}";
try { count(json_decode($badUtf8, true)); } catch (TypeError $e) { echo 'utf8:', $e->getMessage(), '|'; }
$escaped = '{"a\u0062":1,"x":2}';
echo count(json_decode($escaped, true)), '|';
$duplicate = '{"a":1,"a":2,"b":3}';
echo count(json_decode($duplicate, true)), '|';
$numeric = '{"1":1,"01":2}';
echo count(json_decode($numeric, true)), '|';
$large = '{' . implode(',', array_map(fn($i) => '"k' . $i . '":1', range(1, 100))) . '}';
echo count(json_decode($large, true)), '|';
echo "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "3|deep:count(): Argument #1 ($value) must be of type Countable|array, null given|\
         utf8:count(): Argument #1 ($value) must be of type Countable|array, null given|\
         2|2|2|100|\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Ports the scalar/search/JSON-encode and fused filter/reduce cases from php-src.
///
/// `strpos.phpt` supplies the offset/NUL/empty-needle/false cases, `sqrt.phpt` the
/// integer truncation and checked-add promotion cases, `json_encode_*` the scalar/array/
/// escaping/flags cases, `array_filter` the modulo-count cases (including a negative
/// divisor and DivisionByZeroError), and `array_reduce` the empty/overflow cases.
#[test]
fn php_src_strpos_sqrt_json_encode_filter_reduce_match_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo (int)strpos("Hello World", "o"), '|';
echo (int)strpos("Hello World", "o", 5), '|';
echo (int)strpos("Hello World", "x"), '|';
echo var_export(strpos("Hello World", "x"), true), '|';
echo (int)strpos("a\0b", "b"), '|';
echo (int)strpos("abc", ""), '|';
echo (int)strpos("abc", "c", -1), '|';
echo (int)sqrt(9), '|', (int)sqrt(2), '|', (int)sqrt(PHP_INT_MAX), '|';
$a = PHP_INT_MAX;
$b = 1;
echo (int)sqrt($a + $b), '|';
echo json_encode([1, 2, 3]), '|';
echo json_encode(["a" => "x", "b" => true, "c" => null]), '|';
echo json_encode("é"), '|';
echo json_encode("a/b"), '|';
echo json_encode(1.5), '|';
echo json_encode([]), '|';
echo json_encode(["a" => "x"], JSON_UNESCAPED_SLASHES), '|';
echo strlen(json_encode(["a" => 1, "b" => [1, 2, 3], "c" => "text"])), '|';
$ints = range(1, 10);
echo count(array_filter($ints, fn($x) => $x % 2 === 0)), '|';
$empty = [];
echo count(array_filter($empty, fn($x) => $x % 2 === 0)), '|';
$threes = range(1, 10);
echo count(array_filter($threes, fn($x) => $x % 3 === 0)), '|';
$negs = range(1, 10);
echo count(array_filter($negs, fn($x) => $x % -3 === 0)), '|';
try { count(array_filter($ints, fn($x) => $x % 0 === 0)); } catch (DivisionByZeroError $e) { echo 'div0|'; }
$r = range(1, 100);
echo array_reduce($r, fn($c, $x) => $c + $x, 0), '|';
echo array_reduce($empty, fn($c, $x) => $c + $x, 7), '|';
$ovr = [PHP_INT_MAX, PHP_INT_MAX];
$v = array_reduce($ovr, fn($c, $x) => $c + $x, 0);
echo is_float($v) ? 'float|' : 'int|';
echo md5(""), '|';
echo md5("abc"), '|';
echo strlen(md5("abc")), '|';
echo bin2hex(md5("abc", true)), '|';
echo base64_encode(""), '|';
echo base64_encode("abc"), '|';
echo strlen(base64_encode("abcdef")), '|';
echo strlen(base64_encode("abc")), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "4|7|0|false|2|0|2|3|1|3037000499|3037000499|[1,2,3]|\
         {\"a\":\"x\",\"b\":true,\"c\":null}|\"\\u00e9\"|\"a\\/b\"|1.5|[]|\
         {\"a\":\"x\"}|30|5|0|3|3|div0|5050|7|float|\
         d41d8cd98f00b204e9800998ecf8427e|900150983cd24fb0d6963f7d28e17f72|32|\
         900150983cd24fb0d6963f7d28e17f72||YWJj|8|4\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Ports php-src search/translate/split/case edge cases for the String batch.
///
/// Covers `stripos_basic1`/`stripos_variation*` first and repeated matches, `strrpos_*`
/// last-match/empty/negative cases, `strstr.phpt` hit/prefix/miss, `strtr_basic` plus the
/// replacement-pair form, `chunk_split_*` defaults/empty/invalid length, and the
/// `ucwords`/`ucfirst`/`lcfirst` non-ASCII and default-separator variants.
#[test]
fn php_src_string_search_translate_split_case_match_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo stripos("Hello World", "WORLD"), '|';
echo stripos("ABCabc", "abc"), '|';
echo strrpos("aAaA", "aA"), '|';
echo strrpos("abc", ""), '|';
echo strstr("hello", "ll"), '|';
echo strstr("hello", "ll", true), '|';
var_export(strstr("hello", "zz")); echo '|';
echo strtr("hi all", "hi", "ib"), '|';
echo strtr("abc", ["a" => "1", "b" => "2"]), '|';
echo json_encode(chunk_split("abcdef", 2, "-")), '|';
echo json_encode(chunk_split("", 3, "-")), '|';
echo json_encode(ucwords("foo\tbar\rbaz\fbat\vqux")), '|';
echo ucfirst("élan"), '|';
echo lcfirst("Élan"), '|';
try { chunk_split("abc", 0); } catch (ValueError $e) { echo 'VE'; }
echo "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "6|0|2|3|llo|he|false|ib all|12c|\"ab-cd-ef-\"|\"-\"|\"Foo\\tBar\\rBaz\\fBat\\u000bQux\"|élan|Élan|VE\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Ports php-src URL, Base64, count_chars, CRC32 and str_split edge cases.
///
/// Covers `ext/standard/tests/url/urls.inc` selected `parse_url_basic_*` rows plus
/// `parse_url_relative_scheme`, `base64_decode_basic_001/002`, `count_chars.phpt`,
/// `count_chars_basic.phpt`, `crc32_basic.phpt`, `crc32_variation2/3/4.phpt`, and
/// `str_split_basic.phpt` with `str_split_variation3..7`. The URL loop also exercises the
/// `count(parse_url())` sink; the Base64 loop exercises strict/lax decoding and its length
/// sink; `count_chars($s, 1)` and `count(str_split())` cover their count sinks.
#[test]
fn php_src_url_base64_count_chars_crc32_split_match_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$urls = [
    '64.246.30.37',
    'http://64.246.30.37',
    'php.net:80/',
    'http://www.php.net:80/index.php?foo&',
    'http://secret:hideout@www.php.net:80/index.php?test=1&test2=char#ref',
    'zlib:http://foo@bar',
    'mailto:me@mydomain.com',
    '/foo.php?a=b&c=d',
    'file:///path/to/file',
    'http://x:?',
    'http://[x:80]/',
    'http:///blah.com',
    'http://blah.com:123456',
];
foreach ($urls as $u) { echo json_encode(parse_url($u), JSON_UNESCAPED_SLASHES), '|'; }
echo "\n";
echo json_encode(parse_url('http://secret:hideout@www.php.net:80/index.php?x=1#f', PHP_URL_HOST), JSON_UNESCAPED_SLASHES), '|';
echo json_encode(parse_url('//example.org'), JSON_UNESCAPED_SLASHES), '|';
echo count(parse_url('https://user:pass@example.com:8080/path?q=1#frag')), '|';
echo count(parse_url('http://example.com')), "\n";

$b64 = [
    'aGVsbG8gd29ybGQh',
    "a GVs   bG8gd2\n 9ybGQh",
    'aGVsbG8gd29ybGQh*',
    'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/VV==',
    'UEhQIGlzIGEgcG9wdWxhciBnZW5lcmFsLXB1cnBvc2Ugc2NyaXB0aW5nIGxhbmd1YWdlIHRoYXQgaXMgZXNwZWNpYWxseSBzdWl0ZWQgdG8gd2ViIGRldmVsb3BtZW50',
];
foreach ($b64 as $c) { echo bin2hex(base64_decode($c)), '|'; }
foreach ($b64 as $c) { $r = base64_decode($c, true); echo $r === false ? 'F' : bin2hex($r), '|'; }
echo strlen(base64_decode($b64[4])), "\n";

$s = 'het leven is net erwtensoep - je kunt er geen touw aan vastknopen';
echo implode(',', count_chars($s, 1)), '|';
echo count(count_chars($s, 1)), '|';
echo count_chars($s, 3), '|';
echo strlen(count_chars($s, 4)), '|';
$bin = "\x00\x00\xff\xffabc";
echo count(count_chars($bin, 1)), '|';
echo implode(',', count_chars($bin, 1)), "\n";

foreach (['', ' ', 'hello world', 'HELLO WORLD', 'string_val1234', "abc\0def"] as $c) { echo crc32($c), '|'; }
echo "\n";

$vals = ['', ' ', '1234', 'simple string', "string\twith\rws\nchars", "a\0b"];
foreach ($vals as $v) { echo json_encode(str_split($v, 3)), '|'; }
echo json_encode(str_split('This is basic testcase', 5)), '|';
echo count(str_split(str_repeat('a', 26), 5)), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        r#"{"path":"64.246.30.37"}|{"scheme":"http","host":"64.246.30.37"}|{"host":"php.net","port":80,"path":"/"}|{"scheme":"http","host":"www.php.net","port":80,"path":"/index.php","query":"foo&"}|{"scheme":"http","host":"www.php.net","port":80,"user":"secret","pass":"hideout","path":"/index.php","query":"test=1&test2=char","fragment":"ref"}|{"scheme":"zlib","path":"http://foo@bar"}|{"scheme":"mailto","path":"me@mydomain.com"}|{"path":"/foo.php","query":"a=b&c=d"}|{"scheme":"file","path":"/path/to/file"}|{"scheme":"http","host":"x","query":""}|{"scheme":"http","host":"[x:80]","path":"/"}|false|false|
"www.php.net"|{"host":"example.org"}|8|2
68656c6c6f20776f726c6421|68656c6c6f20776f726c6421|68656c6c6f20776f726c6421|00108310518720928b30d38f41149351559761969b71d79f8218a39259a7a29aabb2dbafc31cb3d35db7e39ebbf3dfbf55|504850206973206120706f70756c61722067656e6572616c2d707572706f736520736372697074696e67206c616e6775616765207468617420697320657370656369616c6c792073756974656420746f2077656220646576656c6f706d656e74|68656c6c6f20776f726c6421|68656c6c6f20776f726c6421|F|00108310518720928b30d38f41149351559761969b71d79f8218a39259a7a29aabb2dbafc31cb3d35db7e39ebbf3dfbf55|504850206973206120706f70756c61722067656e6572616c2d707572706f736520736372697074696e67206c616e6775616765207468617420697320657370656369616c6c792073756974656420746f2077656220646576656c6f706d656e74|96
12,1,3,12,1,1,1,1,2,1,8,3,2,2,3,6,2,2,2|19| -aeghijklnoprstuvw|237|5|2,1,1,1,2
0|3916222277|222957957|2279966299|256895812|3632661392|
[]|[" "]|["123","4"]|["sim","ple"," st","rin","g"]|["str","ing","\twi","th\r","ws\n","cha","rs"]|["a\u0000b"]|["This ","is ba","sic t","estca","se"]|6
"#,
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Ports php-src `strip_tags_array.phpt` and the allow-list rows from `strip_tags_basic2.phpt`.
///
/// The array form joins keys/values into `<...>` and ignores value truthiness, matching PHP
/// 8.5: `['p' => true, 'a' => false]` keeps both `p` and `a`, and `['p' => 'a']` keeps the
/// key name rather than its value. The length-only sink is covered separately by the
/// optimizer suite, so these calls keep asserting the emitted tag bytes.
#[test]
fn php_src_strip_tags_array_and_allow_list_match_php() {
    let out = compile_and_run_with_heap_debug(
        r##"<?php
$string = '<p>foo <b>bar</b> <a href="#">foobar</a></p>';
var_dump(strip_tags($string));
var_dump(strip_tags($string, ['a']));
var_dump(strip_tags($string, ['p', 'a']));
var_dump(strip_tags($string, []));
var_dump(strip_tags($string, ['p' => true, 'a' => false]));
var_dump(strip_tags($string, ['p' => 'a']));
var_dump(strip_tags($string, [0]));
var_dump(strip_tags($string, [1]));
var_dump(strip_tags($string, [1, 2]));
var_dump(strip_tags($string, ['color' => 'red', 'item' => 'pen']));
$s2 = "<html><p>hello</p><b>world</b><a href=\"#fragment\">Other text</a></html><?php echo hello ?>";
foreach (["<html>", "<p>", "<a>", "<?php", "<html><p><a><?php"] as $tags) {
    var_dump(strip_tags($s2, $tags));
}
"##,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(14) \"foo bar foobar\"\n\
string(30) \"foo bar <a href=\"#\">foobar</a>\"\n\
string(37) \"<p>foo bar <a href=\"#\">foobar</a></p>\"\n\
string(14) \"foo bar foobar\"\n\
string(14) \"foo bar foobar\"\n\
string(30) \"foo bar <a href=\"#\">foobar</a>\"\n\
string(14) \"foo bar foobar\"\n\
string(14) \"foo bar foobar\"\n\
string(14) \"foo bar foobar\"\n\
string(14) \"foo bar foobar\"\n\
string(33) \"<html>helloworldOther text</html>\"\n\
string(27) \"<p>hello</p>worldOther text\"\n\
string(44) \"helloworld<a href=\"#fragment\">Other text</a>\"\n\
string(20) \"helloworldOther text\"\n\
string(64) \"<html><p>hello</p>world<a href=\"#fragment\">Other text</a></html>\"\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Covers the network address families used by the direct IPv4 parser and the libc IPv6 path.
///
/// php-src has no dedicated `inet_pton.phpt`/`inet_ntop.phpt` at this revision, so the rows
/// follow `ext/standard/network.c` and the manual: canonical and zero-padded IPv4, invalid
/// octet counts/ranges/trailing bytes, compressed IPv6, embedded IPv4, and the `false`
/// result for a three-byte address.
#[test]
fn php_src_inet_pton_and_ntop_match_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
foreach (['127.0.0.1','01.2.3.4','255.255.255.255','1.2.3','256.1.1.1','1.2.3.4.','1.2.3.4 ','::1','2001:db8::1','::ffff:192.0.2.128'] as $s) {
    $p = inet_pton($s);
    echo $s, '=', $p === false ? 'F' : bin2hex($p), ';';
}
echo "\n";
foreach (['::1','2001:db8::1'] as $s) { echo inet_ntop(inet_pton($s)), ';'; }
echo "\n";
var_dump(inet_ntop('abc'));
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    let leading_zero = if cfg!(any(target_os = "macos", target_os = "ios")) {
        "01.2.3.4=01020304;"
    } else {
        "01.2.3.4=F;"
    };
    assert_eq!(
        out.stdout,
        format!(
            "127.0.0.1=7f000001;{}255.255.255.255=ffffffff;1.2.3=F;256.1.1.1=F;1.2.3.4.=F;1.2.3.4 =F;::1=00000000000000000000000000000001;2001:db8::1=20010db8000000000000000000000001;::ffff:192.0.2.128=00000000000000000000ffffc0000280;\n::1;2001:db8::1;\nbool(false)\n",
            leading_zero
        )
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
