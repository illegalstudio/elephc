<?php
declare(strict_types=0);
class NullableObjectArgument {}
class NullableOtherArgument {}
function nullableObjectCandidate(bool $present): mixed { return $present ? new NullableObjectArgument() : null; }
function nullableObjectOther(): mixed { return new NullableOtherArgument(); }
function nullableObjectLater(): int { echo 'L'; return 0; }
function nullableObjectConsume(NullableObjectArgument $value, int $later): void { echo 'V'; }
$value = nullableObjectCandidate(false);
try { nullableObjectConsume($value, nullableObjectLater()); } catch (TypeError $error) { echo 'N|'; }
$value = nullableObjectOther();
try { nullableObjectConsume($value, nullableObjectLater()); } catch (TypeError $error) { echo 'C|'; }
$value = nullableObjectCandidate(true);
nullableObjectConsume($value, nullableObjectLater());
echo '|';
function nullableNominalDispatch(callable $callback, array $arguments): mixed { return $callback(...$arguments); }
function nullableNominalVariadic(NullableObjectArgument ...$values): int { return count($values); }
function nullableNominalOptional(?NullableObjectArgument ...$values): mixed { return $values[0]; }
class NullableRelativeVariadic { public static function accept(self ...$values): int { return count($values); } }
try { nullableNominalDispatch(nullableNominalVariadic(...), [null]); } catch (TypeError $error) { echo 'V|'; }
try { nullableNominalDispatch(NullableRelativeVariadic::accept(...), [null]); } catch (TypeError $error) { echo 'S|'; }
echo nullableNominalDispatch(nullableNominalVariadic(...), [new NullableObjectArgument()]), ':',
    nullableNominalDispatch(NullableRelativeVariadic::accept(...), [new NullableRelativeVariadic()]), ':',
    gettype(nullableNominalDispatch(nullableNominalOptional(...), [null]));
