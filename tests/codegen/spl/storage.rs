//! Purpose:
//! End-to-end tests for SPL storage iterator classes.
//! Covers EmptyIterator, ArrayIterator, and ArrayObject as Phase 5 storage foundations.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - ArrayIterator and ArrayObject preserve insertion-order keys through Mixed keys/values storage.

/// Destructors observe published nested replacements, even when throwing or retargeting the parent.
#[test]
fn test_spl_and_mixed_nested_overwrite_publication_survives_destructor_reentry() {
    let out = crate::support::compile_and_run_with_heap_debug(r#"<?php
class NestedPublicationNew { public string $label = 'new'; }
class SplPublicationOld {
    public bool $throwing = false; public bool $retarget = false;
    public function __destruct() {
        global $splPublication;
        echo $splPublication['x'][0]->label, '|';
        if ($this->retarget) { $splPublication['x'] = ['retargeted']; }
        if ($this->throwing) { throw new Exception('retired'); }
    }
}
class MixedPublicationOld {
    public bool $throwing = false; public bool $retarget = false;
    public function __destruct() {
        global $mixedPublication;
        echo $mixedPublication['x'][0]->label, '|';
        if ($this->retarget) { $mixedPublication['x'] = ['retargeted']; }
        if ($this->throwing) { throw new Exception('retired'); }
    }
}
function makeSplPublication(bool $throwing, bool $retarget): ArrayObject {
    $old = new SplPublicationOld(); $old->throwing = $throwing; $old->retarget = $retarget;
    return new ArrayObject(['x' => [$old]]);
}
function makeMixedPublication(bool $throwing, bool $retarget): mixed {
    $old = new MixedPublicationOld(); $old->throwing = $throwing; $old->retarget = $retarget;
    return ['x' => [$old]];
}
$splPublication = makeSplPublication(false, false);
$splPublication['x'][0] = new NestedPublicationNew(); echo $splPublication['x'][0]->label, '|';
unset($splPublication);
$splPublication = makeSplPublication(true, false);
try { $splPublication['x'][0] = new NestedPublicationNew(); }
catch (Exception $error) { echo $error->getMessage(), '|'; unset($error); }
echo $splPublication['x'][0]->label, '|'; unset($splPublication);
$splPublication = makeSplPublication(false, true);
$splPublication['x'][0] = new NestedPublicationNew(); echo $splPublication['x'][0], '|';
unset($splPublication);
$mixedPublication = makeMixedPublication(false, false);
$mixedPublication['x'][0] = new NestedPublicationNew(); echo $mixedPublication['x'][0]->label, '|';
unset($mixedPublication);
$mixedPublication = makeMixedPublication(true, false);
try { $mixedPublication['x'][0] = new NestedPublicationNew(); }
catch (Exception $error) { echo $error->getMessage(), '|'; unset($error); }
echo $mixedPublication['x'][0]->label, '|'; unset($mixedPublication);
$mixedPublication = makeMixedPublication(false, true);
$mixedPublication['x'][0] = new NestedPublicationNew(); echo $mixedPublication['x'][0], '|';
unset($mixedPublication);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "new|new|new|retired|new|new|retargeted|new|new|new|retired|new|new|retargeted|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Attached SPL writes detach snapshots and clones, including missing and runtime-boxed parents.
#[test]
fn test_spl_nested_writes_preserve_storage_snapshots_and_getter_overrides() {
    let out = crate::support::compile_and_run_with_heap_debug(r#"<?php
class NestedStorageSetter extends ArrayObject {
    public function offsetSet(mixed $key, mixed $value): void { echo 'unexpected-setter|'; }
}
class NestedStorageGetter extends ArrayObject {
    public function offsetGet(mixed $key): mixed { echo 'get|'; return parent::offsetGet($key); }
}
function nestedStorageBox(): mixed { return new ArrayIterator(['x' => [1]]); }
$a = new ArrayObject(['x' => [1]]); $snapshot = $a->getArrayCopy(); $clone = clone $a;
$a['x'][] = 2; $a['missing']['n'] = 3;
echo json_encode($a->getArrayCopy()), '|', json_encode($snapshot), '|', json_encode($clone->getArrayCopy()), '|';
$iterator = nestedStorageBox(); $iterator['x'][] = 4; $iterator['missing']['n'] = 5;
echo json_encode($iterator->getArrayCopy()), '|';
$setter = new NestedStorageSetter(['x' => [1]]); $setter['x'][] = 6;
echo json_encode($setter->getArrayCopy()), '|';
set_error_handler(function(int $severity, string $message, string $file, int $line): bool { return true; });
$getter = new NestedStorageGetter(['x' => [1]]); $getter['x'][] = 7;
restore_error_handler(); echo json_encode($getter->getArrayCopy());
unset($getter, $setter, $iterator, $clone, $snapshot, $a);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "{\"x\":[1,2],\"missing\":{\"n\":3}}|{\"x\":[1]}|{\"x\":[1]}|{\"x\":[1,4],\"missing\":{\"n\":5}}|{\"x\":[1,6]}|get|{\"x\":[1]}");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

use crate::support::*;

/// Verifies that storage classes are declared and implement contracts.
#[test]
fn test_storage_classes_are_declared_and_implement_contracts() {
    let out = compile_and_run(
        r#"<?php
var_dump(class_exists("EmptyIterator"));
var_dump(class_exists("ArrayIterator"));
var_dump(class_exists("ArrayObject"));
var_dump(new EmptyIterator() instanceof Iterator);
var_dump(new ArrayIterator([]) instanceof SeekableIterator);
var_dump(new ArrayIterator([]) instanceof ArrayAccess);
var_dump(new ArrayObject([]) instanceof IteratorAggregate);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}

/// Verifies that empty iterator foreach has no values.
#[test]
fn test_empty_iterator_foreach_has_no_values() {
    let out = compile_and_run(
        r#"<?php
echo "start:";
foreach (new EmptyIterator() as $k => $v) {
    echo "bad";
}
echo "end";
"#,
    );
    assert_eq!(out, "start:end");
}

/// Verifies that array iterator iterates associative keys and values.
#[test]
fn test_array_iterator_iterates_associative_keys_and_values() {
    let out = compile_and_run(
        r#"<?php
$it = new ArrayIterator(["a" => 10, "b" => 20]);
foreach ($it as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo ";";
}
"#,
    );
    assert_eq!(out, "a=10;b=20;");
}

/// Verifies that array iterator count seek and current.
#[test]
fn test_array_iterator_count_seek_and_current() {
    let out = compile_and_run(
        r#"<?php
$it = new ArrayIterator(["x" => "first", "y" => "second"]);
echo count($it);
echo ":";
$it->seek(1);
echo $it->key();
echo "=";
echo $it->current();
"#,
    );
    assert_eq!(out, "2:y=second");
}

/// Verifies that array iterator array access and mutation.
#[test]
fn test_array_iterator_array_access_and_mutation() {
    let out = compile_and_run(
        r#"<?php
$it = new ArrayIterator(["a" => 1]);
echo $it["a"];
echo ":";
var_dump($it->offsetExists("b"));
$it["b"] = 2;
$it[] = 3;
foreach ($it as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo ";";
}
"#,
    );
    assert_eq!(out, "1:bool(false)\na=1;b=2;2=3;");
}

/// Verifies that array object returns array iterator.
#[test]
fn test_array_object_returns_array_iterator() {
    let out = compile_and_run(
        r#"<?php
$obj = new ArrayObject(["left" => 4, "right" => 5]);
echo count($obj);
echo ":";
foreach ($obj as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo ";";
}
"#,
    );
    assert_eq!(out, "2:left=4;right=5;");
}

/// Verifies that array iterator get array copy preserves keys.
#[test]
fn test_array_iterator_get_array_copy_preserves_keys() {
    let out = compile_and_run(
        r#"<?php
$it = new ArrayIterator(["a" => 1]);
$it["b"] = 2;
$copy = $it->getArrayCopy();
foreach ($copy as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo ";";
}
"#,
    );
    assert_eq!(out, "a=1;b=2;");
}

/// Verifies the SPL gate opens for `unserialize`, whose class name lives in the DATA.
///
/// A valid serialized `SplFixedArray` came back as `__PHP_Incomplete_Class` because no static walk
/// can read the name inside the payload — and adding an unrelated `class_exists("SplFixedArray")`
/// to the program made the same payload work, which is the signature of a gate closing on a
/// reference it cannot see. The date/time gate already treated `unserialize` this way.
#[test]
fn test_spl_registers_for_unserialize() {
    let out = compile_and_run(
        r#"<?php
$o = unserialize('O:13:"SplFixedArray":2:{i:0;i:7;i:1;i:9;}');
echo get_class($o), ":", $o[0], $o[1];
"#,
    );
    assert_eq!(out, "SplFixedArray:79");
}
