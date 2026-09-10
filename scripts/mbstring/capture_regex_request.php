<?php
// Each invocation captures one fresh PHP request, including state after caught errors.
if (PHP_VERSION !== '8.5.10' || MB_ONIGURUMA_VERSION !== '6.9.10') {
    throw new RuntimeException('The mbregex oracle requires PHP 8.5.10 and Oniguruma 6.9.10');
}

// Encode every PHP string and array key without losing binary values or register order.
function pack_regex_value(mixed $value): mixed {
    if (is_string($value)) { return ['bytes' => bin2hex($value)]; }
    if (!is_array($value)) { return $value; }
    $pairs = [];
    foreach ($value as $key => $item) { $pairs[] = [pack_regex_value($key), pack_regex_value($item)]; }
    return ['array' => $pairs];
}

// Decode only the exact tagged scalar/array fixtures used to initialize a by-reference output.
function unpack_regex_value(mixed $value): mixed {
    if (!is_array($value)) { return $value; }
    if (isset($value['bytes'])) { return hex2bin($value['bytes']); }
    $result = [];
    foreach ($value['array'] as [$key, $item]) { $result[unpack_regex_value($key)] = unpack_regex_value($item); }
    return $result;
}

// Retain chained exceptions when an ignored option error is followed by another failure.
function pack_regex_error(Throwable $error): array {
    return [get_class($error), bin2hex($error->getMessage()),
        $error->getPrevious() ? pack_regex_error($error->getPrevious()) : null];
}

// Run one public operation with already typed strings so this oracle isolates shared request semantics.
function regex_step(array $step): array {
    $warnings = [];
    $callbacks = [];
    $matches = unpack_regex_value(array_key_exists('matches', $step) ? $step['matches'] : ['bytes' => bin2hex('old')]);
    $matches_at_warning = [];
    $capture = in_array($step['op'], ['ereg', 'eregi'], true);
    $entered = false;
    set_error_handler(function ($level, $message) use (&$warnings, &$callbacks, &$entered, &$matches, &$matches_at_warning, $capture, $step) {
        $warnings[] = bin2hex($message);
        if ($capture) {
            $matches_at_warning[] = pack_regex_value($matches);
            if (array_key_exists('warning_matches', $step)) { $matches = unpack_regex_value($step['warning_matches']); }
        }
        if (!$entered && isset($step['on_warning'])) {
            $entered = true;
            $callbacks[] = array_map('regex_step', $step['on_warning']);
            if ($step['throw'] ?? false) { throw new RuntimeException('regex handler failed'); }
        }
        return true;
    });
    $output = [];
    try {
        $pattern = isset($step['pattern']) ? hex2bin($step['pattern']) : null;
        $options = isset($step['options']) ? hex2bin($step['options']) : null;
        $value = match ($step['op']) {
            'encoding' => mb_regex_encoding(hex2bin($step['value'])),
            'options' => mb_regex_set_options(hex2bin($step['value'])),
            'init' => mb_ereg_search_init(hex2bin($step['subject']), $pattern, $options),
            'search' => mb_ereg_search($pattern, $options),
            'pos' => mb_ereg_search_pos($pattern, $options),
            'regs' => mb_ereg_search_regs($pattern, $options),
            'getregs' => mb_ereg_search_getregs(),
            'setpos' => mb_ereg_search_setpos($step['value']),
            'match' => mb_ereg_match($pattern, hex2bin($step['subject']), $options),
            'split' => mb_split($pattern, hex2bin($step['subject']), $step['limit']),
            'replace' => mb_ereg_replace($pattern, hex2bin($step['replacement']), hex2bin($step['subject']), $options),
            'ireplace' => mb_eregi_replace($pattern, hex2bin($step['replacement']), hex2bin($step['subject']), $options),
            'ereg' => ($step['with_matches'] ?? true)
                ? mb_ereg($pattern, hex2bin($step['subject']), $matches) : mb_ereg($pattern, hex2bin($step['subject'])),
            'eregi' => ($step['with_matches'] ?? true)
                ? mb_eregi($pattern, hex2bin($step['subject']), $matches) : mb_eregi($pattern, hex2bin($step['subject'])),
            'retry' => ini_set('mbstring.regex_retry_limit', (string)$step['value']),
            'stack' => ini_set('mbstring.regex_stack_limit', (string)$step['value']),
            default => throw new LogicException('Unknown regex operation'),
        };
        $output['value'] = pack_regex_value($value);
    } catch (Throwable $error) {
        $output['error'] = pack_regex_error($error);
    } finally {
        restore_error_handler();
    }
    $output['warnings'] = $warnings;
    if ($capture) { $output['matches'] = pack_regex_value($matches); $output['matches_at_warning'] = $matches_at_warning; }
    if (isset($step['on_warning'])) { $output['callbacks'] = $callbacks; }
    $output['position'] = mb_ereg_search_getpos();
    $output['encoding'] = mb_regex_encoding();
    $output['options'] = mb_regex_set_options();
    return $output;
}

if (realpath($_SERVER['SCRIPT_FILENAME']) === __FILE__) {
    $case = json_decode(stream_get_contents(STDIN), true, 512, JSON_THROW_ON_ERROR);
    $case['trace'] = array_map('regex_step', $case['steps']);
    echo json_encode($case, JSON_THROW_ON_ERROR), "\n";
}
