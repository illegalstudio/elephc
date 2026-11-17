//! Purpose:
//! Regression coverage for concrete values collected from typed associative arrays.
//!
//! Called from:
//! - The `codegen_tests` array suite.
//!
//! Key details:
//! - Pinned builders must keep their element layout across typed property stores.
//! - Heap debugging checks temporary boxes and string payloads.

use crate::support::*;

/// A call can replace a global foreach binding without permitting an append conversion.
#[test]
fn test_foreach_hash_scalar_volatile_appends_preserve_runtime_values() {
    let source = r#"<?php
function clobberGlobal(): void { global $g; $g = 'abc'; }
array<int> $ids = [];
foreach (['a' => 1] as $g) { clobberGlobal(); $ids[] = $g; }
foreach ($ids as $item) { echo gettype($item), ':', $item, ';'; }
echo '|';
function collectGlobal(array<string, int> $source): void {
    global $g;
    array<int> $kept = [];
    foreach ($source as $g) { clobberGlobal(); $kept[] = $g; }
    foreach ($kept as $item) { echo gettype($item), ':', $item, ';'; }
    unset($kept, $item);
}
collectGlobal(['a' => 1]);
echo '|';
function collectMissing(array<string, int> $source): void {
    array<int> $kept = [];
    foreach ($source as $value) {}
    $kept[] = $value;
    foreach ($kept as $item) { echo gettype($item); }
    unset($kept, $value, $item);
}
collectMissing([]);
unset($ids, $g, $item);
"#;
    for optimize in [false, true] {
        let out = if optimize { compile_and_run_with_heap_debug(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug(source)) };
        assert!(out.success, "ir-opt={optimize}: {}", out.stderr);
        assert_eq!(out.stdout, "string:abc;|string:abc;|NULL",
            "ir-opt={optimize}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "ir-opt={optimize}: {}", out.stderr);
    }
}

/// Declared arrays still widen for unknown, nullable, or differently typed append values.
#[test]
fn test_foreach_hash_scalar_unproven_appends_keep_runtime_values() {
    let source = r#"<?php
function unknownValue(): mixed { return 'abc'; }
function nullableValue(bool $present): ?int { return $present ? 5 : null; }
function unknownCallable(): mixed { return 'strtoupper'; }
array<int> $ids = [];
$unknown = unknownValue(); $ids[] = $unknown;
$nullable = nullableValue(false); $ids[] = $nullable;
foreach (['a' => 1, 'b' => 'two'] as $value) { $ids[] = $value; }
foreach ($ids as $value) { echo gettype($value), ':', $value, ';'; }
echo '|';
array<string> $names = []; $names[] = 42;
array<float> $floats = []; $floats[] = 42;
array<bool> $flags = []; $flags[] = 42;
foreach ($names as $value) { echo gettype($value), ':', $value, ';'; }
foreach ($floats as $value) { echo gettype($value), ':', $value, ';'; }
foreach ($flags as $value) { echo gettype($value), ':', $value, ';'; }
echo '|';
array<callable> $callbacks = [];
$unknownCallback = unknownCallable(); $callbacks[] = $unknownCallback;
echo gettype($callbacks[0]);
unset($ids, $names, $floats, $flags, $callbacks, $value, $unknown, $nullable, $unknownCallback);
"#;
    for optimize in [false, true] {
        let out = if optimize { compile_and_run_with_heap_debug(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug(source)) };
        assert!(out.success, "ir-opt={optimize}: {}", out.stderr);
        assert_eq!(out.stdout,
            "string:abc;NULL:;integer:1;string:two;|integer:42;integer:42;integer:42;|string",
            "ir-opt={optimize}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "ir-opt={optimize}: {}", out.stderr);
    }
}

/// Flipped values keep the runtime source-key kind after reindexing or key-preserving sorts.
#[test]
fn test_foreach_hash_scalar_flip_preserves_runtime_key_types() {
    let source = r#"<?php
$ascending = ['b' => 2, 'a' => 1];
sort($ascending);
$flipped = array_flip($ascending);
foreach ($flipped as $value) { echo gettype($value), ':', $value, ';'; }
echo '|';
$descending = ['b' => 2, 'a' => 1];
rsort($descending);
foreach (array_flip($descending) as $key => $value) { echo $key, ':', gettype($value), ':', $value, ';'; }
echo '|';
$preserved = ['b' => 2, 'a' => 1];
asort($preserved);
foreach (array_flip($preserved) as $value) { echo gettype($value), ':', $value, ';'; }
echo '|';
$integerKeys = [10 => 'x', 20 => 'y'];
foreach (array_flip($integerKeys) as $value) { echo gettype($value), ':', $value, ';'; }
echo '|', json_encode(array_flip(array_flip($preserved))), '|';
$types = array_map(fn($item): string => gettype($item), array_flip($preserved));
echo implode(',', $types);
echo '|';
array<string, string> $forward = [];
$forward['a'] = 'x'; $forward['b'] = 'y';
array<string, string> $inverse = array_flip($forward);
foreach ($inverse as $k => $v) { echo $k, '=', $v, ';'; }
echo '|';
function invert(array<string, string> $m): array<string, string> { return array_flip($m); }
foreach (invert(['a' => 'x', 'b' => 'y']) as $k => $v) { echo $k, '=', $v, ';'; }
unset($ascending, $flipped, $descending, $preserved, $integerKeys, $value, $types, $forward, $inverse, $k, $v);
"#;
    for optimize in [false, true] {
        let out = if optimize { compile_and_run_with_heap_debug(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug(source)) };
        assert!(out.success, "ir-opt={optimize}: {}", out.stderr);
        assert_eq!(out.stdout,
            "integer:0;integer:1;|2:integer:0;1:integer:1;|string:a;string:b;|integer:10;integer:20;|{\"a\":1,\"b\":2}|string,string|x=a;y=b;|x=a;y=b;",
            "ir-opt={optimize}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "ir-opt={optimize}: {}", out.stderr);
    }
}

/// Keyed and keyless hash loops preserve int, float, bool, and string builders through property stores.
#[test]
fn test_foreach_hash_scalar_values_keep_pinned_builder_types() {
    for (ty, left, right, display, expected) in [
        ("int", "7", "9", "$value", "1:7|1:9|2:7"),
        ("float", "1.25", "-2.5", "$value", "1:1.25|1:-2.5|2:1.25"),
        ("bool", "false", "true", "$value ? 'true' : 'false'", "1:false|1:true|2:false"),
        ("string", "str_repeat('a', 3)", "str_repeat('b', 4)", "$value", "1:aaa|1:bbbb|2:aaa"),
    ] {
        let source = format!(r#"<?php
final class Values
{{
    private array<string, {ty}> $active = [];
    private array<{ty}> $kept = [];
    public function __construct()
    {{
        $this->active['left'] = {left};
        $this->active['right'] = {right};
    }}
    public function rebuild(string $wanted): void
    {{
        array<{ty}> $kept = [];
        foreach ($this->active as $key => $value) {{
            if ($key === $wanted) {{ $kept[] = $value; }}
        }}
        $this->kept = $kept;
    }}
    public function rebuildAll(): void
    {{
        array<{ty}> $kept = [];
        foreach ($this->active as $value) {{ $kept[] = $value; }}
        $this->kept = $kept;
        $this->active = [];
    }}
    public function describe(): void
    {{
        $value = $this->kept[0];
        echo count($this->kept), ':', {display};
    }}
}}
$values = new Values();
$values->rebuild('left'); $values->describe(); echo '|';
$values->rebuild('right'); $values->describe(); echo '|';
$values->rebuildAll(); $values->describe();
unset($values);
"#);
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "{ty}: {}", out.stderr);
        assert_eq!(out.stdout, expected, "{ty}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{ty}: {}", out.stderr);
    }
}
