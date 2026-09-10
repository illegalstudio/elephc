//! Purpose:
//! End-to-end regressions for PHP-visible object `print_r()` debug output.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::object_debug_output` through Rust's test harness.
//!
//! Key details:
//! - Classes without `__debugInfo()` enumerate initialized declared properties.
//! - Visibility suffixes, private shadow slots, and nested array indentation must match PHP 8.5.

use crate::support::{compile_and_run, compile_and_run_capture};

/// Verifies the dynamic debug-info path preserves ordinary declared-property output.
#[test]
fn print_r_object_without_debug_info_matches_php_visibility_and_initialization() {
    let out = compile_and_run(
        r#"<?php
class PrintRParent {
    private int $a = 1;
    protected string $b = 'B';
    public int $uninitialized;
}
class PrintRChild extends PrintRParent {
    private int $c = 3;
    public array $d = [4];
}
print_r(new PrintRChild());
"#,
    );
    assert_eq!(
        out,
        concat!(
            "PrintRChild Object\n",
            "(\n",
            "    [a:PrintRParent:private] => 1\n",
            "    [b:protected] => B\n",
            "    [c:PrintRChild:private] => 3\n",
            "    [d] => Array\n",
            "        (\n",
            "            [0] => 4\n",
            "        )\n",
            "\n",
            ")\n",
        )
    );
}

/// Verifies parent and child same-name private properties retain distinct physical slots.
#[test]
fn object_debug_output_preserves_initialized_private_shadow_slots() {
    let out = compile_and_run(
        r#"<?php
class PrivateShadowParent {
    private $x = 'parent';
}
class PrivateShadowChild extends PrivateShadowParent {
    private $x = 'child';
}
$object = new PrivateShadowChild();
print_r($object);
var_dump($object);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "PrivateShadowChild Object\n",
            "(\n",
            "    [x:PrivateShadowParent:private] => parent\n",
            "    [x:PrivateShadowChild:private] => child\n",
            ")\n",
            "object(PrivateShadowChild)#1 (2) {\n",
            "  [\"x\":\"PrivateShadowParent\":private]=>\n",
            "  string(6) \"parent\"\n",
            "  [\"x\":\"PrivateShadowChild\":private]=>\n",
            "  string(5) \"child\"\n",
            "}\n",
        )
    );
}

/// Verifies an uninitialized child shadow does not hide the initialized private parent slot.
#[test]
fn object_debug_output_preserves_uninitialized_private_shadow_slot() {
    let out = compile_and_run(
        r#"<?php
class TypedShadowParent {
    private $x = 'parent';
}
class TypedShadowChild extends TypedShadowParent {
    private string $x;
}
$object = new TypedShadowChild();
print_r($object);
var_dump($object);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "TypedShadowChild Object\n",
            "(\n",
            "    [x:TypedShadowParent:private] => parent\n",
            ")\n",
            "object(TypedShadowChild)#1 (1) {\n",
            "  [\"x\":\"TypedShadowParent\":private]=>\n",
            "  string(6) \"parent\"\n",
            "  [\"x\":\"TypedShadowChild\":private]=>\n",
            "  uninitialized(string)\n",
            "}\n",
        )
    );
}

/// Verifies `print_r()` invokes re-entrant `__debugInfo()` before guarding its projection.
#[test]
fn print_r_debug_info_reentry_matches_php_guard_order() {
    let out = compile_and_run(
        r#"<?php
class ReentrantDebugInfo {
    public int $n = 0;
    public function __debugInfo() {
        $this->n = $this->n + 1;
        echo 'd', $this->n, "\n";
        if ($this->n < 3) {
            print_r($this);
        }
        return ['n' => $this->n];
    }
}
print_r(new ReentrantDebugInfo());
"#,
    );
    assert_eq!(
        out,
        concat!(
            "d1\n",
            "d2\n",
            "d3\n",
            "ReentrantDebugInfo Object\n(\n    [n] => 3\n)\n",
            "ReentrantDebugInfo Object\n(\n    [n] => 3\n)\n",
            "ReentrantDebugInfo Object\n(\n    [n] => 3\n)\n",
        )
    );
}

/// Verifies an untyped scalar `__debugInfo()` return is process-fatal, even inside a catch.
#[test]
fn invalid_debug_info_return_is_process_fatal() {
    let out = compile_and_run_capture(
        r#"<?php
class InvalidDebugInfoReturn {
    public function __debugInfo() {
        return 123;
    }
}
try {
    var_dump(new InvalidDebugInfoReturn());
} catch (Throwable $error) {
    echo 'caught';
}
echo 'continued';
"#,
    );
    assert!(!out.success, "invalid __debugInfo() return unexpectedly continued");
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        "Fatal error: __debuginfo() must return an array\n"
    );
}

/// Verifies declared and dynamic properties render in PHP insertion order with nested values.
#[test]
fn object_debug_output_includes_dynamic_property_hash_tails() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class DynamicDebugBag {
    public $a = 1;
}
$bag = new DynamicDebugBag();
$bag->z = 2;
$bag->q = [3];
print_r($bag);
var_dump($bag);

$plain = new stdClass();
$plain->first = 'A';
$plain->nested = ['k' => 4];
print_r($plain);
var_dump($plain);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "DynamicDebugBag Object\n(\n",
            "    [a] => 1\n",
            "    [z] => 2\n",
            "    [q] => Array\n        (\n            [0] => 3\n        )\n\n",
            ")\n",
            "object(DynamicDebugBag)#1 (3) {\n",
            "  [\"a\"]=>\n  int(1)\n",
            "  [\"z\"]=>\n  int(2)\n",
            "  [\"q\"]=>\n  array(1) {\n    [0]=>\n    int(3)\n  }\n",
            "}\n",
            "stdClass Object\n(\n",
            "    [first] => A\n",
            "    [nested] => Array\n        (\n            [k] => 4\n        )\n\n",
            ")\n",
            "object(stdClass)#2 (2) {\n",
            "  [\"first\"]=>\n  string(1) \"A\"\n",
            "  [\"nested\"]=>\n  array(1) {\n    [\"k\"]=>\n    int(4)\n  }\n",
            "}\n",
        )
    );
}

/// Verifies only top-level debug-projection keys are demangled as object properties.
#[test]
fn object_debug_projection_demangles_private_and_protected_keys_only_at_top_level() {
    let out = compile_and_run(
        r#"<?php
class DebugProjectionKeys {
    public function __debugInfo() {
        return [
            "\0DebugProjectionKeys\0x" => 1,
            "\0*\0y" => 2,
            "z" => ["\0DebugProjectionKeys\0inner" => 3],
        ];
    }
}
$object = new DebugProjectionKeys();
print_r($object);
var_dump($object);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "DebugProjectionKeys Object\n(\n",
            "    [x:DebugProjectionKeys:private] => 1\n",
            "    [y:protected] => 2\n",
            "    [z] => Array\n        (\n",
            "            [\0DebugProjectionKeys\0inner] => 3\n",
            "        )\n\n)\n",
            "object(DebugProjectionKeys)#1 (3) {\n",
            "  [\"x\":\"DebugProjectionKeys\":private]=>\n  int(1)\n",
            "  [\"y\":protected]=>\n  int(2)\n",
            "  [\"z\"]=>\n  array(1) {\n",
            "    [\"\0DebugProjectionKeys\0inner\"]=>\n    int(3)\n",
            "  }\n}\n",
        )
    );
}

/// Verifies runtime metadata forces builtin SPL debug bodies without direct magic calls.
#[test]
fn builtin_spl_debug_info_is_lowered_for_indirect_rendering() {
    let out = compile_and_run(
        r#"<?php
$list = new SplDoublyLinkedList();
$list->push('A');
print_r($list);
var_dump($list);

$multiple = new MultipleIterator();
print_r($multiple);
var_dump($multiple);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "SplDoublyLinkedList Object\n(\n",
            "    [flags:SplDoublyLinkedList:private] => 0\n",
            "    [dllist:SplDoublyLinkedList:private] => Array\n",
            "        (\n            [0] => A\n        )\n\n",
            ")\n",
            "object(SplDoublyLinkedList)#1 (2) {\n",
            "  [\"flags\":\"SplDoublyLinkedList\":private]=>\n  int(0)\n",
            "  [\"dllist\":\"SplDoublyLinkedList\":private]=>\n",
            "  array(1) {\n    [0]=>\n    string(1) \"A\"\n  }\n",
            "}\n",
            "MultipleIterator Object\n(\n",
            "    [storage:SplObjectStorage:private] => Array\n        (\n        )\n\n",
            ")\n",
            "object(MultipleIterator)#2 (1) {\n",
            "  [\"storage\":\"SplObjectStorage\":private]=>\n",
            "  array(0) {\n  }\n",
            "}\n",
        )
    );
}

/// Verifies non-empty SPL storage projections preserve object identity and info values.
#[test]
fn builtin_spl_storage_debug_info_matches_php_object_info_pairs() {
    let out = compile_and_run(
        r#"<?php
$object = new stdClass();
$storage = new SplObjectStorage();
$storage->offsetSet($object, 'I');
$debug = $storage->__debugInfo();
$rows = $debug["\0SplObjectStorage\0storage"];
echo count($debug), '|', count($rows), '|';
echo $rows[0]['obj'] === $object ? 'same' : 'different';
echo '|', $rows[0]['inf'], "\n";

$iterator = new ArrayIterator(['A']);
$multiple = new MultipleIterator();
$multiple->attachIterator($iterator, 'K');
$debug = $multiple->__debugInfo();
$rows = $debug["\0SplObjectStorage\0storage"];
echo count($debug), '|', count($rows), '|';
echo $rows[0]['obj'] === $iterator ? 'same' : 'different';
echo '|', $rows[0]['inf'], "\n";
"#,
    );
    assert_eq!(out, "1|1|same|I\n1|1|same|K\n");
}
