<?php
function nullableReceiverKey(): int { echo 'K'; return 0; }
function nullableReceiverRead(mixed $value): mixed { return $value[nullableReceiverKey()]; }
function nullableReceiverProbe(mixed $value): mixed { return $value[0][0] ?? 'q'; }
function nullableReceiverOwned(): mixed { return [null][0]; }
function nullableReceiverOwnedKey(): string { echo 'O'; return str_repeat('k', 2); }
function nullableReceiverOwnedRead(mixed $value): mixed { return $value[nullableReceiverOwnedKey()]; }
function nullableReceiverTyped(?array $value): mixed { return $value[nullableReceiverKey()]; }
try { nullableReceiverRead(null); } catch (TypeError $error) { echo 'N'; }
echo '|', nullableReceiverRead([7]), '|';
echo nullableReceiverProbe(null), ':', nullableReceiverProbe([[null]]), '|';
try { nullableReceiverOwnedRead(nullableReceiverOwned()); } catch (TypeError $error) { echo 'N'; }
echo '|';
try { nullableReceiverTyped(null); } catch (TypeError $error) { echo 'T'; }
