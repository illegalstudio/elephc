<?php
// Runtime-selected source keeps this example on the interpreter callback path.
$fragment = $argv[1] ?? '
class Reporter {
    public function format($message) { return "report: " . $message; }
    public function __destruct() { echo "reporter released\n"; }
}
$reporter = new Reporter();
for ($i = 0; $i < 10; $i++) {
    call_user_func([$reporter, "format"], "ready");
}
unset($reporter);
echo "finished\n";
';
eval($fragment);
