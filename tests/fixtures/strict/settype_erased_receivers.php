<?php
function convertErasedEntry(mixed $target, mixed &$source): void { $target($source['k'], 'int'); }
function convertErasedProperty(mixed $target, mixed $source): void { $target($source->value, 'int'); }
$xml = new SimpleXMLElement('<r><value>7.9</value></r>');
$target = $argc > 0 ? 'settype' : 'SETTYPE';
mixed $items = ['k' => '9.9']; $copy = $items; convertErasedEntry($target, $items);
echo $items['k'], ':', $copy['k'], '|';
class ErasedConversionPlace { public mixed $value = '8.9'; }
$box = new ErasedConversionPlace(); $alias =& $box->value; convertErasedProperty($target, $box);
echo $alias, ':', $box->value, '|';
convertErasedProperty($target, $xml); echo (string)$xml->value;

class FreshConversionPlace {
    public mixed $value = ['old'];
    public function __construct() { $alias =& $this->value; }
    public function __destruct() { echo 'D'; $this->value = 'changed'; }
}
function freshConversionPlace(): mixed { echo 'F'; return new FreshConversionPlace(); }
set_error_handler(function (): bool { echo 'W'; return true; });
echo '|'; $target(freshConversionPlace()->value, 'string'); restore_error_handler();
class SecretConversionPlace {
    private mixed $value = 'hidden';
    public function __construct() { $alias =& $this->value; }
}
$secret = new SecretConversionPlace();
try { convertErasedProperty($target, $secret); echo '|X'; }
catch (Error $error) { echo '|', str_contains($error->getMessage(), 'private property') ? 'E' : 'X'; }
