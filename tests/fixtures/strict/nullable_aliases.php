<?php
declare(strict_types=0);
function nullableAliasValue(): mixed { return null; }
function nullableAliasReplace(&$value): void { $value = nullableAliasValue(); echo 'bad'; }
class NullableAliasSlot { public string $value = 'kept'; public static string $shared = 'saved'; }
$slot = new NullableAliasSlot();
$alias =& $slot->value;
try { $alias = nullableAliasValue(); } catch (TypeError $error) { echo 'R'; }
echo ':', $slot->value, '|';
$staticAlias =& NullableAliasSlot::$shared;
try { $staticAlias = nullableAliasValue(); } catch (TypeError $error) { echo 'S'; }
echo ':', NullableAliasSlot::$shared, '|';
try { nullableAliasReplace($slot->value); } catch (TypeError $error) { echo 'P'; }
echo ':', $slot->value, '|';
$alias = 7;
echo gettype($alias), ':', $slot->value, '|';
unset($slot->value);
$alias = nullableAliasValue();
echo gettype($alias);
