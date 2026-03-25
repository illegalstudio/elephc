<?php

// Constants with const keyword
const MAX_RETRIES = 3;
const APP_NAME = "elephc";
const VERSION = 0.7;

// Constants with define()
define("PI", 3.14159);
define("GREETING", "Hello");

// Using constants in expressions
echo APP_NAME . " v" . VERSION . "\n";
echo GREETING . " World!\n";
echo "PI = " . PI . "\n";
echo "Max retries: " . MAX_RETRIES . "\n";

// Constants are accessible inside functions
function show_info() {
    echo "App: " . APP_NAME . "\n";
    echo "PI * 2 = " . (PI * 2) . "\n";
}
show_info();

// List unpacking
[$first, $second, $third] = [10, 20, 30];
echo $first . " + " . $second . " + " . $third . " = " . ($first + $second + $third) . "\n";

// call_user_func_array
function multiply($a, $b) {
    return $a * $b;
}
$result = call_user_func_array("multiply", [6, 7]);
echo "6 * 7 = " . $result . "\n";
