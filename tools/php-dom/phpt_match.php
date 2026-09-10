<?php

/**
 * Match one PHPT output using the exact PHP 8.5.8 run-tests.php rules.
 *
 * Usage: php -n phpt_match.php EXPECT|EXPECTF|EXPECTREGEX expected-file actual-file
 */

/** Normalize output exactly like php-src's run-tests.php comparison path. */
function normalize_phpt_output(string $output): string
{
    return trim(preg_replace('/\r\n/', "\n", $output));
}

/** Convert an EXPECTF payload to the PCRE used by PHP 8.5.8 run-tests.php. */
function expectf_to_regex(string $wanted): string
{
    $wantedRe = preg_replace('/\r\n/', "\n", $wanted);

    // Quote literal areas while preserving %r...%r raw-PCRE regions.
    $quoted = '';
    $startOffset = 0;
    $length = strlen($wantedRe);
    while ($startOffset < $length) {
        $start = strpos($wantedRe, '%r', $startOffset);
        if ($start !== false) {
            $end = strpos($wantedRe, '%r', $start + 2);
            if ($end === false) {
                $end = $start = $length;
            }
        } else {
            $start = $end = $length;
        }

        $quoted .= preg_quote(substr($wantedRe, $startOffset, $start - $startOffset), '/');
        if ($end > $start) {
            $quoted .= '(' . substr($wantedRe, $start + 2, $end - $start - 2) . ')';
        }
        $startOffset = $end + 2;
    }

    return strtr($quoted, [
        '%e' => preg_quote(DIRECTORY_SEPARATOR, '/'),
        '%s' => '[^\r\n]+',
        '%S' => '[^\r\n]*',
        '%a' => '.+?',
        '%A' => '.*?',
        '%w' => '\s*',
        '%i' => '[+-]?\d+',
        '%d' => '\d+',
        '%x' => '[0-9a-fA-F]+',
        '%f' => '[+-]?(?:\d+|(?=\.\d))(?:\.\d+)?(?:[Ee][+-]?\d+)?',
        '%c' => '.',
        '%0' => '\x00',
    ]);
}

/** Read one required matcher input or terminate with a harness error. */
function read_required_file(string $path): string
{
    $contents = @file_get_contents($path);
    if ($contents === false) {
        fwrite(STDERR, "cannot read matcher input: {$path}\n");
        exit(2);
    }
    return $contents;
}

if ($argc !== 4 || !in_array($argv[1], ['EXPECT', 'EXPECTF', 'EXPECTREGEX'], true)) {
    fwrite(STDERR, "usage: phpt_match.php EXPECT|EXPECTF|EXPECTREGEX expected-file actual-file\n");
    exit(2);
}

$mode = $argv[1];
$wanted = normalize_phpt_output(read_required_file($argv[2]));
$actual = normalize_phpt_output(read_required_file($argv[3]));

if ($mode === 'EXPECT') {
    exit(strcmp($wanted, $actual) === 0 ? 0 : 1);
}

$wantedRegex = $mode === 'EXPECTF' ? expectf_to_regex($wanted) : $wanted;
$pregError = null;
set_error_handler(static function (int $severity, string $message) use (&$pregError): bool {
    $pregError = $message;
    return true;
});
$matched = preg_match('/^' . $wantedRegex . '$/s', $actual);
restore_error_handler();

if ($matched === false) {
    fwrite(STDERR, "invalid {$mode} pattern: {$pregError}\n");
    exit(2);
}

exit($matched === 1 ? 0 : 1);
