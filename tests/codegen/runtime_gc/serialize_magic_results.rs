//! Purpose:
//! Checks native magic serializer return layouts and ownership across recursive encoding.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Declared array returns cross a boxed Mixed boundary, unlike concrete inferred arrays.
//! - Both execution modes check wire bytes; heap debugging also checks exceptional cleanup.

use crate::support::*;

/// Indexed and associative magic results retain their payloads until their wire bodies are complete.
#[test]
fn test_core_serialize_boxed_magic_array_results_release_their_owners() {
    let source = r#"<?php
class P {
    public function __serialize(): array { return ['x' => 1, 'y' => str_repeat('a', 3)]; }
}
class Q {
    public function __serialize(): array { return [5, str_repeat('h', 2)]; }
}
for ($i = 0; $i < 2; $i++) {
    $p = new P();
    $q = new Q();
    echo 'prefix|' . serialize($p), '|', serialize($q), "\n";
    unset($p, $q);
}
"#;
    let expected = "prefix|O:1:\"P\":2:{s:1:\"x\";i:1;s:1:\"y\";s:3:\"aaa\";}|O:1:\"Q\":2:{i:0;i:5;i:1;s:2:\"hh\";}\n";
    assert_clean_magic_result(source, &expected.repeat(2));
}

/// Concrete inferred return storage remains valid alongside declared boxed array returns.
#[test]
fn test_core_serialize_inferred_magic_array_result_remains_supported() {
    let source = r#"<?php
class U {
    public function __serialize() { return ['z' => str_repeat('b', 2)]; }
}
$u = new U();
echo serialize($u);
unset($u);
"#;
    assert_clean_magic_result(source, "O:1:\"U\":1:{s:1:\"z\";s:2:\"bb\";}");
}

/// A dynamic non-array return is rejected before reading a container header and is still released.
#[test]
fn test_core_serialize_invalid_magic_result_is_catchable_and_released() {
    let source = r#"<?php
function invalidMagicResult(int $count): mixed { return str_repeat('bad', $count); }
class Bad {
    public function __serialize() { return invalidMagicResult(2); }
}
$bad = new Bad();
try { serialize($bad); }
catch (TypeError $error) { echo get_class($error), '|'; unset($error); }
unset($bad);
echo serialize(9);
"#;
    assert_clean_magic_result(source, "TypeError|i:9;");
}

/// Releasing the magic return owner must not invalidate a separately owned object property.
#[test]
fn test_core_serialize_magic_shared_property_array_keeps_its_owner() {
    let source = r#"<?php
class S {
    public array $values = ['k' => 'v'];
    public function __serialize(): array { return $this->values; }
}
$s = new S();
echo serialize($s), '|', serialize($s), '|', $s->values['k'];
unset($s);
"#;
    assert_clean_magic_result(source, "O:1:\"S\":1:{s:1:\"k\";s:1:\"v\";}|O:1:\"S\":1:{s:1:\"k\";s:1:\"v\";}|v");
}

/// An exception from a nested magic hook still retires its enclosing temporary array and children.
#[test]
fn test_core_serialize_magic_nested_throw_releases_returned_array() {
    let source = r#"<?php
class I {
    public function __serialize(): array { throw new Exception('stop'); }
    public function __destruct() { echo 'inner|'; }
}
class O {
    public function __serialize(): array { return ['text' => str_repeat('x', 4), 'child' => new I()]; }
}
for ($i = 0; $i < 2; $i++) {
    $o = new O();
    try { serialize($o); }
    catch (Exception $error) { echo $error->getMessage(), '|'; unset($error); }
    unset($o);
}
echo serialize(7);
"#;
    assert_clean_magic_result(source, "inner|stop|inner|stop|i:7;");
}

/// A destructor throwing while the return array is retired propagates only after that array is freed.
#[test]
fn test_core_serialize_magic_result_destructor_throw_retires_container() {
    let source = r#"<?php
class V {
    public function __destruct() { echo 'drop|'; throw new Exception('cleanup'); }
}
class R {
    public function __serialize(): array { return [new V()]; }
}
$r = new R();
try { serialize($r); }
catch (Exception $error) { echo $error->getMessage(), '|'; unset($error); }
unset($r);
echo serialize(8);
"#;
    assert_clean_magic_result(source, "drop|cleanup|i:8;");
}

/// Boxed sleep names preserve property order and visibility mangling without leaking their return owner.
#[test]
fn test_core_serialize_sleep_boxed_names_preserve_order_and_mangled_keys() {
    let source = r#"<?php
class T {
    public int $x = 7;
    protected string $y = 'z';
    private bool $a = true;
    public function __sleep(): array { return ['a', 'x', 'y']; }
}
for ($i = 0; $i < 2; $i++) {
    $t = new T();
    echo serialize($t), "\n";
    unset($t);
}
"#;
    let expected = "O:1:\"T\":3:{s:4:\"\0T\0a\";b:1;s:1:\"x\";i:7;s:4:\"\0*\0y\";s:1:\"z\";}\n";
    assert_clean_magic_result(source, &expected.repeat(2));
}

/// Sleep consumes array values, including boxed associative entries, rather than assuming packed strings.
#[test]
fn test_core_serialize_sleep_associative_names_share_their_property_owner() {
    let source = r#"<?php
class H {
    public int $x = 7;
    public string $tag = 'ready';
    public array $names = ['first' => 'tag', 'second' => 'x'];
    public function __sleep(): array { return $this->names; }
}
$h = new H();
echo serialize($h), '|', serialize($h), '|', $h->names['first'];
unset($h);
"#;
    let wire = "O:1:\"H\":2:{s:3:\"tag\";s:5:\"ready\";s:1:\"x\";i:7;}";
    assert_clean_magic_result(source, &format!("{wire}|{wire}|tag"));
}

/// A nested serializer exception retires sleep's names and converted string before the outer catch.
#[test]
fn test_core_serialize_sleep_nested_throw_releases_name_owners() {
    let source = r#"<?php
class SleepChild {
    public function __serialize(): array { throw new Exception('nested'); }
    public function __destruct() { echo 'drop|'; }
}
class SleepOuter {
    public SleepChild $child;
    public function __construct() { $this->child = new SleepChild(); }
    public function __sleep(): array { return [str_repeat('child', 1)]; }
}
for ($i = 0; $i < 2; $i++) {
    $outer = new SleepOuter();
    try { serialize($outer); }
    catch (Exception $error) { echo $error->getMessage(), '|'; unset($error); }
    unset($outer);
}
echo serialize(8);
"#;
    assert_clean_magic_result(source, "nested|drop|nested|drop|i:8;");
}

/// A warning handler may throw on an invalid sleep result without abandoning that result's owner.
#[test]
fn test_core_serialize_sleep_invalid_return_warning_throw_releases_owner() {
    let source = r#"<?php
function invalidSleepResult(): mixed { return str_repeat('bad', 2); }
class BadSleep {
    public function __sleep() { return invalidSleepResult(); }
}
set_error_handler(function (int $level, string $message): bool { throw new Exception('warning'); });
$bad = new BadSleep();
try { serialize($bad); }
catch (Exception $error) { echo $error->getMessage(), '|'; unset($error); }
restore_error_handler();
unset($bad);
echo serialize(9);
"#;
    assert_clean_magic_result(source, "warning|i:9;");
}

/// Class-qualified sleep warnings arrive intact on both native ABIs before a handled null result.
#[test]
fn test_core_serialize_sleep_warning_handler_receives_class_and_message() {
    let source = r#"<?php
function invalidNames(): mixed { return str_repeat('bad', 2); }
class WarningSleep {
    public function __sleep() { return invalidNames(); }
}
set_error_handler(function (int $level, string $message): bool {
    echo $level, ':', str_contains($message, 'WarningSleep::__sleep()') ? 'class' : 'bad', '|';
    return true;
});
$object = new WarningSleep();
echo serialize($object);
restore_error_handler();
unset($object);
"#;
    assert_clean_magic_result(source, "2:class|N;");
}

/// Invalid sleep returns replace only the provisional object prefix after a handled warning.
#[test]
fn test_core_serialize_sleep_invalid_return_preserves_outer_concat_prefix() {
    let source = r#"<?php
function scalarSleepResult(): mixed { return str_repeat('bad', 2); }
class InvalidSleep {
    public function __sleep() { return scalarSleepResult(); }
}
set_error_handler(function (int $level, string $message): bool { return true; });
$bad = new InvalidSleep();
echo 'prefix|' . serialize($bad), '|', serialize(1);
restore_error_handler();
unset($bad);
"#;
    assert_clean_magic_result(source, "prefix|N;|i:1;");
}

/// Requires exact serialization output and balanced native heap ownership without weakening failures.
fn assert_clean_magic_result(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}
