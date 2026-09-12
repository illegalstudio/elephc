//! Purpose:
//! Checks exception-safe final-owner cleanup across native heap containers and callable captures.
//!
//! Called from:
//! - The runtime GC codegen suite on each executable target.
//!
//! Key details:
//! - Destructors throw after allocating owned fields, so output alone cannot hide interrupted cleanup.
//! - The GC protection flag is checked separately from a later explicit collection.

use crate::support::*;

/// Repeated dynamic dispatch preserves receiver ownership and independently owns copied selector names.
#[test]
fn test_core_dynamic_method_loop_preserves_receiver_owner() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class KeptDynamicReceiver {
    public static int $destroyed = 0;
    public function ping(): void { echo "a"; }
    public function pong(): void { echo "b"; }
    public function __destruct() { self::$destroyed++; }
}
$receiver = new KeptDynamicReceiver();
foreach (["ping", "pong", "ping"] as $method) { $receiver->$method(); }
echo ":", KeptDynamicReceiver::$destroyed;
unset($receiver);
echo ":", KeptDynamicReceiver::$destroyed;
"#);
    assert!(out.success, "stdout={:?}\nstderr={}\nGenerated user assembly:\n{}", out.stdout, out.stderr, asm);
    assert_eq!(out.stdout, "aba:0:1", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A typed static property retains local objects independently across scope exit and widening.
#[test]
fn test_core_static_property_keeps_concrete_and_widened_local_objects_alive() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class PublishedLocalValue {
    public int $number = 17;
    public static int $released = 0;
    public function __destruct() { self::$released++; }
}
class PublishedLocalHolder { public static PublishedLocalValue $value; }
function publishConcreteLocal(): void {
    $value = new PublishedLocalValue();
    PublishedLocalHolder::$value = $value;
}
function publishWidenedLocal(): void {
    $value = new PublishedLocalValue();
    PublishedLocalHolder::$value = $value;
    $value = 42;
    echo $value, ":";
}
publishConcreteLocal();
echo PublishedLocalHolder::$value->number, ":", PublishedLocalValue::$released, "|";
publishWidenedLocal();
echo PublishedLocalHolder::$value->number, ":", PublishedLocalValue::$released;
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "17:0|42:17:1", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A caught element destructor observes a completed removal, with surviving hash entries intact.
#[test]
fn test_core_hash_unset_commits_removal_before_a_throwing_destructor() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class RetiredHashElement {
    public static int $calls = 0;
    public function __destruct() { self::$calls++; throw new RuntimeException("removed"); }
}
$table = ["drop" => new RetiredHashElement(), "keep" => 41];
try { unset($table["drop"]); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
echo count($table), ":", $table["keep"], ":", RetiredHashElement::$calls, "|";
echo array_key_exists("drop", $table) ? "present" : "absent";
unset($table["drop"]);
echo ":", RetiredHashElement::$calls;
unset($table);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "removed|1:41:1|absent:1", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Reentrant destructors see the removed property as absent and can reinstall it before throwing.
#[test]
fn test_core_dynamic_property_unset_preserves_reentrant_replacement_after_throw() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReentrantUnsetValue {
    public static stdClass $owner;
    public function __destruct() {
        $owner = self::$owner;
        echo isset($owner->value) ? "present|" : "absent|";
        $owner->value = 17;
        $owner->next = "fresh";
        throw new RuntimeException("replaced");
    }
}
$owner = new stdClass();
ReentrantUnsetValue::$owner = $owner;
$owner->value = new ReentrantUnsetValue();
try { unset($owner->value); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
echo $owner->value, ":", $owner->next;
unset($owner->value, $owner->next);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "absent|replaced|17:fresh", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Publishing the COW table before removal leaves the snapshot and its element owner unchanged.
#[test]
fn test_core_hash_unset_preserves_shared_snapshot_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class SharedUnsetValue {
    public static int $calls = 0;
    public function __destruct() { self::$calls++; }
}
$table = ["drop" => new SharedUnsetValue(), "keep" => 41];
$snapshot = $table;
unset($table["drop"]);
echo count($table), ":", count($snapshot), ":", SharedUnsetValue::$calls, "|";
unset($snapshot["drop"]);
echo SharedUnsetValue::$calls, ":", $snapshot["keep"], ":", $table["keep"];
unset($snapshot, $table);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:2:0|1:41:41", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A destructor escaping an explicit unset cannot make frame unwinding release its retired owner again.
#[test]
fn test_core_unset_throwing_local_retires_owner_before_unwinding() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class RetiredLocalDestructor {
    public static int $calls = 0;
    public function __destruct() { self::$calls++; throw new RuntimeException("retired"); }
}
function retireThrowingLocal(): void {
    $value = new RetiredLocalDestructor();
    unset($value);
}
try { retireThrowingLocal(); }
catch (RuntimeException $error) {
    echo $error->getMessage(), ":", RetiredLocalDestructor::$calls;
    unset($error);
}
echo "|after";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "retired:1|after", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Catching in a PHP function retires child frames without releasing the surviving function's owners.
#[test]
fn test_core_native_unwind_preserves_the_catching_frame_owners() {
    let out = compile_and_run(r#"<?php
class SurvivingCatchOwner {
    public static int $destroyed = 0;
    public int $value;
    public function __construct(int $value) { $this->value = $value; }
    public function __destruct() { self::$destroyed++; }
}
function abortCatchChild(Exception $error): void {
    $child = new SurvivingCatchOwner(7);
    echo $child->value, ":";
    throw $error;
}
function preserveCatchOwner(Exception $error): void {
    $owner = new SurvivingCatchOwner(42);
    for ($i = 0; $i < 3; $i++) {
        try { abortCatchChild($error); }
        catch (Exception $caught) {
            echo $owner->value, ":", SurvivingCatchOwner::$destroyed, "|";
            unset($caught);
        }
    }
    echo $owner->value, "|";
}
$error = new Exception("shared");
preserveCatchOwner($error);
unset($error);
echo SurvivingCatchOwner::$destroyed;
"#);
    assert_eq!(out, "7:42:1|7:42:2|7:42:3|42|4");
}

/// A throw to the caller releases every owned local in an ordinary native executable frame.
#[test]
fn test_core_native_unwind_releases_owned_frame_locals() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NativeUnwindOwner {
    public static int $destroyed = 0;
    public int $value = 7;
    public function __destruct() { self::$destroyed++; }
}
function abortOwnedNativeFrame(Exception $exception): void {
    $first = new NativeUnwindOwner();
    $second = new NativeUnwindOwner();
    echo $first->value + $second->value, ":";
    throw $exception;
}
$exception = new Exception("original");
for ($i = 0; $i < 3; $i++) {
    try { abortOwnedNativeFrame($exception); }
    catch (Exception $caught) { echo $caught->getMessage(), "|"; unset($caught); }
}
unset($exception);
echo NativeUnwindOwner::$destroyed;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "14:original|14:original|14:original|6", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Destructor throws during unwinding preserve the original exception and finish sibling owners.
#[test]
fn test_core_native_unwind_finishes_throwing_sibling_locals() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NativeUnwindFirst {
    public function __destruct() { echo "first|"; throw new RuntimeException("first"); }
}
class NativeUnwindSecond {
    public function __destruct() { echo "second|"; throw new RuntimeException("second"); }
}
function abortThrowingNativeFrame(): void {
    $first = new NativeUnwindFirst();
    $second = new NativeUnwindSecond();
    echo get_class($first), ":", get_class($second), "|";
    throw new Exception("original");
}
try { abortThrowingNativeFrame(); }
catch (Throwable $caught) {
    echo $caught->getMessage(), ":", $caught->getPrevious()->getMessage(), ":";
    echo $caught->getPrevious()->getPrevious()->getMessage();
    unset($caught);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "NativeUnwindFirst:NativeUnwindSecond|first|second|second:first:original", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing last-owner destructor must not leak the promoted local reference cell.
#[test]
fn test_core_throwing_reference_payload_retires_its_local_cell() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReferenceCellThrowingValue {
    public int $value = 7;
    public static RuntimeException $error;
    public function __destruct() { throw self::$error; }
}
function retireThrowingReferenceCell(): void {
    $value = new ReferenceCellThrowingValue();
    $alias =& $value;
    echo $alias->value, ":";
}
ReferenceCellThrowingValue::$error = new RuntimeException("reference");
for ($i = 0; $i < 3; $i++) {
    try { retireThrowingReferenceCell(); }
    catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "7:reference|7:reference|7:reference|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Every container finishes sibling cleanup and preserves both exceptions before freeing its storage.
#[test]
fn test_core_throwing_destructors_release_objects_arrays_hashes_and_callable_captures() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class ThrowingCleanupChild {
    public string $message;
    public string $buffer;
    public function __construct(string $message) {
        $this->message = $message;
        $this->buffer = str_repeat("x", 48);
    }
    public function __destruct() { throw new Exception($this->message); }
}
class ThrowingCleanupParent {
    public ThrowingCleanupChild $first;
    public ThrowingCleanupChild $second;
    public function __construct() {
        $this->first = new ThrowingCleanupChild("first");
        $this->second = new ThrowingCleanupChild("second");
    }
}
function releaseCleanupObject(): void {
    $value = new ThrowingCleanupParent();
    unset($value);
}
function releaseCleanupArray(): void {
    $value = [new ThrowingCleanupChild("first"), new ThrowingCleanupChild("second")];
    unset($value);
}
function releaseCleanupHash(): void {
    $value = ["first" => new ThrowingCleanupChild("first"), "second" => new ThrowingCleanupChild("second")];
    unset($value);
}
function releaseCleanupCallable(): void {
    $first = new ThrowingCleanupChild("first");
    $second = new ThrowingCleanupChild("second");
    $value = static function () use ($first, $second): void {};
    unset($first, $second);
    unset($value);
}
function describeCleanupChain(Throwable $error): void {
    $previous = $error->getPrevious();
    echo $previous !== null && $previous->getPrevious() === null ? "chain|" : "bad|";
}
try { releaseCleanupObject(); } catch (Throwable $error) { describeCleanupChain($error); unset($error); }
try { releaseCleanupArray(); } catch (Throwable $error) { describeCleanupChain($error); unset($error); }
try { releaseCleanupHash(); } catch (Throwable $error) { describeCleanupChain($error); unset($error); }
try { releaseCleanupCallable(); } catch (Throwable $error) { describeCleanupChain($error); unset($error); }
$status = gc_status();
echo $status["protected"] ? "suppressed|" : "unprotected|";
unset($status);
class CleanupLaterCycle { public mixed $self = null; }
gc_disable();
$cycle = new CleanupLaterCycle();
$cycle->self = $cycle;
unset($cycle);
echo gc_collect_cycles() > 0 ? "collected" : "suppressed";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "chain|chain|chain|chain|unprotected|collected", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nGenerated user assembly:\n{}", out.stderr, asm);
}
