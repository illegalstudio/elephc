//! Purpose:
//! Covers literal unpacking of boxed PHP arrays and independently owned spread results.
//!
//! Called from:
//! - The runtime GC codegen integration suite on executable targets.
//!
//! Key details:
//! - Declared array parameters and returns keep runtime layout dispatch observable.
//! - Spreads preserve string keys, reindex integer keys, and never consume a borrowed source.

use crate::support::*;

/// Boxed packed and hash spreads preserve key order and overwrite string keys without changing inputs.
#[test]
fn test_core_boxed_array_spread_keys_and_mixed_elements() {
    let source = r#"<?php
function describeBoxedSpread(array $left, array $right): void {
    $result = ["head", ...$left, false, ...$right, "tail"];
    echo implode(",", array_keys($result)), ":", implode(",", $result), "|";
    echo implode(",", $left), ":", implode(",", $right), "|";
}
describeBoxedSpread([10, 20], [30]);
describeBoxedSpread([8 => "first", "same" => "old"], ["same" => "new", -4 => "last"]);
describeBoxedSpread([], []);
"#;
    assert_eq!(compile_and_run(source),
        "0,1,2,3,4,5:head,10,20,,30,tail|10,20:30|0,1,same,2,3,4:head,first,new,,last,tail|first,old:new,last|0,1,2:head,,tail|:|");
}

/// Unannotated closure returns keep hash storage for generic sources and retain the mixed tail value.
#[test]
fn test_core_boxed_array_spread_closure_return_storage() {
    let source = r#"<?php
$spread = function(array $items, mixed $tail) { return [...$items, $tail]; };
$packed = $spread([1, 2], "s");
echo implode(",", $packed), "|";
$named = $spread(["key" => 3, 9 => 4], "t");
echo implode(",", array_keys($named)), ":", implode(",", $named);
"#;
    assert_eq!(compile_and_run(source), "1,2,s|key,0,1:3,4,t");
}

/// Empty method results can be spread beside a raw local without reading a boxed cell as an array.
#[test]
fn test_core_boxed_array_spread_empty_method_and_raw_source() {
    let source = r#"<?php
class BoxedSpreadSource {
    public function none(): array { return []; }
    public function combined(): array { $local = []; return [...$local, ...$this->none()]; }
    public function named(): array { return ["key" => "value"]; }
}
$source = new BoxedSpreadSource();
echo count($source->combined()), "|";
$raw = ["first", "second"];
$result = [...$raw, ...$source->named(), ...$raw];
echo implode(",", array_keys($result)), ":", implode(",", $result), "|", implode(",", $raw);
"#;
    assert_eq!(compile_and_run(source), "0|0,1,key,2,3:first,second,value,first,second|first,second");
}

/// Fresh boxed results and borrowed packed sources each keep balanced owners across repeated spreads.
#[test]
fn test_core_boxed_array_spread_owners_and_cow() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function freshBoxedSpread(): array { return ["fresh" => str_repeat("f", 24)]; }
function copyBoxedSpread(array $items): void {
    $packed = [str_repeat("p", 24)];
    for ($i = 0; $i < 30; $i++) {
        $result = [...$packed, ...$items, ...freshBoxedSpread(), ...$packed];
        if (count($result) !== 4) { echo "bad-count"; }
        $result["key"] = "changed";
        if ($items["key"] !== "original") { echo "bad-cow"; }
        unset($result);
    }
    echo strlen($packed[0]), ":", $items["key"];
}
copyBoxedSpread(["key" => "original"]);
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "24:original", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A dynamic non-array spread throws a catchable Error instead of dereferencing an invalid payload.
#[test]
fn test_core_boxed_array_spread_rejects_scalar_values() {
    let source = r#"<?php
function rejectBoxedSpread(mixed $items): array { return [...$items]; }
try { rejectBoxedSpread(7); } catch (Error $error) { echo $error->getMessage(); }
"#;
    assert_eq!(compile_and_run(source), "Only arrays and Traversables can be unpacked");
}

/// Invalid spreads retire earlier temporary owners and prevent later element side effects.
#[test]
fn test_core_boxed_array_spread_throw_order_and_cleanup() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function ownedSpreadItems(): array { return [str_repeat("a", 24)]; }
function invalidSpreadItems(): mixed { return str_repeat("b", 24); }
function unreachableSpreadTail(): int { echo "bad-tail"; return 9; }
function throwingSpreadTail(): int { throw new Exception("tail"); }
try {
    $result = [str_repeat("c", 24), ...ownedSpreadItems(), ...invalidSpreadItems(), unreachableSpreadTail()];
} catch (Error $error) {
    echo "invalid|";
    unset($error);
}
try {
    $result = [...ownedSpreadItems(), throwingSpreadTail()];
} catch (Exception $error) {
    echo $error->getMessage();
    unset($error);
}
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "invalid|tail", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
