<?php
declare(strict_types=0);
// Builtin reflection uses a native catalog and is covered by the native fixture.
function nullableBuiltinValue(): mixed { return null; }
function nullableBuiltinSeven(): mixed { return 7; }
function nullableBuiltinLater(): int { echo 'L'; return 1; }
try { strlen(nullableBuiltinValue()); } catch (TypeError $error) { echo 'D'; }
try { strlen(string: nullableBuiltinValue()); } catch (TypeError $error) { echo 'N'; }
try { strlen(...[nullableBuiltinValue()]); } catch (TypeError $error) { echo 'S'; }
try { call_user_func('strlen', nullableBuiltinValue()); } catch (TypeError $error) { echo 'C'; }
try { call_user_func_array('strlen', [nullableBuiltinValue()]); } catch (TypeError $error) { echo 'A'; }
$callable = strlen(...);
try { $callable(nullableBuiltinValue()); } catch (TypeError $error) { echo 'F'; }
try { substr(nullableBuiltinValue(), nullableBuiltinLater()); } catch (TypeError $error) { echo 'T'; }
try { json_decode(nullableBuiltinValue()); } catch (TypeError $error) { echo 'J'; }
echo '|', substr('abc', 1, null), ':', strlen(nullableBuiltinSeven()), ':', gettype(null), ':', strlen((string)null);
