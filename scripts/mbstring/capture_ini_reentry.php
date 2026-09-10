<?php
// One isolated PHP worker for INI diagnostics that reenter configuration or throw.
require __DIR__ . '/capture_ini.php';

/** Evaluates retained getter values and intentionally distinct runtime or interned strings. */
function iniText(mixed $value): mixed {
    if (!is_array($value)) { return $value; }
    return match ($value[0]) {
        'get' => ini_get($value[1]),
        'flat' => ini_get_all('mbstring', false)[$value[1]],
        'local', 'global' => ini_get_all('mbstring', true)[$value[1]][$value[0] . '_value'],
        'copy' => pack('a*', iniText($value[1])),
        'lower' => strtolower(iniText($value[1])),
        'upper' => strtoupper(iniText($value[1])),
        'reverse' => strrev(iniText($value[1])),
        'literal' => match ($value[1]) { '' => '', '3' => '3', 'ASCII' => 'ASCII', 'SJIS' => 'SJIS' },
    };
}

/** Executes one fixture action and records its public return value or exception. */
function iniAction(array $action): mixed {
    return match ($action[0]) {
        'set' => ini_set($action[1], iniText($action[2])),
        'restore' => ini_restore($action[1]),
        'language' => mb_language($action[1]),
        'internal' => mb_internal_encoding($action[1]),
        'output' => mb_http_output($action[1]),
        'detect' => mb_detect_order($action[1]),
        'substitute' => mb_substitute_character($action[1]),
    };
}

/** Observes available public getters without confusing raw regex-limit text with parsed live limits. */
function reentrySnapshot(): array { return array_slice(iniSnapshot(), 0, 4); }

$case = json_decode($argv[1], true, flags: JSON_THROW_ON_ERROR);
error_reporting(0);
foreach ($case['before'] as $action) { iniAction($action); }
$events = [];
set_error_handler(static function (int $level, string $message) use ($case, &$events): bool {
    $event = ['level' => $level, 'message' => iniValue($message), 'before' => reentrySnapshot(), 'inner' => []];
    foreach ($case['callback'] as $action) { $event['inner'][] = iniValue(iniAction($action)); }
    $event['after'] = reentrySnapshot();
    $events[] = $event;
    if ($case['throw']) { throw new Exception('capture throw'); }
    return true;
});
try { $case['output'] = iniValue(iniAction($case['outer'])); }
catch (Throwable $error) { $case['error'] = [get_class($error), $error->getMessage()]; }
restore_error_handler();
$case['events'] = $events;
$case['state'] = reentrySnapshot();
// Nested equal-byte copies can expose Zend's previous-string over-release in the input array.
// Read the immutable original JSON again only after taking every observable snapshot.
$answer = json_decode($argv[1], true, flags: JSON_THROW_ON_ERROR);
foreach (['output', 'error', 'events', 'state'] as $field) {
    if (array_key_exists($field, $case)) { $answer[$field] = $case[$field]; }
}
echo json_encode($answer, JSON_THROW_ON_ERROR), "\n";
