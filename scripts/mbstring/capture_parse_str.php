<?php
// Capture mb_parse_str query registration, conversion, limits, and request-state transitions.
// Run: php scripts/mbstring/capture_parse_str.php

/** Preserves array iteration order, integer keys, and arbitrary binary strings. */
function queryValue(mixed $value): mixed {
    if (is_string($value)) { return ['bytes' => bin2hex($value)]; }
    if (!is_array($value)) { return $value; }
    $entries = [];
    foreach ($value as $key => $child) { $entries[] = [queryValue($key), queryValue($child)]; }
    return ['array' => $entries];
}

/** Records one parser call after seeding an earlier aggregate identification. */
function captureQuery(string $query, array $encodings, string $to, bool $strict, int|string $substitute): void {
    @ini_set('mbstring.http_input', implode(',', $encodings));
    ini_set('mbstring.strict_detection', $strict ? '1' : '0');
    mb_internal_encoding($to);
    mb_substitute_character($substitute);
    @mb_parse_str('previous=value', $previous);
    $before = mb_get_info('illegal_chars');
    $warnings = [];
    set_error_handler(static function (int $severity, string $message) use (&$warnings): bool {
        $warnings[] = [$severity, $message];
        return true;
    });
    $output = 'old';
    $result = mb_parse_str($query, $output);
    restore_error_handler();
    echo json_encode([
        'query' => bin2hex($query), 'encodings' => $encodings, 'to' => $to,
        'strict' => $strict, 'substitute' => $substitute,
        'separator' => bin2hex(ini_get('arg_separator.input')),
        'max_vars' => (int) ini_get('max_input_vars'),
        'max_nesting' => (int) ini_get('max_input_nesting_level'),
        'display_errors' => (bool) ini_get('display_errors'),
        'result' => $result, 'output' => queryValue($output),
        'identified' => mb_http_input(), 'string_source' => mb_http_input('S'),
        'illegal' => mb_get_info('illegal_chars') - $before, 'warnings' => $warnings,
    ], JSON_THROW_ON_ERROR), "\n";
}

if (($argv[1] ?? '') === '--worker') {
    $queries = ['', '&', '&&', 'a=1', 'a=1&b=2&a=3', 'a+b=x+y&x.y=z', '=v&+=v&[]=v',
        'a', 'a=b=c', 'a=%00x%ff&x%00tail=z', "a=1\0&b=2", 'a=%2G%+%2&b=+%20',
        'a[]=1&a[]=2', 'a[2]=x&a[]=y', 'a[-5]=x&a[]=y', 'a[01]=x&a[+1]=y&a[-0]=z',
        'a=1&a[x]=2&a[]=3&a[x][]=4', 'a[x]=1&a=2', 'a[=1&a[foo=2&a[b.c=3',
        'a[b][=x&a[b][c=y', 'a[b]tail[c]=x', 'a[ ]=x&a[  ]=y&a[%09]=z&a[%0a]=q',
        'a[b.c]=x&a[b+c]=y', '0=x&00=y&-1=z&+1=q',
        '9223372036854775807=x&9223372036854775808=y&-9223372036854775808=z',
        'a[9223372036854775807]=x&a[]=y&a[]=z',
        'a[9223372036854775807]=x&a[][z][q]=y',
        '__Host-x=a&.+Host-y=b&__Secure-z=c&.+Secure-w=d',
        'x[__Host-y]=a&__Host-x[__Host-y]=b&x[__Secure-z]=c',
        'a[x][y][z]=1&a[]=2&b=3', 'a=old&a[x][y][z]=new&a[]=end',
        'a=1;b=2&c=3', '&&a=1&&b=2&&c=3&&', 'a=1&b=2&c=3&d=4',
        '%C3%A9=%C3%A0&%E7%8C%AB=%E6%9D%B1%E4%BA%AC',
        'name=%82%A0&%82%A2=%82%A4', 'a=%FF&b=%80', 'a=%FE%FF%00a&b=%FE%FF%00b'];
    for ($byte = 0; $byte < 256; $byte++) {
        $hex = sprintf('%%%02X', $byte);
        $queries[] = "a{$hex}b=x{$hex}y&a[{$hex}]=z";
    }
    foreach ([['UTF-8'], ['SJIS', 'UTF-8', 'EUC-JP'], ['ASCII', 'UTF-8'], ['pass']] as $encodings) {
        foreach ([false, true] as $strict) {
            foreach ($queries as $query) { captureQuery($query, $encodings, 'UTF-8', $strict, 63); }
        }
    }
    foreach (mb_list_encodings() as $to) {
        foreach (['a=%FF&b=%C3%A9', '%E7%8C%AB=%E6%9D%B1%E4%BA%AC'] as $query) {
            foreach ([63, 'none', 'long', 'entity'] as $substitute) {
                captureQuery($query, ['UTF-8'], $to, false, $substitute);
            }
        }
    }
    exit;
}

$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/parse_str.jsonl.gz', 'wb9');
$count = 0;
foreach ([['&', 1000, 64, 1], ['&;', 2, 2, 0], ['&;', 2, 2, 1], ['', 1000, 0, 0],
    ['&', 1000, 2, 0], [';', 1000, 2, 1]] as $config) {
    [$separator, $maxVars, $maxNesting, $display] = $config;
    $command = [PHP_BINARY, '-d', 'arg_separator.input=' . $separator,
        '-d', 'max_input_vars=' . $maxVars, '-d', 'max_input_nesting_level=' . $maxNesting,
        '-d', 'display_errors=' . $display, '-d', 'log_errors=0', __FILE__, '--worker'];
    $process = proc_open($command, [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => STDERR], $pipes);
    fclose($pipes[0]);
    while (($line = fgets($pipes[1])) !== false) { gzwrite($stream, $line); $count++; }
    fclose($pipes[1]);
    if (proc_close($process) !== 0) { throw new RuntimeException('PHP query oracle failed'); }
}
gzclose($stream);
echo "Captured $count query cases on PHP ", PHP_VERSION, "\n";
