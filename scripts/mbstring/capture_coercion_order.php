<?php
// Capture callback, diagnostic, array-reference, and lazy encoding-list order from PHP.
// Run: php scripts/mbstring/capture_coercion_order.php

error_reporting(E_ALL);
$weak = static fn(string $function, array $arguments): mixed => $function(...$arguments);
$strict = eval('declare(strict_types=1); return static fn(string $function, array $arguments): mixed => $function(...$arguments);');
$trace = [];

/** Runs a visible callback at exactly the point PHP requests object string conversion. */
final class MbOrderText {
    public function __construct(private string $label, private string $text, private ?Closure $action = null) {}
    public function __toString(): string {
        $GLOBALS['trace'][] = ['stringify', $this->label];
        if ($this->action !== null) { ($this->action)(); }
        return $this->text;
    }
}

/** Encodes successful scalar results and binary exception messages losslessly. */
function orderResult(mixed $value): array {
    return match (true) {
        is_string($value) => ['string', bin2hex($value)],
        is_bool($value) => ['bool', $value],
        is_int($value) => ['int', $value],
        default => throw new LogicException('unexpected result'),
    };
}

/** Builds independent PHP arguments, retaining mutation observers until the call has returned. */
function orderArguments(string $scenario, array &$observers): array {
    $encoding = static fn() => new MbOrderText('encoding', 'UTF-8');
    switch ($scenario) {
        case 'array_reference_mutation':
            $bytes = 'ok';
            $array = ['name' => &$bytes];
            $observers['bytes'] = &$bytes;
            $object = new MbOrderText('encoding', 'UTF-8', static function () use (&$bytes) { $bytes = "\xff"; });
            return ['mb_check_encoding', [$array, $object]];
        case 'array_cow_mutation':
            $array = ['name' => 'ok'];
            $observers['array'] = &$array;
            $object = new MbOrderText('encoding', 'UTF-8', static function () use (&$array) { $array['name'] = "\xff"; });
            return ['mb_check_encoding', [$array, $object]];
        case 'scalar_reference_mutation':
            $bytes = 'ok';
            $observers['bytes'] = &$bytes;
            $object = new MbOrderText('encoding', 'UTF-8', static function () use (&$bytes) { $bytes = "\xff"; });
            return ['mb_check_encoding', [&$bytes, $object]];
        case 'earlier_stringable_mutates_later_scalar':
            $offset = 0;
            $observers['offset'] = &$offset;
            $source = new MbOrderText('source', 'abcdef', static function () use (&$offset) { $offset = 2; });
            return ['mb_substr', [$source, &$offset, 1, '8bit']];
        case 'stringable_changes_internal':
            return ['mb_strlen', [new MbOrderText('source', "\xff\xff", static function () { mb_internal_encoding('8bit'); })]];
        case 'stringable_throws':
            return ['mb_strlen', [new MbOrderText('source', 'abc', static function () { throw new RuntimeException('callback stopped'); }), $encoding()]];
        case 'null_before_stringable':
            return ['mb_strlen', [null, $encoding()]];
        case 'lossy_before_stringable':
            return ['mb_substr', ['abcdef', 0.5, 1, $encoding()]];
        case 'nan_before_stringable':
            return ['mb_strlen', [NAN, $encoding()]];
        case 'all_coercions_before_bad_encoding':
            return ['mb_substr', [null, 0.5, 1.5, new MbOrderText('encoding', 'not-an-encoding')]];
        case 'list_stops_at_invalid':
            return ['mb_detect_encoding', ['abc', [new MbOrderText('first', 'not-an-encoding'), new MbOrderText('later', 'UTF-8')], true]];
        case 'list_auto_before_language_change':
            return ['mb_detect_encoding', ["\x82\xa0", ['auto', new MbOrderText('later', 'UTF-8', static function () { mb_language('Japanese'); })], true]];
        case 'list_auto_after_language_change':
            return ['mb_detect_encoding', ["\x82\xa0", [new MbOrderText('first', 'UTF-8', static function () { mb_language('Japanese'); }), 'auto'], true]];
        case 'list_conversion_outer_then_elements':
            return ['mb_convert_encoding', ["\x82\xa0", new MbOrderText('target', 'UTF-8', static function () { mb_language('Japanese'); }),
                ['auto', new MbOrderText('source', 'UTF-8', static function () { mb_language('neutral'); })]]];
        case 'list_throws':
            return ['mb_detect_encoding', ['abc', ['UTF-8', new MbOrderText('failure', 'ASCII', static function () { throw new RuntimeException('list callback stopped'); }), new MbOrderText('later', 'ASCII')], true]];
        case 'list_null_before_invalid':
            return ['mb_detect_encoding', ['abc', [null, new MbOrderText('later', 'UTF-8')], true]];
        case 'list_numeric_before_stringable':
            return ['mb_detect_encoding', ['abc', [1.5, new MbOrderText('later', 'UTF-8')], true]];
        case 'float_format_before_later_warning':
            return ['mb_substr', [1.23456789, 0.5, 20, '8bit']];
    }
    throw new LogicException('unknown scenario');
}

/** Keeps observer strings and nested arrays binary-safe without changing their original values. */
function orderObserver(mixed $value): mixed {
    if (is_string($value)) { return ['bytes' => bin2hex($value)]; }
    if (is_array($value)) { return array_map(orderObserver(...), $value); }
    return $value;
}

$scenarios = ['array_reference_mutation', 'array_cow_mutation', 'scalar_reference_mutation',
    'earlier_stringable_mutates_later_scalar', 'stringable_changes_internal', 'stringable_throws',
    'null_before_stringable', 'lossy_before_stringable', 'nan_before_stringable', 'all_coercions_before_bad_encoding',
    'list_stops_at_invalid', 'list_auto_before_language_change', 'list_auto_after_language_change',
    'list_conversion_outer_then_elements', 'list_throws', 'list_null_before_invalid', 'list_numeric_before_stringable',
    'float_format_before_later_warning'];
$cases = [];
foreach ($scenarios as $scenario) {
    foreach ([false, true] as $isStrict) {
        foreach (['observe', 'throw', 'mutate'] as $handler) {
            mb_internal_encoding('UTF-8');
            mb_language('neutral');
            mb_substitute_character(63);
            ini_set('precision', '14');
            $trace = [];
            $observers = [];
            [$function, $arguments] = orderArguments($scenario, $observers);
            set_error_handler(static function (int $level, string $message) use ($handler): bool {
                $GLOBALS['trace'][] = ['diagnostic', $level, bin2hex($message)];
                if ($handler === 'throw') { throw new RuntimeException('diagnostic stopped'); }
                if ($handler === 'mutate') {
                    mb_substitute_character(33);
                    mb_internal_encoding('8bit');
                    ini_set('precision', '3');
                }
                return true;
            });
            try { $result = orderResult(($isStrict ? $strict : $weak)($function, $arguments)); }
            catch (Throwable $error) { $result = ['error', $error::class, bin2hex($error->getMessage())]; }
            restore_error_handler();
            $cases[] = ['scenario' => $scenario, 'strict' => $isStrict, 'handler' => $handler,
                'result' => $result, 'trace' => $trace, 'observed' => orderObserver($observers),
                'internal' => mb_internal_encoding(), 'language' => mb_language(),
                'substitution' => mb_substitute_character(), 'precision' => ini_get('precision')];
        }
    }
}
file_put_contents(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/coercion_order.json', json_encode($cases, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR) . "\n");
fwrite(STDOUT, 'Captured ' . count($cases) . " PHP ordering cases\n");
