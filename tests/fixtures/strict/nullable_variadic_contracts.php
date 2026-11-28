<?php
declare(strict_types=0);
function nullableVariadicDispatch(callable $callback, array $values): mixed { return $callback(...$values); }
function nullableVariadicArray(array ...$values): void { echo 'bad'; }
function nullableVariadicIterable(iterable ...$values): void { echo 'bad'; }
function nullableVariadicCallable(callable ...$values): void { echo 'bad'; }
function nullableVariadicNull(): mixed { return null; }
try { nullableVariadicDispatch(nullableVariadicArray(...), [nullableVariadicNull()]); }
catch (TypeError $error) { echo 'A'; }
try { nullableVariadicDispatch(nullableVariadicIterable(...), ['named' => nullableVariadicNull()]); }
catch (TypeError $error) { echo 'I'; }
try { nullableVariadicDispatch(nullableVariadicCallable(...), [nullableVariadicNull()]); }
catch (TypeError $error) { echo 'C'; }
function nullableVariadicArrayControl(array ...$values): int { return count($values); }
function nullableVariadicIterableControl(iterable ...$values): int { return count($values); }
function nullableVariadicCallableControl(callable ...$values): int { return count($values); }
function nullableVariadicOptional(?array ...$values): mixed { return $values[0]; }
function nullableVariadicOpen(mixed ...$values): mixed { return $values['named']; }
echo '|', nullableVariadicDispatch(nullableVariadicArrayControl(...), [[7]]), ':',
    nullableVariadicDispatch(nullableVariadicIterableControl(...), ['named' => [8]]), ':',
    nullableVariadicDispatch(nullableVariadicCallableControl(...), [strlen(...)]), ':',
    gettype(nullableVariadicDispatch(nullableVariadicOptional(...), [null])), ':',
    gettype(nullableVariadicDispatch(nullableVariadicOpen(...), ['named' => null]));
