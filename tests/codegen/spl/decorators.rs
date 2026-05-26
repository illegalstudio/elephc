//! Purpose:
//! End-to-end tests for SPL iterator decorator classes.
//! Covers forwarding, IteratorAggregate normalization, windows, no-rewind behavior, cycling, filters, cache, and multi-source decorators.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - Decorators compose over `Iterator` implementations and are consumed through `foreach`.

use crate::support::*;

/// Verifies that decorator classes are declared and implement contracts.
#[test]
fn test_decorator_classes_are_declared_and_implement_contracts() {
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

var_dump(class_exists("IteratorIterator"));
var_dump(class_exists("LimitIterator"));
var_dump(class_exists("NoRewindIterator"));
var_dump(class_exists("InfiniteIterator"));
var_dump(class_exists("FilterIterator"));
var_dump(class_exists("CallbackFilterIterator"));
var_dump(class_exists("CachingIterator"));
var_dump(class_exists("AppendIterator"));
var_dump(class_exists("MultipleIterator"));
var_dump(class_exists("__ElephcAppendIteratorArrayIterator", false));
$names = spl_classes();
var_dump(has_name($names, "FilterIterator"));
var_dump(has_name($names, "CallbackFilterIterator"));
var_dump(has_name($names, "CachingIterator"));
var_dump(has_name($names, "AppendIterator"));
var_dump(has_name($names, "MultipleIterator"));
$declared = get_declared_classes();
var_dump(has_name($declared, "__ElephcAppendIteratorArrayIterator"));
var_dump(new IteratorIterator(new ArrayIterator([])) instanceof OuterIterator);
var_dump(new LimitIterator(new ArrayIterator([])) instanceof OuterIterator);
var_dump(new NoRewindIterator(new ArrayIterator([])) instanceof Iterator);
var_dump(new InfiniteIterator(new ArrayIterator([])) instanceof Iterator);
$filter = new CallbackFilterIterator(new ArrayIterator([]), function($current, $key, $iterator) {
    return true;
});
var_dump($filter instanceof FilterIterator);
var_dump($filter instanceof OuterIterator);
$cache = new CachingIterator(new ArrayIterator([]));
var_dump($cache instanceof ArrayAccess);
var_dump($cache instanceof Countable);
var_dump($cache instanceof Stringable);
var_dump(new AppendIterator() instanceof OuterIterator);
var_dump(new MultipleIterator() instanceof Iterator);
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
            "bool(false)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(false)\n",
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

/// Verifies that iterator iterator forwards keys values and inner.
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

/// Verifies that iterator iterator normalizes iterator aggregate inputs.
#[test]
fn test_iterator_iterator_normalizes_iterator_aggregate_inputs() {
    let out = compile_and_run(
        r#"<?php
function dump_wrapped(Traversable $items): void {
    $wrap = new IteratorIterator($items);
    foreach ($wrap as $k => $v) {
        echo $k;
        echo "=";
        echo $v;
        echo ";";
    }
    echo "|";
}

dump_wrapped(new ArrayObject(["left" => "L", "right" => "R"]));
dump_wrapped(new ArrayIterator(["direct" => "I"]));
"#,
    );
    assert_eq!(out, "left=L;right=R;|direct=I;|");
}

/// Verifies that iterator iterator second arg downcasts iterator aggregate.
#[test]
fn test_iterator_iterator_second_arg_downcasts_iterator_aggregate() {
    let out = compile_and_run(
        r#"<?php
$class = "ArrayObject";
$wrap = new IteratorIterator(new ArrayObject(["left" => "L"]), $class);
foreach ($wrap as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo ";";
}

class BaseAgg implements IteratorAggregate {
    public function getIterator(): Traversable {
        return new ArrayIterator(["base" => "B"]);
    }
}
class ChildAgg extends BaseAgg {}

$parent = new IteratorIterator(new ChildAgg(), "BaseAgg");
foreach ($parent as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo ";";
}
"#,
    );
    assert_eq!(out, "left=L;base=B;");
}

/// Verifies that iterator iterator second arg is evaluated and ignored for iterators.
#[test]
fn test_iterator_iterator_second_arg_is_evaluated_and_ignored_for_iterators() {
    let out = compile_and_run(
        r#"<?php
function invalid_downcast_name(): string {
    echo "class;";
    return "NoSuchClass";
}

$wrap = new IteratorIterator(new ArrayIterator([9]), invalid_downcast_name());
echo $wrap->current();
"#,
    );
    assert_eq!(out, "class;9");
}

/// Verifies that iterator iterator second arg preserves positional source order.
#[test]
fn test_iterator_iterator_second_arg_preserves_positional_source_order() {
    let out = compile_and_run(
        r#"<?php
function ordered_source(): Traversable {
    echo "source;";
    return new ArrayObject(["named" => "N"]);
}

function ordered_downcast(): string {
    echo "class;";
    return "ArrayObject";
}

$wrap = new IteratorIterator(ordered_source(), ordered_downcast());
foreach ($wrap as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo ";";
}
"#,
    );
    assert_eq!(out, "source;class;named=N;");
}

/// Verifies that iterator iterator second arg accepts keyword named argument.
#[test]
fn test_iterator_iterator_second_arg_accepts_keyword_named_argument() {
    let out = compile_and_run(
        r#"<?php
$wrap = new IteratorIterator(new ArrayObject(["named" => "N"]), class: "ArrayObject");
foreach ($wrap as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo ";";
}
"#,
    );
    assert_eq!(out, "named=N;");
}

/// Verifies that iterator iterator second arg rejects invalid aggregate downcasts.
#[test]
fn test_iterator_iterator_second_arg_rejects_invalid_aggregate_downcasts() {
    let out = compile_and_run(
        r#"<?php
try {
    $tmp = new IteratorIterator(new ArrayObject([1]), "Iterator");
    echo "bad-interface";
} catch (LogicException $e) {
    echo "interface:";
    echo $e->getMessage();
    echo "|";
}

class PlainBase {}
class AggChild extends PlainBase implements IteratorAggregate {
    public function getIterator(): Traversable {
        return new ArrayIterator([1]);
    }
}

try {
    $tmp = new IteratorIterator(new AggChild(), "PlainBase");
    echo "bad-base";
} catch (LogicException $e) {
    echo "base";
}
"#,
    );
    assert_eq!(
        out,
        "interface:Class to downcast to not found or not base class or does not implement Traversable|base"
    );
}

/// Verifies that no rewind iterator preserves inner position.
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

/// Verifies that limit iterator slices by offset and limit.
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

/// Verifies that infinite iterator cycles when limited.
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

/// Verifies that infinite iterator over empty iterator has no values.
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

/// Verifies that filter iterator subclass skips rejected items.
#[test]
fn test_filter_iterator_subclass_skips_rejected_items() {
    let out = compile_and_run(
        r#"<?php
class SkipKeyFilter extends FilterIterator {
    public function accept(): bool {
        return $this->key() !== "skip";
    }
}

$it = new SkipKeyFilter(new ArrayIterator(["keep" => 1, "skip" => 2, "tail" => 3]));
foreach ($it as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "keep=1;tail=3;");
}

/// Verifies that callback filter iterator uses callback current key and inner.
#[test]
fn test_callback_filter_iterator_uses_callback_current_key_and_inner() {
    let out = compile_and_run(
        r#"<?php
function keep_after_first(int $current, string $key, Iterator $iterator): bool {
    echo "cb:";
    echo $key;
    echo "=";
    echo $current;
    echo ":";
    echo $iterator instanceof ArrayIterator ? "it" : "bad";
    echo ";";
    return $current > 1;
}

$it = new CallbackFilterIterator(
    new ArrayIterator(["a" => 1, "b" => 2, "c" => 3]),
    keep_after_first(...)
);
foreach ($it as $key => $value) {
    echo "out:";
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}
$cb = keep_after_first(...);
$it2 = new CallbackFilterIterator(
    new ArrayIterator(["x" => 1, "y" => 2]),
    $cb
);
foreach ($it2 as $key => $value) {
    echo "var:";
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(
        out,
        "cb:a=1:it;cb:b=2:it;out:b=2;cb:c=3:it;out:c=3;cb:x=1:it;cb:y=2:it;var:y=2;"
    );
}

/// Verifies that callback filter iterator preserves captured closure env.
#[test]
fn test_callback_filter_iterator_preserves_captured_closure_env() {
    let out = compile_and_run(
        r#"<?php
$limit = 1;
$suffix = "!";
$it = new CallbackFilterIterator(
    new ArrayIterator(["a" => 1, "b" => 2, "c" => 3]),
    function(int $value, string $key, Iterator $inner) use ($limit, $suffix): bool {
        echo $key;
        echo $suffix;
        echo $inner instanceof ArrayIterator ? "it" : "bad";
        echo ";";
        return $value > $limit;
    }
);
foreach ($it as $key => $value) {
    echo "out:";
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "a!it;b!it;out:b=2;c!it;out:c=3;");
}

/// Verifies that callback filter iterator preserves variable closure env.
#[test]
fn test_callback_filter_iterator_preserves_variable_closure_env() {
    let out = compile_and_run(
        r#"<?php
$limit = 2;
$cb = function(int $value, int $key, Iterator $inner) use ($limit): bool {
    return $value >= $limit;
};
$it = new CallbackFilterIterator(new ArrayIterator([1, 2, 3]), $cb);
foreach ($it as $key => $value) {
    echo $key;
    echo ":";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "1:2;2:3;");
}

/// Verifies that caching iterator tracks has next and string value.
#[test]
fn test_caching_iterator_tracks_has_next_and_string_value() {
    let out = compile_and_run(
        r#"<?php
$it = new CachingIterator(new ArrayIterator(["a" => "A", "b" => "B"]));
foreach ($it as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo "/";
    echo $it->hasNext() ? "Y" : "N";
    echo "/";
    echo (string) $it;
    echo ";";
}
try {
    $it->getCache();
} catch (BadMethodCallException $e) {
    echo "|";
    echo $e->getMessage();
}
try {
    $it->setFlags(CachingIterator::FULL_CACHE);
} catch (InvalidArgumentException $e) {
    echo "|";
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "a=A/Y/A;b=B/N/B;|CachingIterator does not use a full cache (see CachingIterator::__construct)|Unsetting flag CALL_TO_STRING is not possible"
    );
}

/// Verifies that caching iterator full cache array access and flags.
#[test]
fn test_caching_iterator_full_cache_array_access_and_flags() {
    let out = compile_and_run(
        r#"<?php
$it = new CachingIterator(
    new ArrayIterator(["a" => "A", "b" => "B"]),
    CachingIterator::FULL_CACHE | CachingIterator::TOSTRING_USE_KEY
);
echo $it->getFlags();
echo ":";
foreach ($it as $key => $value) {
    echo (string) $it;
    echo "=";
    echo $value;
    echo "/";
    echo $it->hasNext() ? "Y" : "N";
    echo ";";
}
echo ":";
echo $it->count();
echo ":";
var_dump($it->offsetExists("a"));
echo $it["a"];
$it["z"] = "Z";
unset($it["a"]);
$cache = $it->getCache();
echo ":";
echo count($cache);
echo ":";
echo $cache["b"];
echo "/";
echo $cache["z"];

try {
    $bad = new CachingIterator(new ArrayIterator([]), CachingIterator::CALL_TOSTRING | CachingIterator::TOSTRING_USE_KEY);
    echo "bad";
} catch (ValueError $e) {
    echo "|flags";
}

try {
    $noString = new CachingIterator(new ArrayIterator([1]), CachingIterator::FULL_CACHE);
    $noString->rewind();
    echo (string) $noString;
} catch (BadMethodCallException $e) {
    echo "|string";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "258:a=A/Y;b=B/N;:2:",
            "bool(true)\n",
            "A:2:B/Z|flags|string",
        )
    );
}

/// Verifies that append iterator skips empty iterators and exposes storage.
#[test]
fn test_append_iterator_skips_empty_iterators_and_exposes_storage() {
    let out = compile_and_run(
        r#"<?php
$append = new AppendIterator();
var_dump(is_null($append->getIteratorIndex()));
var_dump(is_null($append->getInnerIterator()));
$append->append(new ArrayIterator([]));
$append->append(new ArrayIterator(["x" => 3, "y" => 4]));
$append->append(new ArrayIterator([9]));
foreach ($append as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo "@";
    echo $append->getIteratorIndex();
    echo ";";
}
$storage = $append->getArrayIterator();
echo "|";
var_dump($storage instanceof ArrayIterator);
var_dump($storage === $append->getArrayIterator());
echo $storage->count();
$storage->offsetSet(7, new ArrayIterator(["z" => 5]));
echo ":";
echo $storage->count();
echo ":";
var_dump($storage->offsetExists(7));
$slot = $storage->offsetGet(7);
foreach ($slot as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
}
echo ":";
foreach ($storage as $source => $inner) {
    echo $source;
    echo "~";
    foreach ($inner as $key => $value) {
        echo $key;
        echo "=";
        echo $value;
    }
    echo ";";
}
echo ":";
foreach ($append as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo "@";
    echo $append->getIteratorIndex();
    echo ";";
}
$storage->offsetUnset(1);
echo ":";
echo $storage->count();
echo ":";
foreach ($append as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo "@";
    echo $append->getIteratorIndex();
    echo ";";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "x=3@1;y=4@1;0=9@2;|",
            "bool(true)\n",
            "bool(true)\n",
            "3:4:",
            "bool(true)\n",
            "z=5:",
            "0~;1~x=3y=4;2~0=9;7~z=5;:",
            "x=3@1;y=4@1;0=9@2;z=5@7;:",
            "3:0=9@2;z=5@7;",
        )
    );
}

/// Verifies that multiple iterator need any numeric outputs null for exhausted sources.
#[test]
fn test_multiple_iterator_need_any_numeric_outputs_null_for_exhausted_sources() {
    let out = compile_and_run(
        r#"<?php
$multi = new MultipleIterator(MultipleIterator::MIT_NEED_ANY);
$multi->attachIterator(new ArrayIterator(["a" => 1, "b" => 2]));
$multi->attachIterator(new ArrayIterator([10]));
echo $multi->getFlags();
echo ":";
foreach ($multi as $keys => $values) {
    echo count($keys);
    echo "/";
    echo count($values);
    echo ":";
    echo $keys[0];
    echo ",";
    echo is_null($keys[1]) ? "null" : $keys[1];
    echo "=";
    echo $values[0];
    echo ",";
    echo is_null($values[1]) ? "null" : $values[1];
    echo ";";
}
"#,
    );
    assert_eq!(out, "0:2/2:a,0=1,10;2/2:b,null=2,null;");
}

/// Verifies that multiple iterator assoc flags and need all mode.
#[test]
fn test_multiple_iterator_assoc_flags_and_need_all_mode() {
    let out = compile_and_run(
        r#"<?php
$multi = new MultipleIterator(MultipleIterator::MIT_NEED_ALL | MultipleIterator::MIT_KEYS_ASSOC);
$multi->attachIterator(new ArrayIterator(["a" => 1, "b" => 2]), "left");
$multi->attachIterator(new ArrayIterator(["x" => 10]), "right");
foreach ($multi as $keys => $values) {
    echo $keys["left"];
    echo "/";
    echo $keys["right"];
    echo "=";
    echo $values["left"];
    echo "/";
    echo $values["right"];
    echo ";";
}
echo "|";
$multi->setFlags(MultipleIterator::MIT_NEED_ANY | MultipleIterator::MIT_KEYS_ASSOC);
$multi->rewind();
foreach ($multi as $keys => $values) {
    echo $keys["left"];
    echo "/";
    echo is_null($keys["right"]) ? "null" : $keys["right"];
    echo "=";
    echo $values["left"];
    echo "/";
    echo is_null($values["right"]) ? "null" : $values["right"];
    echo ";";
}
"#,
    );
    assert_eq!(out, "a/x=1/10;|a/x=1/10;b/null=2/null;");
}

/// Verifies that multiple iterator updates duplicate attach info.
#[test]
fn test_multiple_iterator_updates_duplicate_attach_info() {
    let out = compile_and_run(
        r#"<?php
$multi = new MultipleIterator(MultipleIterator::MIT_KEYS_ASSOC);
$it = new ArrayIterator([5]);
$multi->attachIterator($it, "first");
$multi->attachIterator($it, "second");
echo $multi->countIterators();
echo ":";
foreach ($multi as $keys => $values) {
    echo $keys["second"];
    echo "=";
    echo $values["second"];
}
"#,
    );
    assert_eq!(out, "1:0=5");
}

/// Verifies that multiple iterator direct invalid current and key match PHP.
#[test]
fn test_multiple_iterator_direct_invalid_current_and_key_match_php() {
    let out = compile_and_run(
        r#"<?php
$empty = new MultipleIterator();
try {
    $empty->current();
} catch (RuntimeException $e) {
    echo "c:";
    echo $e->getMessage();
    echo ";";
}
try {
    $empty->key();
} catch (RuntimeException $e) {
    echo "k:";
    echo $e->getMessage();
    echo ";";
}

$all = new MultipleIterator(MultipleIterator::MIT_NEED_ALL);
$all->attachIterator(new ArrayIterator([]));
try {
    $all->current();
} catch (RuntimeException $e) {
    echo "C:";
    echo $e->getMessage();
    echo ";";
}
try {
    $all->key();
} catch (RuntimeException $e) {
    echo "K:";
    echo $e->getMessage();
    echo ";";
}

$any = new MultipleIterator(MultipleIterator::MIT_NEED_ANY);
$any->attachIterator(new ArrayIterator([]));
$current = $any->current();
$key = $any->key();
echo ":";
echo is_null($current[0]) ? "cn" : "cv";
echo "/";
echo is_null($key[0]) ? "kn" : "kv";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "c:Called current() on an invalid iterator;",
            "k:Called key() on an invalid iterator;",
            "C:Called current() with non valid sub iterator;",
            "K:Called key() with non valid sub iterator;",
            ":cn/kn",
        )
    );
}

/// Verifies that multiple iterator contains detach and assoc null info error.
#[test]
fn test_multiple_iterator_contains_detach_and_assoc_null_info_error() {
    let out = compile_and_run(
        r#"<?php
$multi = new MultipleIterator();
$one = new ArrayIterator([1]);
$two = new ArrayIterator([2]);
var_dump($multi->containsIterator($one));
$multi->attachIterator($one);
$multi->attachIterator($two);
echo $multi->countIterators();
$multi->detachIterator($one);
var_dump($multi->containsIterator($one));
foreach ($multi as $keys => $values) {
    echo $values[0];
}

try {
    $bad = new MultipleIterator(MultipleIterator::MIT_KEYS_ASSOC);
    $bad->attachIterator(new ArrayIterator([7]));
    foreach ($bad as $keys => $values) {
        echo "bad";
    }
} catch (InvalidArgumentException $e) {
    echo "|";
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(false)\n",
            "2bool(false)\n",
            "2|Sub-Iterator is associated with NULL",
        )
    );
}
