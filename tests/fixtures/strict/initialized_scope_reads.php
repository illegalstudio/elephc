<?php
function initializedStatic(bool $detach): void {
    static $value = 3;
    if ($detach) { unset($value); }
    try { echo $value; } catch (Error $error) { echo 'I'; }
    echo '|';
}
initializedStatic(false); initializedStatic(true); initializedStatic(false);
function initializedGlobal(bool $detach): void {
    global $value;
    if ($detach) { unset($value); }
    try { echo $value; } catch (Error $error) { echo 'I'; }
    echo '|';
}
$value = 4;
initializedGlobal(false); initializedGlobal(true); initializedGlobal(false);
if ($argc > 10000) { $rootValue = 5; }
function initializedRootProducer(bool $write): void { if ($write) { global $rootValue; $rootValue=5; } }
initializedRootProducer(false);
echo isset($rootValue) ? 'P' : 'A', ':', $rootValue ?? 'missing', '|';
try { echo $rootValue; } catch (Error $error) { echo 'I'; }
echo '|';
initializedRootProducer(true); echo $rootValue, '|';
function initializedNested(bool $write): void {
    if ($write) { $items = [7]; }
    echo isset($items[0]) ? 'P' : 'A', empty($items[0]) ? 'E' : 'F', ':', $items[0] ?? 'missing', '|';
    try { echo $items[0]; } catch (Error $error) { echo 'I'; }
    echo '|';
}
initializedNested(false); initializedNested(true);
function initializedQuietKey(): int { echo 'K'; return 0; }
echo isset($absentArray[initializedQuietKey()]) ? 'P' : 'A', '|';
echo empty($absentArray[initializedQuietKey()]) ? 'E' : 'F', '|';
echo $absentArray[initializedQuietKey()] ?? 'missing', '|';
function initializedOrdinaryKey(bool $write): void {
    if ($write) { $index = 0; }
    try { isset($absentArray[$index]); echo 'bad'; } catch (Error $error) { echo 'I'; }
    echo '|';
}
initializedOrdinaryKey(false);
class InitializationProperties { public $untyped; public int $typed; }
$object = new InitializationProperties();
echo gettype($object->untyped), '|';
try { echo $object->typed; } catch (Error $error) { echo 'I'; }
echo '|'; unset($object->untyped);
try { echo $object->untyped; } catch (Error $error) { echo 'I'; }
echo '|';
echo isset($object->untyped) ? 'P' : 'A', empty($object->untyped) ? 'E' : 'F', ':', $object->untyped ?? 'missing', '|';
class InitializedMagicProperty {
    public $value;
    public int $getCalls = 0;
    public int $issetCalls = 0;
    function __get(string $name): int { $this->getCalls++; return 9; }
    function __isset(string $name): bool { $this->issetCalls++; return true; }
}
$magic = new InitializedMagicProperty(); unset($magic->value);
echo $magic->value, ':', $magic->value ?? 0, '|';
echo isset($magic->value) ? 'P' : 'A', empty($magic->value) ? 'E' : 'F', ':', $magic->getCalls, ':', $magic->issetCalls, '|';
class InitializedGetterOnly {
    public $value;
    function __get(string $name): int { echo 'G'; return 6; }
}
$getterOnly = new InitializedGetterOnly(); unset($getterOnly->value);
echo $getterOnly->value ?? 0, '|', isset($getterOnly->value) ? 'P' : 'A', '|', empty($getterOnly->value) ? 'E' : 'F', '|';
