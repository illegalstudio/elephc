//! Purpose:
//! Regression tests for the associative-array sorts that reorder a hash table's
//! insertion-order chain: `ksort()`, `krsort()`, `asort()`, `arsort()`, `natsort()`
//! and `natcasesort()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is verbatim `LC_ALL=C php` (PHP 8.4.20) output for the same fixture.
//! - Before this suite, `ksort()`/`krsort()` on a hash were runtime no-ops that returned
//!   the receiver untouched with no diagnostic; the string-key case is the original repro.
//! - Sorting only relinks `prev`/`next`/`head`/`tail`, so the fixtures also assert that key
//!   association, later key lookups, later inserts and copy-on-write all still hold, and
//!   one fixture re-checks the heap under `--heap-debug`.
//! - A large reverse-order fixture guards the merge-sort path against accidentally
//!   reintroducing quadratic insertion behavior.
//! - PHP's key ordering is `zend_compare`, not a byte-wise order: `10` sorts before
//!   `'Banana'` and `'0.5'` before `2`, which the mixed-key fixture pins.

use crate::support::*;

/// Issue repro: `ksort()`/`krsort()` over a string-keyed associative array used to leave the
/// receiver in insertion order without any diagnostic. Both directions must now reorder it.
#[test]
fn test_ksort_krsort_string_keys() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1];
ksort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
krsort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "a=3;b=2;c=1;|c=1;b=2;a=3;");
}

/// The original one-line repro: `implode(",", array_keys($a))` after `ksort()`.
#[test]
fn test_ksort_string_keys_through_array_keys() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1];
ksort($a);
echo implode(",", array_keys($a));
"#,
    );
    assert_eq!(out, "a,b,c");
}

/// Sparse integer keys must sort numerically (`-1 < 2 < 10 < 33`), not by insertion order
/// and not by the decimal text of the key.
#[test]
fn test_ksort_krsort_integer_keys() {
    let out = compile_and_run(
        r#"<?php
$a = [10 => "x", 2 => "y", 33 => "z", -1 => "w"];
ksort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
krsort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "-1=w;2=y;10=x;33=z;|33=z;10=x;2=y;-1=w;");
}

/// Mixed integer and string keys follow PHP's standard comparison, so `''` sorts before the
/// integer `2` and the integer `10` sorts before `'Banana'` — not a lexicographic order.
#[test]
fn test_ksort_krsort_mixed_int_and_string_keys() {
    let out = compile_and_run(
        r#"<?php
$a = [10 => "a", "9" => "b", "apple" => "c", "Banana" => "d", 2 => "e", "" => "f"];
ksort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
$b = [10 => "a", "9" => "b", "apple" => "c", "Banana" => "d", 2 => "e", "" => "f"];
krsort($b);
foreach ($b as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(
        out,
        "=f;2=e;9=b;10=a;Banana=d;apple=c;|apple=c;Banana=d;10=a;9=b;2=e;=f;"
    );
}

/// An empty receiver — both a literal `[]` and a hash emptied with `unset()` — must sort to
/// itself without touching the header's head/tail sentinels.
#[test]
fn test_ksort_krsort_empty_array() {
    let out = compile_and_run(
        r#"<?php
$a = [];
ksort($a);
echo count($a), ";";
krsort($a);
echo count($a), ";";
$b = ["k" => 1];
unset($b["k"]);
ksort($b);
echo count($b), ";";
krsort($b);
echo count($b);
"#,
    );
    assert_eq!(out, "0;0;0;0");
}

/// A single-entry hash must survive both directions with its one key/value pair intact.
#[test]
fn test_ksort_krsort_single_element() {
    let out = compile_and_run(
        r#"<?php
$a = ["only" => 7];
ksort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
krsort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "only=7;only=7;");
}

/// `asort()`/`arsort()` over duplicate values must be stable in both directions: `b`, `d`
/// and `e` all hold `2` and keep their original relative order, exactly like PHP 8.
#[test]
fn test_asort_arsort_duplicate_values_are_stable() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1, "d" => 2, "e" => 2];
asort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
$b = ["b" => 2, "a" => 3, "c" => 1, "d" => 2, "e" => 2];
arsort($b);
foreach ($b as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "c=1;b=2;d=2;e=2;a=3;|a=3;b=2;d=2;e=2;c=1;");
}

/// `asort()`/`arsort()` over string values compare with PHP's ordering, not by slot width.
#[test]
fn test_asort_arsort_string_values() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => "pear", "a" => "apple", "c" => "fig"];
asort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
arsort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "a=apple;c=fig;b=pear;|b=pear;c=fig;a=apple;");
}

/// Copy-on-write: a copy taken before the sort must keep the original iteration order, in
/// both directions. The sorters mutate the table in place, so the receiver has to be split
/// with `__rt_hash_ensure_unique` first.
#[test]
fn test_ksort_krsort_does_not_mutate_aliased_copy() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1];
$copy = $a;
ksort($a);
foreach ($a as $k => $v) { echo $k; }
echo "|";
foreach ($copy as $k => $v) { echo $k; }
echo "|";
$other = $a;
krsort($a);
foreach ($a as $k => $v) { echo $k; }
echo "|";
foreach ($other as $k => $v) { echo $k; }
"#,
    );
    assert_eq!(out, "abc|bac|cba|abc");
}

/// Sorting must not disturb the hash's probe layout: key lookups, a later insert, and the
/// live count all still work on the reordered table.
#[test]
fn test_ksort_preserves_lookup_and_later_inserts() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1];
ksort($a);
echo $a["a"], $a["b"], $a["c"], ";";
$a["d"] = 4;
foreach ($a as $k => $v) { echo $k, $v; }
echo ";", count($a);
"#,
    );
    assert_eq!(out, "321;a3b2c1d4;4");
}

/// Repeated sorts in both directions must keep converging on the same orders instead of
/// corrupting the insertion-order chain after the first relink.
#[test]
fn test_repeated_key_and_value_sorts_stay_consistent() {
    let out = compile_and_run(
        r#"<?php
$a = ["b" => 2, "a" => 3, "c" => 1, "d" => 4, "e" => 5];
krsort($a);
foreach ($a as $k => $v) { echo $k; }
echo "|";
ksort($a);
foreach ($a as $k => $v) { echo $k; }
echo "|";
asort($a);
foreach ($a as $k => $v) { echo $k; }
"#,
    );
    assert_eq!(out, "edcba|abcde|cbade");
}

/// An input that is already in the requested order must come back unchanged, which also
/// exercises the backward scan's immediate-stop path.
#[test]
fn test_key_sorts_on_already_ordered_input() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
ksort($a);
foreach ($a as $k => $v) { echo $k; }
echo "|";
$b = ["c" => 3, "b" => 2, "a" => 1];
krsort($b);
foreach ($b as $k => $v) { echo $k; }
"#,
    );
    assert_eq!(out, "abc|cba");
}

/// A large packed array exercises several bottom-up merge passes and the packed-to-hash
/// promotion used by descending key order. The first, second, and last keys pin the full
/// relink without making the assertion depend on timing.
#[test]
fn test_krsort_scales_to_large_reverse_key_order() {
    let out = compile_and_run(
        r#"<?php
$a = range(0, 2047);
krsort($a);
$keys = array_keys($a);
echo count($keys), ":", $keys[0], ":", $keys[1], ":", $keys[2047];
"#,
    );
    assert_eq!(out, "2048:2047:2046:0");
}

/// `ksort()` on an indexed array stays a no-op: its keys are the slot positions `0..n-1`,
/// which are already in ascending key order, and the values keep their slots.
#[test]
fn test_ksort_on_indexed_array_is_a_noop() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
ksort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "0=3;1=1;2=2;");
}

/// `krsort()` promotes non-empty indexed storage, returns true, and preserves direct lookup
/// while exposing descending integer keys through iteration.
#[test]
fn test_krsort_on_indexed_array_returns_true_and_preserves_lookup() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
echo krsort($a) ? "true:" : "false:";
echo $a[0], ":";
foreach ($a as $key => $value) { echo $key, "=", $value, ";"; }
"#,
    );
    assert_eq!(out, "true:1:2=3;1=2;0=1;");
}

/// `krsort()` on a statically empty indexed array stays accepted, because an empty receiver
/// is trivially representable in either direction.
#[test]
fn test_krsort_on_empty_indexed_array_is_accepted() {
    let out = compile_and_run(
        r#"<?php
$a = [];
krsort($a);
echo count($a);
"#,
    );
    assert_eq!(out, "0");
}

/// Sorting must not acquire, persist or release anything: it only rewrites slot indices in
/// the chain. Running the string-keyed and string-valued fixtures under `--heap-debug`
/// pins that, including the copy-on-write split the sorters ask for.
#[test]
fn test_hash_sorts_leave_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["bb" => "two", "aa" => "three", "cc" => "one"];
$b = $a;
ksort($a);
krsort($b);
asort($a);
arsort($b);
foreach ($a as $k => $v) { echo $k, $v; }
foreach ($b as $k => $v) { echo $k, $v; }
"#,
    );
    assert_eq!(
        out.stdout, "cconeaathreebbtwobbtwoaathreeccone",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// `natsort()`/`natcasesort()` over a string-keyed hash must keep every key attached to its
/// own value while only the iteration order moves.
///
/// php sorts naturally with `zend_array_sort(Z_ARRVAL_P(array), php_array_natural_compare, 0)`
/// (ext/standard/array.c): the trailing `0` is `renumber`, the same argument `asort()` passes
/// and `sort()` passes as `1`. Before this fixture the backend refused a hash receiver
/// outright — "unsupported EIR backend feature: natsort for PHP type AssocArray" — even
/// though the checker accepted it.
#[test]
fn test_natsort_natcasesort_preserve_string_keys() {
    let out = compile_and_run(
        r#"<?php
$a = ["first" => "img12", "second" => "img10", "third" => "img2", "fourth" => "img1"];
natsort($a);
foreach ($a as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
$b = ["w" => "IMG12", "x" => "img10", "y" => "Img2", "z" => "IMG1"];
natcasesort($b);
foreach ($b as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(
        out,
        "fourth=img1;third=img2;second=img10;first=img12;|z=IMG1;y=Img2;x=img10;w=IMG12;"
    );
}

/// Integer keys are preserved verbatim, never renumbered: php reports `9,2,0,5` after the
/// sort, which is exactly the permutation `renumber = 0` leaves behind.
#[test]
fn test_natsort_integer_keys_are_preserved_not_renumbered() {
    let out = compile_and_run(
        r#"<?php
$c = [5 => "img12", 2 => "img2", 9 => "img1", 0 => "img10"];
natsort($c);
foreach ($c as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
echo implode(",", array_keys($c));
"#,
    );
    assert_eq!(out, "9=img1;2=img2;0=img10;5=img12;|9,2,0,5");
}

/// PHP 8 sorts are stable, and the natural comparator returns 0 for genuinely equal fields,
/// so `p`, `r` and `s` all hold `img2` and keep their original relative order. The
/// case-insensitive spelling ties `A1` with `a1` the same way.
#[test]
fn test_natsort_duplicate_values_are_stable() {
    let out = compile_and_run(
        r#"<?php
$d = ["p" => "img2", "q" => "img1", "r" => "img2", "s" => "img2"];
natsort($d);
foreach ($d as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
$e = ["p" => "A1", "q" => "a1", "r" => "B1"];
natcasesort($e);
foreach ($e as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(out, "q=img1;p=img2;r=img2;s=img2;|p=A1;q=a1;r=B1;");
}

/// Copy-on-write: a copy taken before the sort keeps the original iteration order, so the
/// receiver has to be split with `__rt_hash_ensure_unique` before the chain is relinked.
#[test]
fn test_natsort_does_not_mutate_aliased_copy() {
    let out = compile_and_run(
        r#"<?php
$f = ["b" => "img10", "a" => "img2", "c" => "img1"];
$copy = $f;
natsort($f);
foreach ($f as $k => $v) { echo $k; }
echo "|";
foreach ($copy as $k => $v) { echo $k; }
"#,
    );
    assert_eq!(out, "cab|bac");
}

/// Relinking must not disturb the hash's probe layout: key lookups, a later insert and the
/// live count all still work on the reordered table.
#[test]
fn test_natsort_preserves_lookup_and_later_inserts() {
    let out = compile_and_run(
        r#"<?php
$g = ["b" => "img10", "a" => "img2", "c" => "img1"];
natsort($g);
$g["d"] = "img3";
echo $g["b"], ":", $g["a"], ":", count($g), ":", implode(",", array_keys($g));
"#,
    );
    assert_eq!(out, "img10:img2:4:c,a,b,d");
}

/// Already-ordered input, a single entry, and the natural-order edge cases the comparator
/// owns — a leading zero making a field fractional (`a002` < `a01` < `a1`) and whitespace
/// skipped before a field (`a 3` between `a2` and `a10`) — all measured on a hash receiver.
#[test]
fn test_natsort_on_hash_edge_cases() {
    let out = compile_and_run(
        r#"<?php
$h = ["k1" => "img1", "k2" => "img2", "k3" => "img10"];
natsort($h);
foreach ($h as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
$i = ["solo" => "only"];
natsort($i);
foreach ($i as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
$j = ["a" => "a01", "b" => "a1", "c" => "a2", "d" => "a002", "e" => "a 3", "f" => "a10"];
natsort($j);
foreach ($j as $k => $v) { echo $k, "=", $v, ";"; }
"#,
    );
    assert_eq!(
        out,
        "k1=img1;k2=img2;k3=img10;|solo=only;|d=a002;a=a01;b=a1;c=a2;e=a 3;f=a10;"
    );
}

/// Pins the natural-order hash sorters inside the REAL generated runtime, on BOTH targets.
///
/// Generating the runtime text directly is the only mechanism that covers the hand-written
/// x86_64 bodies from an aarch64 host, and it proves something an emitter unit test cannot:
/// that each helper is actually REGISTERED in `emit_runtime`.
///
/// The needles are the load-bearing pieces of the story. Each entry point must name its OWN
/// comparator — that is both how `natsort` gets php's natural order and why a `ksort`-only
/// program can still dead-strip the ~1.4 KB comparator pair — and the shared engine must
/// call through the frame slot rather than any fixed comparator symbol, or the selection
/// would be inert.
#[test]
fn the_runtime_defines_the_natural_hash_sorts_on_both_targets() {
    for (target_name, indirect_call, tail) in [
        ("macos-aarch64", "blr x9", "b __rt_natcmp"),
        ("linux-x86_64", "call QWORD PTR [rbp - 96]", "jmp __rt_natcmp"),
    ] {
        let target = elephc::codegen::platform::Target::parse(target_name).expect("valid target");
        let runtime_asm = elephc::codegen::generate_runtime_with_features(
            8_388_608,
            target,
            elephc::codegen::RuntimeFeatures::none(),
        );
        // Slices one helper's text: from its own header comment to the next helper's. The
        // headers come in two spellings — `--- runtime: name ---` and
        // `--- runtime: name (description) ---` — so the name is matched without either tail.
        let body_of = |helper: &str| -> String {
            let header = format!("--- runtime: {}", helper);
            let start = runtime_asm.find(&header).unwrap_or_else(|| {
                panic!("{helper} is not registered in the {target_name} runtime")
            });
            let rest = &runtime_asm[start + header.len()..];
            let end = rest
                .find("--- runtime: ")
                .map(|offset| start + header.len() + offset)
                .unwrap_or(runtime_asm.len());
            runtime_asm[start..end].to_string()
        };

        // Each entry point names its own comparator, and ONLY its own: that per-atom
        // reference is what lets the linker drop the natural comparators from a program
        // that never natsorts.
        for (entry, wanted, unwanted) in [
            ("__rt_hash_natsort", "__rt_hash_natcmp", "__rt_php_compare"),
            ("__rt_hash_natcasesort", "__rt_hash_natcasecmp", "__rt_php_compare"),
            ("__rt_hash_ksort", "__rt_php_compare", "__rt_hash_natcmp"),
            ("__rt_hash_asort", "__rt_php_compare", "__rt_hash_natcmp"),
        ] {
            let body = body_of(entry);
            assert!(
                body.contains(wanted),
                "{entry} on {target_name} must select {wanted}"
            );
            assert!(
                !body.contains(unwanted),
                "{entry} on {target_name} must NOT reference {unwanted}: that reference is \
                 what keeps the linker from dead-stripping the unused comparator"
            );
        }

        // Both adapters tail-branch into php's natural comparator, and guard on the string
        // tag so a non-string payload can never be dereferenced as a pointer.
        let natcmp = body_of("__rt_hash_natcmp");
        assert!(
            natcmp.contains(tail),
            "__rt_hash_natcmp on {target_name} must tail-branch into __rt_natcmp"
        );
        assert!(
            natcmp.contains("__rt_php_compare"),
            "__rt_hash_natcmp on {target_name} must keep the non-string fallback"
        );
        assert!(
            body_of("__rt_hash_natcasecmp").contains(&tail.replace("natcmp", "natcasecmp")),
            "__rt_hash_natcasecmp on {target_name} must tail-branch into __rt_natcasecmp"
        );

        // The engine calls whatever the entry point selected; naming a comparator here
        // would re-attach every sort to it and defeat the split above.
        let engine = body_of("hash_sort_links");
        assert!(
            engine.contains(indirect_call),
            "the {target_name} hash sort engine must call the selected comparator indirectly"
        );
        for fixed in ["__rt_php_compare", "__rt_hash_natcmp"] {
            assert!(
                !engine.contains(fixed),
                "the {target_name} hash sort engine must not name {fixed} directly"
            );
        }
    }
}

/// Issue repro: `natsort()` on an INDEXED receiver used to reindex it.
///
/// `php -n -r '$n=["img12","img2"]; natsort($n); echo json_encode($n);'` prints
/// `{"1":"img2","0":"img12"}` — `renumber = 0` moves the iteration order and leaves every key
/// on its own value, so a packed receiver whose keys ARE its slot positions cannot express the
/// answer. The receiver is now promoted to int-keyed hash storage before the sort, which is why
/// all five consumers below agree with php at once: the object-shaped `json_encode`, the permuted
/// `foreach` keys, the `$n[0]` lookup that still finds the ORIGINAL first element, the count, and
/// the iteration-ordered `implode`.
#[test]
fn test_natsort_on_an_indexed_receiver_preserves_its_keys() {
    let out = compile_and_run(
        r#"<?php
$n = ["img12", "img10", "img2", "img1"];
natsort($n);
echo json_encode($n), "|";
foreach ($n as $k => $v) { echo $k, "=", $v, ";"; }
echo "|", $n[0], "|", count($n), "|", implode(",", $n);
"#,
    );
    assert_eq!(
        out,
        "{\"3\":\"img1\",\"2\":\"img2\",\"1\":\"img10\",\"0\":\"img12\"}\
         |3=img1;2=img2;1=img10;0=img12;|img12|4|img1,img2,img10,img12"
    );
}

/// `natcasesort()` promotes the same way, and the promoted receiver stays an ordinary hash
/// afterwards: `ksort()` puts the keys back in `0,1` order (which php then prints as a JSON
/// LIST, not an object), a bare append picks the next free integer key, and `unset()` removes
/// one without renumbering the rest.
#[test]
fn test_natcasesort_promotes_and_the_receiver_stays_a_hash() {
    let out = compile_and_run(
        r#"<?php
$c = ["IMG12", "img10", "Img2"];
natcasesort($c);
echo json_encode($c), "|";
foreach ($c as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
$d = ["img12", "img2"];
natsort($d);
ksort($d);
echo json_encode($d), "|";
$e = ["img12", "img2"];
natsort($e);
$e[] = "img3";
unset($e[0]);
echo json_encode($e), "|", json_encode(array_keys($e)), "|", json_encode(array_values($e));
"#,
    );
    assert_eq!(
        out,
        "{\"2\":\"Img2\",\"1\":\"img10\",\"0\":\"IMG12\"}|2=Img2;1=img10;0=IMG12;\
         |[\"img12\",\"img2\"]|{\"1\":\"img2\",\"2\":\"img3\"}|[1,2]|[\"img2\",\"img3\"]"
    );
}

/// The promotion also has to work where the checker's environment does NOT reach the lowering.
///
/// A function body is lowered against its SIGNATURE's parameter types, not against the checker's
/// final environment, so `$a` is still packed there and the lowering performs the conversion
/// itself. That leaves the frame slot boxed `Mixed` (a slot cannot be both packed and hashed),
/// which is the shape that exposed the hash sorters' ownership pairing: the receiver is unboxed
/// with an extra owned reference, and without releasing the slot's box first AND re-boxing
/// without consuming that reference, the table was split, released twice, and read back freed —
/// every one of these four calls printed the EMPTY string. The `$src` check pins that php's
/// by-VALUE parameter is still a copy: the caller's array is untouched.
#[test]
fn test_natsort_promotes_a_by_value_parameter_and_keeps_the_table_alive() {
    let out = compile_and_run(
        r#"<?php
function nat(array $a): string {
    natsort($a);
    return json_encode($a);
}
$src = ["img12", "img2"];
echo nat($src), "|", json_encode($src), "|";

function boxed(array $a): string {
    $a["k"] = "img2";
    natsort($a);
    return json_encode($a);
}
echo boxed(["img12", "img10"]), "|";

function boxedasort(array $a): string {
    $a["k"] = "img2";
    asort($a);
    return json_encode($a);
}
echo boxedasort(["img12", "img10"]), "|";

function boxedksort(array $a): string {
    $a["k"] = "img2";
    ksort($a);
    return json_encode($a);
}
echo boxedksort(["img12", "img10"]);
"#,
    );
    assert_eq!(
        out,
        "{\"1\":\"img2\",\"0\":\"img12\"}|[\"img12\",\"img2\"]\
         |{\"k\":\"img2\",\"1\":\"img10\",\"0\":\"img12\"}\
         |{\"1\":\"img10\",\"0\":\"img12\",\"k\":\"img2\"}\
         |{\"0\":\"img12\",\"1\":\"img10\",\"k\":\"img2\"}"
    );
}

/// A receiver that ALIASES other storage is deliberately left packed.
///
/// Converting `$s` would rewrite the storage `$r` also names while `$r` stays compiled against
/// the packed layout, which then reads the hash header as elements — measured, an unguarded
/// promotion printed `[5,0]` for `json_encode($r)`. Both the checker and the lowering skip
/// reference aliases, so the ORDER php produces still holds through every name for the array
/// (asserted here, and the assertion a corrupted receiver breaks); only the permuted KEYS are
/// missing, which is this receiver shape's tracked divergence.
#[test]
fn test_natsort_on_a_reference_alias_keeps_packed_storage() {
    let out = compile_and_run(
        r#"<?php
$r = ["img12", "img10", "img2"];
$s = &$r;
natsort($s);
echo implode(",", $r), "|", implode(",", $s), "|", count($r);
"#,
    );
    assert_eq!(out, "img2,img10,img12|img2,img10,img12|3");
}

/// The comparator's own edge cases, asserted through the KEYS this time.
///
/// `tests/codegen/io/filesystem.rs` pins the same four fixtures through `implode()`, which reads
/// only the iteration order. These are the permutations php leaves behind for each — whitespace
/// skipped before a field, digit runs compared as integers, a leading zero turning the field
/// fractional, case folded UP, and PHP 8's stable order for the three equal `img2` values, which
/// keeps keys `0`, `2`, `3` in that relative order.
#[test]
fn test_promoted_natural_sorts_permute_keys_on_the_comparator_edge_cases() {
    let out = compile_and_run(
        r#"<?php
$n = ["img2", "img1", "img2", "img2"];
natsort($n);
echo json_encode($n), "|";
$m = ["a 3", "a002", "a01", "a1", "a2", "a10"];
natsort($m);
echo json_encode($m), "|";
$c = ["B", "a", "C", "b", "A_x", "Ax"];
natcasesort($c);
echo json_encode($c), "|";
$d = ["x", "", " ", "x1y10", "x1y9", "x01y2"];
natsort($d);
echo json_encode($d);
"#,
    );
    assert_eq!(
        out,
        "{\"1\":\"img1\",\"0\":\"img2\",\"2\":\"img2\",\"3\":\"img2\"}\
         |{\"1\":\"a002\",\"2\":\"a01\",\"3\":\"a1\",\"4\":\"a2\",\"0\":\"a 3\",\"5\":\"a10\"}\
         |{\"1\":\"a\",\"5\":\"Ax\",\"4\":\"A_x\",\"0\":\"B\",\"3\":\"b\",\"2\":\"C\"}\
         |{\"1\":\"\",\"2\":\" \",\"0\":\"x\",\"5\":\"x01y2\",\"4\":\"x1y9\",\"3\":\"x1y10\"}"
    );
}

/// The promotion allocates one hash for the receiver and nothing else: `Op::ArrayToHash` consumes
/// the packed array it converts, and the relinking sorter only rewrites the iteration chain. A
/// loop is what makes a per-iteration leak visible at all — one leaked table is a rounding error,
/// twenty is not.
#[test]
fn test_promoted_natural_sorts_leave_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 20; $i++) {
    $n = ["img12", "img10", "img2"];
    natsort($n);
    $m = ["IMG12", "img10", "Img2"];
    natcasesort($m);
    echo implode(",", $n), implode(",", $m);
}
"#,
    );
    assert!(
        out.stdout.ends_with("img2,img10,img12Img2,img10,IMG12"),
        "stdout: {} stderr: {}",
        out.stdout,
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// The natural-order sorters reuse the same relinking engine, so they must be just as
/// heap-neutral as `ksort`/`asort`: no key or value changes ownership, and the copy-on-write
/// split the receiver asks for is the only allocation involved.
#[test]
fn test_natural_hash_sorts_leave_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$m = ["bb" => "img10", "aa" => "img2", "cc" => "img1"];
$n = $m;
natsort($m);
natcasesort($n);
foreach ($m as $k => $v) { echo $k, $v; }
foreach ($n as $k => $v) { echo $k, $v; }
"#,
    );
    assert_eq!(
        out.stdout, "ccimg1aaimg2bbimg10ccimg1aaimg2bbimg10",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// `ksort()` on an indexed receiver is deliberately NOT promoted.
///
/// A packed array's keys are already `0..n-1` in ascending order, so php's `ksort()` leaves
/// both the order and the LIST shape untouched: measured, `$f=[3,1,2]; ksort($f);` prints
/// `[3,1,2]` and `$f[0]` is `3`. elephc already matched that byte for byte, so converting the
/// receiver would allocate a hash to reproduce the packed answer it already had. `krsort()` is
/// the opposite case — descending key order has no packed form — and keeps its explanatory
/// refusal; see `crate::types::key_preserving_sort_promotes` for both.
#[test]
fn test_ksort_leaves_an_indexed_receiver_packed() {
    let out = compile_and_run(
        r#"<?php
$f = [3, 1, 2];
ksort($f);
echo json_encode($f), "|", $f[0];
"#,
    );
    assert_eq!(out, "[3,1,2]|3");
}

