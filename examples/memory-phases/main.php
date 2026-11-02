<?php
// Sample before printing so reporting does not contaminate the measured phase.
$started = hrtime(true);
$beforeBytes = memory_get_usage();
$beforeAllocs = memory_get_allocations();
$beforeFrees = memory_get_frees();
memory_reset_peak_usage();

$entities = [];
for ($id = 0; $id < 10000; $id++) {
    $entities[] = $id;
}

$liveBytes = memory_get_usage();
$peakBytes = memory_get_peak_usage();
$afterAllocs = memory_get_allocations();
$afterFrees = memory_get_frees();
$elapsed = hrtime(true) - $started;
$allocations = $afterAllocs - $beforeAllocs;
$frees = $afterFrees - $beforeFrees;
echo "Build phase: ", $elapsed, " ns\n";
echo "Live byte delta: ", $liveBytes - $beforeBytes, "\n";
echo "Peak live bytes: ", $peakBytes, "\n";
echo "Allocations: ", $allocations, ", frees: ", $frees, "\n";
echo "Reserved arena bytes: ", memory_get_usage(true), "\n";

unset($entities);
echo "Live bytes after release: ", memory_get_usage(), "\n";
