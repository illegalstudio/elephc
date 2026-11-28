<?php
class PropertyKeyOwner {
    public array $items = ['base' => 1];
    public static array $shared = ['base' => 1];
}
function propertyKeyWrite(mixed $key): void {
    $owner = new PropertyKeyOwner();
    $snapshot = $owner->items;
    try { $owner->items[$key] = 9; }
    catch (TypeError $error) { echo 'K|'; unset($error); }
    echo json_encode($owner->items), '/', json_encode($snapshot), '|';
}
function staticKeyWrite(mixed $key): void {
    PropertyKeyOwner::$shared = ['base' => 1];
    $snapshot = PropertyKeyOwner::$shared;
    try { PropertyKeyOwner::$shared[$key] = 9; }
    catch (TypeError $error) { echo 'K|'; unset($error); }
    echo json_encode(PropertyKeyOwner::$shared), '/', json_encode($snapshot), '|';
}
function nestedKeyWrite(mixed $key): void {
    $parent = [['base' => 1]];
    $snapshot = $parent;
    try { $parent[0][$key] = 9; }
    catch (TypeError $error) { echo 'K|'; unset($error); }
    echo json_encode($parent), '/', json_encode($snapshot), '|';
}
set_error_handler(static function (int $level, string $message): bool { return true; });
propertyKeyWrite(true); propertyKeyWrite(0.5); propertyKeyWrite(null); propertyKeyWrite('key');
staticKeyWrite(true); staticKeyWrite(0.5); staticKeyWrite(null); staticKeyWrite('key');
nestedKeyWrite(true); nestedKeyWrite(0.5); nestedKeyWrite(null); nestedKeyWrite('key');
restore_error_handler();
class PropertyKeyPayload {
    public function __destruct() { echo 'D'; gc_collect_cycles(); }
}
function freshPropertyKey(): mixed { return true; }
function freshPropertyValue(): mixed { echo 'V'; return new PropertyKeyPayload(); }
function temporaryPropertyWrite(): void {
    $owner = new PropertyKeyOwner();
    try { $owner->items[freshPropertyKey()] = freshPropertyValue(); }
    catch (TypeError $error) { echo 'K'; unset($error); }
    echo '|';
}
function temporaryStaticWrite(): void {
    PropertyKeyOwner::$shared = ['base' => 1];
    try { PropertyKeyOwner::$shared[freshPropertyKey()] = freshPropertyValue(); }
    catch (TypeError $error) { echo 'K'; unset($error); }
    echo '|';
    PropertyKeyOwner::$shared = ['base' => 1];
}
temporaryPropertyWrite(); echo '|'; temporaryStaticWrite(); echo '|';
class WrappedOffsetTarget implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetSet(mixed $key, mixed $value): void { echo gettype($key), ':', $value, '|'; }
    public function offsetUnset(mixed $key): void {}
}
class WrappedOffsetHolder {
    public mixed $child = null;
    public static mixed $shared = null;
}
function wrappedOffsetWrites(mixed $target): void {
    $holder = new WrappedOffsetHolder(); $holder->child = $target;
    WrappedOffsetHolder::$shared = $target;
    $holder->child[true] = 3;
    WrappedOffsetHolder::$shared[true] = 4;
    $parents = [$target]; $parents[0][true] = 5;
    WrappedOffsetHolder::$shared = null;
}
wrappedOffsetWrites(new WrappedOffsetTarget());
class DiscardingWrappedOffsetTarget extends WrappedOffsetTarget {
    public function offsetSet(mixed $key, mixed $value): void { echo 'O'; }
}
function ownedStaticWrappedWrite(): void {
    WrappedOffsetHolder::$shared = new DiscardingWrappedOffsetTarget();
    WrappedOffsetHolder::$shared[true] = new PropertyKeyPayload();
    echo '|';
    WrappedOffsetHolder::$shared = null;
}
ownedStaticWrappedWrite();
class ThrowingDiscardingOffsetTarget extends WrappedOffsetTarget {
    public function offsetSet(mixed $key, mixed $value): void { echo 'T'; throw new RuntimeException('setter'); }
}
function throwingOwnedStaticWrappedWrite(): void {
    WrappedOffsetHolder::$shared = new ThrowingDiscardingOffsetTarget();
    $value = new PropertyKeyPayload();
    try { WrappedOffsetHolder::$shared[true] = $value; }
    catch (RuntimeException $error) { echo 'C'; unset($error); }
    unset($value); echo '|';
    WrappedOffsetHolder::$shared = null;
}
function resultOwnedStaticWrappedWrite(): void {
    WrappedOffsetHolder::$shared = new DiscardingWrappedOffsetTarget();
    $result = (WrappedOffsetHolder::$shared[true] = new PropertyKeyPayload());
    echo gettype($result), '|'; unset($result); echo '|';
    WrappedOffsetHolder::$shared = null;
}
throwingOwnedStaticWrappedWrite(); resultOwnedStaticWrappedWrite();
class WrappedOwnedKey {
    public function __destruct() { echo 'Q'; gc_collect_cycles(); }
}
function ownedStaticWrappedKey(): void {
    WrappedOffsetHolder::$shared = new DiscardingWrappedOffsetTarget();
    WrappedOffsetHolder::$shared[new WrappedOwnedKey()] = 9;
    echo '|';
    WrappedOffsetHolder::$shared = null;
}
ownedStaticWrappedKey();
function showPropertyKeyKinds(array $items): void {
    foreach ($items as $key => $value) { echo gettype($key), ':', $key, ':', $value, '|'; }
}
function exactPropertyKeyKinds(): void {
    $owner = new PropertyKeyOwner();
    $owner->items['0'] = 2; $owner->items[0] = 3; $owner->items['01'] = 4;
    showPropertyKeyKinds($owner->items);
    PropertyKeyOwner::$shared = ['base' => 1];
    PropertyKeyOwner::$shared['0'] = 2; PropertyKeyOwner::$shared[0] = 3; PropertyKeyOwner::$shared['01'] = 4;
    showPropertyKeyKinds(PropertyKeyOwner::$shared);
    $parents = [['base' => 1]];
    $parents[0]['0'] = 2; $parents[0][0] = 3; $parents[0]['01'] = 4;
    showPropertyKeyKinds($parents[0]);
}
exactPropertyKeyKinds();
