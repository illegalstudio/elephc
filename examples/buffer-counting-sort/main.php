<?php

// A counting sort over grid cells, the broadphase step of a collision grid. `$counts[$cell]++`
// and `--$ends[...]` update a scalar buffer element in place: one typed read, add and store per
// update, with no allocation in the loop. The postfix form yields the old element value and the
// prefix form the new one, exactly as on a PHP array.

$cellCount = 4;
$cellOf = [2, 0, 3, 2, 1, 0, 2, 3];
$items = count($cellOf);

// Count the items in each cell...
$counts = buffer_new<int>($cellCount);
foreach ($cellOf as $cell) {
    $counts[$cell]++;
}

// ...turn each count into the end of the cell's run...
$ends = buffer_new<int>($cellCount);
$end = 0;
for ($cell = 0; $cell < $cellCount; $cell++) {
    $end += $counts[$cell];
    $ends[$cell] = $end;
}

// ...and fill each run backwards, so every cell lists its items in their original order.
$order = buffer_new<int>($items);
for ($item = $items - 1; $item >= 0; $item--) {
    $order[--$ends[$cellOf[$item]]] = $item;
}

echo "cell counts:";
for ($cell = 0; $cell < $cellCount; $cell++) {
    echo " ", $counts[$cell];
}
echo "; sorted items:";
for ($i = 0; $i < $items; $i++) {
    echo " ", $order[$i];
}
echo "\n";

// A float buffer steps by 1.0, and an f32 element rounds what it stores.
$weights = buffer_new<f32>(1);
$weights[0] = 16777216.0;
$weights[0]++;
echo "f32 after ++: ", $weights[0], "\n";

buffer_free($counts);
buffer_free($ends);
buffer_free($order);
buffer_free($weights);
