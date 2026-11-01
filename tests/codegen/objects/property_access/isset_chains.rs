//! Regression coverage for quiet typed-property probes through compound receivers.

use super::*;

/// Verifies `isset()`, `empty()` and `??` probe a typed property through a receiver whose class
/// is only known at run time. An UNTYPED parameter carries no class for the compiler to name a
/// slot in, so these all fell back to the ordinary read and died with "Typed property Box::$name
/// must not be accessed before initialization" — where PHP answers false, true and the default.
/// Every generated Symfony container factory is written `static function f($container)`, so this
/// is the receiver shape the whole DI container is built on. PHP outputs
/// "unset|set|empty|filled|fallback|ready".
#[test]
fn test_typed_property_probes_through_an_untyped_receiver() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public string $name;
    public string $ready = 'ready';
}

function probeIsset($o): string { return isset($o->name) ? 'set' : 'unset'; }
function probeIssetReady($o): string { return isset($o->ready) ? 'set' : 'unset'; }
function probeEmpty($o): string { return empty($o->name) ? 'empty' : 'filled'; }
function probeEmptyReady($o): string { return empty($o->ready) ? 'empty' : 'filled'; }
function probeCoalesce($o): string { return $o->name ?? 'fallback'; }
function probeCoalesceReady($o): string { return $o->ready ?? 'fallback'; }

$box = new Box();
echo probeIsset($box), '|', probeIssetReady($box), '|';
echo probeEmpty($box), '|', probeEmptyReady($box), '|';
echo probeCoalesce($box), '|', probeCoalesceReady($box);
"#,
    );
    assert_eq!(out, "unset|set|empty|filled|fallback|ready");
}

/// Verifies `??=` assigns through an untyped receiver whose typed slot is uninitialized, and
/// leaves an already-initialized one alone. PHP outputs "assigned|already".
#[test]
fn test_coalesce_assign_through_an_untyped_receiver() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public string $name;
    public string $ready = 'already';
}

function fill($o): string { return $o->name ??= 'assigned'; }
function keep($o): string { return $o->ready ??= 'overwritten'; }

$box = new Box();
echo fill($box), '|', keep($box);
"#,
    );
    assert_eq!(out, "assigned|already");
}


#[test]
fn test_eval_closure_constructor_argument_can_be_stored_and_invoked_natively() {
    let out = compile_and_run(r#"<?php
class NativeStoredClosure {
    public function __construct(private Closure $factory) {}
    public function run(): int { return ($this->factory)(); }
}
class NativeClosureProvider {
    public function value(): int { return 42; }
}
eval('$arrow = static fn () => 42;
echo (new NativeStoredClosure($arrow))->run(), "|";
$provider = new NativeClosureProvider();
$method = $provider->value(...);
echo (new NativeStoredClosure($method))->run();');
"#);
    assert_eq!(out, "42|42");
}

#[test]
fn test_eval_closure_argument_is_accepted_by_native_constructor() {
    let out = compile_and_run(r#"<?php
class NativeClosureConstructor {
    public function __construct(Closure $factory) { echo 'constructed'; }
}
eval('$factory = static fn () => 42; new NativeClosureConstructor($factory);');
"#);
    assert_eq!(out, "constructed");
}

#[test]
fn test_native_dynamic_override_heap_control_without_method_calls() {
    let out = compile_and_run_with_gc_stats(r#"<?php
eval('class DynamicHeapControl {}');
echo 'control';
"#);
    assert_eq!(out.stdout, "control");
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "dynamic declaration baseline leaked: {}", out.stderr);
}

#[test]
fn test_native_dynamic_override_probe_releases_native_call_temporaries() {
    let out = compile_and_run_with_gc_stats(r#"<?php
class NativeProbeReceiver {
    public function value(int $value): int { return $value; }
}
function repeatNativeProbe(NativeProbeReceiver $object): int {
    $value = 7;
    $sum = 0;
    for ($i = 0; $i < 128; $i++) { $sum += $object->value($value); }
    return $sum;
}
eval('class NativeProbeDynamicMarker {}');
$object = new NativeProbeReceiver();
echo repeatNativeProbe($object);
unset($object);
"#);
    assert_eq!(out.stdout, "896");
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "native dispatch fallback leaked: {}", out.stderr);
}

#[test]
fn test_native_dynamic_override_preserves_reference_argument() {
    let out = compile_and_run(r#"<?php
class DynamicRefBase {
    public function update(int &$value): void { $value += 1; }
}
function updateDynamicRef(DynamicRefBase $object, int &$value): void { $object->update($value); }
eval('class DynamicRefChild extends DynamicRefBase {
    public function update(int &$value): void { $value += 10; }
}');
$name = 'DynamicRefChild';
$value = 2;
updateDynamicRef(new $name(), $value);
echo $value;
"#);
    assert_eq!(out, "12");
}

#[test]
fn test_native_dynamic_override_preserves_protected_caller_scope() {
    let out = compile_and_run(r#"<?php
class DynamicProtectedBase {
    protected function value(): int { return 1; }
    public function read(): int { return $this->value(); }
}
function readDynamicProtected(DynamicProtectedBase $object): int { return $object->read(); }
eval('class DynamicProtectedChild extends DynamicProtectedBase {
    protected function value(): int { return 42; }
}');
$name = 'DynamicProtectedChild';
echo readDynamicProtected(new $name());
"#);
    assert_eq!(out, "42");
}

fn dynamic_override_receiver_source(receiver_type: &str) -> String {
    r#"<?php
interface DynamicReadContract { public function read(): int; }
class DynamicReadBase implements DynamicReadContract {
    protected int $value;
    public function read(): int { return $this->value; }
}
function readDynamicReceiver(RECEIVER_TYPE $object): int { return $object->read(); }
eval('class DynamicReadChild extends DynamicReadBase {
    public function read(): int {
        if (!isset($this->value)) { $this->value = 42; }
        return $this->value;
    }
}');
$name = 'DynamicReadChild';
echo readDynamicReceiver(new $name());
"#.replace("RECEIVER_TYPE", receiver_type)
}

#[test]
fn test_native_nullable_call_honors_dynamic_override() {
    assert_eq!(compile_and_run(&dynamic_override_receiver_source("?DynamicReadBase")), "42");
}

#[test]
fn test_native_interface_call_honors_dynamic_override() {
    assert_eq!(compile_and_run(&dynamic_override_receiver_source("DynamicReadContract")), "42");
}

#[test]
fn test_native_mixed_call_honors_dynamic_override() {
    assert_eq!(compile_and_run(&dynamic_override_receiver_source("mixed")), "42");
}

#[test]
fn test_native_private_call_ignores_dynamic_override_name_collision() {
    let out = compile_and_run(r#"<?php
class DynamicPrivateBase {
    private function secret(): string { return 'base'; }
    public function read(): string { return $this->secret(); }
}
function readDynamicPrivate(DynamicPrivateBase $object): string { return $object->read(); }
eval('class DynamicPrivateChild extends DynamicPrivateBase {
    public function secret(): string { return "child"; }
}');
$name = 'DynamicPrivateChild';
echo readDynamicPrivate(new $name());
"#);
    assert_eq!(out, "base");
}

#[test]
fn test_native_typed_call_honors_dynamic_override_initializing_property() {
    let out = compile_and_run(r#"<?php
class DynamicOverrideBase {
    protected int $value;
    public function read(): int { return $this->value; }
}
function readDynamicOverride(DynamicOverrideBase $object): int { return $object->read(); }
eval('class DynamicOverrideChild extends DynamicOverrideBase {
    public function read(): int {
        if (!isset($this->value)) { $this->value = 42; }
        return $this->value;
    }
}');
$name = 'DynamicOverrideChild';
echo readDynamicOverride(new $name());
"#);
    assert_eq!(out, "42");
}

#[test]
fn test_native_member_exists_uses_runtime_subclass_of_typed_receiver() {
    let out = compile_and_run(r#"<?php
class MemberProbeBase {}
class MemberProbeChild extends MemberProbeBase {
    public int $value = 1;
    public function __isset(string $name): bool { return false; }
}
function inspectMemberProbe(MemberProbeBase $object): void {
    echo (int) method_exists($object, '__isset'), ':', (int) property_exists($object, 'value'), '|';
}
inspectMemberProbe(new MemberProbeBase());
inspectMemberProbe(new MemberProbeChild());
"#);
    assert_eq!(out, "0:0|1:1|");
}

#[test]
fn test_native_isset_chain_magic_getter_only_false_probe_and_null_receiver() {
    let out = compile_and_run(r#"<?php
class IssetMagicLeaf { public int $value = 0; }
class IssetMagicPrivate {}
class IssetGetterOnly {
    private IssetMagicPrivate $child;
    public function __get(string $name): IssetMagicLeaf { echo 'get:'; return new IssetMagicLeaf(); }
}
class IssetDisabled extends IssetGetterOnly {
    public function __isset(string $name): bool { echo 'no:'; return false; }
}
function probeMagic(?IssetGetterOnly $holder): bool { return isset($holder->child->value); }
echo (int) probeMagic(null), '|';
echo (int) probeMagic(new IssetGetterOnly()), '|';
echo (int) probeMagic(new IssetDisabled());
"#);
    assert_eq!(out, "0|get:1|no:0");
}

#[test]
fn test_native_isset_chain_magic_receiver_lives_through_both_callbacks() {
    let out = compile_and_run(r#"<?php
class IssetHeldLeaf { public int $value = 1; }
class IssetHeldPrivate {}
class IssetHeldReceiver {
    private IssetHeldPrivate $child;
    public function __isset(string $name): bool { echo 'has:'; return true; }
    public function __get(string $name): IssetHeldLeaf { echo 'get:'; return new IssetHeldLeaf(); }
    public function __destruct() { echo 'drop:'; }
}
function createHeldReceiver(): IssetHeldReceiver { echo 'factory:'; return new IssetHeldReceiver(); }
echo isset(createHeldReceiver()->child->value) ? 'ready' : 'bad';
"#);
    assert_eq!(out, "factory:has:get:drop:ready");
}

#[test]
fn test_native_isset_chain_magic_probe_can_release_the_original_variable() {
    let out = compile_and_run(r#"<?php
class IssetPinnedLeaf { public int $value = 1; }
class IssetPinnedPrivate {}
class IssetPinnedReceiver {
    private IssetPinnedPrivate $child;
    public function __isset(string $name): bool {
        global $magicRoot;
        echo 'has:';
        $magicRoot = null;
        return true;
    }
    public function __get(string $name): IssetPinnedLeaf { echo 'get:'; return new IssetPinnedLeaf(); }
    public function __destruct() { echo 'drop:'; }
}
$magicRoot = new IssetPinnedReceiver();
echo isset($magicRoot->child->value) ? 'ready' : 'bad';
"#);
    assert_eq!(out, "has:get:drop:ready");
}

#[test]
fn test_native_global_object_release_without_argument_owner() {
    let out = compile_and_run(r#"<?php
class IssetGlobalReleaseControl {
    public function __destruct() { echo 'drop:'; }
}
function clearGlobalReleaseControl(): void {
    global $releaseControl;
    $releaseControl = null;
}
$releaseControl = new IssetGlobalReleaseControl();
clearGlobalReleaseControl();
echo 'done';
"#);
    assert_eq!(out, "drop:done");
}

#[test]
fn test_native_typed_receiver_global_release_without_property_probe() {
    let out = compile_and_run(r#"<?php
class IssetLifetimeControl {
    public function clear(): void {
        global $controlRoot;
        echo 'clear:';
        $controlRoot = null;
    }
    public function __destruct() { echo 'drop:'; }
}
function probeLifetimeControl(IssetLifetimeControl $receiver): void {
    $receiver->clear();
}
$controlRoot = new IssetLifetimeControl();
probeLifetimeControl($controlRoot);
echo 'done';
"#);
    assert_eq!(out, "clear:drop:done");
}

#[test]
fn test_native_isset_chain_typed_receiver_survives_global_release() {
    let out = compile_and_run(r#"<?php
class IssetTypedLeaf { public int $value = 1; }
class IssetTypedReceiver {
    private IssetTypedLeaf $child;
    public function __isset(string $name): bool {
        global $typedRoot;
        echo 'has:';
        $typedRoot = null;
        return true;
    }
    public function __get(string $name): IssetTypedLeaf { echo 'get:'; return new IssetTypedLeaf(); }
    public function __destruct() { echo 'drop:'; }
}
function probeTypedReceiver(IssetTypedReceiver $receiver): bool {
    return isset($receiver->child->value);
}
$typedRoot = new IssetTypedReceiver();
echo probeTypedReceiver($typedRoot) ? 'ready' : 'bad';
"#);
    assert_eq!(out, "has:get:drop:ready");
}

#[test]
fn test_native_isset_chain_nullable_instance_and_static_method_receivers() {
    let out = compile_and_run(r#"<?php
class IssetNullableLeaf { public int $value = 0; }
class IssetNullableFactory {
    public function next(bool $present): ?IssetNullableLeaf {
        echo 'instance:';
        return $present ? new IssetNullableLeaf() : null;
    }
    public static function make(bool $present): ?IssetNullableLeaf {
        echo 'static:';
        return $present ? new IssetNullableLeaf() : null;
    }
}
$factory = new IssetNullableFactory();
echo (int) isset($factory->next(false)->value), '|';
echo (int) isset($factory->next(true)->value), '|';
echo (int) isset(IssetNullableFactory::make(false)->value), '|';
echo (int) isset(IssetNullableFactory::make(true)->value);
"#);
    assert_eq!(out, "instance:0|instance:1|static:0|static:1");
}

#[test]
fn test_native_isset_chain_uninitialized_leaf_null_and_zero() {
    let out = compile_and_run(r#"<?php
class IssetNode { public ?IssetNode $child = null; public ?int $value; }
$root = new IssetNode();
$root->child = new IssetNode();
echo isset($root->child->value) ? 'bad' : 'empty';
$root->child->value = 0;
echo isset($root->child->value) ? ':zero' : ':bad';
$root->child->value = null;
echo isset($root->child->value) ? ':bad' : ':null';
"#);
    assert_eq!(out, "empty:zero:null");
}

#[test]
fn test_native_isset_chain_cloned_inherited_uninitialized_property() {
    let out = compile_and_run(r#"<?php
class IssetPayload {}
abstract class IssetBase {
    protected ?IssetPayload $state;
    public function process(IssetPayload $state): void {
        $this->state = $state;
        try { $this->visit(); } finally { $this->state = null; }
    }
    abstract protected function visit(): void;
}
class IssetWorker extends IssetBase {
    private ?self $copy = null;
    public function process(IssetPayload $state): void {
        $this->copy = clone $this;
        parent::process($state);
    }
    protected function visit(): void {
        if (!isset($this->copy->state)) {
            echo 'empty:';
            $this->copy->state = new IssetPayload();
        }
        echo isset($this->copy->state) ? 'ready' : 'bad';
    }
}
(new IssetWorker())->process(new IssetPayload());
"#);
    assert_eq!(out, "empty:ready");
}

#[test]
fn test_native_isset_chain_uninitialized_and_null_prefix() {
    let out = compile_and_run(r#"<?php
class IssetLeaf { public int $value = 3; }
class IssetHolder { public ?IssetLeaf $child; }
$root = new IssetHolder();
echo isset($root->child->value) ? 'bad' : 'uninitialized';
$root->child = null;
echo isset($root->child->value) ? ':bad' : ':null';
$root->child = new IssetLeaf();
echo isset($root->child->value) ? ':ready' : ':bad';
"#);
    assert_eq!(out, "uninitialized:null:ready");
}

#[test]
fn test_native_isset_chain_method_receiver_evaluated_and_released_once() {
    let out = compile_and_run(r#"<?php
class IssetTemporary {
    public ?int $value;
    public function __destruct() { echo 'drop:'; }
}
class IssetFactory {
    public function next(): IssetTemporary { echo 'call:'; return new IssetTemporary(); }
}
$factory = new IssetFactory();
echo isset($factory->next()->value) ? 'bad' : 'empty';
"#);
    assert_eq!(out, "call:drop:empty");
}

#[test]
fn test_native_isset_chain_uses_magic_getter_result_not_private_slot_type() {
    let out = compile_and_run(r#"<?php
class IssetHidden {}
class IssetVisible { public int $value = 3; }
class IssetMagicHolder {
    private IssetHidden $child;
    public function __isset(string $name): bool { echo 'has:'; return true; }
    public function __get(string $name): IssetVisible { echo 'get:'; return new IssetVisible(); }
}
$holder = new IssetMagicHolder();
echo isset($holder->child->value) ? 'ready' : 'bad';
"#);
    assert_eq!(out, "has:get:ready");
}
