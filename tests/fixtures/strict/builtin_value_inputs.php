<?php
function strictBuiltinLength(mixed $value): void { echo strlen($value), 'A'; }
function strictBuiltinRepeat(mixed $value): void { echo str_repeat('a', $value), 'A'; }
function strictBuiltinValue(mixed $value): mixed { return $value; }
function strictBuiltinLater(): int { echo 'E'; return 2; }
function strictBuiltinNamed(mixed $value): void { echo strlen(string: $value); }
function strictBuiltinSpread(array $values): void { echo strlen(...$values); }
function strictBuiltinCall(mixed $value): void { $call = strlen(...); echo $call($value); }
function strictBuiltinOrder(mixed $value): void { echo str_repeat($value, strictBuiltinLater()); }
function strictBuiltinNamedOrder(mixed $value): void { echo str_repeat(times: strictBuiltinLater(), string: $value); }
function strictBuiltinJson(mixed $value): void { echo json_decode($value); }
class StrictBuiltinText { public function __toString(): string { echo 'S'; return 'converted'; } }
try { strictBuiltinLength(7); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictBuiltinLength(true); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictBuiltinLength(null); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictBuiltinLength(new StrictBuiltinText()); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictBuiltinRepeat(2.0); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictBuiltinNamed(7); echo 'X'; } catch (TypeError $error) { echo 'N'; unset($error); }
try { strictBuiltinSpread([7]); echo 'X'; } catch (TypeError $error) { echo 'P'; unset($error); }
try { strictBuiltinCall(7); echo 'X'; } catch (TypeError $error) { echo 'C'; unset($error); }
try { strictBuiltinOrder(new StrictBuiltinText()); echo 'X'; } catch (TypeError $error) { echo 'O'; unset($error); }
try { strictBuiltinNamedOrder(new StrictBuiltinText()); echo 'X'; } catch (TypeError $error) { echo 'M'; unset($error); }
try { strictBuiltinJson(new StrictBuiltinText()); echo 'X'; } catch (TypeError $error) { echo 'J'; unset($error); }
echo '|'; strictBuiltinLength(strval(7));
echo '|', substr('abc', 0, null), substr('abc', 1, strictBuiltinValue(null));
gc_collect_cycles();
