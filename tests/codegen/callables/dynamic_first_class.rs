//! Purpose:
//! Regression coverage for dynamic first-class callable syntax (issue 828).
//!
//! Called from:
//! - The codegen integration test suite.
//!
//! Key details:
//! - Exercises callable conversion, capture lifetime, and single evaluation.

use crate::support::*;

/// The issue's wrapper returns a Closure that preserves its captured environment.
#[test]
fn test_issue_828_dynamic_first_class_callable_wrapper() {
    let out = compile_and_run(r#"<?php
function wrap(callable $method): Closure { return $method(...); }
$prefix = "hello ";
$original = function(string $name) use ($prefix): string { return $prefix . $name; };
$wrapped = wrap($original);
unset($original);
echo get_class($wrapped), "|", $wrapped("Ada"), "|";
$builtin = "strtoupper";
$upper = $builtin(...);
echo $upper("ok");
"#);
    assert_eq!(out, "Closure|hello Ada|OK");
}

/// Postfix and grouped expressions evaluate once before the resulting closure is called.
#[test]
fn test_issue_828_dynamic_first_class_callable_expressions() {
    let out = compile_and_run(r#"<?php
namespace Issue828;
class Closure {}
function factory(): callable {
    echo "created|";
    return fn(int $n): int => $n + 2;
}
$created = factory()(...);
echo $created(3), "|";
$callbacks = [fn(int $n): int => $n * 2];
$element = $callbacks[0](...);
echo $element(4), "|";
$grouped = ($created)(...);
echo $grouped(5), "|", get_class($grouped);
"#);
    assert_eq!(out, "created|5|8|7|Closure");
}

/// Conversion preserves bound receivers and reference parameter writeback.
#[test]
fn test_issue_828_dynamic_first_class_callable_receiver_and_reference() {
    let out = compile_and_run(r#"<?php
class Counter828 {
    public int $offset = 4;
    public function add(int &$value): int { $value += $this->offset; return $value; }
}
$object = new Counter828();
$callback = [$object, "add"];
$closure = $callback(...);
unset($callback);
unset($object);
$value = 3;
echo $closure($value), "|", $value;
"#);
    assert_eq!(out, "7|7");
}

/// Names discovered at runtime keep otherwise unreachable functions and methods alive.
#[test]
fn test_issue_828_dynamic_first_class_callable_reachability() {
    let out = compile_and_run(r#"<?php
function hidden828(): string { return "function|"; }
function pick828(): string { return "hidden828"; }
$name = $argc > 0 ? "hidden828" : "hidden828";
$function = $name(...);
echo $function();
$factory = pick828()(...);
echo $factory();
class Hidden828 {
    public function ping(): string { return "method"; }
    public function pong(): string { return "other"; }
}
$box = new Hidden828();
$method = $argc > 1 ? "pong" : "ping";
$callback = [$box, $method];
$closure = $callback(...);
echo $closure();
"#);
    assert_eq!(out, "function|function|method");
}

/// Invalid targets throw Error for syntax and TypeError for the explicit PHP factory.
#[test]
fn test_issue_828_dynamic_first_class_callable_errors() {
    let out = compile_and_run(r#"<?php
$name = "missing_callable_828";
try { $f = $name(...); } catch (Error $e) { echo get_class($e), "|"; }
$number = 1;
try { $f = $number(...); } catch (Error $e) { echo get_class($e), "|"; }
$invalid = $argc > 0 ? 1 : "missing_callable_828";
try { $f = $invalid(...); } catch (Error $e) { echo get_class($e), "|"; }
try { $f = Closure::fromCallable($name); } catch (TypeError $e) { echo get_class($e); }
$short = ["strtoupper"];
try { $f = $short(...); } catch (Error $e) { echo "|short"; }
$extra = ["Missing828", "m", "extra"];
try { $f = $extra(...); } catch (Error $e) { echo "|extra"; }
$keys = [0 => "Missing828", 2 => "m"];
try { $f = $keys(...); } catch (Error $e) { echo "|keys"; }
"#);
    assert_eq!(out, "Error|Error|Error|TypeError|short|extra|keys");
}

/// All callable storage shapes preserve runtime receivers and key-based array selectors.
#[test]
fn test_issue_828_dynamic_first_class_callable_storage_shapes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Parent828 {}
class Child828 extends Parent828 {
    public function __invoke(): string { return "invoke|"; }
    public function read(): string { return "array|"; }
}
function objectWrap828(object $value): Closure { return $value(...); }
function parentWrap828(Parent828 $value): Closure { return $value(...); }
function iterableWrap828(iterable $value): Closure { return $value(...); }
$object = new Child828();
$assoc = [1 => "read", 0 => $object];
$closure = $assoc(...);
echo $closure();
$iterable = iterableWrap828($assoc);
echo $iterable();
$generic = objectWrap828($object);
echo $generic();
$parent = parentWrap828($object);
echo $parent();
$plain = new Parent828();
try { $invalid = $plain(...); } catch (Error $e) { echo get_class($e); }
"#);
    assert_eq!(out.stdout, "array|array|invoke|invoke|Error", "{}", out.stderr);
    assert!(out.success, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The explicit factory shares named and unpacked argument planning with ordinary calls.
#[test]
fn test_issue_828_dynamic_first_class_callable_factory_arguments() {
    let out = compile_and_run(r#"<?php
$name = "strtoupper";
$spread = Closure::fromCallable(...[$name]);
echo $spread("ok"), "|";
$named = Closure::fromCallable(callback: $name);
echo $named("yes");
"#);
    assert_eq!(out, "OK|YES");
}

/// Factory descriptors retain lexical access after escaping and deny outside conversions.
#[test]
fn test_issue_828_dynamic_first_class_callable_lexical_scope() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Scoped828 {
    private function secret(): string { return "private|"; }
    protected function inherited(): string { return "protected|"; }
    private static function hiddenStatic(): string { return "static|"; }
    public function make(): Closure {
        $callback = [$this, "secret"];
        return $callback(...);
    }
    public function factory(): Closure {
        $callback = [$this, "secret"];
        return Closure::fromCallable($callback);
    }
    public static function makeStatic(): Closure {
        $callback = self::class . "::hiddenStatic";
        return $callback(...);
    }
    public static function makeStaticArray(string $class): Closure {
        $callback = [$class, "hiddenStatic"];
        return $callback(...);
    }
    public static function makeStaticString(string $class): Closure {
        $callback = $class . "::hiddenStatic";
        return Closure::fromCallable($callback);
    }
}
class ScopedChild828 extends Scoped828 {
    private function secret(): string { return "child|"; }
    public function makeProtected(): Closure {
        $callback = [1 => "inherited", 0 => $this];
        return $callback(...);
    }
}
class ScopedStaticShadow828 extends Scoped828 {
    private static function hiddenStatic(): string { return "shadow|"; }
}
class ScopedStaticGrandchild828 extends ScopedStaticShadow828 {}
class ProtectedRoot828 {
    protected function f(): string { return "root|"; }
    protected static function g(): string { return "root|"; }
}
class ProtectedLeft828 extends ProtectedRoot828 {
    public function make(object $object): Closure {
        $callback = [$object, "f"];
        return $callback(...);
    }
    public static function makeStatic(string $class): Closure {
        $callback = [$class, "g"];
        return Closure::fromCallable($callback);
    }
}
class ProtectedRight828 extends ProtectedRoot828 {
    protected function f(): string { return "right|"; }
    protected static function g(): string { return "right|"; }
}
class ProtectedGrandchild828 extends ProtectedRight828 {}
$object = new Scoped828();
$child = new ScopedChild828();
$first = $object->make();
$second = $object->factory();
$third = Scoped828::makeStatic();
$fourth = $child->makeProtected();
$fifth = $child->make();
echo $first(), $second(), $third(), $fourth(), $fifth();
$sixth = Scoped828::makeStaticArray(ScopedChild828::class);
$seventh = Scoped828::makeStaticString(ScopedChild828::class);
echo $sixth(), $seventh();
$left = new ProtectedLeft828();
$sibling = $left->make(new ProtectedRight828());
$descendant = $left->make(new ProtectedGrandchild828());
$siblingStatic = ProtectedLeft828::makeStatic(ProtectedRight828::class);
$descendantStatic = ProtectedLeft828::makeStatic(ProtectedGrandchild828::class);
echo $sibling(), $descendant(), $siblingStatic(), $descendantStatic();
try { $bad = Scoped828::makeStaticArray(ScopedStaticShadow828::class); } catch (Error $e) { echo "shadow|"; }
try { $bad = Scoped828::makeStaticString(ScopedStaticGrandchild828::class); } catch (TypeError $e) { echo "grandchild|"; }
$outside = [$object, "secret"];
try { $bad = $outside(...); } catch (Error $e) { echo get_class($e), "|"; }
try { $bad = Closure::fromCallable($outside); } catch (TypeError $e) { echo get_class($e); }
"#);
    assert_eq!(out.stdout, "private|private|static|protected|private|static|static|right|right|right|right|shadow|grandchild|Error|TypeError", "{}", out.stderr);
    assert!(out.success, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
