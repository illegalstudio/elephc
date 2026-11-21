<?php
function markedWriteKey(mixed $key): mixed { echo 'K'; return $key; }
function markedWriteValue(): int { echo 'V'; return 9; }
function writeMapCase(mixed $key): void {
    $array = [0 => 1, 1 => 2, '' => 3];
    $copy = $array;
    try { $array[markedWriteKey($key)] = markedWriteValue(); echo 'W'; }
    catch (TypeError $error) { echo 'E'; unset($error); }
    echo $array[0], ':', $array[1], ':', $array[''], ':', $copy[0], '|';
}
writeMapCase(false);
writeMapCase(1.0);
writeMapCase(null);
function replaceWriteKey(mixed &$key, mixed $next): int { echo 'V'; $key = $next; return 9; }
function writeLateCase(mixed $next): void {
    $array = [1, 2];
    $key = 0;
    try { $array[$key] = replaceWriteKey($key, $next); echo 'W'; }
    catch (TypeError $error) { echo 'E'; unset($error); }
    echo $array[0], ':', $array[1], '|';
}
writeLateCase(true);
function writeOpaqueCase(mixed $parent, mixed $key): void {
    try { $parent[markedWriteKey($key)] = markedWriteValue(); echo 'W'; }
    catch (Error $error) { echo 'E'; unset($error); }
    echo gettype($parent), '|';
}
writeOpaqueCase([1, 2], false);
writeOpaqueCase(null, false);
writeOpaqueCase(false, false);
writeOpaqueCase('ab', false);
class WrittenPayload {
    public function __construct() { echo 'V'; }
    public function __destruct() { echo 'D'; gc_collect_cycles(); }
}
function writeOwnedCase(mixed $key): void {
    $array = [1];
    try { $array[markedWriteKey($key)] = new WrittenPayload(); echo 'W'; }
    catch (TypeError $error) { echo 'E'; unset($error); }
    unset($array);
    echo '|';
}
writeOwnedCase(false);
class WritePropertyReceiver {
    public int $reads = 0;
    public array $items = [1];
}
function markedPropertyReceiver(WritePropertyReceiver $box): WritePropertyReceiver {
    $box->reads++;
    echo 'G';
    return $box;
}
function writePropertyReceiverCase(mixed $key): void {
    $box = new WritePropertyReceiver();
    try { markedPropertyReceiver($box)->items[markedWriteKey($key)] = markedWriteValue(); echo 'W'; }
    catch (TypeError $error) { echo 'E'; unset($error); }
    echo $box->reads, '|';
}
writePropertyReceiverCase(false);
function writeOpaqueUpdateCase(mixed $parent, int $mode): void {
    try {
        if ($mode === 0) { $parent[false] += 1; }
        elseif ($mode === 1) { ++$parent[false]; }
        else { $parent[false][0] = 1; }
        echo 'W';
        if ($mode === 2) { echo $parent[0][0]; }
        else { echo $parent[0]; }
    } catch (Error $error) { echo 'E'; unset($error); }
    echo gettype($parent), '|';
}
foreach ([0, 1, 2] as $writeUpdateMode) {
    writeOpaqueUpdateCase(null, $writeUpdateMode);
    writeOpaqueUpdateCase(false, $writeUpdateMode);
}
function writeTypedNestedCase(mixed $key): void {
    $parent = [[1]];
    try { $parent[markedWriteKey($key)][0] = 2; echo 'W'; }
    catch (Error $error) { echo 'E'; unset($error); }
    echo $parent[0][0], '|';
}
function writeHashNestedCase(mixed $key): void {
    $parent = [0 => [1], 'a' => [2]];
    try { $parent[markedWriteKey($key)][0] = 3; echo 'W'; }
    catch (Error $error) { echo 'E'; unset($error); }
    echo $parent[0][0], '|';
}
writeTypedNestedCase(false);
writeHashNestedCase(false);
function writeOpaqueLeafCase(mixed $child): void {
    $parent = [$child];
    try { $parent[0][false] = 1; echo 'W'; }
    catch (Error $error) { echo 'E'; unset($error); }
    echo gettype($parent[0]), '|';
}
// The valid outer fetch materializes a null child before checking the leaf key.
writeOpaqueLeafCase(null);
writeOpaqueLeafCase(false);
function writeDynamicFalsePropertyCase(mixed $key, int $mode): void {
    $box = new stdClass();
    $box->value = false;
    try {
        if ($mode === 0) { $box->value[$key] += 1; }
        else { ++$box->value[$key]; }
        echo 'W';
    } catch (Error $error) { echo 'E'; unset($error); }
    echo gettype($box->value), '|';
}
writeDynamicFalsePropertyCase(false, 0);
writeDynamicFalsePropertyCase(false, 1);
