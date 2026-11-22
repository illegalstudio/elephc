<?php
// Compile with: elephc build --strict-locals examples/strict-static-locals/main.php

function updateCounter(mixed $replacement): void {
    $counter="scratch";
    $previous=&$counter;
    static $counter=1;
    try {
        $counter=$replacement;
    } catch (TypeError $error) {
        echo "Counter requires an integer", PHP_EOL;
    }
    echo "Counter: ", $counter, "; previous: ", $previous, PHP_EOL;

    // Ending this local name leaves the persistent counter available to the next call.
    unset($counter);
    $counter=0.5;
    echo "Temporary counter: ", $counter, PHP_EOL;
}

updateCounter(2);
updateCounter(2.5);
updateCounter(3);
