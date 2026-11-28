<?php
function rawOffsetRead(mixed $receiver, mixed $key): mixed { return $receiver[$key]; }
function opaqueOffsetReceiver(mixed $receiver): mixed { return $receiver; }
class OffsetKeyKinds implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { echo gettype($key), ';'; gc_collect_cycles(); return $key; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void {}
}
class OffsetArrayCopy implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { $key[] = 2; return $key; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void {}
}
class OffsetThrowing implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { echo gettype($key), ';'; gc_collect_cycles(); throw new RuntimeException('getter'); }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void {}
}
class OffsetKeyPayload {
    public static int $retired = 0;
    public function __destruct() { self::$retired++; gc_collect_cycles(); }
}
$reader = new OffsetKeyKinds();
foreach ([false, 1.5, null, 7, '1', '01', [], new stdClass()] as $key) {
    $result = rawOffsetRead($reader, $key);
    echo gettype($result), '|';
}
$opaque = opaqueOffsetReceiver($reader);
echo $opaque['1'], '|';
echo strlen($opaque[str_repeat('x', 64)]), '|';
$numericTarget = opaqueOffsetReceiver([0 => 'zero', 1 => 'one', 2 => 'two']);
$runtimeIndex = $argc;
echo $numericTarget[$runtimeIndex + 1], '|';
echo $opaque[$runtimeIndex + 1], '|';
$source = [1];
$copy = rawOffsetRead(new OffsetArrayCopy(), $source);
echo count($source), ':', count($copy), '|';
try { rawOffsetRead(new OffsetThrowing(), new OffsetKeyPayload()); }
catch (RuntimeException $error) { echo 'C|'; }
unset($error);
gc_collect_cycles();
echo OffsetKeyPayload::$retired, '|';
$handle = tmpfile();
$resourceResult = rawOffsetRead($reader, $handle);
echo gettype($resourceResult), '|';
fclose($handle);
