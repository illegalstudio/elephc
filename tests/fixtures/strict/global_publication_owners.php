<?php
class RetiredProgramValue {
    public function __destruct() { echo 'D'; throw new RuntimeException('retired'); }
}
class KeptProgramValue { public int $value = 2; }
function retiredProgramProducer(): mixed { return new RetiredProgramValue(); }
function readProgramPublication(): callable {
    global $publishedProgramValue;
    return function() use (&$publishedProgramValue): mixed { return $publishedProgramValue; };
}
$publishedProgramValue = retiredProgramProducer();
$readProgramPublication = readProgramPublication();
try { $publishedProgramValue = new KeptProgramValue(); echo 'unreachable'; }
catch (Throwable $error) { echo 'C'; }
$publishedObject = $readProgramPublication(); echo gettype($publishedObject), ':', $publishedObject->value, '|';
$publishedProgramValue = retiredProgramProducer();
try { $publishedProgramValue = [3]; echo 'unreachable'; }
catch (Throwable $error) { echo 'C'; }
$publishedArray = $readProgramPublication(); echo gettype($publishedArray), ':', $publishedArray[0], '|';
$publishedProgramValue = retiredProgramProducer();
try { $publishedProgramValue = str_repeat('x', 2); echo 'unreachable'; }
catch (Throwable $error) { echo 'C'; }
echo gettype($publishedProgramValue), ':', $readProgramPublication(), '|';
