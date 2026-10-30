//! Purpose:
//! Checks value snapshots from reference-returning class overrides.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - Parent contracts remain by value while their selected implementation returns a cell.
//! - Eight explicit arguments exercise stack overflow on both target architectures.
//! - Native eval bridges and late-static callable entries preserve the same copy semantics.

use crate::support::*;

/// Instance, late-static and callable uses copy the value and retire each transferred lease.
#[test]
fn test_class_reference_override_value_snapshots_bundle() {
    let declarations = r#"<?php
class ClassReferenceParent {
    public function items(int $a, int $b, int $c, int $d, int $e, int $f, int $g, int $h): mixed { return []; }
    public static function staticItems(): mixed { return []; }
    public static function forwarded(): mixed { return static::staticItems(); }
    public static function callback(): callable { return static::staticItems(...); }
}
class ClassReferenceChild extends ClassReferenceParent {
    public mixed $data = ['a' => 1, 'b' => 2];
    public static mixed $staticData = null;
    public function &items(int $a, int $b, int $c, int $d, int $e, int $f, int $g, int $h): mixed {
        if ($a + $b + $c + $d + $e + $f + $g + $h !== 36) { throw new RuntimeException('arguments'); }
        return $this->data;
    }
    public static function &staticItems(): mixed { $value = self::$staticData; return $value; }
}
class ClassReferenceTextParent { public function text(): string { return 'parent'; } }
class ClassReferenceTextChild extends ClassReferenceTextParent {
    private string $value = 'child';
    public function &text(): string { return $this->value; }
}
function classReferenceRead(ClassReferenceParent $value): mixed {
    return $value->items(1, 2, 3, 4, 5, 6, 7, 8);
}
function classReferenceText(ClassReferenceTextParent $value): string { return $value->text(); }
"#;
    let exercise = r#"
$object = new ClassReferenceChild();
for ($i = 0; $i < 3; $i++) {
    $copy = classReferenceRead($object);
    $copy['z'] = 9;
    echo json_encode($copy), '|', json_encode($object->data), "\n";
    unset($copy);
}
$direct = $object->items(1, 2, 3, 4, 5, 6, 7, 8);
echo count($direct), '|'; unset($direct);
ClassReferenceChild::$staticData = ['a' => 1, 'b' => 2];
$static = ClassReferenceChild::forwarded();
$static['z'] = 9;
echo json_encode($static), '|', json_encode(ClassReferenceChild::$staticData), "\n";
$callback = ClassReferenceChild::callback();
$called = $callback();
$called['z'] = 9;
echo json_encode($called), '|', json_encode(ClassReferenceChild::$staticData), "\n";
$text = classReferenceText(new ClassReferenceTextChild()); echo $text, "\n";
ClassReferenceChild::$staticData = null;
unset($object, $static, $callback, $called, $text);
"#;
    let aliases = r#"
$aliasOwner = new ClassReferenceChild();
$alias = &$aliasOwner->items(1, 2, 3, 4, 5, 6, 7, 8);
$alias['a'] = 7; echo $aliasOwner->data['a'], "\n";
$textOwner = new ClassReferenceTextChild();
$textAlias = &$textOwner->text(); $textAlias = 'changed';
echo classReferenceText($textOwner), "\n";
unset($alias, $aliasOwner, $textAlias, $textOwner);
"#;
    let by_value_source = format!("{declarations}{exercise}");
    let source = format!("{by_value_source}{aliases}");
    let quote = |body: &str| body.replace('\\', "\\\\").replace('\'', "\\'");
    let native_eval = format!("{declarations}\n$source = $argc > 0 ? '{}' : ''; eval($source);", quote(exercise))
        .replace("ClassReference", "NativeEvalClassReference").replace("classReference", "nativeEvalClassReference");
    let renamed = by_value_source.replace("ClassReference", "EvalClassReference").replace("classReference", "evalClassReference");
    let eval = format!("<?php $source = $argc > 0 ? '{}' : ''; eval($source);", quote(renamed.strip_prefix("<?php").unwrap()));
    let copy_line = "{\"a\":1,\"b\":2,\"z\":9}|{\"a\":1,\"b\":2}\n";
    let expected = format!("{}2|{}{}child\n", copy_line.repeat(3), copy_line, copy_line);
    let aot_expected = format!("{expected}7\nchanged\n");
    assert_bundle_with(&[
        BundleCase { name: "class_reference_override_aot", source: &source, expected: &aot_expected },
        BundleCase { name: "class_reference_override_native_eval", source: &native_eval, expected: &expected },
        BundleCase { name: "class_reference_override_eval", source: &eval, expected: &expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}
