<?php
class H { public function greet(string $id): string { return $id; } public static function st() { return "st"; } }
abstract class Abs { abstract public function am(); public static function make() {} }
function opaque(int $i): mixed {
    $h = new H();
    switch ($i) {
        case 0: return [$h];
        case 1: return [$h, 'greet', 'x'];
        case 2: return [[$h], 'greet'];
        case 3: return [$h, 5];
        case 4: return ['NoSuchClass', 'm'];
        case 5: return [$h, 'nosuch'];
        case 6: return ['Abs', 'am'];
        case 7: return [5, 'greet'];
        case 8: return [null, 'greet'];
        case 9: return [];
        case 10: return ['H', 'greet'];
        case 11: return 'nosuchfn';
        case 12: return 5;
    }
    return null;
}
function viaCuf(mixed $c) { return call_user_func($c, "id"); }
function viaCufa(mixed $c) { return call_user_func_array($c, ["id"]); }
function viaMap(mixed $c) { return array_map($c, ["id"]); }
function viaDirect(mixed $c) { return $c("id"); }
foreach (["viaCuf", "viaCufa", "viaMap", "viaDirect"] as $route) {
    for ($i = 0; $i <= 12; $i++) {
        try { $route(opaque($i)); echo "$route $i: ok\n"; }
        catch (Throwable $e) { echo "$route $i: ", get_class($e), ": ", $e->getMessage(), "\n"; }
    }
}
EVAL_LINE
