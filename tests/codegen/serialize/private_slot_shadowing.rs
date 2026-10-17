//! Purpose:
//! Regressions for serialization metadata of private parent slots shadowed by children.
//!
//! Called from:
//! - The `serialize` codegen integration test module.
//!
//! Key details:
//! - Compare complete objects and wire bytes, including NUL-mangled private keys.
//! - Hydration checks, storage tags and diagnostic owners must describe the same physical slot.
//! - Compare retained heap accounting with unshadowed controls; typed hydration already retains boxes.

use crate::support::*;

/// Reads final heap counters without confusing a balanced path with a clean-success guarantee.
fn live_footprint(stderr: &str) -> (u64, u64) {
    let counter = |key: &str| -> u64 {
        stderr.split_whitespace().find_map(|word| word.strip_prefix(key)?.parse().ok())
            .unwrap_or_else(|| panic!("missing {key} in {stderr}"))
    };
    (counter("live_blocks="), counter("live_bytes="))
}

/// Checks complete PHP output and prevents shadowing from adding to the known hydration residue.
fn assert_shadowed_slots(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);

    // Rename only the hidden parent slot, keeping the same value types, wire shape and allocations.
    // Successful typed hydration already retains parsed cells even without name shadowing.
    let control_source = source.replace("private int $x", "private int $p")
        .replace("private string $x", "private string $p")
        .replace(r"\0P\0x", r"\0P\0p");
    let control_expected = expected.replace("\"x\":\"P\":private", "\"p\":\"P\":private")
        .replace("\0P\0x", "\0P\0p").replace("P::$x", "P::$p");
    let control = compile_and_run_with_heap_debug(&control_source);
    assert!(control.success, "{}", control.stderr);
    assert_eq!(control.stdout, control_expected, "{}", control.stderr);
    let actual = live_footprint(&out.stderr);
    let baseline = live_footprint(&control.stderr);
    assert!(actual.0 <= baseline.0 && actual.1 <= baseline.1,
        "shadowed slots retain more than distinct-name slots: {actual:?} > {baseline:?}\n{}\n{}",
        out.stderr, control.stderr);
}

/// A valid child string must not inherit the private parent integer mask.
#[test]
fn test_unserialize_shadowed_private_parent_keeps_both_slots() {
    assert_shadowed_slots(r#"<?php
class P { private int $x = 1; }
class C extends P { public string $x = "default"; }
$value = unserialize('O:1:"C":1:{s:1:"x";s:2:"ok";}');
var_dump($value);
echo serialize($value), "\n";
unset($value);
"#, "object(C)#1 (2) {\n  [\"x\":\"P\":private]=>\n  int(1)\n  [\"x\"]=>\n  string(2) \"ok\"\n}\nO:1:\"C\":2:{s:4:\"\0P\0x\";i:1;s:1:\"x\";s:2:\"ok\";}\n");
}

/// Mangled parent and public child keys write independent physical slots.
#[test]
fn test_unserialize_shadowed_private_parent_uses_distinct_wire_destinations() {
    assert_shadowed_slots(r#"<?php
class P { private int $x = 1; }
class C extends P { public string $x = "default"; }
$wire = "O:1:\"C\":2:{s:4:\"\0P\0x\";i:12;s:1:\"x\";s:2:\"ok\";}";
$value = unserialize($wire);
var_dump($value);
var_dump(serialize($value) === $wire);
unset($value);
"#, "object(C)#1 (2) {\n  [\"x\":\"P\":private]=>\n  int(12)\n  [\"x\"]=>\n  string(2) \"ok\"\n}\nbool(true)\n");
}

/// Both rejected values name the declaration belonging to the selected physical slot.
#[test]
fn test_unserialize_shadowed_private_parent_reports_each_declaring_type() {
    assert_shadowed_slots(r#"<?php
class P { private int $x = 1; }
class C extends P { public string $x = "default"; }
foreach (['O:1:"C":1:{s:1:"x";i:2;}', "O:1:\"C\":1:{s:4:\"\0P\0x\";s:2:\"no\";}"] as $wire) {
    try { unserialize($wire); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
"#, "Cannot assign int to property C::$x of type string\nCannot assign string to property P::$x of type int\n");
}

/// An untyped child must not inherit the declaredness of its private parent slot.
#[test]
fn test_unserialize_shadowed_untyped_child_accepts_a_different_value() {
    assert_shadowed_slots(r#"<?php
class P { private int $x = 1; }
class C extends P { public $x = null; }
$value = unserialize('O:1:"C":1:{s:1:"x";s:2:"ok";}');
var_dump($value);
unset($value);
"#, "object(C)#1 (2) {\n  [\"x\":\"P\":private]=>\n  int(1)\n  [\"x\"]=>\n  string(2) \"ok\"\n}\n");
}

/// Debug projections affect rendering while serialization retains the physical properties.
#[test]
fn test_serialize_shadowed_private_parent_ignores_debug_info_projection() {
    assert_shadowed_slots(r#"<?php
class P { private int $x = 1; }
class C extends P {
    public string $x = "default";
    public function __debugInfo(): array { return ["shown" => $this->x]; }
}
$value = unserialize('O:1:"C":1:{s:1:"x";s:2:"ok";}');
var_dump($value);
echo serialize($value), "\n";
unset($value);
"#, "object(C)#1 (1) {\n  [\"shown\"]=>\n  string(2) \"ok\"\n}\nO:1:\"C\":2:{s:4:\"\0P\0x\";i:1;s:1:\"x\";s:2:\"ok\";}\n");
}

/// A hidden heap-owning parent and a scalar child retain their distinct representations.
#[test]
fn test_unserialize_shadowed_private_string_parent_keeps_its_payload() {
    assert_shadowed_slots(r#"<?php
class P { private string $x = "parent"; }
class C extends P { public int $x = 2; }
$wire = "O:1:\"C\":2:{s:4:\"\0P\0x\";s:6:\"hidden\";s:1:\"x\";i:7;}";
$value = unserialize($wire);
var_dump($value);
echo serialize($value), "\n";
unset($value);
"#, "object(C)#1 (2) {\n  [\"x\":\"P\":private]=>\n  string(6) \"hidden\"\n  [\"x\"]=>\n  int(7)\n}\nO:1:\"C\":2:{s:4:\"\0P\0x\";s:6:\"hidden\";s:1:\"x\";i:7;}\n");
}
