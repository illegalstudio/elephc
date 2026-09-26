<?php
// Capture live-reference replacement between entries using PHP's real object destructors.
require __DIR__ . '/capture_regex_request.php';

class RetargetCaptureHolder { public mixed $output; }
class InitializeCaptureOutput {
    public function __destruct() {
        $GLOBALS['output'][0] = new ReassignCaptureOutput();
        $GLOBALS['copy'] = $GLOBALS['output'];
    }
}
class ReassignCaptureOutput {
    public function __destruct() {
        $GLOBALS['events'][] = 'overwrite';
        $GLOBALS['output'] = ['replacement' => 'new'];
        if ($GLOBALS['capture_throw']) { throw new RuntimeException('capture overwrite'); }
    }
}

$cases = [];
foreach (['mb_ereg', 'mb_eregi'] as $op) {
    foreach ([false, true] as $capture_throw) {
        $holder = new RetargetCaptureHolder();
        $output =& $holder->output;
        $output = new InitializeCaptureOutput();
        $copy = null;
        $events = [];
        $trace = [];
        $pattern = '(?<name>a)(?<optional>b)?';
        $subject = $op === 'mb_eregi' ? 'A' : 'a';
        try { $trace['value'] = $op($pattern, $subject, $output); }
        catch (Throwable $error) { $trace['error'] = pack_regex_error($error); }
        $trace['matches'] = pack_regex_value($output);
        $trace['copy'] = pack_regex_value($copy);
        $trace['events'] = $events;
        $cases[] = ['op' => $op, 'pattern' => bin2hex($pattern), 'subject' => bin2hex($subject),
            'storage' => 'mixed', 'actions' => [], 'mutate' => false, 'throw' => false,
            'capture_retarget' => true, 'capture_throw' => $capture_throw, 'trace' => $trace];
    }
}
echo json_encode($cases, JSON_THROW_ON_ERROR | JSON_PRETTY_PRINT), "\n";
