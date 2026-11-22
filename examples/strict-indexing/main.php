<?php
// Compile with: elephc build --strict-indexing examples/strict-indexing/main.php
$stock = ['pens' => 12, 'paper' => null];
foreach (['pens', 'paper', 'ink'] as $item) {
    try {
        $quantity = $stock[$item];
        echo $item, ': ', $quantity ?? 'not counted', PHP_EOL;
    } catch (OutOfBoundsException $error) {
        echo $item, ': unknown item', PHP_EOL;
    }
}
// Intentional probes still accept a missing key.
echo 'Optional item: ', $stock['staples'] ?? 'not listed', PHP_EOL;
