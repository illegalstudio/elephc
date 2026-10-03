//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of a spread inside an
//! ASSOCIATIVE array literal — the shape an associative literal's key/value pair list has no slot
//! for, and which the parser used to discard.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout.
//! - Expected output is verbatim `LC_ALL=C php` 8.5.10.

use crate::support::*;

/// Issue #1049's repro: the spread's entries vanished, silently, and only the string-keyed entry
/// survived.
#[test]
fn test_indexed_spread_into_an_associative_literal() {
    let out = compile_and_run(
        r#"<?php
$idx = [3, 4];
$a = [...$idx, "c" => 8];
echo count($a), "|";
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "3|0=3;1=4;c=8;");
}

/// The issue reports an INDEXED source; every source kind was dropped, because the parser never
/// looked at the source — it discarded the spread before the AST existed.
///
/// The int-keyed source is read back at key `0`, not `5`: PHP renumbers a spread's INTEGER keys
/// into the destination's running counter and keeps only its string keys, and the runtime helper
/// does the same.
#[test]
fn test_every_spread_source_kind_into_an_associative_literal() {
    let out = compile_and_run(
        r#"<?php
$assoc = ["a" => 1, "b" => 2];
$intkeyed = [5 => 7];
$strings = ["x", "y"];
$floats = [1.5, 2.5];
$a = [...$assoc, "c" => 8];
$b = [...$intkeyed, "c" => 8];
$c = [...$strings, "c" => "z"];
$d = [...$floats, "c" => 3.5];
echo count($a), count($b), count($c), count($d), "|", $a["a"], $b[0], $c[1], $d[0];
"#,
    );
    assert_eq!(out, "3233|17y1.5");
}

/// The spread AFTER the string key, and two spreads in one literal.
#[test]
fn test_spread_position_inside_an_associative_literal() {
    let out = compile_and_run(
        r#"<?php
$p = [1, 2];
$q = [3];
$after = ["c" => 8, ...$p];
$two = [...$p, ...$q, "c" => 8];
echo count($after), count($two), "|";
foreach ($after as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "34|c=8;0=1;1=2;");
}

/// An unkeyed element after a spread in an already-associative literal uses PHP's runtime next key.
#[test]
fn test_unkeyed_element_after_associative_spread_uses_runtime_next_key() {
    let out = compile_and_run(
        r#"<?php
$a = ["c" => 8, ...[3, 4], 5];
echo count($a), "|", implode(",", array_keys($a)), "|", $a[0], $a[1], $a[2], $a["c"];
"#,
    );
    assert_eq!(out, "4|c,0,1,2|3458");
}

/// A literal spread source rather than a variable: the source is an owning temporary, so the
/// consuming promotion below must NOT be handed an extra reference.
#[test]
fn test_literal_spread_source_into_an_associative_literal() {
    let out = compile_and_run(
        r#"<?php
$a = [...[3, 4], "c" => 8];
echo count($a), "|", $a[0], $a[1], $a["c"];
"#,
    );
    assert_eq!(out, "3|348");
}

/// The spread must not CONSUME its source. `Op::ArrayToHash` lowers to a conversion that decrefs
/// its input, which is right where it replaces a local's own value and wrong here — the source
/// array was freed under the caller and `count($idx)` answered 0.
#[test]
fn test_associative_literal_spread_leaves_its_source_intact() {
    let out = compile_and_run(
        r#"<?php
$idx = [3, 4];
$a = [...$idx, "c" => 8];
$b = [...$idx, "d" => 9];
echo count($idx), $idx[0], $idx[1], "|", count($a), count($b);
"#,
    );
    assert_eq!(out, "234|33");
}

/// Every spread SOURCE FORM that borrows its reference from storage the caller keeps owning: a
/// local, a static property, a typed instance property. The consuming promotion must be handed a
/// reference of its own for each, or the source is freed under its owner.
///
/// The static property is the one an op-list gate misses most easily — `LoadStaticLocal` and
/// `LoadStaticProperty` are the same "the slot keeps its own reference" shape under different
/// names, and leaving the second out answered `count(C::$stat)` with 0.
#[test]
fn test_borrowed_spread_sources_survive_the_literal() {
    let out = compile_and_run(
        r#"<?php
class C { public static $stat = [3, 4]; public array $items = [5, 6]; }
$o = new C();
$local = [7, 8];
$a = [...C::$stat, "c" => 1];
$b = [...$o->items, "c" => 2];
$c = [...$local, "c" => 3];
echo count($a), count($b), count($c), "|";
echo count(C::$stat), count($o->items), count($local), "|";
echo C::$stat[0], $o->items[0], $local[0];
"#,
    );
    assert_eq!(out, "333|222|357");
}

/// A TERNARY source is a `LoadLocal` of a hidden temp the ternary moved out of: it already owns
/// the only reference there is, so acquiring another leaked one array per evaluation. The op alone
/// cannot tell it from a user local's load — the slot's kind can.
#[test]
fn test_ternary_spread_source_into_an_associative_literal() {
    let out = compile_and_run(
        r#"<?php
$c = true;
$a = [...($c ? [1, 2] : [3, 4]), "c" => 8];
$b = [...(!$c ? [1, 2] : [3, 4]), "c" => 9];
echo count($a), count($b), "|", $a[0], $a[1], $b[0], $b[1];
"#,
    );
    assert_eq!(out, "33|1234");
}

/// An UNKEYED element AFTER a spread cannot take a key computed at parse time: how many the spread
/// contributes is a runtime fact. Assigning one anyway gave the `5` key 1 and overwrote the
/// spread's first entry — four entries where PHP has five.
#[test]
fn test_unkeyed_element_after_a_spread_keeps_php_numbering() {
    let out = compile_and_run(
        r#"<?php
$idx = [3, 4];
$a = [1, ...$idx, 5, "c" => 8];
echo count($a), "|", implode(",", array_keys($a)), "|", $a[0], $a[1], $a[2], $a[3], $a["c"];
"#,
    );
    assert_eq!(out, "5|0,1,2,3,c|13458");
}

/// The controls this must not disturb: an indexed spread into an INDEXED literal, and an
/// associative spread into one — both already worked and take different helpers.
#[test]
fn test_indexed_literal_spreads_are_unchanged() {
    let out = compile_and_run(
        r#"<?php
$idx = [3, 4];
$assoc = ["a" => 1, "b" => 2];
$a = [...$idx, 9];
$b = [...$assoc];
echo count($a), "|", $a[0], $a[1], $a[2], "|", count($b), $b["a"], $b["b"], "|", count($idx), count($assoc);
"#,
    );
    assert_eq!(out, "3|349|212|22");
}
