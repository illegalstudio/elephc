<?php

// Demonstrates the runtime script cache: the one OPcache tier a compiled binary can
// still grow. Files the compiler sees are compiled into the binary; files found only at
// run time (here, discount rules picked up with glob()) are included through eval(), and
// OPcache keeps their parsed form so a second include skips the read and the parse.
//
// The CLI leaves OPcache off by default, as `php` does. Turn it on at compile time:
//
//   elephc --ini opcache.enable_cli=1 examples/opcache-runtime-cache/main.php
//
// Add `--ini opcache.file_cache=/tmp/rules-cache` to keep the parsed rules on disk, so
// the next process starts warm.

/** Loads one rule file chosen at run time; the include has to go through eval(). */
function load_rule(string $path): array
{
    return eval('return include ' . var_export($path, true) . ';');
}

$rules = glob(__DIR__ . '/rules/*.php');
sort($rules);

$cart = 120.0;
foreach ([1, 2] as $pass) {
    $total = $cart;
    foreach ($rules as $path) {
        $rule = load_rule($path);
        $total = $total * (100 - $rule['percent']) / 100;
    }
    printf("pass %d: %.2f -> %.2f\n", $pass, $cart, $total);
}

$status = opcache_get_status(false);
if ($status === false) {
    echo "OPcache is off for this binary; rebuild with --ini opcache.enable_cli=1\n";
    exit(0);
}

foreach ($rules as $path) {
    echo basename($path), ' cached: ', opcache_is_script_cached($path) ? 'yes' : 'no', "\n";
}

// Invalidating a rule drops its cached form; the next include reads and parses it again.
$first = $rules[0];
opcache_invalidate($first, true);
echo basename($first), ' after invalidate: ', opcache_is_script_cached($first) ? 'yes' : 'no', "\n";
load_rule($first);
echo basename($first), ' after reload: ', opcache_is_script_cached($first) ? 'yes' : 'no', "\n";
