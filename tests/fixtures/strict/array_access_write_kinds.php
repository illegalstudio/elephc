<?php
class OffsetWriteProbe implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetSet(mixed $key, mixed $value): void {
        echo gettype($key), ':';
        if (is_string($key)) { echo strlen($key); } else { echo '-'; }
        echo ':', gettype($value), '|';
    }
    public function offsetUnset(mixed $key): void {}
}
class OffsetWriteIdentity {}
function writeOpaqueOffset(mixed $parent, mixed $key, mixed $value): void {
    try { $parent[$key] = $value; }
    catch (Throwable $error) { echo 'E:', get_class($error), '|'; unset($error); }
}
$probe = new OffsetWriteProbe();
writeOpaqueOffset($probe, true, 9);
writeOpaqueOffset($probe, 0.5, 9);
writeOpaqueOffset($probe, null, 9);
writeOpaqueOffset($probe, '0', 9);
writeOpaqueOffset($probe, '01', 9);
writeOpaqueOffset($probe, "a\0b", 9);
writeOpaqueOffset($probe, [1], 9);
writeOpaqueOffset($probe, new OffsetWriteIdentity(), 9);
$closure = static fn(): int => 1;
writeOpaqueOffset($probe, $closure, 9);
$fixed = new SplFixedArray(1);
writeOpaqueOffset($fixed, false, 7);
echo 'F:', $fixed[0], '|';
writeOpaqueOffset($fixed, 0.0, 8);
echo 'F:', $fixed[0], '|';
writeOpaqueOffset($fixed, null, 9);
$list = new SplDoublyLinkedList();
$list->push('seed');
writeOpaqueOffset($list, false, 'next');
echo 'L:', $list[0], '|';
$map = new WeakMap();
$key = new OffsetWriteIdentity();
writeOpaqueOffset($map, $key, 5);
echo 'M:', $map[$key], '|';
writeOpaqueOffset($map, false, 6);
class OffsetWritePayload {
    public function __destruct() { echo 'D'; gc_collect_cycles(); }
}
function coalesceOpaqueOffset(mixed $parent, mixed $key, mixed $value): mixed {
    return $parent[$key] ??= $value;
}
$payload = coalesceOpaqueOffset($probe, 'x', new OffsetWritePayload());
echo gettype($payload), '|';
unset($payload);
echo '|';
class ThrowingOffsetWriteProbe extends OffsetWriteProbe {
    public function offsetSet(mixed $key, mixed $value): void {
        echo 'T'; gc_collect_cycles(); throw new RuntimeException('offset');
    }
}
writeOpaqueOffset(new ThrowingOffsetWriteProbe(), new OffsetWritePayload(), new OffsetWritePayload());
echo '|';
