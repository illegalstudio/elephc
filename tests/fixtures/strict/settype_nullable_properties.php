<?php
class NullableConversionPlace {
    public ?int $number = 7;
    public int|string $choice = '18.9';
    public ?string $text = '19.9';
    public float|string $decimal = '20.9';
    public ?bool $enabled = true;
}
$box = new NullableConversionPlace();
$number =& $box->number;
$choice =& $box->choice;
$text =& $box->text;
$decimal =& $box->decimal;
$enabled =& $box->enabled;
$cast = settype(...);
settype($box->number, 'null');
echo is_null($number) ? 'N' : 'X';
$cast($box->number, 'int');
echo '|', $number;
$box->number = 9;
echo '|', $number;
try { $cast($box->number, 'array'); echo 'X'; }
catch (TypeError $error) { echo '|A', $number; }
settype($box->choice, 'int');
echo '|', $choice;
$cast($box->choice, 'string');
echo is_string($choice) ? '|S' : '|X';
$box->choice = 22;
echo '|', $choice;
$cast($box->text, 'int');
echo '|', $text;
settype($box->decimal, 'int');
echo is_float($decimal) ? '|F' : '|X';
$cast($box->enabled, 'int');
echo $enabled ? '|T' : '|X';
function replaceNullableConversionValue(mixed &$value, mixed $replacement): void {
    $value = $replacement;
}
try { replaceNullableConversionValue($box->number, ['refused']); echo 'X'; }
catch (TypeError $error) { echo '|R', $number; }
