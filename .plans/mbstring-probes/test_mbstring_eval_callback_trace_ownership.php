<?php
// With zend.exception_ignore_args=0, PHP prints caught, done, released.
// With zend.exception_ignore_args=1, PHP prints released, caught, done.
// Native eval currently follows the second order because throwable traces do not retain arguments.
mb_ereg_match("", "");
$source = $argc > 0 ? '
class EvalTraceOutput {
    public function __destruct() { echo "released\n"; }
}
try { call_user_func("mb_ereg", "", "a", new EvalTraceOutput()); }
catch (ValueError $error) { echo "caught\n"; }
echo "done\n";
' : '';
eval($source);
