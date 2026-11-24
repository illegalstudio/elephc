<?php
function opaque(int $i): mixed {
    switch ($i) {
        case 0: return 'tick';
        case 1: return 'nosuchfunction';
        case 2: return 5;
    }
    return null;
}
$ticks = 0;
function tick() { global $ticks; $ticks++; return $ticks < 3; }
for ($i = 0; $i <= 2; $i++) {
    $it = new ArrayIterator([1, 2, 3, 4]);
    try { $r = iterator_apply($it, opaque($i)); echo "case $i: $r\n"; }
    catch (Throwable $e) { echo "case $i: ", get_class($e), ": ", $e->getMessage(), "\n"; }
}
echo "ticks $ticks\n";
