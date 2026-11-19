<?php
declare(strict_types=0);
class NullableReceiverNode { public mixed $value = 8; public mixed $child = null; }
function nullablePropertyRead(mixed $value): mixed { return $value->value; }
function nullablePropertyName(): string { echo 'K'; return str_repeat('value', 1); }
function nullablePropertyDynamic(mixed $value): mixed { return $value->{nullablePropertyName()}; }
function nullablePropertyQuiet(?NullableReceiverNode $value): mixed { return $value->child->value ?? 'q'; }
function nullablePropertyIsset(?NullableReceiverNode $value): bool { return isset($value->child->value); }
function nullablePropertyEmpty(?NullableReceiverNode $value): bool { return empty($value->child->value); }
function nullablePropertyWalk(?NullableReceiverNode $value): mixed { return $value?->child->value; }
function nullablePropertyOwned(): mixed { return [null][0]; }
function nullablePropertySelector(mixed $outer, mixed $selector): bool { return isset($outer->{$selector->value}); }
try { nullablePropertyRead(null); } catch (TypeError $error) { echo 'P'; }
echo '|', nullablePropertyRead(new NullableReceiverNode()), '|';
try { nullablePropertyDynamic(null); } catch (TypeError $error) { echo 'N'; }
echo '|', nullablePropertyQuiet(null), ':', (int)nullablePropertyIsset(null), ':', (int)nullablePropertyEmpty(null), ':', nullablePropertyQuiet(new NullableReceiverNode());
echo '|', nullablePropertyWalk(null) ?? 'q', '|';
try { nullablePropertyWalk(new NullableReceiverNode()); } catch (TypeError $error) { echo 'CN'; }
echo '|';
try { nullablePropertyRead(nullablePropertyOwned()); } catch (TypeError $error) { echo 'O'; }
echo '|';
try { nullablePropertySelector(new NullableReceiverNode(), null); } catch (TypeError $error) { echo 'Q'; }
