//! Purpose: verify assignment-expression ownership at runtime evaluation boundaries.
//! Called from: the runtime GC integration suite.
//! Key details: assignment results must preserve aliases without stranding temporary owners.

use crate::support::*;

#[test]
fn test_eval_assignment_conditions_release_owned_values_and_preserve_aliases() {
    let out = compile_cli_files_and_run(
        &[
            ("main.php", r#"<?php
$file = 'runtime';
include __DIR__ . '/' . $file . '.php';
"#),
            ("runtime.php", r#"<?php
class AssignmentValue {
    public function __destruct() { echo 'd'; }
}
class AssignmentHolder {
    public mixed $value = null;
}
if ($a = new AssignmentValue()) { echo 'a'; }
unset($a);
$b = new AssignmentValue();
$c = ($a = $b);
unset($b, $a);
echo 'b';
unset($c);
$holder = new AssignmentHolder();
if ($holder->value = new AssignmentValue()) { echo 'p'; }
$holder->value = null;
$array = [];
if ($array['value'] = new AssignmentValue()) { echo 'q'; }
unset($array);
"#),
        ],
        "main.php",
    );
    assert_eq!(out, "adbdpdqd");
}

#[test]
fn test_eval_array_append_releases_owned_values_and_preserves_borrowed_values() {
    let out = compile_cli_files_and_run(
        &[
            ("main.php", "<?php $file = 'runtime'; include __DIR__ . '/' . $file . '.php';"),
            ("runtime.php", r#"<?php
class AppendValue {
    public function __destruct() { echo 'd'; }
}
$array = [];
$array[] = new AppendValue();
unset($array);
echo 'a';
$value = new AppendValue();
$array = [];
$array[] = $value;
unset($array);
echo 'b';
unset($value);
$array = [];
$array[] = [new AppendValue(), []];
unset($array);
echo 'c';
"#),
        ],
        "main.php",
    );
    assert_eq!(out, "dabddc");
}

#[test]
fn test_eval_indexed_array_literal_releases_generated_keys() {
    assert_eval_heap_stable_across_branches(r#"<?php
$many = $argv[1] === 'many';
eval('if ($many) { $array = [[1, 2], [3, 4], [5, 6], [7, 8]]; } else { $array = [[1, 2]]; } unset($array);');
echo 'ok';
"#);
}

#[test]
fn test_eval_string_bytes_metadata_reads_do_not_accumulate_copies() {
    assert_eval_heap_stable_across_branches(r#"<?php
$many = $argv[1] === 'many';
$reflection = new ReflectionClass('stdClass');
eval('if ($many) { $name = $reflection->getName(); $name = $reflection->getName(); $name = $reflection->getName(); $name = $reflection->getName(); } else { $name = $reflection->getName(); } unset($name);');
echo 'ok';
"#);
}

#[test]
fn test_eval_assignment_to_absent_locals_releases_fetch_placeholders() {
    assert_eval_heap_stable_across_branches(r#"<?php
$many = $argv[1] === 'many';
eval('function assignAbsentLocals(bool $many): void { if ($many) { if ($a = 1) {} if ($b = 2) {} if ($c = 3) {} if ($d = 4) {} } else { if ($a = 1) {} } } assignAbsentLocals($many);');
echo 'ok';
"#);
}

#[test]
fn test_eval_destructuring_releases_literal_key_cells() {
    assert_eval_heap_stable_across_branches(r#"<?php
$many = $argv[1] === 'many';
eval('function destructureValues(bool $many): void { $source = [10, 20]; if ($many) { [$a, $b] = $source; [$a, $b] = $source; [$a, $b] = $source; [$a, $b] = $source; } else { [$a, $b] = $source; } } destructureValues($many);');
echo 'ok';
"#);
}

/// Repeated native calls must not retain another owner for each global transfer.
#[test]
fn test_eval_native_global_transfers_do_not_accumulate_owners() {
    assert_eval_heap_stable_across_branches(r#"<?php
function nativeTouch(): void {
    global $counter;
    $counter = 9;
}
$GLOBALS['counter'] = 1;
$many = $argv[1] === 'many';
eval('if ($many) { nativeTouch(); nativeTouch(); nativeTouch(); nativeTouch(); } else { nativeTouch(); }');
echo 'ok';
"#);
}

/// Heap-backed replacements must release superseded payloads, not just scalar cells.
#[test]
fn test_eval_native_global_heap_replacements_do_not_accumulate_owners() {
    assert_eval_heap_stable_across_branches(r#"<?php
function nativeAppend(): void {
    $GLOBALS['counter'] = $GLOBALS['counter'] . 'x';
}
$GLOBALS['counter'] = 'seed';
$many = $argv[1] === 'many';
eval('if ($many) { nativeAppend(); nativeAppend(); nativeAppend(); nativeAppend(); } else { nativeAppend(); }');
unset($GLOBALS['counter']);
echo 'ok';
"#);
}

#[test]
fn test_eval_native_global_keyword_heap_replacements_do_not_accumulate_owners() {
    assert_eval_heap_stable_across_branches(r#"<?php
function nativeAppend(): void {
    global $counter;
    $counter .= 'x';
}
$GLOBALS['counter'] = 'seed';
$many = $argv[1] === 'many';
eval('if ($many) { nativeAppend(); nativeAppend(); nativeAppend(); nativeAppend(); } else { nativeAppend(); }');
unset($GLOBALS['counter']);
echo 'ok';
"#);
}

#[test]
fn test_native_global_heap_replacements_without_eval_do_not_accumulate_owners() {
    assert_eval_heap_stable_across_branches(r#"<?php
function nativeAppend(): void {
    $GLOBALS['counter'] = $GLOBALS['counter'] . 'x';
}
$GLOBALS['counter'] = 'seed';
$many = $argv[1] === 'many';
if ($many) { nativeAppend(); nativeAppend(); nativeAppend(); nativeAppend(); } else { nativeAppend(); }
unset($GLOBALS['counter']);
echo 'ok';
"#);
}

/// Holds binary, eval activation, argument count and argument lengths constant.
#[test]
fn test_eval_append_key_scan_releases_temporary_cells() {
    assert_eval_heap_stable_across_branches(r#"<?php
$many = $argv[1] === 'many';
eval('function appendValues(bool $many): void { $a = [10, 20, 30]; if ($many) { $a[] = 40; $a[] = 50; $a[] = 60; $a[] = 70; } else { $a[] = 40; } } appendValues($many);');
echo 'ok';
"#);
}

/// Holds binary, eval activation, argument count and argument lengths constant.
#[test]
fn test_eval_append_key_scan_releases_ignored_and_smaller_keys() {
    assert_eval_heap_stable_across_branches(r#"<?php
$many = $argv[1] === 'many';
eval('function appendSparseValues(bool $many): void { $a = [8 => 10, 2 => 20, "label" => 30]; if ($many) { $a[] = 40; $a[] = 50; $a[] = 60; $a[] = 70; } else { $a[] = 40; } } appendSparseValues($many);');
echo 'ok';
"#);
}

/// Holds binary, eval activation, argument count and argument lengths constant.
#[test]
fn test_eval_throwable_constructor_releases_overwritten_default_cells() {
    assert_eval_heap_stable_across_branches(r#"<?php
$many = $argv[1] === 'many';
$source = 'if ($many) { $exception = new Exception("message"); unset($exception); $exception = new Exception("message"); unset($exception); } $exception = new Exception("message"); unset($exception);';
eval($source . ' //' . $argv[1]);
echo 'ok';
"#);
}

#[test]
fn test_eval_array_spread_releases_owned_entry_cells() {
    assert_eval_heap_stable_across_branches(r#"<?php
$many = $argv[1] === 'many';
$source = '$input = ["name" => "value", 7 => 9]; if ($many) { $out = [...$input]; unset($out); $out = [...$input]; unset($out); } $out = [...$input]; unset($out); unset($input);';
eval($source . ' //' . $argv[1]);
echo 'ok';
"#);
}

#[test]
fn test_eval_native_mixed_ref_slot_owner_is_balanced_when_unchanged() {
    assert_eval_heap_stable_across_branches(r#"<?php
class MixedRefOwner { public function __construct(mixed &$value) {} }
$many = $argv[1] === 'many';
$source = '$value = 1; if ($many) { $box = new MixedRefOwner($value); unset($box); $box = new MixedRefOwner($value); unset($box); } $box = new MixedRefOwner($value); unset($box); unset($value);';
eval($source . ' //' . $argv[1]);
echo 'ok';
"#);
}

#[test]
fn test_eval_native_mixed_ref_writeback_preserves_shared_source() {
    let out = compile_cli_files_and_run(&[("main.php", r#"<?php
class MixedRefCopy {
    public function __construct(mixed &$target, mixed $source) { $target = $source; }
}
$file = 'runtime';
include __DIR__ . '/' . $file . '.php';
"#), ("runtime.php", r#"<?php
$source = 'payload';
$target = 1;
$box = new MixedRefCopy($target, $source);
echo $source, ':', $target;
$source = 'other';
echo ':', $target;
"#)], "main.php");
    assert_eq!(out, "payload:payload:payload");
}

#[test]
fn test_eval_native_constructor_argument_indexes_are_released() {
    assert_eval_heap_stable_across_branches(r#"<?php
class ArgumentOwner {
    public function __construct(int $a, int $b, int $c) {}
}
$many = $argv[1] === 'many';
$source = 'if ($many) { $object = new ArgumentOwner(1, 2, 3); unset($object); $object = new ArgumentOwner(1, 2, 3); unset($object); } $object = new ArgumentOwner(1, 2, 3); unset($object);';
eval($source . ' //' . $argv[1]);
echo 'ok';
"#);
}

#[test]
fn test_eval_native_constructor_conversions_preserve_caller_and_stored_values() {
    let out = compile_cli_files_and_run(&[("main.php", r#"<?php
class BorrowedConstructorSource { public string $name = 'Ada'; }
class BorrowedConstructorTarget {
    public array $items;
    public BorrowedConstructorSource $source;
    public function __construct(array $items, BorrowedConstructorSource $source) {
        $this->items = $items;
        $this->source = $source;
        $items[] = 99;
    }
}
$file = 'runtime';
include __DIR__ . '/' . $file . '.php';
"#), ("runtime.php", r#"<?php
$items = [1, 2];
$source = new BorrowedConstructorSource();
$box = new BorrowedConstructorTarget($items, $source);
echo count($items), ':', count($box->items), ':', $source->name, ':', $box->source->name;
unset($box);
echo ':', $source->name, ':', count($items);
"#)], "main.php");
    assert_eq!(out, "2:2:Ada:Ada:Ada:2");
}

#[test]
fn test_eval_native_non_string_default_argument_owners_are_released() {
    assert_eval_heap_stable_across_branches(r#"<?php
class DefaultArgumentLeaf {}
class NonStringDefaultOwner {
    public function __construct(array $items = [], DefaultArgumentLeaf $object = new DefaultArgumentLeaf(), mixed $value = []) {}
}
$many = $argv[1] === 'many';
$source = 'if ($many) { $object = new NonStringDefaultOwner(); unset($object); $object = new NonStringDefaultOwner(); unset($object); } $object = new NonStringDefaultOwner(); unset($object);';
eval($source . ' //' . $argv[1]);
echo 'ok';
"#);
}

#[test]
fn test_eval_native_constructor_default_argument_owners_are_released() {
    assert_eval_heap_stable_across_branches(r#"<?php
class DefaultArgumentOwner {
    public function __construct(int $a = 1, string $b = "default", mixed $c = null) {}
}
$many = $argv[1] === 'many';
$source = 'if ($many) { $object = new DefaultArgumentOwner(); unset($object); $object = new DefaultArgumentOwner(); unset($object); } $object = new DefaultArgumentOwner(); unset($object);';
eval($source . ' //' . $argv[1]);
echo 'ok';
"#);
}

#[test]
fn test_eval_native_array_property_replacement_releases_previous_owner() {
    assert_eval_heap_stable_across_branches(r#"<?php
class ArrayPropertyOwner { public array $value = []; }
$many = $argv[1] === 'many';
$owner = new ArrayPropertyOwner();
$source = 'if ($many) { $owner->value = [1]; $owner->value = ["key" => 2]; } $owner->value = [3];';
eval($source . ' //' . $argv[1]);
unset($owner);
echo 'ok';
"#);
}

#[test]
fn test_eval_native_mixed_property_replacement_releases_previous_owner() {
    assert_eval_heap_stable_across_branches(r#"<?php
class MixedPropertyOwner { public mixed $value = null; }
$many = $argv[1] === 'many';
$owner = new MixedPropertyOwner();
$source = 'if ($many) { $owner->value = [1]; $owner->value = "text"; } $owner->value = [3];';
eval($source . ' //' . $argv[1]);
unset($owner);
echo 'ok';
"#);
}

#[test]
fn test_eval_native_object_property_replacement_runs_destructor() {
    let out = compile_cli_files_and_run(&[("main.php", r#"<?php
class ReplacedValue { public function __destruct() { echo 'd'; } }
class ObjectPropertyOwner { public ReplacedValue $value; }
$file = 'runtime';
include __DIR__ . '/' . $file . '.php';
"#), ("runtime.php", r#"<?php
$owner = new ObjectPropertyOwner();
$owner->value = new ReplacedValue();
$owner->value = new ReplacedValue();
echo 'r';
unset($owner);
"#)], "main.php");
    assert_eq!(out, "drd");
}

#[test]
fn test_eval_imported_native_owner_unset_releases_property() {
    let out = compile_cli_files_and_run(&[("main.php", r#"<?php
class ImportedOwnedValue { public function __destruct() { echo 'd'; } }
class ImportedPropertyOwner { public ImportedOwnedValue $value; }
$owner = new ImportedPropertyOwner();
$file = 'runtime';
include __DIR__ . '/' . $file . '.php';
"#), ("runtime.php", r#"<?php
$owner->value = new ImportedOwnedValue();
$owner->value = new ImportedOwnedValue();
echo 'r';
unset($owner);
"#)], "main.php");
    assert_eq!(out, "drd");
}

#[test]
fn test_eval_native_string_property_replacement_releases_previous_owner() {
    assert_eval_heap_stable_across_branches(r#"<?php
class StringPropertyOwner { public string $value = "seed"; }
$many = $argv[1] === 'many';
$owner = new StringPropertyOwner();
$source = 'if ($many) { $owner->value = "first"; $owner->value = "second"; } $owner->value = "last";';
eval($source . ' //' . $argv[1]);
unset($owner);
echo 'ok';
"#);
}

#[test]
fn test_eval_native_method_argument_owners_are_released() {
    assert_eval_heap_stable_across_branches(r#"<?php
class ArgumentReader {
    public function read(int $a, int $b, int $c): int { return $a + $b + $c; }
    public static function readStatic(int $a, int $b, int $c): int { return $a + $b + $c; }
}
$many = $argv[1] === 'many';
$reader = new ArgumentReader();
$source = 'if ($many) { $reader->read(1, 2, 3); ArgumentReader::readStatic(1, 2, 3); } $reader->read(1, 2, 3); ArgumentReader::readStatic(1, 2, 3);';
eval($source . ' //' . $argv[1]);
unset($reader);
echo 'ok';
"#);
}

#[test]
fn test_eval_native_method_argument_owner_preserves_return_alias() {
    let out = compile_cli_files_and_run(&[("main.php", r#"<?php
class AliasReader {
    public function read(mixed $value): mixed { return $value; }
    public static function readStatic(mixed $value): mixed { return $value; }
}
$reader = new AliasReader();
$file = 'runtime';
include __DIR__ . '/' . $file . '.php';
unset($reader);
"#), ("runtime.php", r#"<?php
$value = $reader->read("payload"); echo $value, ':'; unset($value);
$value = AliasReader::readStatic("payload"); echo $value; unset($value);
"#)], "main.php");
    assert_eq!(out, "payload:payload");
}

#[test]
fn test_eval_nested_native_array_calls_release_argument_results() {
    assert_eval_heap_stable_across_branches(r#"<?php
class ArrayHolder {
    private array $arguments = [];
    public function setArguments(array $arguments): static {
        $this->arguments = $arguments;
        return $this;
    }
    public function getArguments(): array { return $this->arguments; }
}
$code = '$holder = new ArrayHolder(); $source = ["key" => "payload"];
$holder->setArguments($source);
if ($argv[1] === "many") {
    $holder->setArguments($holder->getArguments());
    $holder->setArguments($holder->getArguments());
    $holder->setArguments($holder->getArguments());
} else { $holder->setArguments($holder->getArguments()); }
unset($holder, $source);';
eval($code);
echo 'ok';
"#);
}

#[test]
fn test_native_array_setter_roundtrip_releases_conversion_owners() {
    assert_eval_heap_stable_across_branches(r#"<?php
class ArrayHolder {
    private array $arguments = [];
    public function setArguments(array $arguments): static {
        $this->arguments = $arguments;
        return $this;
    }
    public function getArguments(): array { return $this->arguments; }
}
function exercise(): void {
    $holder = new ArrayHolder();
    $source = ['key' => 'payload'];
    $holder->setArguments($source);
    $holder->setArguments($holder->getArguments());
    unset($holder, $source);
}
if ($argv[1] === 'many') {
    exercise(); exercise(); exercise(); exercise();
} else { exercise(); }
echo 'ok';
"#);
}

fn assert_eval_heap_stable_across_branches(source: &str) {
    let dir = make_cli_test_dir("elephc_eval_temporary_ownership");
    fs::write(dir.join("main.php"), source).unwrap();
    let compiled = elephc_cli_command(&dir)
        .args(["--heap-debug", "--quiet", "main.php"])
        .output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    // Eval keeps process-level metadata. Compare one binary, one eval activation and
    // equal-sized arguments; only the PHP branch under test changes.
    let heap = |argument: &str| {
        let out = Command::new(dir.join("main"))
            .current_dir(&dir).arg(argument).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(out.stdout, b"ok");
        String::from_utf8_lossy(&out.stderr).lines()
            .find(|line| line.starts_with("HEAP DEBUG: leak summary:"))
            .expect("heap debug summary").to_owned()
    };
    assert_eq!(heap("once"), heap("many"), "evaluated branch accumulated allocations");
    fs::remove_dir_all(&dir).unwrap();
}
