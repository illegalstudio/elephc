<?php
// Capture PHP's outer scalar/array parameter coercion and callback ordering.
// Run: php scripts/mbstring/capture_coercions.php

error_reporting(E_ALL);
ini_set('precision', '14');
$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/coercions.jsonl.gz', 'wb9');
$trace = [];

/** Supplies a visible Stringable callback without performing external I/O. */
final class CoercionText {
    public function __construct(private string $text, private bool $throws) {}
    public function __toString(): string {
        $GLOBALS['trace'][] = 'stringify';
        if ($this->throws) { throw new RuntimeException('coercion callback failed'); }
        return $this->text;
    }
}
final class CoercionObject {}

/** Reconstructs one concrete value from lossless, deterministic input metadata. */
function coercionValue(array $input): mixed {
    return match ($input['kind']) {
        'null' => null,
        'bool', 'int' => $input['value'],
        'float' => unpack('E', hex2bin($input['bits']))[1],
        'string' => hex2bin($input['bytes']),
        'array' => $input['invalid'] ? ['name' => "\xff"] : ['names' => ['猫', 'é']],
        'object' => new CoercionObject(),
        'stringable' => new CoercionText(hex2bin($input['bytes']), $input['throws']),
        'resource', 'closed-resource' => fopen('php://memory', 'r+'),
    };
}

/** Encodes values and exception messages without losing binary string data. */
function coercionResult(mixed $value): array {
    if (is_string($value)) { return ['string', bin2hex($value)]; }
    if (is_bool($value)) { return ['bool', $value]; }
    if (is_int($value)) { return ['int', $value]; }
    throw new LogicException('unexpected result type');
}

$weak = static fn(string $function, array $arguments): mixed => $function(...$arguments);
$strict = eval('declare(strict_types=1); return static fn(string $function, array $arguments): mixed => $function(...$arguments);');
$subjects = [
    ['mb_decode_mimeheader', 0, ['']],
    ['mb_substr', 0, ['', 0, null, '8bit']],
    ['mb_substr', 1, ['abcdef', 0, 1, '8bit']],
    ['mb_substr', 2, ['abcdef', 1, null, '8bit']],
    ['mb_strstr', 2, ['abcd', 'b', false, '8bit']],
    ['mb_check_encoding', 0, [null, 'UTF-8']],
    ['mb_substitute_character', 0, [null]],
    ['mb_internal_encoding', 0, [null]],
];
$inputs = [['kind' => 'null'], ['kind' => 'bool', 'value' => false], ['kind' => 'bool', 'value' => true]];
foreach ([0, 1, -1, 33, 63, PHP_INT_MIN, PHP_INT_MAX] as $value) { $inputs[] = ['kind' => 'int', 'value' => $value]; }
foreach (['', '0', '00', '-0', '+0', '1', '1.0', '.5', '-.5', '1.', '1e1', '1e-1',
    ' 1.5 ', "\t\n\r\v\f 1.5 \t\n\r\v\f", '1tail', '1e', '1e+', '1e-9999', '1e9999',
    '0x10', '010', 'INF', '-INF', 'NAN', "1\0", "1\xff", "\xff", '猫', 'UTF-8', 'ASCII', 'UTF-16', 'NoNe',
    'long', 'entity', '65', "none\0", '9223372036854775807', '9223372036854775808',
    '-9223372036854775808', '-9223372036854775809', '9223372036854775807.0',
    '-9223372036854775808.0', '-9223372036854775809.0', '9.223372036854775e18',
    '9.223372036854776e18', str_repeat('0', 1000) . '1', str_repeat('9', 1000)] as $bytes) {
    $inputs[] = ['kind' => 'string', 'bytes' => bin2hex($bytes)];
}
for ($index = 0; $index < 512; $index++) {
    $random = hash('sha256', 'mbstring-numeric-string-' . $index);
    $integer = hexdec(substr($random, 0, 5));
    $fraction = hexdec(substr($random, 5, 3)) % 1000;
    $exponent = hexdec(substr($random, 8, 1)) % 5 - 2;
    $bytes = ($index % 2 ? '-' : '+') . $integer . '.' . $fraction . 'e' . $exponent;
    if ($index % 3 === 0) { $bytes = "\v " . $bytes . " \t"; }
    if ($index % 7 === 0) { $bytes .= "\0"; }
    $inputs[] = ['kind' => 'string', 'bytes' => bin2hex($bytes)];
}
$floats = [0.0, -0.0, 1.0, 33.0, 33.5, -33.5, 0.0001, 0.00001, 1.5e-20,
    0.9999999999999999, 9007199254740991.0, 9007199254740992.0,
    9223372036854775808.0, -9223372036854775808.0, -9223372036854777856.0, INF, -INF, NAN];
foreach ($floats as $value) { $inputs[] = ['kind' => 'float', 'bits' => bin2hex(pack('E', $value))]; }
for ($index = 0; $index < 2048; $index++) {
    $inputs[] = ['kind' => 'float', 'bits' => bin2hex(substr(hash('sha256', 'mbstring-coercion-' . $index, true), 0, 8))];
}
foreach ([false, true] as $invalid) { $inputs[] = ['kind' => 'array', 'invalid' => $invalid]; }
$inputs[] = ['kind' => 'object'];
foreach (['abc', 'UTF-8', 'none', '33', "\xff"] as $bytes) {
    foreach ([false, true] as $throws) { $inputs[] = ['kind' => 'stringable', 'bytes' => bin2hex($bytes), 'throws' => $throws]; }
}
$inputs[] = ['kind' => 'resource'];
$inputs[] = ['kind' => 'closed-resource'];
$count = 0;
foreach ($inputs as $input) {
    foreach ([false, true] as $isStrict) {
        foreach ($subjects as [$function, $parameter, $arguments]) {
            mb_internal_encoding('UTF-8');
            mb_substitute_character(63);
            $trace = [];
            $warnings = [];
            $value = coercionValue($input);
            if ($input['kind'] === 'closed-resource') { fclose($value); }
            $metadata = $input;
            if (is_float($value)) { $metadata['formatted'] = bin2hex(@(string)$value); }
            $arguments[$parameter] = $value;
            set_error_handler(static function (int $level, string $message) use (&$warnings): bool {
                $warnings[] = [$level, bin2hex($message)];
                return true;
            });
            try { $result = coercionResult(($isStrict ? $strict : $weak)($function, $arguments)); }
            catch (Throwable $error) { $result = ['error', $error::class, bin2hex($error->getMessage())]; }
            restore_error_handler();
            if (is_resource($value)) { fclose($value); }
            gzwrite($stream, json_encode(['function' => $function, 'parameter' => $parameter, 'strict' => $isStrict,
                'input' => $metadata, 'result' => $result, 'warnings' => $warnings, 'trace' => $trace,
                'substitution' => coercionResult(mb_substitute_character()), 'internal' => mb_internal_encoding()], JSON_THROW_ON_ERROR) . "\n");
            $count++;
        }
    }
}
gzclose($stream);
fwrite(STDOUT, "Captured $count PHP coercion cases\n");

// Wrong arity must fail before any supplied Stringable argument is converted.
$catalog = file_get_contents(__DIR__ . '/../../crates/elephc-builtin-contract/src/catalog_mbstring.rs');
preg_match_all('/id: BuiltinId::from_canonical_name\("([^"]+)"\), name:/', $catalog, $matches);
$arity = [];
foreach ($matches[1] as $function) {
    $reflection = new ReflectionFunction($function);
    $counts = [$reflection->getNumberOfParameters() + 1];
    if ($reflection->getNumberOfRequiredParameters() > 0) { $counts[] = $reflection->getNumberOfRequiredParameters() - 1; }
    foreach ($counts as $count) {
        $trace = [];
        try { $weak($function, array_fill(0, $count, new CoercionText('UTF-8', false))); }
        catch (ArgumentCountError $error) {
            $arity[] = ['function' => $function, 'count' => $count, 'message' => $error->getMessage(), 'trace' => $trace];
        }
    }
}
file_put_contents(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/coercion_arity.json', json_encode($arity, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR) . "\n");
fwrite(STDOUT, 'Captured ' . count($arity) . " PHP arity cases\n");
