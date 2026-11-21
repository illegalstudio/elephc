<?php
function guardedKeyRead(mixed $array, mixed $key): mixed { return $array[$key]; }
function originalKey(mixed $key): mixed { return $key; }
function temporaryKeyArray(): array { return [5, 6]; }
class GuardKeyAccess implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { echo gettype($key), ':'; return 'object'; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void {}
}
class GuardKeyPayload {
    public function __destruct() { echo 'D'; gc_collect_cycles(); }
}
function discardedKeyLiteral(mixed $key): void {
    try { [$key => 1]; echo 'A'; } catch (TypeError $error) { echo 'K'; }
}
function constructedKeyLiteral(mixed $key): void {
    try { $built = [$key => new GuardKeyPayload()]; echo 'A'; unset($built); }
    catch (TypeError $error) { echo 'K'; }
}
$hash = [0 => 10, 1 => 11, '' => 12];
foreach ([false, 1.0, null] as $key) {
    try { echo guardedKeyRead($hash, $key), ','; } catch (TypeError $error) { echo 'K,'; }
}
echo '|';
$packed = [10, 11];
try { echo $packed[originalKey(false)]; } catch (TypeError $error) { echo 'K'; }
echo '|';
$typed = [1 => 20, '1' => 30, '01' => 40];
echo guardedKeyRead($typed, 1), ':', guardedKeyRead($typed, '1'), ':', guardedKeyRead($typed, '01'), '|';
echo guardedKeyRead('abc', 1.0), '|';
echo guardedKeyRead(new GuardKeyAccess(), true), '|';
discardedKeyLiteral(false); echo '|';
constructedKeyLiteral(false); echo '|';
try { echo guardedKeyRead(temporaryKeyArray(), false); } catch (TypeError $error) { echo 'K'; }
echo '|';
try { echo isset($packed[originalKey(false)]) ? 'Y' : 'N'; } catch (TypeError $error) { echo 'K'; }
echo ':';
try { echo isset($typed[originalKey(false)]) ? 'Y' : 'N'; } catch (TypeError $error) { echo 'K'; }
echo '|';
