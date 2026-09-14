<?php
// Capture diagnostic policy after query writes run a destructor that changes display_errors.
// Run: php -d max_input_nesting_level=1 -d log_errors=0 scripts/mbstring/capture_parse_str_display.php
class QueryDisplayOwner {
    public function __construct(public int $setting) {}
    public function __destruct() {
        global $events;
        $events[] = ['destroy', $this->setting];
        ini_set('display_errors', (string) $this->setting);
    }
}
$traces = [];
foreach ([0, 1] as $newDisplay) {
    @ini_set('mbstring.http_input', 'ASCII,UTF-8');
    ini_set('mbstring.strict_detection', '1');
    ini_set('display_errors', (string) (1 - $newDisplay));
    $events = [];
    $output = null;
    set_error_handler(static function ($severity, $message) use (&$output, &$events, $newDisplay) {
        $events[] = ['warning', $message];
        if (str_contains($message, 'Unable to detect')) { $output['a'] = new QueryDisplayOwner($newDisplay); }
        return true;
    });
    $result = mb_parse_str('a[x][y]=%FF', $output);
    restore_error_handler();
    $traces[] = ['new_display' => $newDisplay, 'events' => $events, 'output' => $output, 'result' => $result];
}
echo json_encode($traces, JSON_THROW_ON_ERROR), "\n";
