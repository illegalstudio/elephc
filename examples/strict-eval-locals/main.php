<?php
// Compile with: elephc --strict-locals examples/strict-eval-locals/main.php

// A runtime plugin keeps parameter declarations and callable capabilities.
$plugin = $argc > 0 ? '
function updatePluginQuantity(int $quantity, mixed $replacement): void {
    try {
        $quantity = $replacement;
    } catch (TypeError $error) {
        echo "The plugin quantity kept its integer value", PHP_EOL;
    }
    echo "Plugin quantity: ", $quantity, PHP_EOL;
}
function formatPluginQuantity(int $quantity): string {
    return "Quantity: " . strval($quantity);
}
function makePluginUpdater(int $quantity): callable {
    return static function(mixed $replacement) use($quantity): void {
        try {
            $quantity = $replacement;
        } catch (TypeError $error) {
            echo "The captured quantity kept its integer value", PHP_EOL;
        }
    };
}

$format = static function(int $quantity): string {
    return "Quantity: " . strval($quantity);
};
$format = "formatPluginQuantity";
updatePluginQuantity(2, 2.5);
echo $format(3), PHP_EOL;
$update = makePluginUpdater(2);
$update(2.5);
' : '';
eval($plugin);
