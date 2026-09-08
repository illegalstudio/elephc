<?php

// Demonstrates the OPcache-core `opcache_get_configuration()` builtin: it always
// returns an array `['directives' => [...], 'version' => [...], 'blacklist' => []]`
// with the compile-target's typed, normalized `opcache.*` directive defaults.

$config = opcache_get_configuration();

echo $config['version']['opcache_product_name'], "\n"; // Zend OPcache
echo $config['version']['version'], "\n";               // 8.5.10-dev (default target)

$directives = $config['directives'];
echo $directives['opcache.enable'] ? "enabled\n" : "disabled\n";     // enabled
echo $directives['opcache.enable_cli'] ? "cli-on\n" : "cli-off\n";   // cli-off (default)
echo $directives['opcache.jit'], "\n";                                // disable
echo $directives['opcache.memory_consumption'], "\n";                 // 134217728
echo $directives['opcache.optimization_level'], "\n";                 // 2147401727
echo $directives['opcache.jit_hot_loop'], "\n";                       // 61 (8.5)

echo count($directives), "\n";          // 54 directives on the 8.5 target
echo count($config['blacklist']), "\n"; // 0 (empty in this increment)

echo function_exists('opcache_get_configuration') ? "exists\n" : "missing\n";
