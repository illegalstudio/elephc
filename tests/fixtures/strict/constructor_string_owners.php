<?php
class ConversionStringBox {
    public string $value;
    public function __construct(string $value) { $this->value = $value; }
}
function conversionBox(string $class, mixed $value): mixed { return new $class($value); }
function showConversionBox(mixed $value): void {
    $box = conversionBox('ConversionStringBox', $value);
    echo '[', $box->value, ']';
}
class ConversionOwnedText { public function __toString(): string { return str_repeat('m', 2); } }
class ConversionLiteralText { public function __toString(): string { return 'lit'; } }
showConversionBox(str_repeat('v', 2));
showConversionBox(2);
showConversionBox(false);
showConversionBox(1.5);
showConversionBox(new ConversionOwnedText());
showConversionBox(new ConversionLiteralText());
echo '|';
class ConversionPair {
    public string $first;
    public string $second;
    public function __construct(string $first, string $second) { $this->first = $first; $this->second = $second; }
}
function conversionPair(string $class, mixed $first, mixed $second): mixed { return new $class($first, $second); }
class ConversionResetText { public function __toString(): string { echo 123; gc_collect_cycles(); return 'late'; } }
$pair = conversionPair('ConversionPair', 7, new ConversionResetText());
echo $pair->first, ':', $pair->second, '|';
class ConversionThrowText { public function __toString(): string { echo 'Q'; throw new RuntimeException('conversion'); } }
try { conversionPair('ConversionPair', str_repeat('a', 2), new ConversionThrowText()); }
catch (RuntimeException $error) { echo 'T|'; unset($error); }
class ConversionThrowConstructor {
    public function __construct(string $value) { echo 'E'; gc_collect_cycles(); throw new RuntimeException('constructor'); }
}
try { conversionBox('ConversionThrowConstructor', str_repeat('f', 2)); }
catch (RuntimeException $error) { echo 'T|'; unset($error); }
class ConversionReference {
    public string $value;
    public function __construct(string $value, mixed &$counter) { $this->value = $value; $counter++; }
}
function conversionReference(string $class, mixed $value, mixed &$counter): mixed { return new $class($value, $counter); }
$counter = 1;
$referenced = conversionReference('ConversionReference', str_repeat('r', 2), $counter);
echo $referenced->value, ':', $counter, '|';
function conversionTypedReference(string $class, mixed $value, int &$counter): mixed { return new $class($value, $counter); }
$typedCounter = 1;
$typedReference = conversionTypedReference('ConversionReference', str_repeat('s', 2), $typedCounter);
echo $typedReference->value, ':', $typedCounter, '|';
class ConversionFour {
    public function __construct(string $a, string $b, string $c, string $d) {
        gc_collect_cycles(); echo $a, ':', $b, ':', $c, ':', $d, '|';
    }
}
function conversionFour(string $class, mixed $a, mixed $b, mixed $c, mixed $d): mixed { return new $class($a, $b, $c, $d); }
conversionFour('ConversionFour', 4, 5, 6, new ConversionResetText());
