<?php
class MissingKeyArrays {
    public array $items = ['' => 10, 0 => 20];
    public static array $shared = ['' => 10, 0 => 20];
}
class MissingKeyOffsets implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { echo 'O:', gettype($key), '|'; return 1; }
    public function offsetSet(mixed $key, mixed $value): void { echo 'S:', gettype($key), '|'; }
    public function offsetUnset(mixed $key): void { echo 'U:', gettype($key), '|'; }
}
function missingKeyRhs(): int { echo 'V'; return 30; }
function missingKeyReads(int $index): void {
    $ints = [0]; $strings = ['']; $target = ['' => 10, 0 => 20];
    try { echo $target[$ints[$index]], 'bad|'; }
    catch (TypeError $error) { echo 'rI|'; unset($error); }
    try { echo $target[@$strings[$index]], 'bad|'; }
    catch (TypeError $error) { echo 'rS|'; unset($error); }
    try { $target[$ints[$index]] = missingKeyRhs(); echo 'bad|'; }
    catch (TypeError $error) { echo 'wI|'; unset($error); }
    try { $literal = [$strings[$index] => 1]; echo 'bad|'; }
    catch (TypeError $error) { echo 'lS|'; unset($error); }
    try { unset($target[$ints[$index]]); echo 'bad|'; }
    catch (TypeError $error) { echo 'uI|'; unset($error); }
    $owner = new MissingKeyArrays();
    try { $owner->items[$strings[$index]] = 30; echo 'bad|'; }
    catch (TypeError $error) { echo 'pS|'; unset($error); }
    try { unset($owner->items[$ints[$index]]); echo 'bad|'; }
    catch (TypeError $error) { echo 'pI|'; unset($error); }
    try { MissingKeyArrays::$shared[$strings[$index]] = 30; echo 'bad|'; }
    catch (TypeError $error) { echo 'sS|'; unset($error); }
    try { unset(MissingKeyArrays::$shared[$ints[$index]]); echo 'bad|'; }
    catch (TypeError $error) { echo 'sI|'; unset($error); }
    try { $reference = &$target[$ints[$index]]; echo 'bad|'; }
    catch (TypeError $error) { echo 'bI|'; unset($error); }
    $copiedInt = @$ints[$index]; $secondInt = $copiedInt;
    try { echo $target[$secondInt], 'bad|'; }
    catch (TypeError $error) { echo 'cI|'; unset($error); }
    $copiedString = @$strings[$index]; $secondString = $copiedString;
    try { echo $target[$secondString], 'bad|'; }
    catch (TypeError $error) { echo 'cS|'; unset($error); }
    try { echo $target[$expressionCopy = @$strings[$index]], 'bad|'; }
    catch (TypeError $error) { echo 'eS|'; unset($error); }
    echo $target[(int)@$ints[$index]], '/', $target[(string)@$strings[$index]], '|';
    echo $target[$ints[0]], '/', $target[$strings[0]], '|';
    $sentinel = [9223372036854775806]; $large = [9223372036854775806 => 99];
    echo $large[$sentinel[0]], '|';
    $offsets = new MissingKeyOffsets();
    echo $offsets[@$ints[$index]], '|';
    $offsets[@$strings[$index]] = 1;
    unset($offsets[@$ints[$index]]);
    echo json_encode($target), '/', json_encode($owner->items), '/', json_encode(MissingKeyArrays::$shared), '|';
}
set_error_handler(static function (int $level, string $message): bool { return true; });
missingKeyReads(1);
restore_error_handler();
