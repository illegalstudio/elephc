<?php

declare(strict_types=1);

require __DIR__ . '/canonical_value.php';

if ($argc < 2) {
    fwrite(STDERR, "usage: bootstrap.php <case.php> [case args...]\n");
    exit(64);
}

$casePath = $argv[1];
$caseArgv = [$casePath, ...array_slice($argv, 2)];
$GLOBALS['argv'] = $caseArgv;
$GLOBALS['argc'] = count($caseArgv);
$telemetryPath = getenv('ELEPHC_ORACLE_TELEMETRY');
if ($telemetryPath === false || $telemetryPath === '') {
    fwrite(STDERR, "ELEPHC_ORACLE_TELEMETRY is required\n");
    exit(64);
}

$events = [];
$exception = null;
$caught = null;
$returnValue = null;
$observations = null;

set_error_handler(
    static function (
        int $severity,
        string $message,
        string $file,
        int $line,
    ) use (&$events): bool {
        if ((error_reporting() & $severity) === 0) {
            return false;
        }
        $events[] = [
            'sequence' => count($events),
            'severity' => $severity,
            'severity_name' => match ($severity) {
                E_DEPRECATED => 'E_DEPRECATED',
                E_USER_DEPRECATED => 'E_USER_DEPRECATED',
                E_NOTICE => 'E_NOTICE',
                E_USER_NOTICE => 'E_USER_NOTICE',
                E_WARNING => 'E_WARNING',
                E_USER_WARNING => 'E_USER_WARNING',
                default => 'E_' . $severity,
            },
            'message' => $message,
            'file' => $file,
            'line' => $line,
        ];
        return false;
    },
);

try {
    $caseResult = (static fn (string $path): mixed => include $path)($casePath);
    if (
        is_array($caseResult)
        && array_key_exists('__oracle', $caseResult)
        && array_key_exists('return', $caseResult)
    ) {
        $returnValue = $caseResult['return'];
        $observations = $caseResult['__oracle'];
    } else {
        $returnValue = $caseResult;
    }
} catch (Throwable $throwable) {
    $caught = $throwable;
    $exception = [
        'class' => get_class($throwable),
        'message' => $throwable->getMessage(),
        'code' => $throwable->getCode(),
        'file' => $throwable->getFile(),
        'line' => $throwable->getLine(),
    ];
}

restore_error_handler();

$telemetry = [
    'schema_version' => 1,
    'events' => $events,
    'exception' => $exception,
    'return' => elephc_oracle_canonical_value($returnValue),
    'observations' => $observations === null
        ? null
        : elephc_oracle_canonical_value($observations),
];
file_put_contents(
    $telemetryPath,
    json_encode(
        $telemetry,
        JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR,
    ) . "\n",
);

if ($caught !== null) {
    throw $caught;
}
