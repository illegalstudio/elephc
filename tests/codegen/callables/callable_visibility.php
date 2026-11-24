<?php
class H {
    private function priv(string $id): string { return "priv:$id"; }
    protected function prot(string $id): string { return "prot:$id"; }
    public function pub(string $id): string { return "pub:$id"; }
    private static function spriv(string $id): string { return "spriv:$id"; }
    protected static function sprot(string $id): string { return "sprot:$id"; }
    public function __invoke($x) { return "inv:$x"; }
}
function opaque(int $i): mixed {
    $h = new H();
    switch ($i) {
        case 0: return [$h, 'priv'];
        case 1: return [$h, 'prot'];
        case 2: return [$h, 'pub'];
        case 3: return ['H', 'spriv'];
        case 4: return ['H', 'sprot'];
        case 5: return 'H::spriv';
        case 6: return 'H::sprot';
        case 7: return [$h, 'nosuch'];
        case 8: return ['NoClass', 'x'];
        case 9: return 'nofunction';
        case 10: return 42;
    }
    return null;
}
function viaCuf(mixed $c) { return call_user_func($c, "id"); }
function viaCufa(mixed $c) { return call_user_func_array($c, ["id"]); }
function viaMap(mixed $c) { return array_map($c, ["id"]); }
function viaDirect(mixed $c) { return $c("id"); }
function viaUsort(mixed $c) { $a = [2, 1]; usort($a, $c); return $a; }
foreach (["viaCuf", "viaCufa", "viaMap", "viaDirect", "viaUsort"] as $route) {
    for ($i = 0; $i <= 10; $i++) {
        try { $r = $route(opaque($i)); echo "$route $i: ok\n"; }
        catch (Throwable $e) { echo "$route $i: ", get_class($e), ": ", $e->getMessage(), "\n"; }
    }
}
class Other {
    private function hidden() {}
    public function run(mixed $c) { return $c("id"); }
    public function viaCuf(mixed $c) { return call_user_func($c, "id"); }
}
foreach ([3, 4, 5, 6, 7, 8] as $i) {
    foreach (["run", "viaCuf"] as $m) {
        try { (new Other())->$m(opaque($i)); } catch (Throwable $e) { echo "scope $m $i: ", get_class($e), ": ", $e->getMessage(), "\n"; }
    }
}
try { (new Other())->run(["H", "spriv"]); } catch (Throwable $e) { echo "literal: ", get_class($e), ": ", $e->getMessage(), "\n"; }
try { (new Other())->run(["H", "nosuch"]); } catch (Throwable $e) { echo "literal: ", get_class($e), ": ", $e->getMessage(), "\n"; }
EVAL_LINE
