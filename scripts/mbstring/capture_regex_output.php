<?php
require __DIR__ . '/capture_regex_request.php';

// Preserve untouched output objects without invoking conversion callbacks during observation.
function pack_capture_output(mixed $value): mixed {
    return is_object($value) ? ['object' => get_class($value)] : pack_regex_value($value);
}

// Observe reference storage while output initialization is releasing its previous value.
class CaptureOutputOwner {
    public bool $armed = true;
    public function __destruct() {
        if (!$this->armed) { return; }
        $GLOBALS['initialization'][] = pack_capture_output($GLOBALS['capture_ref']);
        foreach ($GLOBALS['case']['actions'] as $step) { $GLOBALS['actions'][] = regex_step($step); }
        if ($GLOBALS['case']['mutate']) {
            if (!is_array($GLOBALS['capture_ref'])) { $GLOBALS['capture_ref'] = []; }
            $GLOBALS['capture_ref']['kept'] = 'initialization';
            $GLOBALS['capture_copy'] = $GLOBALS['capture_ref'];
        }
        if ($GLOBALS['case']['throw']) { throw new RuntimeException('output cleanup'); }
    }
}
class CaptureMixedProperty { public mixed $value; }
class CaptureUnionProperty { public array|CaptureOutputOwner $value; }
class CaptureObjectProperty { public CaptureOutputOwner $value; }
class CaptureIntProperty { public int $value = 17; }

$case = json_decode(stream_get_contents(STDIN), true, 512, JSON_THROW_ON_ERROR);
$before = array_map('regex_step', $case['before']);
$initialization = [];
$actions = [];
$capture_copy = null;
switch ($case['storage']) {
    case 'local': $matches = new CaptureOutputOwner(); break;
    case 'mixed': $box = new CaptureMixedProperty(); $box->value = new CaptureOutputOwner(); $matches =& $box->value; break;
    case 'union': $box = new CaptureUnionProperty(); $box->value = new CaptureOutputOwner(); $matches =& $box->value; break;
    case 'object': $box = new CaptureObjectProperty(); $box->value = new CaptureOutputOwner(); $matches =& $box->value; break;
    case 'int': $box = new CaptureIntProperty(); $matches =& $box->value; break;
    default: throw new LogicException('Unknown output storage');
}
$capture_ref =& $matches;
$warnings = [];
$matches_at_warning = [];
set_error_handler(function ($level, $message) use (&$warnings, &$matches_at_warning, &$matches): bool {
    $warnings[] = bin2hex($message);
    $matches_at_warning[] = pack_capture_output($matches);
    return true;
});
$result = [];
try {
    $result['value'] = $case['op'](hex2bin($case['pattern']), hex2bin($case['subject']), $matches);
} catch (Throwable $error) {
    $result['error'] = pack_regex_error($error);
}
restore_error_handler();
$result['matches'] = pack_capture_output($matches);
$result['copy'] = pack_capture_output($capture_copy);
$result['initialization'] = $initialization;
$result['actions'] = $actions;
$result['warnings'] = $warnings;
$result['matches_at_warning'] = $matches_at_warning;
$result['encoding'] = mb_regex_encoding();
$result['options'] = mb_regex_set_options();
$result['position'] = mb_ereg_search_getpos();
$result['registers'] = pack_regex_value(mb_ereg_search_getregs());
if (is_object($matches)) { $matches->armed = false; }
$case['trace'] = $result;
echo json_encode($case, JSON_THROW_ON_ERROR), "\n";
