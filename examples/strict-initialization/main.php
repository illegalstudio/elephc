<?php
// Compile with: elephc build --strict-initialization examples/strict-initialization/main.php
function showSetting(bool $load): void {
    if ($load) {
        $setting = 'ready';
    }
    // Intentional probes accept an absent binding.
    echo 'Status: ', $setting ?? 'not loaded', PHP_EOL;
    try {
        $value = $setting;
        echo 'Value: ', $value, PHP_EOL;
    } catch (Error $error) {
        echo 'The setting must be initialized before reading it.', PHP_EOL;
    }
}
showSetting(true);
showSetting(false);

$setting = null; // Initialized null is a value, not a missing binding.
echo gettype($setting), PHP_EOL;
