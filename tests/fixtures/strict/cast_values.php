<?php
function checkedCastInt(mixed $value): int { return (int)$value; }
function checkedCastFloat(mixed $value): float { return (float)$value; }
class CheckedCastObject { public string $value = 'owned'; }
echo checkedCastInt('9223372036854775807.9'), '|', checkedCastInt('-9223372036854775808.9'), '|';
echo checkedCastInt('92233720368547758070e-1'), '|', checkedCastInt('0e999999999999999999999'), '|';
echo checkedCastInt('1e-99999999999999999999'), '|', checkedCastInt('-7.9'), '|';
echo checkedCastInt(true), checkedCastInt(null), '|';
echo is_float(checkedCastFloat(1)) ? 'F' : 'X';
echo is_float(checkedCastFloat('1e999')) ? 'I' : 'X';
echo is_float(checkedCastFloat(NAN)) ? 'N|' : 'X|';
foreach (['12tail', '1e', '9223372036854775808', '-9223372036854775809', '1e99', "1\0"] as $bad) {
    try { checkedCastInt($bad); echo 'X'; } catch (ValueError $error) { echo 'V'; } finally { echo '.'; }
}
foreach ([INF, -INF, NAN, 1.0e20] as $badFloat) {
    try { checkedCastInt($badFloat); echo 'X'; } catch (ValueError $error) { echo 'R'; }
}
try { checkedCastInt([str_repeat('fresh', 2)]); echo 'X'; } catch (TypeError $error) { echo 'A'; }
try { checkedCastFloat(new CheckedCastObject()); echo 'X'; } catch (TypeError $error) { echo 'O'; }
echo '|', checkedCastInt(7.9), '|', checkedCastInt(-7.9), '|';
