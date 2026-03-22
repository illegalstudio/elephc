<?php
function factorial($n) {
    if ($n <= 1) {
        return 1;
    }
    return $n * factorial($n - 1);
}

for ($i = 0; $i <= 12; $i++) {
    echo $i . "! = " . factorial($i) . "\n";
}
