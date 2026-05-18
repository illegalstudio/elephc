<?php
$scores = [10, 20, 30];

foreach ($scores as &$score) {
    $score += 5;
}

foreach ($scores as $index => $value) {
    echo $index . ": " . $value . "\n";
}
