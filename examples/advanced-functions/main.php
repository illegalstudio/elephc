<?php
// Advanced functions: global variables, static variables, pass by reference

// --- Global variables ---
// Share state between main scope and functions

$total = 0;

function add_to_total($amount) {
    global $total;
    $total = $total + $amount;
}

add_to_total(10);
add_to_total(25);
add_to_total(15);
echo "Total: " . $total . "\n";

// --- Static variables ---
// Persistent state across function calls

function make_id() {
    static $next_id = 0;
    $next_id++;
    return $next_id;
}

echo "ID: " . make_id() . "\n";
echo "ID: " . make_id() . "\n";
echo "ID: " . make_id() . "\n";

// --- Pass by reference ---
// Modify the caller's variables directly

function clamp_ref(&$val, $lo, $hi) {
    if ($val < $lo) {
        $val = $lo;
    }
    if ($val > $hi) {
        $val = $hi;
    }
}

$temperature = 150;
clamp_ref($temperature, 0, 100);
echo "Clamped: " . $temperature . "\n";

// --- Combining features ---
// A simple accumulator using global + reference + static

$log_count = 0;

function log_message(&$count, $msg) {
    static $prefix = 0;
    $prefix++;
    $count++;
    echo "[" . $prefix . "] " . $msg . "\n";
}

log_message($log_count, "Starting up");
log_message($log_count, "Processing");
log_message($log_count, "Done");
echo "Logged " . $log_count . " messages\n";
