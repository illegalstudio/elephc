<?php
class RemovalKeyOwner {
    public array $items = ['base' => 1, 0 => 2, '' => 3, '01' => 4];
    public static array $shared = ['base' => 1, 0 => 2, '' => 3, '01' => 4];
}
function removalSnapshot(array $items, array $snapshot): void {
    echo json_encode($items), '/', json_encode($snapshot), '|';
}
function removeHashKey(mixed $key): void {
    $items = ['base' => 1, 0 => 2, '' => 3, '01' => 4]; $snapshot = $items;
    try { unset($items[$key]); } catch (TypeError $error) { echo 'K|'; unset($error); }
    removalSnapshot($items, $snapshot);
}
function removePackedKey(mixed $key): void {
    $items = [11, 22]; $snapshot = $items;
    try { unset($items[$key]); } catch (TypeError $error) { echo 'K|'; unset($error); }
    removalSnapshot($items, $snapshot);
}
function removeBoxedKey(array $items, mixed $key): void {
    $snapshot = $items;
    try { unset($items[$key]); } catch (TypeError $error) { echo 'K|'; unset($error); }
    removalSnapshot($items, $snapshot);
}
function removePropertyKey(mixed $key): void {
    $owner = new RemovalKeyOwner(); $snapshot = $owner->items;
    try { unset($owner->items[$key]); } catch (TypeError $error) { echo 'K|'; unset($error); }
    removalSnapshot($owner->items, $snapshot);
}
function removeStaticKey(mixed $key): void {
    RemovalKeyOwner::$shared = ['base' => 1, 0 => 2, '' => 3, '01' => 4];
    $snapshot = RemovalKeyOwner::$shared;
    try { unset(RemovalKeyOwner::$shared[$key]); } catch (TypeError $error) { echo 'K|'; unset($error); }
    removalSnapshot(RemovalKeyOwner::$shared, $snapshot);
}
set_error_handler(static function (int $level, string $message): bool { return true; });
removeHashKey(false); removeHashKey(0.5); removeHashKey(null); removeHashKey('base'); removeHashKey([]);
removePackedKey(true); removePackedKey(0.5); removePackedKey(null); removePackedKey('0');
removeBoxedKey([11, 22], true); removeBoxedKey([11, 22], '0');
removePropertyKey(false); removePropertyKey(0.5); removePropertyKey(null); removePropertyKey('base');
removeStaticKey(false); removeStaticKey(0.5); removeStaticKey(null); removeStaticKey('base');
restore_error_handler();
class RemovalOffsetTarget implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void { echo 'U:', gettype($key), '|'; }
}
class RemovalOffsetHolder { public mixed $child = null; }
function removeObjectKey(mixed $key): void {
    $owner = new RemovalOffsetHolder(); $owner->child = new RemovalOffsetTarget();
    unset($owner->child[$key]);
}
class RemovalOwnedKey { public function __destruct() { echo 'Q'; gc_collect_cycles(); } }
removeObjectKey(true); removeObjectKey(0.5); removeObjectKey(null); removeObjectKey([]); removeObjectKey(new RemovalOwnedKey()); echo '|';
function removeOtherParent(mixed &$parent, mixed $key): void {
    set_error_handler(static function (int $level, string $message): bool { echo 'W:', $level, '|'; return true; });
    try { unset($parent[$key]); } catch (Error $error) { echo 'E:', get_class($error), '|'; unset($error); }
    restore_error_handler(); echo gettype($parent), ':', json_encode($parent), '|';
}
$falseParent = false; removeOtherParent($falseParent, true); removeOtherParent($falseParent, []);
$nullParent = null; removeOtherParent($nullParent, true); removeOtherParent($nullParent, []);
$trueParent = true; removeOtherParent($trueParent, true);
function removalKeyKinds(array $items): void {
    foreach ($items as $key => $value) { echo gettype($key), ':', $key, ':', $value, '|'; }
}
function removeExactKeyKinds(): void {
    $items = ['0' => 10, 0 => 20, '01' => 30]; unset($items['0']); removalKeyKinds($items);
    $items2 = ['0' => 10, 0 => 20, '01' => 30]; unset($items2[0]); removalKeyKinds($items2);
    $owner = new RemovalKeyOwner(); $owner->items = ['0' => 10, 0 => 20, '01' => 30];
    unset($owner->items['0']); removalKeyKinds($owner->items);
    RemovalKeyOwner::$shared = ['0' => 10, 0 => 20, '01' => 30];
    unset(RemovalKeyOwner::$shared[0]); removalKeyKinds(RemovalKeyOwner::$shared);
}
removeExactKeyKinds();
class ReadonlyRemovalHolder {
    public readonly mixed $child;
    public function __construct(mixed $child) { $this->child = $child; }
}
function removeOwnedObjectKey(): void {
    $owner = new RemovalOffsetHolder(); $owner->child = new RemovalOffsetTarget();
    unset($owner->child[new RemovalOwnedKey()]); echo '|';
    $readonly = new ReadonlyRemovalHolder(new RemovalOffsetTarget());
    unset($readonly->child[new RemovalOwnedKey()]); echo '|';
    $array = new ReadonlyRemovalHolder(['base' => 1]);
    try { unset($array->child[false]); } catch (Error $error) { echo 'R:', get_class($error), '|'; unset($error); }
}
function replaceRemovalHash(array &$items): mixed {
    echo 'H'; $items = ['keep' => 2, 'delete' => 3]; return 'delete';
}
function replaceRemovalHashWithBadKey(array &$items): mixed {
    echo 'B'; $items = ['keep' => 2, 0 => 3]; return false;
}
function replaceRemovalPacked(array &$items): mixed {
    echo 'P'; $items = [99, 88]; return 0;
}
function removeAfterKeyReplacement(): void {
    $items = ['old' => 1]; $snapshot = $items;
    unset($items[replaceRemovalHash($items)]); removalSnapshot($items, $snapshot);
    $badItems = ['old' => 1]; $badSnapshot = $badItems;
    try { unset($badItems[replaceRemovalHashWithBadKey($badItems)]); }
    catch (TypeError $error) { echo 'K|'; unset($error); }
    removalSnapshot($badItems, $badSnapshot);
    $packed = [11, 22]; $packedSnapshot = $packed;
    unset($packed[replaceRemovalPacked($packed)]); removalSnapshot($packed, $packedSnapshot);
}
removeOwnedObjectKey(); removeAfterKeyReplacement();
function removeAfterInlineStorageChange(array $floats, array $assoc, mixed &$scalar): void {
    $floatSnapshot = $floats;
    unset($floats[count($floats = [1.5, 2.5]) - 2]); echo json_encode($floats), '/', json_encode($floatSnapshot), '|';
    $assocSnapshot = $assoc;
    unset($assoc[count($assoc = ['keep' => 7]) - 1]); echo json_encode($assoc), '/', json_encode($assocSnapshot), '|';
    try { unset($scalar[($scalar = 42) > 0 ? 0 : 0]); }
    catch (Error $error) { echo 'S:', get_class($error), '|'; unset($error); }
    echo json_encode($scalar), '|';
}
$scalarRemoval = 0; removeAfterInlineStorageChange([11, 22], [11, 22], $scalarRemoval);
