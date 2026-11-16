<?php

// Sum the lengths of the command-line arguments and count the long ones. Both
// accumulators stay in integer registers: they are boxed only when the loop ends.
$total = 0;
$long = 0;
foreach ($argv as $arg) {
    $total += strlen($arg);
    if (strlen($arg) > 8) {
        $long++;
    }
}
echo $total, " characters, ", $long, " long arguments\n";
