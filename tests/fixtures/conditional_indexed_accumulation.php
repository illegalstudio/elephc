<?php
function conditionalIndexedSum(array<int> $list, array<string, int> $later, bool $silent): int {
    $sum = 0;
    for ($i = 0; $i < 4; $i++) {
        $key = $i . '';
        $sum = $sum + ($silent ? ($list[$key] ?? 0) : $list[$key]);
    }
    foreach ($later as $key => $value) {
        $sum = $sum + $value;
    }
    return $sum;
}

function measureConditionalIndexedSum(): void {
    echo conditionalIndexedSum([1, 2, 3, 4], [], false), ':',
        conditionalIndexedSum([1, 2, 3, 4], [], true), ':', conditionalIndexedSum([], [], true), "\n";
    for ($trial = 0; $trial < 3; $trial++) {
        $before = memory_get_usage();
        conditionalIndexedSum([1, 2, 3, 4], [], false);
        $afterOrdinary = memory_get_usage();
        conditionalIndexedSum([1, 2, 3, 4], [], true);
        $afterSilent = memory_get_usage();
        conditionalIndexedSum([], [], true);
        $afterMissing = memory_get_usage();
        echo 'ordinary=', $afterOrdinary - $before, ' silent=', $afterSilent - $afterOrdinary,
            ' missing=', $afterMissing - $afterSilent, "\n";
    }
}

measureConditionalIndexedSum();
