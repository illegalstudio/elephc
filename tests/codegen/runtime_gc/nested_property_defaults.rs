//! Purpose:
//! Covers recursive native property defaults through AOT and opaque eval allocation.
//!
//! Called from:
//! - The native codegen regression harness on executable CI targets.
//!
//! Key details:
//! - Mixed defaults combine indexed arrays, hashes, empty children, and normalized duplicate keys.
//! - Independent instances and repeated destruction exercise nested owner transfer and COW.

use crate::support::*;

const NATIVE_DEFAULTS: &str = r#"
class NativeNestedDefaults {
    public mixed $list = [1, [2, 3], [], ["key" => [4, "value"]]];
    public mixed $hash = ["tree" => ["leaf" => [5, true, null]], "2" => ["old"], 2 => ["new"]];
}
"#;

/// Opaque eval allocates native defaults, including inherited physical slots and nested hash values.
#[test]
fn test_core_native_nested_defaults_read_from_eval() {
    let source = format!(r#"<?php
{NATIVE_DEFAULTS}
$source = 'class EvalNestedDefaults extends NativeNestedDefaults {{}}
foreach ([new NativeNestedDefaults(), new EvalNestedDefaults()] as $object) {{
    echo $object->list[0], ":", $object->list[1][1], ":", count($object->list[2]), ":";
    echo $object->list[3]["key"][1], ":", $object->hash["tree"]["leaf"][0], ":";
    echo $object->hash["tree"]["leaf"][1] ? "true:" : "bad:";
    echo $object->hash["tree"]["leaf"][2] === null ? "null:" : "bad:";
    echo $object->hash[2][0], "|";
}}' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(compile_and_run(&source), "1:3:0:value:5:true:null:new|1:3:0:value:5:true:null:new|");
}

/// Direct AOT defaults remain independent across both instance allocation and detached array writes.
#[test]
fn test_core_native_nested_defaults_preserve_cow() {
    let source = format!(r#"<?php
{NATIVE_DEFAULTS}
$first = new NativeNestedDefaults();
$second = new NativeNestedDefaults();
$copy = $first->list;
$copy[1][0] = 9;
$first->hash["tree"]["leaf"][0] = 8;
echo $copy[1][0], ":", $first->list[1][0], ":", $second->list[1][0], "|";
echo $first->hash["tree"]["leaf"][0], ":", $second->hash["tree"]["leaf"][0];
"#);
    assert_eq!(compile_and_run(&source), "9:2:2|8:5");
}

/// Mutating the original property or its detached copy leaves the other nested value unchanged.
#[test]
fn test_core_native_nested_defaults_cow_in_both_directions() {
    let source = format!(r#"<?php
{NATIVE_DEFAULTS}
$object = new NativeNestedDefaults();
$list = $object->list;
$hash = $object->hash;
$object->list[1][0] = 8;
$object->hash["tree"]["leaf"][0] = 9;
echo $list[1][0], ":", $hash["tree"]["leaf"][0], "|";
$list[1][1] = 10;
$hash["tree"]["leaf"][1] = false;
echo $object->list[1][0], ":", $object->list[1][1], ":", $list[1][1], "|";
echo $object->hash["tree"]["leaf"][0], ":";
echo $object->hash["tree"]["leaf"][1] ? "true:" : "false:";
echo $hash["tree"]["leaf"][1] ? "true" : "false";
"#);
    assert_eq!(compile_and_run(&source), "2:5|8:3:10|9:true:false");
}

/// A by-reference Mixed root publishes separated cells through its caller slot, preserving aliases.
#[test]
fn test_core_native_nested_defaults_reference_root_preserves_aliases() {
    let source = format!(r#"<?php
{NATIVE_DEFAULTS}
function mutateNestedRoot(mixed &$tree): void {{ $tree[1][0] = 9; }}
$object = new NativeNestedDefaults();
$copy = $object->list;
$alias =& $copy;
mutateNestedRoot($alias);
echo $copy[1][0], ":", $alias[1][0], ":", $object->list[1][0];
"#);
    assert_eq!(compile_and_run(&source), "9:9:2");
}

/// Static Mixed defaults retain nested indexed/hash storage and scalar runtime tags.
#[test]
fn test_core_native_static_nested_defaults_preserve_values() {
    let source = r#"<?php
class StaticNestedDefaults {
    public static mixed $list = [[1.5, 2.5], [], [true, false]];
    public static mixed $hash = ["tree" => ["leaf" => [3, "value"]]];
}
echo StaticNestedDefaults::$list[0][1], ":", count(StaticNestedDefaults::$list[1]), ":";
echo gettype(StaticNestedDefaults::$list[2][0]), ":", StaticNestedDefaults::$hash["tree"]["leaf"][1];
"#;
    assert_eq!(compile_and_run(source), "2.5:0:boolean:value");
}

/// Every nested literal allocation is released when a directly native eval object is destroyed.
#[test]
fn test_core_native_nested_default_owners_release() {
    super::core_builtins::assert_core_eval_collection_cleanup_with_native(
        NATIVE_DEFAULTS, "", "$object = new NativeNestedDefaults(); unset($object);",
    );
}

/// Inherited native defaults transfer ownership into eval subclasses without retaining hidden owners.
#[test]
fn test_core_inherited_nested_default_owners_release() {
    super::core_builtins::assert_core_eval_collection_cleanup_with_native(
        NATIVE_DEFAULTS, "class EvalNestedDefaults extends NativeNestedDefaults {}",
        "$object = new EvalNestedDefaults(); unset($object);",
    );
}

/// Repeated native nested mutations release the detached root, child zvals, and container owners.
#[test]
fn test_core_native_nested_write_owners_release() {
    let native = format!(r#"{NATIVE_DEFAULTS}
function mutate_native_nested_defaults(): void {{
    $object = new NativeNestedDefaults();
    $copy = $object->list;
    $copy[1][0] = 9;
    $object->hash["tree"]["leaf"][0] = 8;
    unset($copy, $object);
}}
"#);
    super::core_builtins::assert_core_eval_collection_cleanup_with_native(
        &native, "", "mutate_native_nested_defaults();",
    );
}
