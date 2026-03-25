<?php
// System information example
// Demonstrates: PHP_EOL, PHP_OS, DIRECTORY_SEPARATOR, time(), microtime(),
//               getenv(), phpversion(), php_uname(), shell_exec()

echo "=== System Info ===" . PHP_EOL;
echo "OS: " . PHP_OS . PHP_EOL;
echo "Directory separator: " . DIRECTORY_SEPARATOR . PHP_EOL;
echo "elephc version: " . phpversion() . PHP_EOL;
echo "System name: " . php_uname() . PHP_EOL;

echo PHP_EOL . "=== Environment ===" . PHP_EOL;
$home = getenv("HOME");
echo "HOME: " . $home . PHP_EOL;
$user = getenv("USER");
echo "USER: " . $user . PHP_EOL;

echo PHP_EOL . "=== Time ===" . PHP_EOL;
$t = time();
echo "Unix timestamp: " . $t . PHP_EOL;
$mt = microtime(true);
echo "Microtime: " . $mt . PHP_EOL;

echo PHP_EOL . "=== Shell ===" . PHP_EOL;
$hostname = trim(shell_exec("hostname"));
echo "Hostname: " . $hostname . PHP_EOL;

echo PHP_EOL . "=== Timing ===" . PHP_EOL;
$start = microtime(true);
usleep(1000);
$end = microtime(true);
$elapsed = $end - $start;
echo "usleep(1000) took ~" . number_format($elapsed * 1000000, 0) . " microseconds" . PHP_EOL;
