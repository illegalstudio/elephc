<?php
function numericPart(int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $i; }
    return (int)$sum;
}
function closurePart(int $n): int {
    $f = function($x) { return $x + 1; };
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $f($i); }
    return (int)$sum;
}
function stringPart(int $n): int {
    $s = '';
    for ($i = 0; $i < $n; $i++) { $s .= 'x'; }
    return strlen($s);
}
function evalPart(string $source, int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { eval($source); }
    return (int)$sum;
}
function jsonPart(string $source): int {
    $value = json_decode($source, true);
    return count($value);
}
$n = (int)$argv[1];
$source = $argv[2];
$json = (string)file_get_contents($argv[3]);
$evalCount = (int)$argv[4];
$start = hrtime(true);
$a = numericPart($n);
$t1 = hrtime(true);
$b = closurePart($n);
$t2 = hrtime(true);
$c = stringPart($n);
$t3 = hrtime(true);
$d = evalPart($source, $evalCount);
$t4 = hrtime(true);
$e = jsonPart($json);
$t5 = hrtime(true);
echo $a, ':', $b, ':', $c, ':', $d, ':', $e, '|', $t5-$start, '|',
     $t1-$start, ':', $t2-$t1, ':', $t3-$t2, ':', $t4-$t3, ':', $t5-$t4;
