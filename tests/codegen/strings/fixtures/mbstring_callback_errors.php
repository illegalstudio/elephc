<?php
function callback_failure(array $matches): string { echo "called\n"; throw new RuntimeException("callback failed"); }
function callback_needs_extra(array $matches, string $extra): string { return $matches[0] . $extra; }
try { mb_ereg_replace_callback("a", "callback_failure", "a a"); }
catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
try { mb_ereg_replace_callback("a", "missing_callback", chr(255)); }
catch (TypeError $e) { echo "invalid callback\n"; }
try { mb_ereg_replace_callback("a", ["MissingClass", "missing"], "a"); }
catch (TypeError $e) { echo "invalid method\n"; }
try { mb_ereg_replace_callback("a", "callback_failure", "a", "Q"); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_ereg_replace_callback("a", "callback_needs_extra", "a"); }
catch (ArgumentCountError $e) { echo get_class($e), "\n"; }
var_dump(mb_ereg_replace_callback("a", function(array $m): string { return "ok"; }, "a"));
