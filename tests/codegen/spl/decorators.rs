//! Purpose:
//! End-to-end tests for SPL iterator decorator classes.
//! Covers forwarding, limited windows, no-rewind behavior, and infinite cycling.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - Decorators compose over `Iterator` implementations and are consumed through `foreach`.

use crate::support::*;

#[test]
fn test_decorator_classes_are_declared_and_implement_contracts() {
    let out = compile_and_run(
        r#"<?php
var_dump(class_exists("IteratorIterator"));
var_dump(class_exists("LimitIterator"));
var_dump(class_exists("NoRewindIterator"));
var_dump(class_exists("InfiniteIterator"));
var_dump(new IteratorIterator(new ArrayIterator([])) instanceof OuterIterator);
var_dump(new LimitIterator(new ArrayIterator([])) instanceof OuterIterator);
var_dump(new NoRewindIterator(new ArrayIterator([])) instanceof Iterator);
var_dump(new InfiniteIterator(new ArrayIterator([])) instanceof Iterator);
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
        )
    );
}

#[test]
fn test_iterator_iterator_forwards_keys_values_and_inner() {
    let out = compile_and_run(
        r#"<?php
$wrap = new IteratorIterator(new ArrayIterator(["a" => 10, "b" => 20]));
$inner = $wrap->getInnerIterator();
echo $inner->current();
echo ":";
foreach ($wrap as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo ";";
}
"#,
    );
    assert_eq!(out, "10:a=10;b=20;");
}

#[test]
fn test_no_rewind_iterator_preserves_inner_position() {
    let out = compile_and_run(
        r#"<?php
$inner = new ArrayIterator([10, 20, 30]);
$inner->next();
$wrap = new NoRewindIterator($inner);
foreach ($wrap as $v) {
    echo $v;
    echo ";";
}
"#,
    );
    assert_eq!(out, "20;30;");
}

#[test]
fn test_limit_iterator_slices_by_offset_and_limit() {
    let out = compile_and_run(
        r#"<?php
$it = new LimitIterator(new ArrayIterator(["a" => 10, "b" => 20, "c" => 30, "d" => 40]), 1, 2);
foreach ($it as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo ";";
}
echo ":";
$it->seek(2);
echo $it->getPosition();
echo "=";
echo $it->current();
"#,
    );
    assert_eq!(out, "b=20;c=30;:2=30");
}

#[test]
fn test_infinite_iterator_cycles_when_limited() {
    let out = compile_and_run(
        r#"<?php
$it = new LimitIterator(new InfiniteIterator(new ArrayIterator([1, 2])), 0, 5);
foreach ($it as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo ";";
}
"#,
    );
    assert_eq!(out, "0=1;1=2;0=1;1=2;0=1;");
}

#[test]
fn test_infinite_iterator_over_empty_iterator_has_no_values() {
    let out = compile_and_run(
        r#"<?php
echo "start:";
foreach (new InfiniteIterator(new EmptyIterator()) as $v) {
    echo "bad";
}
echo "end";
"#,
    );
    assert_eq!(out, "start:end");
}
