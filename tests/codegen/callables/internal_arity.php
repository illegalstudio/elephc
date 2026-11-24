<?php
function r1(mixed $f) { call_user_func($f, []); }
function r2(mixed $f) { call_user_func($f); }
function r3(mixed $f) { call_user_func_array($f, ["a", "b"]); }
function r4(mixed $f) { array_map($f, [[1]]); }
function r5(mixed $f) { $a = ["a", "b"]; usort($a, $f); }
function r6(mixed $f) { $f([]); }
function r7(mixed $f) { $f(); }
function show(string $label, string $name, mixed $f): void {
    try { $name($f); echo $label, ": ok\n"; }
    catch (Throwable $e) { echo $label, ": ", get_class($e), ": ", $e->getMessage(), "\n"; }
}
function routes(string $scope, mixed $f): void {
    foreach (["r1", "r2", "r3", "r4", "r5", "r6", "r7"] as $name) { show("$scope $name", $name, $f); }
}
class Holder {
    private function secret(): string { return "s"; }
    public function run(mixed $f): void {
        routes("private-scope", $f);
        try { call_user_func($f, []); } catch (Throwable $e) { echo "private-scope inline: ", get_class($e), ": ", $e->getMessage(), "\n"; }
        try { array_map($f, [[1]]); } catch (Throwable $e) { echo "private-scope inline map: ", get_class($e), ": ", $e->getMessage(), "\n"; }
        try { $f(); } catch (Throwable $e) { echo "private-scope inline direct: ", get_class($e), ": ", $e->getMessage(), "\n"; }
    }
}
function opaque(): mixed { return strlen(...); }
$f = opaque();
routes("outside", $f);
(new Holder)->run($f);
$code = $argc > 5 ? 'return 1;' : '';
EVAL_LINE
