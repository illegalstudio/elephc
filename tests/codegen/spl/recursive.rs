//! Purpose:
//! End-to-end tests for SPL recursive iterator classes.
//! Covers recursive array storage, recursive filters, parent filtering, and traversal modes.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - RecursiveArrayIterator children are built from runtime mixed array values.
//! - RecursiveIteratorIterator keeps live stack cursors for depth and sub-iterator access.

use crate::support::*;

/// Verifies that recursive classes are declared and implement contracts.
#[test]
fn test_recursive_classes_are_declared_and_implement_contracts() {
    let out = compile_and_run(
        r#"<?php
function has_name(array $names, string $target): bool {
    foreach ($names as $name) {
        if ($name === $target) {
            return true;
        }
    }
    return false;
}

var_dump(class_exists("RecursiveArrayIterator"));
var_dump(class_exists("RecursiveFilterIterator"));
var_dump(class_exists("RecursiveCallbackFilterIterator"));
var_dump(class_exists("RecursiveIteratorIterator"));
var_dump(class_exists("ParentIterator"));
$names = spl_classes();
var_dump(has_name($names, "RecursiveArrayIterator"));
var_dump(has_name($names, "RecursiveFilterIterator"));
var_dump(has_name($names, "RecursiveCallbackFilterIterator"));
var_dump(has_name($names, "RecursiveIteratorIterator"));
var_dump(has_name($names, "ParentIterator"));
var_dump(new RecursiveArrayIterator([]) instanceof ArrayIterator);
var_dump(new RecursiveArrayIterator([]) instanceof RecursiveIterator);
$filter = new RecursiveCallbackFilterIterator(new RecursiveArrayIterator([]), function($current, $key, $iterator) {
    return true;
});
var_dump($filter instanceof CallbackFilterIterator);
var_dump($filter instanceof RecursiveIterator);
var_dump(new RecursiveIteratorIterator(new RecursiveArrayIterator([])) instanceof OuterIterator);
var_dump(new ParentIterator(new RecursiveArrayIterator([])) instanceof RecursiveFilterIterator);
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
            "bool(true)\n",
            "bool(true)\n",
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

/// Verifies that recursive array iterator children from mixed values.
#[test]
fn test_recursive_array_iterator_children_from_mixed_values() {
    let out = compile_and_run(
        r#"<?php
$it = new RecursiveArrayIterator(["a" => ["x" => 1], "b" => 2]);
$it->rewind();
echo $it->key();
echo $it->hasChildren() ? ":children:" : ":leaf:";
$child = $it->getChildren();
echo $child instanceof RecursiveArrayIterator ? "child:" : "bad:";
foreach ($child as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
}
$it->next();
echo $it->key();
echo $it->hasChildren() ? ":bad" : ":leaf";
"#,
    );
    assert_eq!(out, "a:children:child:x=1b:leaf");
}

/// Verifies that recursive iterator iterator traversal modes.
#[test]
fn test_recursive_iterator_iterator_traversal_modes() {
    let out = compile_and_run(
        r#"<?php
function dump_value(mixed $value): void {
    echo gettype($value) === "array" ? "array" : $value;
}

$data = ["a" => ["x" => 1, "y" => ["z" => 2]], "b" => 3];

echo "leaves:";
$leaves = new RecursiveIteratorIterator(new RecursiveArrayIterator($data));
foreach ($leaves as $key => $value) {
    echo $leaves->getDepth();
    echo ":";
    echo $key;
    echo "=";
    dump_value($value);
    echo ";";
}

echo "self:";
$self = new RecursiveIteratorIterator(
    new RecursiveArrayIterator(["a" => ["x" => 1], "b" => 2]),
    RecursiveIteratorIterator::SELF_FIRST
);
foreach ($self as $key => $value) {
    echo $self->getDepth();
    echo ":";
    echo $key;
    echo "=";
    dump_value($value);
    echo ";";
}

echo "child:";
$child = new RecursiveIteratorIterator(
    new RecursiveArrayIterator(["a" => ["x" => 1], "b" => 2]),
    RecursiveIteratorIterator::CHILD_FIRST
);
foreach ($child as $key => $value) {
    echo $child->getDepth();
    echo ":";
    echo $key;
    echo "=";
    dump_value($value);
    echo ";";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "leaves:1:x=1;2:z=2;0:b=3;",
            "self:0:a=array;1:x=1;0:b=2;",
            "child:1:x=1;0:a=array;0:b=2;",
        )
    );
}

/// Verifies that recursive iterator iterator sees source mutation after rewind.
#[test]
fn test_recursive_iterator_iterator_sees_source_mutation_after_rewind() {
    let out = compile_and_run(
        r#"<?php
$root = new RecursiveArrayIterator(["a" => ["x" => 1]]);
$it = new RecursiveIteratorIterator($root, RecursiveIteratorIterator::SELF_FIRST);
$it->rewind();
$root["b"] = 2;
while ($it->valid()) {
    echo $it->getDepth();
    echo ":";
    echo $it->key();
    echo ";";
    $it->next();
}
"#,
    );
    assert_eq!(out, "0:a;1:x;0:b;");
}

/// Verifies that recursive iterator iterator sub iterators track live cursors.
#[test]
fn test_recursive_iterator_iterator_sub_iterators_track_live_cursors() {
    let out = compile_and_run(
        r#"<?php
$it = new RecursiveIteratorIterator(
    new RecursiveArrayIterator(["a" => ["x" => 1], "b" => 2]),
    RecursiveIteratorIterator::SELF_FIRST
);
foreach ($it as $key => $value) {
    echo $it->getDepth();
    echo ":";
    echo $key;
    echo ":";
    echo $it->getInnerIterator()->key();
    echo ":";
    echo $it->getSubIterator()->key();
    if ($it->getDepth() > 0) {
        echo ":";
        echo $it->getSubIterator(0)->key();
        echo ":";
        echo $it->getSubIterator(1)->key();
    }
    echo ";";
}
"#,
    );
    assert_eq!(out, "0:a:a:a;1:x:x:x:a:x;0:b:b:b;");
}

/// Verifies that recursive callback filter iterator preserves callback for children.
#[test]
fn test_recursive_callback_filter_iterator_preserves_callback_for_children() {
    let out = compile_and_run(
        r#"<?php
$min = 1;
$filter = new RecursiveCallbackFilterIterator(
    new RecursiveArrayIterator(["a" => ["x" => 1, "y" => 2], "b" => 3]),
    function($current, $key, $iterator) use ($min): bool {
        return gettype($current) === "array" || $current > $min;
    }
);
$it = new RecursiveIteratorIterator($filter);
foreach ($it as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "y=2;b=3;");
}

/// Verifies that parent iterator filters parents recursively.
#[test]
fn test_parent_iterator_filters_parents_recursively() {
    let out = compile_and_run(
        r#"<?php
$parents = new RecursiveIteratorIterator(
    new ParentIterator(new RecursiveArrayIterator([
        "a" => ["x" => 1],
        "b" => 2,
        "c" => ["y" => ["z" => 3]],
    ])),
    RecursiveIteratorIterator::SELF_FIRST
);
foreach ($parents as $key => $value) {
    echo $parents->getDepth();
    echo ":";
    echo $key;
    echo "=";
    echo gettype($value);
    echo ";";
}
"#,
    );
    assert_eq!(out, "0:a=array;0:c=array;1:y=array;");
}
