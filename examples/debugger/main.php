<?php
// Build with --debug for stable locals, or with --debug-optimized to keep the
// same breakpoints and metadata while optimized-away locals report
// <optimized out> in `elephc debug`, DAP clients and `monitor --debug-ui`.
function greet(string $name, int $times): string {
    $message = str_repeat($name, $times);
    return $message;
}

$label = "hello";
$result = greet($label, 3);
echo $result, "\n";
