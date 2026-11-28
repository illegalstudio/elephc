<?php
class EvalGuardHolder {
    public mixed $items = [0 => 10, 1 => 20, '' => 30];
    public static mixed $shared = [0 => 10, 1 => 20, '' => 30];
}
class EvalGuardOffsets implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { echo 'G:', gettype($key), '|'; return 1; }
    public function offsetSet(mixed $key, mixed $value): void { echo 'O:', gettype($key), '|'; }
    public function offsetUnset(mixed $key): void { echo 'V:', gettype($key), '|'; }
}
class EvalGuardOwnedKey { public function __destruct() { echo 'D|'; } }
function evalArrayKeyGuardObservation(mixed $key): void {
    $target = [0 => 10, 1 => 20, '' => 30];
    try { echo $target[$key], '|'; } catch (TypeError $error) { echo 'R|'; unset($error); }
    try { $target[$key] = 40; echo 'w|'; } catch (TypeError $error) { echo 'W|'; unset($error); }
    try { $literal = [$key => 50]; echo 'l|'; } catch (TypeError $error) { echo 'L|'; unset($error); }
    try { unset($target[$key]); echo 'u|'; } catch (TypeError $error) { echo 'U|'; unset($error); }
    $owner = new EvalGuardHolder();
    try { $owner->items[$key] = 40; echo 'p|'; } catch (TypeError $error) { echo 'P|'; unset($error); }
    try { unset($owner->items[$key]); echo 'q|'; } catch (TypeError $error) { echo 'Q|'; unset($error); }
    EvalGuardHolder::$shared = [0 => 10, 1 => 20, '' => 30];
    try { EvalGuardHolder::$shared[$key] = 40; echo 's|'; } catch (TypeError $error) { echo 'S|'; unset($error); }
    try { unset(EvalGuardHolder::$shared[$key]); echo 't|'; } catch (TypeError $error) { echo 'T|'; unset($error); }
    $offsets = new EvalGuardOffsets(); echo $offsets[$key], '|';
    $offsets[$key] = 1; unset($offsets[$key]);
}
