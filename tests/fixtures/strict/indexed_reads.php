<?php
function checkedPacked(array $values, int $index): mixed {
    return $values[$index];
}
$index = $argc + 7;
$packed = [11, 22];
try { echo $packed[$index]; } catch (OutOfBoundsException $error) { echo 'P'; }
try { echo checkedPacked([1, 2], $index); } catch (OutOfBoundsException $error) { echo 'B'; }
$hash = ['present' => 4, 'nil' => null];
$key = str_repeat('x', 3);
try { echo $hash[$key]; } catch (OutOfBoundsException $error) { echo 'H'; }
echo ':', gettype($hash['nil']), ':', $packed[0], ':', $hash['present'], '|';
$text = str_repeat('abc', 2);
echo $text[-1], ':', $text[-6], '|';
try { echo $text[$index]; } catch (OutOfBoundsException $error) { echo 'S'; }
try { echo str_repeat('a', 3)[-9]; } catch (OutOfBoundsException $error) { echo 'N'; }
echo '|', $packed[$index] ?? 'q', ':', $hash[$key] ?? 'q', ':', $text[$index] ?? 'q';
echo ':', isset($packed[$index]) ? 'bad' : 'i', ':', empty($hash[$key]) ? 'e' : 'bad';
echo '|', $unknown['x']['y'] ?? 'quiet', '|';
try { $packed[$index]; } catch (OutOfBoundsException $error) { echo 'D'; }

echo '|';
function indexingKey(mixed $value): mixed { echo 'k'; return $value; }
function indexingArray(): array { echo 'r'; return [11, 22]; }
try { echo indexingArray()[indexingKey('absent')]; }
catch (OutOfBoundsException $error) { echo 'K'; }
finally { echo 'f'; }
$mixedKey = $argc > 0 ? 'outside' : 99;
try { $packed[$mixedKey]; } catch (OutOfBoundsException $error) { echo 'X'; }
$rows = [[1, 2]];
try { foreach ($rows[$index] as &$element) { echo 'bad'; } }
catch (OutOfBoundsException $error) { echo 'F'; }
foreach ($rows[0] as &$element) { $element += 1; }
echo ':', $rows[0][0], '|';
set_error_handler(function ($severity, $message) { echo 'W'; return true; });
$floatIndex = $argc + 10.5;
try { $packed[$floatIndex]; } catch (OutOfBoundsException $error) { echo 'Z'; }
restore_error_handler();
