//! Purpose:
//! Pins runtime producers whose output outgrows the 64 KiB `_concat_buf` scratch:
//! `json_encode()` (including the integer-tree and enum encoders), `serialize()` (including its
//! hooks), the `printf` family, typed `implode()`, `var_export()`, `strrev()`, and the
//! `__rt_strcopy` users (`ucfirst()`, `lcfirst()`, `ucwords()`, `str_pad()`); it also pins the
//! ownership of large results nobody keeps and the float formatters near a full scratch.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the regressions codegen test module.
//!
//! Key details:
//! - Each expected string is BUILT IN RUST from the PHP format rules, so a test compares the
//!   whole output byte for byte, not just a length or a prefix. The corruption these pin was in
//!   the middle (a wrong `a:` header, digits overwritten near the start) or a crash at exit.
//! - The sizes are chosen to cross 64 KiB by a wide margin, and several shapes nest, so a grown
//!   child buffer is copied into a grown parent and then released.

use super::*;

/// Joins `parts` with `glue`, the way PHP's `implode()` does.
fn join<T: ToString>(glue: &str, parts: impl IntoIterator<Item = T>) -> String {
    parts
        .into_iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join(glue)
}

/// `json_encode()` of a 20,000-entry int => int hash used to stream past the end of the scratch
/// into the adjacent globals and segfault; the encoder now grows its output onto the heap.
#[test]
fn test_json_encode_large_sparse_int_hash_is_complete() {
    let out = compile_and_run(
        r#"<?php
$a = [];
for ($i = 0; $i < 20000; $i++) { $a[$i * 2 + 1] = $i; }
echo json_encode($a);
"#,
    );
    let expected = format!(
        "{{{}}}",
        join(",", (0..20000).map(|i| format!("\"{}\":{}", i * 2 + 1, i)))
    );
    assert_eq!(out, expected);
}

/// `json_encode(range(1, 50000))` is about 289 KB: the integer-array encoder must grow instead of
/// writing past the 64 KiB scratch.
#[test]
fn test_json_encode_large_int_list_is_complete() {
    let out = compile_and_run("<?php echo json_encode(range(1, 50000));");
    assert_eq!(out, format!("[{}]", join(",", 1..=50000)));
}

/// A list of small hashes nests three encoders (dynamic array → assoc → string); every child
/// result is appended into a parent that has already moved onto the heap.
#[test]
fn test_json_encode_large_list_of_records_is_complete() {
    let out = compile_and_run(
        r#"<?php
$rows = [];
for ($i = 0; $i < 3000; $i++) { $rows[] = ["id" => $i, "name" => "user" . $i, "ok" => $i % 2 == 0]; }
echo json_encode($rows);
"#,
    );
    let expected = format!(
        "[{}]",
        join(
            ",",
            (0..3000).map(|i| format!(
                "{{\"id\":{},\"name\":\"user{}\",\"ok\":{}}}",
                i,
                i,
                if i % 2 == 0 { "true" } else { "false" }
            ))
        )
    );
    assert_eq!(out, expected);
}

/// A single string longer than the scratch tail is escaped into its own transient block, both
/// at top level and as a hash value.
#[test]
fn test_json_encode_long_string_with_escapes_is_complete() {
    let out = compile_and_run(
        r#"<?php
$s = str_repeat("a\"b/", 30000);
echo strlen(json_encode($s)), "\n";
echo json_encode(["k" => $s]) === "{\"k\":" . json_encode($s) . "}" ? "same" : "diff", "\n";
echo json_encode($s);
"#,
    );
    let escaped = format!("\"{}\"", "a\\\"b\\/".repeat(30000));
    assert_eq!(out, format!("{}\nsame\n{}", escaped.len(), escaped));
}

/// `JSON_PRETTY_PRINT` adds an indented line per element; the reservation covers the indentation.
#[test]
fn test_json_encode_large_pretty_print_is_complete() {
    let out = compile_and_run(
        r#"<?php
$data = ["items" => range(1, 15000)];
echo json_encode($data, JSON_PRETTY_PRINT);
"#,
    );
    let items = join(",\n", (1..=15000).map(|i| format!("        {}", i)));
    assert_eq!(out, format!("{{\n    \"items\": [\n{}\n    ]\n}}", items));
}

/// An object's public array property is encoded through the object encoder's own grown buffer.
#[test]
fn test_json_encode_object_with_large_property_is_complete() {
    let out = compile_and_run(
        r#"<?php
class Bag { public array $items = []; public string $name = "bag"; }
$b = new Bag();
$b->items = range(0, 19999);
echo json_encode($b);
"#,
    );
    assert_eq!(
        out,
        format!("{{\"items\":[{}],\"name\":\"bag\"}}", join(",", 0..20000))
    );
}

/// `serialize(range(1, 50000))` came back with a corrupted `a:41208…` header and the wrong
/// length; the serializer now writes into a stream that grows onto the heap.
#[test]
fn test_serialize_large_int_list_is_complete() {
    let out = compile_and_run("<?php echo serialize(range(1, 50000));");
    let body: String = (0..50000).map(|i| format!("i:{};i:{};", i, i + 1)).collect();
    assert_eq!(out, format!("a:50000:{{{}}}", body));
}

/// String keys, string values, floats, and a nested array, all past the scratch limit, and the
/// result round-trips through `unserialize()`.
#[test]
fn test_serialize_large_mixed_hash_is_complete_and_round_trips() {
    let out = compile_and_run(
        r#"<?php
$h = [];
for ($i = 0; $i < 8000; $i++) { $h["k" . $i] = str_repeat("x", $i % 7); }
$h["f"] = [0.5, 1.25, -3.0];
$s = serialize($h);
echo unserialize($s) === $h ? "round-trip" : "mismatch", "\n";
echo $s;
"#,
    );
    let mut body: String = (0..8000)
        .map(|i| {
            let key = format!("k{}", i);
            let value = "x".repeat(i % 7);
            format!("s:{}:\"{}\";s:{}:\"{}\";", key.len(), key, value.len(), value)
        })
        .collect();
    body.push_str("s:1:\"f\";a:3:{i:0;d:0.5;i:1;d:1.25;i:2;d:-3;}");
    assert_eq!(out, format!("round-trip\na:8001:{{{}}}", body));
}

/// An object whose `__serialize()` returns a large array is serialized through the magic-result
/// boundary while the stream grows; a property walk and a back-reference ride along.
#[test]
fn test_serialize_large_object_payloads_are_complete() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public array $items;
    public function __construct() { $this->items = range(1, 12000); }
    public function __serialize(): array { return ["items" => $this->items]; }
}
class Plain { public array $items; public function __construct() { $this->items = range(1, 12000); } }
$b = new Box();
$p = new Plain();
echo serialize([$b, $p, $p]);
"#,
    );
    let items: String = (0..12000).map(|i| format!("i:{};i:{};", i, i + 1)).collect();
    let array = format!("a:12000:{{{}}}", items);
    let expected = format!(
        "a:3:{{i:0;O:3:\"Box\":1:{{s:5:\"items\";{array}}}i:1;O:5:\"Plain\":1:{{s:5:\"items\";{array}}}i:2;r:{};}}",
        // r: counts every value before the repeated object: the outer array (1), Box (2), its
        // items array (3) and 12,000 ints, then Plain itself is value 12,004.
        1 + 1 + 1 + 12000 + 1
    );
    assert_eq!(out, expected);
}

/// `implode(",", range(1, 50000))` ran `__rt_itoa` per element, whose 21-byte scratch window
/// marched past the end of `_concat_buf` and wrote digits into the adjacent runtime globals.
#[test]
fn test_implode_large_int_float_and_bool_lists_are_complete() {
    let out = compile_and_run(
        r#"<?php
echo implode(",", range(1, 50000)), "\n";
$f = [];
for ($i = 0; $i < 20000; $i++) { $f[] = $i + 0.25; }
echo implode(";", $f), "\n";
$b = [];
for ($i = 0; $i < 40000; $i++) { $b[] = $i % 3 == 0; }
echo implode("|", $b);
"#,
    );
    let ints = join(",", 1..=50000);
    let floats = join(";", (0..20000).map(|i| format!("{}.25", i)));
    let bools = join("|", (0..40000).map(|i| if i % 3 == 0 { "1" } else { "" }));
    assert_eq!(out, format!("{}\n{}\n{}", ints, floats, bools));
}

/// `var_export(range(1, 50000))` exhausted the default 128 MB heap with "heap memory
/// exhausted": the prelude grew one string by repeated concatenation. It now joins parts once.
#[test]
fn test_var_export_large_list_fits_the_default_heap() {
    let out = compile_and_run(
        r#"<?php
var_export(range(1, 50000));
echo "\n";
var_export(["a" => [true, null, 1.5, "x'y"], 7 => []]);
"#,
    );
    let list = format!(
        "array (\n{})",
        (0..50000)
            .map(|i| format!("  {} => {},\n", i, i + 1))
            .collect::<String>()
    );
    let nested = "array (\n  'a' => \n  array (\n    0 => true,\n    1 => NULL,\n    2 => 1.5,\n    3 => 'x\\'y',\n  ),\n  7 => \n  array (\n  ),\n)";
    assert_eq!(out, format!("{}\n{}", list, nested));
}

/// `strrev()` and the `__rt_strcopy` users (`ucfirst()`, `lcfirst()`, `ucwords()`, a no-op
/// `str_pad()`) wrote their whole result at the scratch cursor with no bound, so a result longer
/// than the free scratch corrupted the bytes around offset 65536 and the globals after it.
#[test]
fn test_strrev_and_copied_case_transforms_past_the_scratch_are_complete() {
    let out = compile_and_run(
        r#"<?php
$s = str_repeat("ab cd", 14000);
echo strrev($s), "\n";
echo ucfirst($s) === "A" . substr($s, 1) ? "ucfirst" : "bad", "\n";
echo lcfirst(strtoupper($s)) === "a" . substr(strtoupper($s), 1) ? "lcfirst" : "bad", "\n";
echo str_pad($s, 5) === $s ? "str_pad" : "bad", "\n";
echo ucwords($s);
"#,
    );
    let source = "ab cd".repeat(14000);
    let reversed: String = source.chars().rev().collect();
    // PHP's default ucwords() separators are whitespace, so only the very first "a" and every
    // "c" after a space are capitalised: "Ab Cdab Cdab Cd…".
    let mut words = String::with_capacity(source.len());
    let mut word_start = true;
    for ch in source.chars() {
        words.push(if word_start { ch.to_ascii_uppercase() } else { ch });
        word_start = ch == ' ';
    }
    assert_eq!(out, format!("{}\nucfirst\nlcfirst\nstr_pad\n{}", reversed, words));
}

/// Builds `serialize(range(1, n))`: `a:n:{i:0;i:1;i:1;i:2;...}`.
fn serialized_range(n: usize) -> String {
    let body: String = (0..n).map(|i| format!("i:{};i:{};", i, i + 1)).collect();
    format!("a:{}:{{{}}}", n, body)
}

/// Every serializer that writes bytes must reserve through the growable stream: the legacy
/// `Serializable::serialize()` envelope (`C:`), a `__sleep()` property walk, and a `__serialize()`
/// whose body itself calls `serialize()` (which gets its own stream) all land past 64 KiB.
#[test]
fn test_serialize_hooks_write_through_the_growable_stream() {
    let out = compile_and_run(
        r#"<?php
class Leg implements Serializable {
    public $d;
    function __construct() { $this->d = str_repeat("ab", 40000); }
    function serialize() { return $this->d; }
    function unserialize($s) { $this->d = $s; }
}
class Sl { public $a; public $b = 5; function __construct() { $this->a = range(1, 12000); } function __sleep() { return ["a", "b"]; } }
class Inner { function __serialize(): array { return ["x" => serialize(range(1, 12000))]; } }
echo serialize([range(1, 12000), new Leg, new Sl, new Inner]);
"#,
    );
    let list = serialized_range(12000);
    let expected = format!(
        "a:4:{{i:0;{list}i:1;C:3:\"Leg\":80000:{{{}}}i:2;O:2:\"Sl\":2:{{s:1:\"a\";{list}s:1:\"b\";i:5;}}i:3;O:5:\"Inner\":1:{{s:1:\"x\";s:{}:\"{list}\";}}}}",
        "ab".repeat(40000),
        list.len()
    );
    assert_eq!(out, expected);
}

/// A `__serialize()` or `__sleep()` hook that throws after the output stream outgrew the scratch
/// used to leave the grown heap buffer behind on every caught exception.
#[test]
fn test_serialize_hook_exceptions_do_not_leak_the_grown_stream() {
    let out = compile_and_run(
        r#"<?php
class Boom { function __serialize(): array { throw new Exception("boom"); } }
class Boom2 { public $x = 1; function __sleep() { throw new Exception("sleep"); } }
$before = 0;
$caught = 0;
for ($i = 0; $i < 300 + $argc - 1; $i++) {
    if ($i == 20) { $before = memory_get_usage(); }
    try { serialize([range(1, 20000), new Boom]); } catch (Exception $e) { $caught++; }
    try { serialize([range(1, 20000), new Boom2]); } catch (Exception $e) { $caught++; }
}
echo $caught, "|", memory_get_usage() - $before < 200000 ? "flat" : "leak";
"#,
    );
    assert_eq!(out, "600|flat");
}

/// A serialize() inside a hook, a hook that throws inside an outer serialize() that is itself
/// caught, and plain serialize() afterwards must all keep their own streams apart.
#[test]
fn test_serialize_streams_stay_separate_across_nested_calls_and_throws() {
    let out = compile_and_run(
        r#"<?php
class Thrower { function __serialize(): array { serialize(range(1, 15000)); throw new Exception("t"); } }
class Nest { function __serialize(): array { return ["a" => serialize(range(1, 15000)), "b" => serialize("s")]; } }
try { serialize([range(1, 15000), new Thrower]); } catch (Exception $e) { echo "caught|"; }
echo serialize([new Nest, range(1, 15000)]);
"#,
    );
    let list = serialized_range(15000);
    let nest = format!(
        "O:4:\"Nest\":2:{{s:1:\"a\";s:{}:\"{list}\";s:1:\"b\";s:8:\"s:1:\"s\";\";}}",
        list.len()
    );
    assert_eq!(out, format!("caught|a:2:{{i:0;{nest}i:1;{list}}}"));
}

/// A string result past the scratch that the caller never keeps (`md5(str_repeat(...))` as a
/// statement, a digest of a large serialize() result) used to leak its whole subject per call: the
/// digests sat in the default "may alias the argument" ownership bucket, which suppresses the
/// release of an owned subject temporary.
#[test]
fn test_large_string_argument_of_a_digest_is_released() {
    let out = compile_and_run(
        r#"<?php
function usage(): int { return memory_get_usage(); }
$n = 300 + $argc - 1;
md5(str_repeat("x", 70000));
$base = usage();
for ($i = 0; $i < $n; $i++) { md5(str_repeat("x", 70000)); }
$a = usage() - $base;
$base = usage();
for ($i = 0; $i < $n; $i++) { $h = sha1(str_repeat("x", 70000)); }
$b = usage() - $base;
$base = usage();
for ($i = 0; $i < $n; $i++) { $h = md5(serialize(range(1, 20000))); }
$c = usage() - $base;
$base = usage();
for ($i = 0; $i < $n; $i++) { $h = md5(json_encode(range(1, 20000))); }
$d = usage() - $base;
echo $a < 100000 ? "md5" : "md5-leak $a", "|", $b < 100000 ? "sha1" : "sha1-leak $b", "|",
     $c < 100000 ? "serialize" : "serialize-leak $c", "|", $d < 100000 ? "json" : "json-leak $d", "|", strlen($h);
"#,
    );
    assert_eq!(out, "md5|sha1|serialize|json|32");
}

/// Floats converted while the scratch is within a few dozen bytes of full used to be written at
/// `_concat_off` with no bound (integers had a spill ring, floats did not). Every formatter must
/// produce the same text near the end of the scratch as it does with the scratch empty.
#[test]
fn test_float_formatting_near_a_full_scratch_matches_an_empty_scratch() {
    let out = compile_and_run(
        r#"<?php
$k = $argc;
$vals = [-1.2345678901234E-300 * $k, 0.1 * $k + 0.2, 123456789012345678.0 * $k, 100.0 * $k, 1.5e300 * $k];
$bad = 0;
foreach ($vals as $f) {
    $plain = [(string)$f, json_encode($f), serialize($f), var_export($f, true), json_encode($f, JSON_PRESERVE_ZERO_FRACTION)];
    foreach ([65470, 65490, 65500, 65510, 65520, 65526, 65530, 65534] as $pad) {
        $z = str_repeat("z", $pad);
        $x = [substr($z . $f, $pad), substr($z . json_encode($f), $pad), substr($z . serialize($f), $pad),
              substr($z . var_export($f, true), $pad), substr($z . json_encode($f, JSON_PRESERVE_ZERO_FRACTION), $pad)];
        if ($x !== $plain) { $bad++; echo "pad $pad value $f: ", json_encode($x), " vs ", json_encode($plain), "\n"; }
    }
}
ob_start();
var_dump(str_repeat("r", 65532) . $vals[0]);
var_dump($vals[0], $vals[1]);
$o = ob_get_clean();
echo $bad, "|", substr_count($o, "float("), "|", strlen($o) > 65532 ? "dump" : "short";
"#,
    );
    assert_eq!(out, "0|2|dump");
}

/// `json_encode()` of nested all-integer lists (the bounded integer-tree fast path) well past
/// 64 KiB, with and without a non-integer sibling that forces the general encoder.
#[test]
fn test_json_integer_tree_outputs_past_the_scratch_are_complete() {
    let out = compile_and_run(
        r#"<?php
$rows = [];
for ($i = 0; $i < 9000; $i++) { $rows[] = [$i, $i + 1, -$i, [$i * 3, 7]]; }
echo json_encode($rows), "\n";
$rows[] = [1.5, "x"];
echo json_encode($rows);
"#,
    );
    let rows = |extra: bool| {
        let mut parts: Vec<String> = (0..9000i64)
            .map(|i| format!("[{},{},{},[{},7]]", i, i + 1, -i, i * 3))
            .collect();
        if extra {
            parts.push("[1.5,\"x\"]".to_string());
        }
        format!("[{}]", parts.join(","))
    };
    assert_eq!(out, format!("{}\n{}", rows(false), rows(true)));
}

/// Backed enums serialize through the enum dispatch (no object recursion): arrays and object
/// properties of them, plus the unit-enum placeholder under partial output, past 64 KiB.
///
/// The cases are picked from arrays rather than with `$c ? Suit::A : Suit::B`: appending such a
/// ternary to an array inside a loop types the element as a string on main and fails before any
/// JSON is produced, which is a separate defect.
#[test]
fn test_json_enum_outputs_past_the_scratch_are_complete() {
    let out = compile_and_run(
        r#"<?php
enum Suit: string { case Hearts = "hearts"; case Spades = "spades"; }
enum Level: int { case Low = 5; case High = 123; }
enum Unit { case Only; }
class Card { public function __construct(public Suit $suit, public Level $level, public int $n) {} }
$suitCases = [Suit::Spades, Suit::Hearts];
$cardSuits = [Suit::Hearts, Suit::Spades];
$levelCases = [Level::High, Level::Low, Level::Low];
$suits = []; $levels = []; $cards = [];
for ($i = 0; $i < 9000; $i++) {
    $suits[] = $suitCases[$i % 2];
    $levels[] = $levelCases[$i % 3];
    $cards[] = new Card($cardSuits[$i % 2], Level::High, $i);
}
echo json_encode($suits), "\n";
echo json_encode($levels), "\n";
echo json_encode($cards), "\n";
$sj = json_encode($suits);
$partial = json_encode(["s" => $suits, "u" => [Unit::Only, 1]], JSON_PARTIAL_OUTPUT_ON_ERROR);
echo $partial === '{"s":' . $sj . ',"u":[0,1]}' ? "partial" : "bad", "|", json_last_error();
"#,
    );
    let suits: Vec<&str> = (0..9000)
        .map(|i| if i % 2 == 1 { "\"hearts\"" } else { "\"spades\"" })
        .collect();
    let levels: Vec<&str> = (0..9000).map(|i| if i % 3 != 0 { "5" } else { "123" }).collect();
    let cards: Vec<String> = (0..9000)
        .map(|i| {
            format!(
                "{{\"suit\":\"{}\",\"level\":123,\"n\":{}}}",
                if i % 2 == 1 { "spades" } else { "hearts" },
                i
            )
        })
        .collect();
    assert_eq!(
        out,
        format!(
            "[{}]\n[{}]\n[{}]\npartial|11",
            suits.join(","),
            levels.join(","),
            cards.join(",")
        )
    );
}

/// A `jsonSerialize()` that itself calls `json_encode()` (so a nested encoder runs mid-walk,
/// including past 64 KiB, and a bigger result is returned through a string) must leave the outer
/// result exactly as if the inner text had been supplied by hand.
#[test]
fn test_json_serializable_calling_json_encode_keeps_the_outer_result_intact() {
    let out = compile_and_run(
        r#"<?php
class Wrap implements JsonSerializable {
    public function __construct(private mixed $d) {}
    public function jsonSerialize(): mixed { return json_encode($this->d) . "!"; }
}
class Inner implements JsonSerializable {
    public function __construct(private mixed $d) {}
    public function jsonSerialize(): mixed { return ["inner" => json_encode($this->d), "n" => json_encode(1.5)]; }
}
echo json_encode(["pad" => str_repeat("p", 100), "o" => new Wrap([1, 2, 3])]), "\n";
echo json_encode([str_repeat("p", 100), 1, new Wrap(["x" => 1]), new Inner(["y" => [1]]), "tail"]), "\n";
$a = json_encode([new Inner(range(1, 9000)), 2.5, new Wrap(range(1, 9000)), "tail"]);
$b = json_encode([["inner" => json_encode(range(1, 9000)), "n" => "1.5"], 2.5, json_encode(range(1, 9000)) . "!", "tail"]);
echo strlen($a) > 70000 ? "big" : "small", "|", $a === $b ? "same" : "diff", "\n";
$c = json_encode([str_repeat("p", 65000), new Wrap(range(1, 100)), new Inner(range(1, 100))]);
$d = json_encode([str_repeat("p", 65000), json_encode(range(1, 100)) . "!", ["inner" => json_encode(range(1, 100)), "n" => "1.5"]]);
echo strlen($c) > 65000 ? "edge" : "short", "|", $c === $d ? "same" : "diff";
"#,
    );
    let expected = format!(
        "{{\"pad\":\"{}\",\"o\":\"[1,2,3]!\"}}\n[\"{}\",1,\"{{\\\"x\\\":1}}!\",{{\"inner\":\"{{\\\"y\\\":[1]}}\",\"n\":\"1.5\"}},\"tail\"]\nbig|same\nedge|same",
        "p".repeat(100),
        "p".repeat(100),
    );
    assert_eq!(out, expected);
}
