<?php
// Numeric job keys and named metadata share the same PHP array.
$jobs = [8 => 'compile', 'status' => 1, 12 => 'test'];
$snapshot = $jobs;
array_unshift($jobs);
echo 'keys: ', implode(',', array_keys($jobs)), "\n";
array_unshift($jobs, 'plan');
echo 'first: ', array_shift($jobs), "\n";
echo 'last: ', array_pop($jobs), "\n";
echo 'remaining: ', json_encode($jobs), "\n";
echo 'snapshot: ', json_encode($snapshot), "\n";
