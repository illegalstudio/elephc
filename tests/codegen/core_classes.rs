//! Purpose:
//! Exercises the completed Core class surface through native code and opaque eval.
//!
//! Called from:
//! - The codegen_tests integration harness.
//!
//! Key details:
//! - Weak ownership, serialization wire compatibility, attributes, and enum interfaces have runtime checks.

use crate::support::*;

/// Nested WeakMap writes are read back from the stored entry rather than a discarded temporary.
#[test]
fn test_core_classes_weak_map_nested_writes() {
    let source = r#"<?php
class NestedWeakKey {}
$key = new NestedWeakKey();
$map = new WeakMap();
$map[$key] = ['count' => 0];
$map[$key]['count'] = 1;
$map[$key]['count'] += 2;
echo $map[$key]['count'];
unset($map, $key);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

mod review;
mod throwable_state;
mod weak_map_places;

/// Property markers accept inherited native and interpreted class and interface contracts.
#[test]
fn test_core_classes_property_override_in_opaque_eval() {
    let out = compile_and_run(r#"<?php
class NativeOverrideParent { public int $value = 1; }
interface NativeOverrideContract { public int $value { get; } }
class NativeOverrideChild extends NativeOverrideParent { #[Override] public int $value = 2; }
class NativeOverrideImpl implements NativeOverrideContract { #[Override] public int $value = 3; }
echo (new NativeOverrideChild())->value, ':', (new NativeOverrideImpl())->value, ':';
$source = 'class EvalNativeOverride extends NativeOverrideParent { #[Override] public int $value = 4; }
class EvalNativeImpl implements NativeOverrideContract { #[Override] public int $value = 5; }
class EvalOverrideParent { public int $value = 6; }
class EvalOverrideChild extends EvalOverrideParent { #[Override] public int $value = 7; }
interface EvalOverrideContract { public int $value { get; } }
class EvalOverrideImpl implements EvalOverrideContract { #[Override] public int $value = 8; }
echo (new EvalNativeOverride())->value, ":", (new EvalNativeImpl())->value, ":";
echo (new EvalOverrideChild())->value, ":", (new EvalOverrideImpl())->value;' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "2:3:4:5:7:8");
}

/// Key cycles do not become strong edges through a WeakMap observer registration.
#[test]
fn test_core_classes_weak_map_cycle_collection() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class MapCycleKey { public ?MapCycleKey $self = null; }
$map = new WeakMap();
$key = new MapCycleKey(); $key->self = $key;
$map[$key] = str_repeat('v', 30);
$weak = WeakReference::create($key);
unset($key); gc_collect_cycles();
echo count($map), ':', $weak->get() === null ? 'dead' : 'bad';
unset($map, $weak); gc_collect_cycles();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "0:dead");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A resurrected key keeps its weak observers until its final release.
#[test]
fn test_core_classes_weak_map_resurrected_key() {
    let out = compile_and_run(r#"<?php
$saved = null;
class ResurrectedMapKey {
    public function __destruct() { global $saved; $saved = $this; echo 'saved:'; }
}
$map = new WeakMap(); $key = new ResurrectedMapKey();
$weak = WeakReference::create($key); $map[$key] = 'value';
unset($key);
echo count($map), ':', $weak->get() === $saved ? 'live:' : 'bad:';
unset($saved);
echo count($map), ':', $weak->get() === null ? 'dead' : 'bad';
"#);
    assert_eq!(out, "saved:1:live:0:dead");
}

/// Throwing value destructors do not leave other maps registered against a dead key.
#[test]
fn test_core_classes_weak_map_throwing_value_cleanup() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingMapValue {
    public function __destruct() { echo 'drop:'; throw new Exception('value'); }
}
$first = new WeakMap(); $second = new WeakMap(); $key = new stdClass();
$first[$key] = new ThrowingMapValue(); $second[$key] = new ThrowingMapValue();
try { unset($key); } catch (Throwable $error) { echo 'caught:'; }
echo count($first), ':', count($second);
unset($error, $first, $second); gc_collect_cycles();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "drop:drop:caught:0:0");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Delayed Core target errors surface when native and interpreted Reflection instantiate them.
#[test]
fn test_core_classes_delayed_attribute_validation() {
    let out = compile_and_run(r#"<?php
#[DelayedTargetValidation, SensitiveParameter] class DeferredCoreAttribute {}
$attributes = (new ReflectionClass(DeferredCoreAttribute::class))->getAttributes(SensitiveParameter::class);
try { $attributes[0]->newInstance(); } catch (Error $error) { echo 'native:'; }
$source = '#[DelayedTargetValidation, SensitiveParameter] class EvalDeferredCoreAttribute {}
$reflection = new ReflectionClass(EvalDeferredCoreAttribute::class);
$attributes = $reflection->getAttributes(SensitiveParameter::class);
try { $attributes[0]->newInstance(); } catch (Error $error) { echo "eval"; }' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "native:eval");
}

/// Opaque eval emits NoDiscard warnings for ignored returns from interpreted functions.
#[test]
fn test_core_classes_no_discard_in_opaque_eval() {
    let out = compile_and_run_capture(r#"<?php
$source = '#[NoDiscard] function eval_marked(): int { return 7; }
eval_marked(); $_ = eval_marked(); echo $_;' . ' // ' . $argc;
eval($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "7");
    assert_eq!(out.stderr.matches("The return value of eval_marked()").count(), 1, "{}", out.stderr);
}

/// Serializable writes an opaque C envelope and restores binary bytes without construction.
#[test]
fn test_core_classes_serializable_roundtrip() {
    let out = compile_and_run(r#"<?php
class LegacyPacket implements Serializable {
    public string $payload = '';
    public function serialize(): ?string { return $this->payload; }
    public function unserialize(string $data): void { $this->payload = $data; }
}
$packet = new LegacyPacket();
$packet->payload = "a}\0b";
$wire = serialize($packet);
echo bin2hex($wire), ':';
$restored = unserialize($wire);
echo get_class($restored), ':', bin2hex($restored->payload), ':';
$pair = unserialize(serialize([$packet, $packet]));
echo $pair[0] === $pair[1] ? 'same:' : 'bad:';
echo get_class(unserialize($wire, ['allowed_classes' => false]));
"#);
    assert_eq!(out, "433a31323a224c65676163795061636b6574223a343a7b617d00627d:LegacyPacket:617d0062:same:__PHP_Incomplete_Class");
}

/// Modern magic hooks supersede legacy serialization and a null legacy result writes N.
#[test]
fn test_core_classes_serializable_hook_precedence_and_null() {
    let out = compile_and_run(r#"<?php
class LegacyNull implements Serializable {
    public function serialize(): ?string { return null; }
    public function unserialize(string $data): void { echo 'bad'; }
}
class ModernLegacy implements Serializable {
    public function serialize(): ?string { return 'legacy'; }
    public function unserialize(string $data): void { echo 'bad'; }
    public function __serialize(): array { return ['answer' => 42]; }
    public function __unserialize(array $data): void { echo $data['answer']; }
}
echo serialize(new LegacyNull()), ':';
$wire = serialize(new ModernLegacy());
echo $wire, ':';
unserialize($wire);
"#);
    assert_eq!(out, "N;:O:12:\"ModernLegacy\":1:{s:6:\"answer\";i:42;}:42");
}

/// Untyped legacy parameters use boxed cells and hook exceptions retain balanced cleanup.
#[test]
fn test_core_classes_serializable_ownership_and_boxed_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class BoxedLegacy implements Serializable {
    public string $value = '';
    public function serialize(): ?string { return str_repeat('p', 30); }
    public function unserialize($data): void { $this->value = (string)$data; }
}
for ($i = 0; $i < 5; $i++) {
    $object = unserialize(serialize(new BoxedLegacy()));
    echo strlen($object->value), ':';
    unset($object);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "30:30:30:30:30:");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Invalid opaque payload lengths fail preflight before any user deserialization runs.
#[test]
fn test_core_classes_serializable_rejects_truncated_payload() {
    let out = compile_and_run(r#"<?php
class CheckedLegacy implements Serializable {
    public function serialize(): ?string { return 'x'; }
    public function unserialize(string $data): void { echo 'hook:'; }
}
var_dump(@unserialize('C:13:"CheckedLegacy":99:{x}'));
var_dump(@unserialize('C:13:"CheckedLegacy":18446744073709551615:{x}'));
"#);
    assert_eq!(out, "bool(false)\nbool(false)\n");
}

/// Sensitive fixed and variadic arguments are wrapped without changing their live values.
#[test]
fn test_core_classes_sensitive_backtrace_arguments() {
    let out = compile_and_run(r#"<?php
function secret(#[SensitiveParameter] string $password, int $visible): void {
    $frames = debug_backtrace();
    $args = $frames[0]['args'];
    echo get_class($args[0]), ':', $args[0]->getValue(), ':', $args[1], ':';
    echo func_get_arg(0), ':';
    debug_print_backtrace();
}
secret('password', 7);
function tokens(#[SensitiveParameter] string ...$tokens): void {
    $frames = debug_backtrace();
    $args = $frames[0]['args'];
    echo get_class($args[0]), ':', $args[0]->getValue(), ':';
    echo get_class($args[1]), ':', $args[1]->getValue();
}
tokens('first', 'second');
"#);
    assert!(out.starts_with("SensitiveParameterValue:password:7:password:"), "{out}");
    assert!(out.contains("secret(Object(SensitiveParameterValue), 7)"), "{out}");
    assert!(out.ends_with("SensitiveParameterValue:first:SensitiveParameterValue:second"), "{out}");
}

/// Interpreted parameters retain SensitiveParameter metadata for backtrace redaction.
#[test]
fn test_core_classes_sensitive_backtrace_in_opaque_eval() {
    let out = compile_and_run(r#"<?php
$source = 'function eval_secret(#[SensitiveParameter] string $password): void {
    $frames = debug_backtrace(); $args = $frames[0]["args"];
    echo get_class($args[0]), ":", $args[0]->getValue();
} eval_secret("eval-password");' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "SensitiveParameterValue:eval-password");
}

/// WeakMap owns values while key release removes observing entries immediately.
#[test]
fn test_core_classes_weak_map_key_lifetime_and_null_values() {
    let out = compile_and_run(r#"<?php
class WeakMapValue { public function __destruct() { echo 'released:'; } }
$map = new WeakMap();
$key = new stdClass();
$map[$key] = new WeakMapValue();
echo count($map), ':', isset($map[$key]) ? 'present:' : 'bad:';
unset($key);
echo count($map), ':';
$nullKey = new stdClass();
$map[$nullKey] = null;
echo count($map), ':', isset($map[$nullKey]) ? 'bad:' : 'null:';
unset($map[$nullKey]);
echo count($map);
"#);
    assert_eq!(out, "1:present:released:0:1:null:0");
}

/// Cloned maps own independent value tables while sharing weak key lifetime.
#[test]
fn test_core_classes_weak_map_clone_and_live_iteration() {
    let out = compile_and_run(r#"<?php
$map = new WeakMap();
$first = new stdClass();
$second = new stdClass();
$map[$first] = 'first';
$map[$second] = 'second';
$copy = clone $map;
$copy[$first] = 'copy';
echo $map[$first], ':', $copy[$first], ':';
foreach ($map as $object => $value) {
    echo $value, ':', $object === $first || $object === $second ? 'object:' : 'bad:';
}
unset($object, $first, $second);
echo count($map), ':', count($copy);
"#);
    assert_eq!(out, "first:copy:first:object:second:object:0:0");
}

/// Opaque eval uses the same native weak map state and lifetime callbacks.
#[test]
fn test_core_classes_weak_map_in_opaque_eval() {
    let out = compile_and_run(r#"<?php
$source = '$map = new WeakMap(); $key = new stdClass();
$map[$key] = "eval"; echo $map[$key], ":", count($map), ":";
unset($key); echo count($map);' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "eval:1:0");
}

/// Destroyed maps remove their weak registrations before ordinary property cleanup.
#[test]
fn test_core_classes_weak_map_registry_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
for ($i = 0; $i < 10; $i++) {
    $map = new WeakMap();
    $key = new stdClass();
    $map[$key] = str_repeat('v', 24);
    $copy = clone $map;
    unset($map, $key, $copy);
}
gc_collect_cycles();
echo 'clean';
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "clean");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Weak wrappers preserve cache identity without keeping their referents alive.
#[test]
fn test_core_classes_weak_reference_lifetime_and_identity() {
    let out = compile_and_run(r#"<?php
class WeakTarget { public int $value = 42; }
$object = new WeakTarget();
$reference = WeakReference::create($object);
$same = WeakReference::create($object);
echo $reference === $same ? 'same:' : 'bad:';
echo $reference->get() === $object ? 'live:' : 'bad:';
unset($object);
echo $reference->get() === null ? 'dead:' : 'bad:';
$replacement = new WeakTarget();
echo $reference->get() === null ? 'no-reuse:' : 'bad:';
$second = WeakReference::create($replacement);
unset($second);
$second = WeakReference::create($replacement);
echo $second->get() === $replacement ? 'recreated:' : 'bad:';
$source = '$object = new stdClass(); $weak = WeakReference::create($object);
echo $weak->get() === $object ? "eval-live:" : "bad:";
unset($object); echo $weak->get() === null ? "eval-dead" : "bad";' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "same:live:dead:no-reuse:recreated:eval-live:eval-dead");
}

/// GC traces strong object cycles while ignoring weak referent pointers.
#[test]
fn test_core_classes_weak_reference_cycle_collection() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class WeakCycle { public mixed $next = null; }
$left = new WeakCycle();
$right = new WeakCycle();
$left->next = $right;
$right->next = $left;
$weak = WeakReference::create($left);
unset($left, $right);
gc_collect_cycles();
echo $weak->get() === null ? 'collected' : 'retained';
unset($weak);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "collected");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// ErrorException keeps its six-argument constructor state through widened Throwable access.
#[test]
fn test_core_classes_error_exception() {
    let out = compile_and_run(r#"<?php
function location(Exception $error): string { return $error->getFile() . ':' . $error->getLine(); }
$previous = new Exception('previous');
$error = new ErrorException('failure', 12, E_WARNING, 'input.php', 45, $previous);
echo $error->getMessage(), ':', $error->getCode(), ':', $error->getSeverity(), ':';
echo location($error), ':', $error->getPrevious()->getMessage(), ':';
$source = '$error = new ErrorException(message: "eval", severity: E_NOTICE, filename: "eval.php", line: 7);
echo $error->getMessage(), ":", $error->getSeverity(), ":", $error->getFile(), ":", $error->getLine();' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "failure:12:2:input.php:45:previous:eval:8:eval.php:7");
}

/// Core throwable additions preserve their parents, standard state, and eval constructors.
#[test]
fn test_core_classes_throwable_hierarchy_and_eval() {
    let out = compile_and_run(r#"<?php
$previous = new Exception('previous');
$parse = new ParseError('syntax', 17, $previous);
echo $parse instanceof CompileError ? 'compile:' : 'bad:';
echo $parse instanceof Error ? 'error:' : 'bad:';
echo $parse->getMessage(), ':', $parse->getCode(), ':', $parse->getPrevious()->getMessage(), ':';
echo (new ClosedGeneratorException('closed'))->getMessage(), ':';
echo (new RequestParseBodyException('body'))->getMessage(), ':';
$source = '$error = new ParseError("eval", 9);
echo $error instanceof CompileError ? "compile:" : "bad:";
echo $error->getMessage(), ":", $error->getCode(), ":";
echo (new ClosedGeneratorException("closed"))->getMessage();' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "compile:error:syntax:17:previous:closed:body:compile:eval:9:closed");
}

/// Core enum interfaces are implicit, transitive, and available through runtime metadata.
#[test]
fn test_core_classes_enum_interfaces() {
    let out = compile_and_run(r#"<?php
interface Marker extends UnitEnum {}
enum Plain implements Marker { case First; }
enum Backed: string { case First = 'first'; }
echo interface_exists('UnitEnum') && interface_exists('BackedEnum') ? 'interfaces:' : 'bad:';
echo Plain::First instanceof UnitEnum ? 'unit:' : 'bad:';
echo Plain::First instanceof BackedEnum ? 'bad:' : 'pure:';
echo Backed::First instanceof UnitEnum && Backed::First instanceof BackedEnum ? 'backed:' : 'bad:';
$interfaces = class_implements(Backed::class);
echo $interfaces['UnitEnum'], ':', $interfaces['BackedEnum'], ':';
echo count((new ReflectionClass('UnitEnum'))->getMethods()), ':';
echo count((new ReflectionClass('BackedEnum'))->getMethods());
"#);
    assert_eq!(out, "interfaces:unit:pure:backed:UnitEnum:BackedEnum:1:3");
}

/// Predefined attribute objects expose typed constructor state and their own metadata.
#[test]
fn test_core_classes_attribute_objects() {
    let out = compile_and_run(r#"<?php
$attribute = new Attribute();
echo $attribute->flags, ':', Attribute::TARGET_ALL, ':', Attribute::TARGET_CONSTANT, ':', Attribute::IS_REPEATABLE, ':';
echo (new ReflectionClass(DelayedTargetValidation::class))->getAttributes(Attribute::class)[0]->newInstance()->flags, ':';
$deprecated = new Deprecated(since: '1.2', message: 'use replacement');
echo $deprecated->message, ':', $deprecated->since, ':';
$nodiscard = new NoDiscard('consume result');
echo $nodiscard->message, ':';
echo (new ReflectionClass('AllowDynamicProperties'))->isFinal() ? 'final:' : 'bad:';
echo count((new ReflectionClass('SensitiveParameter'))->getAttributes()), ':';
echo (string)(new ReflectionMethod(WeakReference::class, 'create'))->getParameters()[0]->getType(), ':';
$source = '$attribute = new Attribute(Attribute::TARGET_METHOD);
$deprecated = new Deprecated(message: "eval", since: "2");
echo $attribute->flags, ":", $deprecated->message, ":", $deprecated->since;
echo ":", class_exists("DelayedTargetValidation") ? "delayed" : "bad";
echo ":", (new Attribute())->flags, ":", Attribute::TARGET_CONSTANT, ":", Attribute::IS_REPEATABLE;' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "127:127:64:128:127:use replacement:1.2:consume result:final:1:object:4:eval:2:delayed:127:64:128");
}

/// Sensitive wrappers retain arbitrary values while excluding them from debug output.
#[test]
fn test_core_classes_sensitive_parameter_value() {
    let out = compile_and_run(r#"<?php
$wrapped = new SensitiveParameterValue(['token' => 'secret']);
$value = $wrapped->getValue();
echo $value['token'], ':', count($wrapped->__debugInfo()), ':';
$source = '$wrapped = new SensitiveParameterValue(42); echo $wrapped->getValue();' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "secret:0:42");
}


/// Closures retain their callable identity through weak wrappers without becoming owned keys.
#[test]
fn test_core_classes_weak_closure_keys() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$captured = str_repeat('c', 30);
$closure = fn(): string => $captured;
$weak = WeakReference::create($closure);
echo $weak->get() === $closure ? 'same:' : 'bad:';
$map = new WeakMap(); $map[$closure] = 'closure';
echo $map[$closure], ':', count($map), ':';
$copy = $weak->get(); unset($closure);
echo count($map), ':';
unset($copy);
echo count($map), ':', $weak->get() === null ? 'dead' : 'bad';
unset($map, $weak, $captured); gc_collect_cycles();
$source = '$evalClosure = function (): int { return 42; };
$evalWeak = WeakReference::create($evalClosure); $evalMap = new WeakMap();
$evalMap[$evalClosure] = "closure";
echo $evalWeak->get() === $evalClosure ? ":eval:" : ":bad:";
unset($evalClosure); echo count($evalMap), ":", $evalWeak->get() === null ? "dead" : "bad";
unset($evalMap, $evalWeak);' . ' // ' . $argc;
eval($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "same:closure:1:1:0:dead:eval:0:dead");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Internal Core prelude classes (`__ElephcWeakMapIterator`) stay out of
/// `get_declared_classes()`, so instantiating every listed class does not reach them.
#[test]
fn test_declared_classes_hide_internal_core_prelude_classes() {
    let out = compile_and_run(
        r#"<?php
$map = new WeakMap();
foreach (get_declared_classes() as $class) {
    if (stripos($class, '__elephc') === 0) {
        echo "internal: ", $class, "\n";
    }
}
echo in_array('WeakMap', get_declared_classes(), true) ? "WeakMap listed\n" : "WeakMap missing\n";
"#,
    );
    assert_eq!(out, "WeakMap listed\n");
}

/// User classes and interfaces whose names merely start with `__elephc` stay listed: only the
/// exact compiler-owned prelude names registered as internal are hidden.
#[test]
fn test_declared_classes_keep_user_names_with_the_internal_prefix() {
    let out = compile_and_run(
        r#"<?php
class __ElephcUser {}
interface __ElephcIface {}
$o = new __ElephcUser();
$map = new WeakMap();
var_dump(in_array("__ElephcUser", get_declared_classes(), true));
var_dump(in_array("__ElephcIface", get_declared_interfaces(), true));
var_dump(in_array("__ElephcWeakMapIterator", get_declared_classes(), true));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(false)\n");
}

/// `SessionHandlerInterface` is a builtin contract outside `--web` too: a user handler can
/// implement it and be passed through a parameter typed with it.
#[test]
fn test_session_handler_interface_is_implementable_without_web() {
    let out = compile_and_run(
        r#"<?php
class H implements SessionHandlerInterface {
    public function open(string $path, string $name): bool { return true; }
    public function close(): bool { return true; }
    public function read(string $id): string|false { return "r" . $id; }
    public function write(string $id, string $data): bool { return true; }
    public function destroy(string $id): bool { return true; }
    public function gc(int $max_lifetime): int|false { return 0; }
}
function useHandler(SessionHandlerInterface $h): string|false { return $h->read("1"); }
$h = new H();
echo ($h instanceof SessionHandlerInterface ? 1 : 0), useHandler($h), "\n";
echo interface_exists('SessionHandlerInterface') ? "exists\n" : "missing\n";
"#,
    );
    assert_eq!(out, "1r1\nexists\n");
}
