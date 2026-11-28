<?php
declare(strict_types=0);
function nullableBoundaryNull(): mixed { return null; }
function nullableBoundaryLater(): int { echo 'L'; return 0; }
function nullableBoundaryInput(string $value, int $later): string { return $value; }
function nullableBoundaryReturn(mixed $value): string { return $value; }
function nullableBoundaryOptional(?string $value): ?string { return $value; }
function nullableBoundaryMixed(mixed $value): mixed { return $value; }
function nullableBoundaryVoid(): void {}
class NullableBoundarySlot { public string $value = 'kept'; }
try { nullableBoundaryInput(nullableBoundaryNull(), nullableBoundaryLater()); } catch (TypeError $error) { echo 'P'; }
echo '|';
try { nullableBoundaryReturn(nullableBoundaryNull()); } catch (TypeError $error) { echo 'R'; }
echo '|';
$slot = new NullableBoundarySlot();
try { $slot->value = nullableBoundaryNull(); } catch (TypeError $error) { echo 'S'; }
echo ':', $slot->value, '|';
echo gettype(nullableBoundaryOptional(null)), ':', nullableBoundaryReturn(7), ':', nullableBoundaryInput(8, 0), ':', gettype(nullableBoundaryMixed(null)), ':', gettype(nullableBoundaryVoid());
