<?php
function readIndexedLocal(array<int> $list, array<string, int> $later): int {
    $sum = 0;
    for ($i = 0; $i < 4; $i++) {
        $key = $i . '';
        $sum = $sum + $list[$key];
    }
    foreach ($later as $key => $value) {
        $sum = $sum + $value;
    }
    return $sum;
}

function readIndexedSilent(array<int> $list, array<string, int> $later): int {
    $sum = 0;
    for ($i = 0; $i < 4; $i++) {
        $key = $i . '';
        $sum = $sum + ($list[$key] ?? 0);
    }
    foreach ($later as $key => $value) {
        $sum = $sum + $value;
    }
    return $sum;
}

function readIndexedCall(array<int> $list): int {
    $sum = 0;
    for ($i = 0; $i < 4; $i++) {
        $sum = $sum + $list[str_repeat((string)$i, 1)];
    }
    return $sum;
}

function measureIndexedKeys(): void {
    echo readIndexedLocal([1, 2, 3, 4], []), ':',
        readIndexedSilent([1, 2, 3, 4], []), ':', readIndexedCall([1, 2, 3, 4]), "\n";
    for ($trial = 0; $trial < 3; $trial++) {
        $before = memory_get_usage();
        readIndexedLocal([1, 2, 3, 4], []);
        $afterLocal = memory_get_usage();
        readIndexedSilent([1, 2, 3, 4], []);
        $afterSilent = memory_get_usage();
        readIndexedCall([1, 2, 3, 4]);
        $afterCall = memory_get_usage();
        echo 'local=', $afterLocal - $before, ' silent=', $afterSilent - $afterLocal,
            ' call=', $afterCall - $afterSilent, "\n";
    }
}

measureIndexedKeys();
