<?php
// Capture live output and state transitions around mb_parse_str diagnostics and destructors.
// Every case runs in a fresh PHP process with its own startup parser limits.

/** Snapshots observable data without retaining references to the original output graph. */
function parseSnapshot(mixed $value): mixed {
    if (is_string($value)) { return ['bytes' => bin2hex($value)]; }
    if (is_object($value)) { return ['object' => get_class($value)]; }
    if (!is_array($value)) { return $value; }
    $entries = [];
    foreach ($value as $key => $child) { $entries[] = [parseSnapshot($key), parseSnapshot($child)]; }
    return ['array' => $entries];
}

/** Changes live mbstring settings while output initialization retires the previous value. */
class QueryPreviousOutput {
    public function __destruct() {
        global $output, $events, $case;
        $events[] = ['destroy', parseSnapshot($output)];
        if ($case['seed'] === 'settings') {
            @ini_set('mbstring.http_input', 'SJIS');
            mb_internal_encoding('UTF-8');
            mb_substitute_character(33);
        }
        if ($case['seed'] === 'throw') { throw new RuntimeException('initialization'); }
    }
}

if (($argv[1] ?? '') === '--worker') {
    $case = json_decode(stream_get_contents(STDIN), true, flags: JSON_THROW_ON_ERROR);
    @ini_set('mbstring.http_input', $case['encodings']);
    ini_set('mbstring.strict_detection', $case['strict'] ? '1' : '0');
    mb_internal_encoding($case['internal']);
    mb_substitute_character(63);
    $events = [];
    $copy = null;
    $output = $case['seed'] === 'scalar' ? 'old' : new QueryPreviousOutput();
    set_error_handler(static function (int $severity, string $message) use (&$output, &$events, &$copy, $case): bool {
        $events[] = ['warning', $severity, $message, parseSnapshot($output), mb_http_input()];
        switch ($case['action']) {
            case 'scalar': $output = 'handler'; break;
            case 'array': $output = ['handler' => 'kept']; break;
            case 'copy': $copy = $output; break;
            case 'reference': $GLOBALS['copy'] =& $output; break;
            case 'settings':
                mb_substitute_character(33);
                mb_internal_encoding('ASCII');
                @ini_set('mbstring.http_input', 'pass');
                break;
            case 'nested':
                @ini_set('mbstring.http_input', 'SJIS');
                mb_parse_str('nested=value', $nested);
                $events[] = ['nested', mb_http_input()];
                break;
            case 'throw': throw new RuntimeException('diagnostic');
        }
        return true;
    });
    try { $events[] = ['return', mb_parse_str(hex2bin($case['query']), $output)]; }
    catch (Throwable $error) { $events[] = ['exception', get_class($error), $error->getMessage()]; }
    restore_error_handler();
    echo json_encode(['case' => $case, 'events' => $events, 'output' => parseSnapshot($output),
        'copy' => parseSnapshot($copy), 'identified' => mb_http_input(), 'string_source' => mb_http_input('S'),
        'illegal' => mb_get_info('illegal_chars'), 'internal' => mb_internal_encoding(),
        'configured' => mb_http_input('L'), 'substitute' => mb_substitute_character()], JSON_THROW_ON_ERROR), "\n";
    exit;
}

$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/parse_str_reentry.jsonl.gz', 'wb9');
$count = 0;
$excluded = [];
$exclusionPath = __DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/parse_str_reentry_excluded.json';
$knownExcluded = is_file($exclusionPath) ? json_decode(file_get_contents($exclusionPath), true, flags: JSON_THROW_ON_ERROR) : [];
$sources = [
    ['a=%FF&b=%80', 'ASCII,UTF-8', true, 1000, 64],
    ['a=old&a[x][y]=%FF&b=%FF', 'UTF-8', false, 1000, 1],
    ['a=1&b=2', 'UTF-8', false, 1, 64],
    ['name=%82%A0', 'UTF-8', false, 1000, 64],
];
foreach ($sources as [$query, $encodings, $strict, $maxVars, $nesting]) {
    foreach (['scalar', 'settings', 'throw'] as $seed) {
        foreach (['none', 'scalar', 'array', 'copy', 'reference', 'settings', 'nested', 'throw'] as $action) {
            $case = ['query' => bin2hex($query), 'encodings' => $encodings, 'strict' => $strict,
                'max_vars' => $maxVars, 'max_nesting' => $nesting, 'seed' => $seed, 'action' => $action,
                'internal' => 'UTF-8'];
            foreach ($knownExcluded as $failure) {
                if ($failure['case'] === $case) { $excluded[] = $failure; continue 2; }
            }
            if ($maxVars === 1 && $seed === 'settings' && $action === 'settings') {
                $excluded[] = ['case' => $case, 'reason' => 'Initial capture process terminated without a valid oracle; exit status was not retained.'];
                continue;
            }
            $command = [PHP_BINARY, '-d', 'max_input_vars=' . $maxVars,
                '-d', 'max_input_nesting_level=' . $nesting, '-d', 'display_errors=0', '-d', 'log_errors=0',
                __FILE__, '--worker'];
            $process = proc_open($command, [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
            fwrite($pipes[0], json_encode($case, JSON_THROW_ON_ERROR));
            fclose($pipes[0]);
            $line = stream_get_contents($pipes[1]);
            fclose($pipes[1]);
            $stderr = stream_get_contents($pipes[2]);
            fclose($pipes[2]);
            $status = proc_close($process);
            if ($status !== 0) {
                $excluded[] = ['case' => $case, 'exit' => $status, 'stdout' => bin2hex($line),
                    'stderr' => bin2hex($stderr), 'reason' => 'PHP worker failed; this trace is not a semantic oracle.'];
                continue;
            }
            json_decode($line, true, flags: JSON_THROW_ON_ERROR);
            gzwrite($stream, $line);
            $count++;
        }
    }
}
gzclose($stream);
file_put_contents($exclusionPath,
    json_encode($excluded, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR) . "\n");
echo "Captured $count query reentry traces on PHP ", PHP_VERSION, "\n";
echo 'Excluded ', count($excluded), " failed PHP traces; inspect the exclusion ledger.\n";
