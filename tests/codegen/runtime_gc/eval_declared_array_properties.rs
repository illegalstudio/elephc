//! Purpose:
//! Verifies declared array property storage across native and opaque eval boundaries.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Declared properties preserve packed and sparse keyed layouts in one boxed ABI.
//! - Eval rejects non-array replacements without corrupting the existing property owner.

use crate::support::*;

/// Declared instance and static arrays preserve sparse keys across eval writes and native reads.
#[test]
fn test_core_eval_sparse_declared_array_properties_preserve_native_reads() {
    let source = r#"<?php
function describeSparseArray(array $items): string {
    return count($items) . ":" . implode(",", array_keys($items));
}

class NativeSparseArrayProperties {
    public array $items = [10, 20, 30];
    public static array $shared = [40, 50, 60];

    public function items(): array {
        return $this->items;
    }

    public static function shared(): array {
        return self::$shared;
    }

    public function describe(): string {
        return describeSparseArray($this->items())
            . "|" . describeSparseArray(self::shared());
    }
}

class NativePromotedSparseArrayProperty {
    public function __construct(public array $items) {
    }

    public function describe(): string {
        return describeSparseArray($this->items);
    }
}

$promoted = new NativePromotedSparseArrayProperty([80, 90, 100]);
$source = '$owner = new NativeSparseArrayProperties();
unset($owner->items[1]);
unset(NativeSparseArrayProperties::$shared[0]);
$copy = $owner->items;
$owner->items[1000000] = 70;
unset($promoted->items[1]);
echo $owner->describe(), "|", count($copy), ":", implode(",", array_keys($copy)), "|";
echo $promoted->describe();' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(
        compile_and_run(source),
        "3:0,2,1000000|2:1,2|2:0,2|2:0,2"
    );
}

/// A free-function array return preserves every storage family in a three-way join.
#[test]
fn test_core_declared_array_property_free_function_return_join_uses_boxed_abi() {
    let source = r#"<?php
class FreeFunctionArrayReturnOwner {
    public array $items = ["property" => 41];
}

function chooseFreeFunctionArray(FreeFunctionArrayReturnOwner $owner, int $mode): array {
    if ($mode === 0) {
        return ["indexed"];
    }
    if ($mode === 1) {
        return ["hash" => 42];
    }
    return $owner->items;
}

$owner = new FreeFunctionArrayReturnOwner();
$indexed = chooseFreeFunctionArray($owner, 0);
$hash = chooseFreeFunctionArray($owner, 1);
$property = chooseFreeFunctionArray($owner, 2);
echo $indexed[0], ":", $hash["hash"], ":", $property["property"];
"#;
    assert_eq!(compile_and_run(source), "indexed:42:41");
}

/// A method array return preserves every storage family with the boxed branch first.
#[test]
fn test_core_declared_array_property_method_return_join_uses_boxed_abi() {
    let source = r#"<?php
class MethodArrayReturnOwner {
    public array $items = ["property" => 43];

    public function choose(int $mode): array {
        if ($mode === 0) {
            return $this->items;
        }
        if ($mode === 1) {
            return ["indexed"];
        }
        return ["hash" => 44];
    }
}

$owner = new MethodArrayReturnOwner();
$property = $owner->choose(0);
$indexed = $owner->choose(1);
$hash = $owner->choose(2);
echo $property["property"], ":", $indexed[0], ":", $hash["hash"];
"#;
    assert_eq!(compile_and_run(source), "43:indexed:44");
}

/// Native array property writes preserve both storage layouts and their pre-write COW copies.
#[test]
fn test_core_native_declared_array_property_writes_preserve_copies() {
    let source = r#"<?php
class NativeArrayPropertyWrites {
    public array $items = [1];
    public static array $shared = ["old" => 2];

    public function write(): void {
        $this->items["key"] = 3;
        $this->items[] = 4;
        self::$shared["key"] = 5;
        self::$shared[] = 6;
    }
}

final class NativeArrayPromotedImplode {
    public function __construct(public array $items) {}
}

$owner = new NativeArrayPropertyWrites();
$copy = $owner->items;
$staticCopy = NativeArrayPropertyWrites::$shared;
$owner->write();
echo implode(",", array_keys($owner->items)), ":", implode(",", $owner->items), "|";
echo implode(",", array_keys(NativeArrayPropertyWrites::$shared)), ":", implode(",", NativeArrayPropertyWrites::$shared), "|";
echo implode(",", $copy), "|", implode(",", $staticCopy);
echo "|", implode(",", (new NativeArrayPromotedImplode([7, 8]))->items);
echo ":", implode(",", (new NativeArrayPromotedImplode([true, false, true]))->items);
echo ":", implode(",", (new NativeArrayPromotedImplode(["a", "b"]))->items);
"#;
    assert_eq!(
        compile_and_run(source),
        "0,key,1:1,3,4|old,key,0:2,5,6|1|2|7,8:1,,1:a,b"
    );
}

/// `array_values()` normalizes both runtime layouts without consuming property storage.
#[test]
fn test_core_array_values_accepts_boxed_declared_array_properties() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class NativeArrayValuesProperties {
    public function __construct(
        public array $ints,
        public array $flags,
        public array $words,
        public array $sparse,
    ) {
    }
}

$owner = new NativeArrayValuesProperties(
    [7, 8],
    [true, false, true],
    ["a", "b"],
    [2 => "two", "name" => "Ada", 1000000 => "far"],
);
$ints = array_values($owner->ints);
$flags = array_values($owner->flags);
$words = array_values($owner->words);
$sparse = array_values($owner->sparse);
$owner->ints[0] = 99;
$owner->sparse[2] = "changed";
$flagText = "";
foreach ($flags as $flag) {
    $flagText .= $flag ? "T" : "F";
}
echo implode(",", $ints), "|", $flagText, "|", implode(",", $words), "|";
echo implode(",", $sparse), "|", $ints[0], "|", implode(",", array_values($owner->sparse));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "7,8|TFT|a,b|two,Ada,far|7|changed,Ada,far");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected normalized array_values results to release cleanly, got: {}",
        out.stderr
    );
}

/// Eval reports typed-property errors for non-array replacements and keeps both old values live.
#[test]
fn test_core_eval_declared_array_properties_reject_non_array_replacements() {
    let source = r#"<?php
class NativeArrayPropertyTypeGuard {
    public array $items = ["instance" => 1];
    public static array $shared = ["static" => 2];
}

$source = '$owner = new NativeArrayPropertyTypeGuard();
try { $owner->items = 41; } catch (TypeError $error) { echo "instance|"; }
try { NativeArrayPropertyTypeGuard::$shared = "bad"; } catch (TypeError $error) { echo "static|"; }
echo $owner->items["instance"], ":", NativeArrayPropertyTypeGuard::$shared["static"];' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "instance|static|1:2");
}
