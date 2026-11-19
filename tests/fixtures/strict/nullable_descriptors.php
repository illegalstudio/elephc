<?php
declare(strict_types=0);
function nullableDescriptorInvoke(callable $callback, mixed $value): mixed { return $callback($value); }
function nullableDescriptorSpread(callable $callback, array $values): mixed { return $callback(...$values); }
function nullableDescriptorString(string $value): void { echo 'bad'; }
function nullableDescriptorFloat(float $value): void { echo 'bad'; }
function nullableDescriptorInt(int $value): void { echo 'bad'; }
function nullableDescriptorBool(bool $value): void { echo 'bad'; }
function nullableDescriptorVariadic(string ...$values): void { echo 'bad'; }
function nullableDescriptorNull(): mixed { return null; }
try { nullableDescriptorInvoke(nullableDescriptorString(...), nullableDescriptorNull()); }
catch (TypeError $error) { echo 'S'; }
try { nullableDescriptorInvoke(nullableDescriptorFloat(...), nullableDescriptorNull()); }
catch (TypeError $error) { echo 'F'; }
try { nullableDescriptorInvoke(nullableDescriptorInt(...), nullableDescriptorNull()); }
catch (TypeError $error) { echo 'I'; }
try { nullableDescriptorInvoke(nullableDescriptorBool(...), nullableDescriptorNull()); }
catch (TypeError $error) { echo 'B'; }
try { nullableDescriptorSpread(nullableDescriptorString(...), ['value' => nullableDescriptorNull()]); }
catch (TypeError $error) { echo 'N'; }
try { nullableDescriptorSpread(nullableDescriptorVariadic(...), [nullableDescriptorNull()]); }
catch (TypeError $error) { echo 'V'; }
try { nullableDescriptorSpread(nullableDescriptorVariadic(...), ['named' => nullableDescriptorNull()]); }
catch (TypeError $error) { echo 'W'; }
$growth = ['a','b','c','d','e','f','g','h','i','j','k','l','m','n','o','p','q','r','s','t', nullableDescriptorNull()];
try { nullableDescriptorSpread(nullableDescriptorVariadic(...), $growth); }
catch (TypeError $error) { echo 'G'; }
$growth = ['a'=>'a','b'=>'b','c'=>'c','d'=>'d','e'=>'e','f'=>'f','g'=>'g','h'=>'h','i'=>'i','j'=>'j','k'=>'k','l'=>'l','m'=>'m','n'=>'n','o'=>'o','p'=>'p','q'=>'q','r'=>'r','s'=>'s','t'=>'t','last'=>nullableDescriptorNull()];
try { nullableDescriptorSpread(nullableDescriptorVariadic(...), $growth); }
catch (TypeError $error) { echo 'H'; }
function nullableDescriptorPair(string $first, int $last): void { echo 'bad'; }
function nullableDescriptorLater(): int { echo 'L'; return 0; }
try { nullableDescriptorSpread(nullableDescriptorPair(...), ['first' => nullableDescriptorNull(), 'last' => nullableDescriptorLater()]); }
catch (TypeError $error) { echo 'P'; }
function nullableDescriptorWeak(string $value): string { return $value; }
function nullableDescriptorOptional(?string $value = null): ?string { return $value; }
function nullableDescriptorOpen(mixed $value): mixed { return $value; }
echo '|', nullableDescriptorInvoke(nullableDescriptorWeak(...), 7), ':',
    gettype(nullableDescriptorInvoke(nullableDescriptorOptional(...), null)), ':',
    gettype(nullableDescriptorInvoke(nullableDescriptorOpen(...), null)), ':',
    gettype(nullableDescriptorSpread(nullableDescriptorOptional(...), []));
