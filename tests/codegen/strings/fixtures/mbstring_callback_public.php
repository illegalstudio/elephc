<?php
function capture_identity(array $matches): string { return $matches[0]; }
function capture_trace(array $matches): string { var_dump($matches); return "\\1\0"; }
function capture_needs_extra(array $matches, string $extra): string { return $matches[0] . $extra; }
class CallbackReplacement {
    public static function replace(array $matches): string { return "S"; }
    public function method(array $matches): string { return "M"; }
    public function __invoke(array $matches): string { return "I"; }
}
var_dump(mb_ereg_replace_callback("a", function(array $m): string { return "X"; }, "aba"));
var_dump(\MB_EREG_REPLACE_CALLBACK(string: "aA", callback: "capture_identity", pattern: "a", options: "i"));
var_dump(mb_ereg_replace_callback("(a)(b)?", "count", "a ab"));
var_dump(mb_ereg_replace_callback("(?<name>a)(b)?", "capture_trace", "a ab"));
var_dump(mb_ereg_replace_callback("a", "CAPTURE_IDENTITY", "aba"));
var_dump(mb_ereg_replace_callback("a", ["CallbackReplacement", "replace"], "aba"));
var_dump(mb_ereg_replace_callback("a", [new CallbackReplacement(), "method"], "aba"));
var_dump(mb_ereg_replace_callback("a", new CallbackReplacement(), "aba"));
var_dump(mb_ereg_replace_callback("a", [1 => "replace", 0 => "CallbackReplacement"], "aba"));
$replace = mb_ereg_replace_callback(...);
var_dump($replace("a", capture_identity(...), "a a"));
var_dump(mb_ereg_replace_callback("a", function(array $m): bool { return false; }, "aba"));
var_dump(mb_ereg_replace_callback("a", function(array $m): int { return 12; }, "aba"));
var_dump(mb_ereg_replace_callback("z", "capture_trace", "abc"));
var_dump(mb_ereg_replace_callback("z", "capture_needs_extra", "abc"));
var_dump(mb_ereg_replace_callback("a", "capture_trace", chr(255)));
