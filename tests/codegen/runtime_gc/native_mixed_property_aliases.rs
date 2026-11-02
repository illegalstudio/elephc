//! Purpose:
//! Keeps native Mixed property cells shared with escaping eval aliases.
//!
//! Called from:
//! - The executable runtime ownership suite in CI.
//!
//! Key details:
//! - Native declarations stay outside eval, including an inherited property and destructor.
//! - Aliases outlive both objects without retaining their receivers.

use crate::support::*;

/// Introspection accepts boxed ordinals while keeping borrowed object and value owners intact.
#[test]
fn test_native_object_property_introspection_boxed_indices() {
    let source = r#"<?php
class BoxedIndexNativeOwner { public int $number = 7; public string $text = 'text'; }
function inspectNativeBoxedIndex(object $owner, mixed $index): void {
    echo __elephc_object_prop_name($owner, $index), ':',
        __elephc_object_prop_value($owner, $index), '|';
}
$owner = new BoxedIndexNativeOwner();
inspectNativeBoxedIndex($owner, 0);
inspectNativeBoxedIndex($owner, 1);
unset($owner);
"#;
    let (output, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    let expected = "number:7|text:text|";
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&output), &[expected]);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Managed promoted Mixed cells retain array-entry aliases after their source containers retire.
#[test]
fn test_promoted_mixed_property_retains_managed_element_bundle() {
    let source = r#"<?php
class ManagedElementPromotion {
    public function __construct(public mixed &$value) {}
}
function makeManagedElementPromotion(bool $viaAlias): ManagedElementPromotion {
    $items = ['key' => 'before'];
    if ($viaAlias) {
        $alias =& $items['key'];
        $holder = new ManagedElementPromotion($alias);
        unset($alias);
    } else {
        $holder = new ManagedElementPromotion($items['key']);
    }
    $items['key'] = 'after';
    $items = [];
    return $holder;
}
$direct = makeManagedElementPromotion(false);
$aliased = makeManagedElementPromotion(true);
echo $direct->value, ':', $aliased->value, '|';
$direct->value = ['updated'];
echo json_encode($direct->value), '|';
unset($direct, $aliased);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "after:after|[\"updated\"]|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Native aliases already make object projections shared before eval exports any PHP wrapper.
#[test]
fn test_native_mixed_projection_without_eval_bundle() {
    let source = r#"<?php
class PureNativeMixedProjection {
    public mixed $value = ['stage' => 'initial'];
    public mixed $nullable = null;
    public function &valueRef(): mixed { return $this->value; }
    public function &nullableRef(): mixed { return $this->nullable; }
}
function exercisePureNativeMixedProjection(): void {
    $object = new PureNativeMixedProjection();
    $alias =& $object->valueRef();
    $nullable =& $object->nullableRef();
    $vars = get_object_vars($object);
    $vars['value'] = ['stage' => 'vars'];
    $vars['nullable'] = 'present';
    echo json_encode($alias), ':', $nullable, '|';
    $cast = (array)$object;
    $cast['value'] = ['stage' => 'cast'];
    echo json_encode($object->value), '|';
    $copy = clone $object;
    $copy->value = ['stage' => 'clone'];
    echo json_encode($alias), ':', json_encode($object->value), '|';
    unset($copy, $vars, $cast, $alias, $nullable);
    $separate = get_object_vars($object);
    $separate['value'] = ['stage' => 'separate'];
    echo json_encode($object->value), '|';
    unset($object, $separate);
}
exercisePureNativeMixedProjection();
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "{\"stage\":\"vars\"}:present|{\"stage\":\"cast\"}|{\"stage\":\"clone\"}:{\"stage\":\"clone\"}|{\"stage\":\"clone\"}|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Collector snapshot owners do not keep a singleton inner PHP reference shared during clone.
#[test]
fn test_native_mixed_reference_clone_ignores_collector_wrapper_pin_bundle() {
    let source = r#"<?php
class CollectorPinnedMixedClone {
    public mixed $value = 'old';
    public mixed $cycle = null;
    public bool $run = true;
    public function __destruct() {
        if (!$this->run) { return; }
        $this->run = false;
        $copy = clone $this;
        $copy->cycle = null;
        $copy->value = 'copy';
        echo $this->value, ':', $copy->value, '|';
        unset($copy);
    }
}
function makeCollectorPinnedMixedClone(bool $run): void {
    $object = new CollectorPinnedMixedClone();
    $code = $run ? '
function touchCollectorPinnedMixedClone(&$value): void {}
touchCollectorPinnedMixedClone($object->value);
' : '';
    eval($code);
    $object->cycle = $object;
    unset($object, $code);
}
makeCollectorPinnedMixedClone($argc > 0);
gc_collect_cycles();
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "old:copy|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Mixed constructor properties retain live caller bindings across frame retirement and eval.
#[test]
fn test_promoted_mixed_constructor_owned_reference_lifetimes_bundle() {
    let source = r#"<?php
class OwnedPromotedMixed {
    public function __construct(public mixed &$value = 'default') {}
    public function assign(mixed &$value): void { $this->value = $value; }
}
class OwnedPromotedMixedChild extends OwnedPromotedMixed {}
class PrivateOwnedPromotedMixed {
    public function __construct(private mixed &$value) {}
    public function read(): mixed { return $this->value; }
    public function replace(mixed $value): void { $this->value = $value; }
}
class RawShadowOwnedPromotedMixed extends PrivateOwnedPromotedMixed {
    public string $value = 'shadow';
}
function makePrivateOwnedPromotedMixed(string $class): PrivateOwnedPromotedMixed {
    $value = 'before';
    $object = new $class($value);
    if (!($object instanceof PrivateOwnedPromotedMixed)) { throw new TypeError('unexpected private factory class'); }
    $value = 'after';
    return $object;
}
class ThrowingPromotedMixed {
    public function __construct(public mixed &$value) { throw new Exception('constructor'); }
}
function makeOwnedPromotedMixed(): OwnedPromotedMixedChild {
    $value = ['stage' => 'frame'];
    return new OwnedPromotedMixedChild($value);
}
function exerciseOwnedPromotedMixed(bool $run): void {
    $value = ['stage' => 'initial'];
    $object = new OwnedPromotedMixedChild($value);
    $copy = clone $object;
    $value = ['stage' => 'caller'];
    echo json_encode($object->value), ':', json_encode($copy->value), '|';
    $incoming = ['stage' => 'method'];
    $object->assign($incoming);
    $incoming = ['stage' => 'separate'];
    echo json_encode($value), '|';
    $code = $run ? '
function captureOwnedPromotedMixed(&$value): callable {
    return function (mixed $next) use (&$value): mixed { $value = $next; return $value; };
}
$escaped = captureOwnedPromotedMixed($object->value);
' : '';
    eval($code);
    unset($object->value);
    $object->value = ['stage' => 'new'];
    echo json_encode($object), ':', json_encode($copy), '|';
    unset($object, $copy);
    $code = $run ? '$escaped(["stage" => "escaped"]); unset($escaped);' : '';
    eval($code);
    echo json_encode($value), '|';
    $survivor = makeOwnedPromotedMixed();
    $survivor->value = ['stage' => 'survived'];
    echo json_encode($survivor), '|';
    $default = new OwnedPromotedMixed();
    echo $default->value, '|';
    $fresh = new OwnedPromotedMixed($unsetValue);
    echo json_encode($fresh->value), '|';
    $unsetValue = ['stage' => 'fresh'];
    echo json_encode($fresh->value), '|';
    $class = $run ? 'OwnedPromotedMixed' : 'OwnedPromotedMixedChild';
    $dynamic = new $class($value);
    $value = ['stage' => 'dynamic'];
    echo json_encode($dynamic->value), '|';
    $childClass = $run ? 'OwnedPromotedMixedChild' : 'OwnedPromotedMixed';
    $childValue = 'before';
    $dynamicChild = new $childClass($childValue);
    $childValue = 'inherited';
    echo $dynamicChild->value, '|';
    $hiddenClass = $run ? 'RawShadowOwnedPromotedMixed' : 'PrivateOwnedPromotedMixed';
    $hidden = makePrivateOwnedPromotedMixed($hiddenClass);
    $hiddenCopy = clone $hidden;
    $hidden->replace('updated');
    echo $hidden->read(), ':', $hiddenCopy->read(), ':', json_encode(get_object_vars($hidden)), '|';
    unset($hidden, $hiddenCopy, $hiddenClass);
    try { $failed = new ThrowingPromotedMixed($value); }
    catch (Exception $error) { echo $error->getMessage(), '|'; }
    $code = $run ? '
$evalNativeValue = ["stage" => "eval-new"];
$evalObject = new OwnedPromotedMixed($evalNativeValue);
$evalNativeValue = ["stage" => "eval-updated"];
echo json_encode($evalObject->value), "|";
unset($evalObject, $evalNativeValue);
' : '';
    eval($code);
    unset($value, $incoming, $survivor, $default, $fresh, $unsetValue, $dynamic, $class,
        $dynamicChild, $childClass, $childValue, $code, $error);
}
exerciseOwnedPromotedMixed($argc > 0);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "{\"stage\":\"caller\"}:{\"stage\":\"caller\"}|{\"stage\":\"method\"}|{\"value\":{\"stage\":\"new\"}}:{\"value\":{\"stage\":\"method\"}}|{\"stage\":\"escaped\"}|{\"value\":{\"stage\":\"survived\"}}|default|null|{\"stage\":\"fresh\"}|{\"stage\":\"dynamic\"}|inherited|updated:after:{\"value\":\"shadow\"}|constructor|{\"stage\":\"eval-updated\"}|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// AOT and eval writes, nested mutations, clone and unset preserve one escaping Mixed alias.
#[test]
fn test_native_mixed_property_escaping_eval_aliases_bundle() {
    let source = r#"<?php
class PersistentNativeMixedParent {
    public mixed $value = ['stage' => 'old'];
    public function &valueRef(): mixed { return $this->value; }
    public function __destruct() { echo 'object|'; }
}
class PersistentNativeMixedChild extends PersistentNativeMixedParent {}
class ImplicitNativeMixedAlias {
    public mixed $value = 'old';
    public function __destruct() { echo 'implicit|'; }
}
function mutatePersistentNativeMixed(mixed &$value): void { $value['argument'][] = 'call'; }
function exercisePersistentNativeMixed(bool $run): void {
    $object = new PersistentNativeMixedChild();
    $native =& $object->valueRef();
    $code = $run ? '
function capturePersistentNativeMixed(&$value): callable {
    return function (bool $write, mixed $next = null) use (&$value): mixed {
        if ($write) { $value = $next; }
        return $value;
    };
}
$escaped = capturePersistentNativeMixed($object->value);
' : '';
    eval($code);
    $object->value = ['stage' => 'property'];
    $code = $run ? 'echo json_encode($escaped(false)), "|";' : '';
    eval($code);
    $native = ['stage' => 'native'];
    eval($code);
    $code = $run ? '$escaped(true, ["stage" => "eval"]);' : '';
    eval($code);
    echo json_encode($native), ':', json_encode($object->value), '|';
    $object->value['nested'][] = 'child';
    mutatePersistentNativeMixed($native);
    $code = $run ? 'echo json_encode($escaped(false)), "|";' : '';
    eval($code);
    $copy = clone $object;
    unset($object->value);
    $object->value = ['stage' => 'recreated'];
    echo json_encode($object->value), '|';
    unset($object, $copy);
    $code = $run ? '$escaped(true, ["stage" => "survived"]); echo json_encode($escaped(false)), "|"; unset($escaped);' : '';
    eval($code);
    echo json_encode($native), '|';
    $implicit = new ImplicitNativeMixedAlias();
    $code = $run ? '$implicitAlias = capturePersistentNativeMixed($implicit->value);' : '';
    eval($code);
    $implicit->value = 'updated';
    unset($implicit);
    $code = $run ? 'echo $implicitAlias(false), "|"; unset($implicitAlias);' : '';
    eval($code);
    unset($native, $code);
}
exercisePersistentNativeMixed($argc > 0);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "{\"stage\":\"property\"}|{\"stage\":\"native\"}|{\"stage\":\"eval\"}:{\"stage\":\"eval\"}|{\"stage\":\"eval\",\"nested\":[\"child\"],\"argument\":[\"call\"]}|{\"stage\":\"recreated\"}|object|object|{\"stage\":\"survived\"}|{\"stage\":\"survived\"}|implicit|updated|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Physical private slots and authorized protected exports retain distinct escaping aliases.
#[test]
fn test_native_mixed_reference_hidden_slots_and_eval_scope_bundle() {
    let source = r#"<?php
class NativeHiddenMixedParent {
    private mixed $value = 'parent';
    public function readParent(): mixed { return $this->value; }
    public function writeParent(mixed $next): void { $this->value = $next; }
    public function &parentRef(): mixed { return $this->value; }
}
class NativeHiddenMixedChild extends NativeHiddenMixedParent {
    public mixed $value = 'child';
}
class NativeHiddenRawChild extends NativeHiddenMixedParent {
    public string $value = 'raw';
}
class NativeProtectedMixedParent {
    protected mixed $value = 'old';
    public function readValue(): mixed { return $this->value; }
    public function writeValue(mixed $next): void { $this->value = $next; }
    public function __destruct() { echo 'retired|'; }
}
function bindNativeMixedThroughForeignContext(mixed $object): mixed { return $object->bind(); }
function exerciseNativeMixedScopes(bool $run): void {
    $child = new NativeHiddenMixedChild();
    $raw = new NativeHiddenRawChild();
    $parentAlias =& $child->parentRef();
    $rawAlias =& $raw->parentRef();
    $parentAlias = 'first';
    $rawAlias = 'second';
    echo $child->readParent(), ':', $child->value, '|', $raw->readParent(), ':', $raw->value, '|';
    $child->writeParent('third');
    echo $parentAlias, '|';
    $copy = clone $child;
    $copy->writeParent('shared');
    echo $parentAlias, ':', $copy->value, '|';
    unset($child, $raw, $copy, $parentAlias, $rawAlias);
    $code = $run ? '
function captureNativeScopedMixed(&$value): callable {
    return function (bool $write, mixed $next = null) use (&$value): mixed {
        if ($write) { $value = $next; }
        return $value;
    };
}
class EvalProtectedMixedChild extends NativeProtectedMixedParent {
    public function bind(): callable { return captureNativeScopedMixed($this->value); }
}
$protected = new EvalProtectedMixedChild();
$escapedProtected = $protected->bind();
$foreignProtected = bindNativeMixedThroughForeignContext($protected);
$protected->writeValue("native");
echo $escapedProtected(false), "|", $foreignProtected(false), "|";
$escapedProtected(true, "eval");
echo $protected->readValue(), "|", $foreignProtected(false), "|";
try { captureNativeScopedMixed($protected->value); } catch (Error $error) { echo "protected|"; }
unset($protected);
$escapedProtected(true, "survived");
echo $escapedProtected(false), "|", $foreignProtected(false), "|";
unset($escapedProtected, $foreignProtected);
' : '';
    eval($code);
    unset($code);
}
exerciseNativeMixedScopes($argc > 0);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "first:child|second:raw|third|shared:child|native|native|eval|eval|protected|retired|survived|survived|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Serialization and hydration preserve native cells, persistent wrappers and low-word sentinels.
#[test]
fn test_native_reference_property_serialization_bundle() {
    let source = r#"<?php
class SerializedNativeMixed {
    public mixed $value = ['key' => 'old'];
    public mixed $missing;
    public function &valueRef(): mixed { return $this->value; }
}
class SerializedNativeRaw {
    public string $text = 'old';
    public int $number = 1;
    public function &textRef(): string { return $this->text; }
    public function &numberRef(): int { return $this->number; }
}
class SerializedNativeSleep {
    public mixed $value = 'sleep';
    public mixed $missing;
    public function __sleep(): array { return ['value', 'missing']; }
}
function exerciseNativeReferenceSerialization(bool $run): void {
    $object = new SerializedNativeMixed();
    $alias =& $object->valueRef();
    $alias = ['key' => 'changed'];
    $restored = unserialize(serialize($object));
    echo json_encode($restored), '|';
    $restored->value = ['key' => 'hydrated'];
    echo json_encode($restored->value), '|';
    $raw = new SerializedNativeRaw();
    $text =& $raw->textRef();
    $number =& $raw->numberRef();
    $text = 'text';
    $number = 7;
    $rawCopy = unserialize(serialize($raw));
    echo $rawCopy->text, ':', $rawCopy->number, '|';
    $sleep = new SerializedNativeSleep();
    $sleepCopy = unserialize(serialize($sleep));
    echo json_encode($sleepCopy), '|';
    unset($object->value);
    $unsetCopy = unserialize(serialize($object));
    echo json_encode($unsetCopy), '|';
    $code = $run ? 'echo "eval|";' : '';
    eval($code);
    unset($object, $restored, $raw, $rawCopy, $sleep, $sleepCopy, $unsetCopy, $alias, $text, $number, $code);
}
exerciseNativeReferenceSerialization($argc > 0);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "{\"value\":{\"key\":\"changed\"}}|{\"key\":\"hydrated\"}|text:7|{\"value\":\"sleep\"}|{\"value\":{\"key\":\"old\"}}|eval|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Native static symbols and escaped eval references publish into one persistent Mixed wrapper.
#[test]
fn test_native_mixed_static_property_escaping_eval_aliases_bundle() {
    let source = r#"<?php
class PersistentNativeMixedStatic {
    public static mixed $value = ['stage' => 'old'];
    protected static mixed $secret = 'hidden';
    public static function readSecret(): mixed { return self::$secret; }
    public static function writeSecret(mixed $value): void { self::$secret = $value; }
}
class PersistentNativeMixedStaticShadow extends PersistentNativeMixedStatic {
    public static mixed $value = ['stage' => 'shadow'];
}
class PersistentNativeStaticRetired {
    public function __destruct() { echo 'static-retired|'; }
}
function exerciseNativeMixedStatics(bool $run): void {
    $native =& PersistentNativeMixedStatic::$value;
    $copy = PersistentNativeMixedStatic::$value;
    $code = $run ? '
function captureNativeStaticMixed(&$value): callable {
    return function (bool $write, mixed $next = null) use (&$value): mixed {
        if ($write) { $value = $next; }
        return $value;
    };
}
$escapedStatic = captureNativeStaticMixed(PersistentNativeMixedStatic::$value);
class EvalNativeMixedStaticChild extends PersistentNativeMixedStatic {
    public static function bindSecret(): callable { return captureNativeStaticMixed(parent::$secret); }
    public static function directSecret(): void {
        echo (int)isset(PersistentNativeMixedStatic::$secret), ":", PersistentNativeMixedStatic::$secret, "|";
        PersistentNativeMixedStatic::$secret = "direct";
        echo PersistentNativeMixedStatic::$secret, "|";
        PersistentNativeMixedStatic::$secret = "native-secret";
    }
}
$escapedSecret = EvalNativeMixedStaticChild::bindSecret();
' : '';
    eval($code);
    PersistentNativeMixedStatic::$value = ['stage' => 'native'];
    PersistentNativeMixedStatic::writeSecret('native-secret');
    $code = $run ? 'EvalNativeMixedStaticChild::directSecret();
echo json_encode($escapedStatic(false)), ":", $escapedSecret(false), "|";
$escapedStatic(true, ["stage" => "eval"]); $escapedSecret(true, "eval-secret");' : '';
    eval($code);
    echo json_encode($native), ':', PersistentNativeMixedStatic::readSecret(), '|';
    $native['nested'][] = 'alias';
    PersistentNativeMixedStatic::$value['property'][] = 'native';
    $code = $run ? 'echo json_encode($escapedStatic(false)), "|";
try { unset(PersistentNativeMixedStatic::$value); } catch (Error $error) { echo "static-unset|"; }
' : '';
    eval($code);
    echo json_encode($copy), ':', json_encode(PersistentNativeMixedStaticShadow::$value), '|';
    $code = $run ? 'unset($escapedStatic, $escapedSecret);' : '';
    eval($code);
    unset($native, $copy, $code);
    PersistentNativeMixedStatic::$value = new PersistentNativeStaticRetired();
}
exerciseNativeMixedStatics($argc > 0);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "1:native-secret|direct|{\"stage\":\"native\"}:native-secret|{\"stage\":\"eval\"}:eval-secret|{\"stage\":\"eval\",\"nested\":[\"alias\"],\"property\":[\"native\"]}|static-unset|{\"stage\":\"old\"}:{\"stage\":\"shadow\"}|static-retired|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Committed static stores relinquish operand pins before an old destructor retires the replacement.
#[test]
fn test_native_mixed_static_reference_reentrant_throwing_store_bundle() {
    let source = r#"<?php
class NativeStaticTransferTarget { public static mixed $value; }
class NativeStaticTransferNew {
    public function __destruct() { echo 'new-drop|'; }
}
class NativeStaticTransferOld {
    public function __destruct() {
        echo 'old-enter|';
        NativeStaticTransferTarget::$value = 'third';
        echo 'old-after|';
        throw new Error('retired');
    }
}
function freshNativeStaticTransfer(): mixed { return new NativeStaticTransferNew(); }
function exerciseNativeStaticTransfer(bool $run): void {
    $code = $run ? '
function captureNativeStaticTransfer(&$value): callable {
    return function () use (&$value): mixed { return $value; };
}
$escaped = captureNativeStaticTransfer(NativeStaticTransferTarget::$value);
' : '';
    NativeStaticTransferTarget::$value = 'initial';
    eval($code);
    NativeStaticTransferTarget::$value = new NativeStaticTransferOld();
    try { NativeStaticTransferTarget::$value = freshNativeStaticTransfer(); }
    catch (Error $error) { echo 'catch|'; }
    $code = $run ? 'echo $escaped(), "|"; unset($escaped);' : '';
    eval($code);
    unset($code);
}
exerciseNativeStaticTransfer($argc > 0);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "old-enter|new-drop|old-after|catch|third|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Value walkers follow native cells while array projections preserve real PHP references and COW.
#[test]
fn test_native_reference_property_renderers_and_projection_bundle() {
    let source = r#"<?php
class RenderedNativeReference {
    public mixed $value = ['key' => 'value'];
    public mixed $missing;
    public string $text = 'text';
    public int $number = 7;
    public function &textRef(): string { return $this->text; }
    public function &numberRef(): int { return $this->number; }
}
function exerciseRenderedNativeReferences(bool $run): void {
    $object = new RenderedNativeReference();
    $text =& $object->textRef();
    $number =& $object->numberRef();
    $nativeValue =& $object->value;
    $code = $run ? '
function captureRenderedNativeReference(&$value): callable {
    return function () use (&$value): mixed { return $value; };
}
$escapedRendered = captureRenderedNativeReference($object->value);
' : '';
    eval($code);
    ob_start(); var_dump($object); $dump = ob_get_clean();
    $printed = print_r($object, true);
    $exported = var_export($object, true);
    if (!str_contains($exported, "'text'")) {
        echo '[export:', $exported, ';count:', __elephc_object_prop_count($object),
            ';first:', __elephc_object_prop_name($object, 0), ']';
    }
    echo (int)str_contains($dump, 'uninitialized(mixed)'),
        (int)str_contains($dump, 'string(4) "text"'),
        (int)str_contains($dump, 'int(7)'),
        (int)!str_contains($printed, '[missing]'),
        (int)!str_contains($exported, "'missing'"),
        (int)str_contains($exported, "'text'"), '|';
    $vars = get_object_vars($object);
    echo json_encode($vars), '|';
    $vars['value'] = ['key' => 'vars'];
    echo json_encode($object->value), '|';
    $replaced = array_replace($vars, ['value' => ['key' => 'replacement']]);
    echo json_encode($object->value), ':', json_encode($replaced['value']), '|';
    $cast = (array)$object;
    $cast['value'] = ['key' => 'cast'];
    $code = $run ? 'echo json_encode($escapedRendered()), "|"; unset($escapedRendered);' : '';
    eval($code);
    unset($vars, $cast, $replaced);
    $nativeVars = get_object_vars($object);
    $nativeVars['value'] = ['key' => 'native-only'];
    echo json_encode($nativeValue), '|';
    $peer = new RenderedNativeReference();
    $peer->value = ['key' => 'native-only'];
    echo (int)($object == $peer), '|';
    $plain = new RenderedNativeReference();
    $temporary =& $plain->value;
    unset($temporary);
    $plainVars = get_object_vars($plain);
    $plainVars['value']['key'] = 'copy';
    echo json_encode($plain->value), '|';
    unset($object, $peer, $plain, $nativeVars, $nativeValue, $plainVars, $text, $number, $dump, $printed, $exported, $code);
}
exerciseRenderedNativeReferences($argc > 0);
"#;
    let (output, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    let expected = "111111|{\"value\":{\"key\":\"value\"},\"text\":\"text\",\"number\":7}|{\"key\":\"vars\"}|{\"key\":\"vars\"}:{\"key\":\"replacement\"}|{\"key\":\"cast\"}|{\"key\":\"native-only\"}|1|{\"key\":\"value\"}|";
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&output), &[expected]);
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}
