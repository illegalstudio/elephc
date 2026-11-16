<?php
// With --strict-comparisons, string labels are ordered by their bytes.
function compareLabels(string $left, string $right): int {
    return $left <=> $right;
}

$labels = ['9', '10', '02'];
usort($labels, compareLabels(...));

foreach ($labels as $label) {
    switch ($label) {
        case '02':
            echo 'Label with a preserved leading zero: ', $label, PHP_EOL;
            break;
        default:
            echo 'Label: ', $label, PHP_EOL;
    }
}

if (in_array('02', $labels, true)) {
    echo 'Exact label found: 02', PHP_EOL;
}
