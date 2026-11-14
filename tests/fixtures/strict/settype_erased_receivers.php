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

function convertRuntimeNamedProperty(mixed $target, mixed $source, string $name): void {
    $target($source->{$name}, 'int');
}
$box->value = '10.9'; convertRuntimeNamedProperty($target, $box, 'value'); echo '|', $alias;
convertRuntimeNamedProperty($target, $xml, 'value'); echo '|', (string)$xml->value;
function runtimeConversionPropertyName(int $count): string { echo 'N'; return substr(str_repeat('value', $count + 1), 0, 5); }
set_error_handler(function (): bool { echo 'W'; return true; });
echo '|'; $target(freshConversionPlace()->{runtimeConversionPropertyName($argc)}, 'string'); restore_error_handler();
echo '|';
try { convertRuntimeNamedProperty($target, $secret, runtimeConversionPropertyName($argc)); echo 'X'; }
catch (Error $error) { echo str_contains($error->getMessage(), 'private property') ? 'E' : 'X'; }

$earlierConversionReceiver = new ErasedConversionPlace(); $earlierConversionReceiver->value = '11.9';
$laterConversionReceiver = new ErasedConversionPlace(); $laterConversionReceiver->value = '12.9';
$lateConversionReceiver = $earlierConversionReceiver;
function replaceConversionReceiver(): string {
    global $lateConversionReceiver, $laterConversionReceiver;
    $lateConversionReceiver = $laterConversionReceiver; echo 'N'; return 'value';
}
echo '|'; $target($lateConversionReceiver->{replaceConversionReceiver()}, 'int');
echo $earlierConversionReceiver->value, ':', $laterConversionReceiver->value;
