<?php
// Capture ordinary replacement callback traces from the pinned PHP baseline.
// Input and output are JSON lines; binary strings use hexadecimal byte values.
if (PHP_VERSION !== '8.5.10') {
    fwrite(STDERR, "Requires PHP 8.5.10\n");
    exit(1);
}

function callback_registers(array $matches): array {
    $result = [];
    foreach ($matches as $key => $value) {
        $result[] = [is_int($key) ? $key : ['bytes' => bin2hex($key)],
            is_string($value) ? ['bytes' => bin2hex($value)] : $value];
    }
    return ['array' => $result];
}

while (($line = fgets(STDIN)) !== false) {
    $case = json_decode($line, true, flags: JSON_THROW_ON_ERROR);
    mb_regex_encoding($case['encoding']);
    mb_regex_set_options($case['defaults'] ?? 'pr');
    ini_set('mbstring.regex_stack_limit', '100000');
    ini_set('mbstring.regex_retry_limit', '1000000');
    $matches = [];
    $warnings = [];
    set_error_handler(function (int $level, string $message) use (&$warnings): bool {
        $warnings[] = bin2hex($message);
        return true;
    });
    $result = ['matches' => [], 'warnings' => []];
    try {
        $value = mb_ereg_replace_callback(hex2bin($case['pattern']),
            function (array $groups) use (&$matches, $case): string {
                $matches[] = callback_registers($groups);
                if (isset($case['callback_options'])) {
                    mb_regex_set_options($case['callback_options']);
                }
                if (isset($case['callback_retry'])) {
                    ini_set('mbstring.regex_retry_limit', (string) $case['callback_retry']);
                }
                if (($case['throw_after'] ?? 0) === count($matches)) {
                    throw new RuntimeException('callback failed');
                }
                return hex2bin($case['replacement']);
            }, hex2bin($case['subject']), isset($case['options']) ? hex2bin($case['options']) : null);
        $result['value'] = is_string($value) ? ['bytes' => bin2hex($value)] : $value;
    } catch (Throwable $error) {
        $result['error'] = [get_class($error), bin2hex($error->getMessage()), null];
    } finally {
        restore_error_handler();
    }
    $result['matches'] = $matches;
    $result['warnings'] = $warnings;
    if (isset($case['callback_options'])) {
        $result['options'] = mb_regex_set_options();
    }
    echo json_encode(['input' => $case, 'expected' => $result], JSON_THROW_ON_ERROR), "\n";
}
