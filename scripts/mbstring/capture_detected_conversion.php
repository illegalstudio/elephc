<?php
// Capture source detection and conversion, including validation/diagnostic order.
// Run: php scripts/mbstring/capture_detected_conversion.php

error_reporting(E_ALL & ~E_DEPRECATED);
$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/detected-conversion.jsonl.gz', 'wb9');
mb_language('neutral');
$inputs = ['', 'test', "\xFF\x80", "h\xe9llo", 'héllo 日本', "\xC4\xA2", '+ZeVnLIqe-',
    '&amp;&#65;', 'YQ==', "\0A\0B", "\x1b\x24BF|K\\\x1b(B", 'U+1F1FA'];
$lists = [null, 'auto', 'UTF-8,ISO-8859-1', ['ISO-8859-1', 'UTF-8'], ['UTF-8', 'ASCII'],
    ['UTF-16BE', 'UTF-16LE', 'UTF-8'], ['BASE64'], ['BASE64', 'UTF-8'], ['BASE64', '8bit'],
    ['UTF-7'], [], '', 'a', 'unknown', "UTF-8\0bad", ["UTF-8\0bad"], ['JIS', 'EUC-JP', 'SJIS', 'UTF-8']];
foreach (['UTF-8', 'SJIS', '8bit', 'BASE64'] as $internal) {
    mb_internal_encoding($internal);
    foreach (['UTF-8', 'UTF-16', 'JIS', '8bit', 'BASE64', 'HTML-ENTITIES', 'bad', "UTF-8\0extra"] as $to) {
        foreach ($lists as $from) {
            foreach ($inputs as $input) {
                foreach ([false, true] as $strict) {
                    ini_set('mbstring.strict_detection', $strict ? '1' : '0');
                    foreach ([63, 'none', 'long', 'entity'] as $substitute) {
                        mb_substitute_character(63);
                        mb_substitute_character($substitute);
                        // Invalidate the last-name cache deterministically before the captured call.
                        mb_strlen('', 'ASCII');
                        $warnings = [];
                        set_error_handler(static function (int $level, string $message) use (&$warnings): bool {
                            $warnings[] = [$level, bin2hex($message)];
                            return true;
                        });
                        $case = ['input' => bin2hex($input), 'internal' => $internal, 'to' => bin2hex($to),
                            'from' => is_string($from) ? ['bytes' => bin2hex($from)] : $from,
                            'strict' => $strict, 'substitute' => $substitute];
                        try {
                            $result = mb_convert_encoding($input, $to, $from);
                            $case['result'] = is_string($result) ? bin2hex($result) : $result;
                        } catch (Throwable $error) { $case['error'] = [$error::class, bin2hex($error->getMessage())]; }
                        restore_error_handler();
                        $case['warnings'] = $warnings;
                        gzwrite($stream, json_encode($case, JSON_THROW_ON_ERROR) . "\n");
                    }
                }
            }
        }
    }
}
gzclose($stream);
