<?php
declare(strict_types=0);
function nullableContainerArgument(): mixed { return null; }
function nullableContainerLater(): int { echo 'L'; return 0; }
function nullableArrayArgument(array $value, int $later): void { echo 'bad'; }
function nullableIterableArgument(iterable $value, int $later): void { echo 'bad'; }
function nullableCallableArgument(callable $value, int $later): void { echo 'bad'; }
class NullableArgumentObject {}
function nullableObjectArgument(NullableArgumentObject $value, int $later): void { echo 'bad'; }
$arrays = [[7]];
$callbacks = [strlen(...)];
try { nullableArrayArgument($arrays[(int)($argc + 20)], nullableContainerLater()); }
catch (TypeError $error) { echo 'A|'; }
try { nullableIterableArgument($arrays[(int)($argc + 20)], nullableContainerLater()); }
catch (TypeError $error) { echo 'I|'; }
try { nullableCallableArgument($callbacks[(int)($argc + 20)], nullableContainerLater()); }
catch (TypeError $error) { echo 'C|'; }
try { nullableObjectArgument(nullableContainerArgument(), nullableContainerLater()); }
catch (TypeError $error) { echo 'O'; }
function nullableContainerControls(array $array, iterable $iterable, callable $callback, object $object): void {
    echo '|', count($array), ':', $callback('x'), ':', gettype($object);
}
nullableContainerControls([7], [8], strlen(...), new NullableArgumentObject());
nullableContainerControls([7], [8], 'strlen', new NullableArgumentObject());
