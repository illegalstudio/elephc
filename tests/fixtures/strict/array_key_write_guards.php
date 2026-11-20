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
